use super::*;

async fn logout(market: &Market) -> (i64, Player) {
    let g = &market.game_state;
    let character_id = g.player_characters.read().await[&market.owner].0;
    let player = g.players.read().await[&market.owner].clone();
    g.kick_player(&market.owner, "Logged out", None, &market.auth)
        .await;
    assert!(!g.players.read().await.contains_key(&market.owner));
    assert!(!g.player_characters.read().await.contains_key(&market.owner));
    assert!(!g.inventories.read().await.contains_key(&market.owner));
    assert!(g.stall_of(&market.owner).await.is_some());
    (character_id, player)
}

async fn reconnect(market: &Market, character_id: i64, mut player: Player) -> PlayerId {
    let g = &market.game_state;
    let persistence = g.lock_player_persistence().await;
    player.id = pid("Reconnected Sella");
    let id = player.id;
    g.register_player_character(&id, character_id, 0, attrs_with_cha(12), 0, None)
        .await;
    assert!(g.reconnect_stall_owner(&player, character_id).await);
    drop(persistence);
    g.add_player(player).await;
    id
}

#[tokio::test]
async fn offline_sales_persist_and_reconnect_preserves_stall_and_item_reservations() {
    let m = make_market("stall_offline_sales", 0, 1000).await;
    let g = &m.game_state;
    give(g, &m.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &m.owner, bag_item(2, "apple", 3)).await;
    g.use_item(&m.owner, 1).await;
    g.list_stall_item(&m.owner, 2, 3, 100).await;
    g.set_stall_sign(&m.owner, "Fresh apples".into()).await;
    let id = stall_id(g, &m.owner).await;
    let (character_id, player) = logout(&m).await;
    let mut rx = g.register_connection_channel(&m.customer).await;
    g.open_stall(&m.customer, id).await;
    let mut opened = false;
    while let Ok(bytes) = rx.try_recv() {
        if let ServerMessage::StallState {
            stall_id,
            sign,
            owned,
            ..
        } = rmp_serde::from_slice::<ServerMessage>(&bytes).unwrap()
        {
            assert_eq!(stall_id, id);
            assert_eq!(sign, "Fresh apples");
            assert!(!owned);
            opened = true;
        }
    }
    assert!(opened);
    g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth).await;
    assert_eq!(g.get_player_gold(&m.customer).await, 900);
    let account = m.auth.login_npc("npc_stall_offline_sales").unwrap();
    assert_eq!(
        m.auth
            .get_character_for_account(&account, character_id)
            .unwrap()
            .gold,
        95
    );
    assert_eq!(
        m.auth
            .load_inventory(character_id)
            .unwrap()
            .iter()
            .find(|item| item.item_def_id == "apple")
            .unwrap()
            .quantity,
        2
    );
    assert_gold_consumption(g, GoldSink::StallTax, 1, 5).await;

    let new_id = reconnect(&m, character_id, player).await;
    assert!(g.stall_of(&m.owner).await.is_none());
    let stall = g.stall_of(&new_id).await.unwrap();
    assert_eq!(stall.id, id);
    assert_eq!(stall.owner, new_id);
    assert_eq!(stall.sign, "Fresh apples");
    assert_eq!(g.get_player_gold(&new_id).await, 95);
    assert_eq!(g.stall_reserved_quantity(&new_id, 2).await, 2);
    assert!(g.stall_locked_instances(&new_id).await.contains(&1));
    assert_eq!(
        g.inventories.read().await[&new_id]
            .bag
            .iter()
            .find(|item| item.instance_id == 2)
            .unwrap()
            .quantity,
        2
    );
    g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth).await;
    assert_eq!(g.get_player_gold(&new_id).await, 190);
    g.use_item(&new_id, 1).await;
    assert!(g.stall_of(&new_id).await.is_none());
}

#[tokio::test]
async fn offline_buy_orders_persist_and_deliver_items_on_reconnect() {
    let (m, id, order) = buy_orders::setup("stall_offline_orders", 1000, "apple", 3, 100).await;
    let g = &m.game_state;
    give(g, &m.customer, bag_item(2, "apple", 3)).await;
    let (character_id, player) = logout(&m).await;
    g.sell_to_stall(&m.customer, id, order, 2, 2, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.customer).await, 190);
    assert_eq!(g.stalls.read().await[&m.owner].buy_orders[0].quantity, 1);
    let account = m.auth.login_npc("npc_stall_offline_orders").unwrap();
    assert_eq!(
        m.auth
            .get_character_for_account(&account, character_id)
            .unwrap()
            .gold,
        800
    );
    assert_eq!(
        m.auth
            .load_inventory(character_id)
            .unwrap()
            .iter()
            .find(|item| item.item_def_id == "apple")
            .unwrap()
            .quantity,
        2
    );
    let new_id = reconnect(&m, character_id, player).await;
    assert_eq!(g.get_player_gold(&new_id).await, 800);
    assert_eq!(
        g.inventories.read().await[&new_id]
            .bag
            .iter()
            .find(|item| item.item_def_id == "apple")
            .unwrap()
            .quantity,
        2
    );
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&new_id).await, 700);
    assert!(g.stalls.read().await[&new_id].buy_orders.is_empty());
}

#[tokio::test]
async fn offline_stalls_keep_block_rules_for_sales_buy_orders_and_signs() {
    let (m, id, order) = buy_orders::setup("stall_offline_blocks", 1000, "bread", 1, 100).await;
    let g = &m.game_state;
    give(g, &m.owner, bag_item(2, "apple", 1)).await;
    give(g, &m.customer, bag_item(3, "bread", 1)).await;
    g.list_stall_item(&m.owner, 2, 1, 0).await;
    g.set_stall_sign(&m.owner, "Blocked sign".into()).await;
    g.set_player_blocks(&m.owner, vec!["Bram".into()]).await;
    g.set_player_blocks(&m.customer, vec!["Sella".into()]).await;
    logout(&m).await;
    g.open_stall(&m.customer, id).await;
    assert!(!g.stalls.read().await[&m.owner]
        .viewers
        .contains(&m.customer));
    g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth).await;
    g.sell_to_stall(&m.customer, id, order, 3, 1, &m.auth).await;
    assert_eq!(g.stalls.read().await[&m.owner].listings[0].quantity, 1);
    assert_eq!(g.stalls.read().await[&m.owner].buy_orders[0].quantity, 1);
    assert!(g
        .visible_sign(&g.stall_of(&m.owner).await.unwrap(), &m.customer)
        .await
        .is_empty());
}

#[tokio::test]
async fn offline_buy_orders_still_check_funds_and_carry_capacity() {
    for cause in ["funds", "weight"] {
        let (m, id, order) =
            buy_orders::setup(&format!("stall_offline_{cause}"), 1000, "apple", 1, 100).await;
        let g = &m.game_state;
        give(g, &m.customer, bag_item(2, "apple", 1)).await;
        if cause == "funds" {
            assert!(g.spend_copper(&m.owner, 1000).await);
        } else {
            give(g, &m.owner, bag_item(3, "steel_longsword", 10000)).await;
        }
        logout(&m).await;
        g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
        assert_eq!(g.get_player_gold(&m.customer).await, 0, "{cause}");
        assert_eq!(
            g.stalls.read().await[&m.owner].buy_orders[0].quantity,
            1,
            "{cause}"
        );
    }
}

#[tokio::test]
async fn failed_offline_trade_commit_preserves_both_wallets_inventory_and_orders() {
    let (m, id, order) = buy_orders::setup("stall_offline_commit", 1000, "apple", 1, 100).await;
    let g = &m.game_state;
    give(g, &m.owner, bag_item(2, "bread", 1)).await;
    give(g, &m.customer, bag_item(3, "apple", 1)).await;
    g.list_stall_item(&m.owner, 2, 1, 0).await;
    let (character_id, player) = logout(&m).await;
    let broken = make_test_auth("stall_offline_missing_characters");
    g.buy_from_stall(&m.customer, id, buy(2, 1), &broken).await;
    g.sell_to_stall(&m.customer, id, order, 3, 1, &broken).await;
    assert_eq!(g.get_player_gold(&m.customer).await, 0);
    assert_eq!(g.inventories.read().await[&m.customer].bag.len(), 1);
    assert_eq!(g.stalls.read().await[&m.owner].listings[0].quantity, 1);
    assert_eq!(g.stalls.read().await[&m.owner].buy_orders[0].quantity, 1);
    assert!(g.pending_gold_sinks.read().await.is_empty());
    let new_id = reconnect(&m, character_id, player).await;
    assert_eq!(g.get_player_gold(&new_id).await, 1000);
    assert!(g.inventories.read().await[&new_id]
        .bag
        .iter()
        .any(|item| item.item_def_id == "bread"));
    g.sell_to_stall(&m.customer, id, order, 3, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&new_id).await, 900);
}

#[tokio::test]
async fn deleting_an_offline_character_removes_the_stall() {
    let (m, _, _) = buy_orders::setup("stall_offline_delete", 1000, "apple", 1, 100).await;
    let (character_id, _) = logout(&m).await;
    let account = m.auth.login_npc("npc_stall_offline_delete").unwrap();
    assert!(m
        .game_state
        .delete_character_if_inactive(&m.auth, &account, character_id)
        .await
        .unwrap());
    assert!(m.game_state.stall_of(&m.owner).await.is_none());
}

#[tokio::test]
async fn concurrent_reconnect_and_last_item_purchases_settle_once() {
    let m = make_market("stall_offline_race", 0, 1000).await;
    let g = &m.game_state;
    give(g, &m.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &m.owner, bag_item(2, "apple", 1)).await;
    g.use_item(&m.owner, 1).await;
    g.list_stall_item(&m.owner, 2, 1, 100).await;
    let id = stall_id(g, &m.owner).await;
    let (character_id, player) = logout(&m).await;
    let ((), new_id, ()) = tokio::join!(
        g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth),
        reconnect(&m, character_id, player),
        g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth),
    );
    assert_eq!(g.get_player_gold(&new_id).await, 95);
    assert_eq!(g.get_player_gold(&m.customer).await, 900);
    assert_eq!(g.inventories.read().await[&m.customer].bag[0].quantity, 1);
    assert!(g.stalls.read().await[&new_id].listings.is_empty());
}

#[tokio::test]
async fn failed_logout_saves_are_retried_for_offline_stall_owners() {
    for shutdown in [false, true] {
        let name = format!("stall_offline_retry_{shutdown}");
        let m = make_market(&name, 123, 0).await;
        let g = &m.game_state;
        give(g, &m.owner, bag_item(1, "peddler_stall", 1)).await;
        give(g, &m.owner, bag_item(2, "apple", 3)).await;
        g.use_item(&m.owner, 1).await;
        let character_id = g.player_characters.read().await[&m.owner].0;
        let broken = make_test_auth(&format!("stall_offline_retry_empty_{shutdown}"));
        g.kick_player(&m.owner, "Logged out", None, &broken).await;
        assert!(g.stall_of(&m.owner).await.is_some());
        if shutdown {
            g.persist_shutdown_snapshot(&m.auth).await;
        } else {
            g.flush_dirty_saves(&m.auth).await;
        }
        let account = m.auth.login_npc(&format!("npc_{name}")).unwrap();
        assert_eq!(
            m.auth
                .get_character_for_account(&account, character_id)
                .unwrap()
                .gold,
            123
        );
        assert_eq!(
            m.auth
                .load_inventory(character_id)
                .unwrap()
                .iter()
                .find(|item| item.item_def_id == "apple")
                .unwrap()
                .quantity,
            3
        );
    }
}
