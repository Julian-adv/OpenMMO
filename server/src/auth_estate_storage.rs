use super::family::estate_access_sql;
use super::{AuthError, AuthService, CharacterSaveData, ItemRow};
use onlinerpg_shared::estate_storage::{
    estate_storage_def, EstateChest, EstateChestState, EstateStorageDefinition,
};
use onlinerpg_shared::family::{
    is_family_furniture, permission, FamilyStorageEntry, FAMILY_BOARD_ITEM, FAMILY_CHEST_ITEM,
    FAMILY_CHEST_LIMIT, FAMILY_STORAGE_LOG_KEPT, FAMILY_STORAGE_LOG_SHOWN,
};
use onlinerpg_shared::fence::FencePlot;
use onlinerpg_shared::inventory::ItemInstance;
use onlinerpg_shared::messages::BagLineItem;
use onlinerpg_shared::Position;
use onlinerpg_terrain::land::plot_addr;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};

pub(crate) struct EstateDeposit {
    pub item: ItemInstance,
    pub quantity: u32,
    pub stackable: bool,
}

fn plot_key(x: f32, z: f32) -> (i32, i32, u8) {
    let addr = plot_addr(x, z);
    let tile = (addr.index / 4) as i32;
    (
        addr.rx * 16 + tile % 16,
        addr.rz * 16 + tile / 16,
        (addr.index % 4) as u8,
    )
}

/// Family permission that covers placing and moving a furnishing. Storage
/// chests have none: their contents are personal, so they stay with the
/// estate's own account.
fn family_use(definition: &EstateStorageDefinition) -> Option<u32> {
    (definition.capacity_kg == 0.0 || is_family_furniture(&definition.id))
        .then_some(permission::PLACE_FURNITURE)
}

/// What a character may do with a storage chest.
struct StorageAccess {
    revision: u64,
    item_def_id: String,
    can_deposit: bool,
    can_withdraw: bool,
    family_id: Option<i64>,
}

/// A family chest's contents belong to its family: members use it by rank,
/// deposits only while it stands on an active estate of that family. Once the
/// family is dissolved, only `owner_id` (its last head) may empty it.
fn family_chest_access(
    conn: &Connection,
    chest_id: i64,
    character_id: i64,
) -> Result<Option<StorageAccess>, rusqlite::Error> {
    let row = conn
        .query_row(
            "SELECT c.revision, c.item_def_id, c.family_id, c.owner_id,
                    e.family_id IS c.family_id AND COALESCE(e.missed, 1)=0,
                    (SELECT r.permissions FROM family_members m
                     JOIN family_ranks r ON r.family_id=m.family_id AND r.rank=m.rank
                     WHERE m.character_id=?2 AND m.family_id=c.family_id)
             FROM estate_chests c LEFT JOIN land_estates e ON e.id=c.estate_id
             WHERE c.id=?1",
            params![chest_id, character_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)? as u64,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, Option<u32>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((revision, item_def_id, family_id, owner_id, attached, permissions)) = row else {
        return Ok(None);
    };
    let (can_deposit, can_withdraw) = match (family_id, permissions) {
        (None, _) if owner_id == character_id => (false, true),
        (Some(_), Some(bits)) => (
            attached && bits & permission::STORAGE_DEPOSIT != 0,
            bits & permission::STORAGE_WITHDRAW != 0,
        ),
        _ => return Ok(None),
    };
    Ok(Some(StorageAccess {
        revision,
        item_def_id,
        can_deposit,
        can_withdraw,
        family_id,
    }))
}

fn storage_access(
    conn: &Connection,
    chest_id: i64,
    character_id: i64,
) -> Result<Option<StorageAccess>, rusqlite::Error> {
    let item_def_id: Option<String> = conn
        .query_row(
            "SELECT item_def_id FROM estate_chests WHERE id=?1",
            [chest_id],
            |row| row.get(0),
        )
        .optional()?;
    if item_def_id.as_deref() == Some(FAMILY_CHEST_ITEM) {
        return family_chest_access(conn, chest_id, character_id);
    }
    Ok(
        chest_access(conn, chest_id, character_id, None)?.map(|(revision, active, item_def_id)| {
            StorageAccess {
                revision,
                item_def_id,
                can_deposit: active,
                can_withdraw: true,
                family_id: None,
            }
        }),
    )
}

/// The family that family furniture placed at `estate_id` by `character_id`
/// joins, or why it cannot go there.
fn family_furniture_home(
    tx: &Transaction<'_>,
    estate_id: i64,
    character_id: i64,
    item_def_id: &str,
) -> Result<Result<(i64, i64), &'static str>, rusqlite::Error> {
    let estate: (Option<i64>, i64) = tx.query_row(
        "SELECT family_id, owner_id FROM land_estates WHERE id=?1",
        [estate_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let member_of: Option<i64> = tx
        .query_row(
            "SELECT family_id FROM family_members WHERE character_id=?1",
            [character_id],
            |row| row.get(0),
        )
        .optional()?;
    let (Some(family_id), estate_owner) = estate else {
        return Ok(Err(
            "Family furniture goes on an estate attached to a family.",
        ));
    };
    if member_of != Some(family_id) {
        return Ok(Err("Only members of the estate's family may place this."));
    }
    if item_def_id == FAMILY_CHEST_ITEM {
        let chests: i64 = tx.query_row(
            "SELECT COUNT(*) FROM estate_chests WHERE family_id=?1 AND item_def_id=?2",
            params![family_id, FAMILY_CHEST_ITEM],
            |row| row.get(0),
        )?;
        if chests >= FAMILY_CHEST_LIMIT {
            return Ok(Err("Your family already has four family chests."));
        }
    }
    Ok(Ok((family_id, estate_owner)))
}

fn furniture_family_use(
    conn: &Connection,
    furniture_id: i64,
) -> Result<Option<u32>, rusqlite::Error> {
    let item_def_id: Option<String> = conn
        .query_row(
            "SELECT item_def_id FROM estate_chests WHERE id=?1",
            [furniture_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(item_def_id
        .as_deref()
        .and_then(estate_storage_def)
        .and_then(family_use))
}

fn active_estate_at(
    tx: &Transaction<'_>,
    owner_id: i64,
    x: f32,
    z: f32,
    family_bits: Option<u32>,
) -> Result<Option<i64>, rusqlite::Error> {
    let (tile_x, tile_z, quadrant) = plot_key(x, z);
    tx.query_row(
        &format!(
            "SELECT e.id FROM land_plots p JOIN land_estates e ON e.id=p.estate_id
             WHERE p.tile_x=?1 AND p.tile_z=?2 AND p.quadrant=?3 AND {} AND e.missed=0",
            estate_access_sql("?4", family_bits)
        ),
        params![tile_x, tile_z, quadrant, owner_id],
        |row| row.get(0),
    )
    .optional()
}

fn placement_estate(
    tx: &Transaction<'_>,
    owner_id: i64,
    definition: &EstateStorageDefinition,
    position: Position,
    rotation_deg: f32,
) -> Result<Option<i64>, rusqlite::Error> {
    let radians = rotation_deg.to_radians();
    let footprint = definition.footprint();
    let mut estate_id = None;
    for (dx, dz) in [
        (0.0, 0.0),
        (footprint.min_x, footprint.min_z),
        (footprint.min_x, footprint.max_z),
        (footprint.max_x, footprint.min_z),
        (footprint.max_x, footprint.max_z),
    ] {
        let found = active_estate_at(
            tx,
            owner_id,
            position.x + dx * radians.cos() + dz * radians.sin(),
            position.z - dx * radians.sin() + dz * radians.cos(),
            family_use(definition),
        )?;
        if found.is_none() || estate_id.is_some_and(|id| Some(id) != found) {
            return Ok(None);
        }
        estate_id = found;
    }
    Ok(estate_id)
}

/// Revision, whether the estate is active, and the item def, when
/// `character_id` may handle the furnishing: their account's estate, an
/// abandoned one, or a family estate where their rank holds `family_bits`.
fn chest_access(
    conn: &Connection,
    chest_id: i64,
    character_id: i64,
    family_bits: Option<u32>,
) -> Result<Option<(u64, bool, String)>, rusqlite::Error> {
    conn.query_row(
        &format!(
            "SELECT c.revision, COALESCE(e.missed, 1)=0, c.item_def_id
             FROM estate_chests c LEFT JOIN land_estates e ON e.id=c.estate_id
             WHERE c.id=?1 AND ({} OR e.id IS NULL)",
            estate_access_sql("?2", family_bits)
        ),
        params![chest_id, character_id],
        |row| Ok((row.get::<_, i64>(0)? as u64, row.get(1)?, row.get(2)?)),
    )
    .optional()
}

fn log_family_storage(
    tx: &Transaction<'_>,
    family_id: i64,
    chest_id: i64,
    character: &CharacterSaveData,
    lines: &[(String, i64)],
) -> Result<(), rusqlite::Error> {
    let at = super::unix_now();
    let mut insert = tx.prepare_cached(
        "INSERT INTO family_storage_log
         (family_id, chest_id, character_id, character_name, item_def_id, quantity, at)
         VALUES (?1, ?2, ?3, (SELECT character_name FROM characters WHERE id=?3), ?4, ?5, ?6)",
    )?;
    for (item_def_id, quantity) in lines {
        insert.execute(params![
            family_id,
            chest_id,
            character.character_id,
            item_def_id,
            quantity,
            at
        ])?;
    }
    tx.execute(
        "DELETE FROM family_storage_log WHERE family_id=?1 AND id <= (
            SELECT id FROM family_storage_log WHERE family_id=?1
            ORDER BY id DESC LIMIT 1 OFFSET ?2)",
        params![family_id, FAMILY_STORAGE_LOG_KEPT],
    )?;
    Ok(())
}

fn read_items(conn: &Connection, chest_id: i64) -> Result<Vec<ItemInstance>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT id,item_def_id,quantity,enchant,cape_color,cape_texture,locked
         FROM estate_chest_items WHERE chest_id=?1 ORDER BY id",
    )?;
    let items = stmt
        .query_map([chest_id], |row| {
            Ok(ItemInstance {
                locked: row.get(6)?,
                instance_id: row.get::<_, i64>(0)? as u64,
                item_def_id: row.get(1)?,
                quantity: row.get::<_, i64>(2)? as u32,
                enchant: row.get(3)?,
                cape_color: row.get(4)?,
                cape_texture: row.get(5)?,
            })
        })?
        .collect();
    items
}

impl AuthService {
    pub fn estate_furniture_access(
        &self,
        furniture_id: i64,
        character_id: i64,
    ) -> Result<Option<bool>, AuthError> {
        let conn = self.open_connection()?;
        let family = furniture_family_use(&conn, furniture_id)?;
        Ok(chest_access(&conn, furniture_id, character_id, family)?.map(|(_, active, _)| active))
    }

    pub(super) fn ensure_estate_storage_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS estate_chests (
                id INTEGER PRIMARY KEY,
                estate_id INTEGER NOT NULL,
                owner_id INTEGER NOT NULL,
                item_def_id TEXT NOT NULL DEFAULT 'storage_chest',
                x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL,
                rotation_deg REAL NOT NULL,
                floor_level INTEGER NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (owner_id) REFERENCES characters(id) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS estate_chest_items (
                id INTEGER PRIMARY KEY,
                chest_id INTEGER NOT NULL REFERENCES estate_chests(id) ON DELETE CASCADE,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                enchant INTEGER NOT NULL DEFAULT 0,
                cape_color TEXT,
                cape_texture TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_estate_chest_items_chest
                ON estate_chest_items(chest_id);",
        )?;
        if !Self::table_columns(conn, "estate_chests")?.contains("item_def_id") {
            conn.execute(
                "ALTER TABLE estate_chests
                 ADD COLUMN item_def_id TEXT NOT NULL DEFAULT 'storage_chest'",
                [],
            )?;
        }
        let schema: String = conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='estate_chests'",
            [],
            |row| row.get(0),
        )?;
        if schema.contains("estate_id INTEGER NOT NULL UNIQUE") {
            let foreign_keys: bool = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
            conn.pragma_update(None, "foreign_keys", false)?;
            let migration = conn.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE estate_chests_new (
                    id INTEGER PRIMARY KEY,
                    estate_id INTEGER NOT NULL,
                    owner_id INTEGER NOT NULL,
                    item_def_id TEXT NOT NULL DEFAULT 'storage_chest',
                    x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL,
                    rotation_deg REAL NOT NULL,
                    floor_level INTEGER NOT NULL,
                    revision INTEGER NOT NULL DEFAULT 0,
                    FOREIGN KEY (owner_id) REFERENCES characters(id) ON DELETE CASCADE
                 );
                 INSERT INTO estate_chests_new
                    (id,estate_id,owner_id,item_def_id,x,y,z,rotation_deg,floor_level,revision)
                    SELECT id,estate_id,owner_id,item_def_id,x,y,z,rotation_deg,floor_level,revision
                    FROM estate_chests;
                 CREATE TABLE estate_chest_items_new (
                    id INTEGER PRIMARY KEY,
                    chest_id INTEGER NOT NULL REFERENCES estate_chests(id) ON DELETE CASCADE,
                    item_def_id TEXT NOT NULL,
                    quantity INTEGER NOT NULL CHECK (quantity > 0),
                    enchant INTEGER NOT NULL DEFAULT 0,
                    cape_color TEXT,
                    cape_texture TEXT
                 );
                 INSERT INTO estate_chest_items_new
                    (id,chest_id,item_def_id,quantity,enchant,cape_color,cape_texture)
                    SELECT id,chest_id,item_def_id,quantity,enchant,cape_color,cape_texture
                    FROM estate_chest_items;
                 DROP TABLE estate_chest_items;
                 DROP TABLE estate_chests;
                 ALTER TABLE estate_chests_new RENAME TO estate_chests;
                 ALTER TABLE estate_chest_items_new RENAME TO estate_chest_items;
                 CREATE INDEX idx_estate_chest_items_chest
                    ON estate_chest_items(chest_id);
                 COMMIT;",
            );
            if migration.is_err() {
                let _ = conn.execute_batch("ROLLBACK;");
            }
            conn.pragma_update(None, "foreign_keys", foreign_keys)?;
            migration?;
        }
        if !Self::table_columns(conn, "estate_chest_items")?.contains("locked") {
            conn.execute(
                "ALTER TABLE estate_chest_items ADD COLUMN locked INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        if !Self::table_columns(conn, "estate_chests")?.contains("text") {
            conn.execute("ALTER TABLE estate_chests ADD COLUMN text TEXT", [])?;
        }
        if !Self::table_columns(conn, "estate_chests")?.contains("family_id") {
            conn.execute("ALTER TABLE estate_chests ADD COLUMN family_id INTEGER", [])?;
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_estate_chests_family
                ON estate_chests(family_id) WHERE family_id IS NOT NULL;
             CREATE TABLE IF NOT EXISTS family_storage_log (
                id INTEGER PRIMARY KEY,
                family_id INTEGER NOT NULL,
                chest_id INTEGER NOT NULL,
                character_id INTEGER NOT NULL,
                character_name TEXT NOT NULL,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL,
                at INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_family_storage_log_family
                ON family_storage_log(family_id, id);",
        )?;
        Ok(())
    }

    /// The rank permissions of `character_id` at a family board, when they
    /// belong to its family and the board stands on that family's estate.
    pub fn family_board_permissions(
        &self,
        furniture_id: i64,
        character_id: i64,
    ) -> Result<Option<u32>, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT r.permissions FROM estate_chests c
                 JOIN land_estates e ON e.id=c.estate_id AND e.family_id=c.family_id
                 JOIN family_members m ON m.family_id=c.family_id AND m.character_id=?2
                 JOIN family_ranks r ON r.family_id=m.family_id AND r.rank=m.rank
                 WHERE c.id=?1 AND c.item_def_id=?3",
                params![furniture_id, character_id, FAMILY_BOARD_ITEM],
                |row| row.get(0),
            )
            .optional()?)
    }

    /// The newest family chest movements, for a member of the family.
    pub fn family_storage_log(
        &self,
        character_id: i64,
    ) -> Result<Option<Vec<FamilyStorageEntry>>, AuthError> {
        let conn = self.open_connection()?;
        let family_id: Option<i64> = conn
            .query_row(
                "SELECT family_id FROM family_members WHERE character_id=?1",
                [character_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(family_id) = family_id else {
            return Ok(None);
        };
        let mut stmt = conn.prepare_cached(
            "SELECT character_name, item_def_id, quantity, at FROM family_storage_log
             WHERE family_id=?1 ORDER BY id DESC LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(params![family_id, FAMILY_STORAGE_LOG_SHOWN as i64], |row| {
                Ok(FamilyStorageEntry {
                    character_name: row.get(0)?,
                    item_def_id: row.get(1)?,
                    quantity: row.get(2)?,
                    at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(entries))
    }

    pub(crate) fn estate_furniture_ids_for_owner(
        &self,
        owner_id: i64,
    ) -> Result<Vec<i64>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare("SELECT id FROM estate_chests WHERE owner_id=?1")?;
        let ids = stmt
            .query_map([owner_id], |row| row.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(ids)
    }

    pub fn load_estate_chests(&self) -> Result<Vec<EstateChest>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT c.id,c.estate_id,c.owner_id,c.item_def_id,c.x,c.y,c.z,c.rotation_deg,c.floor_level,
                    COALESCE(e.missed,1)>0,c.revision,c.text
             FROM estate_chests c LEFT JOIN land_estates e ON e.id=c.estate_id",
        )?;
        let chests = stmt
            .query_map([], |row| {
                Ok(EstateChest {
                    id: row.get(0)?,
                    estate_id: row.get(1)?,
                    owner_id: row.get(2)?,
                    item_def_id: row.get(3)?,
                    position: Position {
                        x: row.get::<_, f64>(4)? as f32,
                        y: row.get::<_, f64>(5)? as f32,
                        z: row.get::<_, f64>(6)? as f32,
                    },
                    rotation_deg: row.get::<_, f64>(7)? as f32,
                    floor_level: row.get::<_, i64>(8)? as i8,
                    overdue: row.get(9)?,
                    revision: row.get::<_, i64>(10)? as u64,
                    text: row.get(11)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chests)
    }

    pub fn estate_return_rugs(
        &self,
        character_id: i64,
    ) -> Result<Vec<(Position, f32, i8)>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT c.x,c.y,c.z,c.rotation_deg,c.floor_level
             FROM estate_chests c JOIN land_estates e ON e.id=c.estate_id
             WHERE e.account_name=(SELECT account_name FROM characters WHERE id=?1)
               AND e.grade=1 AND c.item_def_id='furniture_hearthbound_rug'
             ORDER BY c.id",
        )?;
        let rugs = stmt
            .query_map([character_id], |row| {
                Ok((
                    Position {
                        x: row.get::<_, f64>(0)? as f32,
                        y: row.get::<_, f64>(1)? as f32,
                        z: row.get::<_, f64>(2)? as f32,
                    },
                    row.get::<_, f64>(3)? as f32,
                    row.get::<_, i64>(4)? as i8,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rugs)
    }

    /// Where `character_id` may place `item_def_id`.
    pub fn estate_storage_plots(
        &self,
        character_id: i64,
        item_def_id: &str,
    ) -> Result<Vec<FencePlot>, AuthError> {
        self.fence_plots(
            character_id,
            estate_storage_def(item_def_id).and_then(family_use),
        )
    }

    pub fn place_estate_chest(
        &self,
        character: &CharacterSaveData,
        inventory: &[ItemRow],
        item_def_id: String,
        position: Position,
        rotation_deg: f32,
        floor_level: i8,
    ) -> Result<Result<EstateChest, &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(definition) = estate_storage_def(&item_def_id) else {
            return Ok(Err("That item is not estate storage."));
        };
        let Some(estate_id) = placement_estate(
            &tx,
            character.character_id,
            definition,
            position,
            rotation_deg,
        )?
        else {
            return Ok(Err(
                "The whole furnishing must be inside your active estate.",
            ));
        };
        // Family furniture belongs to the estate's owner, so it outlives the
        // member who placed it.
        let (family_id, owner_id) = if is_family_furniture(&item_def_id) {
            match family_furniture_home(&tx, estate_id, character.character_id, &item_def_id)? {
                Ok((family, owner)) => (Some(family), owner),
                Err(reason) => return Ok(Err(reason)),
            }
        } else {
            (None, character.character_id)
        };
        tx.execute(
            "INSERT INTO estate_chests
             (estate_id,owner_id,item_def_id,x,y,z,rotation_deg,floor_level,family_id)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                estate_id,
                owner_id,
                item_def_id,
                position.x,
                position.y,
                position.z,
                rotation_deg,
                floor_level,
                family_id
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::write_character_states(&tx, std::slice::from_ref(character))?;
        Self::replace_inventories(&tx, [(character.character_id, inventory)])?;
        tx.commit()?;
        Ok(Ok(EstateChest {
            id,
            estate_id,
            owner_id,
            item_def_id,
            position,
            rotation_deg,
            floor_level,
            overdue: false,
            revision: 0,
            text: None,
        }))
    }

    pub fn move_estate_furniture(
        &self,
        furniture_id: i64,
        owner_id: i64,
        expected_revision: u64,
        position: Position,
        rotation_deg: f32,
        floor_level: i8,
    ) -> Result<Result<i64, &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let family = furniture_family_use(&tx, furniture_id)?;
        let Some((revision, true, item_def_id)) =
            chest_access(&tx, furniture_id, owner_id, family)?
        else {
            return Ok(Err("You can only move furniture on your active estate."));
        };
        if revision != expected_revision {
            return Ok(Err("The furniture changed. Select it again."));
        }
        let Some(definition) = estate_storage_def(&item_def_id) else {
            return Ok(Err("This furniture has an unknown type."));
        };
        let Some(estate_id) = placement_estate(&tx, owner_id, definition, position, rotation_deg)?
        else {
            return Ok(Err(
                "The whole furnishing must be inside your active estate.",
            ));
        };
        let stays_with_family: bool = tx.query_row(
            "SELECT c.family_id IS NULL OR c.family_id IS e.family_id
             FROM estate_chests c, land_estates e WHERE c.id=?1 AND e.id=?2",
            params![furniture_id, estate_id],
            |row| row.get(0),
        )?;
        if !stays_with_family {
            return Ok(Err("Family furniture stays on its family's estates."));
        }
        tx.execute(
            "UPDATE estate_chests SET estate_id=?2,x=?3,y=?4,z=?5,rotation_deg=?6,
             floor_level=?7,revision=revision+1 WHERE id=?1",
            params![
                furniture_id,
                estate_id,
                position.x,
                position.y,
                position.z,
                rotation_deg,
                floor_level
            ],
        )?;
        tx.commit()?;
        Ok(Ok(estate_id))
    }

    pub fn set_estate_furniture_text(
        &self,
        furniture_id: i64,
        owner_id: i64,
        text: &str,
    ) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let family = furniture_family_use(&conn, furniture_id)?;
        Ok(conn.execute(
            &format!(
                "UPDATE estate_chests SET text=?1,revision=revision+1
                 WHERE id=?2 AND estate_id IN
                 (SELECT e.id FROM land_estates e WHERE {} AND e.missed=0)",
                estate_access_sql("?3", family)
            ),
            params![text, furniture_id, owner_id],
        )? == 1)
    }

    pub fn estate_chest_state(
        &self,
        chest_id: i64,
        character_id: i64,
    ) -> Result<Result<EstateChestState, &'static str>, AuthError> {
        let conn = self.open_connection()?;
        let Some(access) = storage_access(&conn, chest_id, character_id)? else {
            return Ok(Err(
                "You do not own this storage chest, and it is not abandoned.",
            ));
        };
        let item_def_id = access.item_def_id;
        let Some(definition) = estate_storage_def(&item_def_id) else {
            return Ok(Err("This storage chest has an unknown type."));
        };
        if definition.capacity_kg == 0.0 {
            return Ok(Err("This is decorative furniture, not a storage chest."));
        }
        Ok(Ok(EstateChestState {
            chest_id,
            family: item_def_id == FAMILY_CHEST_ITEM,
            item_def_id,
            revision: access.revision,
            max_weight: definition.max_weight(),
            can_deposit: access.can_deposit,
            can_withdraw: access.can_withdraw,
            items: read_items(&conn, chest_id)?,
        }))
    }

    pub fn transfer_estate_items(
        &self,
        character: &CharacterSaveData,
        inventory: &[ItemRow],
        chest_id: i64,
        deposits: &[EstateDeposit],
        withdrawals: &[BagLineItem],
        expected_revision: u64,
    ) -> Result<Result<u64, &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(access) = storage_access(&tx, chest_id, character.character_id)? else {
            return Ok(Err(
                "You do not own this storage chest, and it is not abandoned.",
            ));
        };
        if !deposits.is_empty() && !access.can_deposit {
            return Ok(Err(if access.family_id.is_some() {
                "You cannot store items in this family chest."
            } else {
                "Overdue estates can withdraw items but cannot store new ones."
            }));
        }
        if !withdrawals.is_empty() && !access.can_withdraw {
            return Ok(Err("Your family rank cannot take items from this chest."));
        }
        let revision = access.revision;
        let mut log: Vec<(String, i64)> = Vec::new();
        if revision != expected_revision {
            return Ok(Err("The chest changed. Its contents were refreshed."));
        }
        for withdrawal in withdrawals {
            let stored: Option<(u32, String)> = tx
                .query_row(
                    "SELECT quantity, item_def_id FROM estate_chest_items WHERE id=?1 AND chest_id=?2",
                    params![withdrawal.instance_id as i64, chest_id],
                    |row| Ok((row.get::<_, i64>(0)? as u32, row.get(1)?)),
                )
                .optional()?;
            let Some((stored, item_def_id)) = stored else {
                return Ok(Err("An item is no longer in the chest."));
            };
            log.push((item_def_id, -i64::from(withdrawal.qty)));
            if withdrawal.qty == 0 || withdrawal.qty > stored {
                return Ok(Err("Invalid storage quantity."));
            }
            if withdrawal.qty == stored {
                tx.execute(
                    "DELETE FROM estate_chest_items WHERE id=?1",
                    [withdrawal.instance_id as i64],
                )?;
            } else {
                tx.execute(
                    "UPDATE estate_chest_items SET quantity=quantity-?2 WHERE id=?1",
                    params![withdrawal.instance_id as i64, withdrawal.qty],
                )?;
            }
        }
        for deposit in deposits {
            let item = &deposit.item;
            log.push((item.item_def_id.clone(), i64::from(deposit.quantity)));
            let matching: Option<i64> = if deposit.stackable {
                tx.query_row(
                    "SELECT id FROM estate_chest_items WHERE chest_id=?1 AND item_def_id=?2
                     AND enchant=?3 AND cape_color IS ?4 AND cape_texture IS ?5 AND locked=?6 LIMIT 1",
                    params![
                        chest_id,
                        item.item_def_id,
                        item.enchant,
                        item.cape_color,
                        item.cape_texture,
                        item.locked
                    ],
                    |row| row.get(0),
                )
                .optional()?
            } else {
                None
            };
            if let Some(id) = matching {
                let changed = tx.execute(
                    "UPDATE estate_chest_items SET quantity=quantity+?2
                     WHERE id=?1 AND quantity<=?3",
                    params![id, deposit.quantity, i64::from(u32::MAX - deposit.quantity)],
                )?;
                if changed == 0 {
                    return Ok(Err("A stored stack is full."));
                }
            } else {
                tx.execute(
                    "INSERT INTO estate_chest_items
                     (chest_id,item_def_id,quantity,enchant,cape_color,cape_texture,locked)
                     VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    params![
                        chest_id,
                        item.item_def_id,
                        deposit.quantity,
                        item.enchant,
                        item.cape_color,
                        item.cape_texture,
                        item.locked
                    ],
                )?;
            }
        }
        let next = revision + 1;
        tx.execute(
            "UPDATE estate_chests SET revision=?2 WHERE id=?1",
            params![chest_id, next],
        )?;
        if let Some(family_id) = access.family_id {
            log_family_storage(&tx, family_id, chest_id, character, &log)?;
        }
        Self::write_character_states(&tx, std::slice::from_ref(character))?;
        Self::replace_inventories(&tx, [(character.character_id, inventory)])?;
        tx.commit()?;
        Ok(Ok(next))
    }

    pub fn recover_estate_chest(
        &self,
        character: &CharacterSaveData,
        inventory: &[ItemRow],
        chest_id: i64,
    ) -> Result<Result<(), &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let family = furniture_family_use(&tx, chest_id)?;
        if chest_access(&tx, chest_id, character.character_id, family)?.is_none() {
            return Ok(Err(
                "You can only recover your own or an abandoned storage chest.",
            ));
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM estate_chest_items WHERE chest_id=?1",
            [chest_id],
            |row| row.get(0),
        )?;
        if count != 0 {
            return Ok(Err("Empty the storage chest before recovering it."));
        }
        tx.execute("DELETE FROM estate_chests WHERE id=?1", [chest_id])?;
        Self::write_character_states(&tx, std::slice::from_ref(character))?;
        Self::replace_inventories(&tx, [(character.character_id, inventory)])?;
        tx.commit()?;
        Ok(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_migration_preserves_contents_and_allows_multiple_chests() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys=ON;
             CREATE TABLE characters (id INTEGER PRIMARY KEY);
             INSERT INTO characters (id) VALUES (1);
             CREATE TABLE estate_chests (
                id INTEGER PRIMARY KEY,
                estate_id INTEGER NOT NULL UNIQUE,
                owner_id INTEGER NOT NULL,
                x REAL NOT NULL, y REAL NOT NULL, z REAL NOT NULL,
                rotation_deg REAL NOT NULL,
                floor_level INTEGER NOT NULL,
                revision INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (owner_id) REFERENCES characters(id) ON DELETE CASCADE
             );
             CREATE TABLE estate_chest_items (
                id INTEGER PRIMARY KEY,
                chest_id INTEGER NOT NULL REFERENCES estate_chests(id) ON DELETE CASCADE,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                enchant INTEGER NOT NULL DEFAULT 0,
                cape_color TEXT,
                cape_texture TEXT
             );
             INSERT INTO estate_chests
                (id,estate_id,owner_id,x,y,z,rotation_deg,floor_level)
                VALUES (10,7,1,1.0,2.0,3.0,0.0,0);
             INSERT INTO estate_chest_items
                (id,chest_id,item_def_id,quantity)
                VALUES (20,10,'apple',3);",
        )
        .unwrap();

        AuthService::ensure_estate_storage_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO estate_chests
             (estate_id,owner_id,x,y,z,rotation_deg,floor_level)
             VALUES (7,1,4.0,2.0,5.0,90.0,0)",
            [],
        )
        .unwrap();

        let chest_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM estate_chests", [], |row| row.get(0))
            .unwrap();
        let item_quantity: i64 = conn
            .query_row(
                "SELECT quantity FROM estate_chest_items WHERE id=20 AND chest_id=10",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let item_def_id: String = conn
            .query_row(
                "SELECT item_def_id FROM estate_chests WHERE id=10",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let foreign_keys: bool = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chest_count, 2);
        assert_eq!(item_quantity, 3);
        assert_eq!(item_def_id, "storage_chest");
        assert!(foreign_keys);
    }
}
