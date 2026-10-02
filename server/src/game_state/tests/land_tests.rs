use super::*;
use onlinerpg_terrain::land::{plot_addr, LandGrade, REGION_PLOTS};

async fn add_estate_return_scroll(game: &GameState, name: &str) {
    game.inventories
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .bag
        .push(bag_item(100, "scroll_of_estate_return", 2));
}

async fn estate_return_quantity(game: &GameState, name: &str) -> u32 {
    game.get_player_inventory(&pid(name))
        .await
        .unwrap()
        .bag
        .iter()
        .find(|item| item.instance_id == 100)
        .map_or(0, |item| item.quantity)
}

async fn place_return_rug(
    game: &GameState,
    auth: &crate::auth::AuthService,
    name: &str,
    position: Position,
    floor_level: i8,
) -> onlinerpg_shared::estate_storage::EstateChest {
    let id = pid(name);
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.position = position;
        player.floor_level = floor_level;
    }
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(101, "furniture_hearthbound_rug", 1));
    game.place_estate_chest(&id, 101, position, 90.0, floor_level, auth)
        .await;
    auth.load_estate_chests()
        .unwrap()
        .into_iter()
        .find(|rug| {
            rug.position.x == position.x
                && rug.position.z == position.z
                && rug.floor_level == floor_level
        })
        .expect("rug placed")
}

async fn return_rug_house(game: &GameState) -> onlinerpg_shared::housing::HouseData {
    let rooms: Vec<_> = (0..=1)
        .map(|floor| {
            serde_json::json!({
                "localX": 0, "localZ": 0, "sizeX": 6, "sizeZ": 6,
                "floorLevel": floor, "floorTexture": 0, "roofTexture": 0, "wallHeight": 3.0,
                "wallNorth": [], "wallSouth": [], "wallEast": [], "wallWest": []
            })
        })
        .collect();
    let grids: Vec<_> = (0..=1)
        .map(|floor| {
            serde_json::json!({
                "floorLevel": floor, "originX": 0, "originZ": 0, "width": 6, "depth": 6,
                "cells": vec![0; 36]
            })
        })
        .collect();
    let house = serde_json::from_value(serde_json::json!({
        "id": "return_rug_house", "ownerId": "Settler",
        "origin": {"x": 3.0, "y": 5.0, "z": 3.0},
        "rooms": rooms, "passability": grids
    }))
    .unwrap();
    game.housing_io.write_house(&house).await.unwrap();
    game.passability_add_house(&house).await;
    house
}

#[tokio::test]
async fn estate_return_rug_arrives_on_an_upper_floor_for_account_characters() {
    let game = make_test_game_state("estate_return_indoor_rug");
    let auth = make_test_auth("estate_return_indoor_rug");
    let account = auth.login_google("indoor-rug-owner").unwrap();
    land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    return_rug_house(&game).await;
    let position = pos3(5.5, 8.1, 5.5);
    let rug = place_return_rug(&game, &auth, "Settler", position, 1).await;
    assert!((rug.position.y - 8.15).abs() < 0.001);
    let (_, mut rx) = land_owner(&game, &auth, &account, "Sibling").await;
    add_estate_return_scroll(&game, "Sibling").await;
    let id = pid("Sibling");
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.position = pos3(200.0, -10.0, 200.0);
        player.floor_level = -1;
    }
    game.use_estate_return_scroll(&id, 100, &auth).await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(player.position, position);
    assert_eq!(player.floor_level, 1);
    assert_eq!(player.rotation, std::f32::consts::FRAC_PI_2);
    assert_eq!(estate_return_quantity(&game, "Sibling").await, 1);
    assert!(drain(&mut rx).iter().any(|msg| matches!(msg,
        ServerMessage::PlayerTeleportEffect { position: target, floor_level: 1, phase: onlinerpg_shared::TeleportPhase::Arriving, .. }
        if *target == position
    )));
}

#[tokio::test]
async fn estate_return_rug_preserves_adjusted_height_and_returns_to_the_walking_floor() {
    use crate::game_state::estate_storage::EstateFurnitureMove;
    for (indoors, floor, base_y) in [(false, 0, 5.0), (true, 0, 5.0), (true, 1, 8.1)] {
        let test_name = format!("estate_return_raised_rug_{indoors}_{floor}");
        let game = make_test_game_state(&test_name);
        let auth = make_test_auth(&test_name);
        let account = auth.login_google(&test_name).unwrap();
        land_owner(&game, &auth, &account, "Settler").await;
        claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
        if indoors {
            return_rug_house(&game).await;
        }
        let position = pos3(5.5, base_y, 5.5);
        let rug = place_return_rug(&game, &auth, "Settler", position, floor).await;
        let surface_y = base_y + if indoors { 0.05 } else { 0.0 };
        assert!((rug.position.y - surface_y).abs() < 0.001);
        let id = pid("Settler");
        let target = pos3(5.55, surface_y + 0.2, 5.5);
        game.move_estate_furniture(
            &id,
            EstateFurnitureMove {
                furniture_id: rug.id,
                expected_revision: rug.revision,
                position: target,
                rotation_deg: 90.0,
                floor_level: floor,
            },
            &auth,
        )
        .await;
        let saved = auth.load_estate_chests().unwrap().remove(0);
        assert!((saved.position.y - target.y).abs() < 0.001);
        assert_eq!(saved.revision, rug.revision + 1);
        game.move_estate_furniture(
            &id,
            EstateFurnitureMove {
                furniture_id: rug.id,
                expected_revision: saved.revision,
                position: pos3(target.x, surface_y + 1.0, target.z),
                rotation_deg: 90.0,
                floor_level: floor,
            },
            &auth,
        )
        .await;
        let unchanged = auth.load_estate_chests().unwrap().remove(0);
        assert_eq!(unchanged.position, saved.position);
        assert_eq!(unchanged.revision, saved.revision);
        add_estate_return_scroll(&game, "Settler").await;
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&id).unwrap();
            player.position = pos3(200.0, 5.0, 200.0);
            player.floor_level = 0;
        }
        game.use_estate_return_scroll(&id, 100, &auth).await;
        let player = game.players.read().await[&id].clone();
        assert!((player.position.x - target.x).abs() < 0.001);
        assert!((player.position.y - base_y).abs() < 0.001);
        assert_eq!(player.position.z, target.z);
        assert_eq!(player.floor_level, floor);
        assert_eq!(estate_return_quantity(&game, "Settler").await, 1);
    }
}

#[tokio::test]
async fn estate_return_rug_follows_moves_and_recovery_restores_outdoor_return() {
    use crate::game_state::estate_storage::EstateFurnitureMove;
    let game = make_test_game_state("estate_return_moved_rug");
    let auth = make_test_auth("estate_return_moved_rug");
    let account = auth.login_google("moved-rug-owner").unwrap();
    land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    let rug = place_return_rug(&game, &auth, "Settler", pos3(6.5, 5.0, 6.5), 0).await;
    add_estate_return_scroll(&game, "Settler").await;
    let id = pid("Settler");
    let target = pos3(12.5, 5.0, 12.5);
    game.move_estate_furniture(
        &id,
        EstateFurnitureMove {
            furniture_id: rug.id,
            expected_revision: rug.revision,
            position: target,
            rotation_deg: 180.0,
            floor_level: 0,
        },
        &auth,
    )
    .await;
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(game.players.read().await[&id].position, target);
    game.recover_estate_chest(&id, rug.id, &auth).await;
    assert!(auth.estate_return_rugs(rug.owner_id).unwrap().is_empty());
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_ne!(game.players.read().await[&id].position, target);
    assert_eq!(estate_return_quantity(&game, "Settler").await, 0);
}

#[tokio::test]
async fn estate_return_rug_skips_blocked_and_removed_building_floors() {
    let game = make_test_game_state("estate_return_blocked_rug");
    let auth = make_test_auth("estate_return_blocked_rug");
    let account = auth.login_google("blocked-rug-owner").unwrap();
    land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    let mut house = return_rug_house(&game).await;
    let target = pos3(5.5, 8.1, 5.5);
    place_return_rug(&game, &auth, "Settler", target, 1).await;
    add_estate_return_scroll(&game, "Settler").await;
    house.passability[1].cells[14] = 0b1111;
    game.passability_add_house(&house).await;
    game.use_estate_return_scroll(&pid("Settler"), 100, &auth)
        .await;
    assert_eq!(game.players.read().await[&pid("Settler")].floor_level, 0);
    assert_ne!(game.players.read().await[&pid("Settler")].position, target);
    house.rooms.retain(|room| room.floor_level == 0);
    house.passability.retain(|grid| grid.floor_level == 0);
    game.housing_io.write_house(&house).await.unwrap();
    game.passability_add_house(&house).await;
    game.use_estate_return_scroll(&pid("Settler"), 100, &auth)
        .await;
    assert_eq!(game.players.read().await[&pid("Settler")].floor_level, 0);
    assert_eq!(estate_return_quantity(&game, "Settler").await, 0);
}

#[tokio::test]
async fn estate_return_rug_ignores_other_accounts() {
    let game = make_test_game_state("estate_return_foreign_rug");
    let auth = make_test_auth("estate_return_foreign_rug");
    let account = auth.login_google("foreign-rug-owner").unwrap();
    let (owner, _) = land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    place_return_rug(&game, &auth, "Settler", pos3(6.5, 5.0, 6.5), 0).await;
    let outsider = auth.login_google("foreign-rug-outsider").unwrap();
    let (other, _) = land_owner(&game, &auth, &outsider, "Outsider").await;
    claim_at(&game, &auth, "Outsider", 1, 33.0, 1.0).await;
    assert_eq!(auth.estate_return_rugs(owner).unwrap().len(), 1);
    assert!(auth.estate_return_rugs(other).unwrap().is_empty());
    add_estate_return_scroll(&game, "Outsider").await;
    game.use_estate_return_scroll(&pid("Outsider"), 100, &auth)
        .await;
    let position = game.players.read().await[&pid("Outsider")].position;
    assert_eq!(plot_addr(position.x, position.z), plot_addr(33.0, 1.0));
    assert_eq!(estate_return_quantity(&game, "Outsider").await, 1);
}

#[tokio::test]
async fn estate_return_scroll_returns_from_dungeon_and_spends_one() {
    let game = make_test_game_state("estate_return");
    let auth = make_test_auth("estate_return");
    let account = auth.login_google("estate-return").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    add_estate_return_scroll(&game, "Settler").await;
    let id = pid("Settler");
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.position = Position {
            x: 200.0,
            y: -10.0,
            z: 200.0,
        };
        player.floor_level = -1;
        player.last_combat_at = GameState::now_ms();
    }
    drain(&mut rx);
    game.use_estate_return_scroll(&id, 100, &auth).await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(player.floor_level, 0);
    assert_eq!(
        plot_addr(player.position.x, player.position.z),
        plot_addr(1.0, 1.0)
    );
    assert!((player.position.y - 5.0).abs() < 0.01);
    assert_eq!(estate_return_quantity(&game, "Settler").await, 1);
    let messages = drain(&mut rx);
    let effect = messages.iter().position(|msg| matches!(msg,
        ServerMessage::PlayerTeleportEffect { player_id, position, floor_level: 0, phase: onlinerpg_shared::TeleportPhase::Arriving }
        if *player_id == id && *position == player.position
    )).unwrap();
    let moved = messages
        .iter()
        .position(|msg| matches!(msg, ServerMessage::PlayerTeleported { .. }))
        .unwrap();
    assert!(effect < moved);
    assert!(!messages.iter().any(|msg| matches!(
        msg,
        ServerMessage::PlayerTeleportEffect {
            phase: onlinerpg_shared::TeleportPhase::Departing,
            ..
        }
    )));
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(estate_return_quantity(&game, "Settler").await, 0);
    let elsewhere = Position {
        x: 200.0,
        y: 5.0,
        z: 200.0,
    };
    game.players.write().await.get_mut(&id).unwrap().position = elsewhere;
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(game.players.read().await[&id].position, elsewhere);
}

#[tokio::test]
async fn estate_return_scroll_rejects_nonowners_defeat_and_reserved_items() {
    let game = make_test_game_state("estate_return_guards");
    let auth = make_test_auth("estate_return_guards");
    let account = auth.login_google("estate-return-guards").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let other_account = auth.login_google("estate-return-outsider").unwrap();
    land_owner(&game, &auth, &other_account, "Sibling").await;
    add_estate_return_scroll(&game, "Settler").await;
    add_estate_return_scroll(&game, "Sibling").await;
    let id = pid("Settler");
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(estate_return_quantity(&game, "Settler").await, 2);
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|msg| matches!(msg, ServerMessage::SystemMessage { message, .. } if message.contains("don't own an estate"))));
    assert!(messages.iter().any(|msg| matches!(
        msg,
        ServerMessage::PlayerTeleportEffect {
            phase: onlinerpg_shared::TeleportPhase::Cancelled,
            ..
        }
    )));
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    game.use_estate_return_scroll(&pid("Sibling"), 100, &auth)
        .await;
    assert_eq!(estate_return_quantity(&game, "Sibling").await, 2);
    game.players.write().await.get_mut(&id).unwrap().health = 0;
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(estate_return_quantity(&game, "Settler").await, 2);
    game.players.write().await.get_mut(&id).unwrap().health = 10;
    game.request_player_trade(&id, "Sibling").await;
    game.respond_player_trade(&pid("Sibling"), &id, true).await;
    game.set_player_trade_offer(
        &id,
        vec![onlinerpg_shared::messages::PlayerTradeSlot {
            instance_id: 100,
            quantity: 1,
        }],
        0,
    )
    .await;
    assert_eq!(game.trade_reserved_quantity(&id, 100).await, 1);
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(estate_return_quantity(&game, "Settler").await, 2);
    assert_eq!(game.players.read().await[&id].position.x, 1.0);
}

#[tokio::test]
async fn estate_return_scroll_keeps_scroll_when_estate_is_underwater() {
    let game = make_test_game_state("estate_return_water");
    let auth = make_test_auth("estate_return_water");
    let account = auth.login_google("estate-return-water").unwrap();
    let (character_id, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    game.terrain_io
        .write_land_grades(-1, 0, &vec![LandGrade::Homestead as u8; REGION_PLOTS])
        .await
        .unwrap();
    claim_at(&game, &auth, "Settler", 1, -33.0, 1.0).await;
    assert_eq!(auth.homestead_plots(character_id).unwrap().len(), 1);
    add_estate_return_scroll(&game, "Settler").await;
    let id = pid("Settler");
    let before = game.players.read().await[&id].position;
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(game.players.read().await[&id].position, before);
    assert_eq!(estate_return_quantity(&game, "Settler").await, 2);
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|msg| matches!(msg, ServerMessage::SystemMessage { message, .. } if message.contains("No safe outdoor"))));
    assert!(messages.iter().any(|msg| matches!(
        msg,
        ServerMessage::PlayerTeleportEffect {
            phase: onlinerpg_shared::TeleportPhase::Cancelled,
            ..
        }
    )));
}

#[tokio::test]
async fn estate_return_scroll_avoids_buildings_and_searches_other_owned_plots() {
    use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};
    let game = make_test_game_state("estate_return_blocked");
    let auth = make_test_auth("estate_return_blocked");
    let account = auth.login_google("estate-return-blocked").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    add_estate_return_scroll(&game, "Settler").await;
    game.passability_write().insert(
        "covered_estate".into(),
        RuntimePassability {
            house_origin_x: 0.0,
            house_origin_z: 0.0,
            min_x: 0.0,
            max_x: 32.0,
            min_z: 0.0,
            max_z: 32.0,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 32,
                depth: 32,
                y_base: 5.0,
                wall_height: 3.0,
                cells: vec![0; 32 * 32],
            }],
            stairwells: vec![],
            yields_to_trapped_mover: false,
            allows_projectiles: false,
            is_ground: true,
        },
    );
    let id = pid("Settler");
    game.use_estate_return_scroll(&id, 100, &auth).await;
    assert_eq!(estate_return_quantity(&game, "Settler").await, 2);
    assert!(drain(&mut rx).iter().any(|msg| matches!(msg, ServerMessage::SystemMessage { message, .. } if message.contains("No safe outdoor"))));
    claim_at(&game, &auth, "Settler", 2, 33.0, 1.0).await;
    game.use_estate_return_scroll(&id, 100, &auth).await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(
        plot_addr(player.position.x, player.position.z),
        plot_addr(33.0, 1.0)
    );
    assert_eq!(estate_return_quantity(&game, "Settler").await, 1);
}

async fn land_owner(
    game: &GameState,
    auth: &crate::auth::AuthService,
    account: &str,
    name: &str,
) -> (i64, DirectRx) {
    let deeds = (1..=20).map(|id| bag_item(id, "land_deed", 1)).collect();
    let character_id = estate_owner(game, auth, account, name, pos3(1.0, 0.0, 1.0), deeds).await;
    (character_id, game.register_direct_channel(&pid(name)).await)
}

async fn claim_at(
    game: &GameState,
    auth: &crate::auth::AuthService,
    name: &str,
    deed: u64,
    x: f32,
    z: f32,
) {
    game.players
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .position = Position { x, y: 1.0, z };
    let plot = super::super::land::plot_key(plot_addr(x, z));
    game.claim_land(&pid(name), deed, plot, auth).await;
}

fn rejected(rx: &mut DirectRx, text: &str) {
    assert!(drain(rx).iter().any(
        |message| matches!(message, ServerMessage::LandRejected { reason } if reason.contains(text))
    ));
}

fn claim_test_house(
    id: &str,
    owner_id: &str,
    x: f32,
    z: f32,
) -> onlinerpg_shared::housing::HouseData {
    serde_json::from_value(serde_json::json!({
        "id": id, "ownerId": owner_id, "origin": {"x": x, "y": 5.0, "z": z},
        "sourceScrollId": "scroll_of_small_house",
        "rooms": [{
            "localX": 0, "localZ": 0, "sizeX": 6, "sizeZ": 6, "floorLevel": 0,
            "floorTexture": 0, "roofTexture": 0, "wallHeight": 3.0,
            "wallNorth": [], "wallSouth": [], "wallEast": [], "wallWest": []
        }],
        "passability": [{"floorLevel": 0, "originX": 0, "originZ": 0,
            "width": 6, "depth": 6, "cells": vec![0; 36]}]
    }))
    .unwrap()
}

#[tokio::test]
async fn land_claim_adopts_abandoned_house_and_preserves_existing_owners() {
    let game = make_test_game_state("land_adopts_house");
    let auth = make_test_auth("land_adopts_house");
    let account = auth.login_google("house-claimant").unwrap();
    let (claimant, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let other_account = auth.login_google("existing-house-owner").unwrap();
    let existing = create_test_character(&auth, &other_account, "Existing");
    let former = create_test_character(&auth, &other_account, "Former");
    auth.delete_character(&other_account, former.id).unwrap();
    let abandoned = claim_test_house("abandoned", &former.id.to_string(), 3.0, 3.0);
    let houses = [
        abandoned.clone(),
        claim_test_house("occupied", &existing.id.to_string(), 12.0, 3.0),
        claim_test_house("public", "", 21.0, 3.0),
        claim_test_house("editor", "local", 3.0, 12.0),
        claim_test_house("outside", &former.id.to_string(), 35.0, 3.0),
    ];
    for house in &houses {
        game.housing_io.write_house(house).await.unwrap();
        game.passability_add_house(house).await;
    }
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::LandClaimed { .. })));
    let adopted = game
        .housing_io
        .find_house("abandoned")
        .await
        .unwrap()
        .unwrap();
    let mut expected = abandoned;
    expected.owner_id = claimant.to_string();
    assert_eq!(
        serde_json::to_value(&adopted).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    for house in &houses[1..] {
        assert_eq!(
            game.housing_io
                .find_house(&house.id)
                .await
                .unwrap()
                .unwrap()
                .owner_id,
            house.owner_id
        );
    }
    assert!(auth.pending_estate_house_claims().unwrap().is_empty());
    game.inventories
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .bag
        .push(bag_item(
            100,
            onlinerpg_shared::landscaping::TOOLBOX_ITEM,
            1,
        ));
    game.demolish_house(&pid("Settler"), adopted.id.clone(), &auth)
        .await;
    assert!(game
        .housing_io
        .find_house(&adopted.id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn land_claim_adopts_house_only_after_its_whole_foundation_is_owned() {
    let game = make_test_game_state("land_adopts_whole_house");
    let auth = make_test_auth("land_adopts_whole_house");
    let account = auth.login_google("whole-house-claimant").unwrap();
    let (claimant, _) = land_owner(&game, &auth, &account, "Settler").await;
    let house = claim_test_house("spanning", "999999", 30.0, 3.0);
    game.housing_io.write_house(&house).await.unwrap();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    assert_eq!(
        game.housing_io
            .find_house(&house.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        "999999"
    );
    claim_at(&game, &auth, "Settler", 2, 33.0, 1.0).await;
    assert_eq!(
        game.housing_io
            .find_house(&house.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        claimant.to_string()
    );
}

#[tokio::test]
async fn land_claim_rejection_leaves_abandoned_house_and_deed_unchanged() {
    let game = make_test_game_state("land_house_rejected_claim");
    let auth = make_test_auth("land_house_rejected_claim");
    let account = auth.login_google("rejected-house-claimant").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let house = claim_test_house("nonadjacent", "999999", 100.0, 3.0);
    game.housing_io.write_house(&house).await.unwrap();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    claim_at(&game, &auth, "Settler", 2, 100.0, 1.0).await;
    rejected(&mut rx, "edge");
    assert_eq!(
        game.housing_io
            .find_house(&house.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        "999999"
    );
    assert!(game
        .get_player_inventory(&pid("Settler"))
        .await
        .unwrap()
        .bag
        .iter()
        .any(|item| item.instance_id == 2));
    assert!(auth.pending_estate_house_claims().unwrap().is_empty());
}

#[tokio::test]
async fn land_house_claim_persists_and_retries_after_file_error_and_owner_deletion() {
    let mut game = make_test_game_state("land_house_claim_retry");
    let housing_dir =
        std::env::temp_dir().join(format!("land_house_retry_{}", uuid::Uuid::new_v4()));
    game.housing_io = Arc::new(HousingIO::new(housing_dir.clone()));
    let (auth, path) = make_test_auth_with_path("land_house_claim_retry");
    let account = auth.login_google("house-claim-retry").unwrap();
    let (claimant, _) = land_owner(&game, &auth, &account, "Settler").await;
    let (sibling, _) = land_owner(&game, &auth, &account, "Sibling").await;
    let house = claim_test_house("retry-house", "999999", 3.0, 3.0);
    game.housing_io.write_house(&house).await.unwrap();
    let character = game.get_player_save_data(&pid("Settler")).await.unwrap();
    let mut inventory = game.get_player_inventory(&pid("Settler")).await.unwrap();
    inventory.bag.retain(|item| item.instance_id != 1);
    let rows = super::super::inventory::serialize_inventory(&inventory);
    auth.claim_homestead(
        &character,
        (0, 0, 3),
        &rows,
        0,
        &[(house.id.clone(), house.owner_id.clone())],
    )
    .unwrap()
    .unwrap();
    auth.delete_character(&account, claimant).unwrap();
    let reopened = crate::auth::AuthService::new(path).unwrap();
    assert_eq!(
        reopened.pending_estate_house_claims().unwrap()[0].1,
        sibling
    );
    let house_path = housing_dir.join("r+00_+00/retry-house.json");
    tokio::fs::write(&house_path, b"invalid json")
        .await
        .unwrap();
    assert!(game.apply_estate_house_claims(&reopened).await.is_err());
    assert_eq!(reopened.pending_estate_house_claims().unwrap().len(), 1);
    game.housing_io.write_house(&house).await.unwrap();
    game.apply_estate_house_claims(&reopened).await.unwrap();
    assert_eq!(
        game.housing_io
            .find_house(&house.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        sibling.to_string()
    );
    assert!(reopened.pending_estate_house_claims().unwrap().is_empty());
    game.apply_estate_house_claims(&reopened).await.unwrap();
}

#[tokio::test]
async fn land_house_claim_retry_preserves_houses_moved_or_given_an_owner() {
    let game = make_test_game_state("land_house_claim_changed");
    let auth = make_test_auth("land_house_claim_changed");
    let account = auth.login_google("changed-house-claimant").unwrap();
    let (claimant, _) = land_owner(&game, &auth, &account, "Settler").await;
    let other_account = auth.login_google("changed-house-owner").unwrap();
    let existing = create_test_character(&auth, &other_account, "Existing");
    let mut moved = claim_test_house("moved-pending", "999999", 3.0, 3.0);
    let mut reassigned = claim_test_house("reassigned-pending", "999999", 12.0, 3.0);
    let character = game.get_player_save_data(&pid("Settler")).await.unwrap();
    let inventory = game.get_player_inventory(&pid("Settler")).await.unwrap();
    let rows = super::super::inventory::serialize_inventory(&inventory);
    auth.claim_homestead(
        &character,
        (0, 0, 3),
        &rows,
        0,
        &[
            (moved.id.clone(), moved.owner_id.clone()),
            (reassigned.id.clone(), reassigned.owner_id.clone()),
        ],
    )
    .unwrap()
    .unwrap();
    moved.origin.x = 100.0;
    reassigned.owner_id = existing.id.to_string();
    game.housing_io.write_house(&moved).await.unwrap();
    game.housing_io.write_house(&reassigned).await.unwrap();
    game.apply_estate_house_claims(&auth).await.unwrap();
    assert_ne!(
        game.housing_io
            .find_house(&moved.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        claimant.to_string()
    );
    assert_eq!(
        game.housing_io
            .find_house(&reassigned.id)
            .await
            .unwrap()
            .unwrap()
            .owner_id,
        existing.id.to_string()
    );
    assert!(auth.pending_estate_house_claims().unwrap().is_empty());
}

#[tokio::test]
async fn land_expansion_wraps_across_the_world_seam() {
    let game = make_test_game_state("land_seam");
    let auth = make_test_auth("land_seam");
    let account = auth.login_google("land-seam").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    for rx in [-16, 15] {
        game.terrain_io
            .write_land_grades(rx, 0, &vec![LandGrade::Homestead as u8; REGION_PLOTS])
            .await
            .unwrap();
    }
    claim_at(&game, &auth, "Settler", 1, 16351.0, 1.0).await;
    claim_at(&game, &auth, "Settler", 2, -16415.0, 1.0).await;
    assert_eq!(
        drain(&mut rx)
            .iter()
            .filter(|msg| matches!(msg, ServerMessage::LandClaimed { .. }))
            .count(),
        2
    );
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        18
    );
    let plots = auth.owned_land_plots().unwrap();
    assert_eq!(plots.len(), 2);
    for (x, z) in [(16351.0, 1.0), (-16415.0, 1.0)] {
        let addr = plot_addr(x, z);
        assert!(plots.iter().any(|plot| plot.rx == addr.rx
            && plot.rz == addr.rz
            && plot.index == addr.index
            && plot.owner_name == "Settler"));
    }
}

#[tokio::test]
async fn land_deed_reserved_for_trade_cannot_be_spent() {
    let game = make_test_game_state("land_trade");
    let auth = make_test_auth("land_trade");
    let account = auth.login_google("land-trade").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    land_owner(&game, &auth, &account, "Buyer").await;
    game.request_player_trade(&pid("Settler"), "Buyer").await;
    game.respond_player_trade(&pid("Buyer"), &pid("Settler"), true)
        .await;
    game.set_player_trade_offer(
        &pid("Settler"),
        vec![onlinerpg_shared::messages::PlayerTradeSlot {
            instance_id: 1,
            quantity: 1,
        }],
        0,
    )
    .await;
    assert_eq!(game.trade_reserved_quantity(&pid("Settler"), 1).await, 1);
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "reserved for trade");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );
}

#[tokio::test]
async fn land_preview_spends_nothing_and_confirm_persists_once() {
    let game = make_test_game_state("land_persist");
    let (auth, path) = make_test_auth_with_path("land_persist");
    let account = auth.login_google("land-persist").unwrap();
    let (character, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    assert!(game.try_preview_land_claim(&pid("Settler"), 1, &auth).await);
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::LandClaimPrompt {
            instance_id: 1,
            tile_x: 0,
            tile_z: 0,
            quadrant: 3,
            reason: None,
        }
    )));
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );

    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::LandClaimed { .. })));
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        19
    );
    let reopened = crate::auth::AuthService::new(path.clone()).unwrap();
    assert_eq!(reopened.load_inventory(character).unwrap().len(), 19);
    let conn = rusqlite::Connection::open(path).unwrap();
    let owner: i64 = conn.query_row("SELECT owner_id FROM land_estates JOIN land_plots ON estate_id=land_estates.id WHERE tile_x=0 AND tile_z=0 AND quadrant=3", [], |row| row.get(0)).unwrap();
    assert_eq!(owner, character);
    claim_at(&game, &auth, "Settler", 2, 1.0, 1.0).await;
    rejected(&mut rx, "already belongs");
    claim_at(&game, &auth, "Settler", 1, 33.0, 1.0).await;
    rejected(&mut rx, "not found");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        19
    );
}

#[tokio::test]
async fn land_rechecks_level_location_alive_floor_and_document() {
    let game = make_test_game_state("land_conditions");
    let auth = make_test_auth("land_conditions");
    let account = auth.login_google("land-conditions").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .level = 3;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "level 4");
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .level = 4;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .health = 0;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "alive");
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .health = 1;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .floor_level = -1;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "outdoor");
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .floor_level = 0;
    game.claim_land(&pid("Settler"), 1, (0, 0, 2), &auth).await;
    rejected(&mut rx, "left the selected");
    game.claim_land(&pid("Settler"), 1, (0, 0, 255), &auth)
        .await;
    rejected(&mut rx, "left the selected");
    game.inventories
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .bag[0]
        .item_def_id = "torch".into();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "not found");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );
}

#[tokio::test]
async fn land_uses_current_edited_grades_and_fails_closed_on_bad_files() {
    let game = make_test_game_state("land_grades");
    let auth = make_test_auth("land_grades");
    let account = auth.login_google("land-grades").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    for (grade, reason) in [
        (LandGrade::Crown, "Crown"),
        (LandGrade::Reserved, "reserved"),
    ] {
        game.terrain_io
            .write_land_grades(0, 0, &vec![grade as u8; REGION_PLOTS])
            .await
            .unwrap();
        claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
        rejected(&mut rx, reason);
    }
    let path = onlinerpg_terrain::coords::land_grade_path(game.terrain_io.base_dir(), 0, 0);
    std::fs::write(path, [1]).unwrap();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "temporarily unavailable");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );
}

#[tokio::test]
async fn land_expansion_requires_edges_and_enforces_account_and_size_limits() {
    let game = make_test_game_state("land_limits");
    let auth = make_test_auth("land_limits");
    let account = auth.login_google("land-limits").unwrap();
    let (owner, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let (sibling, mut alt_rx) = land_owner(&game, &auth, &account, "Sibling").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    claim_at(&game, &auth, "Sibling", 1, 33.0, 1.0).await;
    assert!(drain(&mut alt_rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::LandClaimed { .. })));
    assert_eq!(
        auth.homestead_plots(owner).unwrap(),
        auth.homestead_plots(sibling).unwrap()
    );
    claim_at(&game, &auth, "Settler", 2, 65.0, 33.0).await;
    rejected(&mut rx, "shares an edge");
    for i in 2..8 {
        claim_at(&game, &auth, "Settler", i + 1, i as f32 * 32.0 + 1.0, 1.0).await;
    }
    claim_at(&game, &auth, "Settler", 9, 257.0, 1.0).await;
    rejected(&mut rx, "8 by 8");
    for i in 0..8 {
        claim_at(&game, &auth, "Settler", i + 9, i as f32 * 32.0 + 1.0, 33.0).await;
    }
    claim_at(&game, &auth, "Settler", 17, 1.0, 65.0).await;
    rejected(&mut rx, "16 plots");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        5
    );
}

#[tokio::test]
async fn shared_estate_returns_and_tax_account_follow_all_three_characters() {
    let game = make_test_game_state("shared_estate_return");
    let auth = make_test_auth("shared_estate_return");
    let account = auth.login_google("shared-estate-return").unwrap();
    let (owner, _) = land_owner(&game, &auth, &account, "Owner").await;
    claim_at(&game, &auth, "Owner", 1, 1.0, 1.0).await;
    for name in ["Sibling", "Third"] {
        let (character_id, _) = land_owner(&game, &auth, &account, name).await;
        assert_eq!(
            auth.homestead_plots(character_id).unwrap(),
            auth.homestead_plots(owner).unwrap()
        );
        assert_eq!(auth.land_account(character_id).unwrap().plots, 1);
        add_estate_return_scroll(&game, name).await;
        game.players
            .write()
            .await
            .get_mut(&pid(name))
            .unwrap()
            .position = pos3(200.0, 5.0, 200.0);
        game.use_estate_return_scroll(&pid(name), 100, &auth).await;
        let position = game.players.read().await[&pid(name)].position;
        assert_eq!(plot_addr(position.x, position.z), plot_addr(1.0, 1.0));
        assert_eq!(estate_return_quantity(&game, name).await, 1);
    }
    let stranger = auth.login_google("shared-estate-stranger").unwrap();
    let (outsider, _) = land_owner(&game, &auth, &stranger, "Outsider").await;
    assert!(auth.homestead_plots(outsider).unwrap().is_empty());
    assert_eq!(auth.land_account(outsider).unwrap().plots, 0);
    let steward = pid("npc_steward");
    let mut npc = make_player("npc_steward", 1.0, 1.0);
    npc.name = "Aldwin".into();
    npc.is_official_npc = true;
    game.add_player(npc).await;
    game.players
        .write()
        .await
        .get_mut(&pid("Sibling"))
        .unwrap()
        .position = pos3(1.0, 5.0, 1.0);
    game.players
        .write()
        .await
        .get_mut(&pid("Third"))
        .unwrap()
        .position = pos3(1.0, 5.0, 1.0);
    game.player_gold
        .write()
        .await
        .insert(pid("Sibling"), 10_000);
    game.player_gold.write().await.insert(pid("Third"), 0);
    game.land_account_action(&pid("Sibling"), &steward, Some((6_000, true)), &auth)
        .await;
    assert_eq!(auth.land_account(owner).unwrap().treasury, 6_000);
    game.land_account_action(&pid("Third"), &steward, Some((1_000, false)), &auth)
        .await;
    assert_eq!(auth.land_account(owner).unwrap().treasury, 5_000);
    assert_eq!(game.player_gold.read().await[&pid("Third")], 1_000);
    assert_eq!(game.player_gold.read().await[&pid("Sibling")], 4_000);
}

#[tokio::test]
async fn shared_estate_tax_activity_includes_online_and_recent_siblings() {
    let game = make_test_game_state("shared_estate_tax");
    let (auth, path) = make_test_auth_with_path("shared_estate_tax");
    let account = auth.login_google("shared-estate-tax").unwrap();
    let (owner, _) = land_owner(&game, &auth, &account, "Owner").await;
    let (sibling, _) = land_owner(&game, &auth, &account, "Sibling").await;
    claim_at(&game, &auth, "Owner", 1, 1.0, 1.0).await;
    let db = rusqlite::Connection::open(path).unwrap();
    let month: i64 = db
        .query_row("SELECT month FROM land_tax_periods", [], |row| row.get(0))
        .unwrap();
    db.execute_batch("UPDATE land_estates SET treasury=10000,free_months=0; UPDATE characters SET last_seen_at=0;").unwrap();
    auth.collect_land_taxes(month + 1, &[sibling]).unwrap();
    assert_eq!(auth.land_account(owner).unwrap().treasury, 8_000);
    assert_eq!(auth.land_account(owner).unwrap().missed, 0);
    db.execute(
        "UPDATE characters SET last_seen_at=?1 WHERE id=?2",
        rusqlite::params![crate::auth::unix_now(), sibling],
    )
    .unwrap();
    auth.collect_land_taxes(month + 2, &[]).unwrap();
    assert_eq!(auth.land_account(sibling).unwrap().treasury, 6_000);
    assert_eq!(auth.land_account(sibling).unwrap().missed, 0);
}

#[tokio::test]
async fn land_competing_claims_have_one_winner() {
    let game = make_test_game_state("land_race");
    let auth = make_test_auth("land_race");
    let a = auth.login_google("land-race-a").unwrap();
    let b = auth.login_google("land-race-b").unwrap();
    let (_, mut arx) = land_owner(&game, &auth, &a, "First").await;
    let (_, mut brx) = land_owner(&game, &auth, &b, "Second").await;
    let first = pid("First");
    let second = pid("Second");
    tokio::join!(
        game.claim_land(&first, 1, (0, 0, 3), &auth),
        game.claim_land(&second, 1, (0, 0, 3), &auth)
    );
    let messages: Vec<_> = drain(&mut arx).into_iter().chain(drain(&mut brx)).collect();
    assert_eq!(
        messages
            .iter()
            .filter(|msg| matches!(msg, ServerMessage::LandClaimed { .. }))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|msg| matches!(msg, ServerMessage::LandRejected { .. }))
            .count(),
        1
    );
    assert_eq!(
        game.get_player_inventory(&pid("First"))
            .await
            .unwrap()
            .bag
            .len()
            + game
                .get_player_inventory(&pid("Second"))
                .await
                .unwrap()
                .bag
                .len(),
        39
    );
}

#[tokio::test]
async fn land_db_failure_rolls_back_ownership_and_keeps_deed() {
    let game = make_test_game_state("land_rollback");
    let (auth, path) = make_test_auth_with_path("land_rollback");
    let account = auth.login_google("land-rollback").unwrap();
    let (character, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let initial = auth.load_inventory(character).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("CREATE TRIGGER fail_land_inventory BEFORE INSERT ON character_items BEGIN SELECT RAISE(FAIL, 'test failure'); END;").unwrap();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    rejected(&mut rx, "could not be saved");
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );
    assert_eq!(auth.load_inventory(character).unwrap(), initial);
    for table in ["land_estates", "land_plots"] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

async fn preview_rejected(
    game: &GameState,
    auth: &crate::auth::AuthService,
    rx: &mut DirectRx,
    reason: &str,
) {
    assert!(game.try_preview_land_claim(&pid("Settler"), 2, auth).await);
    let messages = drain(rx);
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::LandClaimPrompt { instance_id: 2, reason: Some(text), .. } if text.contains(reason)
    )), "expected preview rejection: {reason}, got {messages:?}");
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::LandClaimed { .. })));
}

#[tokio::test]
async fn land_preview_checks_level_grades_and_ownership_without_spending() {
    let game = make_test_game_state("land_preview_rejected");
    let auth = make_test_auth("land_preview_rejected");
    let account = auth.login_google("land-preview-rejected").unwrap();
    let (_, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .level = 3;
    preview_rejected(&game, &auth, &mut rx, "level 4").await;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .level = 4;
    for (grade, reason) in [
        (LandGrade::Crown, "Crown"),
        (LandGrade::Reserved, "reserved"),
    ] {
        game.terrain_io
            .write_land_grades(0, 0, &vec![grade as u8; REGION_PLOTS])
            .await
            .unwrap();
        preview_rejected(&game, &auth, &mut rx, reason).await;
    }
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        20
    );
    game.terrain_io
        .write_land_grades(0, 0, &vec![LandGrade::Homestead as u8; REGION_PLOTS])
        .await
        .unwrap();
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    drain(&mut rx);
    preview_rejected(&game, &auth, &mut rx, "already belongs").await;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .position
        .x = 65.0;
    preview_rejected(&game, &auth, &mut rx, "shares an edge").await;
    game.players
        .write()
        .await
        .get_mut(&pid("Settler"))
        .unwrap()
        .position
        .x = 33.0;
    assert!(game.try_preview_land_claim(&pid("Settler"), 2, &auth).await);
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::LandClaimPrompt { reason: None, .. })));
    assert_eq!(
        game.get_player_inventory(&pid("Settler"))
            .await
            .unwrap()
            .bag
            .len(),
        19
    );
}

#[tokio::test]
async fn land_tax_account_transfers_persist_and_reject_invalid_requests() {
    let game = make_test_game_state("land_account_transfer");
    let (auth, path) = make_test_auth_with_path("land_account_transfer");
    let account = auth.login_google("land-account-transfer").unwrap();
    let (character, mut rx) = land_owner(&game, &auth, &account, "Settler").await;
    let owner = pid("Settler");
    let steward = pid("npc_steward");
    let mut npc = make_player("npc_steward", 1.0, 1.0);
    npc.name = "Aldwin".to_string();
    npc.is_official_npc = true;
    game.add_player(npc).await;
    game.player_gold.write().await.insert(owner, 10_000);
    game.land_account_action(&owner, &steward, Some((100, true)), &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&owner], 10_000);
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    drain(&mut rx);
    game.land_account_action(&owner, &steward, Some((6_000, true)), &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&owner], 4_000);
    assert_eq!(auth.land_account(character).unwrap().treasury, 6_000);
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::LandAccountState {
            treasury: 6_000,
            monthly_tax: 2_000,
            next_tax: 0,
            plots: 1,
            free_months: 1,
            error: None,
            ..
        }
    )));
    game.land_account_action(&owner, &steward, Some((1_000, false)), &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&owner], 5_000);
    let reopened = crate::auth::AuthService::new(path.clone()).unwrap();
    assert_eq!(reopened.land_account(character).unwrap().treasury, 5_000);
    assert_eq!(reopened.load_inventory(character).unwrap().len(), 19);
    let conn = rusqlite::Connection::open(path).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT gold FROM characters WHERE id=?1",
            [character],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        5_000
    );
    for (amount, deposit) in [
        (0, true),
        (-1, false),
        (5_001, true),
        (5_001, false),
        (i64::MAX, true),
    ] {
        game.land_account_action(&owner, &steward, Some((amount, deposit)), &auth)
            .await;
        assert_eq!(game.player_gold.read().await[&owner], 5_000);
        assert_eq!(auth.land_account(character).unwrap().treasury, 5_000);
    }
    game.players
        .write()
        .await
        .get_mut(&steward)
        .unwrap()
        .position
        .x = 100.0;
    game.land_account_action(&owner, &steward, Some((100, true)), &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&owner], 5_000);
    game.players
        .write()
        .await
        .get_mut(&steward)
        .unwrap()
        .position
        .x = 1.0;
    conn.execute_batch("CREATE TRIGGER fail_tax_inventory BEFORE INSERT ON character_items BEGIN SELECT RAISE(FAIL, 'test failure'); END;").unwrap();
    game.land_account_action(&owner, &steward, Some((100, true)), &auth)
        .await;
    assert_eq!(game.player_gold.read().await[&owner], 5_000);
    assert_eq!(auth.land_account(character).unwrap().treasury, 5_000);
    assert!(drain(&mut rx).iter().any(|m| matches!(m, ServerMessage::LandAccountState { error: Some(reason), .. } if reason.contains("could not be saved"))));
    assert!(auth
        .gold_sinks(crate::auth::unix_now() + 3600, 24)
        .unwrap()
        .entries
        .is_empty());
}

#[tokio::test]
async fn land_tax_rollover_exemption_recovery_and_restart_are_consistent() {
    let game = make_test_game_state("land_tax_rollover");
    let (auth, path) = make_test_auth_with_path("land_tax_rollover");
    let account = auth.login_google("land-tax-rollover").unwrap();
    let (character, _) = land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    let conn = rusqlite::Connection::open(&path).unwrap();
    let month: i64 = conn
        .query_row("SELECT month FROM land_tax_periods", [], |row| row.get(0))
        .unwrap();
    conn.execute("UPDATE land_estates SET treasury=2000", [])
        .unwrap();
    auth.collect_land_taxes(month + 1, &[character]).unwrap();
    let state = auth.land_account(character).unwrap();
    assert_eq!(
        (state.treasury, state.free_months, state.missed),
        (2000, 0, 0)
    );
    assert!(auth
        .gold_sinks(crate::auth::unix_now() + 3600, 24)
        .unwrap()
        .entries
        .is_empty());
    conn.execute_batch("CREATE TRIGGER reject_tax_metric BEFORE INSERT ON gold_sink_samples BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(auth.collect_land_taxes(month + 2, &[character]).is_err());
    assert_eq!(auth.land_account(character).unwrap().treasury, 2000);
    conn.execute_batch("DROP TRIGGER reject_tax_metric;")
        .unwrap();
    auth.collect_land_taxes(month + 2, &[character]).unwrap();
    assert_eq!(auth.land_account(character).unwrap().treasury, 0);
    let reopened = crate::auth::AuthService::new(path).unwrap();
    reopened
        .collect_land_taxes(month + 2, &[character])
        .unwrap();
    assert_eq!(reopened.land_account(character).unwrap().missed, 0);
    reopened
        .collect_land_taxes(month + 4, &[character])
        .unwrap();
    let state = reopened.land_account(character).unwrap();
    assert_eq!((state.missed, state.recovery_cost()), (2, 6000));
    let mut save = game.get_player_save_data(&pid("Settler")).await.unwrap();
    save.gold = 10_000;
    let rows = auth.load_inventory(character).unwrap();
    let (gold, state) = auth
        .transfer_land_gold(save, &rows, 4000, true)
        .unwrap()
        .unwrap();
    assert_eq!((gold, state.treasury, state.missed), (6000, 4000, 2));
    let mut failed_save = game.get_player_save_data(&pid("Settler")).await.unwrap();
    failed_save.gold = gold;
    conn.execute_batch("CREATE TRIGGER reject_recovery_inventory BEFORE INSERT ON character_items BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    assert!(auth
        .transfer_land_gold(failed_save, &rows, 2000, true)
        .is_err());
    assert_eq!(auth.land_account(character).unwrap().treasury, 4000);
    assert_eq!(
        auth.gold_sinks(crate::auth::unix_now() + 3600, 24)
            .unwrap()
            .total_gold,
        2000
    );
    conn.execute_batch("DROP TRIGGER reject_recovery_inventory;")
        .unwrap();
    let mut save = game.get_player_save_data(&pid("Settler")).await.unwrap();
    save.gold = gold;
    let (_, state) = auth
        .transfer_land_gold(save, &rows, 2000, true)
        .unwrap()
        .unwrap();
    assert_eq!((state.treasury, state.missed, state.free_months), (0, 0, 1));
    auth.collect_land_taxes(month + 5, &[character]).unwrap();
    assert_eq!(auth.land_account(character).unwrap().missed, 0);
    auth.collect_land_taxes(month + 6, &[character]).unwrap();
    assert_eq!(auth.land_account(character).unwrap().missed, 1);
    let sinks = reopened
        .gold_sinks(crate::auth::unix_now() + 3600, 24)
        .unwrap();
    assert_eq!(sinks.total_gold, 8000);
    assert_eq!(sinks.entries.len(), 2);
    assert_eq!(sinks.entries[0].sink, GoldSink::LandRecovery);
    assert_eq!(
        (sinks.entries[0].quantity, sinks.entries[0].gold),
        (1, 6000)
    );
    assert_eq!(sinks.entries[1].sink, GoldSink::LandTax);
    assert_eq!(
        (sinks.entries[1].quantity, sinks.entries[1].gold),
        (1, 2000)
    );
}

#[tokio::test]
async fn land_tax_inactive_owner_misses_payment_despite_funded_account() {
    let game = make_test_game_state("land_tax_inactive");
    let (auth, path) = make_test_auth_with_path("land_tax_inactive");
    let account = auth.login_google("land-tax-inactive").unwrap();
    let (character, _) = land_owner(&game, &auth, &account, "Settler").await;
    claim_at(&game, &auth, "Settler", 1, 1.0, 1.0).await;
    let conn = rusqlite::Connection::open(path).unwrap();
    let month: i64 = conn
        .query_row("SELECT month FROM land_tax_periods", [], |row| row.get(0))
        .unwrap();
    conn.execute("UPDATE land_estates SET treasury=100000, free_months=0", [])
        .unwrap();
    conn.execute(
        "UPDATE characters SET last_seen_at=0 WHERE id=?1",
        [character],
    )
    .unwrap();
    auth.collect_land_taxes(month + 1, &[]).unwrap();
    let state = auth.land_account(character).unwrap();
    assert_eq!((state.treasury, state.missed), (100000, 1));
    assert!(auth
        .gold_sinks(crate::auth::unix_now() + 3600, 24)
        .unwrap()
        .entries
        .is_empty());
}
