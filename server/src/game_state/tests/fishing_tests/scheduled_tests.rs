use super::*;
use crate::game_state::fishing::FishingPhase;
use onlinerpg_shared::schedule::ScheduleEntry;

async fn scheduled_angler(game: &GameState) -> (PlayerId, DirectRx, ScheduleEntry) {
    let (id, rx) = make_angler(game, "Tobin").await;
    let entry = ScheduleEntry {
        at: "00:00".into(),
        pos: [-100.0, 0.0, 50.0],
        rotation: -89.0,
        action: Some("fishing".into()),
        ..Default::default()
    };
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.is_official_npc = true;
        player.rotation = entry.rotation.to_radians();
    }
    game.set_npc_schedule("Tobin", vec![entry.clone()]);
    (id, rx, entry)
}

#[tokio::test(start_paused = true)]
async fn a_scheduled_angler_only_has_a_line_after_casting_and_can_land_real_fish() {
    let game = make_test_game_state("scheduled_fishing_catch");
    let (id, mut rx, entry) = scheduled_angler(&game).await;
    advance(Duration::from_secs(60)).await;
    game.tick_fishing(None).await;
    assert!(game.fishing_sessions.read().await.is_empty());
    assert!(drain(&mut rx).is_empty());

    game.start_fishing(&id, entry.fishing_target().unwrap())
        .await;
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(message,
        ServerMessage::FishingCasted { rotation, .. }
            if (*rotation - entry.rotation.to_radians()).abs() < 0.001
    )));
    advance_until_bite(&game, &mut rx).await;
    {
        let mut sessions = game.fishing_sessions.write().await;
        let fish = sessions.get_mut(&id).unwrap().rolled_fish.as_mut().unwrap();
        fish.item_def_id = "raw_minnow".into();
        fish.rarity = 1;
        fish.size_cm = 10;
        fish.trophy = false;
    }
    game.respond_fishing(&id, FishingAction::Hook).await;
    let (outcome, messages) = flow_tests::fight_to_the_end(&game, &id, &mut rx, auto_stance).await;
    assert!(matches!(
        outcome,
        FishingOutcome::Caught { ref item_def_id, .. } if item_def_id == "raw_minnow"
    ));
    assert!(game.inventories.read().await[&id]
        .bag
        .iter()
        .any(|item| item.item_def_id == "raw_minnow"));
    assert!(messages
        .iter()
        .any(|message| matches!(message, ServerMessage::InventoryUpdated { .. })));
    assert!(!game.fishing_sessions.read().await.contains_key(&id));

    advance(Duration::from_secs(60)).await;
    game.tick_fishing(None).await;
    assert!(drain(&mut rx).is_empty());

    game.start_fishing(&id, entry.fishing_target().unwrap())
        .await;
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::FishingCasted { .. })));
    game.stop_fishing(&id).await;

    assert!(!game.fishing_sessions.read().await.contains_key(&id));
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Aborted,
            ..
        }
    )));
}

#[tokio::test(start_paused = true)]
async fn fishing_progresses_without_an_official_identity_or_matching_schedule() {
    for official_at_spot in [false, true] {
        let game = make_test_game_state("scheduled_fishing_guards");
        let (id, mut rx, mut entry) = scheduled_angler(&game).await;
        if official_at_spot {
            entry.pos[0] -= 10.0;
            game.set_npc_schedule("Tobin", vec![entry]);
        } else {
            game.players
                .write()
                .await
                .get_mut(&id)
                .unwrap()
                .is_official_npc = false;
        }
        game.start_fishing(&id, water_target()).await;
        advance_until_bite(&game, &mut rx).await;
        assert!(matches!(
            game.fishing_sessions.read().await[&id].phase,
            FishingPhase::Bite { .. }
        ));
    }
}

#[tokio::test(start_paused = true)]
async fn scheduled_fishing_stops_when_the_rod_is_put_away() {
    let game = make_test_game_state("scheduled_fishing_unequip");
    let (id, mut rx, entry) = scheduled_angler(&game).await;
    game.start_fishing(&id, entry.fishing_target().unwrap())
        .await;
    drain(&mut rx);
    game.unequip_item(&id, EquipSlot::MainHand).await;
    assert!(!game.fishing_sessions.read().await.contains_key(&id));
    assert!(drain(&mut rx).iter().any(|message| matches!(
        message,
        ServerMessage::FishingEnded {
            outcome: FishingOutcome::Aborted,
            ..
        }
    )));
}
