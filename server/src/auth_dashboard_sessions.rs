use super::{AuthError, AuthService};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

pub const DASHBOARD_SESSION_SECONDS: i64 = 30 * 24 * 3600;
const TOKEN_HEX_LEN: usize = 64;

fn token_hash(token: &str) -> Option<Vec<u8>> {
    (token.len() == TOKEN_HEX_LEN && token.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| Sha256::digest(token.as_bytes()).to_vec())
}

impl AuthService {
    pub(super) fn ensure_dashboard_sessions_schema(
        conn: &Connection,
    ) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS dashboard_sessions (
                token_hash BLOB PRIMARY KEY,
                email TEXT NOT NULL,
                expires_at INTEGER NOT NULL
             ) WITHOUT ROWID;",
        )
    }

    /// Only the token's hash is stored; the returned token is the sole copy.
    pub fn create_dashboard_session(&self, email: &str, now: i64) -> Result<String, AuthError> {
        let mut bytes = [0u8; TOKEN_HEX_LEN / 2];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let conn = self.open_connection()?;
        conn.execute(
            "DELETE FROM dashboard_sessions WHERE expires_at <= ?1",
            [now],
        )?;
        conn.execute(
            "INSERT INTO dashboard_sessions (token_hash, email, expires_at) VALUES (?1, ?2, ?3)",
            params![
                Sha256::digest(token.as_bytes()).to_vec(),
                email,
                now + DASHBOARD_SESSION_SECONDS
            ],
        )?;
        Ok(token)
    }

    pub fn dashboard_session_email(
        &self,
        token: &str,
        now: i64,
    ) -> Result<Option<String>, AuthError> {
        let Some(hash) = token_hash(token) else {
            return Ok(None);
        };
        Ok(self
            .open_connection()?
            .query_row(
                "SELECT email FROM dashboard_sessions WHERE token_hash = ?1 AND expires_at > ?2",
                params![hash, now],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn delete_dashboard_session(&self, token: &str) -> Result<(), AuthError> {
        if let Some(hash) = token_hash(token) {
            self.open_connection()?.execute(
                "DELETE FROM dashboard_sessions WHERE token_hash = ?1",
                [hash],
            )?;
        }
        Ok(())
    }
}
