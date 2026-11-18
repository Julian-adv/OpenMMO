use super::fence::axis_id;
use super::{unix_now, AuthError, AuthService, CharacterSaveData, ItemRow};
use onlinerpg_shared::fence::{FenceEdge, ITEM_ID as FENCE_ITEM};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::BTreeMap;

/// How long the captain holds an impound: one turn of Serin, in real seconds.
pub const IMPOUND_HOLD_SECONDS: i64 = onlinerpg_shared::moon::SERIN_PERIOD_DAYS
    * crate::game_state::time::REAL_DAY_DURATION_SECONDS as i64;

/// Offences from this one on are forfeited instead of impounded.
pub const FORFEIT_FROM_OFFENCE: u32 = 2;

pub struct RoadSeizure {
    pub owner_name: String,
    pub offence: u32,
    /// False when an earlier sweep of the same patrol already counted it.
    pub new_offence: bool,
    /// Taken for penning a guard in: forfeited whatever the offence count.
    pub obstruction: bool,
    pub edges: Vec<FenceEdge>,
    /// Furniture and houses pulled off the road.
    pub furniture: Vec<i64>,
    pub houses: u32,
    /// What the furniture and houses come back as if this is not forfeit.
    goods: Vec<ImpoundedStack>,
}

/// What one sweep takes off the road. `houses` gives the owning character
/// and building scroll of each house already pulled down.
#[derive(Default)]
pub struct RoadClearance {
    pub edges: Vec<FenceEdge>,
    pub furniture: Vec<i64>,
    pub houses: Vec<(i64, Option<String>)>,
}

impl RoadSeizure {
    pub fn forfeited(&self) -> bool {
        self.obstruction || self.offence >= FORFEIT_FROM_OFFENCE
    }

    pub fn report(&self) -> onlinerpg_shared::messages::PatrolSeizure {
        onlinerpg_shared::messages::PatrolSeizure {
            owner: self.owner_name.clone(),
            fences: self.edges.len() as u32,
            forfeited: self.forfeited(),
            furniture: self.furniture.len() as u32,
            houses: self.houses,
        }
    }
}

impl RoadSeizure {
    fn new(owner_name: String, obstruction: bool) -> Self {
        Self {
            owner_name,
            offence: 0,
            new_offence: false,
            obstruction,
            edges: Vec::new(),
            furniture: Vec::new(),
            houses: 0,
            goods: Vec::new(),
        }
    }
}

/// One line the captain holds. Fences have no `goods_id`.
#[derive(Clone, Default)]
pub struct ImpoundedStack {
    pub goods_id: Option<i64>,
    pub item_def_id: String,
    pub quantity: u32,
    pub enchant: i32,
    pub cape_color: Option<String>,
    pub cape_texture: Option<String>,
    pub locked: bool,
    /// Whether the reclaim fee applies; what a chest held comes back free.
    pub charged: bool,
}

impl ImpoundedStack {
    fn charged(item_def_id: String) -> Self {
        Self {
            item_def_id,
            quantity: 1,
            charged: true,
            ..Default::default()
        }
    }
}

/// Account and character name of the estate owner behind a fence.
fn fence_owner(
    conn: &Connection,
    edge: &FenceEdge,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT e.account_name, c.character_name FROM land_fences f
         JOIN land_estates e ON e.id=f.estate_id
         JOIN characters c ON c.id=e.owner_id
         WHERE f.x=?1 AND f.z=?2 AND f.axis=?3",
        params![edge.x, edge.z, axis_id(edge.axis)],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Account and character name of a character.
fn character_account(
    conn: &Connection,
    character_id: i64,
) -> Result<Option<(String, String)>, rusqlite::Error> {
    conn.query_row(
        "SELECT account_name, character_name FROM characters WHERE id=?1",
        [character_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
}

/// Drop every impound held longer than `IMPOUND_HOLD_SECONDS`.
fn forfeit_unclaimed_impounds(conn: &Connection) -> Result<(), rusqlite::Error> {
    let overdue = "SELECT account_name FROM guard_impound_since WHERE seized_at <= ?1";
    let cutoff = [unix_now() - IMPOUND_HOLD_SECONDS];
    let mut lines = 0;
    for table in ["guard_impounds", "guard_impound_goods"] {
        lines += conn.execute(
            &format!("DELETE FROM {table} WHERE account_name IN ({overdue})"),
            cutoff,
        )?;
    }
    let accounts = conn.execute(
        "DELETE FROM guard_impound_since WHERE seized_at <= ?1",
        cutoff,
    )?;
    if lines > 0 {
        tracing::info!(accounts, lines, "unclaimed impounds forfeited");
    }
    Ok(())
}

impl AuthService {
    pub(super) fn ensure_road_patrol_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS guard_offences (
                account_name TEXT PRIMARY KEY
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                count INTEGER NOT NULL,
                last_patrol INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS guard_impounds (
                account_name TEXT NOT NULL
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                PRIMARY KEY (account_name, item_def_id)
            );
            CREATE TABLE IF NOT EXISTS guard_impound_goods (
                id INTEGER PRIMARY KEY,
                account_name TEXT NOT NULL
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                item_def_id TEXT NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                enchant INTEGER NOT NULL DEFAULT 0,
                cape_color TEXT,
                cape_texture TEXT,
                locked INTEGER NOT NULL DEFAULT 0,
                charged INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_guard_impound_goods_account
                ON guard_impound_goods(account_name);
            CREATE TABLE IF NOT EXISTS guard_impound_since (
                account_name TEXT PRIMARY KEY
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                seized_at INTEGER NOT NULL
            );
            DROP TABLE IF EXISTS road_patrol_state;",
        )?;
        conn.execute(
            "INSERT OR IGNORE INTO guard_impound_since (account_name, seized_at)
             SELECT DISTINCT account_name, ?1 FROM guard_impounds",
            [unix_now()],
        )?;
        Ok(())
    }

    /// Clear the road. One patrol is one offence per account. A first offence
    /// is impounded: fences, furniture with what it stored, and each house's
    /// building scroll. Later ones and any `obstruction` are forfeited.
    pub fn seize_road(
        &self,
        clearance: &RoadClearance,
        patrol_id: i64,
        obstruction: bool,
    ) -> Result<Vec<RoadSeizure>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        forfeit_unclaimed_impounds(&tx)?;
        let mut by_account: BTreeMap<String, RoadSeizure> = BTreeMap::new();
        fn entry(
            map: &mut BTreeMap<String, RoadSeizure>,
            (account, owner_name): (String, String),
            obstruction: bool,
        ) -> &mut RoadSeizure {
            map.entry(account)
                .or_insert_with(|| RoadSeizure::new(owner_name, obstruction))
        }
        for edge in &clearance.edges {
            let Some(owner) = fence_owner(&tx, edge)? else {
                continue;
            };
            tx.execute(
                "DELETE FROM land_fences WHERE x=?1 AND z=?2 AND axis=?3",
                params![edge.x, edge.z, axis_id(edge.axis)],
            )?;
            entry(&mut by_account, owner, obstruction).edges.push(*edge);
        }
        for id in &clearance.furniture {
            let piece: Option<(i64, String)> = tx
                .query_row(
                    "SELECT owner_id, item_def_id FROM estate_chests WHERE id=?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            let Some((owner, item_def_id)) = piece else {
                continue;
            };
            let Some(owner) = character_account(&tx, owner)? else {
                continue;
            };
            let mut goods = vec![ImpoundedStack::charged(item_def_id)];
            {
                let mut stmt = tx.prepare(
                    "SELECT item_def_id, quantity, enchant, cape_color, cape_texture, locked
                     FROM estate_chest_items WHERE chest_id=?1 ORDER BY id",
                )?;
                let held = stmt.query_map([id], |row| {
                    Ok(ImpoundedStack {
                        goods_id: None,
                        item_def_id: row.get(0)?,
                        quantity: row.get(1)?,
                        enchant: row.get(2)?,
                        cape_color: row.get(3)?,
                        cape_texture: row.get(4)?,
                        locked: row.get(5)?,
                        charged: false,
                    })
                })?;
                for stack in held {
                    goods.push(stack?);
                }
            }
            tx.execute("DELETE FROM estate_chest_items WHERE chest_id=?1", [id])?;
            tx.execute("DELETE FROM estate_chests WHERE id=?1", [id])?;
            let seizure = entry(&mut by_account, owner, obstruction);
            seizure.furniture.push(*id);
            seizure.goods.extend(goods);
        }
        for (owner, scroll) in &clearance.houses {
            if let Some(owner) = character_account(&tx, *owner)? {
                let seizure = entry(&mut by_account, owner, obstruction);
                seizure.houses += 1;
                seizure
                    .goods
                    .extend(scroll.clone().map(ImpoundedStack::charged));
            }
        }
        for (account, seizure) in &mut by_account {
            let (count, last_patrol): (u32, Option<i64>) = tx
                .query_row(
                    "SELECT count, last_patrol FROM guard_offences WHERE account_name=?1",
                    [account.as_str()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .unwrap_or((0, None));
            seizure.new_offence = last_patrol != Some(patrol_id);
            seizure.offence = count + u32::from(seizure.new_offence);
            tx.execute(
                "INSERT OR REPLACE INTO guard_offences (account_name, count, last_patrol)
                 VALUES (?1, ?2, ?3)",
                params![account, seizure.offence, patrol_id],
            )?;
            if !seizure.forfeited() && !seizure.edges.is_empty() {
                tx.execute(
                    "INSERT INTO guard_impounds (account_name, item_def_id, quantity)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(account_name, item_def_id)
                     DO UPDATE SET quantity = quantity + excluded.quantity",
                    params![account, FENCE_ITEM, seizure.edges.len() as u32],
                )?;
            }
            if !seizure.forfeited() {
                tx.execute(
                    "INSERT OR REPLACE INTO guard_impound_since (account_name, seized_at)
                     VALUES (?1, ?2)",
                    params![account, unix_now()],
                )?;
                for stack in &seizure.goods {
                    let fields = params![
                        account,
                        stack.item_def_id,
                        stack.quantity,
                        stack.enchant,
                        stack.cape_color,
                        stack.cape_texture,
                        stack.locked,
                        stack.charged
                    ];
                    let merged = tx.execute(
                        "UPDATE guard_impound_goods SET quantity = quantity + ?3
                         WHERE account_name=?1 AND item_def_id=?2 AND enchant=?4
                           AND cape_color IS ?5 AND cape_texture IS ?6
                           AND locked=?7 AND charged=?8",
                        fields,
                    )?;
                    if merged == 0 {
                        tx.execute(
                            "INSERT INTO guard_impound_goods (account_name, item_def_id,
                                quantity, enchant, cape_color, cape_texture, locked, charged)
                             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                            fields,
                        )?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(by_account.into_values().collect())
    }

    /// Fences first, then furniture and house scrolls, then what chests held.
    pub fn impounded_items(&self, character_id: i64) -> Result<Vec<ImpoundedStack>, AuthError> {
        let conn = self.open_connection()?;
        forfeit_unclaimed_impounds(&conn)?;
        let mut stmt = conn.prepare(
            "SELECT NULL, i.item_def_id, i.quantity, 0, NULL, NULL, 0, 1, 0 AS tier, 0 AS id
             FROM guard_impounds i
             JOIN characters c ON c.account_name=i.account_name WHERE c.id=?1
             UNION ALL
             SELECT g.id, g.item_def_id, g.quantity, g.enchant, g.cape_color, g.cape_texture,
                    g.locked, g.charged, 2 - g.charged, g.id
             FROM guard_impound_goods g
             JOIN characters c ON c.account_name=g.account_name WHERE c.id=?1
             ORDER BY tier, id, 2",
        )?;
        let stacks = stmt
            .query_map([character_id], |row| {
                Ok(ImpoundedStack {
                    goods_id: row.get(0)?,
                    item_def_id: row.get(1)?,
                    quantity: row.get(2)?,
                    enchant: row.get(3)?,
                    cape_color: row.get(4)?,
                    cape_texture: row.get(5)?,
                    locked: row.get(6)?,
                    charged: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(stacks)
    }

    /// Hand `taken` units of each stack back; refuses if the impound changed
    /// since it was priced.
    pub fn reclaim_impounds(
        &self,
        character: &CharacterSaveData,
        inventory: &[ItemRow],
        taken: &[(ImpoundedStack, u32)],
    ) -> Result<Result<(), &'static str>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        for (stack, units) in taken {
            let (table, key, id): (_, _, rusqlite::types::Value) = match stack.goods_id {
                Some(id) => ("guard_impound_goods", "id=?1", id.into()),
                None => (
                    "guard_impounds",
                    "account_name=(SELECT account_name FROM characters WHERE id=?1)",
                    character.character_id.into(),
                ),
            };
            let all = *units == stack.quantity;
            let action = if all {
                format!("DELETE FROM {table}")
            } else {
                format!("UPDATE {table} SET quantity = quantity - ?4")
            };
            let sql = format!("{action} WHERE {key} AND item_def_id=?2 AND quantity=?3");
            let changed = if all {
                tx.execute(&sql, params![id, stack.item_def_id, stack.quantity])?
            } else {
                tx.execute(&sql, params![id, stack.item_def_id, stack.quantity, units])?
            };
            if changed != 1 {
                return Ok(Err("The impound changed. Ask again."));
            }
        }
        Self::write_character_states(&tx, std::slice::from_ref(character))?;
        Self::replace_inventories(&tx, [(character.character_id, inventory)])?;
        tx.commit()?;
        Ok(Ok(()))
    }

    #[cfg(test)]
    pub fn age_impounds(&self, seconds: i64) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE guard_impound_since SET seized_at = seized_at - ?1",
            [seconds],
        )?;
        Ok(())
    }
}
