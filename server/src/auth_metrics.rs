use super::{AuthError, AuthService};
use crate::metrics::{ConcurrentSample, SAMPLE_INTERVAL_SECONDS};
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
        )
    }

    pub fn record_concurrent_accounts(&self, now: i64, accounts: u32) -> Result<(), AuthError> {
        let timestamp = now - now.rem_euclid(SAMPLE_INTERVAL_SECONDS);
        self.open_connection()?.execute(
            "INSERT INTO concurrent_account_samples (timestamp, accounts) VALUES (?1, ?2)
             ON CONFLICT(timestamp) DO UPDATE SET accounts = excluded.accounts",
            params![timestamp, accounts],
        )?;
        Ok(())
    }

    pub fn concurrent_account_samples(
        &self,
        from: i64,
        until: i64,
    ) -> Result<Vec<ConcurrentSample>, AuthError> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT timestamp, accounts FROM concurrent_account_samples
             WHERE timestamp >= ?1 AND timestamp <= ?2 ORDER BY timestamp",
        )?;
        let rows = statement.query_map(params![from, until], |row| {
            Ok(ConcurrentSample {
                timestamp: row.get(0)?,
                accounts: row.get(1)?,
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
        auth.record_concurrent_accounts(65, 3).unwrap();
        auth.record_concurrent_accounts(119, 4).unwrap();
        auth.record_concurrent_accounts(121, 0).unwrap();
        auth.record_concurrent_accounts(305, 7).unwrap();
        drop(auth);

        let auth = AuthService::new(path).unwrap();
        let samples = auth.concurrent_account_samples(60, 300).unwrap();
        assert_eq!(
            samples,
            vec![
                ConcurrentSample {
                    timestamp: 60,
                    accounts: 4
                },
                ConcurrentSample {
                    timestamp: 120,
                    accounts: 0
                },
                ConcurrentSample {
                    timestamp: 300,
                    accounts: 7
                },
            ]
        );
        assert_eq!(auth.concurrent_account_samples(121, 299).unwrap(), vec![]);
    }
}
