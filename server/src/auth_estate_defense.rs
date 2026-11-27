//! Estate defense persistence (doc/ESTATE_DEFENSE.md): who may blow a war
//! horn where, and the disgrace a lost defense leaves.

use super::{AuthError, AuthService};
use onlinerpg_shared::family::FAMILY_RANK_HEAD;
use onlinerpg_shared::messages::EstateTrophyRecord;
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

    /// The estate under `plot` if `character_id` may blow a horn on it: its
    /// own account's unlinked estate, or its family's estate when it is the
    /// head. Arrears refuse both.
    pub fn estate_defense_site(
        &self,
        character_id: i64,
        plot: (i32, i32, u8),
    ) -> Verdict<DefenseSite> {
        let conn = self.open_connection()?;
        let estate: Option<(i64, String, Option<i64>, i64)> = conn
            .query_row(
                "SELECT e.id, e.account_name, e.family_id, e.missed FROM land_plots p
                 JOIN land_estates e ON e.id=p.estate_id
                 WHERE p.tile_x=?1 AND p.tile_z=?2 AND p.quadrant=?3",
                params![plot.0, plot.1, plot.2],
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

    /// Disgrace a family, or a lone horn-blower, until `until`.
    pub fn set_disgrace(
        &self,
        family_id: Option<i64>,
        character_id: i64,
        until: i64,
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
