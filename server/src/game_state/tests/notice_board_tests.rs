use super::*;
use crate::auth::{AuthService, CharacterRecord};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::notice_board::{NoticeBoardRefusal, NOTICE_PAPER_ITEM};

const BOARD: u32 = 7;

fn character(auth: &AuthService, name: &str) -> CharacterRecord {
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    create_test_character(auth, &account, name)
}

async fn enter(game: &GameState, record: &CharacterRecord, x: f32) -> DirectRx {
    enter_with_paper(game, record, x, 5).await
}

async fn enter_with_paper(
    game: &GameState,
    record: &CharacterRecord,
    x: f32,
    papers: u32,
) -> DirectRx {
    let name = record.name.as_str();
    game.add_player(make_player(name, x, 0.0)).await;
    game.register_player_character(&pid(name), record.id, 0, attrs_with_cha(10), 0, None)
        .await;
    let mut inv = PlayerInventory::default();
    if papers > 0 {
        inv.bag.push(bag_item(1, NOTICE_PAPER_ITEM, papers));
    }
    game.inventories.write().await.insert(pid(name), inv);
    game.register_direct_channel(&pid(name)).await
}

async fn papers_left(game: &GameState, name: &str) -> u32 {
    game.inventories.read().await[&pid(name)]
        .bag
        .iter()
        .filter(|i| i.item_def_id == NOTICE_PAPER_ITEM)
        .map(|i| i.quantity)
        .sum()
}

fn listed(
    msgs: Vec<ServerMessage>,
) -> Option<Vec<onlinerpg_shared::notice_board::NoticePostSummary>> {
    msgs.into_iter().find_map(|m| match m {
        ServerMessage::NoticeBoard { posts, .. } => Some(posts),
        _ => None,
    })
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
    game.write_notice_post(
        &pid("alice"),
        BOARD,
        " 파티 구함 ",
        "고블린 동굴",
        None,
        Some((0, 990)),
        &auth,
    )
    .await;
    let sent = drain(&mut alice_rx);
    let written_at = sent.iter().find_map(|m| match m {
        ServerMessage::NoticePostView { post, .. } => Some(post.written_at.clone()),
        _ => None,
    });
    assert_eq!(written_at, Some(stamp));
    let (post_id, title, comments) = last_post(&sent).unwrap();
    assert_eq!((title.as_str(), comments.len()), ("파티 구함", 0));
    assert_eq!(papers_left(&game, "alice").await, 4);

    game.open_notice_board(&pid("bob"), BOARD, &auth).await;
    let listed = listed(drain(&mut bob_rx)).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].author.as_str(), listed[0].comment_count),
        ("alice", 0)
    );
    assert_eq!(
        (listed[0].x, listed[0].y),
        onlinerpg_shared::notice_board::clamp_paper_position(0, 990)
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

    game.write_notice_post(&pid("near"), BOARD, "  ", "body", None, None, &auth)
        .await;
    game.write_notice_post(
        &pid("near"),
        BOARD,
        "t",
        &"x".repeat(2001),
        None,
        None,
        &auth,
    )
    .await;
    game.write_notice_comment(&pid("near"), BOARD, 999, "hi", &auth)
        .await;
    game.write_notice_post(&pid("near"), BOARD, "t", "b", None, None, &auth)
        .await;
    game.write_notice_post(&pid("near"), BOARD, "t2", "b2", None, None, &auth)
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
        auth.notice_posts(
            crate::auth::BoardKey {
                rx: 0,
                rz: 0,
                object_id: BOARD
            },
            super::super::notice_board::notice_clock(game.current_total_game_seconds()),
            0
        )
        .unwrap()
        .len(),
        1
    );
}

#[tokio::test]
async fn posting_needs_paper_and_a_stored_picture() {
    let auth = make_test_auth("notice_board_paper");
    let game = board_game("notice_board_paper");
    let poor = character(&auth, "poor");
    let mut rx = enter_with_paper(&game, &poor, 10.5, 0).await;

    game.write_notice_post(&pid("poor"), BOARD, "t", "b", None, None, &auth)
        .await;
    game.write_notice_post(
        &pid("poor"),
        BOARD,
        "t",
        "",
        Some(&"a".repeat(64)),
        None,
        &auth,
    )
    .await;
    assert_eq!(
        refusals(&drain(&mut rx)),
        [NoticeBoardRefusal::NoPaper, NoticeBoardRefusal::BadImage]
    );
}

#[tokio::test]
async fn peeking_shows_papers_and_the_registrar_takes_down_expired_ones() {
    let auth = make_test_auth("notice_board_expiry");
    let game = board_game("notice_board_expiry");
    let writer = character(&auth, "writer");
    let mut writer_rx = enter(&game, &writer, 10.5).await;
    let start = onlinerpg_shared::world::GameDateTime {
        year: 1000,
        month: 1,
        day: 1,
        hour: 8,
        minute: 0,
    };
    game.debug_set_datetime(&start);
    game.write_notice_post(&pid("writer"), BOARD, "old", "b", None, None, &auth)
        .await;

    game.peek_notice_board(&pid("writer"), BOARD, &auth).await;
    let papers = drain(&mut writer_rx).into_iter().find_map(|m| match m {
        ServerMessage::NoticeBoardPapers { papers, .. } => Some(papers),
        _ => None,
    });
    assert_eq!(papers.map(|p| (p.len(), p[0].expired)), Some((1, false)));

    let registrar_record = character(&auth, "aldwin");
    let mut npc = make_player("npc_steward", 10.5, 0.0);
    npc.name = "Aldwin".into();
    npc.is_official_npc = true;
    game.add_player(npc).await;
    game.register_player_character(
        &pid("npc_steward"),
        registrar_record.id,
        0,
        attrs_with_cha(10),
        0,
        None,
    )
    .await;
    let mut npc_rx = game.register_direct_channel(&pid("npc_steward")).await;

    game.take_down_notices(&pid("npc_steward"), BOARD, &auth)
        .await;
    game.take_down_notices(&pid("writer"), BOARD, &auth).await;
    assert_eq!(
        refusals(&drain(&mut npc_rx)),
        [NoticeBoardRefusal::NothingExpired]
    );
    assert_eq!(
        refusals(&drain(&mut writer_rx)),
        [NoticeBoardRefusal::NotRegistrar]
    );

    let later = onlinerpg_shared::world::GameDateTime {
        month: 2,
        day: 1,
        hour: 12,
        ..start
    };
    game.debug_set_datetime(&later);
    game.open_notice_board(&pid("writer"), BOARD, &auth).await;
    let posts = listed(drain(&mut writer_rx)).unwrap();
    assert!(posts[0].expired);

    game.take_down_notices(&pid("npc_steward"), BOARD, &auth)
        .await;
    let sent = drain(&mut npc_rx);
    assert!(sent
        .iter()
        .any(|m| matches!(m, ServerMessage::NoticesTakenDown { count: 1, .. })));
    assert_eq!(listed(sent).map(|p| p.len()), Some(0));
}

#[tokio::test]
async fn posts_past_the_grace_period_are_hidden_without_a_take_down() {
    let auth = make_test_auth("notice_board_grace");
    let game = board_game("notice_board_grace");
    let writer = character(&auth, "writer");
    let mut rx = enter(&game, &writer, 10.5).await;
    let start = onlinerpg_shared::world::GameDateTime {
        year: 1000,
        month: 1,
        day: 1,
        hour: 8,
        minute: 0,
    };
    game.debug_set_datetime(&start);
    game.write_notice_post(&pid("writer"), BOARD, "old", "b", None, None, &auth)
        .await;
    game.debug_set_datetime(&onlinerpg_shared::world::GameDateTime {
        month: 2,
        day: 2,
        hour: 9,
        ..start
    });
    game.open_notice_board(&pid("writer"), BOARD, &auth).await;
    assert_eq!(listed(drain(&mut rx)).map(|p| p.len()), Some(0));
}

#[tokio::test]
async fn papers_may_barely_cover_others_and_writers_get_three_each() {
    let auth = make_test_auth("notice_board_cover");
    let game = board_game("notice_board_cover");
    let (alice, bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let mut alice_rx = enter(&game, &alice, 10.5).await;
    let mut bob_rx = enter(&game, &bob, 9.5).await;
    let write = |who: &'static str, title: &'static str, at: Option<(u16, u16)>| {
        let game = &game;
        let auth = &auth;
        async move {
            game.notice_cooldowns.lock().unwrap().clear();
            game.write_notice_post(&pid(who), BOARD, title, "b", None, at, auth)
                .await;
        }
    };

    write("alice", "a1", Some((500, 500))).await;
    write("bob", "on top", Some((520, 520))).await;
    write("bob", "beside", Some((700, 500))).await;
    write("alice", "own", Some((520, 520))).await;
    write("alice", "a3", None).await;
    write("alice", "a4", None).await;
    assert_eq!(
        refusals(&drain(&mut bob_rx)),
        [NoticeBoardRefusal::CoversOthers]
    );
    assert_eq!(
        refusals(&drain(&mut alice_rx)),
        [NoticeBoardRefusal::TooManyPosts]
    );
    assert_eq!(papers_left(&game, "bob").await, 4);
}

#[tokio::test]
async fn only_the_writer_removes_a_post_and_the_paper_stays_spent() {
    let auth = make_test_auth("notice_board_remove");
    let game = board_game("notice_board_remove");
    let (alice, bob) = (character(&auth, "alice"), character(&auth, "bob"));
    let mut alice_rx = enter(&game, &alice, 10.5).await;
    let mut bob_rx = enter(&game, &bob, 9.5).await;
    for title in ["a1", "a2", "a3"] {
        game.notice_cooldowns.lock().unwrap().clear();
        game.write_notice_post(&pid("alice"), BOARD, title, "b", None, None, &auth)
            .await;
    }
    let (post_id, ..) = last_post(&drain(&mut alice_rx)).unwrap();
    game.write_notice_comment(&pid("bob"), BOARD, post_id, "hi", &auth)
        .await;
    game.peek_notice_board(&pid("bob"), BOARD, &auth).await;
    drain(&mut bob_rx);

    game.remove_notice_post(&pid("bob"), BOARD, post_id, &auth)
        .await;
    game.remove_notice_post(&pid("bob"), BOARD, 9999, &auth)
        .await;
    assert_eq!(
        refusals(&drain(&mut bob_rx)),
        [
            NoticeBoardRefusal::NotYours,
            NoticeBoardRefusal::PostMissing
        ]
    );

    game.remove_notice_post(&pid("alice"), BOARD, post_id, &auth)
        .await;
    let sent = drain(&mut alice_rx);
    assert!(sent.iter().any(
        |m| matches!(m, ServerMessage::NoticePostRemoved { post_id: id, .. } if *id == post_id)
    ));
    let posts = listed(sent).unwrap();
    assert_eq!(posts.len(), 2);
    assert!(posts.iter().all(|p| p.id != post_id));
    assert_eq!(papers_left(&game, "alice").await, 2);

    game.peek_notice_board(&pid("bob"), BOARD, &auth).await;
    game.read_notice_post(&pid("bob"), BOARD, post_id, &auth)
        .await;
    game.remove_notice_post(&pid("alice"), BOARD, post_id, &auth)
        .await;
    let seen = drain(&mut bob_rx);
    assert!(seen.iter().any(
        |m| matches!(m, ServerMessage::NoticeBoardPapers { papers, .. } if papers.len() == 2)
    ));
    assert_eq!(refusals(&seen), [NoticeBoardRefusal::PostMissing]);
    assert_eq!(
        refusals(&drain(&mut alice_rx)),
        [NoticeBoardRefusal::PostMissing]
    );

    game.notice_cooldowns.lock().unwrap().clear();
    game.write_notice_post(&pid("alice"), BOARD, "a4", "b", None, None, &auth)
        .await;
    assert!(refusals(&drain(&mut alice_rx)).is_empty());
}
