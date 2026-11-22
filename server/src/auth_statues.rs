use super::{AuthError, AuthService};
use rusqlite::{params, Connection};

/// One `dungeon_statues` row. `look` is the frozen `Player` as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct StatueRow {
    pub id: i64,
    pub entrance_id: String,
    pub depth: u8,
    pub character_id: Option<i64>,
    /// `None` once the character is gone.
    pub name: Option<String>,
    pub look: String,
    pub night: i64,
    pub year: u32,
    pub month: u8,
    pub day: u8,
}

impl AuthService {
    pub(super) fn ensure_statue_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dungeon_statues (
                id INTEGER PRIMARY KEY,
                entrance_id TEXT NOT NULL,
                depth INTEGER NOT NULL,
                character_id INTEGER,
                name TEXT NOT NULL,
                look TEXT NOT NULL,
                night INTEGER NOT NULL,
                year INTEGER NOT NULL,
                month INTEGER NOT NULL,
                day INTEGER NOT NULL
            );",
        )
    }

    /// Every statue, oldest first. A deleted character's keeps its look and
    /// loses its name.
    pub fn statues(&self) -> Result<Vec<StatueRow>, AuthError> {
        let conn = self.open_connection()?;
        let mut query = conn.prepare(
            "SELECT s.id, s.entrance_id, s.depth, s.character_id,
                    CASE WHEN c.id IS NULL THEN NULL ELSE s.name END,
                    s.look, s.night, s.year, s.month, s.day
             FROM dungeon_statues s LEFT JOIN characters c ON c.id = s.character_id
             ORDER BY s.id",
        )?;
        let rows = query
            .query_map([], |row| {
                Ok(StatueRow {
                    id: row.get(0)?,
                    entrance_id: row.get(1)?,
                    depth: row.get(2)?,
                    character_id: row.get(3)?,
                    name: row.get(4)?,
                    look: row.get(5)?,
                    night: row.get(6)?,
                    year: row.get(7)?,
                    month: row.get(8)?,
                    day: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// New statues in and crowded-out ones gone, in one transaction.
    pub fn save_statues(&self, added: &[StatueRow], removed: &[i64]) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        let transaction = conn.unchecked_transaction()?;
        for row in added {
            transaction.execute(
                "INSERT OR REPLACE INTO dungeon_statues
                    (id, entrance_id, depth, character_id, name, look, night, year, month, day)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    row.id,
                    row.entrance_id,
                    row.depth,
                    row.character_id,
                    row.name.as_deref().unwrap_or_default(),
                    row.look,
                    row.night,
                    row.year,
                    row.month,
                    row.day
                ],
            )?;
        }
        for id in removed {
            transaction.execute("DELETE FROM dungeon_statues WHERE id=?1", [id])?;
        }
        transaction.commit()?;
        Ok(())
    }
}
