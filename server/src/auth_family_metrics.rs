use super::{AuthError, AuthService};
use crate::metrics::{
    kst_day_start, FamilyDayCounts, FamilyEventCounts, FamilyMetrics, TopFamily, DAY_SECONDS,
};
use onlinerpg_shared::family::{FAMILY_RANK_ELDER, FAMILY_RANK_HEAD};
use rusqlite::{params, Connection};

impl AuthService {
    /// No foreign key: the events outlive the family. The first run seeds
    /// what the tables still show, living families and merged-away names.
    pub(super) fn ensure_family_events_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS family_events (
                id INTEGER PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                family_id INTEGER,
                name TEXT NOT NULL,
                event TEXT NOT NULL,
                into_family_id INTEGER
             );
             CREATE TABLE IF NOT EXISTS family_event_collection (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                started_at INTEGER NOT NULL
             );",
        )?;
        let seeded: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM family_event_collection)",
            [],
            |row| row.get(0),
        )?;
        if !seeded {
            conn.execute_batch(
                "SAVEPOINT family_event_seed;
                 INSERT INTO family_events (timestamp, family_id, name, event)
                    SELECT created_at, id, name, 'founded' FROM families;
                 INSERT INTO family_events (timestamp, name, event, into_family_id)
                    SELECT retired_at, name, 'merged', successor_id FROM family_retired_names
                    WHERE reusable_at IS NULL;
                 INSERT INTO family_event_collection (id, started_at) VALUES (1, unixepoch());
                 RELEASE family_event_seed;",
            )?;
        }
        Ok(())
    }

    /// `daily` covers the last `days` KST days, oldest first.
    pub fn family_metrics(
        &self,
        until: i64,
        days: i64,
        top: usize,
    ) -> Result<FamilyMetrics, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        let collection_started_at = tx.query_row(
            "SELECT started_at FROM family_event_collection",
            [],
            |row| row.get(0),
        )?;
        let (active, members) = tx.query_row(
            "SELECT (SELECT COUNT(*) FROM families), (SELECT COUNT(*) FROM family_members)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let first_day = kst_day_start(until) - (days - 1) * DAY_SECONDS;
        let mut daily: Vec<FamilyDayCounts> = (0..days)
            .map(|i| FamilyDayCounts {
                day_start: first_day + i * DAY_SECONDS,
                counts: FamilyEventCounts::default(),
            })
            .collect();
        let mut total = FamilyEventCounts::default();
        {
            let mut stmt = tx.prepare(
                "SELECT CASE WHEN timestamp < ?1 THEN -1 ELSE (timestamp - ?1) / ?2 END,
                        SUM(event='founded'), SUM(event='merged'), SUM(event='dissolved')
                 FROM family_events WHERE timestamp <= ?3 GROUP BY 1",
            )?;
            let rows = stmt.query_map(params![first_day, DAY_SECONDS, until], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    FamilyEventCounts {
                        founded: row.get(1)?,
                        merged: row.get(2)?,
                        dissolved: row.get(3)?,
                    },
                ))
            })?;
            for row in rows {
                let (day, counts) = row?;
                total.founded += counts.founded;
                total.merged += counts.merged;
                total.dissolved += counts.dissolved;
                if let Some(slot) = usize::try_from(day).ok().and_then(|day| daily.get_mut(day)) {
                    slot.counts = counts;
                }
            }
        }
        let mut families = tx.prepare(
            "SELECT f.id, f.name, f.created_at,
                    (SELECT COUNT(*) FROM family_members m WHERE m.family_id=f.id) AS members,
                    (SELECT COUNT(*) FROM land_estates e WHERE e.family_id=f.id)
             FROM families f ORDER BY members DESC, f.id LIMIT ?1",
        )?;
        let mut leaders = tx.prepare(
            "SELECT c.character_name, m.rank FROM family_members m
             JOIN characters c ON c.id=m.character_id
             WHERE m.family_id=?1 AND m.rank IN (?2, ?3)
             ORDER BY m.rank, m.joined_at, m.character_id",
        )?;
        let mut ranked = families
            .query_map([top as i64], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    TopFamily {
                        name: row.get(1)?,
                        created_at: row.get(2)?,
                        members: row.get(3)?,
                        estates: row.get(4)?,
                        head: None,
                        elders: Vec::new(),
                    },
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for (id, family) in &mut ranked {
            let named = leaders
                .query_map(params![*id, FAMILY_RANK_HEAD, FAMILY_RANK_ELDER], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, u8>(1)?))
                })?;
            for row in named {
                let (name, rank) = row?;
                if rank == FAMILY_RANK_HEAD {
                    family.head = Some(name);
                } else {
                    family.elders.push(name);
                }
            }
        }
        Ok(FamilyMetrics {
            until,
            collection_started_at,
            active,
            members,
            total,
            daily,
            top: ranked.into_iter().map(|(_, family)| family).collect(),
        })
    }
}
