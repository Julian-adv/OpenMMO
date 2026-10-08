use super::*;

async fn setup(name: &str) -> (GameState, PlayerId, DirectRx) {
    let game = make_test_game_state(name);
    let id = pid("auto_attacker");
    game.add_player(make_player("auto_attacker", 0.0, 0.0))
        .await;
    for target in ["m1", "m2"] {
        let mut monster = make_monster(target, pos(1.5), 0);
        monster.health = 10000;
        monster.max_health = 10000;
        game.monsters.write().await.insert(target.into(), monster);
    }
    let rx = game.register_direct_channel(&id).await;
    (game, id, rx)
}

async fn elapse_attack_time(game: &GameState, ms: u64) {
    // Keep the wall-clock cooldown and paused Tokio clock in step.
    for last in game.last_player_attacks.write().await.values_mut() {
        *last = last.saturating_sub(ms);
    }
    tokio::time::advance(std::time::Duration::from_millis(ms)).await;
}

fn attacks(rx: &mut DirectRx, id: PlayerId) -> Vec<String> {
    drain(rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerAttacked {
                player_id,
                monster_id,
                ..
            } if player_id == id => Some(monster_id),
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn scheduler_waits_for_each_players_deadline() {
    let (game, first, mut first_rx) = setup("attack_deadlines").await;
    game.start_player_attack(first, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut first_rx, first), ["m1"]);
    elapse_attack_time(&game, 500).await;
    game.add_player(make_player("later", 0.0, 0.0)).await;
    let later = pid("later");
    let mut later_rx = game.register_direct_channel(&later).await;
    game.start_player_attack(later, "m2".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut later_rx, later), ["m2"]);

    let wait = game.wait_for_player_attack();
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    elapse_attack_time(&game, 500).await;
    assert!(futures_util::poll!(&mut wait).is_pending());
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut first_rx, first).is_empty());
    assert!(attacks(&mut later_rx, later).is_empty());

    elapse_attack_time(&game, 600).await;
    assert!(futures_util::poll!(&mut wait).is_ready());
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut first_rx, first), ["m1"]);
    assert!(attacks(&mut later_rx, later).is_empty());
    elapse_attack_time(&game, 500).await;
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut first_rx, first).is_empty());
    assert_eq!(attacks(&mut later_rx, later), ["m2"]);
}

#[tokio::test(start_paused = true)]
async fn an_earlier_reservation_interrupts_the_existing_wait() {
    let (game, slow, mut slow_rx) = setup("attack_earlier_reservation").await;
    game.register_hunger(&slow, 50).await;
    game.start_player_attack(slow, "m1".into(), false, 1, None)
        .await;
    attacks(&mut slow_rx, slow);
    let wait = game.wait_for_player_attack();
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    elapse_attack_time(&game, 100).await;
    game.add_player(make_player("earlier", 0.0, 0.0)).await;
    let earlier = pid("earlier");
    let mut earlier_rx = game.register_direct_channel(&earlier).await;
    game.start_player_attack(earlier, "m2".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut earlier_rx, earlier), ["m2"]);
    assert!(futures_util::poll!(&mut wait).is_pending());
    elapse_attack_time(&game, 1600).await;
    assert!(futures_util::poll!(&mut wait).is_ready());
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut earlier_rx, earlier), ["m2"]);
    assert!(attacks(&mut slow_rx, slow).is_empty());
}

#[tokio::test(start_paused = true)]
async fn attack_speed_bonuses_schedule_an_earlier_swing() {
    let (game, id, mut rx) = setup("attack_speed_bonus_schedule").await;
    game.register_hunger(&id, 700).await;
    assert!(game.inflict_debuff(&id, "tipsy", Some(true)).await);
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    elapse_attack_time(&game, 1450).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
}

#[tokio::test(start_paused = true)]
async fn debuff_expiry_rechecks_the_attack_without_waiting_for_a_sweep() {
    let (game, id, mut rx) = setup("attack_debuff_expiry_schedule").await;
    game.register_hunger(&id, 700).await;
    assert!(game.inflict_debuff(&id, "cold", Some(true)).await);
    game.hunger.write().await.get_mut(&id).unwrap().debuffs[0].until =
        tokio::time::Instant::now() + std::time::Duration::from_millis(1600);
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    elapse_attack_time(&game, 1650).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test(start_paused = true)]
async fn an_action_wakes_the_scheduler_before_the_next_swing() {
    let (game, id, mut rx) = setup("attack_action_wakeup").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    let wait = game.wait_for_player_attack();
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    game.broadcast_pickup_animation(&id).await;
    assert!(futures_util::poll!(&mut wait).is_ready());
    game.process_due_player_attacks(None).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { request_id: 1, .. })));
    elapse_attack_time(&game, 5000).await;
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
    let idle = game.wait_for_player_attack();
    tokio::pin!(idle);
    assert!(futures_util::poll!(&mut idle).is_pending());
}

#[tokio::test(start_paused = true)]
async fn eating_moves_a_slow_attack_deadline_forward() {
    let (game, id, mut rx) = setup("attack_hunger_recovery").await;
    game.register_hunger(&id, 50).await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    elapse_attack_time(&game, 1600).await;
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
    let wait = game.wait_for_player_attack();
    tokio::pin!(wait);
    assert!(futures_util::poll!(&mut wait).is_pending());
    game.apply_eat(&id, 100).await;
    assert!(futures_util::poll!(&mut wait).is_ready());
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test(start_paused = true)]
async fn becoming_weak_postpones_only_that_players_attack() {
    let (game, id, mut rx) = setup("attack_hunger_slowdown").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    elapse_attack_time(&game, 500).await;
    game.register_hunger(&id, 50).await;
    game.process_due_player_attacks(None).await;
    elapse_attack_time(&game, 1100).await;
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
    elapse_attack_time(&game, 500).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test(start_paused = true)]
async fn overdue_reservations_do_not_replay_missed_swings() {
    let (game, id, mut rx) = setup("attack_overdue_reservation").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    elapse_attack_time(&game, 60000).await;
    for _ in 0..10 {
        game.process_due_player_attacks(None).await;
    }
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test(start_paused = true)]
async fn scheduler_loop_wakes_from_idle_and_drains_on_shutdown() {
    let (game, id, mut rx) = setup("attack_scheduler_loop").await;
    let game = std::sync::Arc::new(game);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
    let task = tokio::spawn(crate::run_player_attacks(game.clone(), None, shutdown_rx));
    tokio::task::yield_now().await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    tokio::task::yield_now().await;
    elapse_attack_time(&game, 1600).await;
    tokio::task::yield_now().await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.stop_player_attack(&id).await;
    drain(&mut rx);
    elapse_attack_time(&game, 5000).await;
    tokio::task::yield_now().await;
    assert!(attacks(&mut rx, id).is_empty());
    shutdown_tx.send(()).unwrap();
    task.await.unwrap();
}

#[tokio::test]
async fn auto_attack_repeats_without_more_client_requests() {
    let (game, id, mut rx) = setup("auto_attack_repeats").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
}

#[tokio::test]
async fn repeated_starts_and_target_changes_keep_the_existing_cooldown() {
    let (game, id, mut rx) = setup("auto_attack_spam").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    let last = game.last_player_attacks.read().await[&id];
    for request in 2..20 {
        game.start_player_attack(id, "m1".into(), false, request, None)
            .await;
    }
    game.start_player_attack(id, "m2".into(), false, 20, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    assert_eq!(game.last_player_attacks.read().await[&id], last);
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m2"]);
}

#[tokio::test]
async fn stop_and_restart_keep_the_existing_cooldown() {
    let (game, id, mut rx) = setup("auto_attack_restart").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.stop_player_attack_request(&id, Some(1)).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { request_id: 1, .. })));
    game.start_player_attack(id, "m1".into(), false, 2, None)
        .await;
    assert!(attacks(&mut rx, id).is_empty());
    game.stop_player_attack_request(&id, Some(1)).await;
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test]
async fn a_stall_never_causes_a_burst_of_catch_up_attacks() {
    let (game, id, mut rx) = setup("auto_attack_stall").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    game.last_player_attacks
        .write()
        .await
        .insert(id, GameState::now_ms() - 60000);
    game.wake_player_attack(&id).await;
    for _ in 0..10 {
        game.process_due_player_attacks(None).await;
    }
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test]
async fn a_rejected_cycle_stops_instead_of_retrying() {
    let (game, id, mut rx) = setup("auto_attack_rejected").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    game.monsters
        .write()
        .await
        .get_mut("m1")
        .unwrap()
        .position
        .x = 30.0;
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|m| matches!(
        m,
        ServerMessage::PlayerAttackRejected {
            reason: AttackRejectReason::OutOfRange,
            ..
        }
    )));
    assert!(messages
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { request_id: 1, .. })));
    game.process_due_player_attacks(None).await;
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn movement_teleport_death_and_disconnect_stop_auto_attacks() {
    for action in ["goal", "direction", "teleport", "death", "disconnect"] {
        let (game, id, mut rx) = setup(&format!("auto_attack_stop_{action}")).await;
        game.start_player_attack(id, "m1".into(), false, 1, None)
            .await;
        attacks(&mut rx, id);
        match action {
            "goal" => game.request_move_goal(id, 1, 4.0, 0.0, false).await,
            "direction" => game.request_move_direction(id, 1, 0.0, 1, 0, false).await,
            "teleport" => game.teleport_player(&id, pos(4.0), 0.0, 0).await,
            "death" => {
                game.players.write().await.get_mut(&id).unwrap().health = 0;
                game.on_player_died(&id, "test").await;
            }
            "disconnect" => game.remove_player(&id).await,
            _ => unreachable!(),
        }
        game.process_due_player_attacks(None).await;
        assert!(
            action == "disconnect"
                || drain(&mut rx)
                    .iter()
                    .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { .. })),
            "{action}"
        );
        assert!(
            !game.auto_attacks.read().await.contains_key(&id),
            "{action}"
        );
        ready(&game, id).await;
        game.process_due_player_attacks(None).await;
        assert!(attacks(&mut rx, id).is_empty(), "{action}");
    }
}

#[tokio::test]
async fn restarting_right_after_another_action_stops_the_old_request_first() {
    let (game, id, mut rx) = setup("auto_attack_stale_restart").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.request_move_goal(id, 1, 4.0, 0.0, false).await;
    game.start_player_attack(id, "m1".into(), false, 2, None)
        .await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { request_id: 1, .. })));
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(
        |m| matches!(m, ServerMessage::PlayerAttacked { monster_id, .. } if monster_id == "m1")
    ));
    assert!(!messages
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { .. })));
}

#[tokio::test]
async fn other_actions_stop_auto_attacks_without_a_stop_request() {
    for action in [
        "emote",
        "interaction",
        "inspection",
        "pickup_pose",
        "pickup_item",
        "pickup_nearby",
    ] {
        let (game, id, mut rx) = setup(&format!("auto_attack_action_{action}")).await;
        game.start_player_attack(id, "m1".into(), false, 1, None)
            .await;
        assert_eq!(attacks(&mut rx, id), ["m1"]);
        match action {
            "emote" => {
                let auth = make_test_auth("auto_attack_emote");
                game.send_chat_message(&id, "/emote excited".into(), &auth)
                    .await;
                assert_eq!(
                    game.players.read().await[&id].object_type.as_deref(),
                    Some("excited")
                );
            }
            "interaction" => {
                game.set_player_interaction(&id, Some("excited".into()), None)
                    .await
            }
            "inspection" => {
                game.use_targeted_ability(
                    &id,
                    onlinerpg_shared::ability::AbilityId::Auscultation,
                    Some("m1"),
                    None,
                    None,
                )
                .await
            }
            "pickup_pose" => game.broadcast_pickup_animation(&id).await,
            "pickup_item" => game.pickup_item(&id, 999).await,
            "pickup_nearby" => game.pickup_nearby_items(&id).await,
            _ => unreachable!(),
        }
        game.process_due_player_attacks(None).await;
        assert!(
            drain(&mut rx)
                .iter()
                .any(|m| matches!(m, ServerMessage::PlayerAttackStopped { request_id: 1, .. })),
            "{action}"
        );
        assert!(
            !game.auto_attacks.read().await.contains_key(&id),
            "{action}"
        );
        ready(&game, id).await;
        game.process_due_player_attacks(None).await;
        assert!(attacks(&mut rx, id).is_empty(), "{action}");
    }
}

#[tokio::test]
async fn restarting_an_attack_ends_the_emote_on_the_server() {
    let (game, id, mut rx) = setup("auto_attack_ends_emote").await;
    game.set_player_interaction(&id, Some("excited".into()), None)
        .await;
    drain(&mut rx);
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    assert!(game.players.read().await[&id].object_type.is_none());
    assert!(game.players.read().await[&id].object_id.is_none());
}

#[tokio::test]
async fn queued_dagger_requests_wait_for_the_next_attack_cycle() {
    let (game, id, mut rx) = setup("auto_attack_queued_skill").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    game.set_player_attack_skill(&id, 1, true).await;
    game.process_due_player_attacks(None).await;
    assert!(drain(&mut rx).is_empty());
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    let messages = drain(&mut rx);
    assert!(messages
        .iter()
        .any(|m| matches!(m, ServerMessage::DaggerDoubleSlashRejected { .. })));
    assert!(messages
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
}

#[tokio::test]
async fn weak_hunger_extends_the_server_driven_cadence() {
    let (game, id, mut rx) = setup("auto_attack_weak").await;
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    attacks(&mut rx, id);
    game.register_hunger(&id, 50).await;
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    assert!(attacks(&mut rx, id).is_empty());
    let interval = (*super::super::combat::PLAYER_ATTACK_INTERVAL_MS as f32
        / onlinerpg_shared::hunger::WEAK_ATTACK_MULT)
        .ceil() as u64;
    game.last_player_attacks
        .write()
        .await
        .insert(id, GameState::now_ms() - interval);
    game.wake_player_attack(&id).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}

#[tokio::test(start_paused = true)]
async fn standing_up_from_a_bed_to_attack_keeps_the_auto_attack() {
    let (game, id, mut rx) = setup("auto_attack_from_bed").await;
    let bed = Position {
        x: 100.0,
        y: 5.0,
        z: 50.0,
    };
    game.players.write().await.get_mut(&id).unwrap().position = bed;
    game.monsters.write().await.get_mut("m1").unwrap().position = Position { x: 101.5, ..bed };
    game.sync_region_furniture(
        0,
        0,
        &[onlinerpg_shared::furniture::FurniturePlacement {
            id: 7,
            type_id: "bed".into(),
            x: bed.x,
            y: bed.y,
            z: bed.z,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    tokio::time::advance(std::time::Duration::from_secs(26)).await;
    game.set_player_interaction(&id, Some("bed".into()), Some(7))
        .await;
    assert_eq!(
        game.players.read().await[&id].object_type.as_deref(),
        Some("bed")
    );
    drain(&mut rx);
    game.start_player_attack(id, "m1".into(), false, 1, None)
        .await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
    assert!(game.players.read().await[&id].object_type.is_none());
    ready(&game, id).await;
    game.process_due_player_attacks(None).await;
    assert_eq!(attacks(&mut rx, id), ["m1"]);
}
