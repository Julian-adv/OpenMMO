//! Village notice boards (doc/NOTICE_BOARD.md).

use super::{auth_db, GameState};
use crate::auth::{AuthError, AuthService, BoardKey, PostView};
use crate::types::PlayerId;
use onlinerpg_shared::notice_board::{
    is_notice_board, validate_notice_text, NoticeBoardRefusal, NOTICE_BODY_MAX_CHARS,
    NOTICE_COMMENT_COOLDOWN_SECS, NOTICE_COMMENT_MAX_CHARS, NOTICE_POST_COOLDOWN_SECS,
    NOTICE_TITLE_MAX_CHARS,
};
use onlinerpg_shared::ServerMessage;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tracing::{error, info};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Write {
    Post,
    Comment,
}

impl Write {
    fn cooldown(self) -> Duration {
        Duration::from_secs(match self {
            Self::Post => NOTICE_POST_COOLDOWN_SECS,
            Self::Comment => NOTICE_COMMENT_COOLDOWN_SECS,
        })
    }

    fn verb(self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Comment => "comment",
        }
    }
}

#[derive(Default)]
pub(super) struct NoticeCooldowns(HashMap<(i64, Write), Instant>);

impl NoticeCooldowns {
    /// Starts the cooldown and returns true when `character_id` may write now.
    fn try_start(&mut self, character_id: i64, write: Write, now: Instant) -> bool {
        let open = |(_, w): &(i64, Write), last: &Instant| now.duration_since(*last) < w.cooldown();
        if self
            .0
            .get_key_value(&(character_id, write))
            .is_some_and(|(key, last)| open(key, last))
        {
            return false;
        }
        if self.0.len() >= 4096 {
            self.0.retain(|key, last| open(key, last));
        }
        self.0.insert((character_id, write), now);
        true
    }
}

struct Writer {
    character_id: i64,
    name: String,
    game_seconds: i64,
}

impl GameState {
    /// The board `object_id` the player stands at, keyed by its region file.
    async fn notice_board_at(&self, player_id: &PlayerId, object_id: u32) -> Option<BoardKey> {
        let Some(((rx, rz), _)) = self
            .reachable_placement(player_id, object_id, |p| is_notice_board(&p.type_id))
            .await
        else {
            self.refuse_notice(player_id, NoticeBoardRefusal::OutOfReach)
                .await;
            return None;
        };
        Some(BoardKey { rx, rz, object_id })
    }

    async fn refuse_notice(&self, player_id: &PlayerId, reason: NoticeBoardRefusal) {
        self.send_direct_message(player_id, ServerMessage::NoticeBoardRefused { reason })
            .await;
    }

    async fn notice_writer(&self, player_id: &PlayerId, write: Write) -> Option<Writer> {
        let name = self.players.read().await.get(player_id)?.name.clone();
        let character_id = self.player_characters.read().await.get(player_id)?.0;
        if self.refuse_if_muted(player_id, &name, write.verb()).await {
            return None;
        }
        let allowed = self
            .notice_cooldowns
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_start(character_id, write, Instant::now());
        if !allowed {
            self.refuse_notice(player_id, NoticeBoardRefusal::TooFast)
                .await;
            return None;
        }
        Some(Writer {
            character_id,
            name,
            game_seconds: Self::datetime_to_total_game_seconds(&self.current_game_datetime()),
        })
    }

    /// Sends the post view, or the refusal that replaced it.
    async fn reply_post(
        &self,
        player_id: &PlayerId,
        board: BoardKey,
        result: Result<Result<PostView, NoticeBoardRefusal>, AuthError>,
    ) {
        match result {
            Ok(Ok((post, comments))) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::NoticePostView {
                        object_id: board.object_id,
                        post,
                        comments,
                    },
                )
                .await
            }
            Ok(Err(reason)) => self.refuse_notice(player_id, reason).await,
            Err(err) => {
                error!("Notice board {board:?} failed: {err}");
                self.refuse_notice(player_id, NoticeBoardRefusal::ServerError)
                    .await;
            }
        }
    }

    pub async fn open_notice_board(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        auth: &AuthService,
    ) {
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let auth = auth.clone();
        match auth_db(move || auth.notice_posts(board)).await {
            Ok(posts) => {
                self.send_direct_message(player_id, ServerMessage::NoticeBoard { object_id, posts })
                    .await
            }
            Err(err) => {
                error!("Failed to load notice board {board:?}: {err}");
                self.refuse_notice(player_id, NoticeBoardRefusal::ServerError)
                    .await;
            }
        }
    }

    pub async fn read_notice_post(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        post_id: i64,
        auth: &AuthService,
    ) {
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let auth = auth.clone();
        let result = auth_db(move || auth.notice_post(board, post_id)).await;
        let result = result.map(|post| post.ok_or(NoticeBoardRefusal::PostMissing));
        self.reply_post(player_id, board, result).await;
    }

    pub async fn write_notice_post(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        title: &str,
        body: &str,
        auth: &AuthService,
    ) {
        let text = validate_notice_text(title, NOTICE_TITLE_MAX_CHARS).and_then(|t| {
            Ok((
                t.to_string(),
                validate_notice_text(body, NOTICE_BODY_MAX_CHARS)?,
            ))
        });
        let (title, body) = match text {
            Ok((t, b)) => (t, b.to_string()),
            Err(reason) => return self.refuse_notice(player_id, reason).await,
        };
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let Some(w) = self.notice_writer(player_id, Write::Post).await else {
            return;
        };
        info!(author = %w.name, ?board, "notice post");
        let auth = auth.clone();
        let result = auth_db(move || {
            auth.add_notice_post(
                board,
                w.character_id,
                &w.name,
                &title,
                &body,
                w.game_seconds,
            )
        })
        .await;
        self.reply_post(player_id, board, result.map(|post| Ok((post, Vec::new()))))
            .await;
    }

    pub async fn write_notice_comment(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        post_id: i64,
        body: &str,
        auth: &AuthService,
    ) {
        let body = match validate_notice_text(body, NOTICE_COMMENT_MAX_CHARS) {
            Ok(b) => b.to_string(),
            Err(reason) => return self.refuse_notice(player_id, reason).await,
        };
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let Some(w) = self.notice_writer(player_id, Write::Comment).await else {
            return;
        };
        info!(author = %w.name, post_id, "notice comment");
        let auth = auth.clone();
        let result = auth_db(move || {
            auth.add_notice_comment(
                board,
                post_id,
                w.character_id,
                &w.name,
                &body,
                w.game_seconds,
            )
        })
        .await;
        self.reply_post(player_id, board, result).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_blocks_until_it_elapses() {
        let mut cooldowns = NoticeCooldowns::default();
        let start = Instant::now();
        assert!(cooldowns.try_start(1, Write::Post, start));
        assert!(!cooldowns.try_start(1, Write::Post, start + Duration::from_secs(1)));
        assert!(cooldowns.try_start(1, Write::Comment, start));
        assert!(cooldowns.try_start(2, Write::Post, start));
        assert!(cooldowns.try_start(
            1,
            Write::Post,
            start + Duration::from_secs(NOTICE_POST_COOLDOWN_SECS)
        ));
    }
}
