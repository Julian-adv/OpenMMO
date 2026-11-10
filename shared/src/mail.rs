//! Letters between characters, posted and collected at mailboxes (doc/MAIL.md).

use crate::estate_storage::estate_storage_def;
use crate::notice_board::{validate_notice_text, NoticeBoardRefusal};
use crate::world::GameDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const LETTER_ITEM: &str = "letter";
pub const LETTER_BODY_MAX_CHARS: usize = 1000;
pub const LETTER_EXCERPT_CHARS: usize = 60;
/// Letters a recipient may hold; more are refused until some are deleted.
pub const MAILBOX_CAPACITY: usize = 100;
pub const MAIL_SEND_COOLDOWN_SECS: u64 = 10;

/// A village mailbox (map editor object) or one placed on an estate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MailboxRef {
    Village { object_id: u32 },
    Estate { furniture_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LetterSummary {
    pub id: i64,
    pub sender: String,
    pub excerpt: String,
    pub written_at: GameDateTime,
    pub read: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Letter {
    pub id: i64,
    pub sender: String,
    pub body: String,
    pub written_at: GameDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailRefusal {
    OutOfReach,
    Empty,
    TooLong,
    TooFast,
    NoLetterPaper,
    NoSuchRecipient,
    RecipientFull,
    LetterMissing,
    ServerError,
}

pub fn letter_excerpt(body: &str) -> String {
    body.chars().take(LETTER_EXCERPT_CHARS).collect()
}

pub fn validate_letter_body(body: &str) -> Result<&str, MailRefusal> {
    validate_notice_text(body, LETTER_BODY_MAX_CHARS).map_err(|e| match e {
        NoticeBoardRefusal::Empty => MailRefusal::Empty,
        _ => MailRefusal::TooLong,
    })
}

/// Catalog objects flagged `mailbox` in data/object_catalog.json.
pub fn is_mailbox(type_id: &str) -> bool {
    #[derive(Deserialize)]
    struct Entry {
        id: String,
        #[serde(default)]
        mailbox: bool,
    }
    static MAILBOXES: OnceLock<HashSet<String>> = OnceLock::new();
    MAILBOXES
        .get_or_init(|| {
            serde_json::from_str::<Vec<Entry>>(include_str!("../../data/object_catalog.json"))
                .expect("object_catalog.json is malformed")
                .into_iter()
                .filter(|e| e.mailbox)
                .map(|e| e.id)
                .collect()
        })
        .contains(type_id)
}

/// Estate furniture whose model is a mailbox.
pub fn is_mailbox_furniture(item_def_id: &str) -> bool {
    estate_storage_def(item_def_id).is_some_and(|def| is_mailbox(&def.model_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_catalog_and_furniture_agree() {
        assert!(is_mailbox("mailbox"));
        assert!(!is_mailbox("notice_board"));
        assert!(is_mailbox_furniture("furniture_mailbox"));
        assert!(!is_mailbox_furniture("storage_chest"));
    }

    #[test]
    fn body_is_trimmed_and_bounded() {
        assert_eq!(validate_letter_body("  hi "), Ok("hi"));
        assert_eq!(validate_letter_body(" "), Err(MailRefusal::Empty));
        let long = "a".repeat(LETTER_BODY_MAX_CHARS + 1);
        assert_eq!(validate_letter_body(&long), Err(MailRefusal::TooLong));
    }
}
