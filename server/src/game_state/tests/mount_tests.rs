use super::*;
use onlinerpg_shared::mount::MountKind;

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
    assert!(game.players.read().await[&id].is_mounted());
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMountChanged {
            mount: Some(MountKind::Horse),
            ..
        }
    )));
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
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

    assert!(game.players.read().await[&id].is_mounted());
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
    assert!(!player.is_mounted());
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
async fn horse_sprint_drains_streamed_waypoints_after_a_turn() {
    let game = make_game_state_with("horse_streamed_waypoints", FlatLand, SeaOnlyWater);
    let id = rider(&game).await;
    let start = Position {
        x: -1497.0764,
        y: 5.0,
        z: 4740.4253,
    };
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.position = start;
        player.rotation = -std::f32::consts::FRAC_PI_2;
    }
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    let mut peak_queue = 0;
    let mut target = start;
    for tick in 0..60 {
        for sample in 0..4 {
            let index = tick * 4 + sample + 1;
            target = Position {
                x: start.x - index as f32 * 0.675,
                z: start.z + 0.4087,
                ..start
            };
            game.update_player_position(
                &id,
                MoveCommand {
                    position: target,
                    rotation: -std::f32::consts::FRAC_PI_2,
                    floor_level: 0,
                    append: index > 1,
                    sprinting: true,
                },
                false,
            )
            .await;
        }
        game.tick_player_movement(0.2).await;
        let queued = game
            .movement_intents
            .read()
            .await
            .get(&id)
            .map_or(0, |queue| queue.len());
        peak_queue = peak_queue.max(queued);
    }
    let player = game.players.read().await[&id].clone();
    let lag = player.position.dist_xz_sq(&target).sqrt();
    assert!(
        peak_queue < 10 && lag < 5.0,
        "peak queue {peak_queue}, lag {lag}"
    );
    game.tick_player_movement(1.0).await;
    assert!(!game.movement_intents.read().await.contains_key(&id));
    assert!(game.players.read().await[&id].position.dist_xz_sq(&target) <= 1.0);
}

#[tokio::test]
async fn keyboard_reverse_and_stationary_turns_preserve_facing_and_speed_limits() {
    use onlinerpg_shared::mount_movement::{angle_delta, BACKWARD_SPEED};
    for mounted in [false, true] {
        let game = make_game_state_with("keyboard_controls", FlatLand, SeaOnlyWater);
        let id = rider(&game).await;
        game.players.write().await.get_mut(&id).unwrap().position.y = 5.0;
        if mounted {
            game.use_item(&id, 1).await;
        }
        let start = game.players.read().await[&id].position;
        game.players.write().await.get_mut(&id).unwrap().rotation = std::f32::consts::FRAC_PI_2;
        let reverse = MoveCommand {
            position: Position {
                x: start.x - 4.0,
                ..start
            },
            rotation: std::f32::consts::FRAC_PI_2,
            floor_level: 0,
            append: true,
            sprinting: true,
        };
        for _ in 0..10 {
            game.update_keyboard_movement(&id, reverse, -1).await;
        }
        assert_eq!(game.movement_intents.read().await[&id].len(), 1);
        game.tick_player_movement(0.2).await;
        let backed = game.players.read().await[&id].clone();
        assert!((backed.position.x - (start.x - BACKWARD_SPEED * 0.2)).abs() < 0.001);
        assert!((backed.position.z - start.z).abs() < 0.001);
        assert!(angle_delta(backed.rotation, reverse.rotation).abs() < 0.001);

        let facing = backed.rotation - 0.3;
        game.update_keyboard_movement(
            &id,
            MoveCommand {
                position: backed.position,
                rotation: facing,
                ..reverse
            },
            0,
        )
        .await;
        game.tick_player_movement(0.2).await;
        let turned = game.players.read().await[&id].clone();
        assert_eq!(turned.position, backed.position);
        assert!(angle_delta(turned.rotation, facing).abs() < 0.001);
        assert!(!game.movement_intents.read().await.contains_key(&id));

        game.update_keyboard_movement(&id, reverse, 2).await;
        assert!(!game.movement_intents.read().await.contains_key(&id));
        let click_target = Position {
            x: start.x + 10.0,
            ..start
        };
        game.update_player_position(&id, move_cmd(click_target, false), false)
            .await;
        assert_eq!(
            game.movement_intents.read().await[&id][0].target.x,
            click_target.x
        );
        game.tick_player_movement(0.2).await;
        assert!(game.players.read().await[&id].position.x > turned.position.x);
    }
}

#[tokio::test]
async fn keyboard_turns_settle_at_the_clients_stop_position_after_delayed_travel() {
    for mounted in [false, true] {
        let game = make_game_state_with("keyboard_turn_stop", FlatLand, SeaOnlyWater);
        let id = rider(&game).await;
        game.players.write().await.get_mut(&id).unwrap().position.y = 5.0;
        if mounted {
            game.use_item(&id, 1).await;
        }
        let start = game.players.read().await[&id].position;
        let stop = Position {
            z: start.z + 2.0,
            y: 5.0,
            ..start
        };
        let command = MoveCommand {
            position: Position {
                z: start.z + 8.0,
                y: 5.0,
                ..start
            },
            rotation: 0.0,
            floor_level: 0,
            append: false,
            sprinting: true,
        };
        game.update_keyboard_movement(&id, command, 1).await;
        game.tick_player_movement(0.2).await;
        game.update_keyboard_movement(
            &id,
            MoveCommand {
                position: stop,
                rotation: -0.4,
                ..command
            },
            0,
        )
        .await;
        for _ in 0..10 {
            let before = game.players.read().await[&id].position;
            game.tick_player_movement(0.2).await;
            let after = game.players.read().await[&id].position;
            let limit: f32 = if mounted { 1.8 } else { 0.6 };
            assert!(after.dist_xz_sq(&before) <= (limit + 0.001).powi(2));
        }
        let player = game.players.read().await[&id].clone();
        assert!(player.position.dist_xz_sq(&stop) < 0.0001);
        assert!(
            onlinerpg_shared::mount_movement::angle_delta(player.rotation, -0.4).abs() < 0.0001
        );
        assert!(!game.movement_intents.read().await.contains_key(&id));
    }
}

#[tokio::test]
async fn keyboard_backsteps_stop_at_solid_furniture() {
    for mounted in [false, true] {
        let game = make_test_game_state("keyboard_backstep_collision");
        let id = rider(&game).await;
        let start = Position {
            x: 0.5,
            y: 5.0,
            z: 4.5,
        };
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&id).unwrap();
            player.position = start;
            player.rotation = std::f32::consts::PI;
        }
        if mounted {
            game.use_item(&id, 1).await;
        }
        game.sync_region_furniture(0, 0, &[table_placement(0.5, 5.5)]);
        game.update_keyboard_movement(
            &id,
            MoveCommand {
                position: Position { z: 8.5, ..start },
                rotation: std::f32::consts::PI,
                floor_level: 0,
                append: false,
                sprinting: true,
            },
            -1,
        )
        .await;
        for _ in 0..10 {
            game.tick_player_movement(0.2).await;
        }
        let player = game.players.read().await[&id].clone();
        assert!(player.position.z < 5.0, "{mounted}: {:?}", player.position);
        assert!(
            onlinerpg_shared::mount_movement::angle_delta(player.rotation, std::f32::consts::PI)
                .abs()
                < 0.001
        );
        assert!(!game.movement_intents.read().await.contains_key(&id));
    }
}

#[tokio::test]
async fn keyboard_targets_match_the_web_replay_without_queue_growth() {
    #[derive(serde::Deserialize)]
    struct Command {
        tick: usize,
        position: Position,
        rotation: f32,
        sprinting: bool,
        forward: i8,
    }
    #[derive(serde::Deserialize)]
    struct Checkpoint {
        tick: usize,
        position: Position,
    }
    #[derive(serde::Deserialize)]
    struct Replay {
        mounted: bool,
        start: Position,
        rotation: f32,
        ticks: usize,
        commands: Vec<Command>,
        checkpoints: Vec<Checkpoint>,
    }
    let replays: Vec<Replay> = serde_json::from_str(include_str!(
        "../../../../client/src/lib/components/player-control/fsm/__fixtures__/mounted-keyboard.json"
    ))
    .unwrap();
    for replay in replays {
        for delay in [0, 1, 2] {
            let game = make_game_state_with("horse_keyboard_targets", FlatLand, SeaOnlyWater);
            let id = rider(&game).await;
            {
                let mut players = game.players.write().await;
                let player = players.get_mut(&id).unwrap();
                player.position = replay.start;
                player.rotation = replay.rotation;
            }
            if replay.mounted {
                game.use_item(&id, 1).await;
            }
            let max_speed = if replay.mounted { 13.5 } else { 4.5 };
            let mut rx = game.register_direct_channel(&id).await;
            let mut commands = replay.commands.iter().peekable();
            let mut checkpoints = replay.checkpoints.iter().peekable();
            for tick in 1..=replay.ticks + delay {
                while commands
                    .peek()
                    .is_some_and(|command| command.tick + delay <= tick)
                {
                    let command = commands.next().unwrap();
                    game.update_keyboard_movement(
                        &id,
                        MoveCommand {
                            position: command.position,
                            rotation: command.rotation,
                            floor_level: 0,
                            append: false,
                            sprinting: command.sprinting,
                        },
                        command.forward,
                    )
                    .await;
                    assert_eq!(game.movement_intents.read().await[&id].len(), 1);
                }
                let before = game.players.read().await[&id].position;
                game.tick_player_movement(0.2).await;
                let player = game.players.read().await[&id].clone();
                assert_eq!(player.is_mounted(), replay.mounted);
                assert!(player.position.dist_xz_sq(&before).sqrt() <= max_speed * 0.2 + 0.01);
                if checkpoints
                    .peek()
                    .is_some_and(|checkpoint| checkpoint.tick == tick)
                {
                    let checkpoint = checkpoints.next().unwrap();
                    let error = player.position.dist_xz_sq(&checkpoint.position).sqrt();
                    // Steering updates can fall anywhere within a 200ms server tick.
                    let tick_margin = (max_speed * 0.2_f32).max(1.0);
                    assert!(
                        error <= tick_margin + max_speed * 0.2 * delay as f32,
                        "mounted {}, tick {tick}, delay {delay}: client/server distance {error}",
                        replay.mounted
                    );
                }
            }
            assert!(!game.movement_intents.read().await.contains_key(&id));
            let stop = replay.checkpoints.last().unwrap().position;
            assert!(game.players.read().await[&id].position.dist_xz_sq(&stop) <= 1.0);
            assert!(!drain(&mut rx)
                .iter()
                .any(|message| matches!(message, ServerMessage::PositionCorrected { .. })));
        }
    }
}

#[tokio::test]
async fn horse_stops_within_one_metre_without_snapping_or_correcting() {
    let game = make_test_game_state("horse_arrival_radius");
    let id = rider(&game).await;
    game.players.write().await.get_mut(&id).unwrap().rotation = std::f32::consts::FRAC_PI_2;
    game.use_item(&id, 1).await;
    let start = game.players.read().await[&id].position;
    let mut rx = game.register_direct_channel(&id).await;
    let target = Position { x: 1.5, ..start };
    game.update_player_position(&id, move_cmd(target, false), false)
        .await;
    game.tick_player_movement(0.2).await;
    let stopped = game.players.read().await[&id].position;
    assert!((0.5..0.8).contains(&stopped.x), "{stopped:?}");
    assert!(stopped.dist_xz_sq(&target) <= 1.0);
    assert!(!game.movement_intents.read().await.contains_key(&id));
    game.update_player_position(&id, move_cmd(target, false), false)
        .await;
    game.tick_player_movement(0.2).await;
    assert_eq!(game.players.read().await[&id].position, stopped);
    assert!(!drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::PositionCorrected { .. })));
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
    assert!(game.players.read().await[&id].is_mounted());
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
        assert!(!game.players.read().await[&id].is_mounted());
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
    assert!(!game.players.read().await[&id].is_mounted());
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    game.use_item(&id, 1).await;
    game.players.write().await.get_mut(&id).unwrap().object_type = Some("sit".into());
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
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
    assert!(game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 12.0;
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
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
        let history = serde_json::to_value(
            game.movement_audit
                .snapshot(id, Instant::now() + std::time::Duration::from_secs(31))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(history["recovery_requests"], 1, "{name}");
        let events = history["recovery_events"].as_array().unwrap();
        assert_eq!(
            events.len(),
            3,
            "{name}: only receipt, acceptance and completion"
        );
        assert_eq!(events[0]["status"], "received");
        assert_eq!(events[1]["status"], "accepted");
        assert_eq!(events[2]["status"], "completed");
        assert_eq!(events[0]["request"]["request_id"], 7);
        assert_eq!(events[0]["request"], events[2]["request"]);
        assert_eq!(
            events[2]["detail"]["pose"]["position"],
            serde_json::json!(p.position)
        );
        let distance = start.dist_xz_sq(&p.position).sqrt();
        assert!(p.is_mounted(), "{name}");
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
            p.is_mounted() && p.position.dist_xz_sq(&goal) <= 1.0,
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
        let history =
            serde_json::to_value(game.movement_audit.snapshot(id, Instant::now()).unwrap())
                .unwrap();
        let event = history["recovery_events"]
            .as_array()
            .unwrap()
            .last()
            .unwrap();
        let (status, reason) = match mode {
            "blocked" => ("rejected", "no_recovery_path"),
            "new_obstacle" => ("failed", "blocked"),
            "cancel" => ("cancelled", "stop"),
            "dismount" => ("failed", "not_mounted"),
            _ => unreachable!(),
        };
        assert_eq!(event["status"], status, "{mode}");
        assert_eq!(event["detail"]["reason"], reason, "{mode}");
        if mode == "new_obstacle" {
            assert!(event["detail"]["block_key"]
                .as_str()
                .unwrap()
                .contains("rear"));
        }
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

#[tokio::test]
async fn horse_recovery_audit_identifies_commands_that_replace_recovery() {
    for reason in ["new_move", "turn", "new_recovery"] {
        let game = make_game_state_with(reason, FlatLand, SeaOnlyWater);
        let id = rider(&game).await;
        game.use_item(&id, 1).await;
        let goal = Position {
            x: 4.0,
            y: 5.0,
            z: 0.0,
        };
        game.recover_horse(&id, 7, goal).await;
        match reason {
            "new_move" => {
                game.update_player_position(&id, move_cmd(goal, true), false)
                    .await
            }
            "turn" => game.turn_horse(&id, 1.0, false).await,
            "new_recovery" => game.recover_horse(&id, 7, goal).await,
            _ => unreachable!(),
        }
        let history =
            serde_json::to_value(game.movement_audit.snapshot(id, Instant::now()).unwrap())
                .unwrap();
        let events = history["recovery_events"].as_array().unwrap();
        assert_eq!(events[1]["status"], "accepted", "{reason}");
        let cancelled: Vec<_> = events
            .iter()
            .filter(|e| e["status"] == "cancelled")
            .collect();
        assert_eq!(cancelled.len(), 1, "{reason}");
        assert_eq!(cancelled[0]["detail"]["reason"], reason);
        assert_eq!(cancelled[0]["request"], events[0]["request"]);
        if reason == "new_recovery" {
            let latest = events.last().unwrap();
            assert_eq!(latest["status"], "accepted");
            assert_ne!(latest["request"]["id"], cancelled[0]["request"]["id"]);
            assert_eq!(
                latest["request"]["request_id"],
                cancelled[0]["request"]["request_id"]
            );
        }
    }
}

async fn boater(game: &GameState) -> PlayerId {
    let mut player = make_player("Boater", -50.0, 0.0);
    player.position.x = -50.0;
    let id = player.id;
    game.add_player(player).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "rowboat", 1)],
            ..Default::default()
        },
    );
    id
}

#[tokio::test]
async fn rowboat_launches_on_water_and_not_on_land() {
    let game = make_test_game_state("rowboat_water");
    let id = boater(&game).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMountChanged {
            mount: Some(MountKind::Rowboat),
            ..
        }
    )));

    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 0.0;
    game.use_item(&id, 1).await;
    assert!(
        !game.players.read().await[&id].is_mounted(),
        "dry land floats no boat"
    );
}

/// The horse throws its rider in combat; a boat has nowhere to throw them.
#[tokio::test]
async fn rowboat_stays_under_its_rider_in_combat() {
    let game = make_test_game_state("rowboat_combat");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.tick_player_movement(0.2).await;
    assert!(game.players.read().await[&id].is_mounted());
}

/// Rowing into the shallows grounds the hull, the mirror of a horse walking
/// into deep water.
#[tokio::test]
async fn rowboat_grounds_itself_in_the_shallows() {
    let game = make_test_game_state("rowboat_ground");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 0.0;
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

/// The client mirrors this in `floatSurfaceY`; disagree and its prediction
/// fights the server's snap-backs.
#[tokio::test]
async fn a_floating_mount_uses_the_water_surface_a_horse_the_bed() {
    let game = make_test_game_state("rowboat_float");
    let at = Position {
        x: -50.0,
        y: 0.0,
        z: 0.0,
    };
    let (bed, depth) = game.ground_and_depth_at(at.x, at.z).await.unwrap();
    assert!(depth > 0.0, "test water expected at x=-50");

    let afloat = game
        .surface_ground_y(0, &at, at.y, Some(MountKind::Rowboat))
        .await;
    assert!(
        (afloat - (bed + depth)).abs() < 1e-3,
        "boat should sit on the surface, got {afloat} for bed {bed} + depth {depth}"
    );
    assert!(afloat > bed, "the surface is above the bed it covers");

    for mount in [None, Some(MountKind::Horse)] {
        let grounded = game.surface_ground_y(0, &at, at.y, mount).await;
        assert!(
            (grounded - bed).abs() < 1e-3,
            "{mount:?} should stand on the bed, got {grounded}"
        );
    }
}

/// The boat is the item, so losing it ends the ride — combat does not.
#[tokio::test]
async fn losing_the_boat_dismounts() {
    let game = make_test_game_state("rowboat_lost");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());

    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .clear();
    game.tick_player_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

/// A new variant with no CSV row would otherwise only fail on use.
#[test]
fn every_mount_kind_has_an_item_that_boards_it() {
    for kind in [MountKind::Horse, MountKind::Rowboat] {
        let def = crate::item_defs::item_defs()
            .get(kind.item_id())
            .unwrap_or_else(|| panic!("{kind:?} names a missing item {}", kind.item_id()));
        match def.use_effect() {
            Some(crate::item_defs::UseEffect::ToggleMount(mapped)) => {
                assert_eq!(mapped, kind, "{} boards the wrong mount", kind.item_id())
            }
            _ => panic!("{} does not board anything", kind.item_id()),
        }
    }
}

/// Combat gates every mounted-movement handler, which suits a horse that is
/// about to be unseated. A boat keeps its rider, so it must keep steering.
#[tokio::test]
async fn a_boat_can_turn_in_combat_a_horse_cannot() {
    let game = make_test_game_state("rowboat_combat_turn");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();

    game.turn_horse(&id, std::f32::consts::FRAC_PI_2, false)
        .await;
    assert!(
        game.movement_intents.read().await.contains_key(&id),
        "the turn should be queued"
    );

    let game = make_test_game_state("horse_combat_turn");
    let id = rider(&game).await;
    game.use_item(&id, 1).await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.turn_horse(&id, std::f32::consts::FRAC_PI_2, false)
        .await;
    assert!(
        !game.movement_intents.read().await.contains_key(&id),
        "the horse ignores the reins in combat"
    );
}

/// Boarding lifts the rider at once and tells the neighbours, rather than
/// waiting for their next step.
#[tokio::test]
async fn boarding_lifts_the_rider_to_the_surface() {
    let game = make_test_game_state("rowboat_lift");
    let id = boater(&game).await;
    let watcher = make_player("Watcher", -50.0, 3.0);
    let watcher_id = watcher.id;
    game.add_player(watcher).await;
    let mut rx = game.register_direct_channel(&watcher_id).await;

    // A wader arrives standing on the bed, which is where the test player
    // must start too — it is minted at sea level, already at the surface.
    let (bed, depth) = game.ground_and_depth_at(-50.0, 0.0).await.unwrap();
    assert!(depth > 0.0);
    game.players.write().await.get_mut(&id).unwrap().position.y = bed;

    game.use_item(&id, 1).await;
    let afloat = game.players.read().await[&id].position.y;
    assert!(
        (afloat - (bed + depth)).abs() < 1e-3,
        "boarding: {bed} -> {afloat}, surface {}",
        bed + depth
    );
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMoved { player_id, position, .. }
            if *player_id == id && (position.y - afloat).abs() < 1e-3
    )));

    game.use_item(&id, 1).await;
    let ashore = game.players.read().await[&id].position.y;
    assert!((ashore - bed).abs() < 1e-3, "leaving sets them back down");
}

#[tokio::test]
async fn boarding_and_leaving_reground_all_queued_waypoints() {
    for boarding in [true, false] {
        let name = if boarding {
            "board_queued"
        } else {
            "leave_queued"
        };
        let game = make_test_game_state(name);
        let id = boater(&game).await;
        let (bed, depth) = game.ground_and_depth_at(-50.0, 0.0).await.unwrap();
        game.players.write().await.get_mut(&id).unwrap().position.y = bed;
        if !boarding {
            game.use_item(&id, 1).await;
        }
        for (z, append) in [(10.0, false), (20.0, true)] {
            game.update_player_position(
                &id,
                move_cmd(
                    Position {
                        x: -50.0,
                        y: bed,
                        z,
                    },
                    append,
                ),
                false,
            )
            .await;
        }

        game.use_item(&id, 1).await;
        let expected_y = if boarding { bed + depth } else { bed };
        assert_eq!(game.movement_intents.read().await[&id].len(), 2);
        game.tick_player_movement(1.0).await;
        assert!(game.players.read().await[&id].position.z > 1.0);
        for _ in 0..10 {
            let player = game.players.read().await[&id].clone();
            assert_eq!(player.is_mounted(), boarding);
            assert!(
                (player.position.y - expected_y).abs() < 1e-3,
                "{name}: {:?}, expected y {expected_y}",
                player.position
            );
            game.tick_player_movement(1.0).await;
        }
        assert!(!game.movement_intents.read().await.contains_key(&id));
        assert!(game.players.read().await[&id].position.z >= 19.0);
    }
}
