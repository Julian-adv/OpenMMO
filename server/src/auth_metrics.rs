use super::{unix_now, AuthError, AuthService, NPC_ACCOUNT_PREFIX};
use crate::metrics::{
    kst_day_start, AccountActivity, CharacterGoldSample, CharacterGoldSeries, ConcurrentCounts,
    ConcurrentHistorySample, GoldHistory, GoldHistorySample, GoldLeaderboard, GoldLeaderboardEntry,
    GoldSample, LevelLeaderboard, LevelLeaderboardEntry, LevelSample, LevelSeries,
    PerAccountGoldHistory, PerAccountGoldHistorySample, PerAccountGoldSample, UniqueHistory,
    UniqueSample, DAY_SECONDS, SAMPLE_INTERVAL_SECONDS,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::{HashMap, HashSet};

fn unique_collection_started_at(conn: &Connection) -> Result<i64, rusqlite::Error> {
    conn.query_row(
        "SELECT started_at FROM unique_account_collection WHERE id = 1",
        [],
        |row| row.get(0),
    )
}

impl AuthService {
    pub(super) fn ensure_level_history_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::ensure_character_history_schema(conn, "level", 1)
    }

    pub(super) fn ensure_gold_history_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::ensure_character_history_schema(conn, "gold", 0)
    }

    fn ensure_character_history_schema(
        conn: &Connection,
        metric: &str,
        minimum: u32,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS character_{metric}_history (
                character_id INTEGER NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
                timestamp INTEGER NOT NULL,
                {metric} INTEGER NOT NULL CHECK ({metric} >= {minimum}),
                PRIMARY KEY (character_id, timestamp)
             );
             INSERT INTO character_{metric}_history (character_id, timestamp, {metric})
             SELECT id, CAST(strftime('%s', 'now') AS INTEGER), {metric} FROM characters c
             WHERE account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*'
                AND NOT EXISTS (SELECT 1 FROM character_{metric}_history WHERE character_id = c.id);
             CREATE TRIGGER IF NOT EXISTS character_{metric}_created AFTER INSERT ON characters
             WHEN NEW.account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*'
             BEGIN
                INSERT INTO character_{metric}_history VALUES (NEW.id, CAST(strftime('%s', 'now') AS INTEGER), NEW.{metric});
             END;
             CREATE TRIGGER IF NOT EXISTS character_{metric}_changed AFTER UPDATE OF {metric} ON characters
             WHEN NEW.{metric} != OLD.{metric} AND NEW.account_name NOT GLOB '{NPC_ACCOUNT_PREFIX}*'
             BEGIN
                INSERT INTO character_{metric}_history VALUES (NEW.id, CAST(strftime('%s', 'now') AS INTEGER), NEW.{metric})
                ON CONFLICT (character_id, timestamp) DO UPDATE SET {metric} = excluded.{metric};
             END;"
        ))
    }

    pub fn level_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<LevelLeaderboard, AuthError> {
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let mut statement = transaction.prepare(
            "SELECT character_name, level, account_name, id FROM characters
             WHERE account_name NOT GLOB ?1
             ORDER BY level DESC, xp DESC, id ASC LIMIT 10",
        )?;
        let rows = statement
            .query_map([format!("{NPC_ACCOUNT_PREFIX}*")], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, u32>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let until = unix_now();
        let from = until - i64::from(hours) * 3600;
        let mut first_ranks = HashMap::new();
        let mut entries = Vec::new();
        let mut series = Vec::new();
        let mut history = transaction.prepare(
            "SELECT timestamp, level FROM character_level_history
             WHERE character_id = ?1 AND timestamp IN (
                SELECT MIN(timestamp) FROM character_level_history WHERE character_id = ?1 AND timestamp <= ?3
                UNION SELECT MAX(timestamp) FROM character_level_history WHERE character_id = ?1 AND timestamp <= ?2
                UNION SELECT MAX(timestamp) FROM character_level_history
                    WHERE character_id = ?1 AND timestamp > ?2 AND timestamp <= ?3 GROUP BY timestamp / ?4
             ) ORDER BY timestamp",
        )?;
        for (index, (name, level, account, id)) in rows.into_iter().enumerate() {
            let recorded = history
                .query_map(params![id, from, until, interval], |row| {
                    Ok(LevelSample {
                        timestamp: row.get(0)?,
                        level: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(first) = recorded.first() {
                let started_at = first.timestamp;
                let baseline = recorded
                    .iter()
                    .rposition(|sample| sample.timestamp <= from)
                    .unwrap_or(0);
                let samples = recorded
                    .into_iter()
                    .skip(baseline)
                    .map(|mut sample| {
                        sample.timestamp = sample.timestamp.max(from);
                        sample
                    })
                    .collect();
                series.push(LevelSeries {
                    name: name.clone(),
                    started_at,
                    samples,
                });
            }
            entries.push(LevelLeaderboardEntry {
                name,
                level,
                account_first_rank: *first_ranks.entry(account).or_insert(index + 1),
            });
        }
        Ok(LevelLeaderboard {
            timestamp: until,
            from,
            sample_interval_seconds: interval,
            entries,
            series,
        })
    }

    pub fn gold_leaderboard(
        &self,
        hours: u32,
        interval: i64,
    ) -> Result<GoldLeaderboard, AuthError> {
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let mut statement = transaction.prepare(
            "SELECT character_name, gold, account_name, id FROM characters
             WHERE account_name NOT GLOB ?1
             ORDER BY gold DESC, id ASC LIMIT 10",
        )?;
        let rows = statement
            .query_map([format!("{NPC_ACCOUNT_PREFIX}*")], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let until = unix_now();
        let from = until - i64::from(hours) * 3600;
        let mut first_ranks = HashMap::new();
        let mut entries = Vec::new();
        let mut series = Vec::new();
        let mut history = transaction.prepare(
            "SELECT timestamp, gold FROM character_gold_history
             WHERE character_id = ?1 AND timestamp IN (
                SELECT MIN(timestamp) FROM character_gold_history WHERE character_id = ?1 AND timestamp <= ?3
                UNION SELECT MAX(timestamp) FROM character_gold_history WHERE character_id = ?1 AND timestamp <= ?2
                UNION SELECT MAX(timestamp) FROM character_gold_history
                    WHERE character_id = ?1 AND timestamp > ?2 AND timestamp <= ?3 GROUP BY timestamp / ?4
             ) ORDER BY timestamp",
        )?;
        for (index, (name, gold, account, id)) in rows.into_iter().enumerate() {
            let recorded = history
                .query_map(params![id, from, until, interval], |row| {
                    Ok(CharacterGoldSample {
                        timestamp: row.get(0)?,
                        gold: row.get(1)?,
                    })
                })?
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(first) = recorded.first() {
                let started_at = first.timestamp;
                let baseline = recorded
                    .iter()
                    .rposition(|sample| sample.timestamp <= from)
                    .unwrap_or(0);
                let samples = recorded
                    .into_iter()
                    .skip(baseline)
                    .map(|mut sample| {
                        sample.timestamp = sample.timestamp.max(from);
                        sample
                    })
                    .collect();
                series.push(CharacterGoldSeries {
                    name: name.clone(),
                    started_at,
                    samples,
                });
            }
            entries.push(GoldLeaderboardEntry {
                name,
                gold,
                account_first_rank: *first_ranks.entry(account).or_insert(index + 1),
            });
        }
        Ok(GoldLeaderboard {
            timestamp: until,
            from,
            sample_interval_seconds: interval,
            entries,
            series,
        })
    }

    pub fn per_account_gold_history(
        &self,
        until: i64,
        hours: u32,
        interval: i64,
        active_days: u32,
    ) -> Result<PerAccountGoldHistory, AuthError> {
        let from = until - i64::from(hours) * 3600;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        let joined = "WITH normalized AS (
            SELECT g.ts, g.total_gold,
                CASE ?1 WHEN 1 THEN u.day_accounts WHEN 7 THEN u.week_accounts
                    WHEN 30 THEN u.month_accounts WHEN 180 THEN u.half_year_accounts
                    WHEN 365 THEN u.year_accounts END AS accounts
            FROM gold_snapshots g
            JOIN unique_account_daily_samples u
                ON u.timestamp = g.ts - (g.ts + 9 * 3600) % 86400
            WHERE g.ts >= ?2 AND g.ts <= ?3
        )";
        let latest_timestamp: Option<i64> = transaction.query_row(
            "SELECT MAX(ts) FROM gold_snapshots WHERE ts <= ?1",
            [until],
            |row| row.get(0),
        )?;
        let latest = transaction
            .query_row(
                &format!(
                    "{joined}
                SELECT ts, total_gold, accounts, total_gold * 1.0 / accounts
                FROM normalized WHERE accounts > 0"
                ),
                params![active_days, latest_timestamp, latest_timestamp],
                |row| {
                    Ok(PerAccountGoldSample {
                        timestamp: row.get(0)?,
                        total_gold: row.get(1)?,
                        accounts: row.get(2)?,
                        gold_per_account: row.get(3)?,
                    })
                },
            )
            .optional()?;
        let mut statement = transaction.prepare(&format!(
            "{joined}
            SELECT MAX((ts / ?4) * ?4, ?2), AVG(total_gold * 1.0 / accounts),
                MAX(total_gold * 1.0 / accounts), COUNT(*)
            FROM normalized WHERE accounts > 0
            GROUP BY ts / ?4 ORDER BY ts / ?4"
        ))?;
        let samples = statement
            .query_map(params![active_days, from, until, interval], |row| {
                Ok(PerAccountGoldHistorySample {
                    timestamp: row.get(0)?,
                    gold_per_account: row.get(1)?,
                    peak_gold_per_account: row.get(2)?,
                    sample_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(PerAccountGoldHistory {
            from,
            until,
            sample_interval_seconds: interval,
            window_seconds: i64::from(active_days) * DAY_SECONDS,
            collection_started_at,
            latest,
            samples,
        })
    }

    pub fn gold_history(
        &self,
        until: i64,
        hours: u32,
        interval: i64,
    ) -> Result<GoldHistory, AuthError> {
        let from = until - i64::from(hours) * 3600;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let latest = transaction
            .query_row(
                "SELECT ts, total_gold FROM gold_snapshots WHERE ts <= ?1 ORDER BY ts DESC LIMIT 1",
                [until],
                |row| {
                    Ok(GoldSample {
                        timestamp: row.get(0)?,
                        total_gold: row.get(1)?,
                    })
                },
            )
            .optional()?;
        let mut statement = transaction.prepare(
            "SELECT MAX((ts / ?3) * ?3, ?1), AVG(total_gold), MAX(total_gold), COUNT(*)
             FROM gold_snapshots WHERE ts >= ?1 AND ts <= ?2
             GROUP BY ts / ?3 ORDER BY ts / ?3",
        )?;
        let samples = statement
            .query_map(params![from, until, interval], |row| {
                Ok(GoldHistorySample {
                    timestamp: row.get(0)?,
                    total_gold: row.get(1)?,
                    peak_gold: row.get(2)?,
                    sample_count: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(GoldHistory {
            from,
            until,
            sample_interval_seconds: interval,
            latest,
            samples,
        })
    }

    pub(super) fn ensure_account_activity_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS unique_account_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO unique_account_collection VALUES (1, CAST(strftime('%s', 'now') AS INTEGER));
             CREATE TABLE IF NOT EXISTS account_activity_sessions (
                id TEXT PRIMARY KEY,
                account_name TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL CHECK (last_seen_at >= started_at)
             );
             CREATE INDEX IF NOT EXISTS account_activity_last_seen ON account_activity_sessions(last_seen_at);
             CREATE TABLE IF NOT EXISTS unique_account_daily_samples (
                timestamp INTEGER PRIMARY KEY,
                day_accounts INTEGER NOT NULL CHECK (day_accounts >= 0),
                week_accounts INTEGER NOT NULL CHECK (week_accounts >= 0),
                month_accounts INTEGER NOT NULL CHECK (month_accounts >= 0),
                half_year_accounts INTEGER NOT NULL CHECK (half_year_accounts >= 0),
                year_accounts INTEGER NOT NULL CHECK (year_accounts >= 0)
             );",
        )
    }

    pub fn record_account_activities(
        &self,
        activities: &[AccountActivity],
    ) -> Result<(), AuthError> {
        if activities.is_empty() {
            return Ok(());
        }
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        {
            let mut statement = transaction.prepare(
                "INSERT INTO account_activity_sessions (id, account_name, started_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET last_seen_at = MAX(last_seen_at, excluded.last_seen_at)",
            )?;
            for activity in activities {
                statement.execute(params![
                    activity.id,
                    activity.account_name.to_ascii_lowercase(),
                    activity.started_at,
                    activity.last_seen_at
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn backfill_daily_unique_accounts(&self, now: i64) -> Result<usize, AuthError> {
        let missing_dates: Vec<i64> = {
            let conn = self.open_connection()?;
            let collection_started_at = unique_collection_started_at(&conn)?;
            let first = kst_day_start(collection_started_at) + DAY_SECONDS;
            let until = kst_day_start(now);
            let mut statement = conn.prepare(
                "SELECT timestamp FROM unique_account_daily_samples
                 WHERE timestamp >= ?1 AND timestamp <= ?2",
            )?;
            let saved_dates = statement
                .query_map(params![first, until], |row| row.get(0))?
                .collect::<Result<HashSet<i64>, _>>()?;
            (first..=until)
                .step_by(DAY_SECONDS as usize)
                .filter(|timestamp| !saved_dates.contains(timestamp))
                .collect()
        };
        let mut count = 0;
        for timestamp in missing_dates {
            count += usize::from(self.aggregate_daily_unique_accounts(timestamp)?);
        }
        Ok(count)
    }

    pub fn aggregate_daily_unique_accounts(&self, now: i64) -> Result<bool, AuthError> {
        let timestamp = kst_day_start(now);
        let mut conn = self.open_connection()?;
        let transaction =
            conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        if timestamp <= collection_started_at
            || transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM unique_account_daily_samples WHERE timestamp = ?1)",
                [timestamp],
                |row| row.get::<_, bool>(0),
            )?
        {
            return Ok(false);
        }
        transaction.execute(
            "WITH recent_accounts AS (
                SELECT account_name, MAX(last_seen_at) AS last_seen_at
                FROM account_activity_sessions
                WHERE started_at < ?1 AND last_seen_at >= ?1 - 365 * 86400
                GROUP BY account_name
             )
             INSERT INTO unique_account_daily_samples
                (timestamp, day_accounts, week_accounts, month_accounts, half_year_accounts, year_accounts)
             SELECT ?1,
                COUNT(CASE WHEN last_seen_at >= ?1 - 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 7 * 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 30 * 86400 THEN 1 END),
                COUNT(CASE WHEN last_seen_at >= ?1 - 180 * 86400 THEN 1 END),
                COUNT(*) FROM recent_accounts", [timestamp],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn unique_account_history(&self, now: i64, days: u32) -> Result<UniqueHistory, AuthError> {
        let until = kst_day_start(now);
        let window = i64::from(days) * DAY_SECONDS;
        let from = until - window;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        let collection_started_at = unique_collection_started_at(&transaction)?;
        let last_aggregated_at = transaction.query_row(
            "SELECT MAX(timestamp) FROM unique_account_daily_samples WHERE timestamp <= ?1",
            [until],
            |row| row.get(0),
        )?;
        let mut statement = transaction.prepare(
            "SELECT timestamp,
                CASE ?3 WHEN 1 THEN day_accounts WHEN 7 THEN week_accounts
                    WHEN 30 THEN month_accounts WHEN 180 THEN half_year_accounts
                    WHEN 365 THEN year_accounts END
             FROM unique_account_daily_samples
             WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp",
        )?;
        let samples = statement
            .query_map(params![from, until, days], |row| {
                Ok(UniqueSample {
                    timestamp: row.get(0)?,
                    accounts: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(UniqueHistory {
            from,
            until,
            window_seconds: window,
            sample_interval_seconds: DAY_SECONDS,
            collection_started_at,
            last_aggregated_at,
            samples,
        })
    }

    pub(super) fn ensure_concurrent_samples_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS concurrent_account_samples (
                timestamp INTEGER PRIMARY KEY,
                accounts INTEGER NOT NULL CHECK (accounts >= 0)
            )",
        )?;
        let columns = Self::table_columns(conn, "concurrent_account_samples")?;
        if !columns.contains("web_accounts") {
            conn.execute(
                "ALTER TABLE concurrent_account_samples
                 ADD COLUMN web_accounts INTEGER NOT NULL DEFAULT 0 CHECK (web_accounts >= 0)",
                [],
            )?;
        }
        if !columns.contains("agent_accounts") {
            conn.execute(
                "ALTER TABLE concurrent_account_samples
                 ADD COLUMN agent_accounts INTEGER NOT NULL DEFAULT 0
                 CHECK (agent_accounts >= 0 AND web_accounts + agent_accounts <= accounts)",
                [],
            )?;
        }
        Ok(())
    }

    pub fn record_concurrent_accounts(
        &self,
        now: i64,
        counts: ConcurrentCounts,
    ) -> Result<(), AuthError> {
        let timestamp = now - now.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        self.open_connection()?.execute(
            "INSERT INTO concurrent_account_samples (timestamp, accounts, web_accounts, agent_accounts)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(timestamp) DO UPDATE SET accounts = excluded.accounts,
                 web_accounts = excluded.web_accounts, agent_accounts = excluded.agent_accounts",
            params![timestamp, counts.total(), counts.web_accounts, counts.agent_accounts],
        )?;
        Ok(())
    }

    pub fn concurrent_account_samples(
        &self,
        from: i64,
        until: i64,
        interval: i64,
    ) -> Result<Vec<ConcurrentHistorySample>, AuthError> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "WITH buckets AS (
                SELECT timestamp / ?3 AS bucket, AVG(accounts) AS accounts,
                       MAX(accounts) AS peak_accounts, COUNT(*) AS sample_count,
                       AVG(web_accounts) AS web_accounts, AVG(agent_accounts) AS agent_accounts,
                       AVG(accounts - web_accounts - agent_accounts) AS other_accounts
                FROM concurrent_account_samples
                WHERE timestamp >= ?1 AND timestamp <= ?2
                GROUP BY timestamp / ?3
             )
             SELECT MAX(bucket * ?3, ?1), accounts, peak_accounts,
                    (SELECT MIN(timestamp) FROM concurrent_account_samples
                     WHERE timestamp >= MAX(bucket * ?3, ?1)
                       AND timestamp <= MIN((bucket + 1) * ?3 - 1, ?2)
                       AND accounts = buckets.peak_accounts),
                    sample_count, web_accounts, agent_accounts, other_accounts
             FROM buckets ORDER BY bucket",
        )?;
        let rows = statement.query_map(params![from, until, interval], |row| {
            Ok(ConcurrentHistorySample {
                timestamp: row.get(0)?,
                accounts: row.get(1)?,
                peak_accounts: row.get(2)?,
                peak_timestamp: row.get(3)?,
                sample_count: row.get(4)?,
                web_accounts: row.get(5)?,
                agent_accounts: row.get(6)?,
                other_accounts: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_history_seeds_existing_characters_and_tracks_saved_changes() {
        let path = crate::test_util::unique_temp_dir("level_history_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_level_created;
             DROP TRIGGER character_level_changed;
             DROP TABLE character_level_history;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, level, created_at)
             VALUES (1, 'player', 'Hero', 10, 100), (2, 'npc_test', 'Npc', 99, 100);",
        )
        .unwrap();
        let before = unix_now();
        AuthService::ensure_level_history_schema(&conn).unwrap();
        let baseline: (i64, u32) = conn
            .query_row(
                "SELECT timestamp, level FROM character_level_history WHERE character_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!((before..=unix_now()).contains(&baseline.0));
        assert_eq!(baseline.1, 10);
        conn.execute_batch(
            "UPDATE character_level_history SET timestamp = timestamp - 86400;
             UPDATE characters SET level = 10 WHERE id = 1;
             INSERT INTO characters (id, account_name, character_name) VALUES (3, 'npcxplayer', 'NewHero');"
        ).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        conn.execute_batch(
            "UPDATE characters SET level = 11 WHERE id = 1;
             UPDATE characters SET level = 9 WHERE id = 1;
             UPDATE characters SET character_name = 'Renamed' WHERE id = 1;",
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(conn.query_row("SELECT level FROM character_level_history WHERE character_id = 1 ORDER BY timestamp DESC LIMIT 1", [], |row| row.get::<_, u32>(0)).unwrap(), 9);
        conn.execute_batch("BEGIN; UPDATE characters SET level = 20 WHERE id = 1; ROLLBACK;")
            .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            count
        );
        assert_eq!(
            auth.level_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_level_history WHERE character_id IN (1, 2)",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn level_history_preserves_baselines_and_last_bucket_values_for_every_period() {
        let path = crate::test_util::unique_temp_dir("level_history_ranges").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player');
             INSERT INTO characters (id, account_name, character_name, level)
             VALUES (1, 'player', 'Hero', 10), (2, 'player', 'NewHero', 1);
             DELETE FROM character_level_history WHERE character_id = 1;",
        )
        .unwrap();
        let now = unix_now();
        let bucket = (now / DAY_SECONDS - 2) * DAY_SECONDS;
        for (timestamp, level) in [
            (now - 400 * DAY_SECONDS, 1),
            (now - 20 * DAY_SECONDS, 5),
            (bucket + 1, 6),
            (bucket + 2, 7),
            (bucket + 3, 6),
            (now - 3600, 10),
        ] {
            conn.execute(
                "INSERT INTO character_level_history VALUES (1, ?1, ?2)",
                params![timestamp, level],
            )
            .unwrap();
        }
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, DAY_SECONDS)] {
            let result = auth.level_leaderboard(hours, interval).unwrap();
            assert_eq!(result.timestamp - result.from, i64::from(hours) * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            let hero = &result.series[0];
            assert_eq!(hero.started_at, now - 400 * DAY_SECONDS);
            assert_eq!(
                hero.samples[0],
                LevelSample {
                    timestamp: result.from,
                    level: if hours == 168 { 5 } else { 1 }
                }
            );
            assert!(hero.samples.contains(&LevelSample {
                timestamp: bucket + 3,
                level: 6
            }));
            assert!(!hero.samples.iter().any(|sample| sample.level == 7));
            assert_eq!(hero.samples.last().unwrap().level, 10);
            assert!(hero
                .samples
                .windows(2)
                .all(|pair| pair[0].timestamp < pair[1].timestamp));
            let new_hero = &result.series[1];
            assert_eq!(new_hero.samples.len(), 1);
            assert_eq!(new_hero.samples[0].timestamp, new_hero.started_at);
            assert!(new_hero.started_at > result.from);
        }
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_level_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            7
        );
    }

    #[test]
    fn gold_history_seeds_existing_characters_and_tracks_saved_changes() {
        let path = crate::test_util::unique_temp_dir("gold_history_lifecycle").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "DROP TRIGGER character_gold_created;
             DROP TRIGGER character_gold_changed;
             DROP TABLE character_gold_history;
             INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');
             INSERT INTO characters (id, account_name, character_name, gold, created_at)
             VALUES (1, 'player', 'Hero', 10, 100), (2, 'npc_test', 'Npc', 99, 100);",
        )
        .unwrap();
        let before = unix_now();
        AuthService::ensure_gold_history_schema(&conn).unwrap();
        let baseline: (i64, i64) = conn
            .query_row(
                "SELECT timestamp, gold FROM character_gold_history WHERE character_id = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!((before..=unix_now()).contains(&baseline.0));
        assert_eq!(baseline.1, 10);
        conn.execute_batch(
            "UPDATE character_gold_history SET timestamp = timestamp - 86400;
             UPDATE characters SET gold = 10 WHERE id = 1;
             INSERT INTO characters (id, account_name, character_name) VALUES (3, 'npcxplayer', 'NewHero');"
        ).unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        conn.execute_batch(
            "UPDATE characters SET gold = 11 WHERE id = 1;
             UPDATE characters SET gold = 9 WHERE id = 1;
             UPDATE characters SET character_name = 'Renamed' WHERE id = 1;",
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(conn.query_row("SELECT gold FROM character_gold_history WHERE character_id = 1 ORDER BY timestamp DESC LIMIT 1", [], |row| row.get::<_, i64>(0)).unwrap(), 9);
        conn.execute_batch("BEGIN; UPDATE characters SET gold = 20 WHERE id = 1; ROLLBACK;")
            .unwrap();
        drop(AuthService::new(path).unwrap());
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            count
        );
        assert_eq!(
            auth.gold_leaderboard(168, 3600).unwrap().series[0].name,
            "Renamed"
        );
        conn.execute("DELETE FROM characters WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM character_gold_history WHERE character_id IN (1, 2)",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }

    #[test]
    fn gold_history_preserves_baselines_and_last_bucket_values_for_every_period() {
        let path = crate::test_util::unique_temp_dir("gold_history_ranges").join("game.db");
        let auth = AuthService::new(path).unwrap();
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player');
             INSERT INTO characters (id, account_name, character_name, gold)
             VALUES (1, 'player', 'Hero', 10), (2, 'player', 'NewHero', 0);
             DELETE FROM character_gold_history WHERE character_id = 1;",
        )
        .unwrap();
        let now = unix_now();
        let bucket = (now / DAY_SECONDS - 2) * DAY_SECONDS;
        for (timestamp, gold) in [
            (now - 400 * DAY_SECONDS, 0),
            (now - 20 * DAY_SECONDS, 5),
            (bucket + 1, 6),
            (bucket + 2, 7),
            (bucket + 3, 6),
            (now - 3600, 10),
        ] {
            conn.execute(
                "INSERT INTO character_gold_history VALUES (1, ?1, ?2)",
                params![timestamp, gold],
            )
            .unwrap();
        }
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, DAY_SECONDS)] {
            let result = auth.gold_leaderboard(hours, interval).unwrap();
            assert_eq!(result.timestamp - result.from, i64::from(hours) * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            let hero = &result.series[0];
            assert_eq!(hero.started_at, now - 400 * DAY_SECONDS);
            assert_eq!(
                hero.samples[0],
                CharacterGoldSample {
                    timestamp: result.from,
                    gold: if hours == 168 { 5 } else { 0 }
                }
            );
            assert!(hero.samples.contains(&CharacterGoldSample {
                timestamp: bucket + 3,
                gold: 6
            }));
            assert!(!hero.samples.iter().any(|sample| sample.gold == 7));
            assert_eq!(hero.samples.last().unwrap().gold, 10);
            assert!(hero
                .samples
                .windows(2)
                .all(|pair| pair[0].timestamp < pair[1].timestamp));
            let new_hero = &result.series[1];
            assert_eq!(new_hero.samples.len(), 1);
            assert_eq!(new_hero.samples[0].timestamp, new_hero.started_at);
            assert!(new_hero.started_at > result.from);
        }
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM character_gold_history", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            7
        );
    }

    #[test]
    fn gold_history_reuses_hourly_snapshots_and_preserves_gaps_and_peaks() {
        let path = crate::test_util::unique_temp_dir("gold_history").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        let empty = auth.gold_history(86400, 24, 3600).unwrap();
        assert_eq!(empty.latest, None);
        assert!(empty.samples.is_empty());
        auth.open_connection()
            .unwrap()
            .execute_batch(
                "INSERT INTO gold_snapshots (ts, total_gold, characters, npc_gold, active_gold, active_characters)
                 VALUES (0, 900, 0, 0, 0, 0), (3600, 0, 0, 0, 0, 0), (7200, 100, 0, 0, 0, 0),
                        (46800, 25, 0, 0, 0, 0), (90000, 999, 0, 0, 0, 0)",
            )
            .unwrap();
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        let history = auth.gold_history(86400, 24, 3600).unwrap();
        assert_eq!(history.samples.len(), 4);
        assert_eq!(history.samples[1].total_gold, 0.0);
        assert_eq!(history.samples[3].timestamp, 46800);
        assert_eq!(
            history.latest,
            Some(GoldSample {
                timestamp: 46800,
                total_gold: 25
            })
        );
        let aggregated = auth.gold_history(86700, 24, 21600).unwrap();
        assert_eq!(
            aggregated.samples,
            vec![
                GoldHistorySample {
                    timestamp: 300,
                    total_gold: 50.0,
                    peak_gold: 100,
                    sample_count: 2
                },
                GoldHistorySample {
                    timestamp: 43200,
                    total_gold: 25.0,
                    peak_gold: 25,
                    sample_count: 1
                },
            ]
        );
        let stale = auth.gold_history(86400, 1, 3600).unwrap();
        assert!(stale.samples.is_empty());
        assert_eq!(stale.latest, history.latest);
    }

    fn activity(id: &str, account: &str, start: i64, end: i64) -> AccountActivity {
        AccountActivity {
            id: id.into(),
            account_name: account.into(),
            started_at: start,
            last_seen_at: end,
        }
    }

    #[test]
    fn per_account_gold_uses_each_kst_days_counts_before_averaging() {
        let midnight = 1000 * DAY_SECONDS - 9 * 3600;
        let (auth, _) = unique_auth("per_account_gold", midnight - 365 * DAY_SECONDS);
        let empty = auth
            .per_account_gold_history(midnight, 24, 3600, 1)
            .unwrap();
        assert!(empty.samples.is_empty());
        assert_eq!(empty.latest, None);
        let conn = auth.open_connection().unwrap();
        for (timestamp, multiplier) in [
            (midnight - DAY_SECONDS, 1),
            (midnight, 2),
            (midnight + DAY_SECONDS, 0),
        ] {
            conn.execute(
                "INSERT INTO unique_account_daily_samples VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    timestamp,
                    2 * multiplier,
                    4 * multiplier,
                    5 * multiplier,
                    10 * multiplier,
                    20 * multiplier
                ],
            )
            .unwrap();
        }
        for (timestamp, gold) in [
            (midnight - 3600, 100),
            (midnight, 100),
            (midnight + 3600, 0),
            (midnight + DAY_SECONDS, 500),
            (midnight + 2 * DAY_SECONDS, 500),
            (midnight + 3 * DAY_SECONDS, 900),
        ] {
            conn.execute(
                "INSERT INTO gold_snapshots (ts, total_gold, characters, npc_gold, active_gold, active_characters)
                 VALUES (?1, ?2, 0, 0, 0, 0)",
                params![timestamp, gold],
            ).unwrap();
        }
        for (days, accounts) in [(1, 4), (7, 8), (30, 10), (180, 20), (365, 40)] {
            let history = auth
                .per_account_gold_history(midnight, 24, 3600, days)
                .unwrap();
            assert_eq!(history.window_seconds, i64::from(days) * DAY_SECONDS);
            assert_eq!(
                history.latest,
                Some(PerAccountGoldSample {
                    timestamp: midnight,
                    total_gold: 100,
                    accounts,
                    gold_per_account: 100.0 / f64::from(accounts),
                })
            );
            assert_eq!(
                history.samples[0].gold_per_account,
                200.0 / f64::from(accounts)
            );
            assert_eq!(
                history.samples[1].gold_per_account,
                100.0 / f64::from(accounts)
            );
        }
        for (hours, interval) in [(4320, 21600), (8760, DAY_SECONDS)] {
            let history = auth
                .per_account_gold_history(midnight + 3600, hours, interval, 1)
                .unwrap();
            assert_eq!(
                history.samples,
                vec![PerAccountGoldHistorySample {
                    timestamp: (midnight - 3600) / interval * interval,
                    gold_per_account: 25.0,
                    peak_gold_per_account: 50.0,
                    sample_count: 3,
                }]
            );
            assert_eq!(history.latest.unwrap().gold_per_account, 0.0);
        }
        for until in [midnight + DAY_SECONDS, midnight + 2 * DAY_SECONDS] {
            let history = auth.per_account_gold_history(until, 168, 3600, 1).unwrap();
            assert_eq!(history.latest, None);
            assert_eq!(history.samples.len(), 3);
        }
        let stale = auth
            .per_account_gold_history(midnight + DAY_SECONDS - 1, 1, 3600, 1)
            .unwrap();
        assert!(stale.samples.is_empty());
        assert_eq!(stale.latest.unwrap().timestamp, midnight + 3600);
        let partial = auth
            .per_account_gold_history(midnight + 300, 1, 3600, 1)
            .unwrap();
        assert_eq!(partial.samples.len(), 1);
        assert_eq!(partial.samples[0].timestamp, midnight);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM unique_account_daily_samples",
                [],
                |row| row.get::<_, u32>(0)
            )
            .unwrap(),
            3
        );
    }

    fn unique_auth(name: &str, started_at: i64) -> (AuthService, std::path::PathBuf) {
        let path = crate::test_util::unique_temp_dir(name).join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        auth.open_connection()
            .unwrap()
            .execute(
                "UPDATE unique_account_collection SET started_at = ?1",
                [started_at],
            )
            .unwrap();
        (auth, path)
    }

    #[test]
    fn daily_unique_counts_deduplicate_each_window_and_include_overlapping_sessions() {
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_windows", end - 400 * DAY_SECONDS);
        auth.record_account_activities(&[
            activity(
                "a1",
                "Alice",
                end - 100 * DAY_SECONDS,
                end - 100 * DAY_SECONDS,
            ),
            activity("a2", "alice", end - 100, end - 1),
            activity("b", "bob", end - 10 * DAY_SECONDS, end + DAY_SECONDS),
            activity("c", "carol", end - 3 * DAY_SECONDS, end - 3 * DAY_SECONDS),
            activity("d", "dan", end - 20 * DAY_SECONDS, end - 20 * DAY_SECONDS),
            activity("e", "eve", end - 100 * DAY_SECONDS, end - 100 * DAY_SECONDS),
            activity(
                "f",
                "frank",
                end - 300 * DAY_SECONDS,
                end - 300 * DAY_SECONDS,
            ),
            activity(
                "old",
                "old",
                end - 366 * DAY_SECONDS,
                end - 366 * DAY_SECONDS,
            ),
            activity("future", "future", end, end),
            activity("short", "short", end - 1, end - 1),
        ])
        .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(end + 30).unwrap());
        for (days, accounts) in [(1, 3), (7, 4), (30, 5), (180, 6), (365, 7)] {
            let history = auth.unique_account_history(end + 123, days).unwrap();
            assert_eq!(
                history.samples,
                vec![UniqueSample {
                    timestamp: end,
                    accounts
                }]
            );
            assert_eq!(history.last_aggregated_at, Some(end));
            assert_eq!(history.sample_interval_seconds, DAY_SECONDS);
            assert_eq!(history.until - history.from, i64::from(days) * DAY_SECONDS);
        }
        assert!(auth
            .aggregate_daily_unique_accounts(end + DAY_SECONDS)
            .unwrap());
        assert_eq!(
            auth.unique_account_history(end + DAY_SECONDS, 1)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            2
        );
    }

    #[test]
    fn daily_unique_results_survive_restart_and_reads_never_recalculate() {
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_daily_cache", end - DAY_SECONDS);
        auth.record_account_activities(&[activity("a", "alice", end - 100, end - 1)])
            .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(end).unwrap());
        auth.record_account_activities(&[activity("late", "bob", end - 50, end - 1)])
            .unwrap();
        assert!(!auth.aggregate_daily_unique_accounts(end + 3600).unwrap());
        drop(auth);
        let auth = AuthService::new(path).unwrap();
        auth.open_connection()
            .unwrap()
            .execute("DROP TABLE account_activity_sessions", [])
            .unwrap();
        assert!(!auth.aggregate_daily_unique_accounts(end + 7200).unwrap());
        let history = auth.unique_account_history(end + 7200, 1).unwrap();
        assert_eq!(
            history.samples,
            vec![UniqueSample {
                timestamp: end,
                accounts: 1
            }]
        );
    }

    #[test]
    fn daily_unique_backfill_respects_collection_start_and_historical_windows() {
        let midnight = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_backfill_windows", midnight + 3600);
        assert_eq!(
            auth.backfill_daily_unique_accounts(midnight + 7200)
                .unwrap(),
            0
        );
        assert!(auth
            .unique_account_history(midnight + 7200, 365)
            .unwrap()
            .samples
            .is_empty());

        let first = midnight + DAY_SECONDS;
        auth.record_account_activities(&[
            activity("a1", "Alice", first - 100, first - 100),
            activity(
                "a2",
                "alice",
                first + DAY_SECONDS + 100,
                first + DAY_SECONDS + 100,
            ),
            activity("b", "bob", first - 50, first + DAY_SECONDS + 50),
            activity(
                "future",
                "future",
                first + 4 * DAY_SECONDS,
                first + 4 * DAY_SECONDS,
            ),
        ])
        .unwrap();
        let now = first + 4 * DAY_SECONDS - 1;
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 4);
        for (offset, accounts) in [2, 1, 2, 0].into_iter().enumerate() {
            let timestamp = first + offset as i64 * DAY_SECONDS;
            assert_eq!(
                auth.unique_account_history(timestamp, 1)
                    .unwrap()
                    .samples
                    .last(),
                Some(&UniqueSample {
                    timestamp,
                    accounts
                })
            );
        }
        for days in [7, 30, 180, 365] {
            let history = auth.unique_account_history(now, days).unwrap();
            assert_eq!(
                history.samples,
                (0..4)
                    .map(|offset| UniqueSample {
                        timestamp: first + offset * DAY_SECONDS,
                        accounts: 2,
                    })
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(auth.backfill_daily_unique_accounts(now + 1).unwrap(), 1);
        assert_eq!(
            auth.unique_account_history(now + 1, 7)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            2
        );
    }

    #[test]
    fn daily_unique_backfill_on_restart_fills_gaps_without_recalculating_saved_days() {
        let first = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_backfill_restart", first - DAY_SECONDS + 3600);
        auth.record_account_activities(&[activity("a", "alice", first - 100, first - 1)])
            .unwrap();
        assert!(auth.aggregate_daily_unique_accounts(first).unwrap());
        assert!(auth
            .aggregate_daily_unique_accounts(first + 2 * DAY_SECONDS)
            .unwrap());
        auth.record_account_activities(&[activity("late", "bob", first - 50, first - 1)])
            .unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let now = first + 5 * DAY_SECONDS + 3600;
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 4);
        let history = auth.unique_account_history(now, 7).unwrap();
        assert_eq!(
            history.samples,
            [1, 2, 1, 2, 2, 2]
                .into_iter()
                .enumerate()
                .map(|(offset, accounts)| UniqueSample {
                    timestamp: first + offset as i64 * DAY_SECONDS,
                    accounts,
                })
                .collect::<Vec<_>>()
        );
        assert_eq!(history.last_aggregated_at, Some(first + 5 * DAY_SECONDS));
        assert!(auth
            .unique_account_history(now, 1)
            .unwrap()
            .samples
            .iter()
            .all(|sample| sample.accounts == 0));
        auth.open_connection()
            .unwrap()
            .execute("DROP TABLE account_activity_sessions", [])
            .unwrap();
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 0);
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples,
            history.samples
        );
    }

    #[test]
    fn daily_unique_backfill_resumes_after_a_failed_day() {
        let first = kst_day_start(1000 * DAY_SECONDS);
        let (auth, path) = unique_auth("unique_backfill_retry", first - DAY_SECONDS);
        auth.open_connection()
            .unwrap()
            .execute_batch(&format!(
                "CREATE TRIGGER fail_daily_sample BEFORE INSERT ON unique_account_daily_samples
                 WHEN NEW.timestamp = {}
                 BEGIN SELECT RAISE(ABORT, 'test failure'); END;",
                first + DAY_SECONDS
            ))
            .unwrap();
        let now = first + 2 * DAY_SECONDS;
        assert!(auth.backfill_daily_unique_accounts(now).is_err());
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples,
            vec![UniqueSample {
                timestamp: first,
                accounts: 0,
            }]
        );
        auth.open_connection()
            .unwrap()
            .execute("DROP TRIGGER fail_daily_sample", [])
            .unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 2);
        assert_eq!(
            auth.unique_account_history(now, 7).unwrap().samples.len(),
            3
        );
        assert_eq!(auth.backfill_daily_unique_accounts(now).unwrap(), 0);
    }

    #[test]
    fn daily_unique_history_distinguishes_pending_zero_and_missed_days() {
        let midnight = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_coverage", midnight + 3600);
        assert!(!auth
            .aggregate_daily_unique_accounts(midnight + 7200)
            .unwrap());
        let pending = auth.unique_account_history(midnight + 7200, 1).unwrap();
        assert!(pending.samples.is_empty());
        assert_eq!(pending.last_aggregated_at, None);
        let first = midnight + DAY_SECONDS;
        assert!(auth.aggregate_daily_unique_accounts(first + 30).unwrap());
        assert!(auth
            .aggregate_daily_unique_accounts(first + 3 * DAY_SECONDS)
            .unwrap());
        let history = auth
            .unique_account_history(first + 3 * DAY_SECONDS, 7)
            .unwrap();
        assert_eq!(
            history.samples,
            vec![
                UniqueSample {
                    timestamp: first,
                    accounts: 0
                },
                UniqueSample {
                    timestamp: first + 3 * DAY_SECONDS,
                    accounts: 0
                }
            ]
        );
        assert_eq!(history.last_aggregated_at, Some(first + 3 * DAY_SECONDS));
        assert_eq!(
            auth.unique_account_history(first + 6 * DAY_SECONDS, 1)
                .unwrap()
                .last_aggregated_at,
            history.last_aggregated_at
        );
    }

    #[test]
    fn daily_unique_collection_uses_korean_midnight_and_preserves_latest_activity() {
        assert_eq!(kst_day_start(15 * 3600 - 1), -9 * 3600);
        assert_eq!(kst_day_start(15 * 3600), 15 * 3600);
        assert_eq!(kst_day_start(DAY_SECONDS), 15 * 3600);
        let end = kst_day_start(1000 * DAY_SECONDS);
        let (auth, _) = unique_auth("unique_daily_heartbeat", end - 3 * DAY_SECONDS);
        auth.record_account_activities(&[activity("a", "alice", end - 2 * DAY_SECONDS, end - 1)])
            .unwrap();
        auth.record_account_activities(&[activity(
            "a",
            "alice",
            end - 2 * DAY_SECONDS,
            end - 2 * DAY_SECONDS,
        )])
        .unwrap();
        auth.aggregate_daily_unique_accounts(end).unwrap();
        assert_eq!(
            auth.unique_account_history(end, 1)
                .unwrap()
                .samples
                .last()
                .unwrap()
                .accounts,
            1
        );
    }

    #[test]
    fn concurrent_samples_survive_restart_and_preserve_missing_minutes() {
        let path = crate::test_util::unique_temp_dir("concurrent_samples").join("game.db");
        let auth = AuthService::new(path.clone()).unwrap();
        for (now, web_accounts, agent_accounts, other_accounts) in [
            (65, 1, 2, 0),
            (119, 2, 1, 1),
            (121, 0, 0, 0),
            (305, 3, 4, 0),
        ] {
            auth.record_concurrent_accounts(
                now,
                ConcurrentCounts {
                    web_accounts,
                    agent_accounts,
                    other_accounts,
                },
            )
            .unwrap();
        }
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let samples = auth.concurrent_account_samples(60, 300, 60).unwrap();
        assert_eq!(
            samples,
            vec![
                ConcurrentHistorySample {
                    timestamp: 60,
                    accounts: 4.0,
                    web_accounts: 2.0,
                    agent_accounts: 1.0,
                    other_accounts: 1.0,
                    peak_accounts: 4,
                    peak_timestamp: 60,
                    sample_count: 1,
                },
                ConcurrentHistorySample {
                    timestamp: 120,
                    accounts: 0.0,
                    web_accounts: 0.0,
                    agent_accounts: 0.0,
                    other_accounts: 0.0,
                    peak_accounts: 0,
                    peak_timestamp: 120,
                    sample_count: 1,
                },
                ConcurrentHistorySample {
                    timestamp: 300,
                    accounts: 7.0,
                    web_accounts: 3.0,
                    agent_accounts: 4.0,
                    other_accounts: 0.0,
                    peak_accounts: 7,
                    peak_timestamp: 300,
                    sample_count: 1,
                },
            ]
        );
        assert_eq!(
            auth.concurrent_account_samples(121, 299, 60).unwrap(),
            vec![]
        );
    }

    #[test]
    fn concurrent_history_aggregates_only_observed_minutes_and_keeps_peaks() {
        let path = crate::test_util::unique_temp_dir("concurrent_aggregates").join("game.db");
        let auth = AuthService::new(path).unwrap();
        for (timestamp, web_accounts, agent_accounts, other_accounts) in [
            (60, 99, 0, 0),
            (120, 0, 0, 0),
            (180, 2, 4, 0),
            (240, 4, 2, 0),
            (1200, 1, 1, 1),
            (1260, 99, 0, 0),
        ] {
            auth.record_concurrent_accounts(
                timestamp,
                ConcurrentCounts {
                    web_accounts,
                    agent_accounts,
                    other_accounts,
                },
            )
            .unwrap();
        }
        assert_eq!(
            auth.concurrent_account_samples(100, 1230, 600).unwrap(),
            vec![
                ConcurrentHistorySample {
                    timestamp: 100,
                    accounts: 4.0,
                    web_accounts: 2.0,
                    agent_accounts: 2.0,
                    other_accounts: 0.0,
                    peak_accounts: 6,
                    peak_timestamp: 180,
                    sample_count: 3,
                },
                ConcurrentHistorySample {
                    timestamp: 1200,
                    accounts: 3.0,
                    web_accounts: 1.0,
                    agent_accounts: 1.0,
                    other_accounts: 1.0,
                    peak_accounts: 3,
                    peak_timestamp: 1200,
                    sample_count: 1,
                },
            ]
        );
    }

    #[test]
    fn concurrent_history_migrates_old_totals_without_inventing_client_types() {
        let path = crate::test_util::unique_temp_dir("concurrent_migration").join("game.db");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE concurrent_account_samples (timestamp INTEGER PRIMARY KEY, accounts INTEGER NOT NULL);
             INSERT INTO concurrent_account_samples VALUES (60, 8), (120, 4)",
        ).unwrap();
        AuthService::ensure_concurrent_samples_schema(&conn).unwrap();
        AuthService::ensure_concurrent_samples_schema(&conn).unwrap();
        drop(conn);
        let auth = AuthService::new(path).unwrap();
        let old = auth.concurrent_account_samples(60, 120, 60).unwrap();
        assert!(old.iter().all(|sample| sample.web_accounts == 0.0
            && sample.agent_accounts == 0.0
            && sample.other_accounts == sample.accounts));
        auth.record_concurrent_accounts(
            180,
            ConcurrentCounts {
                web_accounts: 2,
                agent_accounts: 4,
                other_accounts: 0,
            },
        )
        .unwrap();
        let samples = auth.concurrent_account_samples(0, 599, 600).unwrap();
        assert_eq!(samples.len(), 1);
        let sample = &samples[0];
        assert_eq!(sample.accounts, 6.0);
        assert_eq!(sample.web_accounts, 2.0 / 3.0);
        assert_eq!(sample.agent_accounts, 4.0 / 3.0);
        assert_eq!(sample.other_accounts, 4.0);
        assert_eq!(sample.peak_accounts, 8);
        assert_eq!(sample.sample_count, 3);
    }

    #[test]
    fn concurrent_history_bounds_a_full_year_of_samples() {
        let path = crate::test_util::unique_temp_dir("concurrent_year").join("game.db");
        let auth = AuthService::new(path).unwrap();
        auth.open_connection()
            .unwrap()
            .execute_batch(
                "WITH RECURSIVE minutes(timestamp) AS (
                SELECT 0 UNION ALL SELECT timestamp + 60 FROM minutes WHERE timestamp < 31535940
             )
             INSERT INTO concurrent_account_samples (timestamp, accounts)
             SELECT timestamp, 4 FROM minutes",
            )
            .unwrap();
        let samples = auth.concurrent_account_samples(0, 31536000, 86400).unwrap();
        assert_eq!(samples.len(), 365);
        assert_eq!(
            samples
                .iter()
                .map(|sample| sample.sample_count)
                .sum::<u32>(),
            525600
        );
        assert!(samples.iter().all(|sample| sample.accounts == 4.0
            && sample.web_accounts == 0.0
            && sample.agent_accounts == 0.0
            && sample.other_accounts == 4.0
            && sample.peak_accounts == 4
            && sample.peak_timestamp == sample.timestamp));
    }
}
