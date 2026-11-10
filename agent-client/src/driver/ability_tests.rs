use super::*;
use crate::state::tests::{bag_item, test_player, test_state};
use onlinerpg_shared::ability::{AbilityId, AbilityRejectReason, AbilityTimer};
use onlinerpg_shared::inventory::EquipSlot;
use onlinerpg_shared::{CharacterClass, ServerMessage};
use tokio::sync::mpsc;

const CAST_WARD: &str = r#"{"type":"use_ability","ability":"guardian_ward"}"#;

fn equip(state: &mut SharedState, slot: EquipSlot, item: &str) {
    let id = if slot == EquipSlot::MainHand { 1 } else { 2 };
    state.self_equipped.insert(slot, bag_item(id, item, 1));
}

fn knight_state() -> (SharedState, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = test_state();
    let mut player = test_player(0.0, 0.0);
    player.class = CharacterClass::Knight;
    state.in_game = true;
    state.self_player_id = Some(player.id);
    state.self_player = Some(player);
    state.self_mana = Some((2, 10));
    equip(&mut state, EquipSlot::MainHand, "iron_sword");
    equip(&mut state, EquipSlot::OffHand, "wooden_shield");
    (state, rx)
}

#[tokio::test(start_paused = true)]
async fn guardian_ward_prompt_matches_class_equipment_and_mana() {
    let (mut state, _rx) = knight_state();
    let prompt = state.format_world_state();
    assert!(prompt.contains("Guardian Ward: ready."));
    assert!(prompt.contains(r#"{"type": "use_ability", "ability": "guardian_ward"}"#));
    assert!(prompt.contains("20m on the same floor"));
    assert!(prompt.contains("60s; cooldown 45s"));
    assert!(!prompt.contains("Double Slash:"));

    for (main, off, ready) in [
        ("iron_sword", "wooden_shield", true),
        ("morningstar", "raven_shield", true),
        ("great_sword", "wooden_shield", false),
        ("dagger", "wooden_shield", false),
        ("iron_sword", "torch", false),
    ] {
        equip(&mut state, EquipSlot::MainHand, main);
        equip(&mut state, EquipSlot::OffHand, off);
        assert_eq!(
            state.format_world_state().contains("Guardian Ward: ready."),
            ready,
            "{main} / {off}"
        );
    }
    state.self_equipped.clear();
    assert!(state.format_world_state().contains("equip a one-handed"));
    equip(&mut state, EquipSlot::MainHand, "iron_sword");
    equip(&mut state, EquipSlot::OffHand, "wooden_shield");
    state.push_event(ServerMessage::ManaUpdate {
        mana: 1,
        max_mana: 10,
    });
    assert!(state
        .format_world_state()
        .contains("not enough MP; requires 2 MP"));
    for class in [
        CharacterClass::Rogue,
        CharacterClass::Wizard,
        CharacterClass::Guard,
    ] {
        state.self_player.as_mut().unwrap().class = class;
        assert!(!state.format_world_state().contains("Guardian Ward:"));
    }
}

#[tokio::test(start_paused = true)]
async fn guardian_ward_prompt_rejects_dead_mounted_and_missing_mana_states() {
    let (mut state, _rx) = knight_state();
    state.self_player.as_mut().unwrap().health = 0;
    assert!(state
        .format_world_state()
        .contains("Guardian Ward: respawn first."));
    state.self_player.as_mut().unwrap().health = 10;
    state.self_player.as_mut().unwrap().mount = Some(onlinerpg_shared::mount::MountKind::Horse);
    assert!(state
        .format_world_state()
        .contains("Guardian Ward: dismount first."));
    state.self_player.as_mut().unwrap().mount = None;
    state.self_mana = None;
    assert!(state
        .format_world_state()
        .contains("Guardian Ward: waiting for MP state."));
    state.self_player.as_mut().unwrap().is_official_npc = true;
    assert!(state
        .format_world_state()
        .contains("Guardian Ward: waiting for MP state."));
    state.push_event(ServerMessage::ManaUpdate {
        mana: 2,
        max_mana: 10,
    });
    assert!(state.format_world_state().contains("Guardian Ward: ready."));
}

#[tokio::test(start_paused = true)]
async fn llm_guardian_ward_dispatches_without_target_or_attack_recovery() {
    let (state, mut rx) = knight_state();
    let state = Arc::new(Mutex::new(state));
    let target = handle_response(
        &state,
        &format!(r#"{{"actions":[{CAST_WARD}]}}"#),
        &None,
        &None,
        true,
    )
    .await;
    assert!(target.is_none());
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::GuardianWard,
            monster_id: None,
            target_player_id: None,
        })
    ));
    assert!(rx.try_recv().is_err());
    let mut state = state.lock().await;
    assert!(state.player_attack_wait().is_zero());
    assert!(state.double_slash_wait().is_zero());
    assert_eq!(state.self_mana, Some((2, 10)));
    assert!(state
        .format_world_state()
        .contains("Your ward: not active."));
    assert!(state.drain_agent_events().is_empty());
}

#[tokio::test(start_paused = true)]
async fn guardian_ward_cooldown_and_buff_follow_server_snapshots_and_expire() {
    let (mut state, mut rx) = knight_state();
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![
            AbilityTimer {
                ability: AbilityId::GuardianWard,
                remaining_ms: 45_000,
            },
            AbilityTimer {
                ability: AbilityId::DaggerDoubleSlash,
                remaining_ms: 7_000,
            },
        ],
    });
    state.push_event(ServerMessage::BuffUpdate {
        buffs: vec![AbilityTimer {
            ability: AbilityId::GuardianWard,
            remaining_ms: 60_000,
        }],
    });
    let prompt = state.format_world_state();
    assert!(prompt.contains("Guardian Ward: ready in 45.0s."));
    assert!(prompt.contains("Your ward: active for 60.0s."));
    assert_eq!(state.double_slash_wait(), Duration::from_secs(7));
    let state = Arc::new(Mutex::new(state));
    let response = format!(r#"{{"actions":[{CAST_WARD}]}}"#);
    handle_response(&state, &response, &None, &None, false).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[GuardianWardFailed]")));
    tokio::time::advance(Duration::from_secs(45)).await;
    assert!(state
        .lock()
        .await
        .format_world_state()
        .contains("Guardian Ward: ready. Your ward: active for 15.0s."));
    handle_response(&state, &response, &None, &None, false).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::GuardianWard,
            ..
        })
    ));
    tokio::time::advance(Duration::from_secs(15)).await;
    let mut state = state.lock().await;
    assert!(state
        .format_world_state()
        .contains("Your ward: not active."));
    state.push_event(ServerMessage::BuffUpdate {
        buffs: vec![AbilityTimer {
            ability: AbilityId::GuardianWard,
            remaining_ms: 60_000,
        }],
    });
    state.push_event(ServerMessage::BuffUpdate { buffs: vec![] });
    assert!(state
        .format_world_state()
        .contains("Your ward: not active."));
}

#[tokio::test(start_paused = true)]
async fn guardian_ward_feedback_and_reconnect_use_authoritative_state() {
    let (mut state, _rx) = knight_state();
    let player = state.self_player.clone().unwrap();
    let success = ServerMessage::AbilityUsed {
        ability: AbilityId::GuardianWard,
        player_id: player.id,
        position: player.position,
        floor_level: 0,
        targets: vec![player.id],
    };
    assert!(prompt::format_event(&state, &success)
        .unwrap()
        .contains("cast Guardian Ward on Me"));
    for (reason, expected) in [
        (AbilityRejectReason::Equipment, "off-hand shield"),
        (AbilityRejectReason::NotEnoughMana, "not enough MP"),
        (AbilityRejectReason::Cooldown, "cooldown"),
        (AbilityRejectReason::Unavailable, "current state"),
    ] {
        let rejected = ServerMessage::AbilityRejected {
            ability: AbilityId::GuardianWard,
            reason,
        };
        let line = prompt::format_event(&state, &rejected).unwrap();
        assert!(line.starts_with("[GuardianWardFailed]") && line.contains(expected));
    }
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![AbilityTimer {
            ability: AbilityId::GuardianWard,
            remaining_ms: 45_000,
        }],
    });
    state.push_event(ServerMessage::BuffUpdate {
        buffs: vec![AbilityTimer {
            ability: AbilityId::GuardianWard,
            remaining_ms: 60_000,
        }],
    });
    state.push_event(ServerMessage::JoinSuccess {
        player,
        is_admin: false,
    });
    assert!(state.ability_wait(AbilityId::GuardianWard).is_zero());
    assert!(state
        .format_world_state()
        .contains("Your ward: not active."));
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![AbilityTimer {
            ability: AbilityId::GuardianWard,
            remaining_ms: 20_000,
        }],
    });
    assert_eq!(
        state.ability_wait(AbilityId::GuardianWard),
        Duration::from_secs(20)
    );
}

fn barbarian_state() -> (SharedState, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Barbarian;
    state.self_equipped.clear();
    equip(&mut state, EquipSlot::MainHand, "worn_iron_sword");
    (state, rx)
}

#[tokio::test(start_paused = true)]
async fn rage_prompt_matches_class_melee_weapon_and_mana() {
    let (mut state, _rx) = barbarian_state();
    let prompt = state.format_world_state();
    assert!(prompt.contains("Rage: ready. Your rage: not active."));
    assert!(prompt.contains(r#"{"type": "use_ability", "ability": "rage"}"#));
    assert!(prompt.contains("deal +3 damage and your Guard drops by 2"));
    assert!(!prompt.contains("Guardian Ward:"));
    for (main, ready) in [
        ("great_sword", true),
        ("spear", true),
        ("dagger", true),
        ("bow", false),
    ] {
        equip(&mut state, EquipSlot::MainHand, main);
        assert_eq!(
            state.format_world_state().contains("Rage: ready."),
            ready,
            "{main}"
        );
    }
    state.self_equipped.clear();
    assert!(state
        .format_world_state()
        .contains("Rage: equip a melee weapon in your main hand first."));
    equip(&mut state, EquipSlot::MainHand, "worn_iron_sword");
    state.push_event(ServerMessage::ManaUpdate {
        mana: 0,
        max_mana: 10,
    });
    assert!(state
        .format_world_state()
        .contains("not enough MP; requires 1 MP"));
    state.self_player.as_mut().unwrap().class = CharacterClass::Knight;
    assert!(!state.format_world_state().contains("Rage:"));
}

#[tokio::test(start_paused = true)]
async fn llm_rage_dispatches_and_waits_out_its_cooldown() {
    let (mut state, mut rx) = barbarian_state();
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![AbilityTimer {
            ability: AbilityId::Rage,
            remaining_ms: 20_000,
        }],
    });
    let state = Arc::new(Mutex::new(state));
    let response = r#"{"actions":[{"type":"use_ability","ability":"rage"}]}"#;
    handle_response(&state, response, &None, &None, false).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[RageFailed] Wait 20.0s")));
    tokio::time::advance(Duration::from_secs(20)).await;
    handle_response(&state, response, &None, &None, false).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::Rage,
            monster_id: None,
            target_player_id: None,
        })
    ));
}

#[tokio::test(start_paused = true)]
async fn primal_roar_prompt_needs_no_weapon_and_shows_temporary_hp() {
    let (mut state, _rx) = knight_state();
    let player = state.self_player.as_mut().unwrap();
    player.class = CharacterClass::Caveman;
    player.max_health = 200;
    state.self_equipped.clear();
    let prompt = state.format_world_state();
    assert!(prompt.contains("Primal Roar: ready. Your temporary HP: 0."));
    assert!(prompt.contains(r#"{"type": "use_ability", "ability": "primal_roar"}"#));
    assert!(prompt.contains("You gain 30 temporary HP for 10s"));
    assert!(!prompt.contains("Guardian Ward:"));
    state.push_event(ServerMessage::TempHealthUpdate { amount: 12 });
    assert!(state
        .format_world_state()
        .contains("Your temporary HP: 12."));
    state.self_player.as_mut().unwrap().class = CharacterClass::Barbarian;
    assert!(!state.format_world_state().contains("Primal Roar:"));
}

#[tokio::test(start_paused = true)]
async fn llm_primal_roar_dispatches_and_waits_out_its_cooldown() {
    let (mut state, mut rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Caveman;
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![AbilityTimer {
            ability: AbilityId::PrimalRoar,
            remaining_ms: 30_000,
        }],
    });
    let state = Arc::new(Mutex::new(state));
    let response = r#"{"actions":[{"type":"use_ability","ability":"primal_roar"}]}"#;
    handle_response(&state, response, &None, &None, false).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[PrimalRoarFailed] Wait 30.0s")));
    tokio::time::advance(Duration::from_secs(30)).await;
    handle_response(&state, response, &None, &None, false).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::PrimalRoar,
            monster_id: None,
            target_player_id: None,
        })
    ));
}

fn valkyrie_state() -> (SharedState, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Valkyrie;
    state.self_equipped.clear();
    equip(&mut state, EquipSlot::MainHand, "worn_iron_sword");
    (state, rx)
}

#[tokio::test(start_paused = true)]
async fn valhalla_charge_prompt_matches_class_one_handed_weapon_and_range() {
    let (mut state, _rx) = valkyrie_state();
    let prompt = state.format_world_state();
    assert!(prompt.contains("Valhalla Charge: ready."));
    assert!(prompt.contains(
        r#"{"type": "use_ability", "ability": "valhalla_charge", "target": "<monster id>"}"#
    ));
    assert!(prompt.contains("4-10m away"));
    assert!(prompt.contains("to 60% for 3s (1.5s for bosses). Cooldown 10s."));
    for (main, ready) in [
        ("spear", true),
        ("steel_longsword", true),
        ("great_sword", false),
        ("mace", false),
    ] {
        equip(&mut state, EquipSlot::MainHand, main);
        assert_eq!(
            state
                .format_world_state()
                .contains("Valhalla Charge: ready."),
            ready,
            "{main}"
        );
    }
    state.self_player.as_mut().unwrap().class = CharacterClass::Barbarian;
    assert!(!state.format_world_state().contains("Valhalla Charge:"));
}

#[tokio::test(start_paused = true)]
async fn llm_valhalla_charge_sends_its_target_and_follows_the_dash() {
    let (state, mut rx) = valkyrie_state();
    let state = Arc::new(Mutex::new(state));
    let response =
        r#"{"actions":[{"type":"use_ability","ability":"valhalla_charge","target":"m21"}]}"#;
    handle_response(&state, response, &None, &None, false).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::ValhallaCharge,
            monster_id: Some(id),
            target_player_id: None,
        }) if id == "m21"
    ));
    let mut state = state.lock().await;
    let player = state.self_player.clone().unwrap();
    let landed = onlinerpg_shared::Position {
        x: 5.0,
        y: 0.0,
        z: 3.0,
    };
    let charged = ServerMessage::PlayerCharged {
        player_id: player.id,
        monster_id: "m21".into(),
        from: player.position,
        position: landed,
        rotation: 1.0,
        floor_level: 0,
        duration_ms: 400,
    };
    assert!(prompt::format_event(&state, &charged)
        .unwrap()
        .starts_with("[ValhallaCharge] You charge m21 and land at (5.0, 3.0)"));
    state.push_event(charged);
    assert_eq!(state.self_player.as_ref().unwrap().position, landed);
    for (reason, expected) in [
        (AbilityRejectReason::TooClose, "too close"),
        (AbilityRejectReason::Blocked, "path is blocked"),
        (AbilityRejectReason::Equipment, "one-handed sword or spear"),
    ] {
        let line = prompt::format_event(
            &state,
            &ServerMessage::AbilityRejected {
                ability: AbilityId::ValhallaCharge,
                reason,
            },
        )
        .unwrap();
        assert!(line.starts_with("[ValhallaChargeFailed]") && line.contains(expected));
    }
}

fn priest_state() -> (SharedState, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Priest;
    state.self_equipped.clear();
    state.self_mana = Some((6, 20));
    (state, rx)
}

fn hidden_player(id: u64, name: &str) -> onlinerpg_shared::Player {
    let mut player = test_player(3.0, 0.0);
    player.id = onlinerpg_shared::PlayerId::from(id);
    player.name = name.into();
    player.health = 1;
    player.max_health = 0;
    player
}

fn typed_monster(id: &str, monster_type: &str) -> onlinerpg_shared::Monster {
    let mut monster = crate::state::tests::monster(id);
    monster.monster_type = monster_type.into();
    monster
}

#[tokio::test(start_paused = true)]
async fn healing_prayer_prompt_shows_targets_range_roll_and_undead() {
    let (mut state, _rx) = priest_state();
    state
        .nearby_monsters
        .insert("m21".into(), typed_monster("m21", "skeleton"));
    state
        .nearby_monsters
        .insert("m22".into(), typed_monster("m22", "gnoll"));
    let prompt = state.format_world_state();
    assert!(prompt.contains("Healing Prayer: ready."), "{prompt}");
    assert!(prompt.contains(r#"{"type": "use_ability", "ability": "healing_prayer"}"#));
    assert!(prompt.contains(r#""target": "<player name>" to heal another living player"#));
    assert!(prompt.contains(r#""target": "<monster id>" to harm"#));
    assert!(prompt.contains("Needs 6 MP and works with any gear. The target must be within 10m"));
    assert!(prompt.contains("restores 2d6 + Level/3 + WIS modifier HP"));
    assert!(prompt.contains(
        "harms only undead monsters (Forgotten Dead, Skeleton, Skeleton Knight, Skeleton Warrior)"
    ));
    assert!(prompt.contains("Cooldown 8s."));
    assert!(prompt.contains("Monster: Skeleton [m21] (undead) HP"));
    assert!(prompt.contains("Monster: Gnoll [m22] HP"));

    state.self_player.as_mut().unwrap().level = 9;
    state
        .characters
        .push(onlinerpg_shared::character::Character {
            id: 1,
            deletion_due_at: None,
            name: "Me".into(),
            created_at: 0,
            level: 9,
            xp: 0,
            max_hp: 10,
            attributes: onlinerpg_shared::character::CharacterAttributes {
                r#str: 10,
                dex: 10,
                con: 10,
                int: 10,
                wis: 17,
                cha: 10,
                guard: 10,
            },
            class: CharacterClass::Priest,
            gender: Default::default(),
            equipment: Default::default(),
            titles: Vec::new(),
            active_title: None,
            family: None,
            appearance: Default::default(),
        });
    assert!(state.format_world_state().contains("restores 2d6+6 HP"));
    state.push_event(ServerMessage::ManaUpdate {
        mana: 5,
        max_mana: 20,
    });
    assert!(state
        .format_world_state()
        .contains("Healing Prayer: not enough MP; requires 6 MP."));
    state.self_player.as_mut().unwrap().class = CharacterClass::Knight;
    let prompt = state.format_world_state();
    assert!(!prompt.contains("Healing Prayer:"));
    assert!(!prompt.contains("(undead)"));
}

#[tokio::test(start_paused = true)]
async fn llm_healing_prayer_resolves_self_player_and_monster_targets() {
    let (mut state, mut rx) = priest_state();
    let ally = hidden_player(2, "Ann");
    state.nearby_players.insert(ally.id, ally.clone());
    state
        .nearby_monsters
        .insert("m21".into(), typed_monster("m21", "skeleton"));
    let state = Arc::new(Mutex::new(state));
    let cast = |target: Option<&str>| match target {
        Some(target) => format!(
            r#"{{"actions":[{{"type":"use_ability","ability":"healing_prayer","target":"{target}"}}]}}"#
        ),
        None => r#"{"actions":[{"type":"use_ability","ability":"healing_prayer"}]}"#.to_string(),
    };
    for (target, monster, player) in [
        (None, None, None),
        (Some("me"), None, None),
        (Some("Me"), None, None),
        (Some("ann"), None, Some(ally.id)),
        (Some("m21"), Some("m21"), None),
    ] {
        handle_response(&state, &cast(target), &None, &None, false).await;
        match rx.try_recv() {
            Ok(ClientMessage::UseAbility {
                ability: AbilityId::HealingPrayer,
                monster_id,
                target_player_id,
            }) => {
                assert_eq!(monster_id.as_deref(), monster, "{target:?}");
                assert_eq!(target_player_id, player, "{target:?}");
            }
            other => panic!("{target:?}: {other:?}"),
        }
    }
    handle_response(&state, &cast(Some("Nobody")), &None, &None, false).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[HealingPrayerFailed] No player or monster 'Nobody'")));
    state
        .lock()
        .await
        .push_event(ServerMessage::AbilityCooldowns {
            cooldowns: vec![AbilityTimer {
                ability: AbilityId::HealingPrayer,
                remaining_ms: 8_000,
            }],
        });
    handle_response(&state, &cast(Some("Ann")), &None, &None, false).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[HealingPrayerFailed] Wait 8.0s")));
}

#[tokio::test(start_paused = true)]
async fn healing_prayer_feedback_covers_caster_target_and_rejections() {
    use crate::state::EventUrgency;
    let (mut state, _rx) = priest_state();
    let me = state.self_player_id.unwrap();
    let ann = hidden_player(2, "Ann");
    let bob = hidden_player(3, "Bob");
    state.nearby_players.insert(ann.id, ann.clone());
    state.nearby_players.insert(bob.id, bob.clone());
    let prayed = |player_id, target_player_id, monster_id: Option<&str>, amount| {
        ServerMessage::HealingPrayed {
            player_id,
            target_player_id,
            monster_id: monster_id.map(Into::into),
            amount,
        }
    };
    for (msg, line, urgency) in [
        (
            prayed(me, Some(me), None, 7),
            Some("[HealingPrayer] You heal yourself for 7 HP."),
            EventUrgency::Routine,
        ),
        (
            prayed(me, Some(ann.id), None, 5),
            Some("[HealingPrayer] You heal Ann for 5 HP."),
            EventUrgency::Routine,
        ),
        (
            prayed(me, None, Some("m21"), 9),
            Some("[HealingPrayer] Your prayer sears m21 for 9 damage."),
            EventUrgency::Routine,
        ),
        (
            prayed(ann.id, Some(me), None, 4),
            Some("[HealingPrayer] Ann heals you for 4 HP."),
            EventUrgency::Routine,
        ),
        (
            prayed(ann.id, Some(bob.id), None, 4),
            None,
            EventUrgency::Noise,
        ),
        (
            prayed(ann.id, None, Some("m21"), 4),
            None,
            EventUrgency::Noise,
        ),
    ] {
        assert_eq!(prompt::format_event(&state, &msg).as_deref(), line);
        assert_eq!(state.classify_event(&msg), urgency, "{line:?}");
    }
    for (reason, expected) in [
        (AbilityRejectReason::FullHealth, "already at full HP"),
        (AbilityRejectReason::NotUndead, "only undead monsters"),
        (AbilityRejectReason::Blocked, "a wall blocks your sight"),
        (AbilityRejectReason::OutOfRange, "out of range"),
        (AbilityRejectReason::Unavailable, "unmounted Priest"),
    ] {
        let line = prompt::format_event(
            &state,
            &ServerMessage::AbilityRejected {
                ability: AbilityId::HealingPrayer,
                reason,
            },
        )
        .unwrap();
        assert!(
            line.starts_with("[HealingPrayerFailed]") && line.contains(expected),
            "{line}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn monster_hits_show_exact_hp_only_where_disclosed() {
    let (mut state, _rx) = priest_state();
    let me = state.self_player_id.unwrap();
    let ann = hidden_player(2, "Ann");
    state.nearby_players.insert(ann.id, ann.clone());
    let hit = |player_id, current_health| ServerMessage::MonsterAttackedPlayer {
        monster_id: "m21".into(),
        player_id,
        hit: true,
        roll: 15,
        damage: 3,
        current_health,
        ranged: false,
    };
    for (msg, ending) in [
        (hit(me, 7), "dmg=3 hp=7"),
        (hit(ann.id, 1), "dmg=3 alive (HP hidden)"),
        (hit(ann.id, 0), "dmg=3 dead"),
    ] {
        let line = prompt::format_event(&state, &msg).unwrap();
        assert!(line.ends_with(ending), "{line}");
    }
    state.nearby_players.get_mut(&ann.id).unwrap().max_health = 20;
    assert!(prompt::format_event(&state, &hit(ann.id, 12))
        .unwrap()
        .ends_with("dmg=3 hp=12"));
    let respawned = prompt::format_event(
        &state,
        &ServerMessage::PlayerRespawned {
            player: hidden_player(3, "Bob"),
        },
    )
    .unwrap();
    assert_eq!(respawned, "[Respawn] Bob alive (HP hidden)");
}

#[tokio::test(start_paused = true)]
async fn battle_hymn_prompt_needs_an_instrument_and_shows_playing_and_the_buff() {
    let (mut state, _rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Bard;
    state.self_mana = Some((5, 10));
    let prompt = state.format_world_state();
    assert!(prompt.contains("Battle Hymn: equip an instrument in your bag or hands first."));
    assert!(prompt.contains(r#"{"type": "use_ability", "ability": "battle_hymn"}"#));
    assert!(prompt.contains("within 20m on your floor move and attack 8% faster for 10 minutes"));
    state.self_bag.push(bag_item(5, "worn_mandolin", 1));
    assert!(state.format_world_state().contains("Battle Hymn: ready."));
    state.self_player.as_mut().unwrap().object_type = Some(crate::state::MUSIC_EMOTE.into());
    state.push_event(ServerMessage::AbilityCooldowns {
        cooldowns: vec![AbilityTimer {
            ability: AbilityId::BattleHymn,
            remaining_ms: 10_000,
        }],
    });
    assert!(state
        .format_world_state()
        .contains("Battle Hymn: playing; done in 10.0s if you keep still."));

    let player = state.self_player.as_mut().unwrap();
    player.class = CharacterClass::Knight;
    player.object_type = None;
    state.push_event(ServerMessage::BuffUpdate {
        buffs: vec![AbilityTimer {
            ability: AbilityId::BattleHymn,
            remaining_ms: 600_000,
        }],
    });
    let prompt = state.format_world_state();
    assert!(!prompt.contains("As a Bard"));
    assert!(prompt.contains("Battle Hymn: you move and attack 8% faster for 600s more."));
}

#[tokio::test(start_paused = true)]
async fn llm_battle_hymn_dispatches_without_a_target() {
    let (mut state, mut rx) = knight_state();
    state.self_player.as_mut().unwrap().class = CharacterClass::Bard;
    let state = Arc::new(Mutex::new(state));
    let response = r#"{"actions":[{"type":"use_ability","ability":"battle_hymn"}]}"#;
    handle_response(&state, response, &None, &None, false).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseAbility {
            ability: AbilityId::BattleHymn,
            monster_id: None,
            target_player_id: None,
        })
    ));
}
