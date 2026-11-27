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
    let (game, auth, owner_rx, guard_rx, _) = setup_with_db(tag).await;
    (game, auth, owner_rx, guard_rx)
}

async fn setup_with_db(
    tag: &str,
) -> (
    GameState,
    crate::auth::AuthService,
    DirectRx,
    DirectRx,
    std::path::PathBuf,
) {
    let game = make_flat_world_game_state(tag);
    let (auth, db) = make_test_auth_with_path(tag);
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
    (game, auth, owner_rx, guard_rx, db)
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

fn sweep_result(rx: &mut DirectRx) -> Vec<PatrolSeizure> {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::PatrolSweepResult { seized } => Some(seized),
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
async fn first_offence_is_impounded_and_bought_back_at_half_price() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_first_offence").await;
    build(&game, &auth).await;
    assert_eq!(fences(&game, "Builder").await, 96);

    game.patrol_sweep(&pid("Builder"), 1, &auth).await;
    assert_eq!(auth.load_fences().unwrap().len(), 4);

    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    assert_eq!(
        sweep_result(&mut guard_rx),
        vec![PatrolSeizure {
            owner: "Builder".into(),
            fences: 3,
            forfeited: false,
            furniture: 0,
            houses: 0,
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
            fee: 150,
            enchant: 0,
        }]
    );
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    let (left, reclaimed, error) = impound_state(&mut owner_rx);
    assert_eq!(
        (left.len(), left[0].quantity, reclaimed, error),
        (1, 1, false, None)
    );
    assert_eq!(fences(&game, "Builder").await, 98);
    assert_eq!(game.get_player_gold(&owner).await, 49);

    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert!(impound_state(&mut owner_rx).2.is_some());
    game.player_gold.write().await.insert(owner, 100);
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
        sweep_result(&mut guard_rx),
        vec![PatrolSeizure {
            owner: "Builder".into(),
            fences: 1,
            forfeited: true,
            furniture: 0,
            houses: 0,
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

#[tokio::test]
async fn only_the_captain_opens_the_reclaim_window_and_only_for_someone_close() {
    let (game, _auth, mut owner_rx, mut guard_rx) = setup("offer_impound").await;
    let (owner, karl) = (pid("Builder"), pid("Karl"));
    let offered = |rx: &mut DirectRx| {
        drain(rx)
            .iter()
            .any(|message| matches!(message, ServerMessage::ImpoundOffered { .. }))
    };

    game.offer_impound(&karl, &owner).await;
    assert!(offered(&mut owner_rx));

    game.offer_impound(&owner, &karl).await;
    assert!(!offered(&mut guard_rx));

    game.players
        .write()
        .await
        .get_mut(&owner)
        .unwrap()
        .position
        .x = 60.0;
    game.offer_impound(&karl, &owner).await;
    assert!(!offered(&mut owner_rx));
    assert!(drain(&mut guard_rx).iter().any(|message| matches!(
        message,
        ServerMessage::SystemMessage { message, .. } if message.starts_with("[Impound]")
    )));
}

#[tokio::test]
async fn a_scout_names_the_nearest_road_fence() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_scout").await;
    build(&game, &auth).await;
    let scouted = |rx: &mut DirectRx| {
        drain(rx)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::PatrolScouted { target } => Some(target),
                _ => None,
            })
            .expect("scout answer")
    };

    game.patrol_scout(&pid("Karl")).await;
    let target = scouted(&mut guard_rx).expect("a fence stands on the road");
    let karl = game.players.read().await[&pid("Karl")].position;
    assert!(karl.dist_xz_sq(&target) <= 4.0, "{target:?}");
    assert_eq!(auth.load_fences().unwrap().len(), 4);

    game.patrol_strike(&pid("Builder")).await;
    game.patrol_strike(&pid("Karl")).await;
    let swings = drain(&mut owner_rx)
        .into_iter()
        .filter(|message| matches!(message, ServerMessage::PlayerSwung { .. }))
        .count();
    assert_eq!(swings, 1);
}

fn ring(min: (i32, i32), max: (i32, i32)) -> Vec<FenceEdge> {
    let mut edges = Vec::new();
    for x in min.0..=max.0 {
        for z in [min.1, max.1 + 1] {
            edges.push(FenceEdge {
                x,
                z,
                axis: FenceAxis::X,
            });
        }
    }
    for z in min.1..=max.1 {
        for x in [min.0, max.0 + 1] {
            edges.push(FenceEdge {
                x,
                z,
                axis: FenceAxis::Z,
            });
        }
    }
    edges
}

fn break_out_result(rx: &mut DirectRx) -> (Vec<PatrolSeizure>, bool) {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::PatrolBrokeOut { seized, recalled } => Some((seized, recalled)),
            _ => None,
        })
        .expect("break-out result")
}

async fn stand_guard_at(game: &GameState, x: f32, z: f32) {
    game.teleport_player(&pid("Karl"), pos3(x, 5.05, z), 0.0, 0)
        .await;
}

#[tokio::test]
async fn a_penned_guard_forfeits_every_ring_around_it_without_warning() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_break_out").await;
    let pen: Vec<FenceEdge> = ring((20, 1), (20, 1))
        .into_iter()
        .chain(ring((19, 0), (21, 2)))
        .collect();
    for edge in pen.iter().copied().chain([edge(30)]) {
        game.edit_fence(&pid("Builder"), edge, true, &auth, false)
            .await;
    }
    assert_eq!(auth.load_fences().unwrap().len(), 17);
    stand_guard_at(&game, 20.5, 1.5).await;
    drain(&mut owner_rx);

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    assert_eq!(
        break_out_result(&mut guard_rx),
        (
            vec![PatrolSeizure {
                owner: "Builder".into(),
                fences: 16,
                forfeited: true,
                furniture: 0,
                houses: 0,
            }],
            false
        )
    );
    let left = auth.load_fences().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].edge, edge(30));
    assert!(!is_movement_blocked(
        &game.passability_read(),
        20.5,
        1.5,
        17.5,
        1.5,
        0,
        Some(5.05)
    ));
    let owner_id = game.player_characters.read().await[&pid("Builder")].0;
    assert!(auth.impounded_items(owner_id).unwrap().is_empty());
    assert_eq!(fences(&game, "Builder").await, 83);
    let mail = drain(&mut owner_rx)
        .iter()
        .filter(|message| matches!(message, ServerMessage::MailNotice { .. }))
        .count();
    assert_eq!(mail, 1);
    assert_eq!(player_xz(&game, &pid("Karl")).await, (20.5, 1.5));
}

#[tokio::test]
async fn a_guard_with_a_way_to_the_road_takes_no_fences() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_break_out_free").await;
    let mut open_pen = ring((20, 1), (20, 1));
    open_pen.pop();
    for edge in open_pen {
        game.edit_fence(&pid("Builder"), edge, true, &auth, false)
            .await;
    }
    stand_guard_at(&game, 20.5, 1.5).await;

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    assert_eq!(break_out_result(&mut guard_rx), (vec![], false));
    assert_eq!(auth.load_fences().unwrap().len(), 3);

    game.patrol_break_out(&pid("Builder"), true, &auth).await;
    assert_eq!(player_xz(&game, &pid("Builder")).await, (1.5, 1.5));
}

#[tokio::test]
async fn a_guard_asking_for_recall_is_sent_back_to_town() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_recall").await;
    stand_guard_at(&game, 20.5, 1.5).await;

    game.patrol_break_out(&pid("Karl"), true, &auth).await;
    assert_eq!(break_out_result(&mut guard_rx), (vec![], true));
    let spawn = crate::world_config::world_config()
        .spawn_position
        .position();
    assert_eq!(player_xz(&game, &pid("Karl")).await, (spawn.x, spawn.z));
}

#[tokio::test]
async fn a_guard_penned_by_what_it_cannot_take_down_is_sent_back_to_town() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_break_out_stuck").await;
    for edge in ring((60, 1), (60, 1)) {
        game.edit_fence(&pid("Builder"), edge, true, &auth, true)
            .await;
    }
    assert_eq!(auth.load_fences().unwrap().len(), 4);
    stand_guard_at(&game, 60.5, 1.5).await;

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    assert_eq!(break_out_result(&mut guard_rx), (vec![], true));
    assert_eq!(auth.load_fences().unwrap().len(), 4);
    let spawn = crate::world_config::world_config()
        .spawn_position
        .position();
    assert_eq!(player_xz(&game, &pid("Karl")).await, (spawn.x, spawn.z));
}

#[tokio::test]
async fn a_guard_on_furniture_is_left_alone_even_inside_a_ring_of_fences() {
    use onlinerpg_shared::pathfinding::{build_furniture_passability, FurniturePiece};
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_break_out_seated").await;
    for edge in ring((20, 1), (20, 1)) {
        game.edit_fence(&pid("Builder"), edge, true, &auth, false)
            .await;
    }
    let chair = build_furniture_passability(&[FurniturePiece {
        cells: vec![(20, 1)],
        floor_level: 0,
        y_base: 5.05,
        wall_height: onlinerpg_shared::furniture::FURNITURE_BLOCK_HEIGHT,
    }])
    .unwrap();
    game.passability_write()
        .insert(onlinerpg_shared::furniture::region_cache_key(0, 0), chair);
    stand_guard_at(&game, 20.5, 1.5).await;

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    assert_eq!(break_out_result(&mut guard_rx), (vec![], false));
    assert_eq!(auth.load_fences().unwrap().len(), 4);
    assert_eq!(player_xz(&game, &pid("Karl")).await, (20.5, 1.5));
}

async fn place_crate(game: &GameState, auth: &crate::auth::AuthService, instance: u64, x: f32) {
    let id = pid("Builder");
    let position = pos3(x, 5.05, 6.0);
    game.players.write().await.get_mut(&id).unwrap().position = position;
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(instance, "furniture_crate", 1));
    game.place_estate_chest(&id, instance, position, 0.0, 0, auth)
        .await;
}

async fn road_house(game: &GameState, id: &str, owner: i64, x: f32) {
    let house = serde_json::from_value(serde_json::json!({
        "id": id, "ownerId": owner.to_string(), "sourceScrollId": "scroll_of_small_house",
        "origin": {"x": x, "y": 5.0, "z": 10.0},
        "rooms": [{
            "localX": 0, "localZ": 0, "sizeX": 2, "sizeZ": 2,
            "floorLevel": 0, "floorTexture": 0, "roofTexture": 0, "wallHeight": 3.0,
            "wallNorth": [], "wallSouth": [], "wallEast": [], "wallWest": []
        }],
        "passability": [{
            "floorLevel": 0, "originX": 0, "originZ": 0, "width": 2, "depth": 2,
            "cells": vec![0; 4]
        }]
    }))
    .unwrap();
    game.housing_io.write_house(&house).await.unwrap();
    game.passability_add_house(&house).await;
}

#[tokio::test]
async fn road_furniture_is_impounded_once_without_a_grace_period_then_forfeited() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_furniture").await;
    place_crate(&game, &auth, 50, 3.0).await;
    place_crate(&game, &auth, 51, 15.0).await;
    assert_eq!(auth.load_estate_chests().unwrap().len(), 2);
    drain(&mut owner_rx);

    game.patrol_scout(&pid("Karl")).await;
    let scouted = drain(&mut guard_rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::PatrolScouted { target } => target,
            _ => None,
        });
    assert_eq!(scouted.map(|at| (at.x, at.z)), Some((3.0, 6.0)));

    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    let seized = sweep_result(&mut guard_rx);
    assert_eq!(
        seized,
        vec![PatrolSeizure {
            owner: "Builder".to_string(),
            fences: 0,
            forfeited: false,
            furniture: 1,
            houses: 0,
        }]
    );
    let left = auth.load_estate_chests().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].position.x, 15.0);
    assert_eq!(
        bag_count(&game, &pid("Builder"), "furniture_crate").await,
        0
    );
    assert!(drain(&mut owner_rx).iter().any(|message| matches!(
        message,
        ServerMessage::EstateChestVisibility { removed, .. } if removed.len() == 1
    )));

    let owner = pid("Builder");
    let beside_karl = pos3(3.5, 5.05, 3.5);
    game.players.write().await.get_mut(&owner).unwrap().position = beside_karl;
    game.player_gold.write().await.insert(owner, 1_000);
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert_eq!(impound_state(&mut owner_rx), (vec![], true, None));
    assert_eq!(bag_count(&game, &owner, "furniture_crate").await, 1);
    assert_eq!(game.get_player_gold(&owner).await, 750);

    place_crate(&game, &auth, 52, 3.0).await;
    game.patrol_sweep(&pid("Karl"), 2, &auth).await;
    let seized = sweep_result(&mut guard_rx);
    assert_eq!((seized[0].furniture, seized[0].forfeited), (1, true));
    drain(&mut owner_rx);
    game.players.write().await.get_mut(&owner).unwrap().position = beside_karl;
    game.impound_action(&owner, &pid("Karl"), false, &auth)
        .await;
    assert_eq!(impound_state(&mut owner_rx), (vec![], false, None));
}

#[tokio::test]
async fn a_house_on_the_road_is_pulled_down_and_one_beside_it_stands() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_house").await;
    let owner = game.character_id_of(&pid("Builder")).await.unwrap();
    road_house(&game, "road_house", owner, 2.0).await;
    road_house(&game, "side_house", owner, 12.0).await;
    road_house(&game, "town_house", 0, 2.0).await;

    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    let seized = sweep_result(&mut guard_rx);
    assert_eq!(seized.len(), 1);
    assert_eq!((seized[0].houses, seized[0].furniture), (1, 0));
    let standing: Vec<String> = game
        .housing_io
        .read_all_houses()
        .await
        .unwrap()
        .into_iter()
        .map(|house| house.id)
        .collect();
    assert!(!standing.contains(&"road_house".to_string()));
    assert!(standing.contains(&"side_house".to_string()));
    assert!(standing.contains(&"town_house".to_string()));
}

#[tokio::test]
async fn a_first_offence_returns_the_house_scroll_and_what_a_chest_held() {
    let (game, auth, mut owner_rx, mut guard_rx, db) = setup_with_db("patrol_chest").await;
    let owner = pid("Builder");
    let character = game.character_id_of(&owner).await.unwrap();
    road_house(&game, "road_house", character, 2.0).await;
    let at = pos3(3.0, 5.05, 6.0);
    game.players.write().await.get_mut(&owner).unwrap().position = at;
    game.inventories
        .write()
        .await
        .get_mut(&owner)
        .unwrap()
        .bag
        .push(bag_item(60, "storage_chest", 1));
    game.place_estate_chest(&owner, 60, at, 0.0, 0, &auth).await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    rusqlite::Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO estate_chest_items(chest_id,item_def_id,quantity,enchant,locked,item_id)
             VALUES (?1,'iron_sword',1,3,1,901), (?1,'iron_sword',1,3,1,902),
                    (?1,'apple',5,0,0,NULL)",
            [chest.id],
        )
        .unwrap();

    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    let seized = sweep_result(&mut guard_rx);
    assert_eq!(
        (seized[0].furniture, seized[0].houses, seized[0].forfeited),
        (1, 1, false)
    );
    assert!(auth.load_estate_chests().unwrap().is_empty());

    game.players.write().await.get_mut(&owner).unwrap().position = pos3(3.5, 5.05, 3.5);
    game.player_gold.write().await.insert(owner, 5_600);
    drain(&mut owner_rx);
    game.impound_action(&owner, &pid("Karl"), false, &auth)
        .await;
    let (items, _, _) = impound_state(&mut owner_rx);
    let fees: Vec<(&str, i64)> = items
        .iter()
        .map(|item| (item.item_def_id.as_str(), item.fee))
        .collect();
    assert_eq!(
        fees,
        [
            ("storage_chest", 600),
            ("scroll_of_small_house", 5_000),
            ("iron_sword", 0),
            ("iron_sword", 0),
            ("apple", 0)
        ]
    );
    game.impound_action(&owner, &pid("Karl"), true, &auth).await;
    assert_eq!(impound_state(&mut owner_rx), (vec![], true, None));
    assert_eq!(game.get_player_gold(&owner).await, 0);
    let inventories = game.inventories.read().await;
    let bag = &inventories[&owner].bag;
    let swords: Vec<_> = bag
        .iter()
        .filter(|item| item.item_def_id == "iron_sword")
        .map(|item| (item.instance_id, item.enchant, item.locked))
        .collect();
    assert_eq!(
        swords,
        [(901, 3, true), (902, 3, true)],
        "ids kept, not merged"
    );
    let returned_chest = bag
        .iter()
        .find(|item| item.item_def_id == "storage_chest")
        .unwrap();
    assert_eq!(returned_chest.instance_id, 60);
    for (item, quantity) in [
        ("storage_chest", 1),
        ("scroll_of_small_house", 1),
        ("apple", 5),
    ] {
        let held: u32 = bag
            .iter()
            .filter(|entry| entry.item_def_id == item)
            .map(|entry| entry.quantity)
            .sum();
        assert_eq!(held, quantity, "{item}");
    }
}

async fn drop_crate(
    game: &GameState,
    auth: &crate::auth::AuthService,
    instance: u64,
    x: f32,
    z: f32,
) {
    let owner = pid("Builder");
    let at = pos3(x, 5.05, z);
    game.players.write().await.get_mut(&owner).unwrap().position = at;
    game.inventories
        .write()
        .await
        .get_mut(&owner)
        .unwrap()
        .bag
        .push(bag_item(instance, "furniture_crate", 1));
    game.place_estate_chest(&owner, instance, at, 0.0, 0, auth)
        .await;
}

#[tokio::test]
async fn a_guard_penned_by_furniture_clears_it_and_one_with_a_way_out_does_not() {
    let (game, auth, _owner_rx, mut guard_rx) = setup("patrol_furniture_pen").await;
    let ring = [
        (20.3, 8.5),
        (23.7, 8.5),
        (21.4, 7.3),
        (22.6, 7.3),
        (21.4, 9.7),
    ];
    for (instance, (x, z)) in (70..).zip(ring) {
        drop_crate(&game, &auth, instance, x, z).await;
    }
    assert_eq!(auth.load_estate_chests().unwrap().len(), 5);
    stand_guard_at(&game, 21.5, 8.5).await;

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    assert_eq!(break_out_result(&mut guard_rx), (vec![], false));
    assert_eq!(auth.load_estate_chests().unwrap().len(), 5);

    drop_crate(&game, &auth, 80, 22.6, 9.7).await;
    assert_eq!(auth.load_estate_chests().unwrap().len(), 6);

    game.patrol_break_out(&pid("Karl"), false, &auth).await;
    let (seized, recalled) = break_out_result(&mut guard_rx);
    assert_eq!((seized.len(), recalled), (1, false));
    assert_eq!((seized[0].furniture, seized[0].forfeited), (6, true));
    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert_eq!(player_xz(&game, &pid("Karl")).await, (21.5, 8.5));
}

#[tokio::test]
async fn an_impound_left_a_game_week_is_forfeited() {
    let (game, auth, mut owner_rx, _guard_rx) = setup("patrol_impound_expiry").await;
    build(&game, &auth).await;
    game.patrol_sweep(&pid("Karl"), 1, &auth).await;
    let owner = pid("Builder");
    game.players.write().await.get_mut(&owner).unwrap().position = pos3(3.5, 5.05, 3.5);
    drain(&mut owner_rx);

    auth.age_impounds(crate::auth::IMPOUND_HOLD_SECONDS - 60)
        .unwrap();
    game.impound_action(&owner, &pid("Karl"), false, &auth)
        .await;
    assert_eq!(impound_state(&mut owner_rx).0.len(), 1);

    auth.age_impounds(60).unwrap();
    game.impound_action(&owner, &pid("Karl"), false, &auth)
        .await;
    assert_eq!(impound_state(&mut owner_rx), (vec![], false, None));
}

fn shovelled(rx: &mut DirectRx) -> bool {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::PatrolShovelled { cleared } => Some(cleared),
            _ => None,
        })
        .expect("shovel answer")
}

fn clear_stretches(game: &GameState) -> usize {
    match game.road_snow_message() {
        ServerMessage::RoadSnowCleared {
            reset: true,
            stretches,
        } => stretches.len(),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn a_guard_shovels_lying_snow_off_the_road_once_per_stretch() {
    let (game, auth, mut owner_rx, mut guard_rx) = setup("patrol_snow").await;
    game.set_weather(empty_weather(0.5));

    game.patrol_shovel(&pid("Karl")).await;
    assert!(!shovelled(&mut guard_rx));
    assert_eq!(clear_stretches(&game), 0);

    game.weather_command(&pid("Karl"), "snow").unwrap();
    drain(&mut owner_rx);
    game.patrol_shovel(&pid("Karl")).await;
    assert!(shovelled(&mut guard_rx));
    assert_eq!(clear_stretches(&game), 1);
    assert!(drain(&mut owner_rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::PlayerShovelled { .. })));

    game.patrol_shovel(&pid("Karl")).await;
    assert!(!shovelled(&mut guard_rx));
    assert_eq!(clear_stretches(&game), 1);

    game.patrol_shovel(&pid("Builder")).await;
    assert_eq!(clear_stretches(&game), 1);

    create_test_character(&auth, &auth.login_google("guard2").unwrap(), "Brenna");
    let mut brenna = make_player("Brenna", 3.0, 4.0);
    brenna.position.y = 5.05;
    brenna.is_official_npc = true;
    game.add_player(brenna).await;
    let mut partner_rx = game.register_direct_channel(&pid("Brenna")).await;
    game.patrol_shovel(&pid("Brenna")).await;
    assert!(shovelled(&mut partner_rx), "the partner digs in beside her");
    game.patrol_shovel(&pid("Brenna")).await;
    assert!(!shovelled(&mut partner_rx));
    assert_eq!(clear_stretches(&game), 1);
}

#[tokio::test]
async fn shovelling_along_a_long_road_leaves_no_snowy_gap() {
    let game = make_flat_world_game_state("patrol_snow_long");
    let points: Vec<[f32; 2]> = (0..=40).map(|i| [3.0, i as f32 * 8.0]).collect();
    game.set_roads(crate::road_index::RoadIndex::from_polylines([
        points.as_slice()
    ]));
    game.set_weather(empty_weather(0.5));
    let mut karl = make_player("Karl", 3.0, 0.0);
    karl.is_official_npc = true;
    game.add_player(karl).await;
    game.weather_command(&pid("Karl"), "snow").unwrap();

    let mut z = 0.0;
    while z <= 300.0 {
        game.players
            .write()
            .await
            .get_mut(&pid("Karl"))
            .unwrap()
            .position = pos3(3.0, 0.0, z);
        game.patrol_shovel(&pid("Karl")).await;
        z += 10.0;
    }
    // The way back skips every stop already cleared.
    let mut back = 300.0;
    while back >= 0.0 {
        game.players
            .write()
            .await
            .get_mut(&pid("Karl"))
            .unwrap()
            .position = pos3(4.5, 0.0, back);
        game.patrol_shovel(&pid("Karl")).await;
        back -= 10.0;
    }
    let ServerMessage::RoadSnowCleared { stretches, .. } = game.road_snow_message() else {
        panic!("road snow message");
    };
    let mut z = 0.0;
    while z <= 300.0 {
        let clear = stretches.iter().flat_map(|s| &s.segments).any(|segment| {
            onlinerpg_shared::worldgen::vector_features::point_segment_distance(3.0, z, segment)
                < 0.01
        });
        assert!(clear, "snow left on the road at z={z}");
        z += 1.0;
    }
    assert_eq!(stretches.len(), 31, "one bout per ten metres");
    let longest = stretches
        .iter()
        .flat_map(|s| &s.segments)
        .map(|[ax, az, bx, bz]| (bx - ax).hypot(bz - az))
        .fold(0.0, f32::max);
    assert!(longest <= 8.0, "a bout clears about six metres each way");
}

#[tokio::test]
async fn a_recruit_on_watch_clears_the_road_under_the_sergeants_lap() {
    use onlinerpg_shared::messages::{PatrolMuster, WatchTool};
    use onlinerpg_shared::schedule::WATCH_GEAR_RACK;
    let (game, auth, _owner_rx, _guard_rx) = setup("recruit_sweep").await;
    build(&game, &auth).await;
    let recruit = create_test_character(&auth, &auth.login_google("Recruit").unwrap(), "Recruit");
    let mut walker = make_player("Recruit", 3.0, 2.0);
    walker.position.y = 5.05;
    game.add_player(walker).await;
    game.register_player_character(&pid("Recruit"), recruit.id, 0, attrs_with_cha(10), 0, None)
        .await;
    game.inventories
        .write()
        .await
        .insert(pid("Recruit"), PlayerInventory::default());
    let mut sergeant = make_player("Garrick", 3.0, 3.0);
    sergeant.position.y = 5.05;
    sergeant.is_official_npc = true;
    game.add_player(sergeant).await;
    let mut sergeant_rx = game.register_direct_channel(&pid("Garrick")).await;
    super::watch_shift_tests::lead_watch(&game, "Garrick", "12:30-22:00");
    let rack = onlinerpg_shared::furniture::FurniturePlacement {
        id: 9,
        type_id: WATCH_GEAR_RACK.into(),
        x: 3.0,
        y: 5.05,
        z: 2.5,
        rotation_deg: 0.0,
        floor_level: 0,
        open_doors: 0,
    };
    game.sync_region_furniture(0, 0, &[rack]);
    game.debug_set_datetime(&GameState::total_game_seconds_to_datetime(12 * 3600));

    game.watch_shift_accept(&pid("Recruit"), &pid("Karl"), "Garrick", &auth)
        .await;
    game.watch_rack_use(&pid("Recruit"), 9, &auth).await;
    game.patrol_roll_call(&pid("Garrick"), PatrolMuster::Out, 41, &auth)
        .await;
    game.watch_tool_use(&pid("Recruit"), WatchTool::Spear, &auth)
        .await;
    let left = auth.load_fences().unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].edge, edge(OFF_ROAD));

    // The sergeant's sweep on the same lap is no second offence.
    game.edit_fence(&pid("Builder"), edge(ON_ROAD[0]), true, &auth, false)
        .await;
    game.patrol_sweep(&pid("Garrick"), 41, &auth).await;
    assert!(!sweep_result(&mut sergeant_rx)[0].forfeited);
}
