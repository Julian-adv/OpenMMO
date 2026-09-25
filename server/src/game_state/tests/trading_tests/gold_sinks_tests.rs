use super::*;
use onlinerpg_shared::messages::{BagLineItem, TradeLineItem};

#[tokio::test]
async fn gold_sinks_record_actual_purchases_and_survive_flush_failures_logout_and_shutdown() {
    let game = make_test_game_state("gold_sinks_persistence");
    let (auth, path) = make_test_auth_with_path("gold_sinks_persistence");
    let account = auth.login_google("gold-sinks-buyer").unwrap();
    let character = create_test_character(&auth, &account, "Buyer");
    let (_rx, _npc_rx) = setup_haggle(&game, 18, 100_000).await;
    let buyer = pid("buyer");
    let merchant = pid("npc_rica");
    game.register_player_character(&buyer, character.id, 0, attrs_with_cha(18), 100_000, None)
        .await;
    game.inventories
        .write()
        .await
        .insert(buyer, PlayerInventory::default());
    for quantity in [1, 3] {
        game.offer_deal(
            &merchant,
            &buyer,
            "healing_potion",
            DealKind::Buy,
            -25,
            "deal",
        )
        .await;
        if quantity == 1 {
            game.buy_item(&buyer, &merchant, "healing_potion").await;
        } else {
            game.buy_items(
                &buyer,
                &merchant,
                vec![TradeLineItem {
                    item_def_id: "healing_potion".into(),
                    qty: quantity,
                }],
            )
            .await;
        }
    }
    let spent = 100_000 - game.get_player_gold(&buyer).await;
    assert!(spent > 0);
    let sink = GoldSink::ItemPurchase {
        item_def_id: "healing_potion".into(),
    };
    assert_gold_consumption(&game, sink.clone(), 4, spent).await;
    game.buy_items(
        &buyer,
        &merchant,
        vec![TradeLineItem {
            item_def_id: "healing_potion".into(),
            qty: 1_000_000,
        }],
    )
    .await;
    assert_gold_consumption(&game, sink, 4, spent).await;

    let hour = game
        .pending_gold_sinks
        .read()
        .await
        .values()
        .next()
        .unwrap()
        .timestamp;
    game.flush_gold_sinks(&auth, hour + 3599, false).await;
    assert_eq!(auth.gold_sinks(hour + 3600, 1).unwrap().total_gold, 0);
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_gold_sinks BEFORE INSERT ON gold_sink_samples BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    game.flush_gold_sinks(&auth, hour + 3600, false).await;
    assert_eq!(game.pending_gold_sinks.read().await.len(), 1);
    conn.execute_batch("DROP TRIGGER reject_gold_sinks;")
        .unwrap();
    game.flush_gold_sinks(&auth, hour + 3600, false).await;
    game.flush_gold_sinks(&auth, hour + 3600, false).await;
    assert_eq!(auth.gold_sinks(hour + 3600, 1).unwrap().total_gold, spent);
    assert!(game.pending_gold_sinks.read().await.is_empty());

    game.buy_item(&buyer, &merchant, "healing_potion").await;
    let spent = 100_000 - game.get_player_gold(&buyer).await;
    game.persist_and_detach_player(&buyer, &auth).await;
    assert!(!game.pending_gold_sinks.read().await.is_empty());
    game.persist_shutdown_snapshot(&auth).await;
    let sinks = auth.gold_sinks(hour + 3600, 1).unwrap();
    assert_eq!((sinks.entries[0].quantity, sinks.total_gold), (5, spent));
    assert!(game.pending_gold_sinks.read().await.is_empty());
}

#[tokio::test]
async fn gold_sinks_separate_buybacks_and_ignore_resident_transfers() {
    let game = make_test_game_state("gold_sinks_buybacks");
    let (_rx, _npc_rx) = setup_haggle(&game, 10, 0).await;
    let buyer = pid("buyer");
    let merchant = pid("npc_rica");
    game.inventories.write().await.insert(
        buyer,
        PlayerInventory {
            bag: vec![bag_item(7, "healing_potion", 3)],
            ..Default::default()
        },
    );
    game.sell_items(
        &buyer,
        &merchant,
        vec![BagLineItem {
            instance_id: 7,
            qty: 3,
        }],
    )
    .await;
    let payout = game.get_player_gold(&buyer).await;
    let entries: Vec<_> = game.buybacks.read().await[&(1, "Rica".into())]
        .iter()
        .map(|stored| stored.entry.entry_id)
        .collect();
    assert_eq!(entries.len(), 3);
    game.buyback_item(&buyer, &merchant, entries[0]).await;
    game.buyback_items(&buyer, &merchant, entries[1..].to_vec())
        .await;
    game.buyback_items(&buyer, &merchant, entries).await;
    assert_eq!(game.get_player_gold(&buyer).await, 0);
    assert_gold_consumption(
        &game,
        GoldSink::ItemBuyback {
            item_def_id: "healing_potion".into(),
        },
        3,
        payout,
    )
    .await;
    assert_eq!(game.pending_gold_sinks.read().await.len(), 1);

    let resident_game = make_test_game_state("gold_sinks_resident");
    setup_resident_trade(
        &resident_game,
        0,
        vec![bag_item(8, "healing_potion", 3)],
        vec![],
    )
    .await;
    resident_game
        .player_gold
        .write()
        .await
        .insert(pid("seller"), 10_000);
    resident_game
        .buy_item(&pid("seller"), &pid("npc_karl"), "healing_potion")
        .await;
    resident_game
        .buy_items(
            &pid("seller"),
            &pid("npc_karl"),
            vec![TradeLineItem {
                item_def_id: "healing_potion".into(),
                qty: 2,
            }],
        )
        .await;
    assert_eq!(resident_game.get_player_gold(&pid("npc_karl")).await, 1800);
    assert!(resident_game.pending_gold_sinks.read().await.is_empty());
}
