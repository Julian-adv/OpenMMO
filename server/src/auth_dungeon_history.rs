use super::{AuthError, AuthService};
use rusqlite::{params, Connection};

impl AuthService {
    /// When each character last opened a dungeon's treasure chest, in world
    /// clock seconds. Stored against the game clock (also persisted, see
    /// `world_time`) rather than wall time because the chest refills at
    /// nightfall; keeping the raw timestamp instead of a derived night index
    /// leaves the refill rule free to change later.
    pub(super) fn ensure_dungeon_chest_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_dungeon_chests (
                character_id INTEGER NOT NULL,
                entrance_id TEXT NOT NULL,
                opened_game_seconds INTEGER NOT NULL,
                PRIMARY KEY (character_id, entrance_id),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Ok(())
    }

    /// Dungeon entrances each character has discovered (world-map markers).
    /// Row presence is the whole fact — losing one only means rediscovering
    /// by walking near the entrance again.
    pub(super) fn ensure_dungeon_discovery_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_dungeon_discoveries (
                character_id INTEGER NOT NULL,
                entrance_id TEXT NOT NULL,
                PRIMARY KEY (character_id, entrance_id),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        Ok(())
    }

    fn dungeon_chest_opens_on(
        conn: &Connection,
        character_id: i64,
    ) -> Result<Vec<(String, i64)>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT entrance_id, opened_game_seconds FROM character_dungeon_chests \
             WHERE character_id = ?1",
        )?;
        let opens = stmt
            .query_map(params![character_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(opens)
    }

    fn dungeon_discoveries_on(
        conn: &Connection,
        character_id: i64,
    ) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = conn.prepare(
            "SELECT entrance_id FROM character_dungeon_discoveries WHERE character_id = ?1",
        )?;
        let ids = stmt
            .query_map(params![character_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }

    /// Chest opens and discovered entrances together, as ((entrance id,
    /// world clock seconds) pairs, entrance ids), sharing one connection.
    /// Read once at login into `GameState`.
    #[allow(clippy::type_complexity)]
    pub fn load_dungeon_history(
        &self,
        character_id: i64,
    ) -> Result<(Vec<(String, i64)>, Vec<String>), AuthError> {
        let conn = self.open_connection()?;
        Ok((
            Self::dungeon_chest_opens_on(&conn, character_id)?,
            Self::dungeon_discoveries_on(&conn, character_id)?,
        ))
    }

    pub fn record_dungeon_chest_open(
        &self,
        character_id: i64,
        entrance_id: &str,
        opened_game_seconds: i64,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO character_dungeon_chests \
                 (character_id, entrance_id, opened_game_seconds) \
             VALUES (?1, ?2, ?3) \
             ON CONFLICT(character_id, entrance_id) \
             DO UPDATE SET opened_game_seconds = excluded.opened_game_seconds",
            params![character_id, entrance_id, opened_game_seconds],
        )?;
        Ok(())
    }

    pub(super) fn insert_dungeon_discoveries(
        conn: &Connection,
        rows: &[(i64, String)],
    ) -> Result<(), rusqlite::Error> {
        if rows.is_empty() {
            return Ok(());
        }
        // OR IGNORE does not cover FOREIGN KEY violations: a row queued for a
        // since-deleted character must not fail the whole batch, so skip it.
        let mut insert = conn.prepare(
            "INSERT OR IGNORE INTO character_dungeon_discoveries \
                 (character_id, entrance_id) \
                 SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM characters WHERE id = ?1)",
        )?;
        for (character_id, entrance_id) in rows {
            insert.execute(params![character_id, entrance_id])?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::temp_auth;

    /// EnterGame refuses the session when this load errs, so a missing table
    /// must surface as an error — never as a valid empty history.
    #[test]
    fn dungeon_history_load_fails_when_storage_is_unavailable() {
        let (auth, _) = temp_auth("auth_dungeon_history");
        let (opens, discoveries) = auth.load_dungeon_history(1).unwrap();
        assert!(opens.is_empty());
        assert!(discoveries.is_empty());

        for table in ["character_dungeon_chests", "character_dungeon_discoveries"] {
            let (auth, db_path) = temp_auth("auth_dungeon_history");
            let conn = Connection::open(db_path).unwrap();
            conn.execute(&format!("DROP TABLE {table}"), []).unwrap();

            assert!(auth.load_dungeon_history(1).is_err());
        }
    }
}
