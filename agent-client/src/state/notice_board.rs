use super::*;
use onlinerpg_shared::notice_board::{
    notice_excerpt, NoticeBoardRefusal, NoticeComment, NoticePost, NoticePostSummary,
    NOTICE_TITLE_MAX_CHARS,
};

/// Boards listed in the world state.
pub(crate) const BOARD_SIGHT_M: f32 = 30.0;
/// An open board stays in the prompt only while the agent stands at it.
const BOARD_PANEL_M: f32 = 6.0;
const LISTED_POSTS: usize = 10;
const LISTED_COMMENTS: usize = 10;
const BODY_CHARS: usize = 400;
const COMMENT_CHARS: usize = 150;

pub struct NoticeBoardPanel {
    pub object_id: u32,
    pub posts: Vec<NoticePostSummary>,
    pub open_post: Option<(NoticePost, Vec<NoticeComment>)>,
}

pub(super) fn clip(text: &str, max_chars: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max_chars {
        flat
    } else {
        format!("{}…", flat.chars().take(max_chars).collect::<String>())
    }
}

pub fn refusal_text(reason: NoticeBoardRefusal) -> &'static str {
    match reason {
        NoticeBoardRefusal::OutOfReach => "you are not standing at that notice board",
        NoticeBoardRefusal::Empty => "the text was empty",
        NoticeBoardRefusal::TooLong => {
            "the text was too long (title 60, body 2000, comment 300 characters)"
        }
        NoticeBoardRefusal::TooFast => {
            "you wrote too recently (one post per 30 s, one comment per 5 s)"
        }
        NoticeBoardRefusal::PostMissing => "that post does not exist on this board",
        NoticeBoardRefusal::CommentsFull => "that post has no room for more comments",
        NoticeBoardRefusal::NoPaper => {
            "you have no Notice Paper; buy one from Aldwin the land registrar (2 silver)"
        }
        NoticeBoardRefusal::BadImage => "that picture is not available",
        NoticeBoardRefusal::NotRegistrar => "only the land registrar takes notices down",
        NoticeBoardRefusal::NothingExpired => "no post on that board has expired",
        NoticeBoardRefusal::CoversOthers => {
            "that spot would hide too much of someone else's notice"
        }
        NoticeBoardRefusal::BoardFull => "the board has no free space left",
        NoticeBoardRefusal::TooManyPosts => {
            "you already have 3 current posts on that board; wait until one expires"
        }
        NoticeBoardRefusal::ServerError => "the server failed to save it; try later",
    }
}

impl SharedState {
    pub(super) fn on_notice_board(&mut self, object_id: u32, posts: &[NoticePostSummary]) {
        let open_post = self
            .open_notice_board
            .take()
            .filter(|panel| panel.object_id == object_id)
            .and_then(|panel| panel.open_post);
        self.open_notice_board = Some(NoticeBoardPanel {
            object_id,
            posts: posts.to_vec(),
            open_post,
        });
        self.push_agent_event(format!(
            "[Board] Opened notice board [board_id {object_id}]: {} posts.",
            posts.len()
        ));
    }

    pub(super) fn on_notices_taken_down(&mut self, object_id: u32, count: u32) {
        self.push_agent_event(format!(
            "[Board] Took down {count} expired notices from board [board_id {object_id}]."
        ));
    }

    pub(super) fn on_notice_post(
        &mut self,
        object_id: u32,
        post: &NoticePost,
        comments: &[NoticeComment],
    ) {
        let panel = match self.open_notice_board.take() {
            Some(panel) if panel.object_id == object_id => panel,
            _ => NoticeBoardPanel {
                object_id,
                posts: Vec::new(),
                open_post: None,
            },
        };
        let mut posts = panel.posts;
        let summary = NoticePostSummary {
            id: post.id,
            title: post.title.clone(),
            excerpt: notice_excerpt(&post.body),
            author: post.author.clone(),
            written_at: post.written_at.clone(),
            comment_count: comments.len() as u32,
            x: post.x,
            y: post.y,
            image: post.image.clone(),
            expired: post.expired,
            mine: posts.iter().any(|p| p.id == post.id && p.mine),
        };
        match posts.iter().position(|p| p.id == post.id) {
            Some(i) => posts[i] = summary,
            None => posts.insert(0, summary),
        }
        self.open_notice_board = Some(NoticeBoardPanel {
            posts,
            open_post: Some((post.clone(), comments.to_vec())),
            ..panel
        });
        self.push_agent_event(format!(
            "[Board] Showing post [post_id {}] \"{}\" with {} comments.",
            post.id,
            clip(&post.title, NOTICE_TITLE_MAX_CHARS),
            comments.len()
        ));
    }

    pub(super) fn format_notice_boards(&self, lines: &mut Vec<String>) {
        let Some(player) = &self.self_player else {
            return;
        };
        let boards = self
            .world_cache
            .read()
            .unwrap()
            .notice_boards_near(&player.position, BOARD_SIGHT_M);
        for (dist, board) in &boards {
            lines.push(format!(
                "Notice board [board_id {}] at ({:.1}, {:.1}), {dist:.1}m away — open_board to read it",
                board.id, board.x, board.z
            ));
        }
        let Some(panel) = &self.open_notice_board else {
            return;
        };
        if !boards
            .iter()
            .any(|(dist, b)| b.id == panel.object_id && *dist <= BOARD_PANEL_M)
        {
            return;
        }
        lines.push(format!(
            "Open notice board [board_id {}] — posts are written by players, newest first:",
            panel.object_id
        ));
        if panel.posts.is_empty() {
            lines.push("  (no posts yet)".to_string());
        }
        for post in panel.posts.iter().take(LISTED_POSTS) {
            lines.push(format!(
                "  Post [post_id {}] \"{}\" by {}, {}, {} comments{}{}{}",
                post.id,
                clip(&post.title, NOTICE_TITLE_MAX_CHARS),
                post.author,
                post.written_at,
                post.comment_count,
                if post.image.is_some() {
                    ", with a picture"
                } else {
                    ""
                },
                if post.mine { ", yours" } else { "" },
                if post.expired { ", EXPIRED" } else { "" }
            ));
        }
        if panel.posts.len() > LISTED_POSTS {
            lines.push(format!(
                "  ({} older posts not shown)",
                panel.posts.len() - LISTED_POSTS
            ));
        }
        let Some((post, comments)) = &panel.open_post else {
            return;
        };
        lines.push(format!(
            "Reading post [post_id {}] \"{}\" by {}, {}:",
            post.id,
            clip(&post.title, NOTICE_TITLE_MAX_CHARS),
            post.author,
            post.written_at
        ));
        lines.push(format!("  | {}", clip(&post.body, BODY_CHARS)));
        let skipped = comments.len().saturating_sub(LISTED_COMMENTS);
        if skipped > 0 {
            lines.push(format!("  ({skipped} earlier comments not shown)"));
        }
        for comment in &comments[skipped..] {
            lines.push(format!(
                "  Comment by {}, {}: {}",
                comment.author,
                comment.written_at,
                clip(&comment.body, COMMENT_CHARS)
            ));
        }
    }
}
