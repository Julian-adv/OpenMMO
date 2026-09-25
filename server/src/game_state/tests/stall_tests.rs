// ---- Stalls (/lay_stall, the peddler_stall item, consignment) -------------

use super::*;
use onlinerpg_shared::character::CharacterClass;
use onlinerpg_shared::messages::StallBuyLine;
use onlinerpg_shared::stall::STALL_LEASH_M;

#[tokio::test]
async fn only_a_merchant_lays_a_stall_and_only_one_at_a_time() {
    let game_state = make_test_game_state("stall_lay");
    let auth = make_test_auth("stall_lay");
    let knight_id = pid("knight");
    game_state
        .add_player(make_player("knight", 100.0, 50.0))
        .await;
    let merchant_id = pid("npc_wick");
    let mut merchant = make_player("npc_wick", 100.0, 50.0);
    merchant.class = CharacterClass::Merchant;
    merchant.is_official_npc = true;
    game_state.add_player(merchant).await;

    game_state
        .send_chat_message(&knight_id, "/lay_stall".to_string(), &auth)
        .await;
    assert!(
        game_state.stalls.read().await.is_empty(),
        "non-merchants are refused"
    );

    game_state
        .send_chat_message(&merchant_id, "/lay_stall".to_string(), &auth)
        .await;
    {
        let stalls = game_state.stalls.read().await;
        assert_eq!(stalls.len(), 1);
        assert_eq!(stalls.values().next().unwrap().stall.owner, merchant_id);
    }

    game_state
        .send_chat_message(&merchant_id, "/lay_stall".to_string(), &auth)
        .await;
    assert_eq!(
        game_state.stalls.read().await.len(),
        1,
        "a second stall by the same owner is refused"
    );
}

#[tokio::test]
async fn pack_stall_and_logout_both_fold_the_table() {
    let game_state = make_test_game_state("stall_pack");
    let auth = make_test_auth("stall_pack");
    let merchant_id = pid("npc_wick");
    let mut merchant = make_player("npc_wick", 100.0, 50.0);
    merchant.class = CharacterClass::Merchant;
    game_state.add_player(merchant).await;

    game_state
        .send_chat_message(&merchant_id, "/pack_stall".to_string(), &auth)
        .await;
    assert!(game_state.stalls.read().await.is_empty());

    game_state
        .send_chat_message(&merchant_id, "/lay_stall".to_string(), &auth)
        .await;
    assert_eq!(game_state.stalls.read().await.len(), 1);
    game_state
        .send_chat_message(&merchant_id, "/pack_stall".to_string(), &auth)
        .await;
    assert!(
        game_state.stalls.read().await.is_empty(),
        "/pack_stall removes the stall"
    );

    game_state
        .send_chat_message(&merchant_id, "/lay_stall".to_string(), &auth)
        .await;
    assert_eq!(game_state.stalls.read().await.len(), 1);
    game_state.remove_player(&merchant_id).await;
    assert!(
        game_state.stalls.read().await.is_empty(),
        "logout packs the stall up"
    );
}

/// A stallholder and a customer backed by real DB characters, so a sale can be
/// asserted against what actually landed on disk.
struct Market {
    game_state: GameState,
    auth: crate::auth::AuthService,
    owner: PlayerId,
    customer: PlayerId,
}

async fn make_market(test_name: &str, owner_gold: i64, customer_gold: i64) -> Market {
    let auth = make_test_auth(test_name);
    let account = auth.login_npc(&format!("npc_{test_name}")).unwrap();
    let game_state = make_test_game_state(test_name);
    for (name, x, gold) in [("Sella", 100.0, owner_gold), ("Bram", 101.0, customer_gold)] {
        let record = create_test_character(&auth, &account, name);
        let mut player = make_player(name, x, 50.0);
        player.name = name.to_string();
        game_state.add_player(player).await;
        game_state
            .register_player_character(&pid(name), record.id, 0, attrs_with_cha(12), gold, None)
            .await;
        game_state
            .inventories
            .write()
            .await
            .insert(pid(name), PlayerInventory::default());
    }
    Market {
        game_state,
        auth,
        owner: pid("Sella"),
        customer: pid("Bram"),
    }
}

async fn give(game_state: &GameState, player_id: &PlayerId, item: ItemInstance) {
    game_state
        .inventories
        .write()
        .await
        .get_mut(player_id)
        .unwrap()
        .bag
        .push(item);
}

fn buy(instance_id: u64, quantity: u32) -> Vec<StallBuyLine> {
    vec![StallBuyLine {
        instance_id,
        quantity,
    }]
}

async fn stall_id(game_state: &GameState, owner: &PlayerId) -> u64 {
    game_state.stall_of(owner).await.expect("stall out").id
}

#[tokio::test]
async fn the_item_lays_the_table_out_and_using_it_again_packs_it_up() {
    let market = make_market("stall_item", 0, 0).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;

    game_state.use_item(&market.owner, 1).await;

    {
        let stalls = game_state.stalls.read().await;
        let entry = stalls.get(&market.owner).expect("stall laid out");
        assert_eq!(entry.stall.owner_name, "Sella");
        assert_eq!(entry.placed_with, 1);
        // Rotation 0 faces +z, so the table lands ahead of the owner.
        assert!(entry.stall.position.z > 50.0);
    }
    assert_eq!(
        game_state.inventories.read().await[&market.owner].bag.len(),
        1,
        "the stall item is not consumed"
    );

    game_state.use_item(&market.owner, 1).await;
    assert!(game_state.stalls.read().await.is_empty());
}

#[tokio::test]
async fn item_lock_blocks_stall_listing_and_stale_stall_sales() {
    let market = make_market("item_lock_stall", 0, 10000).await;
    let game = &market.game_state;
    give(game, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game, &market.owner, bag_item(2, "steel_longsword", 1)).await;
    game.use_item(&market.owner, 1).await;
    game.set_item_locked(&market.owner, 2, true).await;
    game.list_stall_item(&market.owner, 2, 1, 100).await;
    assert!(game.stalls.read().await[&market.owner].listings.is_empty());
    game.set_item_locked(&market.owner, 2, false).await;
    game.list_stall_item(&market.owner, 2, 1, 100).await;
    game.set_item_locked(&market.owner, 2, true).await;
    assert!(!game.get_player_inventory(&market.owner).await.unwrap().bag[1].locked);
    game.inventories
        .write()
        .await
        .get_mut(&market.owner)
        .unwrap()
        .bag[1]
        .locked = true;
    game.buy_from_stall(
        &market.customer,
        stall_id(game, &market.owner).await,
        buy(2, 1),
        &market.auth,
    )
    .await;
    assert_eq!(
        game.get_player_inventory(&market.owner)
            .await
            .unwrap()
            .bag
            .len(),
        2
    );
    assert!(game
        .get_player_inventory(&market.customer)
        .await
        .unwrap()
        .bag
        .is_empty());
    assert_eq!(game.player_gold.read().await[&market.customer], 10000);
}

#[tokio::test]
async fn straying_past_the_leash_packs_the_stall_up() {
    let market = make_market("stall_leash", 0, 0).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    game_state.use_item(&market.owner, 1).await;
    let table = game_state.stall_of(&market.owner).await.unwrap().position;

    let near = Position {
        x: table.x,
        y: 0.0,
        z: table.z + STALL_LEASH_M - 1.0,
    };
    game_state
        .teleport_player(&market.owner, near, 0.0, 0)
        .await;
    assert_eq!(game_state.stalls.read().await.len(), 1);

    let far = Position {
        x: table.x,
        y: 0.0,
        z: table.z + STALL_LEASH_M + 3.0,
    };
    game_state.teleport_player(&market.owner, far, 0.0, 0).await;
    assert!(
        game_state.stalls.read().await.is_empty(),
        "walking away folds the table"
    );
}

#[tokio::test]
async fn buying_off_a_stall_moves_the_goods_and_taxes_the_seller() {
    let market = make_market("stall_buy", 0, 1_000).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game_state, &market.owner, bag_item(2, "healing_potion", 5)).await;
    game_state.use_item(&market.owner, 1).await;
    let id = stall_id(game_state, &market.owner).await;

    game_state.list_stall_item(&market.owner, 2, 5, 100).await;
    game_state
        .buy_from_stall(&market.customer, id, buy(2, 3), &market.auth)
        .await;

    let inventories = game_state.inventories.read().await;
    let sold = inventories[&market.owner]
        .bag
        .iter()
        .find(|i| i.instance_id == 2)
        .expect("the rest stays on the shelf");
    assert_eq!(sold.quantity, 2);
    assert_eq!(
        inventories[&market.customer].bag[0].item_def_id,
        "healing_potion"
    );
    assert_eq!(inventories[&market.customer].bag[0].quantity, 3);
    drop(inventories);

    // 300 copper at 5% tax: the customer pays the tag, the seller keeps 285.
    assert_eq!(game_state.get_player_gold(&market.customer).await, 700);
    assert_eq!(game_state.get_player_gold(&market.owner).await, 285);
    assert_gold_consumption(game_state, GoldSink::StallTax, 1, 15).await;
    assert_eq!(
        game_state.stalls.read().await[&market.owner].listings[0].quantity,
        2
    );
}

#[tokio::test]
async fn a_sold_out_listing_leaves_the_table_and_refuses_the_next_customer() {
    let market = make_market("stall_soldout", 0, 1_000).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game_state, &market.owner, bag_item(2, "apple", 2)).await;
    game_state.use_item(&market.owner, 1).await;
    let id = stall_id(game_state, &market.owner).await;
    game_state.list_stall_item(&market.owner, 2, 2, 10).await;

    game_state
        .buy_from_stall(&market.customer, id, buy(2, 2), &market.auth)
        .await;
    assert!(game_state.stalls.read().await[&market.owner]
        .listings
        .is_empty());

    let gold = game_state.get_player_gold(&market.customer).await;
    game_state
        .buy_from_stall(&market.customer, id, buy(2, 1), &market.auth)
        .await;
    assert_eq!(
        game_state.get_player_gold(&market.customer).await,
        gold,
        "the second customer pays nothing for goods already gone"
    );
}

#[tokio::test]
async fn listed_goods_and_the_deployed_stall_itself_are_locked() {
    let market = make_market("stall_lock", 0, 0).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game_state, &market.owner, bag_item(2, "apple", 4)).await;
    game_state.use_item(&market.owner, 1).await;

    game_state.list_stall_item(&market.owner, 1, 1, 500).await;
    assert!(
        game_state.stalls.read().await[&market.owner]
            .listings
            .is_empty(),
        "the table's own item cannot go on the table"
    );

    game_state.list_stall_item(&market.owner, 2, 4, 10).await;
    assert_eq!(
        game_state.stall_reserved_quantity(&market.owner, 2).await,
        4
    );
    game_state.drop_item(&market.owner, 2).await;
    assert_eq!(
        game_state.inventories.read().await[&market.owner].bag[1].quantity,
        4,
        "listed goods cannot be dropped out from under a customer"
    );

    game_state.unlist_stall_item(&market.owner, 2).await;
    assert_eq!(
        game_state.stall_reserved_quantity(&market.owner, 2).await,
        0
    );
}

/// A cart is one purchase: if any line cannot be filled, nothing moves and
/// every line goes back on the table. Matches the merchant's `BuyItems`.
#[tokio::test]
async fn a_cart_line_that_cannot_be_filled_rolls_the_whole_purchase_back() {
    let market = make_market("stall_cart", 0, 1_000).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game_state, &market.owner, bag_item(2, "apple", 4)).await;
    give(game_state, &market.owner, bag_item(3, "bread", 2)).await;
    game_state.use_item(&market.owner, 1).await;
    let id = stall_id(game_state, &market.owner).await;
    game_state.list_stall_item(&market.owner, 2, 4, 10).await;
    game_state.list_stall_item(&market.owner, 3, 2, 10).await;

    // Second line asks for more bread than is on the table.
    game_state
        .buy_from_stall(
            &market.customer,
            id,
            vec![
                StallBuyLine {
                    instance_id: 2,
                    quantity: 2,
                },
                StallBuyLine {
                    instance_id: 3,
                    quantity: 5,
                },
            ],
            &market.auth,
        )
        .await;

    assert_eq!(
        game_state.get_player_gold(&market.customer).await,
        1_000,
        "nothing is charged"
    );
    assert!(game_state.pending_gold_sinks.read().await.is_empty());
    assert!(
        game_state.inventories.read().await[&market.customer]
            .bag
            .is_empty(),
        "no goods move"
    );
    let stalls = game_state.stalls.read().await;
    let listings = &stalls[&market.owner].listings;
    assert_eq!(listings.len(), 2, "both listings are back on the table");
    assert_eq!(
        listings
            .iter()
            .find(|l| l.instance_id == 2)
            .unwrap()
            .quantity,
        4
    );
    assert_eq!(
        listings
            .iter()
            .find(|l| l.instance_id == 3)
            .unwrap()
            .quantity,
        2
    );
}

/// Two lines, one purchase: the tax is charged once on the whole total.
#[tokio::test]
async fn a_multi_line_cart_moves_together_and_is_taxed_once() {
    let market = make_market("stall_cart_ok", 0, 1_000).await;
    let game_state = &market.game_state;
    give(game_state, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(game_state, &market.owner, bag_item(2, "apple", 4)).await;
    give(game_state, &market.owner, bag_item(3, "bread", 2)).await;
    game_state.use_item(&market.owner, 1).await;
    let id = stall_id(game_state, &market.owner).await;
    game_state.list_stall_item(&market.owner, 2, 4, 100).await;
    game_state.list_stall_item(&market.owner, 3, 2, 100).await;

    game_state
        .buy_from_stall(
            &market.customer,
            id,
            vec![
                StallBuyLine {
                    instance_id: 2,
                    quantity: 2,
                },
                StallBuyLine {
                    instance_id: 3,
                    quantity: 2,
                },
            ],
            &market.auth,
        )
        .await;

    // 400 copper at 5%: the customer pays the tags, the seller keeps 380.
    assert_eq!(game_state.get_player_gold(&market.customer).await, 600);
    assert_eq!(game_state.get_player_gold(&market.owner).await, 380);
    assert_gold_consumption(game_state, GoldSink::StallTax, 1, 20).await;
    assert_eq!(
        game_state.inventories.read().await[&market.customer]
            .bag
            .len(),
        2
    );
}

#[tokio::test]
async fn regression_duplicate_cart_lines_cannot_exceed_listed_quantity() {
    let market = make_market("regression_duplicate", 0, 1_000).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &market.owner, bag_item(2, "apple", 4)).await;
    g.use_item(&market.owner, 1).await;
    g.list_stall_item(&market.owner, 2, 2, 10).await;
    let id = stall_id(g, &market.owner).await;
    g.buy_from_stall(
        &market.customer,
        id,
        vec![
            StallBuyLine {
                instance_id: 2,
                quantity: 2,
            },
            StallBuyLine {
                instance_id: 2,
                quantity: 2,
            },
        ],
        &market.auth,
    )
    .await;
    assert_eq!(g.get_player_gold(&market.customer).await, 1_000);
    assert_eq!(g.get_player_gold(&market.owner).await, 0);
    assert_eq!(g.stalls.read().await[&market.owner].listings[0].quantity, 2);
    let inventories = g.inventories.read().await;
    assert!(inventories[&market.customer].bag.is_empty());
    assert_eq!(inventories[&market.owner].bag[1].quantity, 4);
}

#[tokio::test]
async fn regression_large_price_is_rejected_without_overflow() {
    let market = make_market("regression_overflow", 0, 1_000).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &market.owner, bag_item(2, "apple", 1)).await;
    g.use_item(&market.owner, 1).await;
    g.list_stall_item(&market.owner, 2, 1, i64::MAX).await;
    let id = stall_id(g, &market.owner).await;
    g.buy_from_stall(&market.customer, id, buy(2, 1), &market.auth)
        .await;
    assert_eq!(g.get_player_gold(&market.customer).await, 1_000);
}

#[tokio::test]
async fn regression_sale_preserves_cape_customization() {
    let market = make_market("regression_cape", 0, 1_000).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    let mut cape = bag_item(2, "wool_cape", 1);
    cape.cape_color = Some("#3355ff".to_string());
    cape.cape_texture = Some("a".repeat(64));
    give(g, &market.owner, cape).await;
    g.use_item(&market.owner, 1).await;
    g.list_stall_item(&market.owner, 2, 1, 10).await;
    let id = stall_id(g, &market.owner).await;
    g.buy_from_stall(&market.customer, id, buy(2, 1), &market.auth)
        .await;
    let inventories = g.inventories.read().await;
    assert_eq!(
        inventories[&market.customer].bag[0].cape_color.as_deref(),
        Some("#3355ff")
    );
    assert_eq!(
        inventories[&market.customer].bag[0].cape_texture,
        Some("a".repeat(64))
    );
}

#[tokio::test]
async fn regression_opening_another_stall_replaces_subscription() {
    let market = make_market("regression_viewers", 0, 1_000).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &market.customer, bag_item(2, "peddler_stall", 1)).await;
    g.use_item(&market.owner, 1).await;
    g.use_item(&market.customer, 2).await;
    let first = stall_id(g, &market.owner).await;
    let second = stall_id(g, &market.customer).await;
    g.open_stall(&market.customer, first).await;
    g.open_stall(&market.customer, second).await;
    assert!(!g.stalls.read().await[&market.owner]
        .viewers
        .contains(&market.customer));
    assert!(g.stalls.read().await[&market.customer]
        .viewers
        .contains(&market.customer));
    g.open_stall(&market.customer, u64::MAX).await;
    assert!(g.stalls.read().await[&market.customer]
        .viewers
        .contains(&market.customer));
    g.close_stall(&market.customer).await;
    assert!(g
        .stalls
        .read()
        .await
        .values()
        .all(|entry| !entry.viewers.contains(&market.customer)));
}

#[tokio::test]
async fn overflowing_cart_totals_leave_inventory_gold_and_listings_unchanged() {
    for (name, quantity, two_lines) in [
        ("stall_mul_overflow", 2, false),
        ("stall_sum_overflow", 1, true),
    ] {
        let market = make_market(name, 0, i64::MAX).await;
        let g = &market.game_state;
        give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
        give(g, &market.owner, bag_item(2, "apple", quantity)).await;
        g.use_item(&market.owner, 1).await;
        g.list_stall_item(&market.owner, 2, quantity, i64::MAX)
            .await;
        let mut lines = buy(2, quantity);
        if two_lines {
            give(g, &market.owner, bag_item(3, "bread", 1)).await;
            g.list_stall_item(&market.owner, 3, 1, i64::MAX).await;
            lines.push(StallBuyLine {
                instance_id: 3,
                quantity: 1,
            });
        }
        let id = stall_id(g, &market.owner).await;
        g.buy_from_stall(&market.customer, id, lines, &market.auth)
            .await;
        assert_eq!(g.get_player_gold(&market.customer).await, i64::MAX);
        assert_eq!(g.get_player_gold(&market.owner).await, 0);
        let inventories = g.inventories.read().await;
        assert!(inventories[&market.customer].bag.is_empty());
        assert_eq!(inventories[&market.owner].bag[1].quantity, quantity);
        drop(inventories);
        let stalls = g.stalls.read().await;
        assert_eq!(
            stalls[&market.owner].listings.len(),
            if two_lines { 2 } else { 1 }
        );
        assert_eq!(stalls[&market.owner].listings[0].quantity, quantity);
    }
}

#[tokio::test]
async fn seller_wallet_overflow_rolls_back_every_cart_line() {
    let market = make_market("stall_wallet_overflow", i64::MAX, 1_000).await;
    let g = &market.game_state;
    give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
    give(g, &market.owner, bag_item(2, "apple", 3)).await;
    give(g, &market.owner, bag_item(3, "bread", 2)).await;
    g.use_item(&market.owner, 1).await;
    g.list_stall_item(&market.owner, 2, 3, 100).await;
    g.list_stall_item(&market.owner, 3, 2, 100).await;
    let id = stall_id(g, &market.owner).await;
    g.buy_from_stall(
        &market.customer,
        id,
        vec![
            StallBuyLine {
                instance_id: 2,
                quantity: 1,
            },
            StallBuyLine {
                instance_id: 3,
                quantity: 2,
            },
        ],
        &market.auth,
    )
    .await;
    assert_eq!(g.get_player_gold(&market.customer).await, 1_000);
    assert_eq!(g.get_player_gold(&market.owner).await, i64::MAX);
    let inventories = g.inventories.read().await;
    assert!(inventories[&market.customer].bag.is_empty());
    assert_eq!(inventories[&market.owner].bag[1].quantity, 3);
    assert_eq!(inventories[&market.owner].bag[2].quantity, 2);
    drop(inventories);
    let stalls = g.stalls.read().await;
    assert_eq!(stalls[&market.owner].listings.len(), 2);
    assert_eq!(stalls[&market.owner].listings[0].quantity, 3);
    assert_eq!(stalls[&market.owner].listings[1].quantity, 2);
}

#[tokio::test]
async fn maximum_prices_and_wallets_settle_without_intermediate_overflow() {
    for (name, owner_gold, price, expected_owner_gold) in [
        ("stall_max_price", 0, i64::MAX, 8_762_203_435_012_037_017),
        ("stall_max_wallet", i64::MAX - 95, 100, i64::MAX),
    ] {
        let market = make_market(name, owner_gold, price).await;
        let g = &market.game_state;
        give(g, &market.owner, bag_item(1, "peddler_stall", 1)).await;
        give(g, &market.owner, bag_item(2, "apple", 1)).await;
        g.use_item(&market.owner, 1).await;
        g.list_stall_item(&market.owner, 2, 1, price).await;
        let id = stall_id(g, &market.owner).await;
        g.buy_from_stall(&market.customer, id, buy(2, 1), &market.auth)
            .await;
        assert_eq!(g.get_player_gold(&market.customer).await, 0);
        assert_eq!(g.get_player_gold(&market.owner).await, expected_owner_gold);
        assert_eq!(
            g.inventories.read().await[&market.customer].bag[0].quantity,
            1
        );
        assert!(g.stalls.read().await[&market.owner].listings.is_empty());
    }
}
