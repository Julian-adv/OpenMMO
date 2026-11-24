use super::{AuthError, AuthService};
use onlinerpg_shared::seasonal::{MonthDay, Season, SeasonSchedule};
use rusqlite::{params, Connection};

fn read_schedule(row: &rusqlite::Row) -> rusqlite::Result<(String, Option<[u8; 4]>)> {
    let dates = match (row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?) {
        (Some(sm), Some(sd), Some(em), Some(ed)) => Some([sm, sd, em, ed]),
        _ => None,
    };
    Ok((row.get(0)?, dates))
}

impl AuthService {
    /// A row is an operator's schedule for one event: its dates, or NULL
    /// dates for an event switched off. No row means the table dates.
    pub(super) fn ensure_seasonal_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS seasonal_event_schedules (
                event TEXT PRIMARY KEY,
                start_month INTEGER,
                start_day INTEGER,
                end_month INTEGER,
                end_day INTEGER,
                updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            );",
        )
    }

    /// Stored schedules. Dates that are not calendar days are left out.
    pub fn seasonal_schedules(&self) -> Result<Vec<(String, SeasonSchedule)>, AuthError> {
        let conn = self.open_connection()?;
        let mut query = conn.prepare(
            "SELECT event, start_month, start_day, end_month, end_day
             FROM seasonal_event_schedules ORDER BY event",
        )?;
        let rows = query
            .query_map([], read_schedule)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .filter_map(|(event, dates)| {
                let schedule = match dates {
                    None => SeasonSchedule::Off,
                    Some([sm, sd, em, ed]) => SeasonSchedule::Period(Season {
                        start: MonthDay::new(sm, sd)?,
                        end: MonthDay::new(em, ed)?,
                    }),
                };
                Some((event, schedule))
            })
            .collect())
    }

    pub fn save_seasonal_schedule(
        &self,
        event: &str,
        schedule: SeasonSchedule,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let dates = match schedule {
            SeasonSchedule::Default => {
                conn.execute(
                    "DELETE FROM seasonal_event_schedules WHERE event=?1",
                    [event],
                )?;
                return Ok(());
            }
            SeasonSchedule::Off => None,
            SeasonSchedule::Period(season) => Some(season),
        };
        conn.execute(
            "INSERT INTO seasonal_event_schedules
                (event, start_month, start_day, end_month, end_day, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, strftime('%s', 'now'))
             ON CONFLICT(event) DO UPDATE SET
                start_month=excluded.start_month, start_day=excluded.start_day,
                end_month=excluded.end_month, end_day=excluded.end_day,
                updated_at=excluded.updated_at",
            params![
                event,
                dates.map(|s| s.start.month),
                dates.map(|s| s.start.day),
                dates.map(|s| s.end.month),
                dates.map(|s| s.end.day),
            ],
        )?;
        Ok(())
    }
}
