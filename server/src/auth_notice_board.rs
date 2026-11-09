use super::{unix_now, AuthError, AuthService};
use crate::game_state::GameState;
use onlinerpg_shared::notice_board::{
    NoticeBoardRefusal, NoticeComment, NoticePost, NoticePostSummary, NOTICE_COMMENTS_PER_POST,
    NOTICE_LIST_LIMIT,
};
use rusqlite::{params, Connection, OptionalExtension};

/// A board is one placement: its region file and editor object id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BoardKey {
    pub rx: i32,
    pub rz: i32,
    pub object_id: u32,
}

pub type PostView = (NoticePost, Vec<NoticeComment>);

fn load_post(
    conn: &Connection,
    board: BoardKey,
    post_id: i64,
) -> rusqlite::Result<Option<PostView>> {
    let post = conn
        .prepare_cached(
            "SELECT id, title, body, author, game_seconds FROM notice_posts
             WHERE id = ?1 AND region_x = ?2 AND region_z = ?3 AND object_id = ?4",
        )?
        .query_row(
            params![post_id, board.rx, board.rz, board.object_id],
            |row| {
                Ok(NoticePost {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    body: row.get(2)?,
                    author: row.get(3)?,
                    written_at: GameState::total_game_seconds_to_datetime(row.get(4)?),
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
        )
    }

    pub fn notice_posts(&self, board: BoardKey) -> Result<Vec<NoticePostSummary>, AuthError> {
        let conn = self.open_connection()?;
        let mut stmt = conn.prepare_cached(
            "SELECT p.id, p.title, p.author, p.game_seconds,
                    (SELECT COUNT(*) FROM notice_comments c WHERE c.post_id = p.id)
             FROM notice_posts p
             WHERE p.region_x = ?1 AND p.region_z = ?2 AND p.object_id = ?3
             ORDER BY p.id DESC LIMIT ?4",
        )?;
        let posts = stmt
            .query_map(
                params![
                    board.rx,
                    board.rz,
                    board.object_id,
                    NOTICE_LIST_LIMIT as i64
                ],
                |row| {
                    Ok(NoticePostSummary {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        author: row.get(2)?,
                        written_at: GameState::total_game_seconds_to_datetime(row.get(3)?),
                        comment_count: row.get(4)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(posts)
    }

    pub fn notice_post(
        &self,
        board: BoardKey,
        post_id: i64,
    ) -> Result<Option<PostView>, AuthError> {
        let conn = self.open_connection()?;
        Ok(load_post(&conn, board, post_id)?)
    }

    pub fn add_notice_post(
        &self,
        board: BoardKey,
        character_id: i64,
        author: &str,
        title: &str,
        body: &str,
        game_seconds: i64,
    ) -> Result<NoticePost, AuthError> {
        let conn = self.open_connection()?;
        conn.prepare_cached(
            "INSERT INTO notice_posts
                (region_x, region_z, object_id, character_id, author, title, body,
                 created_at, game_seconds)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?
        .execute(params![
            board.rx,
            board.rz,
            board.object_id,
            character_id,
            author,
            title,
            body,
            unix_now(),
            game_seconds
        ])?;
        Ok(NoticePost {
            id: conn.last_insert_rowid(),
            title: title.to_string(),
            body: body.to_string(),
            author: author.to_string(),
            written_at: GameState::total_game_seconds_to_datetime(game_seconds),
        })
    }

    /// The post with the new comment, or why it was refused.
    pub fn add_notice_comment(
        &self,
        board: BoardKey,
        post_id: i64,
        character_id: i64,
        author: &str,
        body: &str,
        game_seconds: i64,
    ) -> Result<Result<PostView, NoticeBoardRefusal>, AuthError> {
        let mut conn = self.open_connection()?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let Some((post, mut comments)) = load_post(&tx, board, post_id)? else {
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
