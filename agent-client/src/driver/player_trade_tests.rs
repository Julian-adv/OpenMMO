use super::*;
use crate::state::tests::{test_player, test_state};
use onlinerpg_shared::inventory::{EquipSlot, ItemInstance};
use onlinerpg_shared::messages::{PlayerTradeItem, PlayerTradeSide, PlayerTradeState};
use onlinerpg_shared::{PlayerId, ServerMessage};
use tokio::sync::mpsc;

const BUY: &str = r#"{"type":"buy_from_player","player":"Alice","item":"iron_sword","quantity":1,"max_copper":100}"#;
const SELL: &str =
    r#"{"type":"sell_to_player","player":"Alice","item":"apple","quantity":5,"min_copper":100}"#;
const CANCEL: &str = r#"{"type":"cancel_player_trade"}"#;

fn trading_state() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    let (mut state, rx) = test_state();
    let player = test_player(0.0, 0.0);
    state.in_game = true;
    state.self_player_id = Some(player.id);
    state.self_player = Some(player);
    state.self_gold = Some(1000);
    let mut partner = test_player(1.0, 0.0);
    partner.id = PlayerId::from(2);
    partner.name = "Alice".into();
    state.nearby_players.insert(partner.id, partner);
    (Arc::new(Mutex::new(state)), rx)
}

fn item(id: u64, name: &str, quantity: u32) -> ItemInstance {
    ItemInstance {
        instance_id: id,
        item_def_id: name.into(),
        quantity,
        enchant: 0,
        cape_color: None,
        cape_texture: None,
        locked: false,
    }
}

fn offered(id: u64, name: &str, quantity: u32) -> PlayerTradeItem {
    PlayerTradeItem {
        instance_id: id,
        item_def_id: name.into(),
        quantity,
        enchant: 0,
        cape_color: None,
        cape_texture: None,
    }
}

fn table(revision: u32) -> PlayerTradeState {
    let side = |id, name: &str| PlayerTradeSide {
        player_id: PlayerId::from(id),
        name: name.into(),
        items: vec![],
        copper: 0,
        locked: false,
        confirmed: false,
    };
    PlayerTradeState {
        revision,
        you: side(1, "Me"),
        them: side(2, "Alice"),
    }
}

async fn act(state: &Arc<Mutex<SharedState>>, action: &str) {
    handle_response(
        state,
        &format!(r#"{{"actions":[{action}]}}"#),
        &None,
        &None,
        false,
    )
    .await;
}

async fn event(state: &Arc<Mutex<SharedState>>, message: ServerMessage) {
    let mut state = state.lock().await;
    state.push_event(message);
    state.drive_player_trade().await;
}

async fn update(state: &Arc<Mutex<SharedState>>, table: &PlayerTradeState) {
    event(
        state,
        ServerMessage::PlayerTradeUpdate {
            state: table.clone(),
        },
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn buy_intent_runs_the_exchange_without_llm_protocol_actions() {
    let (state, mut rx) = trading_state();
    act(&state, BUY).await;
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::PlayerTradeRequest { target_name }) if target_name == "Alice")
    );
    act(&state, BUY).await;
    assert!(rx.try_recv().is_err());
    let mut offer = table(1);
    update(&state, &offer).await;
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::PlayerTradeSetOffer { items, copper: 100 }) if items.is_empty())
    );
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.revision = 2;
    offer.you.copper = 100;
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.revision = 3;
    offer.them.items = vec![offered(9, "iron_sword", 1)];
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeLock { revision: 3 })
    ));
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.you.locked = true;
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.them.locked = true;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeConfirm { revision: 3 })
    ));
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    assert_eq!(state.lock().await.self_gold, Some(1000));
    event(
        &state,
        ServerMessage::PlayerTradeEnded {
            completed: true,
            message: "Trade complete.".into(),
        },
    )
    .await;
    let mut state = state.lock().await;
    assert!(!state.player_trade_in_progress());
    assert!(state
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[PlayerTradeCompleted]")));
    assert!(rx.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn sell_intent_selects_bag_stacks_and_revalidates_changed_payment() {
    let (state, mut rx) = trading_state();
    {
        let mut s = state.lock().await;
        let mut locked = item(3, "apple", 10);
        locked.locked = true;
        let mut enchanted = item(4, "apple", 10);
        enchanted.enchant = 1;
        s.self_bag = vec![locked, enchanted, item(5, "apple", 2), item(6, "apple", 4)];
        s.self_equipped
            .insert(EquipSlot::MainHand, item(7, "apple", 10));
    }
    event(
        &state,
        ServerMessage::PlayerTradeRequested {
            requester_id: PlayerId::from(2),
            requester_name: "Alice".into(),
        },
    )
    .await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .format_world_state()
        .contains("Trade invitation from Alice"));
    act(&state, SELL).await;
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::PlayerTradeRespond { requester_id, accept: true }) if requester_id == PlayerId::from(2))
    );
    let mut offer = table(1);
    update(&state, &offer).await;
    let Ok(ClientMessage::PlayerTradeSetOffer { items, copper: 0 }) = rx.try_recv() else {
        panic!("expected an item offer");
    };
    assert_eq!(
        items
            .iter()
            .map(|slot| (slot.instance_id, slot.quantity))
            .collect::<Vec<_>>(),
        vec![(5, 2), (6, 3)]
    );
    offer.you.items = vec![offered(5, "apple", 2), offered(6, "apple", 3)];
    offer.them.copper = 80;
    offer.revision = 3;
    state.lock().await.drain_agent_events();
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .drain_agent_events()
        .iter()
        .any(|line| line.starts_with("[PlayerTradeOffer]")));
    update(&state, &offer).await;
    assert!(state.lock().await.drain_agent_events().is_empty());
    offer.them.copper = 100;
    offer.revision = 4;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeLock { revision: 4 })
    ));
    offer.you.locked = true;
    offer.them.copper = 1;
    offer.revision = 5;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeUnlock)
    ));
    offer.you.locked = false;
    offer.revision = 6;
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.them.copper = 120;
    offer.revision = 7;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeLock { revision: 7 })
    ));
    offer.you.locked = true;
    offer.them.locked = true;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeConfirm { revision: 7 })
    ));
}

#[tokio::test(start_paused = true)]
async fn buyer_rejects_wrong_items_quantities_enchants_and_mixed_offers() {
    let (state, mut rx) = trading_state();
    act(&state, BUY).await;
    rx.try_recv().unwrap();
    let mut offer = table(1);
    offer.you.copper = 100;
    let mut enchanted = offered(9, "iron_sword", 1);
    enchanted.enchant = 1;
    for items in [
        vec![offered(9, "dagger", 1)],
        vec![offered(9, "iron_sword", 2)],
        vec![enchanted],
        vec![offered(9, "iron_sword", 1), offered(10, "apple", 1)],
    ] {
        offer.them.items = items;
        offer.revision += 1;
        update(&state, &offer).await;
        assert!(rx.try_recv().is_err());
    }
    offer.them.items = vec![offered(9, "iron_sword", 1)];
    offer.them.copper = 1;
    offer.revision += 1;
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
    offer.them.copper = 0;
    offer.revision += 1;
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeLock { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn cancellation_and_errors_never_confirm_a_late_snapshot() {
    for cancel in [true, false] {
        let (state, mut rx) = trading_state();
        act(&state, BUY).await;
        rx.try_recv().unwrap();
        let mut offer = table(3);
        offer.you.copper = 100;
        offer.them.items = vec![offered(9, "iron_sword", 1)];
        update(&state, &offer).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerTradeLock { revision: 3 })
        ));
        if cancel {
            act(&state, CANCEL).await;
        } else {
            event(
                &state,
                ServerMessage::PlayerTradeError {
                    message: "The offer changed — look again.".into(),
                },
            )
            .await;
        }
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerTradeCancel)
        ));
        act(&state, BUY).await;
        assert!(rx.try_recv().is_err());
        offer.you.locked = true;
        offer.them.locked = true;
        update(&state, &offer).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerTradeCancel)
        ));
        assert!(rx.try_recv().is_err());
        event(
            &state,
            ServerMessage::PlayerTradeEnded {
                completed: false,
                message: "Cancelled.".into(),
            },
        )
        .await;
        assert!(!state.lock().await.player_trade_in_progress());
    }
}

#[tokio::test(start_paused = true)]
async fn request_expiry_reconnect_and_unexpected_sessions_discard_authorization() {
    let (state, mut rx) = trading_state();
    event(
        &state,
        ServerMessage::PlayerTradeRequested {
            requester_id: PlayerId::from(2),
            requester_name: "Alice".into(),
        },
    )
    .await;
    tokio::time::advance(Duration::from_secs(31)).await;
    act(&state, BUY).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeRequest { .. })
    ));
    tokio::time::advance(Duration::from_secs(31)).await;
    state.lock().await.drive_player_trade().await;
    assert!(!state.lock().await.player_trade_in_progress());
    assert!(rx.try_recv().is_err());
    update(&state, &table(1)).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeCancel)
    ));
    let player = state.lock().await.self_player.clone().unwrap();
    event(
        &state,
        ServerMessage::JoinSuccess {
            player,
            is_admin: false,
        },
    )
    .await;
    assert!(!state.lock().await.player_trade_in_progress());
    assert!(!state
        .lock()
        .await
        .format_world_state()
        .contains("Player trade with"));
    assert!(rx.try_recv().is_err());
    act(&state, BUY).await;
    rx.try_recv().unwrap();
    let mut wrong = table(1);
    wrong.them.player_id = PlayerId::from(3);
    update(&state, &wrong).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeCancel)
    ));
}

#[tokio::test(start_paused = true)]
async fn invalid_intents_send_nothing_and_pending_invites_can_be_declined() {
    let (state, mut rx) = trading_state();
    for action in [
        BUY.replace("100", "1001"),
        BUY.replace("\"quantity\":1", "\"quantity\":0"),
        BUY.replace("100", "-1"),
        BUY.replace("iron_sword", "worn_iron_sword"),
        SELL.into(),
    ] {
        act(&state, &action).await;
        assert!(rx.try_recv().is_err());
        assert!(!state.lock().await.player_trade_in_progress());
        assert!(state
            .lock()
            .await
            .drain_agent_events()
            .iter()
            .any(|line| line.starts_with("[PlayerTradeFailed]")));
    }
    state
        .lock()
        .await
        .self_player
        .as_mut()
        .unwrap()
        .is_official_npc = true;
    act(&state, BUY).await;
    assert!(rx.try_recv().is_err());
    state
        .lock()
        .await
        .self_player
        .as_mut()
        .unwrap()
        .is_official_npc = false;
    event(
        &state,
        ServerMessage::PlayerTradeRequested {
            requester_id: PlayerId::from(2),
            requester_name: "Alice".into(),
        },
    )
    .await;
    act(&state, CANCEL).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeRespond { accept: false, .. })
    ));
    assert!(rx.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn live_intent_stops_movement_combat_and_duplicate_requests() {
    let (state, mut rx) = trading_state();
    handle_response(
        &state,
        &format!(r#"{{"actions":[{BUY},{{"type":"move","x":10,"z":10}},{{"type":"attack","target":"m1"}}]}}"#),
        &None, &None, false,
    ).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeRequest { .. })
    ));
    assert!(rx.try_recv().is_err());
    assert!(!combat::tick_combat(&state, "m1", None).await);
    assert!(rx.try_recv().is_err());
    event(
        &state,
        ServerMessage::PlayerTradeRequested {
            requester_id: PlayerId::from(2),
            requester_name: "Alice".into(),
        },
    )
    .await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeRespond { accept: true, .. })
    ));
    event(
        &state,
        ServerMessage::PlayerTradeRequestResult {
            target_name: "Alice".into(),
            accepted: false,
            message: "one of you is already trading.".into(),
        },
    )
    .await;
    assert!(state.lock().await.player_trade_in_progress());
    state.lock().await.drive_player_trade().await;
    assert!(rx.try_recv().is_err());
    act(&state, &BUY.replace("100", "200")).await;
    assert!(rx.try_recv().is_err());
    assert!(state
        .lock()
        .await
        .format_world_state()
        .contains("100 copper total"));
}

#[tokio::test(start_paused = true)]
async fn offer_change_during_confirmation_requires_validation_of_the_new_version() {
    let (state, mut rx) = trading_state();
    act(&state, BUY).await;
    rx.try_recv().unwrap();
    let mut offer = table(3);
    offer.you.copper = 100;
    offer.you.locked = true;
    offer.them.locked = true;
    offer.them.items = vec![offered(9, "iron_sword", 1)];
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeConfirm { revision: 3 })
    ));
    offer.revision = 4;
    offer.them.items = vec![offered(9, "dagger", 1)];
    update(&state, &offer).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeUnlock)
    ));
    offer.you.locked = false;
    offer.revision = 5;
    update(&state, &offer).await;
    assert!(rx.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn opened_intents_and_unacknowledged_cancellation_have_bounded_lifetimes() {
    let (state, mut rx) = trading_state();
    act(&state, BUY).await;
    rx.try_recv().unwrap();
    update(&state, &table(1)).await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeSetOffer { .. })
    ));
    tokio::time::advance(Duration::from_secs(181)).await;
    state.lock().await.drive_player_trade().await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerTradeCancel)
    ));
    tokio::time::advance(Duration::from_secs(31)).await;
    state.lock().await.drive_player_trade().await;
    assert!(!state.lock().await.player_trade_in_progress());
    assert!(rx.try_recv().is_err());
}
