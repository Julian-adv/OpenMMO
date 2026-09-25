use super::*;

/// Play the fight with a fixed stance policy until the session ends,
/// returning the outcome (plus every message seen on the way). The
/// policy sees each `FishingFight` beat's state and tension — exactly
/// what a real client (human gauge or agent reflex) gets.
pub(super) async fn fight_to_the_end(
    game_state: &GameState,
    id: &PlayerId,
    rx: &mut DirectRx,
    policy: impl Fn(FishState, u32, bool) -> FishingAction,
) -> (FishingOutcome, Vec<ServerMessage>) {
    fight_to_the_end_with_auth(game_state, id, rx, policy, None).await
}

async fn fight_to_the_end_with_auth(
    game_state: &GameState,
    id: &PlayerId,
    rx: &mut DirectRx,
    policy: impl Fn(FishState, u32, bool) -> FishingAction,
    auth: Option<&crate::auth::AuthService>,
) -> (FishingOutcome, Vec<ServerMessage>) {
    let mut seen = Vec::new();
    // Budget: the fight timeout plus slack, in 250 ms ticks.
    for _ in 0..(FIGHT_TIMEOUT_MS / 250 + 40) {
        for msg in drain(rx) {
            match &msg {
                ServerMessage::FishingFight {
                    player_id,
                    fish_state,
                    tension_pct,
                    trophy,
                    ..
                } if player_id == id => {
                    let action = policy(*fish_state, *tension_pct, *trophy);
                    seen.push(msg.clone());
                    game_state.respond_fishing(id, action).await;
                }
                ServerMessage::FishingEnded { outcome, .. } => {
                    seen.push(msg.clone());
                    return (outcome.clone(), seen);
                }
                _ => seen.push(msg.clone()),
            }
        }
        advance(Duration::from_millis(250)).await;
        game_state.tick_fishing(auth).await;
    }
    panic!("the fight never ended");
}

async fn hook_forced_fish(
    game_state: &GameState,
    id: &PlayerId,
    rx: &mut DirectRx,
    item_def_id: &str,
) {
    game_state.start_fishing(id, water_target()).await;
    advance_until_bite(game_state, rx).await;
    {
        let mut sessions = game_state.fishing_sessions.write().await;
        let fish = sessions.get_mut(id).unwrap().rolled_fish.as_mut().unwrap();
        fish.item_def_id = item_def_id.to_string();
        fish.rarity = game_state
            .item_defs
            .get(item_def_id)
            .unwrap()
            .rarity_tier
            .unwrap_or(0);
        fish.trophy = false;
    }
    game_state.respond_fishing(id, FishingAction::Hook).await;
}

#[tokio::test(start_paused = true)]
async fn cast_requires_rod_water_and_range() {
    let game_state = make_test_game_state("fishing_cast_validation");
    let (id, mut rx) = make_angler(&game_state, "angler_val").await;

    // Land (positive x in the split world) is refused.
    game_state
        .start_fishing(
            &id,
            Position {
                x: 200.0,
                y: 0.0,
                z: 50.0,
            },
        )
        .await;
    // Out of range (>8m) is refused even over water.
    game_state
        .start_fishing(
            &id,
            Position {
                x: -150.0,
                y: 0.0,
                z: 50.0,
            },
        )
        .await;
    let errors = drain(&mut rx)
        .into_iter()
        .filter(|m| matches!(m, ServerMessage::FishingError { .. }))
        .count();
    assert_eq!(errors, 2);

    // No rod → refused.
    let bare = pid("angler_bare");
    game_state
        .add_player(make_player("angler_bare", -100.0, 50.0))
        .await;
    game_state.inventories.write().await.insert(
        bare,
        PlayerInventory {
            active_ammo: None,
            bag: vec![],
            equipped: Default::default(),
        },
    );
    let mut bare_rx = game_state.register_direct_channel(&bare).await;
    game_state.start_fishing(&bare, water_target()).await;
    assert!(drain(&mut bare_rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::FishingError { .. })));

    // Rod + water + in range → the cast broadcast reaches the angler.
    game_state.start_fishing(&id, water_target()).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::FishingCasted { .. })));
}

// Bystanders take the facing from the broadcast (FishingCasted's rotation doc).
#[tokio::test(start_paused = true)]
async fn cast_broadcast_faces_the_water() {
    let game_state = make_test_game_state("fishing_cast_facing");
    let (id, mut rx) = make_angler(&game_state, "angler_facing").await;

    game_state.start_fishing(&id, water_target()).await;
    let rotation = drain(&mut rx)
        .iter()
        .find_map(|m| match m {
            ServerMessage::FishingCasted { rotation, .. } => Some(*rotation),
            _ => None,
        })
        .expect("cast should be accepted");
    // Angler at (-100, 50), water at (-103, 50): facing is atan2(-3, 0).
    let expected = (-3.0f32).atan2(0.0);
    assert!(
        (rotation - expected).abs() < 1e-6,
        "expected rotation {expected}, got {rotation}"
    );
}

#[tokio::test(start_paused = true)]
async fn rowboat_casts_only_astern_without_turning() {
    let game_state = make_test_game_state("fishing_rowboat_facing");
    let (id, mut rx) = make_angler(&game_state, "angler_rowboat").await;
    {
        let mut players = game_state.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.mount = Some(onlinerpg_shared::mount::MountKind::Rowboat);
        player.rotation = 0.0;
    }

    for (x, z) in [
        (-100.0, 54.0),
        (-104.0, 50.0),
        (-104.0, 49.0),
        (-100.0, 50.0),
    ] {
        game_state
            .start_fishing(&id, Position { x, y: 0.0, z })
            .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::FishingError { message }
                if message == "Cast into the water behind the boat."
        )));
        assert!(!game_state.fishing_sessions.read().await.contains_key(&id));
    }

    let target = Position {
        x: -98.0,
        y: 0.0,
        z: 46.0,
    };
    game_state.start_fishing(&id, target).await;
    let (position, rotation) = drain(&mut rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::FishingCasted {
                position, rotation, ..
            } => Some((position, rotation)),
            _ => None,
        })
        .expect("stern cast should be accepted");
    assert_eq!(position.x, target.x);
    assert_eq!(position.z, target.z);
    assert_eq!(rotation, 0.0, "bystanders must keep the boat's heading");
    assert_eq!(game_state.players.read().await[&id].rotation, 0.0);
}

// The regression this PR fixes: a river's bed sits ABOVE sea level (its
// carved channel bottoms out at sea level and climbs into the hills), so
// the old `terrain height < 0` water test rejected every inland river.
// With the unified water field, a river surface above its bed reads as
// water and the cast lands — end to end, all the way to a caught fish.
#[tokio::test(start_paused = true)]
async fn fishing_works_in_a_river_above_sea_level() {
    let game_state = make_river_game_state("fishing_river");
    // Plateau terrain is +5 m; the player and the water are both up there.
    let (id, mut rx) = make_angler(&game_state, "angler_river").await;

    game_state.start_fishing(&id, water_target()).await;
    let casted = drain(&mut rx);
    let bobber = casted.iter().find_map(|m| match m {
        ServerMessage::FishingCasted { position, .. } => Some(*position),
        _ => None,
    });
    let bobber = bobber.expect("river cast should be accepted, not refused as land");
    // The bobber floats on the river surface (~5.4 m), not at sea level.
    assert!(
        bobber.y > 5.0,
        "bobber should sit on the river surface, got y={}",
        bobber.y
    );

    // And the whole loop still resolves to a catch over the river.
    advance_until_bite(&game_state, &mut rx).await;
    game_state.respond_fishing(&id, FishingAction::Hook).await;
    let (outcome, _) = fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(
        matches!(outcome, FishingOutcome::Caught { .. }),
        "perfect play should land a river fish, got {outcome:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn landing_a_golden_sturgeon_earns_the_angler_title() {
    let game_state = make_test_game_state("fishing_title_flow");
    let (id, mut rx) = make_angler(&game_state, "angler_title").await;
    game_state.set_player_titles(&id, Vec::new()).await;

    hook_forced_fish(&game_state, &id, &mut rx, "golden_sturgeon").await;
    let (outcome, msgs) = fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));

    let titles = game_state
        .player_titles
        .read()
        .await
        .get(&id)
        .cloned()
        .unwrap_or_default();
    assert_eq!(titles, ["sturgeon_angler"]);
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::TitleEarned { title } if title == "sturgeon_angler")));
}

#[tokio::test(start_paused = true)]
async fn golden_sturgeon_title_is_persisted_before_disconnect() {
    let auth = make_test_auth("fishing_title_disconnect");
    let account = auth.login_npc("npc_fishing_title_disconnect").unwrap();
    let character = create_test_character(&auth, &account, "Sturgeon");
    let game_state = make_test_game_state("fishing_title_disconnect");
    let (id, mut rx) = make_angler(&game_state, "Sturgeon").await;
    game_state
        .register_player_character(&id, character.id, 0, attrs_with_cha(10), 0, None)
        .await;
    game_state.set_player_titles(&id, Vec::new()).await;

    hook_forced_fish(&game_state, &id, &mut rx, "golden_sturgeon").await;
    let (outcome, _) =
        fight_to_the_end_with_auth(&game_state, &id, &mut rx, auto_stance, Some(&auth)).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));

    game_state.unregister_player_character(&id).await;
    assert_eq!(
        auth.load_titles(character.id).unwrap().0,
        ["sturgeon_angler"]
    );
}

#[tokio::test(start_paused = true)]
async fn full_catch_flow_awards_fish_without_changing_skills() {
    let game_state = make_test_game_state("fishing_catch_flow");
    let (id, mut rx) = make_angler(&game_state, "angler_catch").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;

    game_state.respond_fishing(&id, FishingAction::Hook).await;
    // Answer every struggle round correctly: the catch must land.
    let (outcome, msgs) = fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    let FishingOutcome::Caught {
        item_def_id: fish_id,
        size_cm,
        ..
    } = outcome
    else {
        panic!("perfect struggle play must catch, got {outcome:?}");
    };
    // The fight broadcast its beats, and the fish was visibly exhausted
    // before it could be landed.
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::FishingFight { .. })));
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::FishingFight {
            fish_state: FishState::Exhausted,
            ..
        }
    )));
    assert!(size_cm > 0);
    // All catches arrive as inventory items.
    let inv = game_state.get_player_inventory(&id).await.unwrap();
    assert!(inv
        .bag
        .iter()
        .any(|item| item.item_def_id == fish_id && item.quantity == 1));
    assert!(msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::InventoryUpdated { .. })));
    assert!(!game_state.dirty_skills.read().await.contains(&id));
    // Session is gone: a second hook is an error, not a double catch.
    game_state.respond_fishing(&id, FishingAction::Hook).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::FishingError { .. })));
}

#[tokio::test(start_paused = true)]
async fn ignored_bite_escapes_without_changing_skills() {
    let game_state = make_test_game_state("fishing_bite_timeout");
    let (id, mut rx) = make_angler(&game_state, "angler_afk").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;

    // Sleep through the bite window plus both grace budgets.
    advance_with_ticks(
        &game_state,
        u64::from(BITE_WINDOW_MS + 2 * LATENCY_GRACE_MS + 500),
    )
    .await;
    let msgs = drain(&mut rx);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Escaped,
            ..
        }
    )));
    assert!(!game_state.dirty_skills.read().await.contains(&id));
}

#[tokio::test(start_paused = true)]
async fn hooking_early_scares_the_fish_off() {
    let game_state = make_test_game_state("fishing_early_hook");
    let (id, mut rx) = make_angler(&game_state, "angler_eager").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_with_ticks(&game_state, u64::from(CAST_MS) + 500).await;
    drain(&mut rx);

    game_state.respond_fishing(&id, FishingAction::Hook).await;
    let msgs = drain(&mut rx);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Escaped,
            ..
        }
    )));
    assert!(!game_state.dirty_skills.read().await.contains(&id));
}

#[tokio::test(start_paused = true)]
async fn moving_aborts_the_session() {
    let game_state = make_test_game_state("fishing_move_cancels");
    let (id, mut rx) = make_angler(&game_state, "angler_wanderer").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_with_ticks(&game_state, u64::from(CAST_MS) + 500).await;
    drain(&mut rx);

    // Any real relocation funnels through finish_position_update, which
    // breaks the session.
    game_state
        .update_player_position(
            &id,
            move_cmd(
                Position {
                    x: -95.0,
                    y: 0.0,
                    z: 50.0,
                },
                false,
            ),
            false,
        )
        .await;
    game_state.tick_player_movement(1.0).await;
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Aborted,
            ..
        }
    )));
    // Idempotent: cancelling again stays quiet.
    game_state.cancel_fishing_if_active(&id).await;
    assert!(drain(&mut rx).is_empty());
}

/// A duplicated Hook (client double-send racing the fight open) must be
/// swallowed — not end the session, not change the stance.
#[tokio::test(start_paused = true)]
async fn duplicate_hook_during_the_fight_is_ignored() {
    let game_state = make_test_game_state("fishing_dup_hook");
    let (id, mut rx) = make_angler(&game_state, "angler_doublehook").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;
    game_state.respond_fishing(&id, FishingAction::Hook).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::FishingFight { .. })));

    game_state.respond_fishing(&id, FishingAction::Hook).await;
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|m| matches!(m, ServerMessage::FishingEnded { .. })),
        "a duplicate hook must not end the fight"
    );

    // The fight is intact and still winnable.
    let (outcome, _) = fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));
}

#[tokio::test(start_paused = true)]
async fn fight_broadcasts_the_current_reel_stance() {
    let game_state = make_test_game_state("fishing_reel_stance");
    let (id, mut rx) = make_angler(&game_state, "angler_reel_stance").await;
    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;
    game_state.respond_fishing(&id, FishingAction::Hook).await;
    assert!(drain(&mut rx).iter().any(|msg| matches!(
        msg,
        ServerMessage::FishingFight {
            stance: FishingAction::Hold,
            ..
        }
    )));
    for expected in [
        FishingAction::Reel,
        FishingAction::GiveLine,
        FishingAction::Hold,
    ] {
        game_state.respond_fishing(&id, expected).await;
        advance(Duration::from_millis(250)).await;
        game_state.tick_fishing(None).await;
        assert!(drain(&mut rx).iter().any(|msg| matches!(msg,
            ServerMessage::FishingFight { stance, .. } if *stance == expected
        )));
    }
}

// Species stacking is tested with deterministic awards in inventory_tests.
#[tokio::test(start_paused = true)]
async fn every_catch_lands_in_the_bag() {
    let game_state = make_test_game_state("fishing_catches_bagged");
    let (id, mut rx) = make_angler(&game_state, "angler_bagger").await;

    for _ in 0..3 {
        game_state.start_fishing(&id, water_target()).await;
        advance_until_bite(&game_state, &mut rx).await;
        game_state.respond_fishing(&id, FishingAction::Hook).await;
        let (outcome, _) = fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
        assert!(
            matches!(outcome, FishingOutcome::Caught { .. }),
            "perfect play must catch"
        );
    }
    let inv = game_state.get_player_inventory(&id).await.unwrap();
    let total: u32 = inv.bag.iter().map(|item| item.quantity).sum();
    assert_eq!(total, 3);
}

// The fight: cranking the reel against a running fish pumps tension
// until the line snaps.
#[tokio::test(start_paused = true)]
async fn reeling_against_every_run_snaps_the_line() {
    let game_state = make_test_game_state("fishing_fight_wrong");
    let (id, mut rx) = make_angler(&game_state, "angler_clumsy").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;
    game_state.respond_fishing(&id, FishingAction::Hook).await;

    let (outcome, msgs) =
        fight_to_the_end(&game_state, &id, &mut rx, |_, _, _| FishingAction::Reel).await;
    assert_eq!(outcome, FishingOutcome::Escaped);
    // The snap came from tension, not the timeout: the fight died young.
    let beats = msgs
        .iter()
        .filter(|m| matches!(m, ServerMessage::FishingFight { .. }))
        .count();
    assert!(
        (beats as u32) * 250 < FIGHT_TIMEOUT_MS / 2,
        "reel-only play must snap early, saw {beats} beats"
    );
}

/// Ignoring a hooked fish eventually loses it.
#[tokio::test(start_paused = true)]
async fn ignoring_the_fight_escapes_the_fish() {
    let game_state = make_test_game_state("fishing_fight_afk");
    let (id, mut rx) = make_angler(&game_state, "angler_frozen").await;

    game_state.start_fishing(&id, water_target()).await;
    advance_until_bite(&game_state, &mut rx).await;
    game_state.respond_fishing(&id, FishingAction::Hook).await;

    let mut msgs = Vec::new();
    let mut outcome = None;
    'outer: for _ in 0..(FIGHT_TIMEOUT_MS / 250 + 40) {
        for msg in drain(&mut rx) {
            if let ServerMessage::FishingEnded { outcome: o, .. } = &msg {
                outcome = Some(o.clone());
                msgs.push(msg);
                break 'outer;
            }
            msgs.push(msg);
        }
        advance(Duration::from_millis(250)).await;
        game_state.tick_fishing(None).await;
    }
    assert_eq!(outcome, Some(FishingOutcome::Escaped));
    assert!(!game_state.dirty_skills.read().await.contains(&id));
}

// ---- PR9 hardening: concurrency, radius, aborts, overflow, consumption ----

/// Two anglers control separate sessions and receive their own catches.
#[tokio::test(start_paused = true)]
async fn two_anglers_fish_independently() {
    let game_state = make_test_game_state("fishing_two_anglers");
    let (a, mut rx_a) = make_angler(&game_state, "angler_left").await;
    let (b, mut rx_b) = make_angler(&game_state, "angler_right").await;

    game_state.start_fishing(&a, water_target()).await;
    game_state.start_fishing(&b, water_target()).await;

    let mut ended: std::collections::HashMap<PlayerId, FishingOutcome> = Default::default();
    let mut caught: std::collections::HashMap<PlayerId, String> = Default::default();
    for _ in 0..400 {
        if ended.len() == 2 {
            break;
        }
        for (me, rx) in [(a, &mut rx_a), (b, &mut rx_b)] {
            for msg in drain(rx) {
                match msg {
                    ServerMessage::FishingBite { player_id } if player_id == me => {
                        game_state.respond_fishing(&me, FishingAction::Hook).await;
                    }
                    ServerMessage::FishingFight {
                        player_id,
                        fish_state,
                        tension_pct,
                        trophy,
                        ..
                    } if player_id == me => {
                        game_state
                            .respond_fishing(&me, auto_stance(fish_state, tension_pct, trophy))
                            .await;
                    }
                    ServerMessage::FishingEnded { player_id, outcome } if player_id == me => {
                        if let FishingOutcome::Caught {
                            ref item_def_id, ..
                        } = outcome
                        {
                            caught.insert(me, item_def_id.clone());
                        }
                        ended.insert(me, outcome);
                    }
                    _ => {}
                }
            }
        }
        advance(Duration::from_millis(250)).await;
        game_state.tick_fishing(None).await;
    }

    assert_eq!(ended.len(), 2, "both anglers must finish their sessions");
    for (me, outcome) in &ended {
        assert!(
            matches!(outcome, FishingOutcome::Caught { .. }),
            "correct play must land the catch for {me}: {outcome:?}"
        );
        let inv = game_state.get_player_inventory(me).await.unwrap();
        let caught_id = caught.get(me).expect("caught id");
        assert!(inv.bag.iter().any(|item| &item.item_def_id == caught_id));
        assert!(!game_state.dirty_skills.read().await.contains(me));
    }
}

/// Responding with no session gets a direct error and must not touch
/// anyone else's line.
#[tokio::test(start_paused = true)]
async fn responding_without_a_session_is_an_error_and_touches_nobody() {
    let game_state = make_test_game_state("fishing_kibitzer");
    let (a, mut rx_a) = make_angler(&game_state, "angler_focused").await;
    let b = pid("kibitzer");
    game_state
        .add_player(make_player("kibitzer", -100.0, 50.0))
        .await;
    let mut rx_b = game_state.register_direct_channel(&b).await;

    game_state.start_fishing(&a, water_target()).await;
    game_state.respond_fishing(&b, FishingAction::Hook).await;
    assert!(
        drain(&mut rx_b)
            .iter()
            .any(|m| matches!(m, ServerMessage::FishingError { .. })),
        "the kibitzer is told they are not fishing"
    );

    // The angler's session is unaffected: bite, fight, catch as normal.
    advance_until_bite(&game_state, &mut rx_a).await;
    game_state.respond_fishing(&a, FishingAction::Hook).await;
    let (outcome, _) = fight_to_the_end(&game_state, &a, &mut rx_a, auto_stance).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));
}

/// Fishing broadcasts reach a bystander on the shore but not someone beyond
/// the delivery radius — bobbers render for neighbors, not the whole server.
#[tokio::test(start_paused = true)]
async fn fishing_broadcasts_are_radius_gated() {
    let game_state = make_test_game_state("fishing_radius");
    let (a, mut rx_a) = make_angler(&game_state, "angler_star").await;
    // Bobber lands at (-103, 50); 43 m delivery radius (shared::world).
    let near = pid("bystander_near");
    game_state
        .add_player(make_player("bystander_near", -90.0, 50.0))
        .await;
    let mut rx_near = game_state.register_direct_channel(&near).await;
    let far = pid("bystander_far");
    game_state
        .add_player(make_player("bystander_far", -40.0, 50.0))
        .await;
    let mut rx_far = game_state.register_direct_channel(&far).await;

    game_state.start_fishing(&a, water_target()).await;
    advance_until_bite(&game_state, &mut rx_a).await;
    game_state.respond_fishing(&a, FishingAction::Hook).await;
    let (outcome, _) = fight_to_the_end(&game_state, &a, &mut rx_a, auto_stance).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));

    fn fishing_msg_count(msgs: &[ServerMessage]) -> usize {
        msgs.iter()
            .filter(|m| {
                matches!(
                    m,
                    ServerMessage::FishingCasted { .. }
                        | ServerMessage::FishingBite { .. }
                        | ServerMessage::FishingFight { .. }
                        | ServerMessage::FishingEnded { .. }
                )
            })
            .count()
    }
    // The near bystander sees every phase — not just the cast. Asserting
    // each kind separately keeps a partial regression (e.g. the fight going
    // direct-only) from hiding behind the bobber broadcast.
    let near_msgs = drain(&mut rx_near);
    for (name, seen) in [
        (
            "FishingCasted",
            near_msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::FishingCasted { .. })),
        ),
        (
            "FishingBite",
            near_msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::FishingBite { .. })),
        ),
        (
            "FishingFight",
            near_msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::FishingFight { .. })),
        ),
        (
            "FishingEnded",
            near_msgs
                .iter()
                .any(|m| matches!(m, ServerMessage::FishingEnded { .. })),
        ),
    ] {
        assert!(seen, "a 13 m bystander must see {name}");
    }
    assert_eq!(
        fishing_msg_count(&drain(&mut rx_far)),
        0,
        "a 63 m player hears nothing"
    );
}
