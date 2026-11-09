//! Village notice boards (doc/NOTICE_BOARD.md): posts and their comments.

use crate::world::GameDateTime;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::OnceLock;

pub const NOTICE_TITLE_MAX_CHARS: usize = 60;
pub const NOTICE_BODY_MAX_CHARS: usize = 2000;
pub const NOTICE_COMMENT_MAX_CHARS: usize = 300;
/// Newest posts a board lists; older ones stay stored but unlisted.
pub const NOTICE_LIST_LIMIT: usize = 50;
pub const NOTICE_COMMENTS_PER_POST: usize = 100;
pub const NOTICE_POST_COOLDOWN_SECS: u64 = 30;
pub const NOTICE_COMMENT_COOLDOWN_SECS: u64 = 5;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticePostSummary {
    pub id: i64,
    pub title: String,
    pub author: String,
    pub written_at: GameDateTime,
    pub comment_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticePost {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub written_at: GameDateTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticeComment {
    pub id: i64,
    pub author: String,
    pub body: String,
    pub written_at: GameDateTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeBoardRefusal {
    OutOfReach,
    Empty,
    TooLong,
    TooFast,
    PostMissing,
    CommentsFull,
    ServerError,
}

/// Trimmed text when it is non-empty and within `max_chars`.
pub fn validate_notice_text(text: &str, max_chars: usize) -> Result<&str, NoticeBoardRefusal> {
    let text = text.trim();
    if text.is_empty() {
        Err(NoticeBoardRefusal::Empty)
    } else if text.chars().count() > max_chars {
        Err(NoticeBoardRefusal::TooLong)
    } else {
        Ok(text)
    }
}

/// Catalog objects flagged `noticeBoard` in data/object_catalog.json.
pub fn is_notice_board(type_id: &str) -> bool {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        id: String,
        #[serde(default)]
        notice_board: bool,
    }
    static BOARDS: OnceLock<HashSet<String>> = OnceLock::new();
    BOARDS
        .get_or_init(|| {
            serde_json::from_str::<Vec<Entry>>(include_str!("../../data/object_catalog.json"))
                .expect("object_catalog.json is malformed")
                .into_iter()
                .filter(|e| e.notice_board)
                .map(|e| e.id)
                .collect()
        })
        .contains(type_id)
}
