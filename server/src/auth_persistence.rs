use super::{unix_now, AuthError, AuthService};
use crate::types::GameDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

/// One persisted inventory row: a bag stack (`equip_slot: None`) or an
/// equipped item.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemRow {
    pub locked: bool,
    pub item_def_id: String,
    pub quantity: u32,
    pub equip_slot: Option<String>,
    pub enchant: i32,
    /// Dye on a cape (doc/CAPE_CUSTOMIZATION.md); `None` on everything else.
    pub cape_color: Option<String>,
    /// Texture hash on a cape; `None` on everything else.
    pub cape_texture: Option<String>,
    /// The lasting item id (doc/ITEM_IDS.md); `None` on rows saved before it.
    pub item_id: Option<u64>,
}

/// A permanently learned skill.
#[derive(Debug, Clone)]
pub struct SkillRow {
    pub skill_id: String,
}

#[derive(Clone)]
pub struct CharacterSaveData {
    pub character_id: i64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
    pub xp: u64,
    pub level: u32,
    pub max_hp: u32,
    pub health: u32,
    pub mana: Option<u32>,
    pub floor_level: i8,
    pub dungeon_epoch: Option<i64>,
    pub gold: i64,
    pub satiation: u32,
    /// The archer's chosen pile; rides the periodic save so a relog resumes
    /// the same arrows (doc/COMBAT.md 원거리 전투).
    pub active_ammo: Option<String>,
}

/// One row of the player-trade ledger. `*_items` are JSON arrays of
/// `{def, qty, ench}` — never `instance_id`, which is minted per session and
/// means nothing once the session ends.
pub struct TradeLedgerEntry {
    pub a_character_id: i64,
    pub b_character_id: i64,
    pub a_gold_before: i64,
    pub a_gold_after: i64,
    pub b_gold_before: i64,
    pub b_gold_after: i64,
    pub a_items: String,
    pub b_items: String,
}

impl AuthService {
    pub(super) fn write_character_states(
        conn: &Connection,
        data: &[CharacterSaveData],
    ) -> Result<(), rusqlite::Error> {
        let mut stmt = conn.prepare(
            "UPDATE characters SET last_x = ?1, last_y = ?2, last_z = ?3, last_rotation = ?4, \
             xp = ?5, level = ?6, max_hp = ?7, health = ?8, floor_level = ?9, gold = ?10, \
             satiation = ?11, last_seen_at = ?12, active_ammo = ?14, mana = ?15, \
             dungeon_epoch = ?16 WHERE id = ?13",
        )?;
        let now = unix_now();
        for d in data {
            stmt.execute(params![
                f64::from(d.x),
                f64::from(d.y),
                f64::from(d.z),
                f64::from(d.rotation),
                d.xp as i64,
                i64::from(d.level),
                i64::from(d.max_hp),
                i64::from(d.health),
                i64::from(d.floor_level),
                d.gold,
                i64::from(d.satiation),
                now,
                d.character_id,
                d.active_ammo.as_deref(),
                d.mana.map(i64::from),
                d.dungeon_epoch,
            ])?;
        }
        Ok(())
    }

    fn write_world_time(conn: &Connection, datetime: &GameDateTime) -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO world_time (id, year, month, day, hour, minute, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, strftime('%s', 'now'))
             ON CONFLICT(id) DO UPDATE SET
                year = excluded.year,
                month = excluded.month,
                day = excluded.day,
                hour = excluded.hour,
                minute = excluded.minute,
                updated_at = excluded.updated_at",
            params![
                i64::from(datetime.year),
                i64::from(datetime.month),
                i64::from(datetime.day),
                i64::from(datetime.hour),
                i64::from(datetime.minute),
            ],
        )?;
        Ok(())
    }

    pub(super) fn replace_inventories<'a>(
        conn: &Connection,
        inventories: impl IntoIterator<Item = (i64, &'a [ItemRow])>,
    ) -> Result<(), rusqlite::Error> {
        let mut delete = conn.prepare("DELETE FROM character_items WHERE character_id = ?1")?;
        let mut insert = conn.prepare(
            "INSERT INTO character_items \
             (character_id, item_def_id, quantity, equip_slot, enchant, cape_color, \
              cape_texture, locked, item_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        let mut update_enchant = conn.prepare(
            "UPDATE characters SET weapon_enchant = ?2, armor_enchant = ?3 WHERE id = ?1",
        )?;
        let defs = crate::item_defs::item_defs();

        for (character_id, items) in inventories {
            delete.execute(params![character_id])?;
            for item in items {
                insert.execute(params![
                    character_id,
                    item.item_def_id,
                    item.quantity,
                    item.equip_slot,
                    item.enchant,
                    item.cape_color,
                    item.cape_texture,
                    item.locked,
                    item.item_id.map(|id| id as i64)
                ])?;
            }
            let mut weapon_enchant = 0;
            let mut armor_slots = HashMap::new();
            for item in items.iter().filter(|item| item.quantity > 0) {
                let Some(def) = defs.get(&item.item_def_id) else {
                    continue;
                };
                if def.is_weapon() {
                    weapon_enchant = weapon_enchant.max(item.enchant);
                } else if def.is_armor() {
                    if let Some(slot) = def.equip_slot {
                        let enchant = armor_slots.entry(slot).or_insert(0);
                        *enchant = (*enchant).max(item.enchant);
                    }
                }
            }
            let armor_enchant: i64 = armor_slots
                .values()
                .map(|enchant| i64::from(*enchant))
                .sum();
            update_enchant.execute(params![character_id, weapon_enchant, armor_enchant])?;
        }
        Ok(())
    }

    /// Add learned skills without deleting unknown skill IDs.
    fn upsert_skills<'a>(
        conn: &Connection,
        skills: impl IntoIterator<Item = (i64, &'a [SkillRow])>,
    ) -> Result<(), rusqlite::Error> {
        let mut upsert = conn.prepare(
            "INSERT INTO character_skills (character_id, skill_id) \
             VALUES (?1, ?2) \
             ON CONFLICT(character_id, skill_id) DO NOTHING",
        )?;

        for (character_id, rows) in skills {
            for row in rows {
                upsert.execute(params![character_id, row.skill_id])?;
            }
        }
        Ok(())
    }

    /// The archer's saved ammo choice, or `None` when never chosen.
    pub fn active_ammo(&self, character_id: i64) -> Result<Option<String>, AuthError> {
        let conn = self.open_connection()?;
        let value = conn
            .query_row(
                "SELECT active_ammo FROM characters WHERE id = ?1",
                params![character_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        Ok(value)
    }

    pub(super) fn ensure_character_skills_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "CREATE TABLE IF NOT EXISTS character_skills (
                character_id INTEGER NOT NULL,
                skill_id TEXT NOT NULL,
                PRIMARY KEY (character_id, skill_id),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        let columns = Self::table_columns(&tx, "character_skills")?;
        for column in ["level", "xp"] {
            if columns.contains(column) {
                tx.execute(
                    &format!("ALTER TABLE character_skills DROP COLUMN {column}"),
                    [],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The highest lasting item id issued (doc/ITEM_IDS.md): the written
    /// counter, or a larger id already saved if the server stopped before
    /// writing it.
    pub fn last_item_id(&self) -> Result<u64, AuthError> {
        let conn = self.open_connection()?;
        let last: i64 = conn.query_row(
            "SELECT MAX(id) FROM (
                SELECT last_issued AS id FROM item_id_counter
                UNION ALL SELECT MAX(item_id) FROM character_items
                UNION ALL SELECT MAX(item_id) FROM estate_chest_items
                UNION ALL SELECT MAX(item_id) FROM estate_chests
                UNION ALL SELECT MAX(item_id) FROM guard_impound_goods
                UNION ALL SELECT MAX(item_id) FROM estate_defense_trophies
                UNION ALL SELECT 0
            )",
            [],
            |row| row.get(0),
        )?;
        Ok(last as u64)
    }

    pub(super) fn ensure_item_id_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS item_id_counter (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                last_issued INTEGER NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    pub fn write_last_item_id(&self, last: u64) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO item_id_counter (id, last_issued) VALUES (1, ?1)
             ON CONFLICT(id) DO UPDATE SET last_issued = MAX(last_issued, excluded.last_issued)",
            [last as i64],
        )?;
        Ok(())
    }

    /// Player-trade ledger (doc/TRADE.md). Written in the same transaction as
    /// the trade itself, so it can never disagree with what happened. Gold
    /// before and after both sides is what makes coin appearing from nowhere
    /// detectable. No foreign key: a deleted character must not erase the
    /// record of what they traded away.
    pub(super) fn ensure_trade_ledger_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS player_trades (
                id INTEGER PRIMARY KEY,
                traded_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                a_character_id INTEGER NOT NULL,
                b_character_id INTEGER NOT NULL,
                a_gold_before INTEGER NOT NULL,
                a_gold_after INTEGER NOT NULL,
                b_gold_before INTEGER NOT NULL,
                b_gold_after INTEGER NOT NULL,
                a_items TEXT NOT NULL,
                b_items TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_player_trades_a ON player_trades(a_character_id)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_player_trades_b ON player_trades(b_character_id)",
            [],
        )?;
        Ok(())
    }

    pub(super) fn ensure_world_time_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS world_time (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                year INTEGER NOT NULL,
                month INTEGER NOT NULL,
                day INTEGER NOT NULL,
                hour INTEGER NOT NULL,
                minute INTEGER NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        Ok(())
    }

    /// Save game state in one transaction.
    pub fn save_batch(
        &self,
        characters: &[CharacterSaveData],
        inventories: &[(i64, Vec<ItemRow>)],
        skills: &[(i64, Vec<SkillRow>)],
        discoveries: &[(i64, String)],
        world_time: Option<&GameDateTime>,
    ) -> Result<(), AuthError> {
        if characters.is_empty()
            && inventories.is_empty()
            && skills.is_empty()
            && discoveries.is_empty()
            && world_time.is_none()
        {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let tx = conn.unchecked_transaction()?;
        Self::write_character_states(&tx, characters)?;
        Self::replace_inventories(
            &tx,
            inventories
                .iter()
                .map(|(id, items)| (*id, items.as_slice())),
        )?;
        Self::upsert_skills(&tx, skills.iter().map(|(id, rows)| (*id, rows.as_slice())))?;
        Self::insert_dungeon_discoveries(&tx, discoveries)?;
        if let Some(datetime) = world_time {
            Self::write_world_time(&tx, datetime)?;
        }
        tx.commit()?;
        Ok(())
    }

    /// A completed player trade: both sides' state, both inventories and the
    /// ledger row in one commit. Separate from `save_batch` because the trade
    /// must be durable before either client is told it succeeded, and because
    /// the ledger row has to share the transaction to stay truthful.
    pub fn commit_trade(
        &self,
        characters: &[CharacterSaveData],
        inventories: &[(i64, Vec<ItemRow>)],
        ledger: &TradeLedgerEntry,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let tx = conn.unchecked_transaction()?;
        Self::write_character_states(&tx, characters)?;
        Self::replace_inventories(
            &tx,
            inventories
                .iter()
                .map(|(id, items)| (*id, items.as_slice())),
        )?;
        tx.execute(
            "INSERT INTO player_trades (
                a_character_id, b_character_id,
                a_gold_before, a_gold_after, b_gold_before, b_gold_after,
                a_items, b_items
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                ledger.a_character_id,
                ledger.b_character_id,
                ledger.a_gold_before,
                ledger.a_gold_after,
                ledger.b_gold_before,
                ledger.b_gold_after,
                ledger.a_items,
                ledger.b_items,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn load_world_time(&self) -> Result<Option<GameDateTime>, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT year, month, day, hour, minute FROM world_time WHERE id = 1",
                [],
                |row| {
                    Ok(GameDateTime {
                        year: row.get(0)?,
                        month: row.get(1)?,
                        day: row.get(2)?,
                        hour: row.get(3)?,
                        minute: row.get(4)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_world_time(&self, datetime: &GameDateTime) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        Self::write_world_time(&conn, datetime)?;
        Ok(())
    }

    /// Load all items for a character.
    pub fn load_inventory(&self, character_id: i64) -> Result<Vec<ItemRow>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT item_def_id, quantity, equip_slot, enchant, cape_color, cape_texture, locked, \
             item_id FROM character_items WHERE character_id = ?1",
        )?;
        let rows = stmt
            .query_map(params![character_id], |row| {
                Ok(ItemRow {
                    locked: row.get(6)?,
                    item_def_id: row.get(0)?,
                    quantity: row.get(1)?,
                    equip_slot: row.get(2)?,
                    enchant: row.get(3)?,
                    cape_color: row.get(4)?,
                    cape_texture: row.get(5)?,
                    item_id: row.get::<_, Option<u64>>(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Load learned skills; an absent row means the skill is not learned.
    pub fn load_skills(&self, character_id: i64) -> Result<Vec<SkillRow>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt =
            conn.prepare("SELECT skill_id FROM character_skills WHERE character_id = ?1")?;
        let rows = stmt
            .query_map(params![character_id], |row| {
                Ok(SkillRow {
                    skill_id: row.get(0)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::create;
    use crate::test_util::temp_auth;
    use std::collections::HashSet;

    #[test]
    fn save_batch_skips_discovery_rows_for_deleted_characters() {
        let (auth, _) = temp_auth("auth_disc");
        let player = auth.login_google("sub-disc").unwrap();
        let kept = create(&auth, &player, "Kept").unwrap().id;
        let deleted = create(&auth, &player, "Doomed").unwrap().id;
        auth.delete_character(&player, deleted).unwrap();

        let discoveries = vec![
            (deleted, "old_crypt".to_string()),
            (kept, "old_crypt".to_string()),
        ];
        auth.save_batch(&[], &[], &[], &discoveries, None).unwrap();

        let conn = auth.open_connection().unwrap();
        let rows: Vec<i64> = conn
            .prepare("SELECT character_id FROM character_dungeon_discoveries")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(rows, vec![kept]);
    }

    #[test]
    fn skills_round_trip_and_upsert_preserves_unknown_rows() {
        let (auth, _) = temp_auth("auth_skills");
        let account = auth.login_npc("npc_skills_test").unwrap();
        let record = create(&auth, &account, "Fisherman").unwrap();

        assert!(auth.load_skills(record.id).unwrap().is_empty());

        auth.save_batch(
            &[],
            &[],
            &[(
                record.id,
                vec![SkillRow {
                    skill_id: "underwater_basketweaving".to_string(),
                }],
            )],
            &[],
            None,
        )
        .unwrap();

        for _ in 0..2 {
            auth.save_batch(
                &[],
                &[],
                &[(
                    record.id,
                    vec![SkillRow {
                        skill_id: "fishing".to_string(),
                    }],
                )],
                &[],
                None,
            )
            .unwrap();
            let mut rows = auth.load_skills(record.id).unwrap();
            rows.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));
            assert_eq!(rows.len(), 2);
            assert_eq!(rows[0].skill_id, "fishing");
            assert_eq!(rows[1].skill_id, "underwater_basketweaving");
        }
        let beginner = create(&auth, &account, "Beginner").unwrap();
        auth.save_batch(
            &[],
            &[],
            &[(
                beginner.id,
                vec![SkillRow {
                    skill_id: "fishing".into(),
                }],
            )],
            &[],
            None,
        )
        .unwrap();
        let rows = auth.load_skills(beginner.id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].skill_id, "fishing");
        let conn = auth.open_connection().unwrap();
        assert_eq!(
            AuthService::table_columns(&conn, "character_skills").unwrap(),
            HashSet::from(["character_id".into(), "skill_id".into()])
        );
    }

    #[test]
    fn startup_removes_skill_progress_and_preserves_learned_skills() {
        let db_path = crate::test_util::unique_temp_dir("skill_progress_migration").join("game.db");
        drop(AuthService::new(db_path.clone()).unwrap());
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             ALTER TABLE character_skills ADD COLUMN level INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE character_skills ADD COLUMN xp INTEGER NOT NULL DEFAULT 0;
             INSERT INTO accounts (player_name) VALUES ('skill_migration');
             INSERT INTO characters (id, account_name, character_name, level, xp) VALUES
                (1, 'skill_migration', 'Novice', 1, 0),
                (2, 'skill_migration', 'Angler', 10, 1234),
                (3, 'skill_migration', 'Master', 30, 50000);
             INSERT INTO character_skills (character_id, skill_id, level, xp) VALUES
                (1, 'fishing', 0, 0),
                (2, 'fishing', 0, 10),
                (3, 'fishing', 30, 945500),
                (2, 'unknown_skill', 5, 999);",
        )
        .unwrap();

        for _ in 0..2 {
            let auth = AuthService::new(db_path.clone()).unwrap();
            assert_eq!(
                AuthService::table_columns(&conn, "character_skills").unwrap(),
                HashSet::from(["character_id".into(), "skill_id".into()])
            );
            for character_id in 1..=3 {
                let rows = auth.load_skills(character_id).unwrap();
                assert!(crate::game_state::skills_from_rows(&rows)
                    .has(onlinerpg_shared::skills::SkillId::Fishing));
                assert_eq!(rows.len(), if character_id == 2 { 2 } else { 1 });
            }
            assert!(auth
                .load_skills(2)
                .unwrap()
                .iter()
                .any(|row| row.skill_id == "unknown_skill"));
            assert_eq!(
                conn.query_row("SELECT level, xp FROM characters WHERE id = 2", [], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })
                .unwrap(),
                (10, 1234)
            );
        }

        assert!(conn
            .execute(
                "INSERT INTO character_skills (character_id, skill_id) VALUES (1, 'fishing')",
                []
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO character_skills (character_id, skill_id) VALUES (99, 'fishing')",
                []
            )
            .is_err());
        conn.execute("DELETE FROM characters WHERE id = 2", [])
            .unwrap();
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_skills", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(remaining, 2);
    }
}
