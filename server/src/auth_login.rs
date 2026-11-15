use super::{valid_name, AuthError, AuthService, NPC_ACCOUNT_PREFIX};
use rusqlite::{params, OptionalExtension};

impl AuthService {
    /// Log in with a verified Google subject id, creating the account on
    /// first login. Returns the account's player_name. Account names are
    /// random on purpose — deriving them from token claims (email/name)
    /// would persist personal data.
    pub fn login_google(&self, google_sub: &str) -> Result<String, AuthError> {
        let google_sub = google_sub.trim();
        if google_sub.is_empty() {
            return Err(AuthError::InvalidInput("Google subject id is required"));
        }

        let conn = self.open_connection()?;

        for _ in 0..100 {
            let existing: Option<String> = conn
                .query_row(
                    "SELECT player_name FROM accounts WHERE google_sub = ?1",
                    params![google_sub],
                    |row| row.get(0),
                )
                .optional()?;
            if let Some(name) = existing {
                return Ok(name);
            }

            let candidate = format!("player_{}", &uuid::Uuid::new_v4().simple().to_string()[..6]);
            match conn.execute(
                "INSERT INTO accounts (player_name, google_sub) VALUES (?1, ?2)",
                params![candidate, google_sub],
            ) {
                Ok(_) => return Ok(candidate),
                // Name taken (or lost a same-sub race): retry with a fresh name.
                Err(rusqlite::Error::SqliteFailure(e, _))
                    if e.code == rusqlite::ErrorCode::ConstraintViolation =>
                {
                    continue
                }
                Err(e) => return Err(e.into()),
            }
        }

        Err(AuthError::Database(
            "could not allocate a unique account name".to_string(),
        ))
    }

    /// Debug-build dev login into an existing account (token checked by the caller).
    #[cfg(debug_assertions)]
    pub fn dev_login(&self, account_name: &str) -> Result<String, AuthError> {
        self.open_connection()?
            .query_row(
                "SELECT player_name FROM accounts WHERE player_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(AuthError::InvalidInput("Unknown account"))
    }

    /// Log in a headless NPC account (token already checked by the caller),
    /// creating it on first use. Returns the canonical (trimmed) name.
    ///
    /// NPC accounts live in a reserved `npc_` namespace: player accounts are
    /// named `player_*` (Google) or predate this scheme (legacy), so requiring
    /// the prefix stops the shared NPC token from ever binding to a human's
    /// account, even on a config typo.
    pub fn login_npc(&self, account_name: &str) -> Result<String, AuthError> {
        let account_name = account_name.trim();
        if account_name.is_empty() {
            return Err(AuthError::InvalidInput("Account name is required"));
        }
        if !account_name.starts_with(NPC_ACCOUNT_PREFIX) {
            return Err(AuthError::InvalidInput(
                "NPC account names must start with 'npc_'",
            ));
        }
        if !valid_name(account_name) {
            return Err(AuthError::InvalidInput(
                "Account name is too long or contains invalid characters",
            ));
        }

        let conn = self.open_connection()?;
        let existing_sub: Option<Option<String>> = conn
            .query_row(
                "SELECT google_sub FROM accounts WHERE player_name = ?1",
                params![account_name],
                |row| row.get(0),
            )
            .optional()?;

        match existing_sub {
            Some(None) => Ok(account_name.to_string()),
            Some(Some(_)) => Err(AuthError::InvalidInput(
                "Account name belongs to a player account",
            )),
            None => {
                conn.execute(
                    "INSERT INTO accounts (player_name) VALUES (?1)",
                    params![account_name],
                )?;
                Ok(account_name.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::temp_auth;
    use rusqlite::Connection;

    #[test]
    fn npc_login_enforces_prefix_and_google_separation() {
        let (auth, db_path) = temp_auth("auth_npc");

        assert!(auth.login_npc("merchant_bob").is_err());
        assert!(auth.login_npc("").is_err());
        assert_eq!(auth.login_npc("npc_bob").unwrap(), "npc_bob");

        let player = auth.login_google("sub-123").unwrap();
        assert!(player.starts_with("player_"));
        assert!(auth.login_npc(&player).is_err());

        let conn = Connection::open(&db_path).unwrap();
        conn.execute(
            "UPDATE accounts SET google_sub = 'sub-999' WHERE player_name = 'npc_bob'",
            [],
        )
        .unwrap();
        assert!(auth.login_npc("npc_bob").is_err());
    }
}
