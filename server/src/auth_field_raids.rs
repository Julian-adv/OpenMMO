use super::{AuthError, AuthService};
use crate::metrics::{
    outcome_from_key, outcome_key, FieldRaidEvent, FieldRaidRaider, FieldRaidRecord, FieldRaidTally,
};
use rusqlite::{params, Connection};
use std::collections::HashMap;

impl AuthService {
    pub(super) fn ensure_field_raid_events_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS field_raid_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );
             INSERT OR IGNORE INTO field_raid_collection (id, started_at) VALUES (1, unixepoch());
             CREATE TABLE IF NOT EXISTS field_raid_events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                boss TEXT NOT NULL,
                event TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS field_raids (
                id INTEGER PRIMARY KEY,
                boss TEXT NOT NULL,
                summoned_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL,
                outcome TEXT NOT NULL,
                summoner_id INTEGER,
                boss_health INTEGER NOT NULL,
                boss_max_health INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_field_raids_ended_at ON field_raids(ended_at);
             CREATE TABLE IF NOT EXISTS field_raid_participants (
                raid_id INTEGER NOT NULL REFERENCES field_raids(id) ON DELETE CASCADE,
                character_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                dealt INTEGER NOT NULL,
                taken INTEGER NOT NULL,
                deaths INTEGER NOT NULL,
                near_at_summon INTEGER NOT NULL,
                in_zone INTEGER NOT NULL,
                in_zone_at_end INTEGER NOT NULL,
                PRIMARY KEY (raid_id, character_id)
             );",
        )
    }

    pub fn record_field_raid_events(&self, events: &[FieldRaidEvent]) -> Result<(), AuthError> {
        if events.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        {
            let mut statement = transaction.prepare_cached(
                "INSERT INTO field_raid_events (timestamp, boss, event) VALUES (?1, ?2, ?3)",
            )?;
            for event in events {
                statement.execute(params![event.timestamp, event.boss, event.storage_key()])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// Each finished raid with its raiders and its end event, in one
    /// transaction.
    pub fn record_field_raids(&self, raids: &[FieldRaidRecord]) -> Result<(), AuthError> {
        if raids.is_empty() {
            return Ok(());
        }
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        {
            let mut raid_statement = transaction.prepare_cached(
                "INSERT INTO field_raids
                    (boss, summoned_at, ended_at, outcome, summoner_id, boss_health, boss_max_health)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            let mut event_statement = transaction.prepare_cached(
                "INSERT INTO field_raid_events (timestamp, boss, event) VALUES (?1, ?2, ?3)",
            )?;
            let mut raider_statement = transaction.prepare_cached(
                "INSERT INTO field_raid_participants
                    (raid_id, character_id, name, dealt, taken, deaths,
                     near_at_summon, in_zone, in_zone_at_end)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            )?;
            for raid in raids {
                event_statement.execute(params![
                    raid.ended_at,
                    raid.boss,
                    outcome_key(raid.outcome)
                ])?;
                raid_statement.execute(params![
                    raid.boss,
                    raid.summoned_at,
                    raid.ended_at,
                    outcome_key(raid.outcome),
                    raid.summoner_id,
                    raid.boss_health,
                    raid.boss_max_health,
                ])?;
                let raid_id = transaction.last_insert_rowid();
                for r in &raid.raiders {
                    raider_statement.execute(params![
                        raid_id,
                        r.character_id,
                        r.name,
                        r.dealt as i64,
                        r.taken as i64,
                        r.deaths,
                        r.near_at_summon,
                        r.in_zone,
                        r.in_zone_at_end,
                    ])?;
                }
            }
        }
        transaction.commit()?;
        Ok(())
    }

    /// The `limit` raids that ended last by `until`, newest first, each with
    /// its raiders by damage dealt.
    pub fn recent_field_raids(
        &self,
        until: i64,
        limit: usize,
    ) -> Result<Vec<FieldRaidRecord>, AuthError> {
        let conn = self.open_connection()?;
        let mut raids = conn.prepare(
            "SELECT id, boss, summoned_at, ended_at, outcome, summoner_id, boss_health, boss_max_health
             FROM field_raids WHERE ended_at <= ?1 ORDER BY ended_at DESC, id DESC LIMIT ?2",
        )?;
        let mut raiders = conn.prepare(
            "SELECT character_id, name, dealt, taken, deaths, near_at_summon, in_zone, in_zone_at_end
             FROM field_raid_participants WHERE raid_id = ?1
             ORDER BY dealt DESC, taken DESC, character_id",
        )?;
        let rows = raids
            .query_map(params![until, limit as i64], |row| {
                let key: String = row.get(4)?;
                let outcome = outcome_from_key(&key).ok_or_else(|| {
                    rusqlite::Error::FromSqlConversionFailure(
                        4,
                        rusqlite::types::Type::Text,
                        format!("unknown field raid outcome {key}").into(),
                    )
                })?;
                Ok((
                    row.get::<_, i64>(0)?,
                    FieldRaidRecord {
                        boss: row.get(1)?,
                        summoned_at: row.get(2)?,
                        ended_at: row.get(3)?,
                        outcome,
                        summoner_id: row.get(5)?,
                        boss_health: row.get(6)?,
                        boss_max_health: row.get(7)?,
                        raiders: Vec::new(),
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, mut raid)| {
                raid.raiders = raiders
                    .query_map([id], |row| {
                        Ok(FieldRaidRaider {
                            character_id: row.get(0)?,
                            name: row.get(1)?,
                            dealt: row.get::<_, i64>(2)? as u64,
                            taken: row.get::<_, i64>(3)? as u64,
                            deaths: row.get(4)?,
                            near_at_summon: row.get(5)?,
                            in_zone: row.get(6)?,
                            in_zone_at_end: row.get(7)?,
                        })
                    })?
                    .collect::<Result<_, _>>()?;
                Ok(raid)
            })
            .collect()
    }

    /// Collection start, and each boss's tally up to `until`.
    pub fn field_raid_tallies(
        &self,
        until: i64,
    ) -> Result<(i64, HashMap<String, FieldRaidTally>), AuthError> {
        let conn = self.open_connection()?;
        let started_at = conn.query_row(
            "SELECT started_at FROM field_raid_collection WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        let mut statement = conn.prepare(
            "SELECT boss,
                    SUM(event = 'summoned'), SUM(event = 'defeated'), SUM(event = 'moon_set'),
                    SUM(event = 'dawn'), SUM(event = 'wiped'),
                    MAX(CASE WHEN event = 'defeated' THEN timestamp END)
             FROM field_raid_events WHERE timestamp <= ?1 GROUP BY boss",
        )?;
        let tallies = statement
            .query_map([until], |row| {
                Ok((
                    row.get(0)?,
                    FieldRaidTally {
                        summons: row.get(1)?,
                        defeated: row.get(2)?,
                        moon_set: row.get(3)?,
                        dawn: row.get(4)?,
                        wiped: row.get(5)?,
                        last_defeated_at: row.get(6)?,
                    },
                ))
            })?
            .collect::<Result<_, _>>()?;
        Ok((started_at, tallies))
    }
}
