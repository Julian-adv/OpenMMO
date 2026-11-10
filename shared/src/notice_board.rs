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
/// Spent on each post; Aldwin sells it, and the price is the stamp duty.
pub const NOTICE_PAPER_ITEM: &str = "notice_paper";
/// Game days a post counts as current; after that Aldwin takes it down.
pub const NOTICE_LIFETIME_DAYS: i64 = 30;
/// Game days past the lifetime after which the server hides a post Aldwin missed.
pub const NOTICE_GRACE_DAYS: i64 = 1;

/// Pinnable face of the board in metres (`client/src/lib/utils/notice-board.ts`).
pub const BOARD_FACE_W: f32 = 1.66;
pub const BOARD_FACE_H: f32 = 1.4;
/// Paper positions are the paper centre in thousandths of the face, y down.
pub const PAPER_SCALE: u16 = 1000;

/// Most of another writer's current paper a new one may cover.
pub const NOTICE_MAX_COVER: f32 = 0.2;
/// Current posts one character may have on a board.
pub const NOTICE_POSTS_PER_WRITER: usize = 3;

/// Paper size in metres.
pub const PAPER_W: f32 = 0.3;
pub const PAPER_H: f32 = 0.36;

/// Keeps the whole paper on the face.
pub fn clamp_paper_position(x: u16, y: u16) -> (u16, u16) {
    let scale = f32::from(PAPER_SCALE);
    let half_x = (PAPER_W / 2.0 / BOARD_FACE_W * scale).ceil() as u16;
    let half_y = (PAPER_H / 2.0 / BOARD_FACE_H * scale).ceil() as u16;
    (
        x.clamp(half_x, PAPER_SCALE - half_x),
        y.clamp(half_y, PAPER_SCALE - half_y),
    )
}

/// Share of a paper at `b` hidden by one at `a` (tilt ignored).
pub fn cover_fraction(a: (u16, u16), b: (u16, u16)) -> f32 {
    let dx = f32::from(a.0.abs_diff(b.0)) / f32::from(PAPER_SCALE) * BOARD_FACE_W;
    let dy = f32::from(a.1.abs_diff(b.1)) / f32::from(PAPER_SCALE) * BOARD_FACE_H;
    (PAPER_W - dx).max(0.0) * (PAPER_H - dy).max(0.0) / (PAPER_W * PAPER_H)
}

/// Whether a paper at `spot` leaves each of `protected` at least mostly visible.
pub fn spot_allowed(spot: (u16, u16), protected: &[(u16, u16)]) -> bool {
    protected
        .iter()
        .all(|&p| cover_fraction(spot, p) <= NOTICE_MAX_COVER)
}

/// Where a paper goes when the writer did not choose: of the allowed spots,
/// the one least covering any paper. `None` when the board is full.
pub fn free_paper_spot(protected: &[(u16, u16)], all: &[(u16, u16)]) -> Option<(u16, u16)> {
    let mut best = None;
    let mut best_cover = f32::MAX;
    for row in 0..=8u16 {
        for col in 0..=10u16 {
            let spot = clamp_paper_position(col * 100, row * 125);
            if !spot_allowed(spot, protected) {
                continue;
            }
            let cover: f32 = all.iter().map(|&p| cover_fraction(spot, p)).sum();
            if cover < best_cover {
                best = Some(spot);
                best_cover = cover;
            }
        }
    }
    best
}

/// Spot for posts stored before papers had positions.
pub fn legacy_paper_position(id: i64) -> (u16, u16) {
    let x = 120 + (id.unsigned_abs() * 373 % 760) as u16;
    let y = 200 + (id.unsigned_abs() * 211 % 600) as u16;
    clamp_paper_position(x, y)
}

/// What the board model draws for one post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticePaper {
    pub id: i64,
    pub x: u16,
    pub y: u16,
    pub poster: bool,
    pub expired: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticePostSummary {
    pub id: i64,
    pub title: String,
    /// Start of the body, as much as a paper shows.
    pub excerpt: String,
    pub author: String,
    pub written_at: GameDateTime,
    pub comment_count: u32,
    pub x: u16,
    pub y: u16,
    /// Cape texture store hash of the attached picture.
    pub image: Option<String>,
    pub expired: bool,
    /// Written by the character this list was sent to.
    pub mine: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoticePost {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub author: String,
    pub written_at: GameDateTime,
    pub x: u16,
    pub y: u16,
    pub image: Option<String>,
    pub expired: bool,
}

pub const NOTICE_EXCERPT_CHARS: usize = 80;

pub fn notice_excerpt(body: &str) -> String {
    body.chars().take(NOTICE_EXCERPT_CHARS).collect()
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
    /// The writer carries no notice paper.
    NoPaper,
    /// The picture is not in the store or was blocked.
    BadImage,
    /// Only the land registrar takes notices down.
    NotRegistrar,
    NothingExpired,
    /// The paper would hide too much of someone else's current notice.
    CoversOthers,
    /// No allowed spot is left on the board.
    BoardFull,
    /// The writer already has `NOTICE_POSTS_PER_WRITER` current posts here.
    TooManyPosts,
    /// Only the writer removes a post.
    NotYours,
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
