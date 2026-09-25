use super::*;

#[tokio::test]
async fn stall_offers_and_matching_bag_ids_survive_updates_and_clear_on_close() {
    let (mut state, mut rx) = test_state();
    state.self_player = Some(test_player(0.0, 0.0));
    state.self_player_id = Some(PlayerId::from(1));
    state.self_bag = vec![onlinerpg_shared::inventory::ItemInstance {
        instance_id: 91,
        item_def_id: "scroll_of_enchant_weapon".into(),
        quantity: 5,
        enchant: 0,
        locked: false,
        cape_color: None,
        cape_texture: None,
    }];
    let snapshot = |quantity| ServerMessage::StallState {
        stall_id: 42,
        owner_name: "Sella".into(),
        sign: "Scrolls wanted".into(),
        owned: false,
        listings: vec![],
        buy_orders: vec![onlinerpg_shared::stall::StallBuyOrder {
            order_id: 82,
            item_def_id: "scroll_of_enchant_weapon".into(),
            quantity,
            enchant: 0,
            unit_price: 500,
        }],
    };
    state.push_event(snapshot(3));
    let world = state.format_world_state();
    assert!(world.contains("Wanted [order_id 82]: scroll_of_enchant_weapon +0 x3, 500 copper"));
    assert!(world.contains("Your bag [instance_id 91]"));
    state.push_event(snapshot(1));
    assert!(state.format_world_state().contains("+0 x1, 500 copper"));
    state.send_command(ClientMessage::CloseStall).await.unwrap();
    assert!(matches!(rx.try_recv(), Ok(ClientMessage::CloseStall)));
    assert!(state.open_stall.is_none());
    state.push_event(snapshot(1));
    state.push_event(ServerMessage::StallRemoved { stall_id: 42 });
    assert!(state.open_stall.is_none());
}
