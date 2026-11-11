use super::*;
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::notice_board::{
    NoticeBoardRef, NoticeBoardRefusal, NoticeComment, NoticePost, NoticePostSummary,
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
            door_open: false,
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
        board: NoticeBoardRef::Village { object_id: 7 },
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
        board: NoticeBoardRef::Village { object_id: 7 },
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

fn family_board(id: i64, estate_id: i64, x: f32) -> onlinerpg_shared::estate_storage::EstateChest {
    onlinerpg_shared::estate_storage::EstateChest {
        id,
        estate_id,
        owner_id: 9,
        item_def_id: "family_board".into(),
        position: p(x, 0.0, 0.0),
        rotation_deg: 0.0,
        floor_level: 0,
        overdue: false,
        revision: 1,
        text: None,
        family_id: Some(4),
        emblem: None,
    }
}

#[test]
fn a_family_board_is_listed_for_its_family_and_opens_under_its_own_id() {
    let mut state = board_state();
    state.world_cache.write().unwrap().update_estate_chests(
        PlayerId::from(1),
        &[family_board(7, 11, 3.0), family_board(8, 99, 4.0)],
        &[],
    );
    assert!(
        !state.format_world_state().contains("Family notice board"),
        "outside a family no family board is ours"
    );

    state.push_event(ServerMessage::FamilyState {
        family: Some(super::social_tests::family_info(2)),
    });
    let world = state.format_world_state();
    assert!(
        world.contains("Notice board [board_id 7] at (2.0, 0.0)"),
        "{world}"
    );
    assert!(
        world.contains("Family notice board [family_board_id 7] at (3.0, 0.0), 3.0m away"),
        "{world}"
    );
    assert!(
        !world.contains("[family_board_id 8]"),
        "another family's board: {world}"
    );

    let estate = NoticeBoardRef::Estate { furniture_id: 7 };
    state.push_event(ServerMessage::NoticeBoard {
        board: estate,
        posts: vec![NoticePostSummary {
            id: 1,
            title: "Chest stocked".into(),
            author: "Arya".into(),
            written_at: at(5),
            comment_count: 0,
            excerpt: String::new(),
            x: 500,
            y: 500,
            image: None,
            expired: false,
            mine: false,
        }],
    });
    assert!(state
        .drain_agent_events()
        .iter()
        .any(|e| e == "[Board] Opened family notice board [family_board_id 7]: 1 posts."));
    let world = state.format_world_state();
    assert!(
        world.contains("Open family notice board [family_board_id 7] — posts are written"),
        "{world}"
    );
    assert!(!world.contains("Open notice board [board_id 7]"), "{world}");

    state.push_event(ServerMessage::NoticePostRemoved {
        board: estate,
        post_id: 1,
    });
    assert!(state.drain_agent_events().iter().any(|e| e
        == "[Board] Removed your post [post_id 1] from family notice board [family_board_id 7]."));

    state.push_event(ServerMessage::NoticeBoardRefused {
        reason: NoticeBoardRefusal::NoFamilyRank,
    });
    assert!(state
        .drain_agent_events()
        .iter()
        .any(|e| e == "[BoardFailed] your family rank does not allow that."));
}
