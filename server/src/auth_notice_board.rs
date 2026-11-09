use super::{unix_now, AuthError, AuthService};
use crate::game_state::GameState;
use onlinerpg_shared::notice_board::{
    legacy_paper_position, NoticeBoardRefusal, NoticeComment, NoticePost, NoticePostSummary,
    NOTICE_COMMENTS_PER_POST, NOTICE_EXCERPT_CHARS, NOTICE_LIST_LIMIT,
};
use rusqlite::{params, Connection, OptionalExtension, Row};

/// A board is one placement: its region file and editor object id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardKey {
    pub rx: i32,
    pub rz: i32,
    pub object_id: u32,
}

pub type PostView = (NoticePost, Vec<NoticeComment>);

/// Game-second limits a query is answered against.
#[derive(Debug, Clone, Copy)]
pub struct NoticeClock {
    /// Posts written at or before this are expired.
    pub expired_at: i64,
    /// Posts written at or before this are hidden.
    pub hidden_at: i64,
}

/// A stored paper; `expired` is decided when it is sent.
#[derive(Debug, Clone)]
pub struct StoredPaper {
    pub id: i64,
    pub character_id: i64,
    pub x: u16,
    pub y: u16,
    pub poster: bool,
    pub game_seconds: i64,
}

fn position(row: &Row, id: i64, x: usize) -> rusqlite::Result<(u16, u16)> {
    Ok(match (row.get(x)?, row.get(x + 1)?) {
        (Some(px), Some(py)) => (px, py),
        _ => legacy_paper_position(id),
    })
}

fn load_post(
    conn: &Connection,
    board: BoardKey,
    post_id: i64,
    clock: NoticeClock,
) -> rusqlite::Result<Option<PostView>> {
    let post = conn
        .prepare_cached(
            "SELECT id, title, body, author, game_seconds, pos_x, pos_y, image FROM notice_posts
             WHERE id = ?1 AND region_x = ?2 AND region_z = ?3 AND object_id = ?4
               AND removed_at IS NULL AND game_seconds > ?5",
        )?
        .query_row(
            params![
                post_id,
                board.rx,
                board.rz,
                board.object_id,
                clock.hidden_at
            ],
            |row| {
                let id = row.get(0)?;
                let game_seconds: i64 = row.get(4)?;
                let (x, y) = position(row, id, 5)?;
                Ok(NoticePost {
                    id,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    author: row.get(3)?,
                    written_at: GameState::total_game_seconds_to_datetime(game_seconds),
                    x,
                    y,
                    image: row.get(7)?,
                    expired: game_seconds <= clock.expired_at,
                })
            },
        )
        .optional()?;
    let Some(post) = post else {
        return Ok(None);
    };
    let comments = conn
        .prepare_cached(
            "SELECT id, author, body, game_seconds FROM notice_comments
             WHERE post_id = ?1 ORDER BY id LIMIT ?2",
        )?
        .query_map(params![post_id, NOTICE_COMMENTS_PER_POST as i64], |row| {
            Ok(NoticeComment {
                id: row.get(0)?,
                author: row.get(1)?,
                body: row.get(2)?,
                written_at: GameState::total_game_seconds_to_datetime(row.get(3)?),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((post, comments)))
}

pub struct NewNoticePost<'a> {
    pub character_id: i64,
    pub author: &'a str,
    pub title: &'a str,
    pub body: &'a str,
    pub image: Option<&'a str>,
    pub position: (u16, u16),
    pub game_seconds: i64,
}

impl AuthService {
    pub(super) fn ensure_notice_board_schema(conn: &Connection) -> Result<(), rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS notice_posts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                region_x INTEGER NOT NULL,
                region_z INTEGER NOT NULL,
                object_id INTEGER NOT NULL,
                character_id INTEGER NOT NULL,
                author TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                game_seconds INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS notice_posts_board
                ON notice_posts (region_x, region_z, object_id, id);
            CREATE TABLE IF NOT EXISTS notice_comments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                post_id INTEGER NOT NULL REFERENCES notice_posts(id) ON DELETE CASCADE,
                character_id INTEGER NOT NULL,
                author TEXT NOT NULL,
                body TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                game_seconds INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS notice_comments_post ON notice_comments (post_id, id);",
        )?;
        let columns = Self::table_columns(conn, "notice_posts")?;
        for (column, ty) in [
            ("pos_x", "INTEGER"),
            ("pos_y", "INTEGER"),
            ("image", "TEXT"),
            ("removed_at", "INTEGER"),
        ] {
            if !columns.contains(column) {
                conn.execute(
                    &format!("ALTER TABLE notice_posts ADD COLUMN {column} {ty}"),
                    [],
                )?;
            }
        }
        Ok(())
    }

    /// Current posts on a board; `viewer` is the character id their `mine` is for.
    pub fn notice_posts(
        &self,
        board: BoardKey,
        clock: NoticeClock,
        viewer: i64,
    ) -> Result<Vec<NoticePostSummary>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare_cached(
            "SELECT p.id, p.title, p.author, p.game_seconds,
                    (SELECT COUNT(*) FROM notice_comments c WHERE c.post_id = p.id),
                    p.pos_x, p.pos_y, p.image, substr(p.body, 1, ?7), p.character_id = ?6
             FROM notice_posts p
             WHERE p.region_x = ?1 AND p.region_z = ?2 AND p.object_id = ?3
               AND p.removed_at IS NULL AND p.game_seconds > ?5
             ORDER BY p.id DESC LIMIT ?4",
        )?;
        let posts = stmt
            .query_map(
                params![
                    board.rx,
                    board.rz,
                    board.object_id,
                    NOTICE_LIST_LIMIT as i64,
                    clock.hidden_at,
                    viewer,
                    NOTICE_EXCERPT_CHARS as i64
                ],
                |row| {
                    let id = row.get(0)?;
                    let game_seconds: i64 = row.get(3)?;
                    let (x, y) = position(row, id, 5)?;
                    Ok(NoticePostSummary {
                        id,
                        title: row.get(1)?,
                        excerpt: row.get(8)?,
                        author: row.get(2)?,
                        written_at: GameState::total_game_seconds_to_datetime(game_seconds),
                        comment_count: row.get(4)?,
                        x,
                        y,
                        image: row.get(7)?,
                        expired: game_seconds <= clock.expired_at,
                        mine: row.get(9)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(posts)
    }

    /// Papers still on the board, newest first, for the board model cache.
    pub fn notice_papers(
        &self,
        board: BoardKey,
        hidden_at: i64,
    ) -> Result<Vec<StoredPaper>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare_cached(
            "SELECT id, game_seconds, pos_x, pos_y, image IS NOT NULL, character_id FROM notice_posts
             WHERE region_x = ?1 AND region_z = ?2 AND object_id = ?3
               AND removed_at IS NULL AND game_seconds > ?5
             ORDER BY id DESC LIMIT ?4",
        )?;
        let papers = stmt
            .query_map(
                params![
                    board.rx,
                    board.rz,
                    board.object_id,
                    NOTICE_LIST_LIMIT as i64,
                    hidden_at
                ],
                |row| {
                    let id = row.get(0)?;
                    let (x, y) = position(row, id, 2)?;
                    Ok(StoredPaper {
                        id,
                        character_id: row.get(5)?,
                        x,
                        y,
                        poster: row.get(4)?,
                        game_seconds: row.get(1)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(papers)
    }

    pub fn notice_post(
        &self,
        board: BoardKey,
        post_id: i64,
        clock: NoticeClock,
    ) -> Result<Option<PostView>, AuthError> {
        let conn = self.open_connection()?;
        Ok(load_post(&conn, board, post_id, clock)?)
    }

    pub fn add_notice_post(
        &self,
        board: BoardKey,
        post: &NewNoticePost<'_>,
    ) -> Result<NoticePost, AuthError> {
        let conn = self.open_connection()?;
        conn.prepare_cached(
            "INSERT INTO notice_posts
                (region_x, region_z, object_id, character_id, author, title, body,
                 created_at, game_seconds, pos_x, pos_y, image)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        )?
        .execute(params![
            board.rx,
            board.rz,
            board.object_id,
            post.character_id,
            post.author,
            post.title,
            post.body,
            unix_now(),
            post.game_seconds,
            post.position.0,
            post.position.1,
            post.image
        ])?;
        Ok(NoticePost {
            id: conn.last_insert_rowid(),
            title: post.title.to_string(),
            body: post.body.to_string(),
            author: post.author.to_string(),
            written_at: GameState::total_game_seconds_to_datetime(post.game_seconds),
            x: post.position.0,
            y: post.position.1,
            image: post.image.map(str::to_string),
            expired: false,
        })
    }

    /// Marks posts written at or before `expired_at` as removed; returns how many.
    pub fn take_down_notices(&self, board: BoardKey, expired_at: i64) -> Result<u32, AuthError> {
        let conn = self.open_connection()?;
        let count = conn
            .prepare_cached(
                "UPDATE notice_posts SET removed_at = ?5
                 WHERE region_x = ?1 AND region_z = ?2 AND object_id = ?3
                   AND removed_at IS NULL AND game_seconds <= ?4",
            )?
            .execute(params![
                board.rx,
                board.rz,
                board.object_id,
                expired_at,
                unix_now()
            ])?;
        Ok(count as u32)
    }

    /// The post with the new comment, or why it was refused.
    #[allow(clippy::too_many_arguments)]
    pub fn add_notice_comment(
        &self,
        board: BoardKey,
        post_id: i64,
        character_id: i64,
        author: &str,
        body: &str,
        game_seconds: i64,
        clock: NoticeClock,
    ) -> Result<Result<PostView, NoticeBoardRefusal>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some((post, mut comments)) = load_post(&tx, board, post_id, clock)? else {
            return Ok(Err(NoticeBoardRefusal::PostMissing));
        };
        if comments.len() >= NOTICE_COMMENTS_PER_POST {
            return Ok(Err(NoticeBoardRefusal::CommentsFull));
        }
        tx.prepare_cached(
            "INSERT INTO notice_comments
                (post_id, character_id, author, body, created_at, game_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?
        .execute(params![
            post_id,
            character_id,
            author,
            body,
            unix_now(),
            game_seconds
        ])?;
        comments.push(NoticeComment {
            id: tx.last_insert_rowid(),
            author: author.to_string(),
            body: body.to_string(),
            written_at: GameState::total_game_seconds_to_datetime(game_seconds),
        });
        tx.commit()?;
        Ok(Ok((post, comments)))
    }
}
