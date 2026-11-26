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

/// Looks saved before protocol 180 kept gear as top-level fields; move them
/// under `gear` once.
fn nest_statue_gear(conn: &Connection) -> Result<(), rusqlite::Error> {
    const GEAR: [&str; 6] = [
        "main_hand",
        "off_hand",
        "back",
        "back_color",
        "back_texture",
        "armor",
    ];
    let tx = conn.unchecked_transaction()?;
    let flat: Vec<(i64, String)> = tx
        .prepare("SELECT id, look FROM dungeon_statues WHERE json_type(look, '$.gear') IS NULL")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut update = tx.prepare("UPDATE dungeon_statues SET look = ?1 WHERE id = ?2")?;
    for (id, look) in &flat {
        let Ok(serde_json::Value::Object(mut fields)) = serde_json::from_str(look) else {
            continue;
        };
        let gear: serde_json::Map<_, _> = GEAR
            .iter()
            .filter_map(|key| fields.remove_entry(*key))
            .collect();
        fields.insert("gear".into(), gear.into());
        update.execute(params![serde_json::Value::Object(fields).to_string(), id])?;
    }
    drop(update);
    tx.commit()?;
    if !flat.is_empty() {
        tracing::info!(migrated = flat.len(), "Nested statue gear");
    }
    Ok(())
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
        )?;
        nest_statue_gear(conn)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statues_saved_with_flat_gear_keep_it() {
        let conn = Connection::open_in_memory().unwrap();
        AuthService::ensure_statue_schema(&conn).unwrap();
        let mut look = serde_json::to_value(crate::types::new_player(
            "old".into(),
            1,
            10,
            onlinerpg_shared::CharacterClass::Knight,
            Default::default(),
            onlinerpg_shared::Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0.0,
            false,
            Default::default(),
        ))
        .unwrap();
        let fields = look.as_object_mut().unwrap();
        fields.remove("gear");
        fields.insert("main_hand".into(), "iron_sword".into());
        fields.insert("back_color".into(), "#ff0000".into());
        fields.insert(
            "armor".into(),
            serde_json::json!({ "head": "worn_plate_helmet" }),
        );
        conn.execute(
            "INSERT INTO dungeon_statues VALUES (1, 'crypt', 1, NULL, 'old', ?1, 0, 1, 1, 1)",
            params![look.to_string()],
        )
        .unwrap();

        AuthService::ensure_statue_schema(&conn).unwrap();
        AuthService::ensure_statue_schema(&conn).unwrap();

        let saved: String = conn
            .query_row("SELECT look FROM dungeon_statues", [], |row| row.get(0))
            .unwrap();
        let gear = serde_json::from_str::<onlinerpg_shared::Player>(&saved)
            .unwrap()
            .gear;
        assert_eq!(gear.main_hand.as_deref(), Some("iron_sword"));
        assert_eq!(gear.back_color.as_deref(), Some("#ff0000"));
        assert_eq!(gear.armor.head.as_deref(), Some("worn_plate_helmet"));
    }
}
