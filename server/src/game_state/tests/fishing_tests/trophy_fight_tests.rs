use super::*;
use crate::game_state::fishing::{FishingPhase, RolledFish};

async fn hook_trout(game_state: &GameState, id: &PlayerId, rx: &mut DirectRx, trophy: bool) {
    game_state.start_fishing(id, water_target()).await;
    advance_until_bite(game_state, rx).await;
    game_state
        .fishing_sessions
        .write()
        .await
        .get_mut(id)
        .unwrap()
        .rolled_fish = Some(RolledFish {
        item_def_id: "raw_trout".into(),
        rarity: 3,
        size_cm: if trophy { 60 } else { 30 },
        trophy,
    });
    game_state.respond_fishing(id, FishingAction::Hook).await;
}

#[tokio::test(start_paused = true)]
async fn trophy_is_announced_before_play_and_awards_one_distinct_fish() {
    let game_state = make_test_game_state("trophy_landed");
    let (id, mut rx) = make_angler(&game_state, "trophy_angler").await;
    hook_trout(&game_state, &id, &mut rx, true).await;
    let (outcome, messages) =
        flow_tests::fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(
        matches!(outcome, FishingOutcome::Caught { ref item_def_id, size_cm: 60, trophy: true } if item_def_id == "trophy_raw_trout"),
        "{outcome:?}"
    );
    let beats: Vec<_> = messages
        .iter()
        .filter_map(|m| match m {
            ServerMessage::FishingFight {
                trophy,
                stamina_pct,
                ..
            } => Some((*trophy, *stamina_pct)),
            _ => None,
        })
        .collect();
    assert_eq!(beats.first(), Some(&(true, 100)));
    assert!(beats.iter().all(|(trophy, _)| *trophy));
    assert!(beats.iter().any(|(_, stamina)| *stamina == 0));
    assert!(!game_state.dirty_skills.read().await.contains(&id));
    let inv = game_state.get_player_inventory(&id).await.unwrap();
    assert_eq!(inv.bag.len(), 1);
    assert_eq!(inv.bag[0].item_def_id, "trophy_raw_trout");
    assert_eq!(inv.bag[0].quantity, 1);
}

#[tokio::test(start_paused = true)]
async fn cautious_play_cannot_land_a_trophy_or_award_an_ordinary_fish() {
    let game_state = make_test_game_state("trophy_escaped");
    let (id, mut rx) = make_angler(&game_state, "cautious_angler").await;
    hook_trout(&game_state, &id, &mut rx, true).await;
    let (outcome, _) =
        flow_tests::fight_to_the_end(&game_state, &id, &mut rx, |state, tension, _| {
            auto_stance(state, tension, false)
        })
        .await;
    assert_eq!(outcome, FishingOutcome::Escaped);
    assert!(game_state
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .is_empty());
}

#[tokio::test(start_paused = true)]
async fn ordinary_fish_keep_the_existing_fight_and_reward() {
    let game_state = make_test_game_state("ordinary_landed");
    let (id, mut rx) = make_angler(&game_state, "ordinary_angler").await;
    hook_trout(&game_state, &id, &mut rx, false).await;
    let (outcome, _) = flow_tests::fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(
        matches!(outcome, FishingOutcome::Caught { ref item_def_id, trophy: false, .. } if item_def_id == "raw_trout")
    );
    assert_eq!(
        game_state.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        1
    );
}

#[tokio::test(start_paused = true)]
async fn a_snapped_trophy_line_awards_nothing() {
    let game_state = make_test_game_state("trophy_snapped");
    let (id, mut rx) = make_angler(&game_state, "snapped_angler").await;
    hook_trout(&game_state, &id, &mut rx, true).await;
    let (outcome, _) =
        flow_tests::fight_to_the_end(&game_state, &id, &mut rx, |_, _, _| FishingAction::Reel)
            .await;
    assert_eq!(outcome, FishingOutcome::Escaped);
    assert!(game_state
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .is_empty());
}

#[tokio::test(start_paused = true)]
async fn an_overweight_trophy_lands_on_the_ground_as_one_fish() {
    let game_state = make_test_game_state("trophy_overweight");
    let (id, mut rx) = make_angler(&game_state, "overweight_angler").await;
    hook_trout(&game_state, &id, &mut rx, true).await;
    {
        let mut sessions = game_state.fishing_sessions.write().await;
        let session = sessions.get_mut(&id).unwrap();
        let FishingPhase::Fight { state, .. } = &mut session.phase else {
            panic!()
        };
        state.stamina = 0.0;
        state.distance = state.min_distance;
    }
    game_state
        .inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(900, "iron_sword", 100));
    drain(&mut rx);
    game_state.respond_fishing(&id, FishingAction::Reel).await;
    let (outcome, messages) =
        flow_tests::fight_to_the_end(&game_state, &id, &mut rx, auto_stance).await;
    assert!(matches!(
        outcome,
        FishingOutcome::Caught { trophy: true, .. }
    ));
    assert!(messages.iter().any(|m| matches!(m, ServerMessage::GroundItemSpawned { item, .. } if item.item_def_id == "trophy_raw_trout" && item.quantity == 1)));
}

#[tokio::test]
async fn trophy_stacks_are_separate_from_ordinary_fish() {
    let game_state = make_test_game_state("trophy_stacks");
    let (id, _rx) = make_angler(&game_state, "trophy_sorter").await;
    game_state.award_item(&id, "raw_trout").await;
    game_state.award_item(&id, "trophy_raw_trout").await;
    game_state.award_item(&id, "trophy_raw_trout").await;
    let inv = game_state.get_player_inventory(&id).await.unwrap();
    assert_eq!(inv.bag.len(), 2);
    assert!(inv
        .bag
        .iter()
        .any(|i| i.item_def_id == "raw_trout" && i.quantity == 1));
    assert!(inv
        .bag
        .iter()
        .any(|i| i.item_def_id == "trophy_raw_trout" && i.quantity == 2));
}

#[test]
fn every_fishable_species_has_a_more_valuable_trophy_variant() {
    let defs = ItemDefs::load();
    for candidate in defs.catch_table() {
        let ordinary = defs.get(&candidate.item_def_id).unwrap();
        if !ordinary.is_fish() {
            continue;
        }
        let trophy = defs
            .get(&format!("trophy_{}", candidate.item_def_id))
            .unwrap();
        assert!(trophy.is_fish());
        assert!(trophy.stackable);
        assert_eq!(trophy.base_price.unwrap(), ordinary.base_price.unwrap() * 3);
        assert_eq!(trophy.weight, ordinary.weight * 2.0);
        assert_eq!(trophy.grills_into, ordinary.grills_into);
        assert!(trophy.catch_weight.is_none());
    }
}

#[tokio::test(start_paused = true)]
async fn loose_trophy_hook_can_escape_after_one_second_without_a_reward() {
    let game_state = make_test_game_state("trophy_hook_slip");
    let (id, mut rx) = make_angler(&game_state, "loose_angler").await;
    hook_trout(&game_state, &id, &mut rx, true).await;
    *game_state.fishing_hook_roll.lock().unwrap() = 0.0;
    {
        let mut sessions = game_state.fishing_sessions.write().await;
        let FishingPhase::Fight { state, .. } = &mut sessions.get_mut(&id).unwrap().phase else {
            panic!()
        };
        state.pressure_established = true;
        state.tension = 30.0;
        state.state_ms_left = 10_000.0;
        state.stance = FishingAction::GiveLine;
    }
    drain(&mut rx);
    for tick in 1..=4 {
        advance(Duration::from_millis(250)).await;
        game_state.tick_fishing(None).await;
        let escaped = drain(&mut rx).iter().any(|msg| {
            matches!(
                msg,
                ServerMessage::FishingEnded {
                    outcome: FishingOutcome::Escaped,
                    ..
                }
            )
        });
        assert_eq!(escaped, tick == 4);
    }
    assert!(game_state
        .get_player_inventory(&id)
        .await
        .unwrap()
        .bag
        .is_empty());
}
