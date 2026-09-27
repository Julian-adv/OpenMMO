use super::*;
use crate::state::tests::{test_player, test_state};
use onlinerpg_shared::ability::{AbilityId, AbilityRejectReason, AbilityTimer};
use onlinerpg_shared::inventory::{EquipSlot, ItemInstance};
use onlinerpg_shared::{CharacterClass, ServerMessage};
use tokio::sync::mpsc;

const CAST_WARD: &str = r#"{"type":"use_ability","ability":"guardian_ward"}"#;

fn equip(state: &mut SharedState, slot: EquipSlot, item: &str) {
    state.self_equipped.insert(
        slot,
        ItemInstance {
            instance_id: if slot == EquipSlot::MainHand { 1 } else { 2 },
            item_def_id: item.into(),
            quantity: 1,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
            locked: false,
        },
    );
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
