use super::*;

#[tokio::test]
async fn debug_teleport_lands_above_edited_terrain_and_broadcasts_the_correct_height() {
    let game = make_test_game_state("debug_teleport_surface");
    let id = pid("Admin");
    game.add_player(make_player("Admin", 0.0, 0.0)).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.save_terrain_heightmap(-20, 71, &uniform_heightmap(1.4))
        .await
        .unwrap();
    game.debug_teleport_player(&id, pos3(-1292.4, 0.0, 4554.4))
        .await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(player.floor_level, 0);
    assert_eq!((player.position.x, player.position.z), (-1292.4, 4554.4));
    assert!((player.position.y - 1.4).abs() < 0.001);
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::PlayerTeleported { position, floor_level: 0, .. }
            if (position.y - 1.4).abs() < 0.001)));
    game.debug_teleport_player(&id, pos3(-1292.4, 20.0, 4554.4))
        .await;
    assert_eq!(game.players.read().await[&id].position.y, 20.0);
}

#[tokio::test]
async fn debug_teleport_keeps_explicit_dungeon_destinations_underground() {
    let game = make_test_game_state("debug_teleport_dungeon");
    let id = pid("Admin");
    game.add_player(make_player("Admin", 0.0, 0.0)).await;
    let entrance = game.dungeon_defs.all().next().unwrap();
    let mut destination = entrance.position();
    destination.y = onlinerpg_shared::dungeon::floor_world_y(entrance.y, 1);
    game.debug_teleport_player(&id, destination).await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(player.floor_level, -1);
    assert_eq!(player.position, destination);
}

#[tokio::test]
async fn debug_teleport_restores_the_client_when_destination_terrain_is_unavailable() {
    struct MissingHeight;
    #[async_trait::async_trait]
    impl onlinerpg_terrain::height::HeightTiles for MissingHeight {
        async fn read_heightmap(&self, _tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
            Err(std::io::Error::other("terrain unavailable"))
        }
    }
    let game = make_game_state_with("debug_teleport_missing", MissingHeight, SeaOnlyWater);
    let player = make_player("Admin", 0.0, 0.0);
    let origin = player.position;
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.debug_teleport_player(&id, pos3(-1292.4, 0.0, 4554.4))
        .await;
    assert_eq!(game.players.read().await[&id].position, origin);
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::PlayerTeleported { position, .. } if *position == origin)));
}

/// `GameState.players` is a list (numeric ids can't key a wasm-serialized
/// map), so snapshot assertions look their player up by id.
fn find_player(players: &[Player], id: PlayerId) -> &Player {
    players
        .iter()
        .find(|p| p.id == id)
        .expect("player missing from snapshot")
}

#[tokio::test]
async fn an_operator_kick_closes_plainly_but_a_desync_kick_tells_the_client_to_reload() {
    let auth = make_test_auth("kick_close_codes");
    let game_state = make_test_game_state("kick_close_codes");

    for (close_code, expected) in [
        (None, None),
        (
            Some(onlinerpg_shared::CLOSE_CODE_CLIENT_DESYNC),
            Some(onlinerpg_shared::CLOSE_CODE_CLIENT_DESYNC),
        ),
    ] {
        let account = auth.login_npc("npc_kick_close").unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        game_state
            .register_account_session(&account, tx, &auth)
            .await;
        game_state
            .evict_account_session_locked(&account, "because", close_code, &auth)
            .await;

        match rx.try_recv() {
            Ok(notice) => assert_eq!(notice.close_code, expected),
            other => panic!("expected the session to be ended, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn replacement_login_kicks_the_previous_account_session() {
    let auth = make_test_auth("account_session_replacement");
    let account = auth.login_npc("npc_account_session").unwrap();
    let game_state = make_test_game_state("account_session_replacement");
    let (first_tx, mut first_rx) = tokio::sync::mpsc::unbounded_channel();
    let first_id = game_state
        .register_account_session(&account, first_tx, &auth)
        .await;

    let (second_tx, _second_rx) = tokio::sync::mpsc::unbounded_channel();
    let second_id = game_state
        .register_account_session(&account, second_tx, &auth)
        .await;

    assert!(matches!(
        first_rx.try_recv(),
        Ok(KickNotice {
            message: ServerMessage::Kicked { player_id, .. },
            ..
        }) if player_id == PlayerId::from(0)
    ));
    assert!(
        !game_state
            .is_current_account_session(&account, first_id)
            .await
    );
    assert!(
        game_state
            .is_current_account_session(&account, second_id)
            .await
    );

    game_state
        .end_account_session(&account, first_id, &auth)
        .await;
    assert!(
        game_state
            .is_current_account_session(&account, second_id)
            .await
    );
}

#[tokio::test]
async fn armor_changes_reach_live_and_late_join_players() {
    let game = make_test_game_state("armor_snapshots");
    let wearer = pid("wearer");
    game.add_player(make_player("wearer", 0.0, 0.0)).await;
    game.add_player(make_player("observer", 1.0, 0.0)).await;
    let mut observer = game.register_direct_channel(&pid("observer")).await;
    drain(&mut observer);
    game.inventories.write().await.insert(
        wearer,
        PlayerInventory {
            bag: vec![
                bag_item(1, "worn_breastplate", 1),
                bag_item(2, "worn_plate_helmet", 1),
            ],
            ..Default::default()
        },
    );
    game.equip_item(&wearer, 1).await;
    game.equip_item(&wearer, 2).await;
    assert!(drain(&mut observer).iter().any(|message| matches!(message,
        ServerMessage::PlayerArmorChanged { player_id, armor }
        if *player_id == wearer && armor.chest.as_deref() == Some("worn_breastplate")
            && armor.head.as_deref() == Some("worn_plate_helmet")
    )));
    game.unequip_item(&wearer, EquipSlot::Head).await;
    let expected = game.get_all_players().await[&wearer].armor.clone();
    assert!(expected.head.is_none());
    assert_eq!(expected.chest.as_deref(), Some("worn_breastplate"));
    assert!(drain(&mut observer).iter().any(|message| matches!(message,
        ServerMessage::PlayerArmorChanged { player_id, armor }
        if *player_id == wearer && *armor == expected
    )));
    let snapshot = join_snapshot(&game, make_player("late_armor", 2.0, 0.0)).await;
    assert!(snapshot.iter().any(|message| matches!(message,
        ServerMessage::PlayerAppeared { player } if player.id == wearer && player.armor == expected
    )));
    game.drop_items(
        &wearer,
        vec![onlinerpg_shared::messages::BagLineItem {
            instance_id: 1,
            qty: 1,
        }],
    )
    .await;
    assert_eq!(
        game.get_all_players().await[&wearer].armor,
        Default::default()
    );
}

#[tokio::test]
async fn equipped_torch_syncs_live_and_late_join_player_state() {
    let game_state = make_test_game_state("late_join_torch_snapshot");
    let torch_holder_id = pid("torch_holder");

    game_state
        .add_player(make_player("torch_holder", 0.0, 0.0))
        .await;
    game_state.inventories.write().await.insert(
        torch_holder_id,
        PlayerInventory {
            active_ammo: None,
            bag: vec![bag_item(1, "torch", 1)],
            equipped: Default::default(),
        },
    );

    game_state.equip_item(&torch_holder_id, 1).await;
    assert!(game_state.get_all_players().await[&torch_holder_id].torch_on);

    let snapshot = join_snapshot(&game_state, make_player("late_joiner", 1.0, 0.0))
        .await
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerAppeared { player } => Some(player),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(find_player(&snapshot, torch_holder_id).torch_on);

    game_state
        .unequip_item(&torch_holder_id, EquipSlot::OffHand)
        .await;

    assert!(!game_state.get_all_players().await[&torch_holder_id].torch_on);
}

#[tokio::test]
async fn equipped_main_hand_syncs_live_and_late_join_player_state() {
    let game_state = make_test_game_state("late_join_main_hand_snapshot");
    let angler_id = pid("angler");

    game_state.add_player(make_player("angler", 0.0, 0.0)).await;
    game_state.inventories.write().await.insert(
        angler_id,
        PlayerInventory {
            active_ammo: None,
            bag: vec![bag_item(1, "fishing_rod", 1)],
            equipped: Default::default(),
        },
    );

    game_state.equip_item(&angler_id, 1).await;
    assert_eq!(
        game_state.get_all_players().await[&angler_id].main_hand,
        Some("fishing_rod".to_string())
    );

    let snapshot = join_snapshot(&game_state, make_player("late_joiner", 1.0, 0.0))
        .await
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerAppeared { player } => Some(player),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        find_player(&snapshot, angler_id).main_hand.as_deref(),
        Some("fishing_rod")
    );

    game_state
        .unequip_item(&angler_id, EquipSlot::MainHand)
        .await;

    assert_eq!(
        game_state.get_all_players().await[&angler_id].main_hand,
        None
    );
}

#[tokio::test]
async fn respawn_player_revives_dead_player_only() {
    let game_state = make_test_game_state("respawn_dead");

    let player = Player {
        name: "DeadPlayer".to_string(),
        rotation: 1.25,
        level: 3,
        health: 0,
        max_health: 30,
        ..make_player("player_dead", 12.0, -4.0)
    };
    let player_id = player.id;
    game_state.add_player(player).await;

    let mut direct_rx = game_state.register_direct_channel(&player_id).await;
    let mut broadcast_rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let revived = players
        .get(&player_id)
        .expect("Player should still exist after respawn");
    // No region objects are loaded, so there is no bed: stand at the spot.
    let respawn = &world_config().respawn;
    assert_eq!(revived.health, revived.max_health);
    assert_eq!(revived.position.x, respawn.x);
    assert_eq!(revived.position.y, respawn.y);
    assert_eq!(revived.position.z, respawn.z);
    assert_eq!(revived.rotation, respawn.rotation_deg.to_radians());
    assert_eq!(revived.floor_level, respawn.floor_level);
    assert_eq!(revived.object_type, None);

    // The spawn point sits within discovery range of a dungeon entrance, so
    // an unseeded respawner may receive DungeonDiscoveries alongside this.
    let respawned = drain(&mut direct_rx)
        .into_iter()
        .find_map(|msg| match msg {
            ServerMessage::PlayerRespawned { player } => Some(player),
            _ => None,
        })
        .expect("Expected direct PlayerRespawned");
    assert_eq!(respawned.id, player_id);
    assert_eq!(respawned.health, respawned.max_health);

    match broadcast_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!("Expected no respawn broadcast, got {:?}", server_msg);
        }
        Err(err) => panic!("Expected empty broadcast channel, got {:?}", err),
    }
}

#[tokio::test]
async fn respawn_does_not_disclose_players_on_other_floors() {
    let game_state = make_test_game_state("respawn_cross_floor");
    let respawn = &world_config().respawn;

    let mut dead = Player {
        name: "DeadPlayer".to_string(),
        rotation: 1.25,
        level: 3,
        health: 0,
        max_health: 30,
        ..make_player("cross_floor_dead", 12.0, -4.0)
    };
    let dead_id = dead.id;

    // An observer downstairs must not see the sick room.
    let mut maid = dead.clone();
    maid.id = pid("cross_floor_maid");
    maid.name = "Maid".to_string();
    maid.health = 30;
    maid.position = Position {
        x: respawn.x,
        y: 1.3,
        z: respawn.z,
    };
    maid.floor_level = respawn.floor_level - 1;

    dead.health = 0;
    game_state.add_player(dead).await;
    game_state.add_player(maid.clone()).await;

    let mut maid_rx = game_state.register_direct_channel(&maid.id).await;
    game_state.respawn_player(&dead_id).await;

    let heard = drain(&mut maid_rx).into_iter().any(
        |msg| matches!(msg, ServerMessage::PlayerRespawned { player } if player.id == dead_id),
    );
    assert!(!heard, "respawn must respect the observer space");
}

fn respawn_bed(id: u32, x: f32) -> onlinerpg_shared::furniture::FurniturePlacement {
    let respawn = &world_config().respawn;
    onlinerpg_shared::furniture::FurniturePlacement {
        id,
        type_id: "rustic_bed".to_string(),
        x,
        y: respawn.y,
        z: respawn.z,
        rotation_deg: 180.0,
        floor_level: respawn.floor_level as u8,
    }
}

#[tokio::test]
async fn sickroom_respawn_notifies_nearby_maids_across_floors_once() {
    let game = make_test_game_state("sickroom_maid_notice");
    let respawn = &world_config().respawn;
    let bed_id = respawn.bed_ids[0];
    let (rx, rz) = respawn.region();
    game.sync_region_furniture(rx, rz, &[respawn_bed(bed_id, respawn.x)]);

    let mut dead = make_player("sickroom_guest", respawn.x + 200.0, respawn.z);
    dead.health = 0;
    let dead_id = dead.id;
    game.add_player(dead).await;

    let mut observers = Vec::new();
    for (name, floor, offset, official, class, expected) in [
        ("downstairs_maid", 0, 0.0, true, CharacterClass::Maid, 1),
        ("upstairs_maid", 1, 0.0, true, CharacterClass::Maid, 1),
        ("distant_maid", 0, 100.0, true, CharacterClass::Maid, 0),
        ("dungeon_maid", -1, 0.0, true, CharacterClass::Maid, 0),
        ("other_staff", 0, 0.0, true, CharacterClass::Guard, 0),
        ("ordinary_guest", 0, 0.0, false, CharacterClass::Maid, 0),
    ] {
        let mut observer = make_player(name, respawn.x + offset, respawn.z);
        observer.floor_level = floor;
        observer.is_official_npc = official;
        observer.class = class;
        let id = observer.id;
        game.add_player(observer).await;
        observers.push((name, game.register_direct_channel(&id).await, expected));
    }
    for (_, channel, _) in &mut observers {
        drain(channel);
    }

    game.respawn_player(&dead_id).await;

    for (name, mut channel, expected) in observers {
        let messages = drain(&mut channel);
        let notices: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                ServerMessage::PlayerRespawned { player } if player.id == dead_id => Some(player),
                _ => None,
            })
            .collect();
        assert_eq!(notices.len(), expected, "{name}");
        for player in notices {
            assert_eq!(player.object_id, Some(bed_id));
        }
        if name == "downstairs_maid" {
            assert!(!messages.iter().any(|message| matches!(message,
                ServerMessage::PlayerAppeared { player } if player.id == dead_id
            )));
        }
    }
}

#[tokio::test]
async fn sickroom_respawn_does_not_repeat_a_maid_notice_from_the_death_floor() {
    let game = make_test_game_state("sickroom_maid_same_origin");
    let respawn = &world_config().respawn;
    let (rx, rz) = respawn.region();
    game.sync_region_furniture(rx, rz, &[respawn_bed(respawn.bed_ids[0], respawn.x)]);
    let mut dead = make_player("nearby_dead_guest", respawn.x, respawn.z);
    dead.health = 0;
    let dead_id = dead.id;
    game.add_player(dead).await;
    let mut maid = make_player("nearby_maid", respawn.x, respawn.z);
    maid.class = CharacterClass::Maid;
    maid.is_official_npc = true;
    let maid_id = maid.id;
    game.add_player(maid).await;
    let mut channel = game.register_direct_channel(&maid_id).await;

    game.respawn_player(&dead_id).await;

    assert_eq!(
        drain(&mut channel)
            .iter()
            .filter(|message| matches!(message,
                ServerMessage::PlayerRespawned { player } if player.id == dead_id
            ))
            .count(),
        1
    );
}

async fn kill(game_state: &GameState, id: &PlayerId) {
    game_state.players.write().await.get_mut(id).unwrap().health = 0;
}

#[tokio::test]
async fn respawn_takes_the_first_free_bed_then_stands_beside_them() {
    let game_state = make_test_game_state("respawn_beds");
    let respawn = &world_config().respawn;
    let (rx, rz) = respawn.region();
    let ids = &respawn.bed_ids;
    assert!(ids.len() >= 2, "config must list several beds");
    let beds: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| respawn_bed(*id, respawn.x + i as f32 * 2.0))
        .collect();
    game_state.sync_region_furniture(rx, rz, &beds);

    let mut sleepers = Vec::new();
    for i in 0..ids.len() {
        let name = format!("sleeper{i}");
        let mut player = make_player(&name, 0.0, 0.0);
        // Died mid-sit: the stale pose must not survive the respawn.
        player.object_type = Some("chair".to_string());
        player.object_id = Some(999);
        game_state.add_player(player).await;
        kill(&game_state, &pid(&name)).await;
        game_state.respawn_player(&pid(&name)).await;
        sleepers.push(pid(&name));
    }
    let players = game_state.get_all_players().await;
    for (i, id) in sleepers.iter().enumerate() {
        let p = &players[id];
        assert_eq!(p.object_type.as_deref(), Some("rustic_bed"));
        assert_eq!(p.object_id, Some(ids[i]));
        assert_eq!(p.position.x, beds[i].x);
        assert_eq!(p.rotation, 180f32.to_radians());
        assert_eq!(p.floor_level, respawn.floor_level);
    }

    game_state
        .add_player(make_player("latecomer", 0.0, 0.0))
        .await;
    kill(&game_state, &pid("latecomer")).await;
    game_state.respawn_player(&pid("latecomer")).await;
    let players = game_state.get_all_players().await;
    let p = &players[&pid("latecomer")];
    assert_eq!(p.object_type, None);
    assert_eq!(p.object_id, None);
    assert_eq!(p.position.x, respawn.x);

    // A sleeper getting up frees the bed for the next death.
    game_state
        .set_player_interaction(&sleepers[0], None, None)
        .await;
    kill(&game_state, &pid("latecomer")).await;
    game_state.respawn_player(&pid("latecomer")).await;
    let players = game_state.get_all_players().await;
    assert_eq!(players[&pid("latecomer")].object_id, Some(ids[0]));
}

#[tokio::test]
async fn respawn_player_ignores_alive_player() {
    let game_state = make_test_game_state("respawn_alive");

    let player = Player {
        name: "AlivePlayer".to_string(),
        rotation: 0.75,
        level: 2,
        health: 18,
        max_health: 20,
        ..make_player("player_alive", 5.0, 6.0)
    };
    let player_id = player.id;
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let unchanged = players
        .get(&player_id)
        .expect("Player should still exist after ignored respawn");
    assert_eq!(unchanged.health, 18);
    assert_eq!(unchanged.position.x, 5.0);
    assert_eq!(unchanged.position.y, 0.0);
    assert_eq!(unchanged.position.z, 6.0);
    assert_eq!(unchanged.rotation, 0.75);

    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!(
                "Expected no broadcast for alive respawn, got {:?}",
                server_msg
            );
        }
        Err(err) => panic!("Expected empty channel, got {:?}", err),
    }
}

#[tokio::test]
async fn active_character_cannot_be_deleted_from_another_session() {
    let auth = make_test_auth("active_character_delete_guard");
    let account = auth.login_npc("npc_active_delete").unwrap();
    let record = create_test_character(&auth, &account, "StillPlaying");
    let game_state = Arc::new(make_test_game_state("active_character_delete_guard"));
    let player_id = pid("active_character");

    // Deletion must wait for admission, then reject the registered character.
    let admission = game_state.lock_character_sessions().await;
    let deleting_state = Arc::clone(&game_state);
    let deleting_auth = auth.clone();
    let deleting_account = account.clone();
    let character_id = record.id;
    auth.make_character_deletion_due(character_id);
    let delete = tokio::spawn(async move {
        deleting_state
            .delete_character_if_inactive(&deleting_auth, &deleting_account, character_id)
            .await
            .unwrap()
    });
    tokio::task::yield_now().await;
    assert!(!delete.is_finished());

    game_state
        .register_player_character(&player_id, record.id, 0, attrs_with_cha(12), 0, None)
        .await;
    drop(admission);

    assert!(!delete.await.unwrap());
    assert!(auth.get_character_for_account(&account, record.id).is_ok());

    game_state.unregister_player_character(&player_id).await;
    assert!(game_state
        .delete_character_if_inactive(&auth, &account, record.id)
        .await
        .unwrap());
    assert!(matches!(
        auth.get_character_for_account(&account, record.id),
        Err(crate::auth::AuthError::CharacterNotFound)
    ));
}

// ---- Phoenix talisman (revive in place) ------------------------------------

/// A player on a dungeon floor at `health`/30 HP carrying `talismans` phoenix talismans.
async fn make_talisman_holder(
    game_state: &GameState,
    name: &str,
    health: u32,
    talismans: u32,
) -> PlayerId {
    let mut player = make_player(name, 40.0, -7.0);
    player.health = health;
    player.max_health = 30;
    player.floor_level = -2;
    player.rotation = 1.5;
    let id = player.id;
    game_state.add_player(player).await;
    let mut inv = PlayerInventory::default();
    inv.bag.push(bag_item(7, "phoenix_talisman", talismans));
    game_state.inventories.write().await.insert(id, inv);
    id
}

#[tokio::test]
async fn phoenix_talisman_revives_where_the_player_fell_and_is_spent() {
    let game_state = make_test_game_state("talisman_revive");
    let id = make_talisman_holder(&game_state, "fallen", 0, 2).await;
    let mut direct_rx = game_state.register_direct_channel(&id).await;

    game_state.use_item(&id, 7).await;

    let players = game_state.get_all_players().await;
    let revived = &players[&id];
    assert_eq!(revived.health, 21, "70% of 30");
    assert_eq!(
        revived.position,
        Position {
            x: 40.0,
            y: 0.0,
            z: -7.0
        }
    );
    assert_eq!(revived.floor_level, -2);
    assert_eq!(revived.rotation, 1.5);
    assert_eq!(
        game_state.inventories.read().await[&id].bag[0].quantity,
        1,
        "one talisman is spent"
    );
    let respawned = drain(&mut direct_rx)
        .into_iter()
        .find_map(|msg| match msg {
            ServerMessage::PlayerRespawned { player } => Some(player),
            _ => None,
        })
        .expect("Expected direct PlayerRespawned");
    assert_eq!(respawned.health, 21);
    assert_eq!(respawned.floor_level, -2);
}

#[tokio::test]
async fn phoenix_talisman_is_kept_when_used_alive() {
    let game_state = make_test_game_state("talisman_alive");
    let id = make_talisman_holder(&game_state, "standing", 12, 1).await;
    let mut direct_rx = game_state.register_direct_channel(&id).await;

    game_state.use_item(&id, 7).await;

    assert_eq!(game_state.get_all_players().await[&id].health, 12);
    assert_eq!(game_state.inventories.read().await[&id].bag[0].quantity, 1);
    assert!(drain(&mut direct_rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::SystemMessage { .. })));
}
