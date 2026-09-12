use super::*;

// --- Enchant weapon scrolls ---

/// Instance id the scroll stack takes in every setup below; the equipped
/// pieces number up from 1.
const SCROLL_ID: u64 = 9;

/// Instance id of the whetstone oil every reading burns alongside the scroll.
const OIL_ID: u64 = 10;

/// Spawn a live player wearing `equipped` (slot, def id, enchant) with a stack
/// of `scroll_def_id` and `oil` flasks of whetstone oil in the bag, and return
/// their direct channel.
async fn setup_enchant_reader(
    game_state: &GameState,
    equipped: &[(EquipSlot, &str, i32)],
    scroll_def_id: &str,
    scrolls: u32,
    oil: u32,
) -> DirectRx {
    game_state.add_player(make_player("reader", 0.0, 0.0)).await;
    let rx = game_state.register_direct_channel(&pid("reader")).await;

    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    for (index, (slot, item_def_id, enchant)) in equipped.iter().enumerate() {
        inv.equipped.insert(
            *slot,
            ItemInstance {
                locked: false,
                instance_id: index as u64 + 1,
                item_def_id: item_def_id.to_string(),
                quantity: 1,
                enchant: *enchant,
                cape_color: None,
                cape_texture: None,
            },
        );
    }
    inv.bag.push(bag_item(SCROLL_ID, scroll_def_id, scrolls));
    if oil > 0 {
        inv.bag.push(bag_item(OIL_ID, "whetstone_oil", oil));
    }
    game_state
        .inventories
        .write()
        .await
        .insert(pid("reader"), inv);
    rx
}

/// The weapon-scroll setup: a wielded `weapon` at an enchant level, or nothing
/// in hand.
async fn setup_weapon_enchant_reader(
    game_state: &GameState,
    weapon: Option<(&str, i32)>,
    scrolls: u32,
) -> DirectRx {
    let equipped: Vec<(EquipSlot, &str, i32)> = weapon
        .map(|(def_id, enchant)| (EquipSlot::MainHand, def_id, enchant))
        .into_iter()
        .collect();
    setup_enchant_reader(
        game_state,
        &equipped,
        "scroll_of_enchant_weapon",
        scrolls,
        scrolls,
    )
    .await
}

#[tokio::test]
async fn enchant_scroll_enchants_wielded_weapon() {
    let game_state = make_test_game_state("enchant_ok");
    let _rx = setup_weapon_enchant_reader(&game_state, Some(("iron_sword", 0)), 1).await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    let weapon = inv.equipped.get(&EquipSlot::MainHand).unwrap();
    assert_eq!(weapon.enchant, 1);
    assert!(inv.bag.is_empty(), "the scroll and the oil should be spent");
    assert!(game_state
        .pending_weapon_enchant_failures
        .read()
        .await
        .is_empty());
}

#[tokio::test]
async fn enchant_scroll_preserves_locked_weapons_and_materials() {
    for enchant in [0, 5, 12] {
        let game = make_test_game_state(&format!("locked_enchant_weapon_{enchant}"));
        let mut rx = setup_weapon_enchant_reader(&game, Some(("iron_sword", enchant)), 1).await;
        game.set_item_locked(&pid("reader"), 1, true).await;
        let before = game.get_player_inventory(&pid("reader")).await.unwrap();
        while rx.try_recv().is_ok() {}

        game.use_item(&pid("reader"), SCROLL_ID).await;

        let after = game.get_player_inventory(&pid("reader")).await.unwrap();
        assert_eq!(after.equipped, before.equipped);
        assert_eq!(after.bag, before.bag);
        assert!(matches!(
            rx.try_recv(),
            Ok(ServerMessage::SystemMessage { message }) if message.contains("unlocked")
        ));
        if enchant == 0 {
            game.set_item_locked(&pid("reader"), 1, false).await;
            game.use_item(&pid("reader"), SCROLL_ID).await;
            let after = game.get_player_inventory(&pid("reader")).await.unwrap();
            assert_eq!(after.equipped[&EquipSlot::MainHand].enchant, 1);
            assert!(after.bag.is_empty());
        }
    }
}

#[tokio::test]
async fn unlocked_equipment_can_be_enchanted_with_locked_consumables() {
    let game = make_test_game_state("locked_enchant_consumables");
    let _rx = setup_weapon_enchant_reader(&game, Some(("iron_sword", 0)), 1).await;
    game.set_item_locked(&pid("reader"), SCROLL_ID, true).await;
    game.set_item_locked(&pid("reader"), OIL_ID, true).await;
    game.use_item(&pid("reader"), SCROLL_ID).await;
    let inv = game.get_player_inventory(&pid("reader")).await.unwrap();
    assert!(!inv.equipped[&EquipSlot::MainHand].locked);
    assert_eq!(inv.equipped[&EquipSlot::MainHand].enchant, 1);
    assert!(inv.bag.is_empty());
}

#[tokio::test]
async fn enchant_scroll_requires_wielded_weapon() {
    let game_state = make_test_game_state("enchant_no_weapon");
    let mut rx = setup_weapon_enchant_reader(&game_state, None, 1).await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    assert_eq!(inv.bag.len(), 2, "the scroll and the oil should be kept");
    match rx.try_recv() {
        Ok(ServerMessage::SystemMessage { message }) => {
            assert!(
                message.contains("no weapon"),
                "unexpected message: {message}"
            );
        }
        other => panic!("Expected a system reply, got {:?}", other),
    }
}

// --- Enchant armor scrolls ---

/// The armor-scroll setup: whatever gear the case needs worn.
async fn setup_armor_enchant_reader(
    game_state: &GameState,
    armor: &[(EquipSlot, &str, i32)],
    scrolls: u32,
) -> DirectRx {
    setup_enchant_reader(
        game_state,
        armor,
        "scroll_of_enchant_armor",
        scrolls,
        scrolls,
    )
    .await
}

#[tokio::test]
async fn enchant_armor_scroll_enchants_worn_armor() {
    let game_state = make_test_game_state("enchant_armor_ok");
    let _rx =
        setup_armor_enchant_reader(&game_state, &[(EquipSlot::Chest, "leather_armor", 0)], 1).await;

    let base_guard = game_state.effective_guard(&pid("reader")).await;
    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    assert_eq!(inv.equipped.get(&EquipSlot::Chest).unwrap().enchant, 1);
    assert!(inv.bag.is_empty(), "the scroll and the oil should be spent");
    assert_eq!(
        game_state.effective_guard(&pid("reader")).await,
        base_guard + 1,
        "the enchant should raise guard"
    );
}

#[tokio::test]
async fn enchant_armor_scroll_ignores_weapons_and_accessories() {
    let game_state = make_test_game_state("enchant_armor_targets");
    let _rx = setup_armor_enchant_reader(
        &game_state,
        &[
            (EquipSlot::MainHand, "iron_sword", 0),
            (EquipSlot::Ring, "ring_of_protection", 0),
            (EquipSlot::Chest, "leather_armor", 0),
        ],
        1,
    )
    .await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    assert_eq!(inv.equipped.get(&EquipSlot::Chest).unwrap().enchant, 1);
    assert_eq!(inv.equipped.get(&EquipSlot::MainHand).unwrap().enchant, 0);
    assert_eq!(inv.equipped.get(&EquipSlot::Ring).unwrap().enchant, 0);
}

#[tokio::test]
async fn enchant_armor_scroll_skips_locked_armor() {
    let game = make_test_game_state("enchant_armor_locked_candidate");
    let _rx = setup_armor_enchant_reader(
        &game,
        &[
            (EquipSlot::Chest, "leather_armor", 12),
            (EquipSlot::OffHand, "wooden_shield", 0),
        ],
        3,
    )
    .await;
    game.set_item_locked(&pid("reader"), 1, true).await;
    let before = game.get_player_inventory(&pid("reader")).await.unwrap();

    for enchant in 1..=3 {
        game.use_item(&pid("reader"), SCROLL_ID).await;
        let after = game.get_player_inventory(&pid("reader")).await.unwrap();
        assert_eq!(
            after.equipped[&EquipSlot::Chest],
            before.equipped[&EquipSlot::Chest]
        );
        assert_eq!(after.equipped[&EquipSlot::OffHand].enchant, enchant);
    }
    assert!(game
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap()
        .bag
        .is_empty());
}

#[tokio::test]
async fn enchant_armor_scroll_keeps_materials_when_all_armor_is_locked() {
    let game = make_test_game_state("enchant_armor_all_locked");
    let mut rx = setup_armor_enchant_reader(
        &game,
        &[
            (EquipSlot::Chest, "leather_armor", 12),
            (EquipSlot::OffHand, "wooden_shield", 12),
        ],
        1,
    )
    .await;
    game.set_item_locked(&pid("reader"), 1, true).await;
    game.set_item_locked(&pid("reader"), 2, true).await;
    let before = game.get_player_inventory(&pid("reader")).await.unwrap();
    while rx.try_recv().is_ok() {}

    game.use_item(&pid("reader"), SCROLL_ID).await;

    let after = game.get_player_inventory(&pid("reader")).await.unwrap();
    assert_eq!(after.equipped, before.equipped);
    assert_eq!(after.bag, before.bag);
    assert!(matches!(
        rx.try_recv(),
        Ok(ServerMessage::SystemMessage { message }) if message.contains("unlocked")
    ));
}

#[tokio::test]
async fn enchant_armor_scroll_requires_worn_armor() {
    let game_state = make_test_game_state("enchant_armor_none");
    let mut rx =
        setup_armor_enchant_reader(&game_state, &[(EquipSlot::MainHand, "iron_sword", 0)], 1).await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    assert_eq!(inv.bag.len(), 2, "the scroll and the oil should be kept");
    match rx.try_recv() {
        Ok(ServerMessage::SystemMessage { message }) => {
            assert!(
                message.contains("no armor"),
                "unexpected message: {message}"
            );
        }
        other => panic!("Expected a system reply, got {:?}", other),
    }
}

#[tokio::test]
async fn enchant_armor_scroll_destroys_over_enchanted_armor() {
    let game_state = make_test_game_state("enchant_armor_boom");
    let _rx =
        setup_armor_enchant_reader(&game_state, &[(EquipSlot::Chest, "leather_armor", 12)], 100)
            .await;

    let reader = pid("reader");
    game_state
        .player_characters
        .write()
        .await
        .insert(reader, (1, 0, attrs_with_cha(10)));
    for _ in 0..100 {
        game_state.use_item(&reader, SCROLL_ID).await;
        let inv = game_state.get_player_inventory(&reader).await.unwrap();
        if !inv.equipped.contains_key(&EquipSlot::Chest) {
            assert!(game_state
                .pending_weapon_enchant_failures
                .read()
                .await
                .is_empty());
            return;
        }
    }
    panic!("the armor should have evaporated within 100 reads at 99% odds");
}

#[tokio::test]
async fn enchant_scroll_destroys_over_enchanted_weapon() {
    let game_state = make_test_game_state("enchant_boom");
    // Surviving 100 attempts at +12 has probability about 1e-200.
    let _rx = setup_weapon_enchant_reader(&game_state, Some(("iron_sword", 12)), 100).await;

    let reader = pid("reader");
    for _ in 0..100 {
        game_state.use_item(&reader, SCROLL_ID).await;
        let inv = game_state.get_player_inventory(&reader).await.unwrap();
        if !inv.equipped.contains_key(&EquipSlot::MainHand) {
            return; // evaporated, as expected
        }
    }
    panic!("the weapon should have evaporated within 100 reads at 99% odds");
}

#[tokio::test]
async fn enchanting_needs_whetstone_oil() {
    let game_state = make_test_game_state("enchant_no_oil");
    let mut rx = setup_enchant_reader(
        &game_state,
        &[(EquipSlot::MainHand, "iron_sword", 0)],
        "scroll_of_enchant_weapon",
        1,
        0,
    )
    .await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    assert_eq!(inv.equipped.get(&EquipSlot::MainHand).unwrap().enchant, 0);
    assert_eq!(inv.bag.len(), 1, "the scroll should be kept");
    match rx.try_recv() {
        Ok(ServerMessage::SystemMessage { message }) => {
            assert!(message.contains("Oil"), "unexpected message: {message}");
        }
        other => panic!("Expected a system reply, got {:?}", other),
    }
}

#[tokio::test]
async fn each_reading_burns_one_flask_of_oil() {
    let game_state = make_test_game_state("enchant_oil_spend");
    let _rx = setup_weapon_enchant_reader(&game_state, Some(("iron_sword", 0)), 3).await;

    game_state.use_item(&pid("reader"), SCROLL_ID).await;

    let inv = game_state
        .get_player_inventory(&pid("reader"))
        .await
        .unwrap();
    let oil = inv
        .bag
        .iter()
        .find(|item| item.item_def_id == "whetstone_oil")
        .expect("the oil stack should survive a single reading");
    assert_eq!(oil.quantity, 2, "one flask per reading");
}

#[tokio::test]
async fn weapon_enchant_failures_record_each_loss_and_exclude_official_npcs() {
    for official_npc in [false, true] {
        let game = make_test_game_state("enchant_failure_events");
        let reader = pid("reader");
        for enchant in [8, 7] {
            let _rx = setup_weapon_enchant_reader(&game, Some(("iron_sword", enchant)), 100).await;
            game.player_characters
                .write()
                .await
                .insert(reader, (1, 0, attrs_with_cha(10)));
            game.players
                .write()
                .await
                .get_mut(&reader)
                .unwrap()
                .is_official_npc = official_npc;
            for attempt in 0..100 {
                game.use_item(&reader, SCROLL_ID).await;
                let mut inventories = game.inventories.write().await;
                let inv = inventories.get_mut(&reader).unwrap();
                let Some(weapon) = inv.equipped.get_mut(&EquipSlot::MainHand) else {
                    break;
                };
                assert!(
                    attempt < 99,
                    "weapon should be destroyed within 100 attempts"
                );
                weapon.enchant = enchant;
            }
        }
        let failures = game.pending_weapon_enchant_failures.read().await;
        if official_npc {
            assert!(failures.is_empty());
        } else {
            assert_eq!(failures.len(), 2);
            assert_eq!(
                failures
                    .iter()
                    .map(|event| event.enchant)
                    .collect::<Vec<_>>(),
                [8, 7]
            );
            assert_ne!(failures[0].id, failures[1].id);
            for failure in failures.iter() {
                assert_eq!(failure.character_id, 1);
                assert_eq!(failure.name, "reader");
                assert_eq!(failure.item_def_id, "iron_sword");
                assert_eq!(failure.item_name, game.item_name("iron_sword"));
            }
        }
    }
}

#[tokio::test]
async fn weapon_enchant_failures_retry_saves_after_character_leaves() {
    let game = make_test_game_state("enchant_failure_retry");
    let path = crate::test_util::unique_temp_dir("enchant_failure_retry").join("game.db");
    let auth = crate::auth::AuthService::new(path.clone()).unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "ALTER TABLE weapon_enchant_failures RENAME TO saved_failures",
        [],
    )
    .unwrap();
    let failure = crate::metrics::WeaponEnchantFailure {
        id: "retry-event".into(),
        timestamp: crate::auth::unix_now(),
        character_id: 1,
        name: "departed reader".into(),
        item_def_id: "iron_sword".into(),
        item_name: "Iron Sword".into(),
        enchant: 8,
    };
    game.pending_weapon_enchant_failures
        .write()
        .await
        .push(failure.clone());
    game.flush_dirty_saves(&auth).await;
    assert_eq!(
        game.pending_weapon_enchant_failures.read().await.as_slice(),
        std::slice::from_ref(&failure)
    );
    conn.execute(
        "ALTER TABLE saved_failures RENAME TO weapon_enchant_failures",
        [],
    )
    .unwrap();
    game.flush_dirty_saves(&auth).await;
    game.flush_dirty_saves(&auth).await;
    assert!(game.pending_weapon_enchant_failures.read().await.is_empty());
    let result = auth
        .weapon_enchant_failures(crate::auth::unix_now())
        .unwrap();
    assert_eq!(result.entries.len(), 1);
    assert_eq!(result.entries[0].latest, failure);
    assert_eq!(result.entries[0].failure_count, 1);
}
