use super::{AuthError, AuthService, NPC_ACCOUNT_PREFIX};
use crate::metrics::LootTally;
use rusqlite::{params, params_from_iter, Connection};
use std::collections::HashMap;

pub struct StoredLootTally {
    pub collection_started_at: i64,
    /// (monster_type, effective level, kills)
    pub kills: Vec<(String, u8, u64)>,
    /// item_def_id → (kill drops, other drops)
    pub drops: HashMap<String, (u64, u64)>,
    /// item_def_id → (player bags, NPC bags, estate storage)
    pub holdings: HashMap<String, (u64, u64, u64)>,
}

impl AuthService {
    pub(super) fn ensure_loot_tally_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS loot_tally_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO loot_tally_collection (id, started_at) VALUES (1, unixepoch());
             CREATE TABLE IF NOT EXISTS monster_kill_samples (
                timestamp INTEGER NOT NULL,
                monster_type TEXT NOT NULL,
                level INTEGER NOT NULL CHECK (level BETWEEN 0 AND 255),
                kills INTEGER NOT NULL CHECK (kills > 0),
                PRIMARY KEY (timestamp, monster_type, level)
             );
             CREATE TABLE IF NOT EXISTS item_drop_samples (
                item_def_id TEXT NOT NULL,
                source TEXT NOT NULL CHECK (source IN ('kill', 'other')),
                timestamp INTEGER NOT NULL,
                quantity INTEGER NOT NULL CHECK (quantity > 0),
                PRIMARY KEY (item_def_id, source, timestamp)
             );",
        )
    }

    pub fn record_loot_tally(&self, tally: &LootTally) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        {
            let mut kills = transaction.prepare_cached(
                "INSERT INTO monster_kill_samples (timestamp, monster_type, level, kills)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(timestamp, monster_type, level) DO UPDATE SET
                    kills = kills + excluded.kills",
            )?;
            for ((hour, monster_type, level), count) in &tally.kills {
                kills.execute(params![hour, monster_type, level, count])?;
            }
            let mut drops = transaction.prepare_cached(
                "INSERT INTO item_drop_samples (item_def_id, source, timestamp, quantity)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(item_def_id, source, timestamp) DO UPDATE SET
                    quantity = quantity + excluded.quantity",
            )?;
            for ((hour, item_def_id, from_kill), count) in &tally.drops {
                let source = if *from_kill { "kill" } else { "other" };
                drops.execute(params![item_def_id, source, hour, count])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn loot_tally(&self, item_def_ids: &[&str]) -> Result<StoredLootTally, AuthError> {
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        let collection_started_at = transaction.query_row(
            "SELECT MIN(started_at, COALESCE((SELECT MIN(timestamp) FROM monster_kill_samples), started_at))
             FROM loot_tally_collection WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let kills = transaction
            .prepare(
                "SELECT monster_type, level, SUM(kills) FROM monster_kill_samples
                 GROUP BY monster_type, level",
            )?
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let ids = vec!["?"; item_def_ids.len()].join(", ");
        let mut drops = HashMap::new();
        let mut statement = transaction.prepare(&format!(
            "SELECT item_def_id,
                    COALESCE(SUM(quantity) FILTER (WHERE source = 'kill'), 0),
                    COALESCE(SUM(quantity) FILTER (WHERE source = 'other'), 0)
             FROM item_drop_samples WHERE item_def_id IN ({ids}) GROUP BY item_def_id"
        ))?;
        let mut rows = statement.query(params_from_iter(item_def_ids))?;
        while let Some(row) = rows.next()? {
            drops.insert(row.get(0)?, (row.get(1)?, row.get(2)?));
        }

        let mut holdings: HashMap<String, (u64, u64, u64)> = HashMap::new();
        let npc_glob = format!("{NPC_ACCOUNT_PREFIX}*");
        let mut statement = transaction.prepare(&format!(
            "SELECT i.item_def_id,
                    COALESCE(SUM(i.quantity) FILTER (WHERE NOT c.account_name GLOB ?1), 0),
                    COALESCE(SUM(i.quantity) FILTER (WHERE c.account_name GLOB ?1), 0)
             FROM character_items i JOIN characters c ON c.id = i.character_id
             WHERE i.item_def_id IN ({ids}) GROUP BY i.item_def_id"
        ))?;
        let mut rows = statement.query(params_from_iter(
            std::iter::once(npc_glob.as_str()).chain(item_def_ids.iter().copied()),
        ))?;
        while let Some(row) = rows.next()? {
            let held = holdings.entry(row.get(0)?).or_default();
            (held.0, held.1) = (row.get(1)?, row.get(2)?);
        }
        let mut statement = transaction.prepare(&format!(
            "SELECT item_def_id, SUM(quantity) FROM estate_chest_items
             WHERE item_def_id IN ({ids}) GROUP BY item_def_id"
        ))?;
        let mut rows = statement.query(params_from_iter(item_def_ids))?;
        while let Some(row) = rows.next()? {
            holdings.entry(row.get(0)?).or_default().2 = row.get(1)?;
        }
        Ok(StoredLootTally {
            collection_started_at,
            kills,
            drops,
            holdings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::SAMPLE_INTERVAL_SECONDS;

    #[test]
    fn loot_tally_accumulates_hourly_and_counts_holdings_by_owner() {
        let path = crate::test_util::unique_temp_dir("loot_tally").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let hour = 1_700_000_000 - 1_700_000_000 % SAMPLE_INTERVAL_SECONDS;
        let mut tally = LootTally::default();
        tally.kills.insert((hour, "goblin".into(), 3), 2);
        tally.drops.insert((hour, "stethoscope".into(), true), 1);
        tally.drops.insert((hour, "stethoscope".into(), false), 1);
        auth.record_loot_tally(&tally).unwrap();
        auth.record_loot_tally(&tally).unwrap();

        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test');
                 INSERT INTO characters (id, account_name, character_name) VALUES
                    (1, 'player', 'Hero'), (2, 'npc_test', 'Npc');
                 INSERT INTO character_items (character_id, item_def_id, quantity) VALUES
                    (1, 'stethoscope', 1), (1, 'stethoscope', 2), (2, 'stethoscope', 4);
                 INSERT INTO estate_chests (id, estate_id, owner_id, x, y, z, rotation_deg, floor_level)
                    VALUES (10, 7, 1, 0, 0, 0, 0, 0);
                 INSERT INTO estate_chest_items (chest_id, item_def_id, quantity)
                    VALUES (10, 'stethoscope', 5);",
            )
            .unwrap();

        let stored = auth.loot_tally(&["stethoscope", "apple"]).unwrap();
        assert_eq!(stored.kills, vec![("goblin".to_string(), 3, 4)]);
        assert_eq!(stored.collection_started_at, hour);
        assert_eq!(stored.drops["stethoscope"], (2, 2));
        assert!(!stored.drops.contains_key("apple"));
        assert_eq!(stored.holdings["stethoscope"], (3, 4, 5));
    }
}
