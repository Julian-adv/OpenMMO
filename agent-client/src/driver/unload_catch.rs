//! A fishing NPC's catch piles up in its own bag, and past the carry limit
//! every catch slips to the ground for passers-by. Before that happens, open
//! the coin pouches, drop junk no merchant buys, and sell the rest to Rica.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use onlinerpg_shared::inventory::ItemInstance;
use onlinerpg_shared::messages::BagLineItem;
use onlinerpg_shared::ClientMessage;
use tokio::sync::Mutex;
use tracing::{error, info};

use super::combat::{approach_player, ChaseResult};
use crate::item_defs;
use crate::state::SharedState;

pub(crate) const BUYER: &str = "Rica";
const FULL_FRACTION: f32 = 0.8;
/// One fish each for breakfast and dinner by the campfire.
const MEAL_FISH_KEPT: u32 = 2;
/// Wait before the next trip, so a sale the server refuses cannot loop.
pub(super) const RETRY: Duration = Duration::from_secs(10 * 60);

pub(super) fn is_due(s: &SharedState) -> bool {
    s.catch_slipped
        || s.carry_load()
            .is_some_and(|(carried, limit)| carried >= limit * FULL_FRACTION)
}

#[derive(Debug, Default, PartialEq)]
struct Plan {
    open: Vec<u64>,
    drop: Vec<BagLineItem>,
    sell: BTreeMap<String, Vec<BagLineItem>>,
}

fn plan(bag: &[ItemInstance]) -> Plan {
    let mut plan = Plan::default();
    let mut sellable: Vec<(&ItemInstance, bool, i64)> = Vec::new();
    for item in bag.iter().filter(|i| i.quantity > 0 && !i.locked) {
        let Some(def) = item_defs::get(&item.item_def_id) else {
            continue;
        };
        let line = BagLineItem {
            instance_id: item.instance_id,
            qty: item.quantity,
        };
        match (def.category.as_deref(), def.base_price) {
            (Some("coin_catch"), _) => plan.open.push(item.instance_id),
            (Some("junk"), None) => plan.drop.push(line),
            (Some("junk"), Some(price)) => sellable.push((item, false, price)),
            (Some("fish"), Some(price)) => sellable.push((item, def.grills_into.is_some(), price)),
            _ => {}
        }
    }
    sellable.sort_by_key(|(_, grillable, price)| (!grillable, *price));
    let mut keep = MEAL_FISH_KEPT;
    for (item, grillable, _) in sellable {
        let kept = if grillable {
            keep.min(item.quantity)
        } else {
            0
        };
        keep -= kept;
        if item.quantity > kept {
            plan.sell
                .entry(item.item_def_id.clone())
                .or_default()
                .push(BagLineItem {
                    instance_id: item.instance_id,
                    qty: item.quantity - kept,
                });
        }
    }
    plan
}

/// One trip; the schedule walks us back to the riverbank afterwards.
pub(super) async fn run(state: Arc<Mutex<SharedState>>, label: String) {
    let buyer = {
        let s = state.lock().await;
        if s.trade_busy {
            return;
        }
        match s.resolve_nearby_player(BUYER) {
            Some((id, true)) => id,
            _ => {
                info!("[{label}] Bag nearly full but {BUYER} is not nearby — fishing on");
                return;
            }
        }
    };
    let sell = {
        let mut s = state.lock().await;
        let Plan { open, drop, sell } = plan(&s.self_bag);
        let mut commands: Vec<ClientMessage> = Vec::new();
        if s.self_fishing {
            commands.push(ClientMessage::FishingStop);
        }
        commands.extend(
            open.into_iter()
                .map(|instance_id| ClientMessage::UseItem { instance_id }),
        );
        if !drop.is_empty() {
            commands.push(ClientMessage::DropItems { items: drop });
        }
        for command in commands {
            if let Err(e) = s.send_background_command(command).await {
                error!("[{label}] Failed to clear the catch: {e}");
                return;
            }
        }
        s.catch_slipped = false;
        sell
    };
    if sell.is_empty() {
        return;
    }

    info!("[{label}] Bag nearly full — taking the catch to {BUYER}");
    if !matches!(
        approach_player(&state, &buyer, None).await,
        ChaseResult::InRange
    ) {
        info!("[{label}] Could not reach {BUYER} to sell the catch");
        return;
    }
    let mut s = state.lock().await;
    let mut sold = Vec::new();
    for (def_id, items) in sell {
        let units: u32 = items.iter().map(|l| l.qty).sum();
        let command = ClientMessage::SellItems {
            merchant_player_id: buyer,
            items,
        };
        match s.send_background_command(command).await {
            Ok(()) => sold.push(format!("{units}x {def_id}")),
            Err(e) => error!("[{label}] Failed to sell {def_id}: {e}"),
        }
    }
    let sold = sold.join(", ");
    info!("[{label}] Selling the catch to {BUYER}: {sold}");
    s.push_ambient_event_quiet(format!(
        "[Routine] Your bag was nearly full, so you sold your catch to {BUYER} ({sold}), \
         keeping a couple of fish for your meals. Your routine walks you back to the \
         riverbank."
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(instance_id: u64, id: &str, quantity: u32) -> ItemInstance {
        ItemInstance {
            instance_id,
            item_def_id: id.into(),
            quantity,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
            locked: false,
        }
    }

    fn line(instance_id: u64, qty: u32) -> BagLineItem {
        BagLineItem { instance_id, qty }
    }

    #[test]
    fn plan_opens_pouches_drops_junk_sells_the_rest_and_keeps_meal_fish() {
        let bag = vec![
            item(1, "fishing_rod", 1),
            item(2, "raw_perch", 4),
            item(3, "raw_minnow", 1),
            item(4, "trophy_raw_trout", 1),
            item(5, "sunken_coin_pouch", 1),
            item(6, "sunken_coin_pouch", 1),
            item(7, "old_boot", 1),
            item(8, "clump_of_kelp", 1),
            item(9, "message_in_a_bottle", 1),
            item(10, "grilled_perch", 1),
        ];
        let plan = plan(&bag);
        assert_eq!(plan.open, vec![5, 6]);
        assert_eq!(plan.drop, vec![line(7, 1), line(8, 1)]);
        assert_eq!(
            plan.sell,
            BTreeMap::from([
                ("message_in_a_bottle".to_string(), vec![line(9, 1)]),
                ("raw_perch".to_string(), vec![line(2, 3)]),
                ("trophy_raw_trout".to_string(), vec![line(4, 1)]),
            ]),
            "the minnow and one perch stay for the two meals"
        );
    }

    #[test]
    fn a_trip_is_due_near_the_carry_limit_or_after_a_catch_slipped() {
        use onlinerpg_shared::character::{Character, CharacterAttributes, CharacterClass};
        let (mut s, _rx) = crate::state::tests::test_state();
        s.characters.push(Character {
            id: 1,
            name: "Tobin".into(),
            created_at: 0,
            level: 1,
            xp: 0,
            max_hp: 10,
            attributes: CharacterAttributes {
                r#str: 10,
                dex: 10,
                con: 10,
                int: 10,
                wis: 10,
                cha: 10,
                guard: 0,
            },
            class: CharacterClass::Knight,
            gender: Default::default(),
            equipment: Default::default(),
            titles: Vec::new(),
            active_title: None,
        });
        s.self_bag = vec![item(1, "raw_trout", 119)];
        assert!(!is_due(&s), "119 of 150");
        s.self_bag = vec![item(1, "raw_trout", 120)];
        assert!(is_due(&s), "120 of 150");
        s.self_carry_mult = 2.0;
        assert!(!is_due(&s), "a fed angler carries more");
        s.catch_slipped = true;
        assert!(is_due(&s));
    }

    #[tokio::test]
    async fn without_the_buyer_nearby_the_trip_leaves_the_angler_fishing() {
        let (mut s, mut rx) = crate::state::tests::test_state();
        s.self_fishing = true;
        s.catch_slipped = true;
        s.self_bag = vec![item(1, "raw_trout", 50), item(2, "sunken_coin_pouch", 1)];
        let state = Arc::new(Mutex::new(s));
        run(Arc::clone(&state), "tobin".into()).await;
        assert!(rx.try_recv().is_err(), "no stop, open, drop or sale");
        let s = state.lock().await;
        assert!(s.self_fishing);
        assert!(s.catch_slipped, "the next trip still owes the unload");
    }

    #[test]
    fn plan_leaves_locked_items_alone() {
        let mut pouch = item(1, "sunken_coin_pouch", 1);
        pouch.locked = true;
        let mut trout = item(2, "raw_trout", 5);
        trout.locked = true;
        assert_eq!(plan(&[pouch, trout]), Plan::default());
    }
}
