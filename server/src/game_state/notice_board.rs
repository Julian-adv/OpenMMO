//! Village notice boards (doc/NOTICE_BOARD.md).

use super::inventory::hold_one;
use super::passability::regions_around;
use super::player_interaction::placement_position;
use super::time::GAME_SECONDS_PER_DAY;
use super::{auth_db, GameState};
use crate::auth::{
    AuthError, AuthService, BoardKey, NewNoticePost, NoticeClock, PostView, StoredPaper,
};
use crate::types::PlayerId;
use onlinerpg_shared::notice_board::{
    clamp_paper_position, free_paper_spot, is_notice_board, spot_allowed, validate_notice_text,
    NoticeBoardRefusal, NoticePaper, NOTICE_BODY_MAX_CHARS, NOTICE_COMMENT_COOLDOWN_SECS,
    NOTICE_COMMENT_MAX_CHARS, NOTICE_GRACE_DAYS, NOTICE_LIFETIME_DAYS, NOTICE_PAPER_ITEM,
    NOTICE_POSTS_PER_WRITER, NOTICE_POST_COOLDOWN_SECS, NOTICE_TITLE_MAX_CHARS,
};
use onlinerpg_shared::ServerMessage;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{error, info};

/// How far a player may be from a board and still be sent its papers.
const PEEK_RANGE_M: f32 = 80.0;

/// Papers per board, filled on first peek and dropped on every change.
#[derive(Default)]
pub(super) struct NoticePaperCache(HashMap<BoardKey, Arc<Vec<StoredPaper>>>);

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
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

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

pub(super) fn notice_clock(now: i64) -> NoticeClock {
    NoticeClock {
        expired_at: now - NOTICE_LIFETIME_DAYS * GAME_SECONDS_PER_DAY,
        hidden_at: now - (NOTICE_LIFETIME_DAYS + NOTICE_GRACE_DAYS) * GAME_SECONDS_PER_DAY,
    }
}

impl GameState {
    fn current_notice_clock(&self) -> NoticeClock {
        notice_clock(self.current_total_game_seconds())
    }

    fn forget_notice_papers(&self, board: BoardKey) {
        self.notice_papers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .remove(&board);
    }

    async fn stored_notice_papers(
        &self,
        board: BoardKey,
        auth: &AuthService,
    ) -> Result<Arc<Vec<StoredPaper>>, AuthError> {
        if let Some(papers) = self
            .notice_papers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .get(&board)
        {
            return Ok(Arc::clone(papers));
        }
        let hidden_at = self.current_notice_clock().hidden_at;
        let auth = auth.clone();
        let papers = Arc::new(auth_db(move || auth.notice_papers(board, hidden_at)).await?);
        self.notice_papers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .insert(board, Arc::clone(&papers));
        Ok(papers)
    }

    fn visible_papers(&self, stored: &[StoredPaper]) -> Vec<NoticePaper> {
        let clock = self.current_notice_clock();
        stored
            .iter()
            .rev()
            .filter(|p| p.game_seconds > clock.hidden_at)
            .map(|p| NoticePaper {
                id: p.id,
                x: p.x,
                y: p.y,
                poster: p.poster,
                expired: p.game_seconds <= clock.expired_at,
            })
            .collect()
    }

    /// Where the player's new paper goes, or why it may not go up: the
    /// writer limit, then the cover cap against other writers' current papers.
    async fn pin_spot(
        &self,
        player_id: &PlayerId,
        board: BoardKey,
        position: Option<(u16, u16)>,
        auth: &AuthService,
    ) -> Result<(u16, u16), NoticeBoardRefusal> {
        let me = self
            .character_id_of(player_id)
            .await
            .ok_or(NoticeBoardRefusal::ServerError)?;
        let stored = self
            .stored_notice_papers(board, auth)
            .await
            .map_err(|err| {
                error!("Failed to load notice papers {board:?}: {err}");
                NoticeBoardRefusal::ServerError
            })?;
        let clock = self.current_notice_clock();
        let shown: Vec<_> = stored
            .iter()
            .filter(|p| p.game_seconds > clock.hidden_at)
            .collect();
        let current = shown.iter().filter(|p| p.game_seconds > clock.expired_at);
        if current.clone().filter(|p| p.character_id == me).count() >= NOTICE_POSTS_PER_WRITER {
            return Err(NoticeBoardRefusal::TooManyPosts);
        }
        let protected: Vec<_> = current
            .filter(|p| p.character_id != me)
            .map(|p| (p.x, p.y))
            .collect();
        match position {
            Some((x, y)) => {
                let spot = clamp_paper_position(x, y);
                spot_allowed(spot, &protected)
                    .then_some(spot)
                    .ok_or(NoticeBoardRefusal::CoversOthers)
            }
            None => {
                let all: Vec<_> = shown.iter().map(|p| (p.x, p.y)).collect();
                free_paper_spot(&protected, &all).ok_or(NoticeBoardRefusal::BoardFull)
            }
        }
    }

    /// A board within `PEEK_RANGE_M` of the player.
    async fn notice_board_in_sight(
        &self,
        player_id: &PlayerId,
        object_id: u32,
    ) -> Option<BoardKey> {
        let players = self.players.read().await;
        let player = players.get(player_id)?;
        let index = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner());
        regions_around(player.position.x, player.position.z).find_map(|(rx, rz)| {
            index.get(&(rx, rz))?.iter().find_map(|p| {
                if p.id != object_id || !is_notice_board(&p.type_id) {
                    return None;
                }
                (placement_position(p).dist_xz_sq(&player.position) <= PEEK_RANGE_M * PEEK_RANGE_M)
                    .then_some(BoardKey { rx, rz, object_id })
            })
        })
    }

    pub async fn peek_notice_board(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        auth: &AuthService,
    ) {
        let Some(board) = self.notice_board_in_sight(player_id, object_id).await else {
            return;
        };
        match self.stored_notice_papers(board, auth).await {
            Ok(stored) => {
                let papers = self.visible_papers(&stored);
                self.send_direct_message(
                    player_id,
                    ServerMessage::NoticeBoardPapers { object_id, papers },
                )
                .await;
            }
            Err(err) => error!("Failed to load notice papers {board:?}: {err}"),
        }
    }

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
        let character_id = self.character_id_of(player_id).await?;
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
        let clock = self.current_notice_clock();
        let viewer = self.character_id_of(player_id).await.unwrap_or(0);
        let auth = auth.clone();
        match auth_db(move || auth.notice_posts(board, clock, viewer)).await {
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
        let clock = self.current_notice_clock();
        let auth = auth.clone();
        let result = auth_db(move || auth.notice_post(board, post_id, clock)).await;
        let result = result.map(|post| post.ok_or(NoticeBoardRefusal::PostMissing));
        self.reply_post(player_id, board, result).await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn write_notice_post(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        title: &str,
        body: &str,
        image: Option<&str>,
        position: Option<(u16, u16)>,
        auth: &AuthService,
    ) {
        let title = match validate_notice_text(title, NOTICE_TITLE_MAX_CHARS) {
            Ok(t) => t.to_string(),
            Err(reason) => return self.refuse_notice(player_id, reason).await,
        };
        let body = match (validate_notice_text(body, NOTICE_BODY_MAX_CHARS), image) {
            (Ok(b), _) => b.to_string(),
            (Err(NoticeBoardRefusal::Empty), Some(_)) => String::new(),
            (Err(reason), _) => return self.refuse_notice(player_id, reason).await,
        };
        if let Some(hash) = image {
            if !self.cape_textures.is_wearable(hash).await {
                return self
                    .refuse_notice(player_id, NoticeBoardRefusal::BadImage)
                    .await;
            }
        }
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        if !self.holds_item(player_id, NOTICE_PAPER_ITEM).await {
            return self
                .refuse_notice(player_id, NoticeBoardRefusal::NoPaper)
                .await;
        }
        let position = match self.pin_spot(player_id, board, position, auth).await {
            Ok(spot) => spot,
            Err(reason) => return self.refuse_notice(player_id, reason).await,
        };
        let Some(w) = self.notice_writer(player_id, Write::Post).await else {
            return;
        };
        let hold = self
            .inventories
            .write()
            .await
            .get_mut(player_id)
            .and_then(|inv| hold_one(&mut inv.bag, |i| i.item_def_id == NOTICE_PAPER_ITEM));
        let Some(hold) = hold else {
            return self
                .refuse_notice(player_id, NoticeBoardRefusal::NoPaper)
                .await;
        };
        info!(author = %w.name, ?board, poster = image.is_some(), "notice post");
        let image = image.map(str::to_string);
        let auth = auth.clone();
        let result = auth_db(move || {
            auth.add_notice_post(
                board,
                &NewNoticePost {
                    character_id: w.character_id,
                    author: &w.name,
                    title: &title,
                    body: &body,
                    image: image.as_deref(),
                    position,
                    game_seconds: w.game_seconds,
                },
            )
        })
        .await;
        let saved = result
            .as_ref()
            .map(|_| ())
            .map_err(|_| "notice post not saved");
        if self
            .finish_bag_write(player_id, hold, &[], saved)
            .await
            .is_ok()
        {
            self.log_consumed(player_id, [NOTICE_PAPER_ITEM]).await;
        }
        self.forget_notice_papers(board);
        self.reply_post(player_id, board, result.map(|post| Ok((post, Vec::new()))))
            .await;
    }

    /// The land registrar takes down every post past its lifetime.
    pub async fn take_down_notices(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        auth: &AuthService,
    ) {
        let registrar = self.players.read().await.get(player_id).is_some_and(|p| {
            p.is_official_npc
                && super::trading::trader_def_by_name(&p.name)
                    .is_some_and(|def| def.is_land_registrar())
        });
        if !registrar {
            return self
                .refuse_notice(player_id, NoticeBoardRefusal::NotRegistrar)
                .await;
        }
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let expired_at = self.current_notice_clock().expired_at;
        let db = auth.clone();
        match auth_db(move || db.take_down_notices(board, expired_at)).await {
            Ok(0) => {
                self.refuse_notice(player_id, NoticeBoardRefusal::NothingExpired)
                    .await
            }
            Ok(count) => {
                info!(?board, count, "notices taken down");
                self.forget_notice_papers(board);
                self.send_direct_message(
                    player_id,
                    ServerMessage::NoticesTakenDown { object_id, count },
                )
                .await;
                self.open_notice_board(player_id, object_id, auth).await;
            }
            Err(err) => {
                error!("Failed to take down notices {board:?}: {err}");
                self.refuse_notice(player_id, NoticeBoardRefusal::ServerError)
                    .await;
            }
        }
    }

    /// The writer unpins their own post; its paper is not returned.
    pub async fn remove_notice_post(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        post_id: i64,
        auth: &AuthService,
    ) {
        let Some(board) = self.notice_board_at(player_id, object_id).await else {
            return;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return self
                .refuse_notice(player_id, NoticeBoardRefusal::ServerError)
                .await;
        };
        let hidden_at = self.current_notice_clock().hidden_at;
        let db = auth.clone();
        let result =
            auth_db(move || db.remove_notice_post(board, post_id, character_id, hidden_at)).await;
        match result {
            Ok(Ok(())) => {
                info!(
                    character_id,
                    ?board,
                    post_id,
                    "notice post removed by writer"
                );
                self.forget_notice_papers(board);
                self.send_direct_message(
                    player_id,
                    ServerMessage::NoticePostRemoved { object_id, post_id },
                )
                .await;
                self.open_notice_board(player_id, object_id, auth).await;
            }
            Ok(Err(reason)) => self.refuse_notice(player_id, reason).await,
            Err(err) => {
                error!("Failed to remove notice post {post_id} on {board:?}: {err}");
                self.refuse_notice(player_id, NoticeBoardRefusal::ServerError)
                    .await;
            }
        }
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
        let clock = self.current_notice_clock();
        let auth = auth.clone();
        let result = auth_db(move || {
            auth.add_notice_comment(
                board,
                post_id,
                w.character_id,
                &w.name,
                &body,
                w.game_seconds,
                clock,
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
