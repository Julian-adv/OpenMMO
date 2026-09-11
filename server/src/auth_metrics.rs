use super::{AuthError, AuthService};
use crate::metrics::{ConcurrentCounts, ConcurrentHistorySample, SAMPLE_INTERVAL_SECONDS};
use rusqlite::{params, Connection};

impl AuthService {
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
