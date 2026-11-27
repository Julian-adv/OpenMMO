//! Aid cots and who may wake on them (doc/RESPAWN_POINT.md).

use super::{AuthError, AuthService};
use onlinerpg_shared::respawn::{ShelterAccess, ShelterMarker, AID_COT};
use onlinerpg_shared::Position;
use rusqlite::{params, Connection, OptionalExtension};

type Verdict<T> = Result<Result<T, &'static str>, AuthError>;

/// An estate's plots and its aid cots (id, position, rotation in degrees,
/// floor), first placed first.
#[derive(Debug, Default)]
pub struct ShelterSite {
    pub estate_id: i64,
    pub plots: Vec<(i32, i32, u8)>,
    pub cots: Vec<(i64, Position, f32, i8)>,
}

/// The shelters a character may always choose, wherever they fell.
#[derive(Debug, Default)]
pub struct OwnShelters {
    pub estate: Option<ShelterSite>,
    pub family_seat: Option<ShelterSite>,
}

fn shelter_site(conn: &Connection, estate_id: i64) -> Result<Option<ShelterSite>, rusqlite::Error> {
    let mut stmt = conn.prepare_cached(
        "SELECT id, x, y, z, rotation_deg, floor_level FROM estate_chests
         WHERE estate_id=?1 AND item_def_id=?2 ORDER BY id",
    )?;
    let cots = stmt
        .query_map(params![estate_id, AID_COT], |row| {
            Ok((
                row.get(0)?,
                Position {
                    x: row.get::<_, f64>(1)? as f32,
                    y: row.get::<_, f64>(2)? as f32,
                    z: row.get::<_, f64>(3)? as f32,
                },
                row.get::<_, f64>(4)? as f32,
                row.get::<_, i64>(5)? as i8,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if cots.is_empty() {
        return Ok(None);
    }
    Ok(Some(ShelterSite {
        estate_id,
        plots: super::family::estate_plots(conn, estate_id)?,
        cots,
    }))
}

impl AuthService {
    pub(super) fn ensure_shelter_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        if !Self::table_columns(conn, "land_estates")?.contains("shelter_access") {
            conn.execute(
                "ALTER TABLE land_estates ADD COLUMN shelter_access INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    /// The character's own estate shelter, overdue or not like the estate
    /// return scroll, and the family seat's when it is open to the family and
    /// is another estate.
    pub fn own_shelters(&self, character_id: i64) -> Result<OwnShelters, AuthError> {
        let conn = self.open_connection()?;
        let own = Self::homestead_id(&conn, character_id)?;
        let estate = match own {
            Some(id) => shelter_site(&conn, id)?,
            None => None,
        };
        let seat = match super::family::membership(&conn, character_id)? {
            Some(member) => super::family::family_seat(&conn, member.family_id)?,
            None => None,
        };
        let family_seat = match seat.filter(|seat| Some(*seat) != own) {
            Some(seat) => {
                let open: bool = conn.query_row(
                    "SELECT missed=0 AND shelter_access>=?2 FROM land_estates WHERE id=?1",
                    params![seat, ShelterAccess::Family.code()],
                    |row| row.get(0),
                )?;
                if open {
                    shelter_site(&conn, seat)?
                } else {
                    None
                }
            }
            None => None,
        };
        Ok(OwnShelters {
            estate,
            family_seat,
        })
    }

    /// A public shelter's name and cots, unless it closed or fell overdue.
    pub fn public_shelter(
        &self,
        estate_id: i64,
    ) -> Result<Option<(String, ShelterSite)>, AuthError> {
        let conn = self.open_connection()?;
        let name: Option<String> = conn
            .query_row(
                "SELECT COALESCE(NULLIF(e.name,''), c.character_name) FROM land_estates e
                 JOIN characters c ON c.id=e.owner_id
                 WHERE e.id=?1 AND e.missed=0 AND e.shelter_access=?2",
                params![estate_id, ShelterAccess::Public.code()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(name) = name else {
            return Ok(None);
        };
        Ok(shelter_site(&conn, estate_id)?.map(|site| (name, site)))
    }

    /// Every public shelter's first aid cot, for the world map.
    pub fn public_shelter_markers(&self) -> Result<Vec<ShelterMarker>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare_cached(
            "SELECT e.id, COALESCE(NULLIF(e.name,''), ch.character_name), c.x, c.z
             FROM land_estates e
             JOIN characters ch ON ch.id=e.owner_id
             JOIN estate_chests c ON c.id=(SELECT MIN(id) FROM estate_chests
                 WHERE estate_id=e.id AND item_def_id=?2)
             WHERE e.shelter_access=?1 AND e.missed=0",
        )?;
        let markers = stmt
            .query_map(params![ShelterAccess::Public.code(), AID_COT], |row| {
                Ok(ShelterMarker {
                    estate_id: row.get(0)?,
                    name: row.get(1)?,
                    x: row.get::<_, f64>(2)? as f32,
                    z: row.get::<_, f64>(3)? as f32,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(markers)
    }

    /// The owner names who may wake on their estate's aid cots through one of
    /// those cots. Returns the estate.
    pub fn set_shelter_access(
        &self,
        character_id: i64,
        furniture_id: i64,
        access: ShelterAccess,
    ) -> Verdict<i64> {
        let conn = self.open_connection()?;
        let estate: Option<i64> = conn
            .query_row(
                "SELECT e.id FROM estate_chests c JOIN land_estates e ON e.id=c.estate_id
                 WHERE c.id=?1 AND c.item_def_id=?2
                   AND e.account_name=(SELECT account_name FROM characters WHERE id=?3)",
                params![furniture_id, AID_COT, character_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(estate_id) = estate else {
            return Ok(Err(
                "Only the estate's owner sets who may wake on its aid cots.",
            ));
        };
        conn.execute(
            "UPDATE land_estates SET shelter_access=?2 WHERE id=?1",
            params![estate_id, access.code()],
        )?;
        Ok(Ok(estate_id))
    }

    pub(super) fn shelter_access_of(
        conn: &Connection,
        estate_id: i64,
    ) -> Result<ShelterAccess, rusqlite::Error> {
        Ok(conn
            .query_row(
                "SELECT shelter_access FROM land_estates WHERE id=?1",
                [estate_id],
                |row| row.get(0),
            )
            .optional()?
            .map_or(ShelterAccess::Estate, ShelterAccess::from_code))
    }
}
