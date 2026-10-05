use super::{unix_now, AuthError, AuthService};
use rusqlite::{params, OptionalExtension};

pub(super) const DELETION_DELAY_SECONDS: i64 = if cfg!(debug_assertions) {
    60
} else {
    24 * 60 * 60
};

impl AuthService {
    #[cfg(test)]
    pub(crate) fn make_character_deletion_due(&self, character_id: i64) {
        self.open_connection()
            .unwrap()
            .execute(
                "UPDATE characters SET deletion_due_at=?2 WHERE id=?1",
                params![character_id, unix_now() - 1],
            )
            .unwrap();
    }

    pub fn change_character_deletion(
        &self,
        account: &str,
        character_id: i64,
        cancel: bool,
    ) -> Result<Option<i64>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let due: Option<i64> = tx
            .query_row(
                "SELECT deletion_due_at FROM characters WHERE id=?1 AND account_name=?2",
                params![character_id, account],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(AuthError::CharacterNotFound)?;
        let now = unix_now();
        if due.is_some_and(|deadline| deadline <= now) {
            return Err(AuthError::InvalidInput(
                "The character deletion deadline has passed",
            ));
        }
        let due = if cancel {
            None
        } else {
            Some(due.unwrap_or(now + DELETION_DELAY_SECONDS))
        };
        tx.execute(
            "UPDATE characters SET deletion_due_at=?3 WHERE id=?1 AND account_name=?2",
            params![character_id, account, due],
        )?;
        tx.commit()?;
        match due {
            Some(deletion_due_at) => tracing::info!(
                account,
                character_id,
                deletion_due_at,
                "Character deletion scheduled"
            ),
            None => tracing::info!(account, character_id, "Character deletion cancelled"),
        }
        Ok(due)
    }

    pub fn due_character_deletions(&self) -> Result<Vec<(String, i64)>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare(
            "SELECT account_name,id FROM characters WHERE deletion_due_at<=?1 ORDER BY deletion_due_at,id",
        )?;
        let rows = stmt.query_map([unix_now()], |row| Ok((row.get(0)?, row.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}
