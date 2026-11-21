//! Day recruits of the guard: one row per account that ever took a watch.

use super::{AuthError, AuthService};
use rusqlite::{params, Connection, OptionalExtension};

/// A recruit on the captain's roll. Times are game minutes since the epoch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WatchShiftRow {
    pub character_id: i64,
    pub name: String,
    /// When the watch the recruit was hired for marches.
    pub departs_min: f64,
    /// The sergeant whose watch the recruit was hired for.
    pub leader: String,
    /// Roll calls of the watch since it marched, and those answered.
    pub calls: u32,
    pub answered: u32,
    /// The watch came home.
    pub done: bool,
    /// Which gear is drawn from the rack and not yet back on it. Bag instance
    /// ids change with every login, so the lent pieces are known by kind only.
    pub spear: bool,
    pub shovel: bool,
    pub tabard: bool,
}

#[derive(Debug, PartialEq)]
pub enum WatchHire {
    Hired,
    /// Paid for a watch less than the rest period ago.
    Resting,
    /// Another character of the account is on the roll.
    Taken(String),
}

/// The account's row as a hire sees it; `other` is another character on the roll.
struct HeldShift {
    other: Option<String>,
    departs_min: Option<f64>,
    done: bool,
    paid_min: Option<f64>,
}

impl AuthService {
    pub(super) fn ensure_watch_shift_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        // Shapes from before the first release.
        let columns = Self::table_columns(conn, "guard_watch_roll")?;
        if !columns.is_empty() && (!columns.contains("tabard") || columns.contains("returned")) {
            conn.execute_batch("DROP TABLE guard_watch_roll;")?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS guard_watch_roll (
                account_name TEXT PRIMARY KEY
                    REFERENCES accounts(player_name) ON DELETE CASCADE,
                character_id INTEGER,
                departs_min REAL,
                leader TEXT,
                calls INTEGER NOT NULL DEFAULT 0,
                answered INTEGER NOT NULL DEFAULT 0,
                done INTEGER NOT NULL DEFAULT 0,
                spear INTEGER NOT NULL DEFAULT 0,
                shovel INTEGER NOT NULL DEFAULT 0,
                tabard INTEGER NOT NULL DEFAULT 0,
                paid_min REAL
            );",
        )
    }

    pub fn watch_shifts(&self) -> Result<Vec<WatchShiftRow>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT s.character_id, c.character_name, s.departs_min, s.leader, s.calls, s.answered,
                    s.done, s.spear, s.shovel, s.tabard
             FROM guard_watch_roll s JOIN characters c ON c.id=s.character_id
             WHERE s.leader IS NOT NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WatchShiftRow {
                character_id: row.get(0)?,
                name: row.get(1)?,
                departs_min: row.get(2)?,
                leader: row.get(3)?,
                calls: row.get(4)?,
                answered: row.get(5)?,
                done: row.get(6)?,
                spear: row.get(7)?,
                shovel: row.get(8)?,
                tabard: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Put the character on the roll for `leader`'s watch unless its account is
    /// resting or has another character there; with no `leader`, only say
    /// whether it could be. That character's shift gives way once it was
    /// due to march before `stale_min` with the watch not yet home, or before `held_min`.
    pub fn hire_watch(
        &self,
        character_id: i64,
        watch: Option<(String, f64)>,
        now_min: f64,
        rest_min: f64,
        stale_min: f64,
        held_min: f64,
    ) -> Result<WatchHire, AuthError> {
        let conn = self.open_connection()?;
        let account: String = conn.query_row(
            "SELECT account_name FROM characters WHERE id=?1",
            [character_id],
            |row| row.get(0),
        )?;
        let held: Option<HeldShift> = conn
            .query_row(
                "SELECT c.character_name, s.departs_min, s.done, s.paid_min
                 FROM guard_watch_roll s
                 LEFT JOIN characters c ON c.id=s.character_id AND c.id<>?2
                 WHERE s.account_name=?1",
                params![account, character_id],
                |row| {
                    Ok(HeldShift {
                        other: row.get(0)?,
                        departs_min: row.get(1)?,
                        done: row.get(2)?,
                        paid_min: row.get(3)?,
                    })
                },
            )
            .optional()?;
        if let Some(held) = held {
            if let (Some(name), Some(departs_min)) = (held.other, held.departs_min) {
                let gives_way_before = if held.done { held_min } else { stale_min };
                if departs_min >= gives_way_before {
                    return Ok(WatchHire::Taken(name));
                }
            }
            let rested = |paid: f64| (0.0..rest_min).contains(&(now_min - paid));
            if held.paid_min.is_some_and(rested) {
                return Ok(WatchHire::Resting);
            }
        }
        let Some((leader, departs_min)) = watch else {
            return Ok(WatchHire::Hired);
        };
        conn.execute(
            "INSERT INTO guard_watch_roll (account_name, character_id, departs_min, leader)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(account_name) DO UPDATE SET character_id=excluded.character_id,
                 departs_min=excluded.departs_min, leader=excluded.leader, calls=0, answered=0,
                 done=0, spear=0, shovel=0, tabard=0",
            params![account, character_id, departs_min, leader],
        )?;
        Ok(WatchHire::Hired)
    }

    pub fn save_watch_shift(&self, shift: &WatchShiftRow) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE guard_watch_roll
             SET calls=?2, answered=?3, done=?4, spear=?5, shovel=?6, tabard=?7
             WHERE character_id=?1",
            params![
                shift.character_id,
                shift.calls,
                shift.answered,
                shift.done,
                shift.spear,
                shift.shovel,
                shift.tabard
            ],
        )?;
        Ok(())
    }

    /// Take the character off the roll; `paid_min` starts the account's rest.
    pub fn close_watch(&self, character_id: i64, paid_min: Option<f64>) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE guard_watch_roll
             SET character_id=NULL, departs_min=NULL, leader=NULL, spear=0, shovel=0,
                 tabard=0,
                 paid_min=COALESCE(?2, paid_min)
             WHERE character_id=?1",
            params![character_id, paid_min],
        )?;
        Ok(())
    }
}
