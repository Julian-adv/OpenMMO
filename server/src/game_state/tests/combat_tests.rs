use super::*;
use std::time::Duration;

#[tokio::test]
async fn monster_events_do_not_cross_floors() {
    let game_state = make_test_game_state("monster_floor_segregation");

    // A surface guard and a dungeon delver share the exact XZ footprint: the
    // guard stands directly above the dungeon floor the delver is on.
    let entrance = game_state.dungeon_defs.all().next().unwrap().position();
    let mut guard = make_player("guard", entrance.x, entrance.z);
    guard.floor_level = 0;
    let mut delver = make_player("delver", entrance.x, entrance.z);
    delver.floor_level = -1;
    game_state.add_player(guard).await;
    game_state.add_player(delver).await;

    // Channels registered after join so the AOI snapshots don't pollute them.
    let mut guard_rx = game_state.register_direct_channel(&pid("guard")).await;
    let mut delver_rx = game_state.register_direct_channel(&pid("delver")).await;

    let monster_pos = Position {
        y: entrance.y - 4.0,
        ..entrance
    };
    {
        let mut monsters = game_state.monsters.write().await;
        let monster = make_monster("dungeon_monster", monster_pos, -1);

        monsters.insert("dungeon_monster".to_string(), monster);
    }

    seed_subjects(&game_state).await;
    drain(&mut guard_rx);
    drain(&mut delver_rx);
    game_state
        .apply_ai_move(
            "dungeon_monster",
            -1,
            monster_pos,
            0.0,
            MonsterState::Walk,
            monster_pos,
            None,
        )
        .await;

    // Same-floor delver sees the movement; the surface guard above never does.
    match delver_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { monster_id, .. }) => {
            assert_eq!(monster_id, "dungeon_monster");
        }
        other => panic!(
            "Expected MonsterMoved for same-floor delver, got {:?}",
            other
        ),
    }
    match guard_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!(
            "Surface guard must not receive dungeon monster events, got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn ambient_spawn_stores_authoritative_world_position() {
    let game_state = make_test_game_state("canonical_spawn_position");
    game_state
        .add_player(make_player("spawner", 1.0, 0.0))
        .await;
    // What the placement hands `spawn_monster`: a seam-wrapped X and the
    // ground the server samples, never the reported pose.
    let position = Position {
        x: onlinerpg_shared::wrap_world_x(onlinerpg_shared::WORLD_WIDTH_X * 2.0 + 1.0),
        y: 5.0,
        z: 0.0,
    };

    let monster = game_state
        .spawn_monster(
            "goblin".to_string(),
            position,
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .expect("spawn should fit the test cap");
    assert_eq!(monster.position.x, 1.0);
    assert_eq!(monster.position.y, 5.0);
    assert_eq!(
        game_state.monsters.read().await[&monster.id].position.x,
        1.0
    );
}

/// Server attack resolution still enforces reach.
#[tokio::test]
async fn monster_attack_requires_proximity_to_target() {
    let game_state = make_test_game_state("monster_attack_range");
    let victim_id = pid("victim");

    game_state
        .add_player(make_player("victim", 500.0, 0.0))
        .await;

    // Each half uses its own monster: a rejected attack still consumes the
    // cooldown, so reusing one would block the in-range case for 1.5s.
    {
        let mut monsters = game_state.monsters.write().await;
        for (id, x) in [("far_monster", 0.0), ("near_monster", 499.0)] {
            let monster = make_monster(id, pos(x), 0);

            monsters.insert(id.to_string(), monster);
        }
    }

    game_state.monster_attack("far_monster", &victim_id).await;

    // `last_combat_at` is stamped for any in-range attack, hit or miss, so it
    // records that the swing was processed without depending on a damage roll.
    assert_eq!(
        game_state.players.read().await[&victim_id].last_combat_at,
        0,
        "a monster 500m from its target must not reach it"
    );
    assert_eq!(
        game_state.players.read().await[&victim_id].health,
        10,
        "an out-of-range monster attack must not deal damage"
    );

    game_state.monster_attack("near_monster", &victim_id).await;

    assert_ne!(
        game_state.players.read().await[&victim_id].last_combat_at,
        0,
        "a monster standing next to its target must still land its attack"
    );
}

/// A monster and its target must share a floor, so a surface monster cannot
/// strike a player on the dungeon floor directly beneath it.
#[tokio::test]
async fn cross_floor_monster_attack_is_rejected() {
    let game_state = make_test_game_state("cross_floor_monster_attack");
    let delver_id = pid("delver");

    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let mut delver = make_player("delver", 0.0, 0.0);
    delver.floor_level = -1;
    delver.position.y = -40.0;
    game_state.add_player(delver).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let monster = make_monster("surface_monster", pos(0.0), 0);

        monsters.insert("surface_monster".to_string(), monster);
    }

    game_state
        .monster_attack("surface_monster", &delver_id)
        .await;

    assert_eq!(
        game_state.players.read().await[&delver_id].last_combat_at,
        0,
        "a surface monster must not reach a player one floor below it"
    );
}

#[tokio::test]
async fn cross_floor_player_attack_is_rejected() {
    let game_state = make_test_game_state("cross_floor_attack");

    let mut guard = make_player("guard", 0.0, 0.0);
    guard.floor_level = 0;
    game_state.add_player(guard).await;
    let mut guard_rx = game_state.register_direct_channel(&pid("guard")).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "dungeon_monster".to_string(),
            make_monster(
                "dungeon_monster",
                Position {
                    x: 0.0,
                    y: -40.0,
                    z: 0.0,
                },
                -1,
            ),
        );
    }

    game_state
        .broadcast_player_attack(&pid("guard"), "dungeon_monster".to_string())
        .await;

    // The attack is dropped server-side: the monster keeps full HP and the
    // attacker gets a coarse rejection that must not name the floor.
    let health = game_state
        .monsters
        .read()
        .await
        .get("dungeon_monster")
        .map(|m| m.health)
        .unwrap();
    assert_eq!(health, 10, "cross-floor attack must not damage the monster");
    expect_attack_rejected(
        &mut guard_rx,
        "dungeon_monster",
        AttackRejectReason::InvalidTarget,
    );
}

#[tokio::test]
async fn out_of_range_player_attack_only_provokes_monster() {
    let game_state = make_test_game_state("out_of_range_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let monster = make_monster("distant_monster", pos(3.01), 0);

        monsters.insert("distant_monster".to_string(), monster);
    }

    game_state.tick_monster_ai_by(0.0).await;
    game_state
        .broadcast_player_attack(&player_id, "distant_monster".to_string())
        .await;

    let monsters = game_state.monsters.read().await;
    assert_eq!(
        monsters["distant_monster"].health, 10,
        "an out-of-range attack must not damage the monster"
    );
    drop(monsters);
    assert_eq!(
        game_state.players.read().await[&player_id].last_combat_at,
        0,
        "a rejected attack must not enter combat"
    );
    assert_eq!(
        game_state.brain_target("distant_monster").await,
        Some(player_id)
    );
    assert!(drain(&mut attacker_rx).iter().any(|message| matches!(message, ServerMessage::PlayerAttackRejected { monster_id, reason: AttackRejectReason::OutOfRange } if monster_id == "distant_monster")));
}

#[tokio::test]
async fn player_attack_beyond_provoke_range_is_fully_rejected() {
    let game_state = make_test_game_state("beyond_provoke_range_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let monster = make_monster(
            "remote_monster",
            pos(super::combat::PLAYER_ATTACK_PROVOKE_RANGE_METERS + 0.01),
            0,
        );

        game_state
            .monsters
            .write()
            .await
            .insert("remote_monster".to_string(), monster);
    }

    game_state
        .broadcast_player_attack(&player_id, "remote_monster".to_string())
        .await;

    assert_eq!(
        game_state.monsters.read().await["remote_monster"].health,
        10
    );
    assert_eq!(
        game_state.players.read().await[&player_id].last_combat_at,
        0
    );
    expect_attack_rejected(
        &mut attacker_rx,
        "remote_monster",
        AttackRejectReason::OutOfRange,
    );
}

#[tokio::test]
async fn player_attack_at_melee_range_is_allowed() {
    let game_state = make_test_game_state("melee_range_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "nearby_monster".to_string(),
            make_monster("nearby_monster", pos(2.0), 0),
        );
    }

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    match attacker_rx.try_recv() {
        Ok(ServerMessage::PlayerAttacked {
            player_id: actual_player_id,
            monster_id,
            ..
        }) => {
            assert_eq!(actual_player_id, player_id);
            assert_eq!(monster_id, "nearby_monster");
        }
        other => panic!("Expected an attack echo at melee range, got {other:?}"),
    }
    assert_ne!(
        game_state.players.read().await[&player_id].last_combat_at,
        0,
        "an allowed attack must enter combat"
    );
}

#[tokio::test]
async fn player_attack_interval_is_server_enforced() {
    let game_state = make_test_game_state("player_attack_interval");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    game_state.monsters.write().await.insert(
        "nearby_monster".to_string(),
        make_monster("nearby_monster", pos(1.0), 0),
    );

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    let attack_count = drain(&mut attacker_rx)
        .into_iter()
        .filter(|message| matches!(message, ServerMessage::PlayerAttacked { .. }))
        .count();
    assert_eq!(
        attack_count, 1,
        "back-to-back requests must produce one authoritative attack roll"
    );

    game_state.last_player_attacks.write().await.insert(
        player_id,
        GameState::now_ms().saturating_sub(*super::combat::PLAYER_ATTACK_INTERVAL_MS),
    );
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    assert!(drain(&mut attacker_rx)
        .into_iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
}

#[tokio::test]
async fn rejected_player_attack_does_not_consume_interval() {
    let game_state = make_test_game_state("rejected_player_attack_interval");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    game_state.monsters.write().await.insert(
        "nearby_monster".to_string(),
        make_monster("nearby_monster", pos(1.0), 0),
    );

    game_state
        .broadcast_player_attack(&player_id, "missing_monster".to_string())
        .await;
    expect_attack_rejected(
        &mut attacker_rx,
        "missing_monster",
        AttackRejectReason::InvalidTarget,
    );
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    assert!(drain(&mut attacker_rx)
        .into_iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
}

/// A player at 0 HP (awaiting respawn) must not be able to keep attacking.
#[tokio::test]
async fn dead_player_cannot_attack() {
    let game_state = make_test_game_state("dead_player_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    game_state
        .players
        .write()
        .await
        .get_mut(&player_id)
        .unwrap()
        .health = 0;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "nearby_monster".to_string(),
            make_monster("nearby_monster", pos(2.0), 0),
        );
    }

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    expect_attack_rejected(
        &mut attacker_rx,
        "nearby_monster",
        AttackRejectReason::AttackerDead,
    );
    assert_eq!(
        game_state.monsters.read().await["nearby_monster"].health,
        10,
        "a dead player's attack must deal no damage"
    );
}

/// A stale id — dead or never known — earns the same coarse rejection, so
/// probing ids reveals nothing about hidden monster state.
#[tokio::test]
async fn stale_monster_attack_is_rejected_as_invalid_target() {
    let game_state = make_test_game_state("stale_monster_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("dead_monster", pos(1.0), 0);
        monster.state = MonsterState::Dead;
        monsters.insert("dead_monster".to_string(), monster);
    }

    for target in ["dead_monster", "unknown_monster"] {
        game_state
            .broadcast_player_attack(&player_id, target.to_string())
            .await;
        expect_attack_rejected(&mut attacker_rx, target, AttackRejectReason::InvalidTarget);
    }
}

// --- Ranged attacks (doc/COMBAT.md 원거리 전투) ---

/// An archer at the origin: `weapon` in hand, a full quiver, and the given
/// attribute spread, with a direct channel to read the attack broadcast off.
async fn setup_archer(
    game_state: &GameState,
    weapon: &str,
    attrs: CharacterAttributes,
) -> DirectRx {
    setup_archer_with_ammo(game_state, weapon, attrs, &[("iron_arrow", 20)]).await
}

/// `setup_archer` with the quiver spelled out — `(item_def_id, quantity)`
/// stacks, or none at all.
async fn setup_archer_with_ammo(
    game_state: &GameState,
    weapon: &str,
    attrs: CharacterAttributes,
    quiver: &[(&str, u32)],
) -> DirectRx {
    let player_id = pid("archer");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.equipped.insert(
        EquipSlot::MainHand,
        ItemInstance {
            locked: false,
            instance_id: 1,
            item_def_id: weapon.to_string(),
            quantity: 1,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
        },
    );
    for (index, (item_def_id, quantity)) in quiver.iter().enumerate() {
        inv.bag
            .push(bag_item(index as u64 + 10, item_def_id, *quantity));
    }
    game_state.inventories.write().await.insert(player_id, inv);
    game_state
        .player_characters
        .write()
        .await
        .insert(player_id, (1, 0, attrs));
    game_state.register_direct_channel(&player_id).await
}

/// Attributes with one ability raised to 30 (+10) and the rest at 10 (+0), so
/// which modifier the roll used is readable straight off the damage.
fn attrs_with(str_score: u8, dex: u8) -> CharacterAttributes {
    CharacterAttributes {
        r#str: str_score,
        dex,
        con: 10,
        int: 10,
        wis: 10,
        cha: 10,
        guard: 0,
    }
}

fn at(x: f32) -> Position {
    Position { x, y: 0.0, z: 0.5 }
}

async fn setup_dagger_skill(game: &GameState, weapon: &str) -> DirectRx {
    let rx = setup_archer(game, weapon, attrs_with(30, 10)).await;
    game.players
        .write()
        .await
        .get_mut(&pid("archer"))
        .unwrap()
        .class = CharacterClass::Rogue;
    game.inventories
        .write()
        .await
        .get_mut(&pid("archer"))
        .unwrap()
        .equipped
        .get_mut(&EquipSlot::MainHand)
        .unwrap()
        .enchant = 9;
    let mut monster = make_monster("skill_target", at(1.0), 0);
    monster.health = 500;
    monster.max_health = 500;
    game.monsters
        .write()
        .await
        .insert("skill_target".into(), monster);
    rx
}

#[tokio::test]
async fn dagger_skill_reconnect_reports_and_enforces_remaining_cooldown() {
    use onlinerpg_shared::ability::AbilityId;

    let game = make_test_game_state("dagger_skill_reconnect");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let old_id = pid("archer");
    game.dagger_double_slash(&old_id, "skill_target".into(), None)
        .await;
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::DaggerDoubleSlashStarted { .. })));
    let inventory = game.get_player_inventory(&old_id).await.unwrap();
    game.add_player(make_player("spectator", 0.0, 0.0)).await;
    game.remove_player(&old_id).await;
    game.unregister_player_character(&old_id).await;

    let id = pid("reconnected");
    let mut player = make_player("reconnected", 0.0, 0.5);
    player.class = CharacterClass::Rogue;
    game.add_player(player).await;
    game.register_player_character(&id, 1, 0, attrs_with(30, 10), 0, None)
        .await;
    game.inventories.write().await.insert(id, inventory);
    let mut rx = game.register_direct_channel(&id).await;

    let remaining = |message: ServerMessage| {
        let ServerMessage::AbilityCooldowns { cooldowns } = message else {
            panic!("expected cooldown snapshot")
        };
        cooldowns
            .into_iter()
            .find(|timer| timer.ability == AbilityId::DaggerDoubleSlash)
            .unwrap()
            .remaining_ms
    };
    let before = remaining(game.ability_cooldown_message(&id).await);
    assert!(before > 0 && before <= 10_000);
    game.dagger_double_slash(&id, "skill_target".into(), None)
        .await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::DaggerDoubleSlashRejected { reason, cooldown_ms, .. }
            if reason == "cooldown" && *cooldown_ms > 0 && *cooldown_ms <= before)));
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));

    let other_id = pid("other_character");
    game.register_player_character(&other_id, 2, 0, attrs_with(30, 10), 0, None)
        .await;
    assert_eq!(remaining(game.ability_cooldown_message(&other_id).await), 0);
    game.last_dagger_skills
        .write()
        .await
        .insert(1, GameState::now_ms() - 10_000);
    assert_eq!(remaining(game.ability_cooldown_message(&id).await), 0);
    game.dagger_double_slash(&id, "skill_target".into(), None)
        .await;
    let messages = drain(&mut rx);
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ServerMessage::PlayerAttacked {
                    dagger_strike: Some(_),
                    ..
                }
            ))
            .count(),
        2
    );
}

#[tokio::test]
async fn dagger_skill_finishes_the_existing_approach_before_impact() {
    let game = make_test_game_state("dagger_skill_approach");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let player_id = pid("archer");
    let position = game.players.read().await[&player_id].position;
    game.request_test_move(
        &player_id,
        TestMove {
            position: Position {
                x: position.x + 0.3,
                ..position
            },
            sprinting: false,
        },
        false,
    )
    .await;
    tokio::join!(
        game.dagger_double_slash(&player_id, "skill_target".into(), None),
        async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            game.advance_test_movement(0.1).await;
        }
    );
    let damage: Vec<_> = drain(&mut rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerAttacked {
                damage,
                dagger_strike: Some(_),
                ..
            } => Some(damage),
            _ => None,
        })
        .collect();
    assert_eq!(
        damage.len(),
        2,
        "finishing the approach must not cancel skill damage"
    );
}

#[tokio::test]
async fn dagger_skill_deals_two_full_hits_and_shares_the_attack_window() {
    let game = make_test_game_state("dagger_skill_full_hits");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    game.dagger_double_slash(&pid("archer"), "skill_target".into(), None)
        .await;
    game.broadcast_player_attack(&pid("archer"), "skill_target".into())
        .await;
    let hits: Vec<_> = drain(&mut rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerAttacked {
                damage,
                hit,
                dagger_strike,
                ..
            } => {
                assert!(hit && dagger_strike.is_some());
                assert!(
                    (20..=23).contains(&damage),
                    "1d4 + STR 10 + enchant 9, got {damage}"
                );
                Some((dagger_strike.unwrap(), damage))
            }
            _ => None,
        })
        .collect();
    assert_eq!(hits.len(), 2);
    assert_eq!(
        hits.iter().map(|(strike, _)| *strike).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        game.monsters
            .read()
            .await
            .get("skill_target")
            .unwrap()
            .health,
        500 - hits.iter().map(|(_, damage)| damage).sum::<u32>()
    );
    game.last_player_attacks.write().await.clear();
    game.dagger_double_slash(&pid("archer"), "skill_target".into(), None)
        .await;
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::DaggerDoubleSlashRejected { reason, cooldown_ms, .. } if reason == "cooldown" && *cooldown_ms > 9000)));
}

#[tokio::test]
async fn dagger_skill_rejects_other_classes_without_damage_or_cooldown() {
    let game = make_test_game_state("dagger_skill_class_requirement");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let player_id = pid("archer");
    for class in [
        CharacterClass::Knight,
        CharacterClass::Barbarian,
        CharacterClass::Caveman,
        CharacterClass::Valkyrie,
        CharacterClass::Ranger,
        CharacterClass::Samurai,
        CharacterClass::Monk,
        CharacterClass::Priest,
        CharacterClass::Archaeologist,
        CharacterClass::Healer,
        CharacterClass::Wizard,
        CharacterClass::Tourist,
        CharacterClass::Bard,
        CharacterClass::Merchant,
        CharacterClass::Guard,
        CharacterClass::Maid,
    ] {
        game.players
            .write()
            .await
            .get_mut(&player_id)
            .unwrap()
            .class = class;
        game.dagger_double_slash(&player_id, "skill_target".into(), None)
            .await;
        let messages = drain(&mut rx);
        assert!(messages.iter().any(|message| matches!(message,
            ServerMessage::DaggerDoubleSlashRejected { reason, cooldown_ms, .. }
                if reason == "rogue_required" && *cooldown_ms == 0)));
        assert!(!messages.iter().any(|message| matches!(
            message,
            ServerMessage::DaggerDoubleSlashStarted { .. } | ServerMessage::PlayerAttacked { .. }
        )));
        assert_eq!(game.monsters.read().await["skill_target"].health, 500);
        assert!(game.last_dagger_skills.read().await.is_empty());
        assert!(game.last_player_attacks.read().await.is_empty());
    }
}

#[tokio::test]
async fn dagger_skill_rejects_other_weapon_types_without_spending_cooldown() {
    for weapon in ["iron_sword", "goblin_sword", "great_sword"] {
        let game = make_test_game_state(&format!("dagger_skill_reject_{weapon}"));
        let mut rx = setup_dagger_skill(&game, weapon).await;
        game.dagger_double_slash(&pid("archer"), "skill_target".into(), None)
            .await;
        assert!(drain(&mut rx).iter().any(|message| matches!(message,
            ServerMessage::DaggerDoubleSlashRejected { reason, .. } if reason == "dagger_required")));
        assert!(game.last_dagger_skills.read().await.is_empty());
        assert!(game.last_player_attacks.read().await.is_empty());
    }
}

#[tokio::test]
async fn dagger_skill_cannot_follow_a_basic_attack_immediately() {
    let game = make_test_game_state("dagger_skill_attack_window");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    game.broadcast_player_attack(&pid("archer"), "skill_target".into())
        .await;
    drain(&mut rx);
    game.dagger_double_slash(&pid("archer"), "skill_target".into(), None)
        .await;
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::DaggerDoubleSlashRejected { reason, .. } if reason == "attack_cooldown")));
    assert!(game.last_dagger_skills.read().await.is_empty());
}

#[tokio::test]
async fn dagger_skill_weapon_swap_cancels_the_second_hit() {
    let game = make_test_game_state("dagger_skill_swap");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let player_id = pid("archer");
    tokio::join!(
        game.dagger_double_slash(&player_id, "skill_target".into(), None),
        async {
            tokio::time::sleep(Duration::from_millis(280)).await;
            game.inventories
                .write()
                .await
                .get_mut(&pid("archer"))
                .unwrap()
                .equipped
                .get_mut(&EquipSlot::MainHand)
                .unwrap()
                .item_def_id = "iron_sword".into();
        }
    );
    let count = drain(&mut rx)
        .iter()
        .filter(|message| {
            matches!(
                message,
                ServerMessage::PlayerAttacked {
                    dagger_strike: Some(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn dagger_skill_first_hit_kill_reports_skipped_second_strike_without_duplicate_death() {
    let game = make_test_game_state("dagger_skill_kill");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    game.monsters
        .write()
        .await
        .get_mut("skill_target")
        .unwrap()
        .health = 1;
    seed_subjects(&game).await;
    drain(&mut rx);
    game.dagger_double_slash(&pid("archer"), "skill_target".into(), None)
        .await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::DaggerDoubleSlashSkipped { strike: 2, reason, .. } if reason == "target_defeated")));
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ServerMessage::PlayerAttacked {
                    dagger_strike: Some(_),
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message, ServerMessage::MonsterDead { .. }))
            .count(),
        1
    );
}

#[tokio::test]
async fn dagger_skill_rechecks_range_and_serializes_concurrent_casts() {
    let game = make_test_game_state("dagger_skill_race");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let player_id = pid("archer");
    tokio::join!(
        game.dagger_double_slash(&player_id, "skill_target".into(), None),
        game.dagger_double_slash(&player_id, "skill_target".into(), None),
        async {
            tokio::time::sleep(Duration::from_millis(280)).await;
            game.monsters
                .write()
                .await
                .get_mut("skill_target")
                .unwrap()
                .position = at(100.0);
        }
    );
    let messages = drain(&mut rx);
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message, ServerMessage::DaggerDoubleSlashStarted { .. }))
            .count(),
        1
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ServerMessage::PlayerAttacked {
                    hit: true,
                    dagger_strike: Some(1),
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::DaggerDoubleSlashSkipped {
            strike: 2,
            reason,
            ..
        } if reason == "out_of_range"
    )));
}

#[tokio::test]
async fn dagger_skill_new_movement_command_cancels_the_second_hit() {
    let game = make_test_game_state("dagger_skill_new_move");
    let mut rx = setup_dagger_skill(&game, "dagger").await;
    let player_id = pid("archer");
    let position = game.players.read().await[&player_id].position;
    tokio::join!(
        game.dagger_double_slash(&player_id, "skill_target".into(), None),
        async {
            tokio::time::sleep(Duration::from_millis(280)).await;
            game.request_test_move(
                &player_id,
                TestMove {
                    position: Position {
                        x: position.x + 1.0,
                        ..position
                    },
                    sprinting: false,
                },
                false,
            )
            .await;
        }
    );
    let messages = drain(&mut rx);
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(
                message,
                ServerMessage::PlayerAttacked {
                    dagger_strike: Some(_),
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::DaggerDoubleSlashSkipped { strike: 2, reason, .. } if reason == "interrupted")));
}

/// This monster's attack result, skipping whatever else the swing sent first
/// — spending a round pushes an inventory update ahead of the broadcast.
fn expect_attacked(rx: &mut DirectRx, expected_id: &str) -> (bool, u32) {
    let mut seen = Vec::new();
    while let Ok(message) = rx.try_recv() {
        if let ServerMessage::PlayerAttacked {
            monster_id,
            hit,
            damage,
            ..
        } = &message
        {
            assert_eq!(monster_id, expected_id);
            return (*hit, *damage);
        }
        seen.push(message);
    }
    panic!("Expected a PlayerAttacked broadcast, got {seen:?}")
}

#[tokio::test]
async fn a_bow_cannot_spend_trade_reserved_ammo() {
    let game_state = make_test_game_state("bow_reserved_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    let archer = pid("archer");
    let buyer = pid("buyer");
    game_state.add_player(make_player("buyer", 1.0, 0.5)).await;
    game_state.request_player_trade(&archer, "buyer").await;
    game_state.respond_player_trade(&buyer, &archer, true).await;
    game_state
        .set_player_trade_offer(
            &archer,
            vec![onlinerpg_shared::messages::PlayerTradeSlot {
                instance_id: 10,
                quantity: 20,
            }],
            0,
        )
        .await;
    assert_eq!(game_state.trade_reserved_quantity(&archer, 10).await, 20);
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&archer, "far".to_string())
        .await;

    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::PlayerAttackRejected { monster_id, reason }
            if monster_id == "far" && *reason == AttackRejectReason::OutOfAmmo
    )));
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
    assert_eq!(
        game_state.inventories.read().await[&archer].bag[0].quantity,
        20
    );
    assert!(!game_state
        .last_player_attacks
        .read()
        .await
        .contains_key(&archer));

    game_state.set_player_trade_offer(&archer, vec![], 0).await;
    game_state
        .broadcast_player_attack(&archer, "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    assert_eq!(
        game_state.inventories.read().await[&archer].bag[0].quantity,
        19
    );
}

#[tokio::test]
async fn a_bow_reaches_its_declared_range() {
    let game_state = make_test_game_state("bow_range_reaches");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(9.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "far");
    assert!(hit, "a +10 attack bonus always clears guard 10");
}

/// The server trails the walking player by the network lag, so a shot loosed
/// the moment the chase stops at 10m reads as slightly farther here. Melee has
/// always had that allowance; folding it into a `max` against the weapon range
/// swallowed it, and every shot taken at the edge was refused.
#[tokio::test]
async fn a_bow_shot_at_its_range_survives_the_lag_the_server_trails_by() {
    let game_state = make_test_game_state("bow_range_lag_allowance");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("edge".to_string(), make_monster("edge", at(10.6), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "edge".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "edge");
    assert!(hit);
}

#[tokio::test]
async fn a_bow_shot_past_its_range_is_rejected() {
    let game_state = make_test_game_state("bow_range_rejects");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("beyond".to_string(), make_monster("beyond", at(11.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "beyond".to_string())
        .await;

    expect_attack_rejected(&mut rx, "beyond", AttackRejectReason::OutOfRange);
}

/// A melee weapon keeps the 2m reach (plus lag tolerance) it always had, so
/// an empty `range` column changes nothing.
#[tokio::test]
async fn a_melee_weapon_keeps_its_hardcoded_reach() {
    let game_state = make_test_game_state("melee_reach_unchanged");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(5.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attack_rejected(&mut rx, "far", AttackRejectReason::OutOfRange);
}

#[tokio::test]
async fn a_bow_shoots_over_a_fence_at_close_and_long_range() {
    for distance in [1.5, 8.5] {
        let game_state = make_test_game_state(&format!("bow_over_fence_{distance}"));
        let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
        add_combat_fence(&game_state);
        game_state.monsters.write().await.insert(
            "behind".to_string(),
            make_monster("behind", at(distance), 0),
        );

        game_state
            .broadcast_player_attack(&pid("archer"), "behind".to_string())
            .await;

        let (hit, damage) = expect_attacked(&mut rx, "behind");
        assert!(hit);
        assert!(damage > 0);
        assert_eq!(
            game_state.inventories.read().await[&pid("archer")].bag[0].quantity,
            19
        );
    }
}

#[tokio::test]
async fn a_melee_attack_through_a_fence_is_rejected() {
    let game_state = make_test_game_state("melee_blocked_by_fence");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    add_combat_fence(&game_state);
    game_state
        .monsters
        .write()
        .await
        .insert("behind".to_string(), make_monster("behind", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "behind".to_string())
        .await;

    expect_attack_rejected(&mut rx, "behind", AttackRejectReason::OutOfRange);
}

fn add_combat_fence(game_state: &GameState) {
    use onlinerpg_shared::fence::{sync_passability, Fence, FenceAxis, FenceEdge};

    sync_passability(
        &mut game_state.passability_write(),
        "fences",
        &[Fence {
            edge: FenceEdge {
                x: 1,
                z: 0,
                axis: FenceAxis::Z,
            },
            y: 0.0,
            owner_id: 1,
        }],
    );
}

#[tokio::test]
async fn a_bow_shot_through_a_wall_is_rejected() {
    let game_state = make_test_game_state("bow_walled_off");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    add_combat_fence(&game_state);
    game_state.sync_region_furniture(0, 0, &[table_placement(4.5, 0.5)]);
    game_state
        .monsters
        .write()
        .await
        .insert("behind".to_string(), make_monster("behind", at(8.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "behind".to_string())
        .await;

    expect_attack_rejected(&mut rx, "behind", AttackRejectReason::OutOfRange);
}

/// DEX 30 (+10) with STR 10 (+0): a 1d6 bow can only reach 11 damage through
/// the DEX modifier.
#[tokio::test]
async fn a_ranged_hit_rolls_on_the_weapons_ability() {
    let game_state = make_test_game_state("bow_rolls_dex");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    let (hit, damage) = expect_attacked(&mut rx, "far");
    assert!(hit);
    assert!(damage >= 11, "1d1 + 1d6 arrow + DEX(+10), got {damage}");
}

/// The mirror image: STR 30 (+10) with DEX 10 (+0) on a melee weapon.
#[tokio::test]
async fn a_melee_hit_still_rolls_on_str() {
    let game_state = make_test_game_state("melee_rolls_str");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("near".to_string(), make_monster("near", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "near".to_string())
        .await;

    let (hit, damage) = expect_attacked(&mut rx, "near");
    assert!(hit);
    assert!(damage >= 11, "1d4 + STR(+10), got {damage}");
}

/// A landed ranged shot wakes the server brain onto the shooter.
#[tokio::test]
async fn a_landed_shot_from_range_provokes_the_target() {
    let game_state = make_test_game_state("bow_hit_provokes");
    let _rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    {
        let mut monster = make_monster("far", at(8.0), 0);

        monster.health = 100;
        game_state
            .monsters
            .write()
            .await
            .insert("far".to_string(), monster);
    }

    game_state.tick_monster_ai_by(0.0).await;
    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    assert_eq!(game_state.brain_target("far").await, Some(pid("archer")));
}

// --- Ammunition (doc/COMBAT.md 원거리 전투) ---

#[tokio::test]
async fn a_shot_spends_a_round() {
    let game_state = make_test_game_state("bow_spends_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let quiver = inventories[&pid("archer")]
        .bag
        .iter()
        .find(|item| item.item_def_id == "iron_arrow")
        .expect("the quiver survives one shot");
    assert_eq!(quiver.quantity, 19);
}

/// An empty quiver reads as its own refusal, not as an unreachable target —
/// the client has to be able to tell the player why nothing happened.
#[tokio::test]
async fn an_empty_quiver_refuses_the_shot() {
    let game_state = make_test_game_state("bow_out_of_ammo");
    let mut rx = setup_archer_with_ammo(&game_state, "bow", attrs_with(10, 30), &[]).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attack_rejected(&mut rx, "far", AttackRejectReason::OutOfAmmo);
    assert_eq!(
        game_state.monsters.read().await["far"].health,
        10,
        "a shot with nothing to fire must not damage the target"
    );
}

/// A melee weapon declares no `ammoKind`, so an empty bag never stops a swing.
#[tokio::test]
async fn a_melee_swing_needs_no_ammunition() {
    let game_state = make_test_game_state("melee_needs_no_ammo");
    let mut rx = setup_archer_with_ammo(&game_state, "dagger", attrs_with(30, 10), &[]).await;
    game_state
        .monsters
        .write()
        .await
        .insert("near".to_string(), make_monster("near", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "near".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "near");
    assert!(hit);
}

/// The strongest round of the right kind goes first, so buying better arrows
/// is the whole of using them.
#[tokio::test]
async fn the_strongest_round_is_the_one_spent() {
    let game_state = make_test_game_state("bow_picks_best_ammo");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 5), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("archer")].bag;
    let count = |id: &str| {
        bag.iter()
            .find(|item| item.item_def_id == id)
            .map_or(0, |item| item.quantity)
    };
    assert_eq!(count("steel_arrow"), 4, "the steel arrow is spent first");
    assert_eq!(count("iron_arrow"), 5, "the iron stack is left alone");
}

/// A round refused before the roll is a round still in the quiver — the gates
/// run first, and the cooldown claim after them.
#[tokio::test]
async fn a_refused_shot_keeps_its_round() {
    let game_state = make_test_game_state("bow_refused_keeps_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("beyond".to_string(), make_monster("beyond", at(11.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "beyond".to_string())
        .await;

    expect_attack_rejected(&mut rx, "beyond", AttackRejectReason::OutOfRange);
    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")]
            .bag
            .iter()
            .find(|item| item.item_def_id == "iron_arrow")
            .map(|item| item.quantity),
        Some(20)
    );
}

/// The archer's own choice outranks the strongest — dropping to the cheaper
/// arrow is the whole point of being able to choose.
#[tokio::test]
async fn a_chosen_round_outranks_the_strongest() {
    let game_state = make_test_game_state("bow_honours_choice");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 5), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("archer")].bag;
    let count = |id: &str| {
        bag.iter()
            .find(|item| item.item_def_id == id)
            .map_or(0, |item| item.quantity)
    };
    assert_eq!(count("iron_arrow"), 4, "the chosen pile is the one spent");
    assert_eq!(count("steel_arrow"), 5);
}

/// Running the chosen pile dry drops to the next round rather than reading as
/// an empty quiver, which would be a lie with arrows still in the bag.
#[tokio::test]
async fn an_exhausted_choice_falls_to_the_next_round() {
    let game_state = make_test_game_state("bow_falls_back");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 1), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert("far".to_string(), make_monster("far", at(8.0), 0));
        monsters.get_mut("far").unwrap().health = 500;
    }

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;
    expect_attacked(&mut rx, "far");
    // The cooldown is per player, so the second shot has to wait it out.
    tokio::time::sleep(std::time::Duration::from_millis(
        *super::combat::PLAYER_ATTACK_INTERVAL_MS + 20,
    ))
    .await;
    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;
    expect_attacked(&mut rx, "far");

    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("archer")];
    assert!(
        !inv.bag.iter().any(|item| item.item_def_id == "iron_arrow"),
        "the iron stack is gone"
    );
    assert_eq!(
        inv.bag
            .iter()
            .find(|item| item.item_def_id == "steel_arrow")
            .map(|item| item.quantity),
        Some(4),
        "the second shot came from the steel stack"
    );
    assert_eq!(inv.active_ammo.as_deref(), Some("steel_arrow"));
}

/// Equipping a bow chooses for an archer who has never chosen, so the first
/// shot does not need a trip through the panel.
#[tokio::test]
async fn equipping_a_bow_loads_the_strongest_round() {
    let game_state = make_test_game_state("bow_equip_loads_ammo");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "bow", 1));
    inv.bag.push(bag_item(2, "iron_arrow", 5));
    inv.bag.push(bag_item(3, "steel_arrow", 5));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("archer"), inv);

    game_state.equip_item(&pid("archer"), 1).await;

    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")].active_ammo.as_deref(),
        Some("steel_arrow")
    );
}

/// A deliberate choice survives an unequip — otherwise "use the cheaper
/// arrow" would be undone by any gear change.
#[tokio::test]
async fn re_equipping_a_bow_keeps_the_chosen_round() {
    let game_state = make_test_game_state("bow_keeps_choice");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "bow", 1));
    inv.bag.push(bag_item(2, "iron_arrow", 5));
    inv.bag.push(bag_item(3, "steel_arrow", 5));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("archer"), inv);

    game_state.equip_item(&pid("archer"), 1).await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    game_state
        .unequip_item(&pid("archer"), EquipSlot::MainHand)
        .await;
    game_state.equip_item(&pid("archer"), 1).await;

    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")].active_ammo.as_deref(),
        Some("iron_arrow")
    );
}
