use super::*;
use crate::state::tests::{monster, test_player, test_state};
use onlinerpg_shared::ability::{AbilityId, AbilityTimer};
use onlinerpg_shared::inventory::{EquipSlot, ItemInstance};
use onlinerpg_shared::{CharacterClass, ServerMessage};
use tokio::sync::mpsc;

fn combat_state() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = test_state();
    state.in_game = true;
    let player = test_player(0.0, 0.0);
    state.self_player_id = Some(player.id);
    state.self_player = Some(player);
    state.attack_cooldown = Duration::from_millis(1533);
    for id in ["first", "second", "third"] {
        let mut target = monster(id);
        target.position.x = 1.0;
        state.nearby_monsters.insert(id.into(), target);
    }
    (Arc::new(Mutex::new(state)), rx)
}

fn attacks(rx: &mut mpsc::Receiver<ClientMessage>) -> Vec<String> {
    let mut result = Vec::new();
    while let Ok(command) = rx.try_recv() {
        if let ClientMessage::PlayerAttack { monster_id } = command {
            result.push(monster_id);
        }
    }
    result
}

#[tokio::test(start_paused = true)]
async fn llm_opening_attack_and_combat_tick_share_the_cooldown() {
    let (state, mut rx) = combat_state();
    let target = handle_response(
        &state,
        r#"{"actions":[{"type":"attack","monster_id":"first"}]}"#,
        &None,
        &None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(attacks(&mut rx), ["first"]);

    tokio::time::advance(Duration::from_millis(62)).await;
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert!(attacks(&mut rx).is_empty());
    assert_eq!(
        state.lock().await.player_attack_wait(),
        Duration::from_millis(1471)
    );

    tokio::time::advance(Duration::from_millis(1470)).await;
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert!(attacks(&mut rx).is_empty());

    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert_eq!(attacks(&mut rx), ["first"]);
}

#[tokio::test(start_paused = true)]
async fn repeated_llm_attacks_defer_to_the_latest_target_without_resetting_the_timer() {
    let (state, mut rx) = combat_state();
    assert!(tick_combat(&state, "first", None).await);
    assert_eq!(attacks(&mut rx), ["first"]);
    tokio::time::advance(Duration::from_millis(62)).await;

    let target = handle_response(
        &state,
        r#"{"actions":[{"type":"attack","monster_id":"second"},{"type":"attack","monster_id":"second"},{"type":"attack","monster_id":"third"}]}"#,
        &None,
        &None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(target.0, "third");
    assert!(attacks(&mut rx).is_empty());
    assert!(!state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|event| event.starts_with("[NoResult]") || event.starts_with("[Aborted]")));

    tokio::time::advance(Duration::from_millis(1471)).await;
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert_eq!(attacks(&mut rx), ["third"]);
}

#[tokio::test(start_paused = true)]
async fn failed_attack_dispatch_does_not_consume_the_cooldown() {
    let (state, rx) = combat_state();
    drop(rx);
    let mut state = state.lock().await;
    assert!(state
        .send_command(ClientMessage::PlayerAttack {
            monster_id: "first".into(),
        })
        .await
        .is_err());
    assert!(state.player_attack_wait().is_zero());
}

#[tokio::test(start_paused = true)]
async fn llm_double_slash_is_used_once_then_combat_resumes_with_normal_attacks() {
    let (state, mut rx) = combat_state();
    state.lock().await.self_player.as_mut().unwrap().class = CharacterClass::Rogue;
    let target = handle_response(
        &state,
        r#"{"actions":[{"type":"attack","target":"first","skill":"double_slash"}]}"#,
        &None,
        &None,
        false,
    )
    .await
    .unwrap();
    assert_eq!(target.0, "first");
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::DaggerDoubleSlash { monster_id }) if monster_id == "first")
    );
    assert!(rx.try_recv().is_err());

    tokio::time::advance(Duration::from_millis(100)).await;
    let id = state.lock().await.self_player_id.unwrap();
    state
        .lock()
        .await
        .push_event(ServerMessage::DaggerDoubleSlashStarted {
            player_id: id,
            monster_id: "first".into(),
            cooldown_ms: 10_000,
        });
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert!(attacks(&mut rx).is_empty());
    tokio::time::advance(Duration::from_millis(1533)).await;
    assert!(tick_combat(&state, &target.0, target.1).await);
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::PlayerAttack { monster_id }) if monster_id == "first")
    );
    assert!(rx.try_recv().is_err());
    assert!(!state.lock().await.double_slash_wait().is_zero());
}

#[tokio::test(start_paused = true)]
async fn double_slash_respects_attack_recovery_and_its_own_server_cooldown() {
    let (state, mut rx) = combat_state();
    assert!(tick_combat(&state, "first", None).await);
    assert_eq!(attacks(&mut rx), ["first"]);
    for cooldown_ms in [0, 8_000] {
        if cooldown_ms > 0 {
            tokio::time::advance(Duration::from_millis(1533)).await;
            state
                .lock()
                .await
                .push_event(ServerMessage::AbilityCooldowns {
                    cooldowns: vec![AbilityTimer {
                        ability: AbilityId::DaggerDoubleSlash,
                        remaining_ms: cooldown_ms,
                    }],
                });
        }
        handle_response(
            &state,
            r#"{"actions":[{"type":"attack","target":"first","skill":"double_slash"}]}"#,
            &None,
            &None,
            false,
        )
        .await;
        while let Ok(command) = rx.try_recv() {
            assert!(matches!(command, ClientMessage::PlayerFace { .. }));
        }
        assert!(state
            .lock()
            .await
            .drain_agent_events()
            .iter()
            .any(|event| event.starts_with("[DoubleSlashFailed]")));
    }
}

#[tokio::test(start_paused = true)]
async fn double_slash_readiness_and_feedback_follow_server_events() {
    let (mut state, _rx) = test_state();
    let mut player = test_player(0.0, 0.0);
    player.class = CharacterClass::Rogue;
    state.self_player_id = Some(player.id);
    state.self_player = Some(player.clone());
    assert!(state.format_world_state().contains("equip a dagger"));
    state.self_equipped.insert(
        EquipSlot::MainHand,
        ItemInstance {
            instance_id: 1,
            item_def_id: "dagger".into(),
            quantity: 1,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
            locked: false,
        },
    );
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
    assert!(state
        .format_world_state()
        .contains("Double Slash: ready in 7.0s"));
    state.push_event(ServerMessage::DaggerDoubleSlashStarted {
        player_id: 2.into(),
        monster_id: "first".into(),
        cooldown_ms: 10_000,
    });
    assert_eq!(state.double_slash_wait(), Duration::from_secs(7));
    let rejection = ServerMessage::DaggerDoubleSlashRejected {
        monster_id: "first".into(),
        reason: "cooldown".into(),
        cooldown_ms: 9_000,
    };
    let line = prompt::format_event(&state, &rejection).unwrap();
    assert!(line.contains("[DoubleSlashFailed]") && line.contains("cooldown"));
    state.push_event(rejection);
    assert_eq!(state.double_slash_wait(), Duration::from_secs(9));
    tokio::time::advance(Duration::from_secs(9)).await;
    assert!(state.format_world_state().contains("Double Slash: ready."));
    let skipped = ServerMessage::DaggerDoubleSlashSkipped {
        player_id: player.id,
        monster_id: "first".into(),
        strike: 2,
        reason: "out_of_range".into(),
    };
    let line = prompt::format_event(&state, &skipped).unwrap();
    assert!(line.contains("strike 2") && line.contains("out_of_range"));
    let started = ServerMessage::DaggerDoubleSlashStarted {
        player_id: player.id,
        monster_id: "first".into(),
        cooldown_ms: 10_000,
    };
    assert!(prompt::format_event(&state, &started)
        .unwrap()
        .contains("Strike results follow"));
    let hit = ServerMessage::PlayerAttacked {
        player_id: player.id,
        monster_id: "first".into(),
        hit: true,
        roll: 18,
        damage: 5,
        ammo_item_def_id: None,
        dagger_strike: Some(1),
    };
    assert!(prompt::format_event(&state, &hit)
        .unwrap()
        .contains("Double Slash strike 1"));
}

#[tokio::test(start_paused = true)]
async fn failed_double_slash_dispatch_does_not_consume_attack_recovery() {
    let (state, rx) = combat_state();
    drop(rx);
    let mut state = state.lock().await;
    assert!(state
        .send_command(ClientMessage::DaggerDoubleSlash {
            monster_id: "first".into()
        })
        .await
        .is_err());
    assert!(state.player_attack_wait().is_zero());
    assert!(state.double_slash_wait().is_zero());
}
