//! Estate defense persistence (doc/ESTATE_DEFENSE.md): who may blow a war
//! horn where, and the disgrace a lost defense leaves.

use super::{AuthError, AuthService};
use onlinerpg_shared::family::{FamilyDefenseEntry, FAMILY_DEFENSE_LOG, FAMILY_RANK_HEAD};
use onlinerpg_shared::messages::{EstateDefenseOutcome, EstateTrophyRecord};
use onlinerpg_shared::world::GameDateTime;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

type Verdict<T> = Result<Result<T, &'static str>, AuthError>;

/// The estate a horn is blown on and who defends it.
#[derive(Debug, Clone)]
pub struct DefenseSite {
    pub estate_id: i64,
    pub family_id: Option<i64>,
    /// Characters of the estate's family, the horn-blower included.
    pub members: Vec<i64>,
}

/// Where a trophy's defense was held, kept beside its record for audits.
pub struct TrophySite {
    pub estate_id: i64,
    pub summoner_id: i64,
    pub horn: String,
}

/// One fought defense, as `estate_defense_runs` keeps it.
pub struct DefenseRun {
    pub estate_id: i64,
    pub family_id: Option<i64>,
    pub summoner_id: i64,
    pub summoner_name: String,
    pub horn: String,
    pub outcome: EstateDefenseOutcome,
    pub started_at: i64,
    pub ended_at: i64,
    /// The game day it ended.
    pub on: GameDateTime,
    /// An admin's `/warhorn`, listed under the admin's family.
    pub practice: bool,
}

fn outcome_code(outcome: EstateDefenseOutcome) -> &'static str {
    match outcome {
        EstateDefenseOutcome::Held => "held",
        EstateDefenseOutcome::BannerFell => "banner_fell",
        EstateDefenseOutcome::Wiped => "wiped",
        EstateDefenseOutcome::TimedOut => "timed_out",
    }
}

fn outcome_from_code(code: &str) -> Option<EstateDefenseOutcome> {
    use serde::de::{value::Error, IntoDeserializer};
    use serde::Deserialize;
    EstateDefenseOutcome::deserialize(IntoDeserializer::<Error>::into_deserializer(code)).ok()
}

/// The family's latest defenses for its window.
pub(super) fn family_defenses(
    conn: &Connection,
    family_id: i64,
) -> Result<Vec<FamilyDefenseEntry>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT horn, summoner_name, outcome, year, month, day FROM estate_defense_runs
         WHERE family_id=?1 ORDER BY id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![family_id, FAMILY_DEFENSE_LOG], |row| {
        let outcome: String = row.get(2)?;
        let Some(outcome) = outcome_from_code(&outcome) else {
            return Ok(None);
        };
        Ok(Some(FamilyDefenseEntry {
            horn: row.get(0)?,
            summoner: row.get(1)?,
            outcome,
            on: GameDateTime {
                year: row.get(3)?,
                month: row.get(4)?,
                day: row.get(5)?,
                hour: 0,
                minute: 0,
            },
        }))
    })?;
    rows.filter_map(Result::transpose).collect()
}

/// Active disgraces: until when, by family and by character.
#[derive(Debug, Default, Clone)]
pub struct Disgraces {
    pub families: HashMap<i64, i64>,
    pub characters: HashMap<i64, i64>,
}

impl AuthService {
    pub(super) fn ensure_estate_defense_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        for table in ["characters", "families"] {
            if !Self::table_columns(conn, table)?.contains("disgrace_until") {
                conn.execute(
                    &format!("ALTER TABLE {table} ADD COLUMN disgrace_until INTEGER"),
                    [],
                )?;
            }
        }
        conn.execute(
            "CREATE TABLE IF NOT EXISTS estate_defense_trophies (
                item_id INTEGER PRIMARY KEY,
                estate_id INTEGER NOT NULL,
                summoner_id INTEGER NOT NULL,
                summoner_name TEXT NOT NULL,
                horn TEXT NOT NULL,
                year INTEGER NOT NULL,
                month INTEGER NOT NULL,
                day INTEGER NOT NULL,
                defenders TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS estate_defense_runs (
                id INTEGER PRIMARY KEY,
                estate_id INTEGER NOT NULL,
                family_id INTEGER,
                summoner_id INTEGER NOT NULL,
                summoner_name TEXT NOT NULL,
                horn TEXT NOT NULL,
                outcome TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL,
                year INTEGER NOT NULL,
                month INTEGER NOT NULL,
                day INTEGER NOT NULL,
                practice INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_estate_defense_runs_family
                ON estate_defense_runs(family_id, id);",
        )?;
        Ok(())
    }

    /// Keep the defense behind a trophy; names are as they were that night.
    pub fn record_estate_trophy(
        &self,
        item_id: u64,
        site: &TrophySite,
        record: &EstateTrophyRecord,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let day = &record.defended_on;
        conn.execute(
            "INSERT INTO estate_defense_trophies
             (item_id, estate_id, summoner_id, summoner_name, horn, year, month, day,
              defenders, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                item_id as i64,
                site.estate_id,
                site.summoner_id,
                record.summoner,
                site.horn,
                day.year,
                day.month,
                day.day,
                serde_json::to_string(&record.defenders).unwrap_or_default(),
                super::unix_now()
            ],
        )?;
        Ok(())
    }

    pub fn estate_trophy(&self, item_id: u64) -> Result<Option<EstateTrophyRecord>, AuthError> {
        let conn = self.open_connection()?;
        let record = conn
            .query_row(
                "SELECT summoner_name, year, month, day, defenders
                 FROM estate_defense_trophies WHERE item_id=?1",
                [item_id as i64],
                |row| {
                    let defenders: String = row.get(4)?;
                    Ok(EstateTrophyRecord {
                        summoner: row.get(0)?,
                        defended_on: GameDateTime {
                            year: row.get(1)?,
                            month: row.get(2)?,
                            day: row.get(3)?,
                            hour: 0,
                            minute: 0,
                        },
                        defenders: serde_json::from_str(&defenders).unwrap_or_default(),
                    })
                },
            )
            .optional()?;
        Ok(record)
    }

    /// Both the summoner and the banner must be on the defended estate.
    pub fn estate_defense_site(
        &self,
        character_id: i64,
        origin: (i32, i32, u8),
        banner: (i32, i32, u8),
    ) -> Verdict<DefenseSite> {
        let conn = self.open_connection()?;
        let estate: Option<(i64, String, Option<i64>, i64)> = conn
            .query_row(
                "SELECT e.id, e.account_name, e.family_id, e.missed FROM land_plots p
                 JOIN land_estates e ON e.id=p.estate_id
                 JOIN land_plots b ON b.estate_id=e.id
                 WHERE p.tile_x=?1 AND p.tile_z=?2 AND p.quadrant=?3
                   AND b.tile_x=?4 AND b.tile_z=?5 AND b.quadrant=?6",
                params![origin.0, origin.1, origin.2, banner.0, banner.1, banner.2],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((estate_id, account, family_id, missed)) = estate else {
            return Ok(Err("notOwnEstate"));
        };
        if missed > 0 {
            return Ok(Err("arrears"));
        }
        let Some(family_id) = family_id else {
            let own: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM characters WHERE id=?1 AND account_name=?2)",
                params![character_id, account],
                |row| row.get(0),
            )?;
            return Ok(if own {
                Ok(DefenseSite {
                    estate_id,
                    family_id: None,
                    members: vec![character_id],
                })
            } else {
                Err("notOwnEstate")
            });
        };
        let head = super::family::membership(&conn, character_id)?
            .is_some_and(|m| m.family_id == family_id && m.rank == FAMILY_RANK_HEAD);
        if !head {
            return Ok(Err("notHead"));
        }
        let mut stmt =
            conn.prepare("SELECT character_id FROM family_members WHERE family_id=?1")?;
        let members = stmt
            .query_map([family_id], |row| row.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        Ok(Ok(DefenseSite {
            estate_id,
            family_id: Some(family_id),
            members,
        }))
    }

    /// Disgrace a family, or a lone horn-blower, until `until`; `None` lifts it.
    pub fn set_disgrace(
        &self,
        family_id: Option<i64>,
        character_id: i64,
        until: Option<i64>,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        match family_id {
            Some(id) => conn.execute(
                "UPDATE families SET disgrace_until=?2 WHERE id=?1",
                params![id, until],
            )?,
            None => conn.execute(
                "UPDATE characters SET disgrace_until=?2 WHERE id=?1",
                params![character_id, until],
            )?,
        };
        Ok(())
    }

    pub fn record_estate_defense_run(&self, run: &DefenseRun) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO estate_defense_runs
             (estate_id, family_id, summoner_id, summoner_name, horn, outcome, started_at, ended_at,
              year, month, day, practice)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                run.estate_id,
                run.family_id,
                run.summoner_id,
                run.summoner_name,
                run.horn,
                outcome_code(run.outcome),
                run.started_at,
                run.ended_at,
                run.on.year,
                run.on.month,
                run.on.day,
                run.practice
            ],
        )?;
        Ok(())
    }

    pub fn active_disgraces(&self, now: i64) -> Result<Disgraces, AuthError> {
        let conn = self.open_connection()?;
        let read = |sql: &str| -> Result<HashMap<i64, i64>, AuthError> {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([now], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<Result<_, _>>()?;
            Ok(rows)
        };
        Ok(Disgraces {
            families: read("SELECT id, disgrace_until FROM families WHERE disgrace_until > ?1")?,
            characters: read(
                "SELECT id, disgrace_until FROM characters WHERE disgrace_until > ?1",
            )?,
        })
    }
}
