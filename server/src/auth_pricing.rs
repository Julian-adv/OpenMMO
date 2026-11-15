use super::{unix_now, AuthError, AuthService};
use rusqlite::{params, Connection, OptionalExtension};

/// doc/PRICING.md; `target_m` is the target path at game day `target_day`.
#[derive(Debug, Clone, PartialEq)]
pub struct PricingState {
    pub index_percent: u32,
    pub last_meeting_day: Option<i64>,
    pub target_m: Option<f64>,
    pub target_day: Option<i64>,
}

impl Default for PricingState {
    fn default() -> Self {
        Self {
            index_percent: 100,
            last_meeting_day: None,
            target_m: None,
            target_day: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PricingMeeting {
    pub game_day: i64,
    /// The comparison: the target path, or before it the last meeting.
    #[serde(rename = "m_prev")]
    pub reference: f64,
    pub m_now: f64,
    #[serde(rename = "growth")]
    pub gap: f64,
    pub index_before: u32,
    pub index_after: u32,
}

impl AuthService {
    pub(super) fn ensure_pricing_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pricing_state (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                index_percent INTEGER NOT NULL,
                last_meeting_day INTEGER,
                target_m REAL,
                target_day INTEGER
            );
            CREATE TABLE IF NOT EXISTS pricing_history (
                ts INTEGER NOT NULL,
                game_day INTEGER NOT NULL,
                m_prev REAL NOT NULL,
                m_now REAL NOT NULL,
                growth REAL NOT NULL,
                index_before INTEGER NOT NULL,
                index_after INTEGER NOT NULL
            );",
        )?;
        if !Self::table_columns(conn, "pricing_state")?.contains("target_m") {
            conn.execute_batch(
                "ALTER TABLE pricing_state ADD COLUMN target_m REAL;
                 ALTER TABLE pricing_state ADD COLUMN target_day INTEGER;",
            )?;
        }
        Ok(())
    }

    pub fn load_pricing_state(&self) -> Result<PricingState, AuthError> {
        let conn = self.open_connection()?;
        Ok(conn
            .query_row(
                "SELECT index_percent, last_meeting_day, target_m, target_day
                 FROM pricing_state WHERE id = 1",
                [],
                |row| {
                    Ok(PricingState {
                        index_percent: row.get(0)?,
                        last_meeting_day: row.get(1)?,
                        target_m: row.get(2)?,
                        target_day: row.get(3)?,
                    })
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Persists the state and, for a meeting that moved the index, its
    /// history row, atomically.
    pub fn save_pricing_state(
        &self,
        state: &PricingState,
        meeting: Option<&PricingMeeting>,
    ) -> Result<(), AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO pricing_state \
             (id, index_percent, last_meeting_day, target_m, target_day) VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                state.index_percent,
                state.last_meeting_day,
                state.target_m,
                state.target_day
            ],
        )?;
        if let Some(m) = meeting {
            tx.execute(
                "INSERT INTO pricing_history \
                 (ts, game_day, m_prev, m_now, growth, index_before, index_after) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    unix_now(),
                    m.game_day,
                    m.reference,
                    m.m_now,
                    m.gap,
                    m.index_before,
                    m.index_after
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Notice inputs: last meeting's index change (points) and `M` over the
    /// past day.
    pub fn pricing_notice_inputs(&self, now: i64) -> Result<(i32, Option<f64>), AuthError> {
        let conn = self.open_connection()?;
        let change = conn
            .query_row(
                "SELECT index_after - index_before FROM pricing_history ORDER BY ts DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(0) as i32;
        let day = crate::metrics::DAY_SECONDS;
        Ok((change, self.average_gold_per_account(now - day, now)?))
    }

    /// Mean hourly 30-day wallets per active account in `(from, until]`, or
    /// None without a sample.
    pub fn average_gold_per_account(
        &self,
        from: i64,
        until: i64,
    ) -> Result<Option<f64>, AuthError> {
        Ok(self.open_connection()?.query_row(
            "SELECT AVG(month_gold * 1.0 / month_accounts) FROM account_gold_samples
             WHERE timestamp > ?1 AND timestamp <= ?2 AND month_accounts > 0",
            params![from, until],
            |row| row.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_auth;

    #[test]
    fn pricing_averages_month_wallets_per_active_account() {
        let (auth, _) = temp_auth("auth_gold_per_account");
        assert_eq!(auth.average_gold_per_account(0, 10_800).unwrap(), None);
        let conn = auth.open_connection().unwrap();
        conn.execute_batch(
            "INSERT INTO account_gold_samples VALUES (3600, 1, 1, 1, 1, 300, 3);
             INSERT INTO account_gold_samples VALUES (7200, 1, 1, 1, 1, 500, 4);
             INSERT INTO account_gold_samples VALUES (10800, 0, 0, 0, 0, 0, 0);",
        )
        .unwrap();
        assert_eq!(
            auth.average_gold_per_account(0, 10_800).unwrap(),
            Some(112.5)
        );
        assert_eq!(
            auth.average_gold_per_account(3600, 7200).unwrap(),
            Some(125.0)
        );
        assert_eq!(auth.pricing_notice_inputs(7200).unwrap(), (0, Some(112.5)));
    }

    #[test]
    fn pricing_state_round_trips_with_its_meeting_row() {
        let (auth, _) = temp_auth("auth_price");
        assert_eq!(auth.load_pricing_state().unwrap(), PricingState::default());

        let state = PricingState {
            index_percent: 104,
            last_meeting_day: Some(34),
            target_m: Some(98.5),
            target_day: Some(-86),
        };
        let meeting = PricingMeeting {
            game_day: 34,
            reference: 100.0,
            m_now: 120.5,
            gap: 0.205,
            index_before: 100,
            index_after: 104,
        };
        auth.save_pricing_state(&state, Some(&meeting)).unwrap();
        assert_eq!(auth.load_pricing_state().unwrap(), state);
        let rows: i64 = auth
            .open_connection()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pricing_history", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }
}
