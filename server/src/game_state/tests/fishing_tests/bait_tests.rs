use super::*;
use crate::game_state::fishing::{baited_weights, BaitEffect, CatchCandidate};
use onlinerpg_shared::fishing::{BAIT_FLOTSAM_SHARE_PCT, FLOTSAM_SHARE_PCT};

fn candidate(id: &str, rarity: u32, catch_weight: u32) -> CatchCandidate {
    CatchCandidate {
        item_def_id: id.into(),
        rarity,
        catch_weight,
        min_fishing_level: 0,
    }
}

fn share_of(weights: &[u64], pick: impl Fn(usize) -> bool) -> f64 {
    let total: u64 = weights.iter().sum();
    let part: u64 = weights
        .iter()
        .enumerate()
        .filter(|(i, _)| pick(*i))
        .map(|(_, w)| *w)
        .sum();
    part as f64 / total as f64
}

/// The pure weighting: bait multiplies only its rarity window, inside the
/// fish pool, and trims flotsam to its baited share.
#[test]
fn bait_boosts_its_window_and_trims_flotsam() {
    let table = vec![
        candidate("minnow", 1, 50),
        candidate("trout", 3, 14),
        candidate("sturgeon", 5, 1),
        candidate("boot", 0, 10),
    ];
    let bare = baited_weights(&table, 0, None);
    let shrimp = baited_weights(
        &table,
        0,
        Some(BaitEffect {
            rarity_min: 3,
            rarity_max: 5,
            boost_pct: 200,
        }),
    );

    let is_junk = |i: usize| table[i].rarity == 0;
    assert!((share_of(&bare, is_junk) - FLOTSAM_SHARE_PCT as f64 / 100.0).abs() < 1e-9);
    assert!((share_of(&shrimp, is_junk) - BAIT_FLOTSAM_SHARE_PCT as f64 / 100.0).abs() < 1e-9);

    // Inside the fish pool, trout and sturgeon doubled against the minnow.
    let fish_ratio = |w: &[u64], i: usize| w[i] as f64 / w[0] as f64;
    assert!((fish_ratio(&shrimp, 1) - 2.0 * fish_ratio(&bare, 1)).abs() < 1e-9);
    assert!((fish_ratio(&shrimp, 2) - 2.0 * fish_ratio(&bare, 2)).abs() < 1e-9);
}

#[test]
fn bait_outside_its_window_changes_nothing_but_flotsam() {
    let table = vec![candidate("sturgeon", 5, 1), candidate("boot", 0, 10)];
    let crumbs = baited_weights(
        &table,
        0,
        Some(BaitEffect {
            rarity_min: 1,
            rarity_max: 2,
            boost_pct: 200,
        }),
    );
    assert!((share_of(&crumbs, |i| i == 0) - 0.9).abs() < 1e-9);
}

/// The three shipped baits are usable, stackable, sold by Rica, and carry a
/// sane window each.
#[test]
fn shipped_baits_are_well_formed_and_for_sale() {
    let defs = ItemDefs::load();
    for (id, min, max) in [("breadcrumbs", 1, 2), ("worm", 1, 3), ("shrimp", 3, 5)] {
        let def = defs.get(id).expect(id);
        assert!(def.is_bait() && def.stackable && def.consumable, "{id}");
        let bait = def.bait_effect().expect("bait effect");
        assert_eq!((bait.rarity_min, bait.rarity_max), (min, max), "{id}");
        assert!(bait.boost_pct > 100, "{id} must actually help");
        assert!(def.base_price.is_some(), "{id} must be buyable");
    }
    let rica = crate::merchant_defs::merchant_defs()
        .get_by_npc_name("Rica")
        .expect("rica");
    for id in ["breadcrumbs", "worm", "shrimp"] {
        assert!(rica.catalog.iter().any(|c| c == id), "Rica sells {id}");
    }
}

async fn armed_bait_of(game_state: &GameState, id: &PlayerId) -> Option<String> {
    game_state.armed_bait.read().await.get(id).cloned()
}

async fn bag_qty(game_state: &GameState, id: &PlayerId, def_id: &str) -> u32 {
    game_state
        .get_player_inventory(id)
        .await
        .unwrap()
        .bag
        .iter()
        .filter(|i| i.item_def_id == def_id)
        .map(|i| i.quantity)
        .sum()
}

/// Using a bait arms the hook and spends nothing; each cast then takes one.
#[tokio::test(start_paused = true)]
async fn using_bait_arms_the_hook_and_casts_spend_it() {
    let game_state = make_test_game_state("fishing_bait_spend");
    let (id, mut rx) = make_angler(&game_state, "angler_baiter").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(500, "worm", 2));

    game_state.use_item(&id, 500).await;
    assert_eq!(
        armed_bait_of(&game_state, &id).await.as_deref(),
        Some("worm")
    );
    assert_eq!(
        bag_qty(&game_state, &id, "worm").await,
        2,
        "arming spends nothing"
    );

    game_state.start_fishing(&id, water_target()).await;
    assert_eq!(bag_qty(&game_state, &id, "worm").await, 1);
    assert!(
        game_state
            .fishing_sessions
            .read()
            .await
            .get(&id)
            .unwrap()
            .bait
            .is_some(),
        "the cast carries the bait into the bite roll"
    );

    game_state.stop_fishing(&id).await;
    game_state.start_fishing(&id, water_target()).await;
    assert_eq!(bag_qty(&game_state, &id, "worm").await, 0);
    game_state.stop_fishing(&id).await;

    // The pile is gone: the next cast is bare, the hook disarmed, the angler told.
    let _ = drain(&mut rx);
    game_state.start_fishing(&id, water_target()).await;
    assert!(game_state
        .fishing_sessions
        .read()
        .await
        .get(&id)
        .unwrap()
        .bait
        .is_none());
    assert_eq!(armed_bait_of(&game_state, &id).await, None);
    assert!(
        drain(&mut rx).iter().any(|m| matches!(
            m,
            ServerMessage::SystemMessage { message } if message.contains("bare hook")
        )),
        "the angler is told the hook went bare"
    );
}

/// A cast with nothing armed is the old bare cast.
#[tokio::test(start_paused = true)]
async fn a_bare_cast_carries_no_bait() {
    let game_state = make_test_game_state("fishing_bait_bare");
    let (id, _rx) = make_angler(&game_state, "angler_bare").await;
    game_state.start_fishing(&id, water_target()).await;
    assert!(game_state
        .fishing_sessions
        .read()
        .await
        .get(&id)
        .unwrap()
        .bait
        .is_none());
}

/// Only bait goes on the hook: using something else never arms anything.
#[tokio::test(start_paused = true)]
async fn only_bait_can_be_armed() {
    let game_state = make_test_game_state("fishing_bait_only");
    let (id, _rx) = make_angler(&game_state, "angler_notbait").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(501, "bread", 1));
    game_state.use_item(&id, 501).await;
    assert_eq!(armed_bait_of(&game_state, &id).await, None);
}

/// The armed bait is per player and goes with the player.
#[tokio::test(start_paused = true)]
async fn leaving_forgets_the_armed_bait() {
    let game_state = make_test_game_state("fishing_bait_leave");
    let (id, _rx) = make_angler(&game_state, "angler_leaver").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(502, "shrimp", 1));
    game_state.use_item(&id, 502).await;
    assert_eq!(
        armed_bait_of(&game_state, &id).await.as_deref(),
        Some("shrimp")
    );
    game_state.remove_player(&id).await;
    assert_eq!(armed_bait_of(&game_state, &id).await, None);
}
