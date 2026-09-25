use super::*;
use onlinerpg_shared::messages::BagLineItem;
use onlinerpg_terrain::land::{plot_addr, LandGrade, REGION_PLOTS};

async fn storage_owner(game: &GameState, auth: &crate::auth::AuthService, name: &str) -> i64 {
    let account = auth.login_google(name).unwrap();
    let character = create_test_character(auth, &account, name);
    let mut player = make_player(name, 1.5, 1.5);
    player.position.y = 5.05;
    player.level = 10;
    game.add_player(player).await;
    game.register_player_character(
        &pid(name),
        character.id,
        onlinerpg_shared::xp::xp_for_level(10),
        attrs_with_cha(12),
        0,
        None,
    )
    .await;
    game.inventories.write().await.insert(
        pid(name),
        PlayerInventory {
            bag: vec![
                bag_item(1, "land_deed", 1),
                bag_item(2, "storage_chest", 1),
                bag_item(3, "apple", 3),
                bag_item(4, "worn_torch", 1),
            ],
            ..Default::default()
        },
    );
    game.terrain_io
        .write_land_grades(0, 0, &vec![LandGrade::Homestead as u8; REGION_PLOTS])
        .await
        .unwrap();
    game.claim_land(
        &pid(name),
        1,
        super::super::land::plot_key(plot_addr(1.5, 1.5)),
        auth,
    )
    .await;
    character.id
}

fn item_quantity(inventory: &PlayerInventory, item_def_id: &str) -> u32 {
    inventory
        .bag
        .iter()
        .filter(|item| item.item_def_id == item_def_id)
        .map(|item| item.quantity)
        .sum()
}

#[tokio::test]
async fn moving_storage_preserves_contents_and_updates_collision() {
    use onlinerpg_shared::pathfinding::is_cell_sealed;
    let game = make_flat_world_game_state("estate_furniture_move");
    let auth = make_test_auth("estate_furniture_move");
    let owner_id = storage_owner(&game, &auth, "Mover").await;
    let id = pid("Mover");
    let origin = Position {
        x: 2.5,
        y: 5.0,
        z: 2.5,
    };
    game.place_estate_chest(&id, 2, origin, 0.0, 0, &auth).await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    game.transfer_estate_items(
        &id,
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 2,
        }],
        vec![],
        0,
        &auth,
    )
    .await;
    let stored = auth
        .estate_chest_state(chest.id, owner_id)
        .unwrap()
        .unwrap();
    let inventory = game.get_player_inventory(&id).await.unwrap();
    let mut rx = game.register_direct_channel(&id).await;
    game.start_estate_furniture_move(&id, chest.id, &auth).await;
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::EstateFurnitureMoveMode { furniture, plots }
        if furniture.id == chest.id && furniture.revision == 1 && !plots.is_empty()
    )));
    assert_eq!(auth.load_estate_chests().unwrap()[0].position, origin);
    assert!(is_cell_sealed(
        &game.passability_read(),
        origin.x,
        origin.z,
        0,
        Some(5.05)
    ));
    game.move_estate_furniture(
        &id,
        EstateFurnitureMove {
            furniture_id: chest.id,
            expected_revision: 1,
            position: origin,
            rotation_deg: 90.0,
            floor_level: 0,
        },
        &auth,
    )
    .await;
    let messages = drain(&mut rx);
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::EstateChestEditResult { error: None }
        )),
        "{messages:?}"
    );
    let rotated = auth.load_estate_chests().unwrap().remove(0);
    assert_eq!(
        rotated.rotation_deg, 90.0,
        "ignore the furniture's own collision"
    );
    assert_eq!(rotated.revision, 2);
    let target = Position {
        x: 10.5,
        y: 5.0,
        z: 10.5,
    };
    game.move_estate_furniture(
        &id,
        EstateFurnitureMove {
            furniture_id: chest.id,
            expected_revision: 2,
            position: target,
            rotation_deg: 180.0,
            floor_level: 0,
        },
        &auth,
    )
    .await;
    let moved = auth.load_estate_chests().unwrap().remove(0);
    assert_eq!(moved.id, chest.id);
    assert_eq!(moved.position, target);
    assert_eq!(moved.rotation_deg, 180.0);
    assert_eq!(moved.revision, 3);
    let state = auth
        .estate_chest_state(chest.id, owner_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(state.items).unwrap(),
        serde_json::to_value(stored.items).unwrap()
    );
    assert_eq!(
        serde_json::to_value(game.get_player_inventory(&id).await.unwrap()).unwrap(),
        serde_json::to_value(inventory).unwrap()
    );
    assert!(!is_cell_sealed(
        &game.passability_read(),
        origin.x,
        origin.z,
        0,
        Some(5.05)
    ));
    assert!(is_cell_sealed(
        &game.passability_read(),
        target.x,
        target.z,
        0,
        Some(5.05)
    ));
}

#[tokio::test]
async fn rejected_furniture_moves_keep_the_original_placement_and_collision() {
    use onlinerpg_shared::pathfinding::is_cell_sealed;
    let game = make_flat_world_game_state("estate_move_rejection");
    let (auth, path) = make_test_auth_with_path("estate_move_rejection");
    storage_owner(&game, &auth, "Mover").await;
    let id = pid("Mover");
    let origin = Position {
        x: 2.5,
        y: 5.0,
        z: 2.5,
    };
    game.place_estate_chest(&id, 2, origin, 0.0, 0, &auth).await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    game.give_item(&id, "storage_chest").await;
    let item = game
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .into_iter()
        .find(|item| item.item_def_id == "storage_chest")
        .unwrap();
    game.place_estate_chest(
        &id,
        item.instance_id,
        Position {
            x: 5.5,
            y: 5.0,
            z: 5.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let mut rx = game.register_direct_channel(&id).await;
    for (revision, x, z, floor) in [
        (0, 5.5, 5.5, 0),
        (0, 40.0, 40.0, 0),
        (0, 3.5, 3.5, 1),
        (1, 3.5, 3.5, 0),
        (0, f32::NAN, 3.5, 0),
    ] {
        game.move_estate_furniture(
            &id,
            EstateFurnitureMove {
                furniture_id: chest.id,
                expected_revision: revision,
                position: Position { x, y: 5.0, z },
                rotation_deg: 90.0,
                floor_level: floor,
            },
            &auth,
        )
        .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::EstateChestEditResult { error: Some(_) }
        )));
        let saved = auth
            .load_estate_chests()
            .unwrap()
            .into_iter()
            .find(|p| p.id == chest.id)
            .unwrap();
        assert_eq!(saved.position, origin);
        assert_eq!(saved.revision, 0);
        assert!(is_cell_sealed(
            &game.passability_read(),
            origin.x,
            origin.z,
            0,
            Some(5.05)
        ));
    }
    storage_owner(&game, &auth, "Visitor").await;
    game.move_estate_furniture(
        &pid("Visitor"),
        EstateFurnitureMove {
            furniture_id: chest.id,
            expected_revision: 0,
            position: origin,
            rotation_deg: 90.0,
            floor_level: 0,
        },
        &auth,
    )
    .await;
    assert!(auth
        .move_estate_furniture(chest.id, chest.owner_id + 1, 0, origin, 90.0, 0)
        .unwrap()
        .is_err());
    let db = rusqlite::Connection::open(path).unwrap();
    db.execute(
        "UPDATE land_estates SET missed=1 WHERE id=?1",
        [chest.estate_id],
    )
    .unwrap();
    assert!(auth
        .move_estate_furniture(chest.id, chest.owner_id, 0, origin, 90.0, 0)
        .unwrap()
        .is_err());
    let saved = auth
        .load_estate_chests()
        .unwrap()
        .into_iter()
        .find(|p| p.id == chest.id)
        .unwrap();
    assert_eq!(saved.position, origin);
    assert_eq!(saved.rotation_deg, 0.0);
}

#[tokio::test]
async fn estate_furniture_editing_requires_the_same_floor_but_not_proximity() {
    for item_id in ["furniture_bed", "storage_chest"] {
        let test_name = format!("estate_edit_distance_{item_id}");
        let game = make_flat_world_game_state(&test_name);
        let auth = make_test_auth(&test_name);
        storage_owner(&game, &auth, "Editor").await;
        let owner = pid("Editor");
        game.inventories.write().await.get_mut(&owner).unwrap().bag = vec![bag_item(2, item_id, 1)];
        let origin = Position {
            x: 20.0,
            y: 5.0,
            z: 20.0,
        };
        game.place_estate_chest(&owner, 2, origin, 0.0, 0, &auth)
            .await;
        let furniture = auth.load_estate_chests().unwrap().remove(0);
        let mut rx = game.register_direct_channel(&owner).await;

        game.start_estate_furniture_move(&owner, furniture.id, &auth)
            .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(message,
            ServerMessage::EstateFurnitureMoveMode { furniture: selected, .. }
                if selected.id == furniture.id
        )));

        game.players
            .write()
            .await
            .get_mut(&owner)
            .unwrap()
            .floor_level = 1;
        game.start_estate_furniture_move(&owner, furniture.id, &auth)
            .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::EstateChestEditResult { error: Some(_) }
        )));
        game.move_estate_furniture(
            &owner,
            EstateFurnitureMove {
                furniture_id: furniture.id,
                expected_revision: 0,
                position: origin,
                rotation_deg: 90.0,
                floor_level: 0,
            },
            &auth,
        )
        .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::EstateChestEditResult { error: Some(_) }
        )));
        game.recover_estate_chest(&owner, furniture.id, &auth).await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::EstateChestEditResult { error: Some(_) }
        )));
        assert_eq!(auth.load_estate_chests().unwrap()[0].revision, 0);
        game.players
            .write()
            .await
            .get_mut(&owner)
            .unwrap()
            .floor_level = 0;

        if item_id == "storage_chest" {
            game.open_estate_chest(&owner, furniture.id, &auth).await;
            assert!(drain(&mut rx).iter().any(|message| matches!(
                message,
                ServerMessage::EstateChestState {
                    state: None,
                    error: Some(_)
                }
            )));
        }

        for revision in 0..2 {
            let target = Position {
                x: 22.0 + revision as f32,
                y: 5.0,
                z: 22.0,
            };
            let rotation_deg = 90.0 * (revision + 1) as f32;
            game.move_estate_furniture(
                &owner,
                EstateFurnitureMove {
                    furniture_id: furniture.id,
                    expected_revision: revision,
                    position: target,
                    rotation_deg,
                    floor_level: 0,
                },
                &auth,
            )
            .await;
            let messages = drain(&mut rx);
            assert!(
                messages.iter().any(|message| matches!(
                    message,
                    ServerMessage::EstateChestEditResult { error: None }
                )),
                "{messages:?}"
            );
            let saved = auth.load_estate_chests().unwrap().remove(0);
            assert_eq!(saved.position, target);
            assert_eq!(saved.rotation_deg, rotation_deg);
            assert_eq!(saved.revision, revision + 1);
        }

        game.recover_estate_chest(&owner, furniture.id, &auth).await;
        assert!(auth.load_estate_chests().unwrap().is_empty());
        assert_eq!(
            item_quantity(&game.get_player_inventory(&owner).await.unwrap(), item_id),
            1
        );
    }
}

#[tokio::test]
async fn estate_furniture_moves_preserve_five_centimeter_adjustments() {
    for item_id in ["furniture_bed", "storage_chest"] {
        let test_name = format!("estate_fine_movement_{item_id}");
        let game = make_flat_world_game_state(&test_name);
        let auth = make_test_auth(&test_name);
        storage_owner(&game, &auth, "Editor").await;
        let owner = pid("Editor");
        game.inventories.write().await.get_mut(&owner).unwrap().bag = vec![bag_item(2, item_id, 1)];
        game.place_estate_chest(
            &owner,
            2,
            Position {
                x: 5.0,
                y: 5.0,
                z: 5.0,
            },
            0.0,
            0,
            &auth,
        )
        .await;
        let furniture = auth.load_estate_chests().unwrap().remove(0);
        let mut rx = game.register_direct_channel(&owner).await;

        for (revision, (x, z)) in [(5.05, 5.0), (5.05, 4.95), (5.0, 4.95), (5.0, 5.0)]
            .into_iter()
            .enumerate()
        {
            game.move_estate_furniture(
                &owner,
                EstateFurnitureMove {
                    furniture_id: furniture.id,
                    expected_revision: revision as u64,
                    position: Position { x, y: 5.0, z },
                    rotation_deg: 0.0,
                    floor_level: 0,
                },
                &auth,
            )
            .await;
            let messages = drain(&mut rx);
            assert!(
                messages.iter().any(|message| matches!(
                    message,
                    ServerMessage::EstateChestEditResult { error: None }
                )),
                "{messages:?}"
            );
            let saved = auth.load_estate_chests().unwrap().remove(0);
            assert!((saved.position.x - x).abs() < 0.0001);
            assert!((saved.position.z - z).abs() < 0.0001);
            assert_eq!(saved.position.y, 5.0);
            assert_eq!(saved.revision, revision as u64 + 1);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn estate_beds_support_exclusive_sleep_and_cannot_be_recovered_while_occupied() {
    for item_id in ["furniture_bed", "furniture_rustic_bed"] {
        let game = make_flat_world_game_state(item_id);
        let auth = make_test_auth(item_id);
        storage_owner(&game, &auth, "Sleeper").await;
        let owner = pid("Sleeper");
        game.inventories.write().await.get_mut(&owner).unwrap().bag = vec![bag_item(2, item_id, 1)];
        game.place_estate_chest(
            &owner,
            2,
            Position {
                x: 3.0,
                y: 5.0,
                z: 3.0,
            },
            90.0,
            0,
            &auth,
        )
        .await;
        let bed = auth.load_estate_chests().unwrap().remove(0);
        let object_id = bed.id as u32;
        assert_eq!(bed.rotation_deg, 90.0);

        let mut world_sleeper = make_player("WorldSleeper", 3.0, 3.0);
        world_sleeper.object_type = Some("bed".into());
        world_sleeper.object_id = Some(object_id);
        game.add_player(world_sleeper).await;
        game.set_player_interaction(&owner, Some(item_id.into()), Some(object_id))
            .await;
        assert_eq!(
            game.players.read().await[&owner].object_type.as_deref(),
            Some(item_id)
        );
        game.register_hunger(&owner, 500).await;
        game.players
            .write()
            .await
            .get_mut(&owner)
            .unwrap()
            .max_health = 100;
        let hp = game.players.read().await[&owner].health;
        let player = game.players.read().await[&owner].clone();
        game.register_mana(&player, 10, Some(0)).await;
        tokio::time::advance(std::time::Duration::from_secs(26)).await;
        game.tick_regeneration().await;
        assert_eq!(game.players.read().await[&owner].health, hp + 6);
        assert_eq!(game.mana.read().await[&owner].mana, 4);

        let guest = pid("Guest");
        let mut guest_player = make_player("Guest", 3.0, 3.0);
        guest_player.position.y = 5.0;
        game.add_player(guest_player).await;
        let mut guest_rx = game.register_direct_channel(&guest).await;
        game.set_player_interaction(&guest, Some(item_id.into()), Some(object_id))
            .await;
        assert!(game.players.read().await[&guest].object_type.is_none());
        assert!(drain(&mut guest_rx).iter().any(|message| matches!(
            message, ServerMessage::InteractionRejected { reason } if reason == "occupied"
        )));

        game.move_estate_furniture(
            &owner,
            EstateFurnitureMove {
                furniture_id: bed.id,
                expected_revision: bed.revision,
                position: Position {
                    x: 4.0,
                    y: 5.0,
                    z: 4.0,
                },
                rotation_deg: 180.0,
                floor_level: 0,
            },
            &auth,
        )
        .await;
        assert_eq!(auth.load_estate_chests().unwrap()[0].position, bed.position);
        game.recover_estate_chest(&owner, bed.id, &auth).await;
        assert_eq!(auth.load_estate_chests().unwrap().len(), 1);
        game.set_player_interaction(&owner, None, None).await;
        game.set_player_interaction(&guest, Some(item_id.into()), Some(object_id))
            .await;
        assert_eq!(game.players.read().await[&guest].object_id, Some(object_id));
        game.recover_estate_chest(&owner, bed.id, &auth).await;
        assert_eq!(auth.load_estate_chests().unwrap().len(), 1);

        game.set_player_interaction(&guest, None, None).await;
        game.recover_estate_chest(&owner, bed.id, &auth).await;
        assert!(auth.load_estate_chests().unwrap().is_empty());
        assert_eq!(
            item_quantity(&game.get_player_inventory(&owner).await.unwrap(), item_id),
            1
        );
        game.set_player_interaction(&owner, Some(item_id.into()), Some(object_id))
            .await;
        assert!(game.players.read().await[&owner].object_type.is_none());
    }
}

#[tokio::test]
async fn estate_bed_interactions_validate_type_distance_floor_and_health() {
    let game = make_flat_world_game_state("estate_bed_validation");
    let auth = make_test_auth("estate_bed_validation");
    storage_owner(&game, &auth, "Sleeper").await;
    let owner = pid("Sleeper");
    game.inventories.write().await.get_mut(&owner).unwrap().bag =
        vec![bag_item(2, "furniture_bed", 1)];
    game.place_estate_chest(
        &owner,
        2,
        Position {
            x: 3.0,
            y: 5.0,
            z: 3.0,
        },
        270.0,
        0,
        &auth,
    )
    .await;
    let bed = auth.load_estate_chests().unwrap().remove(0);
    let mut rx = game.register_direct_channel(&owner).await;
    for (item_id, object_id, x, floor, health) in [
        ("furniture_rustic_bed", bed.id as u32, 3.0, 0, 100),
        ("furniture_table", bed.id as u32, 3.0, 0, 100),
        ("furniture_bed", bed.id as u32 + 1, 3.0, 0, 100),
        ("furniture_bed", bed.id as u32, 20.0, 0, 100),
        ("furniture_bed", bed.id as u32, 3.0, 1, 100),
        ("furniture_bed", bed.id as u32, 3.0, 0, 0),
    ] {
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&owner).unwrap();
            player.position.x = x;
            player.floor_level = floor;
            player.health = health;
        }
        game.set_player_interaction(&owner, Some(item_id.into()), Some(object_id))
            .await;
        assert!(game.players.read().await[&owner].object_type.is_none());
        assert!(drain(&mut rx)
            .iter()
            .any(|message| matches!(message, ServerMessage::InteractionRejected { .. })));
    }
}

async fn add_furniture_clerk(game: &GameState) {
    let mut clerk = make_player("Grida", -1452.0, 4777.0);
    clerk.position.y = 1.0;
    clerk.is_official_npc = true;
    game.add_player(clerk).await;
}

#[tokio::test]
async fn furniture_selection_only_notifies_the_available_clerk_about_nearby_tip_items() {
    let game = make_flat_world_game_state("furniture_selection");
    add_furniture_clerk(&game).await;
    let mut clerk_rx = game.register_direct_channel(&pid("Grida")).await;
    let mut shopper = make_player("Shopper", -1451.0, 4782.0);
    shopper.position.y = 1.0;
    game.add_player(shopper).await;
    let id = pid("Shopper");
    let mut shopper_rx = game.register_direct_channel(&id).await;
    let placements = serde_json::json!({"placements": [
        {"id":94,"type":"chest_animated","x":-1453.0,"y":1.0,"z":4783.0,"floorLevel":0},
        {"id":85,"type":"bed","x":-1450.0,"y":1.0,"z":4783.0,"floorLevel":0},
        {"id":98,"type":"rustic_bed","x":-1449.0,"y":1.0,"z":4783.0,"floorLevel":0},
        {"id":103,"type":"scroll","x":-1451.0,"y":1.0,"z":4783.0,"floorLevel":0}
    ]});
    game.terrain_io
        .write_object(-2, 4, &placements)
        .await
        .unwrap();
    for (display, item) in [
        (94, "storage_chest"),
        (85, "furniture_bed"),
        (98, "furniture_rustic_bed"),
    ] {
        assert!(game.notify_furniture_selection(&id, display).await);
        assert!(drain(&mut clerk_rx).iter().any(|m| matches!(m,
            ServerMessage::FurnitureSelectionNotice { player_id, player_name, item_def_id }
            if *player_id == id && player_name == "Shopper" && item_def_id == item
        )));
        assert!(drain(&mut shopper_rx).is_empty());
    }
    for display in [84, 103, 999] {
        assert!(!game.notify_furniture_selection(&id, display).await);
    }
    for (x, floor, health) in [(-1440.0, 0, 10), (-1451.0, 1, 10), (-1451.0, 0, 0)] {
        {
            let mut players = game.players.write().await;
            let shopper = players.get_mut(&id).unwrap();
            shopper.position.x = x;
            shopper.floor_level = floor;
            shopper.health = health;
        }
        assert!(!game.notify_furniture_selection(&id, 94).await);
    }
    game.players.write().await.get_mut(&id).unwrap().health = 10;
    for (official, x) in [(false, -1452.0), (true, -1500.0)] {
        {
            let mut players = game.players.write().await;
            let clerk = players.get_mut(&pid("Grida")).unwrap();
            clerk.is_official_npc = official;
            clerk.position.x = x;
        }
        assert!(!game.notify_furniture_selection(&id, 94).await);
    }
    game.players
        .write()
        .await
        .get_mut(&pid("Grida"))
        .unwrap()
        .position
        .x = -1452.0;
    game.set_npc_schedule(
        "Grida",
        vec![onlinerpg_shared::schedule::ScheduleEntry {
            at: "0:00".into(),
            action: Some("bed".into()),
            ..Default::default()
        }],
    );
    assert!(!game.notify_furniture_selection(&id, 94).await);
    game.set_npc_schedule("Grida", vec![]);
    game.terrain_io
        .write_object(-2, 4, &serde_json::json!({"placements": []}))
        .await
        .unwrap();
    assert!(!game.notify_furniture_selection(&id, 94).await);
    assert!(drain(&mut clerk_rx).is_empty());
}

#[tokio::test]
async fn furniture_checkout_and_decoration_placement_are_persistent() {
    use onlinerpg_shared::furniture_shop::FurnitureOrderLine;
    let game = make_flat_world_game_state("furniture_checkout");
    let auth = make_test_auth("furniture_checkout");
    let character_id = storage_owner(&game, &auth, "Decorator").await;
    let id = pid("Decorator");
    game.player_gold.write().await.insert(id, 5000);
    game.terrain_io.write_object(-2, 4, &serde_json::json!({"placements": [
        {"id":103,"type":"scroll","x":-1450.6,"y":1.8,"z":4781.3,"rotation":270,"floorLevel":0}
    ]})).await.unwrap();
    let order = || {
        vec![FurnitureOrderLine {
            display_id: 103,
            quantity: 2,
        }]
    };
    game.checkout_furniture(&id, order(), 5000, 400, &auth)
        .await;
    assert_eq!(
        game.get_player_gold(&id).await,
        5000,
        "remote purchases must fail"
    );
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: -1448.0,
        y: 1.0,
        z: 4777.0,
    };
    game.checkout_furniture(&id, order(), 5000, 400, &auth)
        .await;
    assert_eq!(
        game.get_player_gold(&id).await,
        5000,
        "checkout needs Grida"
    );
    add_furniture_clerk(&game).await;
    game.checkout_furniture(&id, order(), 5000, 400, &auth)
        .await;
    assert_eq!(game.get_player_gold(&id).await, 4600);
    let inventory = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(item_quantity(&inventory, "furniture_scroll"), 2);
    assert_eq!(item_quantity(&inventory, "scroll_of_return"), 0);
    assert_eq!(
        auth.load_inventory(character_id)
            .unwrap()
            .iter()
            .filter(|row| row.item_def_id == "furniture_scroll")
            .count(),
        2
    );
    game.checkout_furniture(&id, order(), 5000, 400, &auth)
        .await;
    assert_eq!(
        game.get_player_gold(&id).await,
        4600,
        "a repeated payment must not charge twice"
    );

    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: 1.5,
        y: 5.05,
        z: 1.5,
    };
    let instance_id = inventory
        .bag
        .iter()
        .find(|item| item.item_def_id == "furniture_scroll")
        .unwrap()
        .instance_id;
    game.place_estate_chest(
        &id,
        instance_id,
        Position {
            x: 3.0,
            y: 5.8,
            z: 3.0,
        },
        15.0,
        0,
        &auth,
    )
    .await;
    let placed = auth.load_estate_chests().unwrap().remove(0);
    assert_eq!(placed.item_def_id, "furniture_scroll");
    assert!((placed.position.y - 5.8).abs() < 0.001);
    assert!(auth
        .estate_chest_state(placed.id, character_id)
        .unwrap()
        .is_err());
    game.recover_estate_chest(&id, placed.id, &auth).await;
    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert_eq!(
        item_quantity(
            &game.get_player_inventory(&id).await.unwrap(),
            "furniture_scroll"
        ),
        2
    );
}

#[tokio::test]
async fn furniture_checkout_sells_a_functional_storage_chest() {
    use onlinerpg_shared::furniture_shop::FurnitureOrderLine;
    let game = make_flat_world_game_state("showroom_storage_chest");
    let auth = make_test_auth("showroom_storage_chest");
    let character_id = storage_owner(&game, &auth, "Keeper").await;
    let id = pid("Keeper");
    add_furniture_clerk(&game).await;
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .retain(|item| item.item_def_id != "storage_chest");
    game.player_gold.write().await.insert(id, 5000);
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: -1453.0,
        y: 1.0,
        z: 4778.0,
    };
    let mut displays = serde_json::json!({"placements": [
        {"id":94,"type":"chest","x":-1453.0351,"y":0.95,"z":4783.4509,"rotation":0,"floorLevel":0}
    ]});
    let order = || {
        vec![FurnitureOrderLine {
            display_id: 94,
            quantity: 1,
        }]
    };
    game.terrain_io
        .write_object(-2, 4, &displays)
        .await
        .unwrap();
    game.checkout_furniture(&id, order(), 5000, 1200, &auth)
        .await;
    assert_eq!(game.get_player_gold(&id).await, 5000);
    displays["placements"][0]["type"] = "chest_animated".into();
    game.terrain_io
        .write_object(-2, 4, &displays)
        .await
        .unwrap();
    game.checkout_furniture(&id, order(), 5000, 1200, &auth)
        .await;
    assert_eq!(game.get_player_gold(&id).await, 3800);
    let inventory = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(item_quantity(&inventory, "storage_chest"), 1);
    assert_eq!(item_quantity(&inventory, "furniture_chest"), 0);
    let instance_id = inventory
        .bag
        .iter()
        .find(|item| item.item_def_id == "storage_chest")
        .unwrap()
        .instance_id;
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: 1.5,
        y: 5.05,
        z: 1.5,
    };
    game.place_estate_chest(
        &id,
        instance_id,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    assert_eq!(chest.item_def_id, "storage_chest");
    game.transfer_estate_items(
        &id,
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 2,
        }],
        vec![],
        chest.revision,
        &auth,
    )
    .await;
    let stored = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.max_weight, 500.0);
    assert_eq!(stored.items.len(), 1);
    assert_eq!(stored.items[0].item_def_id, "apple");
    assert_eq!(stored.items[0].quantity, 2);
}

#[tokio::test]
async fn furniture_checkout_can_use_grida_sale_proceeds() {
    use onlinerpg_shared::furniture_shop::FurnitureOrderLine;
    let game = make_flat_world_game_state("furniture_sale_proceeds");
    let auth = make_test_auth("furniture_sale_proceeds");
    let character_id = storage_owner(&game, &auth, "Shopper").await;
    let id = pid("Shopper");
    add_furniture_clerk(&game).await;
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: -1453.0,
        y: 1.0,
        z: 4778.0,
    };
    game.player_gold.write().await.insert(id, 0);
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(11, "iron_sword", 1)],
            ..Default::default()
        },
    );
    game.terrain_io.write_object(-2, 4, &serde_json::json!({"placements": [
        {"id":103,"type":"scroll","x":-1450.6,"y":1.8,"z":4781.3,"rotation":270,"floorLevel":0}
    ]})).await.unwrap();
    game.sell_items(
        &id,
        &pid("Grida"),
        vec![BagLineItem {
            instance_id: 11,
            qty: 1,
        }],
    )
    .await;
    assert_eq!(game.get_player_gold(&id).await, 4000);
    game.checkout_furniture(
        &id,
        vec![FurnitureOrderLine {
            display_id: 103,
            quantity: 2,
        }],
        4000,
        400,
        &auth,
    )
    .await;
    assert_eq!(game.get_player_gold(&id).await, 3600);
    let inventory = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(item_quantity(&inventory, "iron_sword"), 0);
    assert_eq!(item_quantity(&inventory, "furniture_scroll"), 2);
    let saved = auth.load_inventory(character_id).unwrap();
    assert!(saved.iter().all(|row| row.item_def_id != "iron_sword"));
    assert_eq!(
        saved
            .iter()
            .filter(|row| row.item_def_id == "furniture_scroll")
            .count(),
        2
    );
}

#[tokio::test]
async fn furniture_checkout_rejects_an_unavailable_clerk() {
    use onlinerpg_shared::furniture_shop::FurnitureOrderLine;
    let game = make_flat_world_game_state("furniture_clerk_unavailable");
    let auth = make_test_auth("furniture_clerk_unavailable");
    storage_owner(&game, &auth, "Shopper").await;
    let id = pid("Shopper");
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: -1453.0,
        y: 1.0,
        z: 4778.0,
    };
    game.player_gold.write().await.insert(id, 5000);
    add_furniture_clerk(&game).await;
    let mut rx = game.register_direct_channel(&id).await;
    for (official, floor, x, asleep, expected) in [
        (false, 0, -1452.0, false, "not available"),
        (true, 1, -1452.0, false, "another floor"),
        (true, 0, -1440.0, false, "Too far"),
        (true, 0, -1452.0, true, "asleep"),
    ] {
        {
            let mut players = game.players.write().await;
            let clerk = players.get_mut(&pid("Grida")).unwrap();
            clerk.is_official_npc = official;
            clerk.floor_level = floor;
            clerk.position.x = x;
        }
        game.set_npc_schedule(
            "Grida",
            vec![onlinerpg_shared::schedule::ScheduleEntry {
                at: "0:00".to_string(),
                action: asleep.then(|| "bed".to_string()),
                ..Default::default()
            }],
        );
        game.checkout_furniture(
            &id,
            vec![FurnitureOrderLine {
                display_id: 103,
                quantity: 1,
            }],
            5000,
            200,
            &auth,
        )
        .await;
        assert!(
            drain(&mut rx).iter().any(|message| matches!(message,
                ServerMessage::FurniturePurchaseResult { error: Some(error) }
                if error.contains(expected)
            )),
            "expected checkout error containing {expected}"
        );
        assert_eq!(game.get_player_gold(&id).await, 5000);
        assert_eq!(
            item_quantity(
                &game.get_player_inventory(&id).await.unwrap(),
                "furniture_scroll"
            ),
            0
        );
    }
}

#[tokio::test]
async fn furniture_checkout_failures_leave_gold_and_inventory_unchanged() {
    use onlinerpg_shared::furniture_shop::FurnitureOrderLine;
    let game = make_flat_world_game_state("furniture_checkout_failures");
    let auth = make_test_auth("furniture_checkout_failures");
    let character_id = storage_owner(&game, &auth, "Shopper").await;
    add_furniture_clerk(&game).await;
    let id = pid("Shopper");
    game.players.write().await.get_mut(&id).unwrap().position = Position {
        x: -1453.0,
        y: 1.0,
        z: 4778.0,
    };
    let display = serde_json::json!({"placements": [
        {"id":103,"type":"scroll","x":-1450.6,"y":1.8,"z":4781.3,"rotation":270,"floorLevel":0}
    ]});
    game.terrain_io.write_object(-2, 4, &display).await.unwrap();
    let original = game.get_player_inventory(&id).await.unwrap();
    for (balance, expected_total, overweight, missing_display) in [
        (100, 400, false, false),
        (5000, 1, false, false),
        (5000, 400, true, false),
        (5000, 400, false, true),
    ] {
        game.player_gold.write().await.insert(id, balance);
        let mut inventory = original.clone();
        if overweight {
            inventory.bag.push(bag_item(900, "campfire_kit", 10000));
        }
        game.inventories.write().await.insert(id, inventory.clone());
        if missing_display {
            game.terrain_io
                .write_object(-2, 4, &serde_json::json!({"placements": []}))
                .await
                .unwrap();
        }
        game.checkout_furniture(
            &id,
            vec![FurnitureOrderLine {
                display_id: 103,
                quantity: 2,
            }],
            balance,
            expected_total,
            &auth,
        )
        .await;
        assert_eq!(game.get_player_gold(&id).await, balance);
        assert_eq!(
            game.get_player_inventory(&id).await.unwrap().bag,
            inventory.bag
        );
        assert!(auth
            .load_inventory(character_id)
            .unwrap()
            .iter()
            .all(|row| row.item_def_id != "furniture_scroll"));
    }
}

#[tokio::test]
async fn decoration_sign_text_is_saved_and_only_editable_by_its_owner() {
    let game = make_flat_world_game_state("furniture_sign");
    let auth = make_test_auth("furniture_sign");
    storage_owner(&game, &auth, "Signmaker").await;
    let id = pid("Signmaker");
    assert!(game.give_item(&id, "furniture_shop_sign").await);
    let instance_id = game
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .iter()
        .find(|item| item.item_def_id == "furniture_shop_sign")
        .unwrap()
        .instance_id;
    game.place_estate_chest(
        &id,
        instance_id,
        Position {
            x: 20.5,
            y: 7.0,
            z: 20.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let placed = auth.load_estate_chests().unwrap().remove(0);
    game.set_estate_furniture_text(&id, placed.id, "우리 집\nWelcome".into(), &auth)
        .await;
    assert_eq!(
        auth.load_estate_chests().unwrap()[0].text.as_deref(),
        Some("우리 집\nWelcome")
    );
    assert!(!auth
        .set_estate_furniture_text(placed.id, placed.owner_id + 1, "wrong owner")
        .unwrap());
    game.players.write().await.get_mut(&id).unwrap().floor_level = 1;
    game.set_estate_furniture_text(&id, placed.id, "wrong floor".into(), &auth)
        .await;
    assert_eq!(
        auth.load_estate_chests().unwrap()[0].text.as_deref(),
        Some("우리 집\nWelcome")
    );
    game.players.write().await.get_mut(&id).unwrap().floor_level = 0;
    game.set_estate_furniture_text(&id, placed.id, "x".repeat(121), &auth)
        .await;
    assert_eq!(
        auth.load_estate_chests().unwrap()[0].text.as_deref(),
        Some("우리 집\nWelcome")
    );
    let target = Position {
        x: 21.5,
        y: 7.0,
        z: 21.5,
    };
    game.move_estate_furniture(
        &id,
        EstateFurnitureMove {
            furniture_id: placed.id,
            expected_revision: 1,
            position: target,
            rotation_deg: 90.0,
            floor_level: 0,
        },
        &auth,
    )
    .await;
    let moved = auth.load_estate_chests().unwrap().remove(0);
    assert_eq!(moved.id, placed.id);
    assert_eq!(moved.position, target);
    assert_eq!(moved.text.as_deref(), Some("우리 집\nWelcome"));
    assert_eq!(moved.rotation_deg, 90.0);
}

#[tokio::test]
async fn item_locks_survive_storage_without_merging_with_unlocked_stacks() {
    let game = make_flat_world_game_state("item_lock_storage");
    let auth = make_test_auth("item_lock_storage");
    let character_id = storage_owner(&game, &auth, "Lockkeeper").await;
    let id = pid("Lockkeeper");
    game.place_estate_chest(
        &id,
        2,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    game.set_item_locked(&id, 3, true).await;
    game.transfer_estate_items(
        &id,
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 2,
        }],
        vec![],
        0,
        &auth,
    )
    .await;
    game.set_item_locked(&id, 3, false).await;
    game.transfer_estate_items(
        &id,
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 1,
        }],
        vec![],
        1,
        &auth,
    )
    .await;
    let state = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(state.items.len(), 2);
    assert_eq!(state.items.iter().find(|i| i.locked).unwrap().quantity, 2);
    assert_eq!(state.items.iter().find(|i| !i.locked).unwrap().quantity, 1);
    let withdrawals = state
        .items
        .iter()
        .map(|item| BagLineItem {
            instance_id: item.instance_id,
            qty: item.quantity,
        })
        .collect();
    game.transfer_estate_items(&id, chest.id, vec![], withdrawals, state.revision, &auth)
        .await;
    let inv = game.get_player_inventory(&id).await.unwrap();
    let apples: Vec<_> = inv
        .bag
        .iter()
        .filter(|item| item.item_def_id == "apple")
        .collect();
    assert_eq!(apples.len(), 2);
    assert_eq!(apples.iter().find(|i| i.locked).unwrap().quantity, 2);
    assert_eq!(apples.iter().find(|i| !i.locked).unwrap().quantity, 1);
    assert!(auth
        .load_inventory(character_id)
        .unwrap()
        .iter()
        .any(|i| i.locked && i.quantity == 2));
}

#[tokio::test]
async fn estate_storage_can_be_placed_anywhere_on_owned_estate() {
    let game = make_flat_world_game_state("estate_storage_remote_placement");
    let (auth, _) = make_test_auth_with_path("estate_storage_remote_placement");
    storage_owner(&game, &auth, "RemoteKeeper").await;

    game.place_estate_chest(
        &pid("RemoteKeeper"),
        2,
        Position {
            x: 30.5,
            y: 5.0,
            z: 30.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;

    let chests = auth.load_estate_chests().unwrap();
    assert_eq!(chests.len(), 1);
    assert_eq!(chests[0].position.x, 30.5);
    assert_eq!(chests[0].position.z, 30.5);
}

#[tokio::test]
async fn estate_can_hold_multiple_storage_chests() {
    let game = make_flat_world_game_state("estate_storage_multiple");
    let (auth, _) = make_test_auth_with_path("estate_storage_multiple");
    storage_owner(&game, &auth, "MultiKeeper").await;

    game.place_estate_chest(
        &pid("MultiKeeper"),
        2,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    game.inventories
        .write()
        .await
        .get_mut(&pid("MultiKeeper"))
        .unwrap()
        .bag
        .push(bag_item(5, "storage_chest", 1));
    game.place_estate_chest(
        &pid("MultiKeeper"),
        5,
        Position {
            x: 10.5,
            y: 5.0,
            z: 10.5,
        },
        90.0,
        0,
        &auth,
    )
    .await;

    assert_eq!(auth.load_estate_chests().unwrap().len(), 2);

    game.inventories
        .write()
        .await
        .get_mut(&pid("MultiKeeper"))
        .unwrap()
        .bag
        .push(bag_item(6, "storage_chest", 1));
    game.place_estate_chest(
        &pid("MultiKeeper"),
        6,
        Position {
            x: 10.5,
            y: 5.0,
            z: 10.5,
        },
        90.0,
        0,
        &auth,
    )
    .await;

    assert_eq!(auth.load_estate_chests().unwrap().len(), 2);
    assert_eq!(
        item_quantity(
            &game
                .get_player_inventory(&pid("MultiKeeper"))
                .await
                .unwrap(),
            "storage_chest"
        ),
        1
    );
}

#[tokio::test]
async fn remote_transfer_error_does_not_reveal_chest_contents() {
    let game = make_flat_world_game_state("estate_storage_remote_transfer");
    let (auth, _) = make_test_auth_with_path("estate_storage_remote_transfer");
    storage_owner(&game, &auth, "RemoteKeeper").await;

    game.place_estate_chest(
        &pid("RemoteKeeper"),
        2,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    let mut rx = game.register_direct_channel(&pid("RemoteKeeper")).await;
    game.players
        .write()
        .await
        .get_mut(&pid("RemoteKeeper"))
        .unwrap()
        .position = Position {
        x: 100.0,
        y: 5.0,
        z: 100.0,
    };

    game.transfer_estate_items(
        &pid("RemoteKeeper"),
        chest.id,
        vec![],
        vec![],
        chest.revision,
        &auth,
    )
    .await;

    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::EstateChestState {
            state: None,
            error: Some(reason),
        } if reason.contains("closer")
    )));
    assert!(!messages.iter().any(|message| matches!(
        message,
        ServerMessage::EstateChestState { state: Some(_), .. }
    )));
}

#[tokio::test]
async fn estate_storage_round_trip_is_persistent_and_recovery_requires_empty() {
    let game = make_flat_world_game_state("estate_storage_round_trip");
    let (auth, path) = make_test_auth_with_path("estate_storage_round_trip");
    let character_id = storage_owner(&game, &auth, "Keeper").await;

    game.place_estate_chest(
        &pid("Keeper"),
        2,
        Position {
            x: 2.5,
            y: 99.0,
            z: 2.5,
        },
        90.0,
        0,
        &auth,
    )
    .await;

    let chest = auth.load_estate_chests().unwrap().remove(0);
    assert!((chest.position.y - 5.0).abs() < 0.01);
    assert_eq!(chest.owner_id, character_id);
    assert_eq!(chest.item_def_id, "storage_chest");
    assert_eq!(
        item_quantity(
            &game.get_player_inventory(&pid("Keeper")).await.unwrap(),
            "storage_chest"
        ),
        0
    );

    game.transfer_estate_items(
        &pid("Keeper"),
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 2,
        }],
        vec![],
        0,
        &auth,
    )
    .await;
    let state = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(state.revision, 1);
    assert_eq!(state.item_def_id, "storage_chest");
    assert_eq!(state.items[0].item_def_id, "apple");
    assert_eq!(state.items[0].quantity, 2);
    let stored_id = state.items[0].instance_id;

    game.transfer_estate_items(
        &pid("Keeper"),
        chest.id,
        vec![BagLineItem {
            instance_id: 4,
            qty: 1,
        }],
        vec![],
        1,
        &auth,
    )
    .await;
    let unchanged = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.revision, 1);
    assert_eq!(unchanged.items.len(), 1);

    let db = rusqlite::Connection::open(path).unwrap();
    db.execute("UPDATE land_estates SET missed=1", []).unwrap();
    game.transfer_estate_items(
        &pid("Keeper"),
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 1,
        }],
        vec![],
        1,
        &auth,
    )
    .await;
    let overdue = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert!(!overdue.can_deposit);
    assert_eq!(overdue.revision, 1);
    assert_eq!(overdue.items[0].quantity, 2);

    game.recover_estate_chest(&pid("Keeper"), chest.id, &auth)
        .await;
    assert_eq!(auth.load_estate_chests().unwrap().len(), 1);

    game.transfer_estate_items(
        &pid("Keeper"),
        chest.id,
        vec![],
        vec![BagLineItem {
            instance_id: stored_id,
            qty: 2,
        }],
        1,
        &auth,
    )
    .await;
    assert!(auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        item_quantity(
            &game.get_player_inventory(&pid("Keeper")).await.unwrap(),
            "apple"
        ),
        3
    );

    db.execute("DELETE FROM land_estates", []).unwrap();
    let scavenger_account = auth.login_google("Scavenger").unwrap();
    let scavenger = create_test_character(&auth, &scavenger_account, "Scavenger");
    let abandoned = auth
        .estate_chest_state(chest.id, scavenger.id)
        .unwrap()
        .unwrap();
    assert!(!abandoned.can_deposit);
    assert!(abandoned.items.is_empty());

    game.recover_estate_chest(&pid("Keeper"), chest.id, &auth)
        .await;
    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert_eq!(
        item_quantity(
            &game.get_player_inventory(&pid("Keeper")).await.unwrap(),
            "storage_chest"
        ),
        1
    );
}

#[tokio::test]
async fn estate_storage_accepts_fifty_kg_and_rejects_more() {
    let game = make_flat_world_game_state("estate_storage_weight_limit");
    let (auth, _) = make_test_auth_with_path("estate_storage_weight_limit");
    let character_id = storage_owner(&game, &auth, "WeightKeeper").await;
    {
        let mut inventories = game.inventories.write().await;
        let inventory = inventories.get_mut(&pid("WeightKeeper")).unwrap();
        inventory.bag.push(bag_item(5, "stone_hearth", 1));
        inventory.bag.push(bag_item(6, "campfire_kit", 51));
    }

    game.place_estate_chest(
        &pid("WeightKeeper"),
        2,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);

    game.transfer_estate_items(
        &pid("WeightKeeper"),
        chest.id,
        vec![
            BagLineItem {
                instance_id: 5,
                qty: 1,
            },
            BagLineItem {
                instance_id: 6,
                qty: 50,
            },
        ],
        vec![],
        0,
        &auth,
    )
    .await;
    let full = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(full.max_weight, 500.0);
    assert_eq!(full.revision, 1);
    assert_eq!(full.items.len(), 2);

    game.transfer_estate_items(
        &pid("WeightKeeper"),
        chest.id,
        vec![BagLineItem {
            instance_id: 3,
            qty: 1,
        }],
        vec![],
        1,
        &auth,
    )
    .await;
    let unchanged = auth
        .estate_chest_state(chest.id, character_id)
        .unwrap()
        .unwrap();
    assert_eq!(unchanged.revision, 1);
    assert_eq!(unchanged.items.len(), 2);
    assert_eq!(
        item_quantity(
            &game
                .get_player_inventory(&pid("WeightKeeper"))
                .await
                .unwrap(),
            "apple"
        ),
        3
    );
}

#[tokio::test]
async fn overweight_recovery_turns_the_empty_chest_into_a_ground_item() {
    let game = make_flat_world_game_state("estate_storage_overweight_recovery");
    let (auth, _) = make_test_auth_with_path("estate_storage_overweight_recovery");
    storage_owner(&game, &auth, "HeavyKeeper").await;

    game.place_estate_chest(
        &pid("HeavyKeeper"),
        2,
        Position {
            x: 2.5,
            y: 5.0,
            z: 2.5,
        },
        0.0,
        0,
        &auth,
    )
    .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    game.inventories
        .write()
        .await
        .get_mut(&pid("HeavyKeeper"))
        .unwrap()
        .bag
        .push(bag_item(5, "campfire_kit", 100));

    game.recover_estate_chest(&pid("HeavyKeeper"), chest.id, &auth)
        .await;

    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert_eq!(
        item_quantity(
            &game
                .get_player_inventory(&pid("HeavyKeeper"))
                .await
                .unwrap(),
            "storage_chest"
        ),
        0
    );
    let ground_items = game.ground_items.read().await;
    let dropped = ground_items.values().next().unwrap();
    assert_eq!(dropped.item.item_def_id, "storage_chest");
    assert_eq!(dropped.item.position, chest.position);
    assert_eq!(dropped.item.floor_level, chest.floor_level);
    assert_eq!(dropped.item.dropped_by, Some(pid("HeavyKeeper")));
}
