use super::*;
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::notice_board::{
    NoticeBoardRefusal, NoticeComment, NoticePost, NoticePostSummary,
};
use onlinerpg_shared::world::GameDateTime;

fn at(hour: u8) -> GameDateTime {
    GameDateTime {
        year: 223,
        month: 4,
        day: 13,
        hour,
        minute: 5,
    }
}

fn board_state() -> SharedState {
    let (mut state, _rx) = test_state();
    state.self_player = Some(test_player(0.0, 0.0));
    state.self_player_id = Some(PlayerId::from(1));
    state.world_cache.write().unwrap().sync_furniture(
        0,
        0,
        vec![FurniturePlacement {
            id: 7,
            type_id: "notice_board".into(),
            x: 2.0,
            y: 0.0,
            z: 0.0,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    state
}

#[test]
fn boards_nearby_and_an_open_post_reach_the_world_state_clipped() {
    let mut state = board_state();
    assert!(state
        .format_world_state()
        .contains("Notice board [board_id 7] at (2.0, 0.0), 2.0m away"));

    state.push_event(ServerMessage::NoticeBoard {
        object_id: 7,
        posts: vec![NoticePostSummary {
            id: 4,
            title: "Party wanted".into(),
            author: "jake5".into(),
            written_at: at(5),
            comment_count: 0,
            excerpt: String::new(),
            x: 500,
            y: 500,
            image: Some("a".repeat(64)),
            expired: true,
            mine: false,
        }],
    });
    let world = state.format_world_state();
    assert!(
        world.contains("Post [post_id 4] \"Party wanted\" by jake5, 0223-04-13 05:05, 0 comments, with a picture, EXPIRED")
    );

    let comments = (0..12)
        .map(|i| NoticeComment {
            id: i,
            author: "bob".into(),
            body: format!("reply {i}"),
            written_at: at(6),
        })
        .collect::<Vec<_>>();
    state.push_event(ServerMessage::NoticePostView {
        object_id: 7,
        post: NoticePost {
            id: 4,
            title: "Party wanted".into(),
            body: "x".repeat(500),
            author: "jake5".into(),
            written_at: at(5),
            x: 500,
            y: 500,
            image: None,
            expired: false,
        },
        comments,
    });
    let world = state.format_world_state();
    assert!(world.contains(&format!("  | {}…", "x".repeat(400))));
    assert!(world.contains("(2 earlier comments not shown)"));
    assert!(world.contains("Comment by bob, 0223-04-13 06:05: reply 11"));
    assert!(!world.contains("reply 1\n"));
    assert!(world.contains("12 comments"));

    state.self_player = Some(test_player(40.0, 0.0));
    assert!(!state.format_world_state().contains("Open notice board"));
}

#[test]
fn a_refusal_becomes_a_failed_event() {
    let mut state = board_state();
    state.push_event(ServerMessage::NoticeBoardRefused {
        reason: NoticeBoardRefusal::TooFast,
    });
    assert!(state
        .drain_agent_events()
        .iter()
        .any(|e| e.starts_with("[BoardFailed] you wrote too recently")));
}
