use super::*;
use onlinerpg_shared::messages::BagLineItem;

#[tokio::test]
async fn gold_sources_wait_for_hourly_flush_and_survive_logout_retries_and_shutdown() {
    let game = make_test_game_state("item_sales_persistence");
    let (auth, path) = make_test_auth_with_path("item_sales_persistence");
    let account = auth.login_npc("npc_seller").unwrap();
    let character = create_test_character(&auth, &account, "Seller");
    game.add_player(make_npc("npc_rica", "Rica", 0.0, 0.0))
        .await;
    game.add_player(make_player("seller", 1.0, 0.0)).await;
    game.register_player_character(&pid("seller"), character.id, 0, attrs_with_cha(18), 0, None)
        .await;
    game.inventories.write().await.insert(
        pid("seller"),
        PlayerInventory {
            bag: vec![bag_item(7, "healing_potion", 8)],
            ..Default::default()
        },
    );
    game.offer_deal(
        &pid("npc_rica"),
        &pid("seller"),
        "healing_potion",
        DealKind::Sell,
        25,
        "deal",
    )
    .await;
    game.sell_items(
        &pid("seller"),
        &pid("npc_rica"),
        vec![BagLineItem {
            instance_id: 7,
            qty: 3,
        }],
    )
    .await;
    game.sell_item(&pid("seller"), &pid("npc_rica"), 7).await;
    assert_eq!(game.get_player_gold(&pid("seller")).await, 1020);
    let hour = game
        .pending_gold_sources
        .read()
        .await
        .values()
        .next()
        .unwrap()
        .timestamp;
    assert_eq!(game.pending_gold_sources.read().await.len(), 1);
    game.flush_dirty_saves(&auth).await;
    game.flush_gold_sources(&auth, hour + 3599, false).await;
    let conn = rusqlite::Connection::open(path).unwrap();
    let count = || {
        conn.query_row("SELECT COUNT(*) FROM item_sale_samples", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap()
    };
    assert_eq!(count(), 0);
    let gold: i64 = conn
        .query_row(
            "SELECT gold FROM characters WHERE id = ?1",
            [character.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(gold, 1020);
    game.sell_items(
        &pid("seller"),
        &pid("npc_rica"),
        vec![BagLineItem {
            instance_id: 7,
            qty: 9,
        }],
    )
    .await;
    assert_eq!(
        game.pending_gold_sources
            .read()
            .await
            .values()
            .next()
            .unwrap()
            .quantity,
        4
    );

    for source in [
        GoldSource::DungeonChest,
        GoldSource::CoinPile,
        GoldSource::CoinPouch,
        GoldSource::NpcSalary,
    ] {
        game.record_gold_source(source, 1, 10).await;
    }
    conn.execute_batch("CREATE TRIGGER reject_item_sales BEFORE INSERT ON item_sale_samples BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    game.flush_gold_sources(&auth, hour + 3600, false).await;
    assert_eq!(count(), 0);
    assert_eq!(game.pending_gold_sources.read().await.len(), 5);
    conn.execute_batch("DROP TRIGGER reject_item_sales;")
        .unwrap();
    game.flush_gold_sources(&auth, hour + 3600, false).await;
    game.flush_gold_sources(&auth, hour + 3600, false).await;
    let sources = auth.item_gold_sources(hour + 3600, 1).unwrap();
    assert_eq!((sources.entries[0].quantity, sources.total_gold), (4, 1060));
    assert_eq!(count(), 5);
    for entry in &sources.entries[1..] {
        assert_eq!((entry.quantity, entry.gold), (1, 10));
    }
    assert!(game.pending_gold_sources.read().await.is_empty());

    game.sell_item(&pid("seller"), &pid("npc_rica"), 7).await;
    game.record_gold_source(GoldSource::CoinPile, 1, 10).await;
    game.persist_and_detach_player(&pid("seller"), &auth).await;
    assert_eq!(
        auth.item_gold_sources(hour + 3600, 1).unwrap().total_gold,
        1060
    );
    assert!(!game.pending_gold_sources.read().await.is_empty());
    game.persist_shutdown_snapshot(&auth).await;
    let sources = auth.item_gold_sources(hour + 3600, 1).unwrap();
    assert_eq!((sources.entries[0].quantity, sources.total_gold), (5, 1310));
    assert_eq!(count(), 5);
    assert!(game.pending_gold_sources.read().await.is_empty());
}
