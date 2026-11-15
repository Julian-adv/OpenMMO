use super::{unix_now, AuthError, AuthService};
use rusqlite::{params, Connection, OptionalExtension};

/// A ban in force on an account. `until_unix` is `None` for a permanent ban.
#[derive(Debug, Clone)]
pub struct AccountBan {
    pub reason: Option<String>,
    pub until_unix: Option<i64>,
}

impl AccountBan {
    pub fn message(&self) -> String {
        ban_message(self.reason.as_deref(), self.until_unix)
    }
}

pub const DEFAULT_BAN_REASON: &str = "Banned by an operator";

/// Client-facing text, so a kicked player learns why and for how long.
pub fn ban_message(reason: Option<&str>, until_unix: Option<i64>) -> String {
    let reason = reason.unwrap_or(DEFAULT_BAN_REASON);
    match until_unix {
        None => reason.to_string(),
        Some(until) => {
            let minutes = ((until - unix_now()).max(0) as u64).div_ceil(60);
            format!("{reason} ({minutes} minute(s) remaining)")
        }
    }
}

impl AuthService {
    /// Bans key on the account, not the character: a banned player can delete
    /// and recreate characters, but the Google subject stays put. `until_unix`
    /// is NULL for a permanent ban and an epoch second for a timed one —
    /// wall-clock, because a monotonic `Instant` cannot survive a restart.
    pub(super) fn ensure_bans_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS account_bans (
                account_name TEXT PRIMARY KEY,
                reason TEXT,
                until_unix INTEGER,
                banned_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                FOREIGN KEY (account_name) REFERENCES accounts(player_name) ON DELETE CASCADE
            )",
            [],
        )?;
        Ok(())
    }

    /// Ban an account, replacing any existing ban so a re-ban can extend or
    /// shorten it. `until_unix` is `None` for a permanent ban.
    pub fn ban_account(
        &self,
        account_name: &str,
        reason: Option<&str>,
        until_unix: Option<i64>,
    ) -> Result<(), AuthError> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO account_bans (account_name, reason, until_unix)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(account_name) DO UPDATE SET
                reason = excluded.reason,
                until_unix = excluded.until_unix,
                banned_at = strftime('%s', 'now')",
            params![account_name, reason, until_unix],
        )?;
        Ok(())
    }

    pub fn unban_account(&self, account_name: &str) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let removed = conn.execute(
            "DELETE FROM account_bans WHERE account_name = ?1",
            params![account_name],
        )?;
        Ok(removed > 0)
    }

    /// The ban in force on an account right now, or `None`. An expired row is
    /// deleted on read so the table does not accumulate dead bans.
    pub fn active_ban(&self, account_name: &str) -> Result<Option<AccountBan>, AuthError> {
        let conn = self.open_connection()?;
        let row: Option<(Option<String>, Option<i64>)> = conn
            .query_row(
                "SELECT reason, until_unix FROM account_bans WHERE account_name = ?1",
                params![account_name],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((reason, until_unix)) = row else {
            return Ok(None);
        };
        if let Some(until) = until_unix {
            if until <= unix_now() {
                // Scoped to the deadline just read: a re-ban landing between
                // the select and this delete carries a different `until_unix`
                // (or NULL) and must survive.
                conn.execute(
                    "DELETE FROM account_bans
                     WHERE account_name = ?1 AND until_unix = ?2",
                    params![account_name, until],
                )?;
                return Ok(None);
            }
        }
        Ok(Some(AccountBan { reason, until_unix }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::tests::create;
    use crate::test_util::temp_auth;

    #[test]
    fn a_ban_survives_recreating_the_character_and_expires_on_its_own() {
        let (auth, _) = temp_auth("auth_ban");
        let account = auth.login_google("sub-ban").unwrap();
        let record = create(&auth, &account, "Ruffian").unwrap();

        // An operator types a character name; the ban lands on the account.
        assert_eq!(
            auth.account_of_character("ruffian").unwrap(),
            Some(("Ruffian".to_string(), account.clone())),
            "resolved ignoring case, like every other name lookup"
        );
        assert!(auth.active_ban(&account).unwrap().is_none());

        auth.ban_account(&account, Some("griefing"), None).unwrap();
        let ban = auth.active_ban(&account).unwrap().expect("ban in force");
        assert_eq!(ban.reason.as_deref(), Some("griefing"));
        assert_eq!(ban.until_unix, None, "no minutes means permanent");

        // Deleting the character does not shed the ban — the account carries it.
        auth.delete_character(&account, record.id).unwrap();
        assert!(auth.active_ban(&account).unwrap().is_some());
        assert_eq!(
            auth.login_google("sub-ban").unwrap(),
            account,
            "the same Google subject still resolves to the banned account"
        );

        // Re-banning replaces the row, so a permanent ban can be shortened.
        let until = unix_now() + 600;
        auth.ban_account(&account, Some("cooling off"), Some(until))
            .unwrap();
        let ban = auth.active_ban(&account).unwrap().expect("timed ban");
        assert_eq!(ban.until_unix, Some(until));
        assert!(ban.message().contains("10 minute"), "{}", ban.message());

        // A ban whose deadline has passed reads as absent and is swept.
        auth.ban_account(&account, None, Some(unix_now() - 1))
            .unwrap();
        assert!(auth.active_ban(&account).unwrap().is_none());
        assert!(
            !auth.unban_account(&account).unwrap(),
            "the expired row was cleared on read, so there is nothing left to lift"
        );

        // Lifting a live ban reports that it did something, once.
        auth.ban_account(&account, None, None).unwrap();
        assert!(auth.active_ban(&account).unwrap().is_some());
        assert!(auth.unban_account(&account).unwrap());
        assert!(!auth.unban_account(&account).unwrap());
        assert!(auth.active_ban(&account).unwrap().is_none());
    }

    /// A ban outlives its characters, so `/unban` has to be able to name the
    /// account directly — otherwise deleting the last character strands it.
    #[test]
    fn an_account_can_be_unbanned_after_its_last_character_is_gone() {
        let (auth, _) = temp_auth("auth_ban_orphan");
        let account = auth.login_npc("npc_ban_orphan").unwrap();
        let record = create(&auth, &account, "Lonely").unwrap();
        auth.ban_account(&account, None, None).unwrap();
        auth.delete_character(&account, record.id).unwrap();

        // The character route is gone...
        assert!(auth.account_of_character("Lonely").unwrap().is_none());
        // ...but the ban is still there, and the account name still lifts it.
        assert!(auth.active_ban(&account).unwrap().is_some());
        assert!(auth.unban_account(&account).unwrap());
        assert!(auth.active_ban(&account).unwrap().is_none());
    }
}
