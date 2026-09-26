use super::*;

pub(super) async fn setup(
    name: &str,
    gold: i64,
    item: &str,
    quantity: u32,
    price: i64,
) -> (Market, u64, u64) {
    let market = make_market(name, gold, 0).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    g.use_item(&market.owner, 1).await;
    g.set_stall_buy_order(&market.owner, item.to_string(), quantity, 0, price)
        .await;
    let id = stall_id(g, &market.owner).await;
    let order = g.stalls.read().await[&market.owner].buy_orders[0].order_id;
    (market, id, order)
}

#[tokio::test]
async fn sale_and_buy_orders_share_a_stall_and_persist_taxed_transfers() {
    let (m, id, order) = setup(
        "stall_orders_settle",
        1000,
        "scroll_of_enchant_weapon",
        5,
        100,
    )
    .await;
    let g = &m.game_state;
    give(g, &m.owner, bag_item(2, "apple", 2)).await;
    g.list_stall_item(&m.owner, 2, 2, 10).await;
    give(g, &m.customer, bag_item(3, "scroll_of_enchant_weapon", 5)).await;
    g.sell_to_stall(&m.customer, id, order, 3, 3, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 700);
    assert_eq!(g.get_player_gold(&m.customer).await, 285);
    assert_gold_consumption(g, GoldSink::StallTax, 1, 15).await;
    {
        let stalls = g.stalls.read().await;
        assert_eq!(stalls[&m.owner].buy_orders[0].quantity, 2);
        assert_eq!(stalls[&m.owner].listings[0].quantity, 2);
    }
    let account = m.auth.login_npc("npc_stall_orders_settle").unwrap();
    for (player, expected_gold, expected_qty) in [(m.owner, 700, 3), (m.customer, 285, 2)] {
        let character = g.player_characters.read().await[&player].0;
        assert_eq!(
            m.auth
                .get_character_for_account(&account, character)
                .unwrap()
                .gold,
            expected_gold
        );
        let items = m.auth.load_inventory(character).unwrap();
        assert_eq!(
            items
                .iter()
                .find(|item| item.item_def_id == "scroll_of_enchant_weapon")
                .unwrap()
                .quantity,
            expected_qty
        );
    }
    g.buy_from_stall(&m.customer, id, buy(2, 1), &m.auth).await;
    assert_eq!(g.get_player_gold(&m.customer).await, 275);
    assert_eq!(g.get_player_gold(&m.owner).await, 710);
}

#[tokio::test]
async fn repricing_and_canceling_invalidate_old_order_ids() {
    let (m, id, old) = setup("stall_orders_reprice", 1000, "apple", 2, 100).await;
    let g = &m.game_state;
    give(g, &m.customer, bag_item(2, "apple", 2)).await;
    g.set_stall_buy_order(&m.owner, "apple".into(), 2, 0, 1)
        .await;
    let new = g.stalls.read().await[&m.owner].buy_orders[0].order_id;
    assert_ne!(old, new);
    g.sell_to_stall(&m.customer, id, old, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    g.remove_stall_buy_order(&m.customer, new).await;
    assert_eq!(g.stalls.read().await[&m.owner].buy_orders.len(), 1);
    g.remove_stall_buy_order(&m.owner, new).await;
    g.sell_to_stall(&m.customer, id, new, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    assert_eq!(g.inventories.read().await[&m.customer].bag[0].quantity, 2);
}

#[tokio::test]
async fn locked_reserved_and_mismatched_items_cannot_fill_orders() {
    let (m, id, _) = setup("stall_orders_match", 1000, "steel_longsword", 1, 100).await;
    let g = &m.game_state;
    g.set_stall_buy_order(&m.owner, "steel_longsword".into(), 1, 2, 100)
        .await;
    let order = g.stalls.read().await[&m.owner].buy_orders[1].order_id;
    give(g, &m.customer, bag_item(2, "steel_longsword", 1)).await;
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    {
        let mut inventories = g.inventories.write().await;
        let item = &mut inventories.get_mut(&m.customer).unwrap().bag[0];
        item.enchant = 2;
        item.locked = true;
    }
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    g.inventories
        .write()
        .await
        .get_mut(&m.customer)
        .unwrap()
        .bag[0]
        .locked = false;
    give(g, &m.customer, bag_item(3, "peddler_stall", 1)).await;
    g.use_item(&m.customer, 3).await;
    g.list_stall_item(&m.customer, 2, 1, 100).await;
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    g.unlist_stall_item(&m.customer, 2).await;
    g.request_player_trade(&m.customer, "Sella").await;
    g.respond_player_trade(&m.owner, &m.customer, true).await;
    g.set_player_trade_offer(
        &m.customer,
        vec![onlinerpg_shared::messages::PlayerTradeSlot {
            instance_id: 2,
            quantity: 1,
        }],
        0,
    )
    .await;
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
}

#[tokio::test]
async fn insufficient_funds_weight_and_wallet_overflow_restore_orders() {
    for cause in ["funds", "weight", "overflow"] {
        let (m, id, order) = setup(&format!("stall_orders_{cause}"), 1000, "apple", 2, 100).await;
        let g = &m.game_state;
        give(g, &m.customer, bag_item(2, "apple", 2)).await;
        match cause {
            "funds" => {
                assert!(g.spend_copper(&m.owner, 1000).await);
            }
            "weight" => give(g, &m.owner, bag_item(3, "steel_longsword", 10000)).await,
            _ => {
                g.player_gold.write().await.insert(m.customer, i64::MAX);
            }
        }
        let before_owner = g.get_player_gold(&m.owner).await;
        let before_customer = g.get_player_gold(&m.customer).await;
        g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
        assert_eq!(g.get_player_gold(&m.owner).await, before_owner, "{cause}");
        assert_eq!(
            g.get_player_gold(&m.customer).await,
            before_customer,
            "{cause}"
        );
        assert_eq!(g.inventories.read().await[&m.customer].bag[0].quantity, 2);
        let stalls = g.stalls.read().await;
        assert_eq!(stalls[&m.owner].buy_orders[0].quantity, 2);
        assert!(g.pending_gold_sinks.read().await.is_empty());
    }
}

#[tokio::test]
async fn concurrent_sellers_cannot_overfill_the_last_unit() {
    let (m, id, order) = setup("stall_orders_race", 1000, "apple", 1, 100).await;
    let g = &m.game_state;
    let account = m.auth.login_npc("npc_stall_orders_race").unwrap();
    let record = create_test_character(&m.auth, &account, "Third");
    let third = pid("Third");
    g.add_player(make_player("Third", 101.0, 50.0)).await;
    g.register_player_character(&third, record.id, 0, attrs_with_cha(12), 0, None)
        .await;
    g.inventories
        .write()
        .await
        .insert(third, PlayerInventory::default());
    give(g, &m.customer, bag_item(2, "apple", 1)).await;
    give(g, &third, bag_item(3, "apple", 1)).await;
    tokio::join!(
        g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth),
        g.sell_to_stall(&third, id, order, 3, 1, &m.auth),
    );
    assert_eq!(g.get_player_gold(&m.owner).await, 900);
    assert_eq!(
        g.get_player_gold(&m.customer).await + g.get_player_gold(&third).await,
        95
    );
    assert!(g.stalls.read().await[&m.owner].buy_orders.is_empty());
    assert_gold_consumption(g, GoldSink::StallTax, 1, 5).await;
}

#[tokio::test]
async fn a_failed_concurrent_sale_preserves_demand_after_another_sale_succeeds() {
    let (m, id, order) = setup("stall_orders_pending", 1000, "apple", 2, 100).await;
    let g = &m.game_state;
    give(g, &m.customer, bag_item(2, "apple", 1)).await;
    give(g, &m.customer, bag_item(3, "apple", 1)).await;
    assert!(g.spend_copper(&m.owner, 900).await);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(
            g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth),
            g.sell_to_stall(&m.customer, id, order, 3, 1, &m.auth),
        );
    })
    .await
    .unwrap();
    assert_eq!(g.get_player_gold(&m.owner).await, 0);
    assert_eq!(g.get_player_gold(&m.customer).await, 95);
    let stalls = g.stalls.read().await;
    assert_eq!(stalls[&m.owner].buy_orders[0].quantity, 1);
}

#[tokio::test]
async fn buy_orders_reject_invalid_items_budgets_and_shared_slot_overflow() {
    let (m, _, _) = setup("stall_orders_limits", 1000, "apple", 1, 100).await;
    let g = &m.game_state;
    for (item, qty, enchant, price) in [
        ("missing", 1, 0, 1),
        ("worn_iron_sword", 1, 0, 1),
        ("bread", 0, 0, 1),
        ("bread", 1, 0, -1),
        ("bread", 1, 1, 1),
        ("bread", 1, -1, 1),
        ("bread", 2, 0, i64::MAX),
        ("bread", 10, 0, 100),
    ] {
        g.set_stall_buy_order(&m.owner, item.into(), qty, enchant, price)
            .await;
        assert_eq!(
            g.stalls.read().await[&m.owner].buy_orders.len(),
            1,
            "{item} {qty} {enchant} {price}"
        );
    }
    for instance in 2..=12 {
        give(g, &m.owner, bag_item(instance, "steel_longsword", 1)).await;
        g.list_stall_item(&m.owner, instance, 1, 1).await;
    }
    g.set_stall_buy_order(&m.owner, "bread".into(), 1, 0, 1)
        .await;
    give(g, &m.owner, bag_item(13, "steel_longsword", 1)).await;
    g.list_stall_item(&m.owner, 13, 1, 1).await;
    let stalls = g.stalls.read().await;
    assert_eq!(stalls[&m.owner].listings.len(), 11);
    assert_eq!(stalls[&m.owner].buy_orders.len(), 1);
}

#[tokio::test]
async fn buy_orders_reject_remote_self_and_official_npc_sales() {
    let (m, id, order) = setup("stall_orders_access", 1000, "apple", 1, 100).await;
    let g = &m.game_state;
    give(g, &m.customer, bag_item(2, "apple", 1)).await;
    give(g, &m.owner, bag_item(3, "apple", 1)).await;
    g.sell_to_stall(&m.owner, id, order, 3, 1, &m.auth).await;
    g.players
        .write()
        .await
        .get_mut(&m.customer)
        .unwrap()
        .position
        .x += 100.0;
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    {
        let mut players = g.players.write().await;
        let customer = players.get_mut(&m.customer).unwrap();
        customer.position.x -= 100.0;
        customer.is_official_npc = true;
    }
    g.sell_to_stall(&m.customer, id, order, 2, 1, &m.auth).await;
    assert_eq!(g.get_player_gold(&m.owner).await, 1000);
    assert_eq!(g.stalls.read().await[&m.owner].buy_orders[0].quantity, 1);
}
