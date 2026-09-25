use super::*;
use crate::state::tests::{monster, test_player, test_state};
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
