use super::characters::class_from_row;
use super::{AuthError, AuthService};
use onlinerpg_shared::messages::FriendEntry;
use rusqlite::{params, Connection};

impl AuthService {
    /// Friendships, one row per direction. Ids rather than names (unlike
    /// `character_blocks`): deleting a character must take its friendships
    /// with it, which both cascades give for free, and a name would leave a
    /// permanently-offline ghost on every friend's list.
    pub(super) fn ensure_friends_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_friends (
                character_id INTEGER NOT NULL,
                friend_id INTEGER NOT NULL,
                PRIMARY KEY (character_id, friend_id),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE,
                FOREIGN KEY (friend_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        // The reverse-direction cascade needs it, and so does nothing else:
        // every read is by `character_id`, which the primary key covers.
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_character_friends_friend_id \
             ON character_friends(friend_id)",
            [],
        )?;
        Ok(())
    }

    /// `/block` lists: character → set of character names it never wants to
    /// hear. Names rather than character ids, so a block written while the
    /// target is offline needs no id lookup and survives the target's
    /// character deletion (a recreated abuser keeps the same name).
    pub(super) fn ensure_blocks_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS character_blocks (
                character_id INTEGER NOT NULL,
                blocked_name TEXT NOT NULL,
                PRIMARY KEY (character_id, blocked_name),
                FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE CASCADE
            )",
            [],
        )?;
        // Names are unique ignoring ASCII case (the other allowed scripts
        // have no case); the index also serves the COLLATE NOCASE lookups.
        // A DB with case-colliding names fails here on purpose: rename the
        // offending characters before starting.
        conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_characters_name_unique_nocase \
             ON characters(character_name COLLATE NOCASE)",
            [],
        )?;
        conn.execute("DROP INDEX IF EXISTS idx_characters_name_nocase", [])?;
        Ok(())
    }

    /// One character's friends as (id, name, level, class, family). The join
    /// is what makes storing ids affordable: offline friends still have a
    /// name and family to show.
    pub fn load_friends(&self, character_id: i64) -> Result<Vec<FriendEntry>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.character_name, c.level, c.class, fam.name
             FROM character_friends f
             JOIN characters c ON c.id = f.friend_id
             LEFT JOIN family_members m ON m.character_id = c.id
             LEFT JOIN families fam ON fam.id = m.family_id
             WHERE f.character_id = ?1",
        )?;
        let friends = stmt
            .query_map(params![character_id], |row| {
                Ok(FriendEntry {
                    character_id: row.get(0)?,
                    name: row.get(1)?,
                    level: row.get(2)?,
                    class: class_from_row(row, 3)?,
                    family: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(friends)
    }

    /// Both directions in one transaction, so no crash can leave a one-sided
    /// friendship the callers never expect to see.
    pub fn add_friend(&self, character_id: i64, friend_id: i64) -> Result<(), AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        for (a, b) in [(character_id, friend_id), (friend_id, character_id)] {
            tx.execute(
                "INSERT OR IGNORE INTO character_friends (character_id, friend_id) \
                 VALUES (?1, ?2)",
                params![a, b],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn remove_friend(&self, character_id: i64, friend_id: i64) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM character_friends \
             WHERE (character_id = ?1 AND friend_id = ?2) \
                OR (character_id = ?2 AND friend_id = ?1)",
            params![character_id, friend_id],
        )?;
        Ok(())
    }

    pub fn load_blocked_names(&self, character_id: i64) -> Result<Vec<String>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt =
            conn.prepare("SELECT blocked_name FROM character_blocks WHERE character_id = ?1")?;
        let names = stmt
            .query_map(params![character_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(names)
    }

    pub fn add_block(&self, character_id: i64, blocked_name: &str) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT OR IGNORE INTO character_blocks (character_id, blocked_name) VALUES (?1, ?2)",
            params![character_id, blocked_name],
        )?;
        Ok(())
    }

    pub fn remove_block(&self, character_id: i64, blocked_name: &str) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM character_blocks WHERE character_id = ?1 AND blocked_name = ?2",
            params![character_id, blocked_name],
        )?;
        Ok(())
    }
}
