use super::*;

/// An overweight bag spills the catch as a ground item — never silently lost.
#[tokio::test]
async fn a_full_bag_spills_the_catch_on_the_ground() {
    let game_state = make_test_game_state("fishing_spill");
    let (id, mut rx) = make_angler(&game_state, "angler_hoarder").await;
    // 150 torches = the full STR-10 carry allowance before the catch.
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(700, "torch", 150));

    game_state.award_item(&id, "raw_minnow").await;

    assert!(
        drain(&mut rx).iter().any(|m| matches!(
            m,
            ServerMessage::GroundItemSpawned { item, .. } if item.item_def_id == "raw_minnow"
        )),
        "the overflow catch must spill to the ground"
    );
    assert!(
        game_state
            .inventories
            .read()
            .await
            .get(&id)
            .unwrap()
            .bag
            .iter()
            .all(|i| i.item_def_id != "raw_minnow"),
        "nothing was bagged"
    );
    assert!(
        game_state
            .ground_items
            .read()
            .await
            .values()
            .any(|g| g.item.item_def_id == "raw_minnow"),
        "the fish is on the ground, pickable"
    );
}

#[tokio::test]
async fn eating_a_fish_regenerates_hp_from_its_nutrition() {
    let game_state = make_test_game_state("fishing_eat");
    let (id, mut rx) = make_angler(&game_state, "angler_hungry").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(800, "raw_trout", 1));
    game_state
        .players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .health = 2;

    game_state.use_item(&id, 800).await;
    assert_eq!(game_state.players.read().await[&id].health, 2);
    for _ in 0..10 {
        game_state.tick_food_regeneration().await;
    }

    let health = game_state.players.read().await.get(&id).unwrap().health;
    assert_eq!(health, 4, "nutrition 40 restores 2 HP over ten ticks");
    assert!(
        game_state
            .inventories
            .read()
            .await
            .get(&id)
            .unwrap()
            .bag
            .iter()
            .all(|i| i.item_def_id != "raw_trout"),
        "the fish was eaten"
    );
    let _ = drain(&mut rx);
}

async fn bag_of(game_state: &GameState, id: &PlayerId) -> Vec<(String, u32)> {
    let mut bag: Vec<(String, u32)> = game_state
        .get_player_inventory(id)
        .await
        .unwrap()
        .bag
        .into_iter()
        .map(|item| (item.item_def_id, item.quantity))
        .collect();
    bag.sort();
    bag
}

// Direct awards avoid random species rolls.
#[tokio::test]
async fn catches_stack_per_species() {
    let game_state = make_test_game_state("fishing_stack_species");
    let (id, _rx) = make_angler(&game_state, "angler_sorter").await;

    for _ in 0..3 {
        game_state.award_item(&id, "raw_trout").await;
    }
    game_state.award_item(&id, "raw_minnow").await;

    assert_eq!(
        bag_of(&game_state, &id).await,
        vec![("raw_minnow".to_string(), 1), ("raw_trout".to_string(), 3)]
    );
}

// Joining a stack preserves its instance ID.
#[tokio::test]
async fn a_catch_joins_the_existing_pile() {
    let game_state = make_test_game_state("fishing_stack_join");
    let (id, _rx) = make_angler(&game_state, "angler_joiner").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(900, "raw_perch", 2));

    game_state.award_item(&id, "raw_perch").await;

    let inv = game_state.get_player_inventory(&id).await.unwrap();
    assert_eq!(inv.bag, vec![bag_item(900, "raw_perch", 3)]);
}

#[tokio::test]
async fn eating_one_fish_from_a_pile_leaves_the_rest() {
    let game_state = make_test_game_state("fishing_stack_eat");
    let (id, _rx) = make_angler(&game_state, "angler_snacker").await;
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(910, "raw_trout", 3));

    game_state.use_item(&id, 910).await;

    assert_eq!(
        bag_of(&game_state, &id).await,
        vec![("raw_trout".to_string(), 2)]
    );
}

#[test]
fn pick_catch_maps_every_roll_and_preserves_species_and_flotsam_weights() {
    use crate::game_state::fishing::{effective_weights, pick_catch, CatchCandidate};
    use onlinerpg_shared::fishing::FLOTSAM_SHARE_PCT;

    let candidate = |id: &str, rarity, catch_weight| CatchCandidate {
        item_def_id: id.into(),
        rarity,
        catch_weight,
    };
    let candidates = vec![
        candidate("common", 1, 130),
        candidate("rare", 5, 5),
        candidate("junk", 0, 8),
    ];
    let weights = effective_weights(&candidates);
    let total: u64 = weights.iter().sum();
    for roll in 0..total {
        assert!(pick_catch(&weights, roll).unwrap() < candidates.len());
    }
    assert_eq!(pick_catch(&weights, total), None);
    assert_eq!(pick_catch(&weights, 0), Some(0));
    assert_eq!(pick_catch(&weights, weights[0] - 1), Some(0));
    assert_eq!(pick_catch(&weights, weights[0]), Some(1));
    assert_eq!(pick_catch(&weights, weights[0] + weights[1]), Some(2));
    assert_eq!(weights[0], weights[1] * 26);
    assert_eq!(weights[2] * 100, total * FLOTSAM_SHARE_PCT);
}

/// Every drop landed on the player's exact position, so a bagful of fish
/// became one sprite underfoot and read as "nothing dropped".
#[tokio::test(start_paused = true)]
async fn dropped_items_scatter_instead_of_stacking() {
    let game_state = make_test_game_state("drop_scatter");
    let (id, _rx) = make_angler(&game_state, "angler_litterbug").await;
    for instance in 600u64..606 {
        game_state
            .inventories
            .write()
            .await
            .get_mut(&id)
            .unwrap()
            .bag
            .push(bag_item(instance, "raw_minnow", 1));
    }

    for instance in 600u64..606 {
        game_state.drop_item(&id, instance).await;
    }

    let dropped: Vec<_> = game_state
        .ground_items
        .read()
        .await
        .values()
        .filter(|g| g.item.item_def_id == "raw_minnow")
        .map(|g| (g.item.position.x, g.item.position.z))
        .collect::<Vec<(f32, f32)>>();
    assert_eq!(dropped.len(), 6, "every drop should reach the ground");
    for (i, a) in dropped.iter().enumerate() {
        for b in dropped.iter().skip(i + 1) {
            assert!(
                (a.0 - b.0).abs() > f32::EPSILON || (a.1 - b.1).abs() > f32::EPSILON,
                "two drops landed on the same spot"
            );
        }
    }
    let player = game_state.players.read().await.get(&id).unwrap().position;
    for (x, z) in &dropped {
        let reach = ((x - player.x).powi(2) + (z - player.z).powi(2)).sqrt();
        assert!(reach <= 2.0, "a drop landed {reach:.2}m away, out of reach");
    }
}
