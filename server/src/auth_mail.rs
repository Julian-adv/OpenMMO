use super::{unix_now, AuthError, AuthService};
use crate::game_state::GameState;
use onlinerpg_shared::mail::{
    Letter, LetterSummary, MailRefusal, LETTER_EXCERPT_CHARS, MAILBOX_CAPACITY,
};
use rusqlite::{params, Connection, OptionalExtension};

/// Letters and notice posts older than this are deleted when the server starts.
pub const MESSAGE_RETENTION_SECS: i64 = 60 * 24 * 60 * 60;

pub struct NewLetter<'a> {
    pub sender_id: i64,
    pub sender: &'a str,
    pub recipient: &'a str,
    pub body: &'a str,
    pub game_seconds: i64,
}

/// The recipient a letter went to.
pub struct Delivered {
    pub character_id: i64,
    pub name: String,
}

impl AuthService {
    pub(super) fn ensure_mail_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS mail (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                sender_id INTEGER NOT NULL,
                sender_name TEXT NOT NULL,
                recipient_id INTEGER NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                game_seconds INTEGER NOT NULL,
                read_at INTEGER,
                deleted_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS mail_recipient ON mail (recipient_id, deleted_at, id);",
        )
    }

    /// Kept letters of `recipient_id`, newest first.
    pub fn mail_inbox(&self, recipient_id: i64) -> Result<Vec<LetterSummary>, AuthError> {
        let conn = self.open_connection()?;
        let letters = conn
            .prepare_cached(
                "SELECT id, sender_name, substr(body, 1, ?2), game_seconds, read_at IS NOT NULL
                 FROM mail WHERE recipient_id = ?1 AND deleted_at IS NULL
                 ORDER BY id DESC LIMIT ?3",
            )?
            .query_map(
                params![
                    recipient_id,
                    LETTER_EXCERPT_CHARS as i64,
                    MAILBOX_CAPACITY as i64
                ],
                |row| {
                    Ok(LetterSummary {
                        id: row.get(0)?,
                        sender: row.get(1)?,
                        excerpt: row.get(2)?,
                        written_at: GameState::total_game_seconds_to_datetime(row.get(3)?),
                        read: row.get(4)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(letters)
    }

    pub fn unread_mail(&self, recipient_id: i64) -> Result<u32, AuthError> {
        let conn = self.open_connection()?;
        let unread = conn
            .prepare_cached(
                "SELECT COUNT(*) FROM mail
                 WHERE recipient_id = ?1 AND deleted_at IS NULL AND read_at IS NULL",
            )?
            .query_row([recipient_id], |row| row.get(0))?;
        Ok(unread)
    }

    /// Opens one of the reader's letters and marks it read.
    pub fn read_mail(
        &self,
        recipient_id: i64,
        letter_id: i64,
    ) -> Result<Option<Letter>, AuthError> {
        let conn = self.open_connection()?;
        let letter = conn
            .prepare_cached(
                "SELECT id, sender_name, body, game_seconds, read_at IS NULL FROM mail
                 WHERE id = ?1 AND recipient_id = ?2 AND deleted_at IS NULL",
            )?
            .query_row(params![letter_id, recipient_id], |row| {
                let letter = Letter {
                    id: row.get(0)?,
                    sender: row.get(1)?,
                    body: row.get(2)?,
                    written_at: GameState::total_game_seconds_to_datetime(row.get(3)?),
                };
                Ok((letter, row.get::<_, bool>(4)?))
            })
            .optional()?;
        let Some((letter, unread)) = letter else {
            return Ok(None);
        };
        if unread {
            conn.prepare_cached("UPDATE mail SET read_at = ?2 WHERE id = ?1 AND read_at IS NULL")?
                .execute(params![letter_id, unix_now()])?;
        }
        Ok(Some(letter))
    }

    /// Returns false when the letter is not the reader's or already gone.
    pub fn delete_mail(&self, recipient_id: i64, letter_id: i64) -> Result<bool, AuthError> {
        let conn = self.open_connection()?;
        let changed = conn
            .prepare_cached(
                "UPDATE mail SET deleted_at = ?3
                 WHERE id = ?1 AND recipient_id = ?2 AND deleted_at IS NULL",
            )?
            .execute(params![letter_id, recipient_id, unix_now()])?;
        Ok(changed > 0)
    }

    /// Delivers a letter unless the recipient does not exist or their box is full.
    pub fn send_mail(
        &self,
        letter: &NewLetter<'_>,
    ) -> Result<Result<Delivered, MailRefusal>, AuthError> {
        let Some((recipient_id, name)) = self.resolve_character_brief(letter.recipient)? else {
            return Ok(Err(MailRefusal::NoSuchRecipient));
        };
        let conn = self.open_connection()?;
        let inserted = conn
            .prepare_cached(
                "INSERT INTO mail
                    (sender_id, sender_name, recipient_id, body, created_at, game_seconds)
                 SELECT ?1, ?2, ?3, ?4, ?5, ?6
                 WHERE (SELECT COUNT(*) FROM mail
                        WHERE recipient_id = ?3 AND deleted_at IS NULL) < ?7",
            )?
            .execute(params![
                letter.sender_id,
                letter.sender,
                recipient_id,
                letter.body,
                unix_now(),
                letter.game_seconds,
                MAILBOX_CAPACITY as i64
            ])?;
        if inserted == 0 {
            return Ok(Err(MailRefusal::RecipientFull));
        }
        Ok(Ok(Delivered {
            character_id: recipient_id,
            name,
        }))
    }

    /// Deletes letters and notice posts (with their comments) written before
    /// `cutoff` (unix seconds); returns (letters, posts).
    pub fn prune_old_messages(&self, cutoff: i64) -> Result<(usize, usize), AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction()?;
        let letters = tx.execute("DELETE FROM mail WHERE created_at < ?1", [cutoff])?;
        tx.execute(
            "DELETE FROM notice_comments WHERE post_id IN
                (SELECT id FROM notice_posts WHERE created_at < ?1)",
            [cutoff],
        )?;
        let posts = tx.execute("DELETE FROM notice_posts WHERE created_at < ?1", [cutoff])?;
        tx.commit()?;
        Ok((letters, posts))
    }
}
