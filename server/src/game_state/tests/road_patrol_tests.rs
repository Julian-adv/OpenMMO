use super::*;
use onlinerpg_shared::fence::{FenceAxis, FenceEdge};
use onlinerpg_shared::messages::{ImpoundedItem, PatrolSeizure};
use onlinerpg_shared::pathfinding::is_movement_blocked;

fn edge(x: i32) -> FenceEdge {
    FenceEdge {
        x,
        z: 1,
        axis: FenceAxis::Z,
    }
}

const ON_ROAD: [i32; 3] = [2, 3, 4];
const OFF_ROAD: i32 = 20;

async fn setup(tag: &str) -> (GameState, crate::auth::AuthService, DirectRx, DirectRx) {
    let game = make_flat_world_game_state(tag);
    let auth = make_test_auth(tag);
    let road = [[3.0, -10.0], [3.0, 20.0]];
    game.set_roads(crate::road_index::RoadIndex::from_polylines([
        road.as_slice()
    ]));
    let account = auth.login_google("Builder").unwrap();
    let bag = vec![
        bag_item(1, "land_deed", 1),
        bag_item(2, "wooden_fence", 100),
    ];
    estate_owner(&game, &auth, &account, "Builder", pos3(1.5, 5.05, 1.5), bag).await;
    claim_plot_at(&game, &auth, "Builder", 1.5, 1.5).await;
    let owner_rx = game.register_direct_channel(&pid("Builder")).await;
    create_test_character(&auth, &auth.login_google("guard").unwrap(), "Karl");
    let mut karl = make_player("Karl", 3.0, 3.0);
    karl.position.y = 5.05;
    karl.is_official_npc = true;
    game.add_player(karl).await;
    let guard_rx = game.register_direct_channel(&pid("Karl")).await;
    (game, auth, owner_rx, guard_rx)
}

async fn build(game: &GameState, auth: &crate::auth::AuthService) {
    for x in ON_ROAD.into_iter().chain([OFF_ROAD]) {
        game.edit_fence(&pid("Builder"), edge(x), true, auth, false)
            .await;
    }
}

async fn fences(game: &GameState, name: &str) -> u32 {
    bag_count(game, &pid(name), "wooden_fence").await
}

fn sweep_result(rx: &mut DirectRx) -> (Vec<String>, Vec<PatrolSeizure>) {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::PatrolSweepResult { warned, seized } => Some((warned, seized)),
            _ => None,
        })
        .expect("sweep result")
}

fn impound_state(rx: &mut DirectRx) -> (Vec<ImpoundedItem>, bool, Option<String>) {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::ImpoundState {
                items,
                reclaimed,
                error,
                ..
            } => Some((items, reclaimed, error)),
            _ => None,
        })
        .expect("impound state")
}

#[tokio::test]
async fn a_patrol_only_warns_until_enforcement_begins() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_grace").await;
    build(&game, &auth).await;
    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    assert_eq!(
        sweep_result(&mut guard_rx),
        (vec!["Builder".to_string()], vec![])
    );
    assert_eq!(auth.load_fences().unwrap().len(), 4);
}

#[tokio::test]
async fn first_offence_is_impounded_and_bought_back_at_half_price() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_first_offence").await;
    auth.set_road_patrol_enforce_from(0).unwrap();
    build(&game, &auth).await;
    assert_eq!(fences(&game, "Builder").await, 96);

    game.patrol_sweep(&pid("Builder"), 1, &auth).await;
    assert_eq!(auth.load_fences().unwrap().len(), 4);

    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    assert_eq!(
        sweep_result(&mut guard_rx).1,
        vec![PatrolSeizure {
            owner: "Builder".into(),
            fences: 3,
            forfeited: false
        }]
    );
    let left = auth.load_fences().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].edge, edge(OFF_ROAD));
    assert!(!is_movement_blocked(
        &game.passability_read(),
        2.5,
        1.5,
        3.5,
        1.5,
        0,
        Some(5.05)
    ));
    assert!(drain(&mut owner_rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::MailNotice { .. })));

    let owner = pid("Builder");
    game.player_gold.write().await.insert(owner, 149);
    game.impound_action(&owner, &pid("Karl"), false, &auth)
        .await;
    let (items, _, error) = impound_state(&mut owner_rx);
    assert_eq!(error, None);
    assert_eq!(
        items,
        vec![ImpoundedItem {
            item_def_id: "wooden_fence".into(),
            quantity: 3,
            fee: 150
        }]
    );
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert!(impound_state(&mut owner_rx).2.is_some());
    assert_eq!(fences(&game, "Builder").await, 96);
    assert_eq!(game.get_player_gold(&owner).await, 149);

    game.player_gold.write().await.insert(owner, 200);
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert_eq!(impound_state(&mut owner_rx), (vec![], true, None));
    assert_eq!(fences(&game, "Builder").await, 99);
    assert_eq!(game.get_player_gold(&owner).await, 50);
    assert_gold_consumption(
        &game,
        GoldSink::ImpoundFee {
            item_def_id: "wooden_fence".into(),
        },
        3,
        150,
    )
    .await;
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert!(impound_state(&mut owner_rx).2.is_some());
    assert_eq!(game.get_player_gold(&owner).await, 50);
}

#[tokio::test]
async fn a_second_patrol_forfeits_what_the_first_would_have_impounded() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_second_offence").await;
    auth.set_road_patrol_enforce_from(0).unwrap();
    game.edit_fence(&pid("Builder"), edge(2), true, &auth, false)
        .await;
    game.patrol_sweep(&pid("Karl"), 7, &auth).await;
    game.edit_fence(&pid("Builder"), edge(3), true, &auth, false)
        .await;
    game.patrol_sweep(&pid("Karl"), 7, &auth).await;
    drain(&mut guard_rx);
    assert_eq!(
        auth.mail_inbox(auth.resolve_character_brief("Builder").unwrap().unwrap().0)
            .unwrap()
            .len(),
        1
    );
    game.edit_fence(&pid("Builder"), edge(4), true, &auth, false)
        .await;
    game.patrol_sweep(&pid("Karl"), 8, &auth).await;
    assert_eq!(
        sweep_result(&mut guard_rx).1,
        vec![PatrolSeizure {
            owner: "Builder".into(),
            fences: 1,
            forfeited: true
        }]
    );
    assert!(auth.load_fences().unwrap().is_empty());
    game.impound_action(&pid("Builder"), &pid("Karl"), false, &auth)
        .await;
    let (items, _, _) = impound_state(&mut owner_rx);
    assert_eq!(items.iter().map(|item| item.quantity).sum::<u32>(), 2);
}

#[tokio::test]
async fn the_captain_keeps_both_standing_notices_posted_and_renews_stale_ones() {
    let (game, auth, _owner_rx, _guard_rx) = setup("guard_notices").await;
    let board = crate::game_state::guard_notices::BOARD;
    let notices = &crate::game_state::guard_notices::NOTICES;
    let count = |auth: &crate::auth::AuthService| {
        auth.notice_posts(
            board,
            crate::game_state::notice_board::notice_clock(1_000),
            0,
        )
        .unwrap()
        .len()
    };
    game.renew_guard_notices(&auth).await;
    assert_eq!(
        auth.renew_standing_notices(board, "Karl", notices, 1_000, i64::MIN)
            .unwrap(),
        0
    );
    assert_eq!(count(&auth), 2);
    assert_eq!(
        auth.renew_standing_notices(board, "Karl", notices, 1_000, i64::MAX)
            .unwrap(),
        2
    );
    assert_eq!(count(&auth), 2);
    assert_eq!(
        auth.renew_standing_notices(board, "Nobody", notices, 1_000, i64::MAX)
            .unwrap(),
        0
    );
}
