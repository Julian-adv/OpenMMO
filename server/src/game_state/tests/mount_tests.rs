use super::*;

async fn rider(game: &GameState) -> PlayerId {
    let player = make_player("Rider", 0.0, 0.0);
    let id = player.id;
    game.add_player(player).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "horse_reins", 1)],
            ..Default::default()
        },
    );
    id
}

#[tokio::test]
async fn horse_reins_toggle_without_consumption_and_broadcast() {
    let game = make_test_game_state("horse_toggle");
    let id = rider(&game).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].mounted);
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerMountChanged { mounted: true, .. })));
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].mounted);
    assert_eq!(
        game.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        1
    );
}

#[tokio::test]
async fn switching_to_bow_while_riding_preserves_enchanted_sword_after_reload() {
    let game = make_test_game_state("horse_weapon_swap");
    let auth = make_test_auth("horse_weapon_swap");
    let account = auth.login_npc("npc_horse_weapon_swap").unwrap();
    let record = create_test_character(&auth, &account, "Rider");
    let id = rider(&game).await;
    game.register_player_character(
        &id,
        record.id,
        record.xp,
        attrs_with_cha(12),
        record.gold,
        None,
    )
    .await;
    let sword = ItemInstance {
        enchant: 5,
        ..bag_item(2, "steel_longsword", 1)
    };
    let shield = bag_item(3, "wooden_shield", 1);
    {
        let mut inventories = game.inventories.write().await;
        let inv = inventories.get_mut(&id).unwrap();
        inv.equipped.insert(EquipSlot::MainHand, sword.clone());
        inv.equipped.insert(EquipSlot::OffHand, shield.clone());
        inv.bag.push(bag_item(4, "bow", 1));
        inv.bag.push(bag_item(5, "iron_arrow", 200));
        inv.bag
            .extend((6..56).map(|n| bag_item(n, "iron_sword", 1)));
    }
    game.use_item(&id, 1).await;
    game.update_player_position(&id, move_cmd(pos(20.0), false), false)
        .await;
    game.tick_player_movement(0.2).await;
    let before_swap = game.players.read().await[&id].position;
    let mut rx = game.register_direct_channel(&id).await;

    game.equip_item(&id, 4).await;
    game.tick_player_movement(0.2).await;

    assert!(game.players.read().await[&id].mounted);
    assert!(
        game.players.read().await[&id]
            .position
            .dist_xz_sq(&before_swap)
            > 0.0
    );
    let inv = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(inv.bag.len(), 54);
    assert!(inv.bag.contains(&sword));
    assert!(inv.bag.contains(&shield));
    assert_eq!(inv.equipped[&EquipSlot::MainHand].item_def_id, "bow");
    assert!(!inv.equipped.contains_key(&EquipSlot::OffHand));
    assert_eq!(inv.active_ammo.as_deref(), Some("iron_arrow"));
    assert!(game.ground_items.read().await.is_empty());
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::InventoryUpdated { inventory } if inventory.bag.contains(&sword)
    )));

    game.flush_dirty_saves(&auth).await;
    game.take_player_inventory(&id).await.unwrap();
    game.load_player_inventory(&id, record.id, &auth).await;
    let reloaded = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(reloaded.bag.len(), inv.bag.len());
    let swords: Vec<_> = reloaded
        .bag
        .iter()
        .filter(|item| item.item_def_id == "steel_longsword")
        .collect();
    assert_eq!(swords.len(), 1);
    assert_eq!(swords[0].enchant, 5);
    assert_eq!(swords[0].quantity, 1);
    let sword_id = swords[0].instance_id;
    game.equip_item(&id, sword_id).await;
    let restored = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(restored.equipped[&EquipSlot::MainHand].enchant, 5);
    assert!(restored.bag.iter().any(|item| item.item_def_id == "bow"));
}

#[tokio::test]
async fn horse_movement_is_three_times_as_fast_and_losing_reins_dismounts() {
    let game = make_test_game_state("horse_speed");
    let id = rider(&game).await;
    game.players.write().await.get_mut(&id).unwrap().rotation = std::f32::consts::FRAC_PI_2;
    game.use_item(&id, 1).await;
    game.update_player_position(
        &id,
        move_cmd(
            Position {
                x: 20.0,
                y: 5.0,
                z: 0.0,
            },
            false,
        ),
        false,
    )
    .await;
    game.tick_player_movement(1.0).await;
    assert!((game.players.read().await[&id].position.x - 9.0).abs() < 0.01);
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .clear();
    game.tick_player_movement(1.0).await;
    let player = game.players.read().await[&id].clone();
    assert!(!player.mounted);
    assert!((player.position.x - 12.0).abs() < 0.01);
}

#[tokio::test]
async fn horse_turns_along_an_arc_and_accepts_a_new_direction() {
    let game = make_test_game_state("horse_turn");
    let id = rider(&game).await;
    game.use_item(&id, 1).await;
    game.update_player_position(&id, move_cmd(pos(20.0), false), false)
        .await;
    game.tick_player_movement(0.2).await;
    let p = game.players.read().await[&id].clone();
    assert!((p.position.x - 0.65 * (1.0 - (std::f32::consts::PI / 6.0).cos())).abs() < 1e-4);
    assert!((p.position.z - 0.325).abs() < 1e-4);
    assert!((p.rotation - std::f32::consts::PI / 6.0).abs() < 1e-5);
    game.turn_horse(&id, -std::f32::consts::FRAC_PI_2, false)
        .await;
    game.tick_player_movement(0.1).await;
    let turned = game.players.read().await[&id].clone();
    assert!(turned.rotation < p.rotation);
    assert!(turned.position.dist_xz_sq(&p.position) > 0.0);
    game.tick_player_movement(1.0).await;
    assert!((game.players.read().await[&id].rotation + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    assert!(!game.movement_intents.read().await.contains_key(&id));
    game.turn_horse(&id, f32::NAN, false).await;
    assert!(!game.movement_intents.read().await.contains_key(&id));
    game.use_item(&id, 1).await;
    game.turn_horse(&id, 0.0, false).await;
    assert!(!game.movement_intents.read().await.contains_key(&id));
}

#[tokio::test]
async fn horse_travel_matches_small_ticks_and_cannot_skip_a_waypoint_turn() {
    let game = make_test_game_state("horse_turn_ticks");
    let id = rider(&game).await;
    game.use_item(&id, 1).await;
    game.update_player_position(&id, move_cmd(pos(20.0), false), false)
        .await;
    for _ in 0..10 {
        game.tick_player_movement(0.1).await;
    }
    let p = game.players.read().await[&id].clone();
    let coarse = make_test_game_state("horse_turn_coarse");
    let coarse_id = rider(&coarse).await;
    coarse.use_item(&coarse_id, 1).await;
    coarse
        .update_player_position(&coarse_id, move_cmd(pos(20.0), false), false)
        .await;
    coarse.tick_player_movement(1.0).await;
    let other = coarse.players.read().await[&coarse_id].clone();
    assert!(p.position.dist_xz_sq(&other.position) < 1e-5);
    assert!((p.rotation - other.rotation).abs() < 1e-4);
    game.update_player_position(&id, move_cmd(p.position, false), false)
        .await;
    game.update_player_position(
        &id,
        move_cmd(
            Position {
                z: -20.0,
                ..p.position
            },
            true,
        ),
        false,
    )
    .await;
    game.tick_player_movement(0.2).await;
    let after = game.players.read().await[&id].clone();
    assert!(after.position.dist_xz_sq(&p.position) > 0.01);
    assert!((after.rotation - std::f32::consts::PI).abs() > 0.5);
    game.stop_horse(&id).await;
    game.tick_player_movement(1.0).await;
    assert_eq!(game.players.read().await[&id].position, after.position);
    assert_eq!(game.players.read().await[&id].rotation, after.rotation);
}

#[tokio::test]
async fn horse_turning_does_not_bypass_solid_furniture() {
    let game = make_test_game_state("horse_turn_collision");
    let id = rider(&game).await;
    let start = Position {
        x: 0.5,
        y: 5.0,
        z: 4.5,
    };
    game.players.write().await.get_mut(&id).unwrap().position = start;
    game.players.write().await.get_mut(&id).unwrap().rotation = 0.0;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].mounted);
    game.sync_region_furniture(0, 0, &[table_placement(0.5, 5.5)]);
    game.turn_horse(&id, -std::f32::consts::FRAC_PI_2, false)
        .await;
    game.tick_player_movement(2.0).await;
    let p = game.players.read().await[&id].clone();
    assert!(
        p.position.z < 5.0,
        "arc crossed the table boundary: {:?}",
        p.position
    );
    assert!(p.position.z > start.z);
    assert!(p.rotation > -std::f32::consts::FRAC_PI_2);
    assert!(!game.movement_intents.read().await.contains_key(&id));
}

#[tokio::test]
async fn horse_mount_rejects_defeat_dungeons_combat_and_water() {
    let game = make_test_game_state("horse_guards");
    let id = rider(&game).await;
    for state in 0..4 {
        {
            let mut players = game.players.write().await;
            let p = players.get_mut(&id).unwrap();
            p.health = if state == 0 { 0 } else { 10 };
            p.floor_level = if state == 1 { -1 } else { 0 };
            p.last_combat_at = if state == 2 { GameState::now_ms() } else { 0 };
            p.position.x = if state == 3 { -50.0 } else { 0.0 };
        }
        game.use_item(&id, 1).await;
        assert!(!game.players.read().await[&id].mounted);
    }
    assert_eq!(
        game.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        1
    );
}

#[tokio::test]
async fn horse_dismounts_when_combat_or_interaction_starts() {
    let game = make_test_game_state("horse_dismount");
    let id = rider(&game).await;
    game.use_item(&id, 1).await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].mounted);
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    game.use_item(&id, 1).await;
    game.players.write().await.get_mut(&id).unwrap().object_type = Some("sit".into());
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].mounted);
}

#[tokio::test]
async fn horse_cannot_mount_indoors_and_dismounts_on_entry() {
    use onlinerpg_shared::pathfinding::RuntimePassability;

    let game = make_test_game_state("horse_indoors");
    let id = rider(&game).await;
    game.passability_write().insert(
        "house:test".into(),
        RuntimePassability {
            house_origin_x: 10.0,
            house_origin_z: 0.0,
            min_x: 10.0,
            max_x: 14.0,
            min_z: -2.0,
            max_z: 2.0,
            floors: vec![],
            stairwells: vec![],
            yields_to_trapped_mover: false,
            allows_projectiles: false,
            is_ground: true,
        },
    );
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].mounted);
    game.players.write().await.get_mut(&id).unwrap().position.x = 12.0;
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].mounted);
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].mounted);
}

fn recovery_fence(
    game: &GameState,
    key: &str,
    x: i32,
    z: i32,
    axis: onlinerpg_shared::fence::FenceAxis,
) {
    use onlinerpg_shared::fence::{self, Fence, FenceEdge};
    fence::sync_passability(
        &mut game.passability_write(),
        key,
        &[Fence {
            edge: FenceEdge { x, z, axis },
            y: 5.0,
            owner_id: 1,
        }],
    );
}

#[tokio::test]
async fn horse_recovery_backs_up_without_turning_and_repaths_for_four_boundary_poses() {
    use onlinerpg_shared::{
        fence::{self, Fence, FenceAxis, FenceEdge},
        pathfinding,
    };
    let game = make_game_state_with("horse_recovery_four", FlatLand, SeaOnlyWater);
    let fences: Vec<_> = [4740, 4748]
        .into_iter()
        .flat_map(|z| {
            (-1465..-1435).map(move |x| Fence {
                edge: FenceEdge {
                    x,
                    z,
                    axis: FenceAxis::X,
                },
                y: 5.0,
                owner_id: 1,
            })
        })
        .chain((4700..4720).map(|z| Fence {
            edge: FenceEdge {
                x: -1555,
                z,
                axis: FenceAxis::Z,
            },
            y: 5.0,
            owner_id: 1,
        }))
        .collect();
    fence::sync_passability(&mut game.passability_write(), "boundary", &fences);
    let mut actors = Vec::new();
    for (name, x, z, rotation, gx, gz) in [
        ("Chef", -1450.7693, 4739.999, 5.345787, -1460.5, 4729.5),
        ("Event", -1554.996, 4706.3853, -2.5081613, -1554.5, 4705.5),
        ("GUARD", -1447.0452, 4747.999, 0.6163312, -1455.5, 4746.5),
        ("GONGJI", -1447.0452, 4747.999, 0.6163312, -1455.5, 4746.5),
    ] {
        let player = make_player(name, x, z);
        let id = player.id;
        game.add_player(player).await;
        game.inventories.write().await.insert(
            id,
            PlayerInventory {
                bag: vec![bag_item(1, "horse_reins", 1)],
                ..Default::default()
            },
        );
        let start = Position { x, y: 5.0, z };
        {
            let mut players = game.players.write().await;
            let p = players.get_mut(&id).unwrap();
            p.name = name.into();
            p.position = start;
            p.rotation = rotation;
        }
        game.use_item(&id, 1).await;
        let mut rx = game.register_direct_channel(&id).await;
        let goal = Position {
            x: gx,
            y: 5.0,
            z: gz,
        };
        game.update_player_position(&id, move_cmd(goal, false), false)
            .await;
        game.tick_player_movement(0.05).await;
        assert_eq!(
            game.players.read().await[&id].position,
            start,
            "{name} must reproduce the blocked arc"
        );
        assert!(
            drain(&mut rx)
                .iter()
                .any(|m| matches!(m, ServerMessage::PositionCorrected { .. })),
            "{name}"
        );
        game.recover_horse(&id, 7, goal).await;
        assert!(
            drain(&mut rx).iter().any(|m| matches!(
                m,
                ServerMessage::MountRecovery {
                    done: false,
                    success: true,
                    ..
                }
            )),
            "{name}"
        );
        actors.push((id, name, start, rotation, goal, rx));
    }
    for _ in 0..20 {
        game.tick_player_movement(0.1).await;
    }
    for (id, name, start, rotation, goal, mut rx) in actors {
        let p = game.players.read().await[&id].clone();
        let distance = start.dist_xz_sq(&p.position).sqrt();
        assert!(p.mounted, "{name}");
        assert_eq!(p.rotation, rotation, "{name}");
        assert!((0.24..=2.01).contains(&distance), "{name}: {distance}");
        assert!(
            (p.position.x - start.x) * rotation.sin() + (p.position.z - start.z) * rotation.cos()
                < 0.0
        );
        assert!(
            drain(&mut rx).iter().any(|m| matches!(
                m,
                ServerMessage::MountRecovery {
                    request_id: 7,
                    done: true,
                    success: true,
                    ..
                }
            )),
            "{name}"
        );
        let route = pathfinding::find_and_smooth_path(
            p.position.x,
            p.position.z,
            0,
            goal.x,
            goal.z,
            0,
            &game.passability_read(),
            2000,
        );
        assert!(route.found);
        for (index, wp) in route.waypoints.iter().enumerate() {
            game.update_player_position(
                &id,
                move_cmd(
                    Position {
                        x: wp.x,
                        y: 5.0,
                        z: wp.z,
                    },
                    index > 0,
                ),
                false,
            )
            .await;
        }
        for _ in 0..100 {
            game.tick_player_movement(0.1).await;
        }
        let p = game.players.read().await[&id].clone();
        assert!(
            p.mounted && p.position.dist_xz_sq(&goal) < 0.01,
            "{name}: {:?}",
            p.position
        );
        assert!(
            !drain(&mut rx)
                .iter()
                .any(|m| matches!(m, ServerMessage::PositionCorrected { .. })),
            "{name}"
        );
    }
}

#[tokio::test]
async fn horse_recovery_refuses_a_blocked_rear_and_stops_for_new_obstacles_or_cancel() {
    use onlinerpg_shared::fence::FenceAxis;
    for mode in ["blocked", "new_obstacle", "cancel", "dismount"] {
        let game = make_game_state_with(mode, FlatLand, SeaOnlyWater);
        let id = rider(&game).await;
        let start = Position {
            x: 0.5,
            y: 5.0,
            z: 0.1,
        };
        game.players.write().await.get_mut(&id).unwrap().position = start;
        game.use_item(&id, 1).await;
        let mut rx = game.register_direct_channel(&id).await;
        if mode == "blocked" {
            recovery_fence(&game, "rear", 0, 0, FenceAxis::X);
        }
        game.recover_horse(&id, 1, Position { x: 4.0, ..start })
            .await;
        if mode == "new_obstacle" {
            recovery_fence(&game, "rear", 0, 0, FenceAxis::X);
        }
        if mode == "cancel" {
            game.stop_horse(&id).await;
        }
        if mode == "dismount" {
            game.use_item(&id, 1).await;
        }
        game.tick_player_movement(1.0).await;
        assert_eq!(game.players.read().await[&id].position, start, "{mode}");
        assert!(!game.movement_intents.read().await.contains_key(&id));
        if mode != "cancel" {
            assert!(
                drain(&mut rx).iter().any(|m| matches!(
                    m,
                    ServerMessage::MountRecovery {
                        done: true,
                        success: false,
                        ..
                    }
                )),
                "{mode}"
            );
        }
    }
}
