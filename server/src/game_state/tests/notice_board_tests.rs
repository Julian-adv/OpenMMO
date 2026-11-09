use super::*;
use crate::auth::{AuthService, CharacterRecord};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::notice_board::NoticeBoardRefusal;

const BOARD: u32 = 7;

fn character(auth: &AuthService, name: &str) -> CharacterRecord {
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    create_test_character(auth, &account, name)
}

async fn enter(game: &GameState, record: &CharacterRecord, x: f32) -> DirectRx {
    let name = record.name.as_str();
    game.add_player(make_player(name, x, 0.0)).await;
    game.register_player_character(&pid(name), record.id, 0, attrs_with_cha(10), 0, None)
        .await;
    game.register_direct_channel(&pid(name)).await
}

fn board_game(name: &str) -> GameState {
    let game = make_test_game_state(name);
    game.sync_region_furniture(
        0,
        0,
        &[FurniturePlacement {
            id: BOARD,
            type_id: "notice_board".into(),
            x: 10.0,
            y: 0.0,
            z: 0.0,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    game
}

fn refusals(msgs: &[ServerMessage]) -> Vec<NoticeBoardRefusal> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::NoticeBoardRefused { reason } => Some(*reason),
            _ => None,
        })
        .collect()
}

fn last_post(msgs: &[ServerMessage]) -> Option<(i64, String, Vec<String>)> {
    msgs.iter().rev().find_map(|m| match m {
        ServerMessage::NoticePostView { post, comments, .. } => Some((
            post.id,
            post.title.clone(),
            comments
                .iter()
                .map(|c| format!("{}: {}", c.author, c.body))
                .collect(),
        )),
        _ => None,
    })
}

#[tokio::test]
async fn players_post_read_and_comment_at_a_board() {
    let auth = make_test_auth("notice_board_flow");
    let game = board_game("notice_board_flow");
    let (alice, bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let mut alice_rx = enter(&game, &alice, 10.5).await;
    let mut bob_rx = enter(&game, &bob, 9.5).await;

    let stamp = onlinerpg_shared::world::GameDateTime {
        year: 1000,
        month: 3,
        day: 14,
        hour: 9,
        minute: 30,
    };
    game.debug_set_datetime(&stamp);
    game.write_notice_post(&pid("alice"), BOARD, " 파티 구함 ", "고블린 동굴", &auth)
        .await;
    let sent = drain(&mut alice_rx);
    let written_at = sent.iter().find_map(|m| match m {
        ServerMessage::NoticePostView { post, .. } => Some(post.written_at.clone()),
        _ => None,
    });
    assert_eq!(written_at, Some(stamp));
    let (post_id, title, comments) = last_post(&sent).unwrap();
    assert_eq!((title.as_str(), comments.len()), ("파티 구함", 0));

    game.open_notice_board(&pid("bob"), BOARD, &auth).await;
    let listed = drain(&mut bob_rx).into_iter().find_map(|m| match m {
        ServerMessage::NoticeBoard { posts, .. } => Some(posts),
        _ => None,
    });
    let listed = listed.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].author.as_str(), listed[0].comment_count),
        ("alice", 0)
    );

    game.write_notice_comment(&pid("bob"), BOARD, post_id, "저요", &auth)
        .await;
    let (_, _, comments) = last_post(&drain(&mut bob_rx)).unwrap();
    assert_eq!(comments, ["bob: 저요"]);

    game.read_notice_post(&pid("alice"), BOARD, post_id, &auth)
        .await;
    assert_eq!(last_post(&drain(&mut alice_rx)).unwrap().2, ["bob: 저요"]);
}

#[tokio::test]
async fn boards_refuse_distant_empty_long_rapid_and_unknown_writes() {
    let auth = make_test_auth("notice_board_refusals");
    let game = board_game("notice_board_refusals");
    let (near, far) = (character(&auth, "near"), character(&auth, "far"));
    let mut near_rx = enter(&game, &near, 10.5).await;
    let mut far_rx = enter(&game, &far, 30.0).await;

    game.open_notice_board(&pid("far"), BOARD, &auth).await;
    game.open_notice_board(&pid("near"), BOARD + 1, &auth).await;
    assert_eq!(
        refusals(&drain(&mut far_rx)),
        [NoticeBoardRefusal::OutOfReach]
    );

    game.write_notice_post(&pid("near"), BOARD, "  ", "body", &auth)
        .await;
    game.write_notice_post(&pid("near"), BOARD, "t", &"x".repeat(2001), &auth)
        .await;
    game.write_notice_comment(&pid("near"), BOARD, 999, "hi", &auth)
        .await;
    game.write_notice_post(&pid("near"), BOARD, "t", "b", &auth)
        .await;
    game.write_notice_post(&pid("near"), BOARD, "t2", "b2", &auth)
        .await;
    assert_eq!(
        refusals(&drain(&mut near_rx)),
        [
            NoticeBoardRefusal::OutOfReach,
            NoticeBoardRefusal::Empty,
            NoticeBoardRefusal::TooLong,
            NoticeBoardRefusal::PostMissing,
            NoticeBoardRefusal::TooFast,
        ]
    );
    assert_eq!(
        auth.notice_posts(crate::auth::BoardKey {
            rx: 0,
            rz: 0,
            object_id: BOARD
        })
        .unwrap()
        .len(),
        1
    );
}
