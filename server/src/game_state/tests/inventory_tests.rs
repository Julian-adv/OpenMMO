use super::*;
use crate::game_state::inventory::{stack_into_bag, BagInsert};

fn insert(
    bag: &mut Vec<ItemInstance>,
    stackable: bool,
    def: &str,
    enchant: i32,
    id: u64,
    qty: u32,
) -> u64 {
    stack_into_bag(
        bag,
        BagInsert {
            locked: false,
            stackable,
            item_def_id: def,
            enchant,
            first_instance_id: id,
            quantity: qty,
            cape_color: None,
            cape_texture: None,
        },
    )
    .ids_used
}

#[test]
fn stack_into_bag_merges_only_same_def_and_enchant() {
    let mut bag = Vec::new();
    insert(&mut bag, true, "apple", 0, 1, 1);
    insert(&mut bag, true, "apple", 0, 2, 1);
    insert(&mut bag, true, "apple", 1, 3, 1);
    insert(&mut bag, false, "torch", 0, 4, 1);
    insert(&mut bag, false, "torch", 0, 5, 1);

    assert_eq!(bag.len(), 4, "merge only the same-def same-enchant pair");
    assert_eq!(bag[0].item_def_id, "apple");
    assert_eq!(bag[0].quantity, 2);
    assert_eq!(bag[1].enchant, 1);
    assert_eq!(bag[1].quantity, 1);
}

#[test]
fn locked_stacks_do_not_merge_with_unlocked_items() {
    let mut bag = vec![ItemInstance {
        locked: true,
        ..bag_item(1, "apple", 3)
    }];
    insert(&mut bag, true, "apple", 0, 2, 2);
    assert_eq!(bag.len(), 2);
    assert_eq!(bag[0].quantity, 3);
    assert!(bag[0].locked);
    let mut locked = BagInsert::one(true, "apple", 0, 3);
    locked.locked = true;
    stack_into_bag(&mut bag, locked);
    assert_eq!(bag.len(), 2);
    assert_eq!(bag[0].quantity, 4);
    assert_eq!(bag[1].quantity, 2);
}

#[tokio::test]
async fn ground_item_audit_links_single_and_batch_drops_to_merged_pickups() {
    use tracing::instrument::WithSubscriber;
    #[derive(Clone)]
    struct LogWriter(Arc<std::sync::Mutex<Vec<u8>>>);
    impl std::io::Write for LogWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let game = make_test_game_state("item_audit");
    let id = pid("keeper");
    game.add_player(make_player("keeper", 0.0, 0.0)).await;
    game.register_player_character(&id, 42, 0, attrs_with_cha(12), 0, None)
        .await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(11, "apple", 3)],
            ..Default::default()
        },
    );
    game.reserve_instance_ids(100).await;
    let buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let writer = LogWriter(buffer.clone());
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    async {
        game.drop_item(&id, 11).await;
        let first = *game.ground_items.read().await.keys().next().unwrap();
        game.pickup_item(&id, first).await;
        game.drop_items(
            &id,
            vec![onlinerpg_shared::messages::BagLineItem {
                instance_id: 11,
                qty: 2,
            }],
        )
        .await;
        let second = *game.ground_items.read().await.keys().next().unwrap();
        game.pickup_item(&id, second).await;
    }
    .with_subscriber(subscriber)
    .await;
    let bytes = buffer.lock().unwrap().clone();
    let logs = std::str::from_utf8(&bytes).unwrap();
    let events: Vec<_> = logs
        .lines()
        .filter(|line| line.contains("item_audit"))
        .collect();
    assert_eq!(events.len(), 4, "{logs}");
    assert_eq!(
        events
            .iter()
            .filter(|line| line.contains("action=\"drop\""))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|line| line.contains("action=\"pickup\""))
            .count(),
        2
    );
    assert!(events
        .iter()
        .all(|line| line.contains("character_id=Some(42)")
            && line.contains("bag_instance_id=Some(11)")));
    assert!(events
        .iter()
        .all(|line| line.contains("item_def_id=\"apple\"")
            && line.contains("enchant=0")
            && line.contains("floor=0")));
    assert!(events[2].contains("quantity=2"));
    assert!(events[3].contains("quantity=2"));
    assert_eq!(
        game.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        3
    );
}

#[tokio::test]
async fn item_lock_blocks_equipped_and_batch_drops_until_unlocked() {
    let game = make_test_game_state("locked_drops");
    let id = pid("keeper");
    game.add_player(make_player("keeper", 0.0, 0.0)).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![
                ItemInstance {
                    enchant: 5,
                    ..bag_item(1, "steel_longsword", 1)
                },
                bag_item(2, "apple", 3),
            ],
            ..Default::default()
        },
    );
    game.set_item_locked(&id, 1, true).await;
    game.equip_item(&id, 1).await;
    game.drop_item(&id, 1).await;
    assert!(game.get_player_inventory(&id).await.unwrap().equipped[&EquipSlot::MainHand].locked);
    game.unequip_item(&id, EquipSlot::MainHand).await;
    game.drop_items(
        &id,
        vec![
            onlinerpg_shared::messages::BagLineItem {
                instance_id: 2,
                qty: 2,
            },
            onlinerpg_shared::messages::BagLineItem {
                instance_id: 1,
                qty: 1,
            },
        ],
    )
    .await;
    let inv = game.get_player_inventory(&id).await.unwrap();
    assert_eq!(inv.bag.len(), 2);
    assert_eq!(
        inv.bag
            .iter()
            .find(|i| i.instance_id == 2)
            .unwrap()
            .quantity,
        3
    );
    assert!(game.ground_items.read().await.is_empty());
    game.set_item_locked(&id, 1, false).await;
    game.drop_item(&id, 1).await;
    let ground = game.ground_items.read().await;
    assert_eq!(ground.len(), 1);
    assert_eq!(ground.values().next().unwrap().item.enchant, 5);
}

#[tokio::test]
async fn item_locks_survive_save_and_reload_in_bag_and_equipment() {
    let game = make_test_game_state("item_lock_persistence");
    let auth = make_test_auth("item_lock_persistence");
    let account = auth.login_npc("npc_item_lock").unwrap();
    let record = create_test_character(&auth, &account, "Lockkeeper");
    let id = pid("Lockkeeper");
    game.add_player(make_player("Lockkeeper", 0.0, 0.0)).await;
    game.register_player_character(&id, record.id, 0, attrs_with_cha(12), 0, None)
        .await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![
                bag_item(1, "steel_longsword", 1),
                bag_item(2, "apple", 3),
                bag_item(3, "apple", 2),
            ],
            ..Default::default()
        },
    );
    game.set_item_locked(&id, 1, true).await;
    game.set_item_locked(&id, 2, true).await;
    game.equip_item(&id, 1).await;
    game.flush_dirty_saves(&auth).await;
    game.take_player_inventory(&id).await.unwrap();
    game.load_player_inventory(&id, record.id, &auth).await;
    let inv = game.get_player_inventory(&id).await.unwrap();
    assert!(inv.equipped[&EquipSlot::MainHand].locked);
    assert_eq!(inv.bag.len(), 2);
    assert_eq!(inv.bag.iter().find(|i| i.locked).unwrap().quantity, 3);
    assert_eq!(inv.bag.iter().find(|i| !i.locked).unwrap().quantity, 2);
    assert!(auth
        .load_inventory(record.id)
        .unwrap()
        .iter()
        .any(|row| row.locked));
}

/// A non-stackable line never collapses into one multi-unit slot, and the
/// returned count is what batch callers advance their reserved range by.
#[test]
fn stack_into_bag_unfolds_non_stackable_quantities_into_one_slot_each() {
    let mut bag = Vec::new();
    assert_eq!(insert(&mut bag, false, "torch", 0, 100, 3), 3);
    assert_eq!(bag.len(), 3);
    assert!(bag.iter().all(|i| i.quantity == 1));
    assert_eq!(
        bag.iter().map(|i| i.instance_id).collect::<Vec<_>>(),
        vec![100, 101, 102]
    );

    assert_eq!(
        insert(&mut bag, true, "apple", 0, 200, 4),
        1,
        "a new stack takes one id"
    );
    assert_eq!(
        insert(&mut bag, true, "apple", 0, 201, 4),
        0,
        "a merge takes none"
    );
    assert_eq!(
        insert(&mut bag, true, "apple", 0, 202, 0),
        0,
        "an empty line is a no-op"
    );
    let apples: Vec<_> = bag.iter().filter(|i| i.item_def_id == "apple").collect();
    assert_eq!(apples.len(), 1);
    assert_eq!(apples[0].quantity, 8);
}

/// Repeated grants merge stackable items.
#[tokio::test]
async fn give_item_stacks_repeated_grants() {
    let game_state = make_test_game_state("give_item_stacks");
    game_state.add_player(make_player("eater", 0.0, 0.0)).await;
    {
        let mut inventories = game_state.inventories.write().await;
        inventories.insert(pid("eater"), Default::default());
    }

    assert!(game_state.give_item(&pid("eater"), "grilled_minnow").await);
    assert!(game_state.give_item(&pid("eater"), "grilled_minnow").await);
    assert!(game_state.give_item(&pid("eater"), "fishing_rod").await);
    assert!(game_state.give_item(&pid("eater"), "fishing_rod").await);

    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("eater")].bag;
    let fish: Vec<_> = bag
        .iter()
        .filter(|i| i.item_def_id == "grilled_minnow")
        .collect();
    assert_eq!(fish.len(), 1);
    assert_eq!(fish[0].quantity, 2);
    let rods: Vec<_> = bag
        .iter()
        .filter(|i| i.item_def_id == "fishing_rod")
        .collect();
    assert_eq!(rods.len(), 2, "non-stackables keep their own slots");
}

#[tokio::test]
async fn give_items_refuses_stack_overflow_without_a_partial_grant() {
    let game_state = make_test_game_state("give_stack_overflow");
    let player_id = pid("archer");
    game_state.add_player(make_player("archer", 0.0, 0.0)).await;
    let mut inventory: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inventory
        .bag
        .push(bag_item(10, "iron_arrow", u32::MAX - 50));
    game_state
        .inventories
        .write()
        .await
        .insert(player_id, inventory);

    assert!(game_state
        .give_items(&player_id, "iron_arrow", 100)
        .await
        .is_err());
    assert_eq!(
        game_state.inventories.read().await[&player_id].bag[0].quantity,
        u32::MAX - 50
    );
    assert!(!game_state
        .dirty_inventories
        .read()
        .await
        .contains(&player_id));
    assert!(game_state
        .give_items(&player_id, "iron_arrow", 50)
        .await
        .is_ok());
    assert_eq!(
        game_state.inventories.read().await[&player_id].bag[0].quantity,
        u32::MAX
    );
    assert!(!game_state.give_item(&player_id, "iron_arrow").await);
}

/// Dropping from a stack must shed exactly one unit — before the stacking fix
/// a whole entry was removed and every unit above the first was destroyed.
#[tokio::test]
async fn dropping_from_a_stack_sheds_one_unit() {
    let game_state = make_test_game_state("drop_stack_unit");
    game_state
        .add_player(make_player("dropper", 0.0, 0.0))
        .await;
    {
        let mut inventories = game_state.inventories.write().await;
        let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
        inv.bag.push(bag_item(11, "apple", 3));
        inventories.insert(pid("dropper"), inv);
    }

    game_state.drop_item(&pid("dropper"), 11).await;

    {
        let inventories = game_state.inventories.read().await;
        let bag = &inventories[&pid("dropper")].bag;
        assert_eq!(bag.len(), 1, "the stack must survive the drop");
        assert_eq!(bag[0].quantity, 2);
        assert_eq!(bag[0].instance_id, 11, "the stack keeps its id");
    }
    {
        let ground_items = game_state.ground_items.read().await;
        assert_eq!(ground_items.len(), 1, "exactly one unit hits the ground");
        let dropped = ground_items.values().next().unwrap();
        assert_eq!(dropped.item.item_def_id, "apple");
        assert_ne!(
            dropped.item.instance_id, 11,
            "the shed unit needs its own id"
        );
    }

    // Draining the stack unit by unit ends with an empty bag, nothing lost.
    game_state.drop_item(&pid("dropper"), 11).await;
    game_state.drop_item(&pid("dropper"), 11).await;
    assert!(game_state.inventories.read().await[&pid("dropper")]
        .bag
        .is_empty());
    assert_eq!(game_state.ground_items.read().await.len(), 3);
}

// --- Two-handed weapons (doc/COMBAT.md 원거리 전투) ---

/// A player carrying a bow and a shield in the bag, plus a direct channel for
/// the refusal message.
async fn setup_two_hand_wielder(game_state: &GameState) -> DirectRx {
    game_state
        .add_player(make_player("wielder", 0.0, 0.0))
        .await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "bow", 1));
    inv.bag.push(bag_item(2, "wooden_shield", 1));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("wielder"), inv);
    game_state.register_direct_channel(&pid("wielder")).await
}

#[tokio::test]
async fn great_sword_clears_and_blocks_off_hand_until_replaced() {
    let game_state = make_test_game_state("great_sword_off_hand");
    let _rx = setup_two_hand_wielder(&game_state).await;
    {
        let mut inventories = game_state.inventories.write().await;
        let inv = inventories.get_mut(&pid("wielder")).unwrap();
        inv.bag[0].item_def_id = "great_sword".into();
        inv.bag.push(bag_item(3, "iron_sword", 1));
        inv.bag.push(bag_item(4, "torch", 1));
    }
    game_state.equip_item(&pid("wielder"), 2).await;
    game_state.equip_item(&pid("wielder"), 1).await;
    game_state.equip_item(&pid("wielder"), 2).await;
    game_state.equip_item(&pid("wielder"), 4).await;
    {
        let inventories = game_state.inventories.read().await;
        let inv = &inventories[&pid("wielder")];
        assert_eq!(
            inv.equipped[&EquipSlot::MainHand].item_def_id,
            "great_sword"
        );
        assert!(!inv.equipped.contains_key(&EquipSlot::OffHand));
        assert!(inv.bag.iter().any(|item| item.instance_id == 2));
        assert!(inv.bag.iter().any(|item| item.instance_id == 4));
    }
    game_state.equip_item(&pid("wielder"), 3).await;
    game_state.equip_item(&pid("wielder"), 2).await;
    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("wielder")];
    assert_eq!(
        inv.equipped[&EquipSlot::OffHand].item_def_id,
        "wooden_shield"
    );
    assert!(inv.bag.iter().any(|item| item.item_def_id == "great_sword"));
}

#[tokio::test]
async fn equipping_a_two_hander_empties_the_off_hand() {
    let game_state = make_test_game_state("two_hand_clears_off_hand");
    let _rx = setup_two_hand_wielder(&game_state).await;

    game_state.equip_item(&pid("wielder"), 2).await;
    game_state.equip_item(&pid("wielder"), 1).await;

    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("wielder")];
    assert_eq!(
        inv.equipped_def_id(EquipSlot::MainHand).as_deref(),
        Some("bow")
    );
    assert!(
        !inv.equipped.contains_key(&EquipSlot::OffHand),
        "the bow must claim the shield's hand"
    );
    assert!(
        inv.bag.iter().any(|i| i.item_def_id == "wooden_shield"),
        "the displaced shield goes back to the bag, not nowhere"
    );
}

#[tokio::test]
async fn an_off_hand_equip_is_refused_under_a_two_hander() {
    let game_state = make_test_game_state("two_hand_blocks_off_hand");
    let _rx = setup_two_hand_wielder(&game_state).await;

    game_state.equip_item(&pid("wielder"), 1).await;
    game_state.equip_item(&pid("wielder"), 2).await;

    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("wielder")];
    assert!(
        !inv.equipped.contains_key(&EquipSlot::OffHand),
        "no off-hand item may join a two-hander"
    );
    assert!(inv.bag.iter().any(|i| i.item_def_id == "wooden_shield"));
}

/// A one-handed weapon leaves the off-hand alone, so the rule costs today's
/// sword-and-shield nothing.
#[tokio::test]
async fn a_one_handed_weapon_keeps_the_off_hand() {
    let game_state = make_test_game_state("one_hand_keeps_off_hand");
    game_state
        .add_player(make_player("wielder", 0.0, 0.0))
        .await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "iron_sword", 1));
    inv.bag.push(bag_item(2, "wooden_shield", 1));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("wielder"), inv);

    game_state.equip_item(&pid("wielder"), 1).await;
    game_state.equip_item(&pid("wielder"), 2).await;

    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("wielder")];
    assert_eq!(
        inv.equipped_def_id(EquipSlot::OffHand).as_deref(),
        Some("wooden_shield")
    );
}
