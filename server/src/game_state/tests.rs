use super::*;
use crate::housing::HousingIO;
use crate::item_defs::ItemDefs;
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterClass, Gender, MonsterState, Position, ServerMessage};
use crate::world_config::world_config;
use onlinerpg_shared::inventory::{GroundItem, ItemInstance};
use onlinerpg_shared::messages::DealKind;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::mpsc::error::TryRecvError as MpscTryRecvError;

fn make_player(id: &str, x: f32, z: f32) -> Player {
    Player {
        id: id.to_string(),
        name: id.to_string(),
        position: Position { x, y: 0.0, z },
        rotation: 0.0,
        level: 1,
        health: 10,
        max_health: 10,
        class: CharacterClass::Knight,
        gender: Gender::default(),
        is_npc: false,
        torch_on: false,
        floor_level: 0,
        object_type: None,
        object_id: None,
        last_combat_at: 0,
    }
}

fn make_test_game_state(test_name: &str) -> GameState {
    let housing_dir = std::env::temp_dir().join(format!(
        "onlinerpg_{test_name}_housing_{}",
        uuid::Uuid::new_v4()
    ));
    let housing_io = Arc::new(HousingIO::new(housing_dir));
    GameState::new(
        MonsterDefs::load(),
        ItemDefs::load(),
        GameState::default_start_datetime(),
        housing_io,
        vec![],
    )
}

#[tokio::test]
async fn respawn_player_revives_dead_player_only() {
    let game_state = make_test_game_state("respawn_dead");

    let player = Player {
        id: "player_dead".to_string(),
        name: "DeadPlayer".to_string(),
        position: Position {
            x: 12.0,
            y: 0.0,
            z: -4.0,
        },
        rotation: 1.25,
        level: 3,
        health: 0,
        max_health: 30,
        class: CharacterClass::Knight,
        gender: Gender::default(),
        is_npc: false,
        torch_on: false,
        floor_level: 0,
        object_type: None,
        object_id: None,
        last_combat_at: 0,
    };
    let player_id = player.id.clone();
    game_state.add_player(player).await;

    let mut direct_rx = game_state.register_direct_channel(&player_id).await;
    let mut broadcast_rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let revived = players
        .get(&player_id)
        .expect("Player should still exist after respawn");
    let spawn = &world_config().spawn_position;
    assert_eq!(revived.health, revived.max_health);
    assert_eq!(revived.position.x, spawn.x);
    assert_eq!(revived.position.y, spawn.y);
    assert_eq!(revived.position.z, spawn.z);
    assert_eq!(revived.rotation, spawn.rotation);

    match direct_rx.try_recv() {
        Ok(ServerMessage::PlayerRespawned { player }) => {
            assert_eq!(player.id, player_id);
            assert_eq!(player.health, player.max_health);
        }
        other => panic!("Expected direct PlayerRespawned, got {:?}", other),
    }

    match broadcast_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!("Expected no respawn broadcast, got {:?}", server_msg);
        }
        Err(err) => panic!("Expected empty broadcast channel, got {:?}", err),
    }
}

#[tokio::test]
async fn respawn_player_ignores_alive_player() {
    let game_state = make_test_game_state("respawn_alive");

    let player = Player {
        id: "player_alive".to_string(),
        name: "AlivePlayer".to_string(),
        position: Position {
            x: 5.0,
            y: 0.0,
            z: 6.0,
        },
        rotation: 0.75,
        level: 2,
        health: 18,
        max_health: 20,
        class: CharacterClass::Knight,
        gender: Gender::default(),
        is_npc: false,
        torch_on: false,
        floor_level: 0,
        object_type: None,
        object_id: None,
        last_combat_at: 0,
    };
    let player_id = player.id.clone();
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let unchanged = players
        .get(&player_id)
        .expect("Player should still exist after ignored respawn");
    assert_eq!(unchanged.health, 18);
    assert_eq!(unchanged.position.x, 5.0);
    assert_eq!(unchanged.position.y, 0.0);
    assert_eq!(unchanged.position.z, 6.0);
    assert_eq!(unchanged.rotation, 0.75);

    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!(
                "Expected no broadcast for alive respawn, got {:?}",
                server_msg
            );
        }
        Err(err) => panic!("Expected empty channel, got {:?}", err),
    }
}

#[tokio::test]
async fn chat_uses_direct_spatial_fanout_instead_of_global_broadcast() {
    let game_state = make_test_game_state("chat_spatial_fanout");
    let speaker_id = "speaker".to_string();
    let near_listener_id = "near_listener".to_string();
    let far_listener_id = "far_listener".to_string();

    game_state
        .add_player(make_player("speaker", 0.0, 0.0))
        .await;
    game_state
        .add_player(make_player("near_listener", 10.0, 0.0))
        .await;
    game_state
        .add_player(make_player("far_listener", 100.0, 0.0))
        .await;

    let mut speaker_rx = game_state.register_direct_channel(&speaker_id).await;
    let mut near_rx = game_state.register_direct_channel(&near_listener_id).await;
    let mut far_rx = game_state.register_direct_channel(&far_listener_id).await;

    let mut broadcast_rx = game_state.subscribe();
    game_state
        .send_chat_message(&speaker_id, "hello".to_string())
        .await;

    match speaker_rx.try_recv() {
        Ok(ServerMessage::ChatMessage { player_id, message }) => {
            assert_eq!(player_id, "speaker");
            assert_eq!(message, "hello");
        }
        other => panic!("Expected direct chat for speaker, got {:?}", other),
    }

    match near_rx.try_recv() {
        Ok(ServerMessage::ChatMessage { player_id, message }) => {
            assert_eq!(player_id, "speaker");
            assert_eq!(message, "hello");
        }
        other => panic!("Expected direct chat for nearby listener, got {:?}", other),
    }

    match far_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!("Expected no direct chat for far listener, got {:?}", other),
    }

    match broadcast_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!("Expected no chat broadcast, got {:?}", server_msg);
        }
        Err(err) => panic!("Expected empty broadcast channel, got {:?}", err),
    }
}

#[tokio::test]
async fn movement_into_aoi_sends_existing_monsters_and_ground_items() {
    let game_state = make_test_game_state("movement_world_entity_aoi");
    let player_id = "walker".to_string();
    let entity_position = Position {
        x: 100.0,
        y: 0.0,
        z: 0.0,
    };

    game_state
        .add_player(make_player(&player_id, 0.0, 0.0))
        .await;
    let mut direct_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "monster_a".to_string(),
            crate::types::Monster {
                id: "monster_a".to_string(),
                monster_type: "test_monster".to_string(),
                position: entity_position,
                rotation: 0.0,
                state: MonsterState::Idle,
                owner_id: None,
                health: 10,
                max_health: 10,
                last_attack_at: 0,
            },
        );
    }

    {
        let mut ground_items = game_state.ground_items.write().await;
        ground_items.insert(
            42,
            ServerGroundItem {
                item: GroundItem {
                    instance_id: 42,
                    item_def_id: "test_item".to_string(),
                    position: entity_position,
                    floor_level: 0,
                },
                dropped_at_ms: 0,
            },
        );
    }

    game_state
        .update_player_position(&player_id, entity_position, 0.0, 0)
        .await;

    match direct_rx.try_recv() {
        Ok(ServerMessage::MonsterSpawned { monster }) => {
            assert_eq!(monster.id, "monster_a");
        }
        other => panic!("Expected MonsterSpawned when entering AOI, got {:?}", other),
    }

    match direct_rx.try_recv() {
        Ok(ServerMessage::GroundItemAppeared { item }) => {
            assert_eq!(item.instance_id, 42);
        }
        other => panic!(
            "Expected GroundItemAppeared when entering AOI, got {:?}",
            other
        ),
    }

    match direct_rx.try_recv() {
        Ok(ServerMessage::PlayerMoved {
            player_id: moved_id,
            ..
        }) => {
            assert_eq!(moved_id, player_id);
        }
        other => panic!(
            "Expected self PlayerMoved after AOI snapshot, got {:?}",
            other
        ),
    }
}

// --- Haggling (economy phase 2) ---

fn make_merchant_npc(id: &str, x: f32, z: f32) -> Player {
    let mut p = make_player(id, x, z);
    p.name = "Rica".to_string();
    p.is_npc = true;
    p
}

fn attrs_with_cha(cha: u8) -> CharacterAttributes {
    CharacterAttributes {
        r#str: 10,
        dex: 10,
        con: 10,
        int: 10,
        wis: 10,
        cha,
        guard: 0,
    }
}

/// Spawn a merchant NPC and a buyer with the given CHA/gold next to each
/// other, returning the buyer's direct-message receiver and the NPC's.
async fn setup_haggle(
    game_state: &GameState,
    cha: u8,
    gold: i64,
) -> (
    tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
    tokio::sync::mpsc::UnboundedReceiver<ServerMessage>,
) {
    game_state
        .add_player(make_merchant_npc("npc_rica", 0.0, 0.0))
        .await;
    game_state.add_player(make_player("buyer", 1.0, 0.0)).await;
    game_state
        .register_player_character(&"buyer".to_string(), 1, 0, attrs_with_cha(cha), gold)
        .await;
    let buyer_rx = game_state
        .register_direct_channel(&"buyer".to_string())
        .await;
    let npc_rx = game_state
        .register_direct_channel(&"npc_rica".to_string())
        .await;
    (buyer_rx, npc_rx)
}

#[test]
fn haggling_band_invariant_boundary() {
    // Rica's actual rate must satisfy the invariant; 60% is the first rate
    // where max haggled sell (60% * 1.25) meets min haggled buy (75%).
    assert!(deals::band_invariant_holds(40));
    assert!(deals::band_invariant_holds(59));
    assert!(!deals::band_invariant_holds(60));
}

#[test]
fn haggling_band_widens_with_cha_within_limits() {
    assert_eq!(deals::deal_half_band_pct(10), 10);
    assert_eq!(deals::deal_half_band_pct(3), 5);
    assert_eq!(deals::deal_half_band_pct(13), 16);
    assert_eq!(deals::deal_half_band_pct(18), 25);
    assert_eq!(deals::deal_half_band_pct(255), 25);
}

#[tokio::test]
async fn offer_deal_clamps_modifier_to_cha_band() {
    let game_state = make_test_game_state("offer_clamp");
    let (mut buyer_rx, mut npc_rx) = setup_haggle(&game_state, 10, 0).await;

    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "iron_sword",
            DealKind::Buy,
            -50,
            "loyal customer",
        )
        .await;

    match buyer_rx.try_recv() {
        Ok(ServerMessage::DealUpdated {
            item_def_id,
            kind,
            modifier_pct,
            ..
        }) => {
            assert_eq!(item_def_id, "iron_sword");
            assert_eq!(kind, DealKind::Buy);
            assert_eq!(modifier_pct, -10, "CHA 10 band is ±10");
        }
        other => panic!("Expected DealUpdated for buyer, got {:?}", other),
    }
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult {
            accepted,
            applied_modifier_pct,
            ..
        }) => {
            assert!(accepted);
            assert_eq!(applied_modifier_pct, -10);
        }
        other => panic!("Expected DealResult for NPC, got {:?}", other),
    }
}

#[tokio::test]
async fn offer_deal_enforces_cooldown_and_player_budget() {
    let game_state = make_test_game_state("offer_limits");
    let (_buyer_rx, mut npc_rx) = setup_haggle(&game_state, 18, 0).await;

    // First offer: accepted (CHA 18 → band ±25, cost 2500 on iron_sword).
    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "iron_sword",
            DealKind::Buy,
            -25,
            "first",
        )
        .await;
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult { accepted, .. }) => assert!(accepted),
        other => panic!("Expected accepted DealResult, got {:?}", other),
    }

    // Immediate second offer: rejected by the cooldown.
    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "dagger",
            DealKind::Buy,
            -5,
            "second",
        )
        .await;
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult {
            accepted, message, ..
        }) => {
            assert!(!accepted);
            assert!(message.contains("cooldown"), "got: {message}");
        }
        other => panic!("Expected cooldown rejection, got {:?}", other),
    }

    // Cooldown lifted: the player's daily discount cap (4000) now rejects a
    // second 2500-cost discount.
    game_state.clear_deal_cooldowns_for_test().await;
    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "iron_sword",
            DealKind::Buy,
            -25,
            "third",
        )
        .await;
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult {
            accepted, message, ..
        }) => {
            assert!(!accepted);
            assert!(message.contains("discount limit"), "got: {message}");
        }
        other => panic!("Expected budget rejection, got {:?}", other),
    }
}

#[tokio::test]
async fn buy_item_applies_deal_once() {
    let game_state = make_test_game_state("buy_with_deal");
    let (_buyer_rx, _npc_rx) = setup_haggle(&game_state, 10, 30_000).await;
    {
        let mut inventories = game_state.inventories.write().await;
        inventories.insert("buyer".to_string(), Default::default());
    }

    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "iron_sword",
            DealKind::Buy,
            -10,
            "deal",
        )
        .await;

    // First buy uses the -10% deal: 10000 → 9000.
    game_state
        .buy_item(&"buyer".to_string(), "npc_rica", "iron_sword")
        .await;
    assert_eq!(
        game_state.get_player_gold(&"buyer".to_string()).await,
        21_000
    );

    // The deal is single-use: the second buy pays full price.
    game_state
        .buy_item(&"buyer".to_string(), "npc_rica", "iron_sword")
        .await;
    assert_eq!(
        game_state.get_player_gold(&"buyer".to_string()).await,
        11_000
    );
}

#[tokio::test]
async fn sell_item_applies_deal_bonus() {
    let game_state = make_test_game_state("sell_with_deal");
    let (_buyer_rx, _npc_rx) = setup_haggle(&game_state, 18, 0).await;
    {
        let mut inventories = game_state.inventories.write().await;
        let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
        inv.bag.push(onlinerpg_shared::inventory::ItemInstance {
            instance_id: 7,
            item_def_id: "iron_sword".to_string(),
            quantity: 1,
        });
        inventories.insert("buyer".to_string(), inv);
    }

    game_state
        .offer_deal(
            &"npc_rica".to_string(),
            "buyer",
            "iron_sword",
            DealKind::Sell,
            25,
            "today's wanted item",
        )
        .await;

    // Sell rate 40% with a +25% bonus: 10000 * 0.4 * 1.25 = 5000.
    game_state.sell_item(&"buyer".to_string(), "npc_rica", 7).await;
    assert_eq!(
        game_state.get_player_gold(&"buyer".to_string()).await,
        5_000
    );
}

// --- Resident (non-merchant) trading (economy phase 3) ---

fn make_resident_npc(id: &str, x: f32, z: f32) -> Player {
    let mut p = make_player(id, x, z);
    p.name = "Karl".to_string();
    p.is_npc = true;
    p
}

fn bag_item(instance_id: u64, item_def_id: &str, quantity: u32) -> ItemInstance {
    ItemInstance {
        instance_id,
        item_def_id: item_def_id.to_string(),
        quantity,
    }
}

/// Spawn the resident trader Karl (wishlist: torch, dagger @120%) next to a
/// seller. Karl's wallet and bag are set explicitly; the seller starts with
/// the given bag and no gold.
async fn setup_resident_trade(
    game_state: &GameState,
    npc_gold: i64,
    npc_bag: Vec<ItemInstance>,
    seller_bag: Vec<ItemInstance>,
) {
    game_state
        .add_player(make_resident_npc("npc_karl", 0.0, 0.0))
        .await;
    game_state.add_player(make_player("seller", 1.0, 0.0)).await;
    game_state
        .register_player_character(&"seller".to_string(), 1, 0, attrs_with_cha(10), 0)
        .await;
    game_state
        .register_player_character(&"npc_karl".to_string(), 2, 0, attrs_with_cha(10), npc_gold)
        .await;
    let mut inventories = game_state.inventories.write().await;
    inventories.insert(
        "npc_karl".to_string(),
        onlinerpg_shared::inventory::PlayerInventory {
            bag: npc_bag,
            ..Default::default()
        },
    );
    inventories.insert(
        "seller".to_string(),
        onlinerpg_shared::inventory::PlayerInventory {
            bag: seller_bag,
            ..Default::default()
        },
    );
}

#[tokio::test]
async fn resident_buys_wishlist_item_at_premium_from_wallet() {
    let game_state = make_test_game_state("resident_sell");
    setup_resident_trade(
        &game_state,
        10_000,
        vec![],
        vec![bag_item(7, "torch", 1)],
    )
    .await;

    // Torch base 50 at Karl's 120% wishlist rate → 60.
    game_state.sell_item(&"seller".to_string(), "npc_karl", 7).await;
    assert_eq!(game_state.get_player_gold(&"seller".to_string()).await, 60);
    assert_eq!(
        game_state.get_player_gold(&"npc_karl".to_string()).await,
        10_000 - 60
    );

    // The torch landed in Karl's real inventory; the seller's bag is empty.
    let inventories = game_state.inventories.read().await;
    assert_eq!(inventories["npc_karl"].bag.len(), 1);
    assert_eq!(inventories["npc_karl"].bag[0].item_def_id, "torch");
    assert!(inventories["seller"].bag.is_empty());
}

#[tokio::test]
async fn resident_rejects_items_off_the_wishlist() {
    let game_state = make_test_game_state("resident_off_wishlist");
    setup_resident_trade(
        &game_state,
        10_000,
        vec![],
        vec![bag_item(7, "iron_sword", 1)],
    )
    .await;
    let mut seller_rx = game_state
        .register_direct_channel(&"seller".to_string())
        .await;

    game_state.sell_item(&"seller".to_string(), "npc_karl", 7).await;

    assert_eq!(game_state.get_player_gold(&"seller".to_string()).await, 0);
    match seller_rx.try_recv() {
        Ok(ServerMessage::TradeError { message }) => {
            assert!(message.contains("no use"), "got: {message}")
        }
        other => panic!("Expected TradeError, got {:?}", other),
    }
    let inventories = game_state.inventories.read().await;
    assert_eq!(inventories["seller"].bag.len(), 1, "item must be retained");
}

#[tokio::test]
async fn resident_wallet_caps_purchases() {
    let game_state = make_test_game_state("resident_wallet_cap");
    // Karl has 59 gold units; the torch costs him 60.
    setup_resident_trade(&game_state, 59, vec![], vec![bag_item(7, "torch", 1)]).await;
    let mut seller_rx = game_state
        .register_direct_channel(&"seller".to_string())
        .await;

    game_state.sell_item(&"seller".to_string(), "npc_karl", 7).await;

    assert_eq!(game_state.get_player_gold(&"seller".to_string()).await, 0);
    assert_eq!(game_state.get_player_gold(&"npc_karl".to_string()).await, 59);
    match seller_rx.try_recv() {
        Ok(ServerMessage::TradeError { message }) => {
            assert!(message.contains("afford"), "got: {message}")
        }
        other => panic!("Expected TradeError, got {:?}", other),
    }
}

#[tokio::test]
async fn resident_sells_stock_but_keeps_wishlist_items() {
    let game_state = make_test_game_state("resident_stock");
    // Karl carries a spear (sellable stock) and a torch (wishlist: kept).
    setup_resident_trade(
        &game_state,
        0,
        vec![bag_item(11, "spear", 1), bag_item(12, "torch", 1)],
        vec![],
    )
    .await;
    {
        let mut gold = game_state.player_gold.write().await;
        gold.insert("seller".to_string(), 10_000);
    }
    let mut seller_rx = game_state
        .register_direct_channel(&"seller".to_string())
        .await;

    // Spear base 3500 — instance moves to the buyer, gold to Karl.
    game_state
        .buy_item(&"seller".to_string(), "npc_karl", "spear")
        .await;
    assert_eq!(
        game_state.get_player_gold(&"seller".to_string()).await,
        6_500
    );
    assert_eq!(game_state.get_player_gold(&"npc_karl".to_string()).await, 3_500);
    {
        let inventories = game_state.inventories.read().await;
        assert_eq!(inventories["seller"].bag.len(), 1);
        assert_eq!(inventories["seller"].bag[0].item_def_id, "spear");
        assert_eq!(inventories["npc_karl"].bag.len(), 1);
        assert_eq!(inventories["npc_karl"].bag[0].item_def_id, "torch");
    }
    while seller_rx.try_recv().is_ok() {}

    // The torch is on Karl's wishlist: he keeps it (no buy-back pump).
    game_state
        .buy_item(&"seller".to_string(), "npc_karl", "torch")
        .await;
    assert_eq!(
        game_state.get_player_gold(&"seller".to_string()).await,
        6_500
    );
    match seller_rx.try_recv() {
        Ok(ServerMessage::TradeError { message }) => {
            assert!(message.contains("part with"), "got: {message}")
        }
        other => panic!("Expected TradeError, got {:?}", other),
    }
}

#[tokio::test]
async fn resident_shop_state_reports_wishlist_and_stock() {
    let game_state = make_test_game_state("resident_shop_state");
    setup_resident_trade(
        &game_state,
        4_321,
        vec![
            bag_item(11, "spear", 1),
            bag_item(12, "torch", 1),
            bag_item(13, "worn_iron_sword", 1),
        ],
        vec![],
    )
    .await;
    let mut seller_rx = game_state
        .register_direct_channel(&"seller".to_string())
        .await;

    game_state.open_shop(&"seller".to_string(), "npc_karl").await;

    match seller_rx.try_recv() {
        Ok(ServerMessage::ShopState {
            merchant_name,
            catalog,
            sell_rate_percent,
            wishlist,
            stock,
            ..
        }) => {
            assert_eq!(merchant_name, "Karl");
            assert!(catalog.is_empty());
            assert_eq!(sell_rate_percent, 120);
            assert_eq!(wishlist, vec!["torch".to_string(), "dagger".to_string()]);
            // Stock excludes the wishlist torch and the unpriced worn sword.
            assert_eq!(stock.len(), 1);
            assert_eq!(stock[0].item_def_id, "spear");
            assert_eq!(stock[0].quantity, 1);
        }
        other => panic!("Expected ShopState, got {:?}", other),
    }
}

#[tokio::test]
async fn resident_deal_band_is_wider_and_wishlist_scoped() {
    let game_state = make_test_game_state("resident_deal_band");
    setup_resident_trade(&game_state, 10_000, vec![], vec![]).await;
    let mut npc_rx = game_state
        .register_direct_channel(&"npc_karl".to_string())
        .await;

    // CHA 10 resident band is ±20 (twice the merchant ±10).
    game_state
        .offer_deal(
            &"npc_karl".to_string(),
            "seller",
            "torch",
            DealKind::Sell,
            40,
            "really need torches tonight",
        )
        .await;
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult {
            accepted,
            applied_modifier_pct,
            ..
        }) => {
            assert!(accepted);
            assert_eq!(applied_modifier_pct, 20);
        }
        other => panic!("Expected DealResult, got {:?}", other),
    }

    // Sell offers outside the wishlist are rejected.
    game_state.clear_deal_cooldowns_for_test().await;
    game_state
        .offer_deal(
            &"npc_karl".to_string(),
            "seller",
            "iron_sword",
            DealKind::Sell,
            10,
            "nice sword",
        )
        .await;
    match npc_rx.try_recv() {
        Ok(ServerMessage::DealResult {
            accepted, message, ..
        }) => {
            assert!(!accepted);
            assert!(message.contains("wishlist"), "got: {message}");
        }
        other => panic!("Expected rejection, got {:?}", other),
    }
}

#[tokio::test]
async fn open_trade_pushes_shop_state_to_the_player() {
    let game_state = make_test_game_state("open_trade");
    setup_resident_trade(&game_state, 1_000, vec![], vec![]).await;
    let mut seller_rx = game_state
        .register_direct_channel(&"seller".to_string())
        .await;
    let npc_rx = game_state
        .register_direct_channel(&"npc_karl".to_string())
        .await;

    game_state
        .open_trade(&"npc_karl".to_string(), "seller")
        .await;
    match seller_rx.try_recv() {
        Ok(ServerMessage::ShopState { merchant_name, .. }) => assert_eq!(merchant_name, "Karl"),
        other => panic!("Expected ShopState, got {:?}", other),
    }

    // A non-trading NPC cannot push a window; the seller hears nothing.
    game_state
        .add_player({
            let mut p = make_player("npc_nobody", 0.5, 0.0);
            p.name = "Nobody".to_string();
            p.is_npc = true;
            p
        })
        .await;
    let mut nobody_rx = game_state
        .register_direct_channel(&"npc_nobody".to_string())
        .await;
    game_state
        .open_trade(&"npc_nobody".to_string(), "seller")
        .await;
    match nobody_rx.try_recv() {
        Ok(ServerMessage::TradeError { message }) => {
            assert!(message.contains("nothing to trade"), "got: {message}")
        }
        other => panic!("Expected TradeError, got {:?}", other),
    }
    drop(npc_rx);
}

#[tokio::test]
async fn salary_pays_once_per_day_rollover_up_to_cap() {
    let game_state = make_test_game_state("salary");
    setup_resident_trade(&game_state, 27_000, vec![], vec![]).await;

    // First tick after boot only records the day.
    game_state.tick_npc_salaries().await;
    assert_eq!(
        game_state.get_player_gold(&"npc_karl".to_string()).await,
        27_000
    );

    // Roll the ledger back a day: the next tick pays one salary, capped at
    // the 30_000 wallet cap (27_000 + 5_000 → 30_000).
    {
        let mut last = game_state.npc_salary_last_day.write().await;
        *last = last.map(|d| d - 1);
    }
    game_state.tick_npc_salaries().await;
    assert_eq!(
        game_state.get_player_gold(&"npc_karl".to_string()).await,
        30_000
    );

    // Same day again: no double payment.
    game_state.tick_npc_salaries().await;
    assert_eq!(
        game_state.get_player_gold(&"npc_karl".to_string()).await,
        30_000
    );
}
