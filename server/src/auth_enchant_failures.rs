use super::{AuthError, AuthService};
use crate::metrics::{WeaponEnchantFailure, WeaponEnchantFailureGroup, WeaponEnchantFailures};
use rusqlite::{params, Connection};

impl AuthService {
    pub(super) fn ensure_weapon_enchant_failures_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS weapon_enchant_failure_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO weapon_enchant_failure_collection VALUES (1, unixepoch());
             CREATE TABLE IF NOT EXISTS weapon_enchant_failures (
                id INTEGER PRIMARY KEY,
                event_id TEXT NOT NULL UNIQUE,
                timestamp INTEGER NOT NULL,
                character_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                item_def_id TEXT NOT NULL,
                item_name TEXT NOT NULL,
                enchant INTEGER NOT NULL CHECK (enchant >= 0)
             );
             CREATE INDEX IF NOT EXISTS weapon_enchant_failures_recent
                ON weapon_enchant_failures(timestamp DESC, id DESC);
             CREATE INDEX IF NOT EXISTS weapon_enchant_failures_group
                ON weapon_enchant_failures(character_id, item_def_id, enchant, timestamp DESC, id DESC);",
        )
    }

    pub fn record_weapon_enchant_failures(
        &self,
        failures: &[WeaponEnchantFailure],
    ) -> Result<(), AuthError> {
        if failures.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        let mut statement = transaction.prepare_cached(
            "INSERT INTO weapon_enchant_failures
                (event_id, timestamp, character_id, name, item_def_id, item_name, enchant)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(event_id) DO NOTHING",
        )?;
        for failure in failures {
            statement.execute(params![
                failure.id,
                failure.timestamp,
                failure.character_id,
                failure.name,
                failure.item_def_id,
                failure.item_name,
                failure.enchant
            ])?;
        }
        drop(statement);
        transaction.commit()?;
        Ok(())
    }

    pub fn weapon_enchant_failures(&self, until: i64) -> Result<WeaponEnchantFailures, AuthError> {
        let conn = self.open_connection()?;
        let collection_started_at = conn.query_row(
            "SELECT started_at FROM weapon_enchant_failure_collection WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "WITH grouped AS (
                SELECT character_id, item_def_id, enchant,
                       MAX(timestamp) AS timestamp, COUNT(*) AS failure_count
                FROM weapon_enchant_failures WHERE timestamp <= ?1
                GROUP BY character_id, item_def_id, enchant
             )
             SELECT f.event_id, f.timestamp, f.character_id, f.name,
                    f.item_def_id, f.item_name, f.enchant, g.failure_count
             FROM grouped g
             JOIN weapon_enchant_failures f ON f.id = (
                SELECT id FROM weapon_enchant_failures
                WHERE character_id = g.character_id AND item_def_id = g.item_def_id
                  AND enchant = g.enchant AND timestamp = g.timestamp
                ORDER BY id DESC LIMIT 1
             )
             ORDER BY f.timestamp DESC, f.id DESC LIMIT 10",
        )?;
        let entries = statement
            .query_map([until], |row| {
                Ok(WeaponEnchantFailureGroup {
                    latest: WeaponEnchantFailure {
                        id: row.get(0)?,
                        timestamp: row.get(1)?,
                        character_id: row.get(2)?,
                        name: row.get(3)?,
                        item_def_id: row.get(4)?,
                        item_name: row.get(5)?,
                        enchant: row.get(6)?,
                    },
                    failure_count: row.get(7)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(WeaponEnchantFailures {
            until,
            collection_started_at,
            entries,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weapon_enchant_failures_limit_groups_after_counting_all_matching_events() {
        let path = crate::test_util::unique_temp_dir("enchant_failure_groups").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let now = crate::auth::unix_now();
        let failures: Vec<_> = (0..12)
            .map(|index| WeaponEnchantFailure {
                id: format!("character-{index}"),
                timestamp: now - 60 + index,
                character_id: index + 1,
                name: "Reader".into(),
                item_def_id: "iron_sword".into(),
                item_name: "Iron Sword".into(),
                enchant: 8,
            })
            .collect();
        auth.record_weapon_enchant_failures(&failures).unwrap();
        let other_weapon = WeaponEnchantFailure {
            id: "other-weapon".into(),
            timestamp: now - 1,
            item_def_id: "dagger".into(),
            item_name: "Dagger".into(),
            ..failures[0].clone()
        };
        let other_grade = WeaponEnchantFailure {
            id: "other-grade".into(),
            timestamp: now - 1,
            enchant: 7,
            ..failures[0].clone()
        };
        auth.record_weapon_enchant_failures(&[other_weapon.clone(), other_grade.clone()])
            .unwrap();
        let repeats: Vec<_> = (0..20)
            .map(|index| WeaponEnchantFailure {
                id: format!("repeat-{index}"),
                timestamp: now,
                name: "Renamed Reader".into(),
                item_name: "Renamed Sword".into(),
                ..failures[0].clone()
            })
            .collect();
        auth.record_weapon_enchant_failures(&repeats).unwrap();
        let older = WeaponEnchantFailure {
            id: "late-save".into(),
            ..failures[0].clone()
        };
        auth.record_weapon_enchant_failures(&[older]).unwrap();

        let result = auth.weapon_enchant_failures(now).unwrap();
        assert_eq!(result.entries.len(), 10);
        assert_eq!(result.entries[0].latest, repeats[19]);
        assert_eq!(result.entries[0].failure_count, 22);
        assert_eq!(result.entries[1].latest, other_grade);
        assert_eq!(result.entries[2].latest, other_weapon);
        for (entry, expected) in result.entries[3..].iter().zip(failures[5..].iter().rev()) {
            assert_eq!(entry.latest, *expected);
        }
        assert!(result.entries[1..]
            .iter()
            .all(|entry| entry.failure_count == 1));
    }

    #[test]
    fn weapon_enchant_failures_group_repeated_losses_and_survive_restart() {
        let path = crate::test_util::unique_temp_dir("enchant_failure_history").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let now = crate::auth::unix_now();
        let empty = auth.weapon_enchant_failures(now).unwrap();
        assert!(empty.entries.is_empty());
        let failures: Vec<_> = (0..12)
            .map(|index| WeaponEnchantFailure {
                id: format!("failure-{index}"),
                timestamp: now - 60 + index / 2,
                character_id: 1,
                name: "Reader".into(),
                item_def_id: "iron_sword".into(),
                item_name: "Iron Sword".into(),
                enchant: if index % 2 == 0 { 8 } else { 7 },
            })
            .collect();
        auth.record_weapon_enchant_failures(&failures).unwrap();
        auth.record_weapon_enchant_failures(&failures).unwrap();
        let future = WeaponEnchantFailure {
            id: "future".into(),
            timestamp: now + 1,
            ..failures[0].clone()
        };
        auth.record_weapon_enchant_failures(&[future]).unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let result = auth.weapon_enchant_failures(now).unwrap();
        assert_eq!(result.collection_started_at, empty.collection_started_at);
        assert_eq!(result.entries.len(), 2);
        assert_eq!(result.entries[0].latest, failures[11]);
        assert_eq!(result.entries[1].latest, failures[10]);
        assert!(result.entries.iter().all(|entry| entry.failure_count == 6));
        let conn = auth.open_connection().unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM weapon_enchant_failures", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            13
        );
    }
}
