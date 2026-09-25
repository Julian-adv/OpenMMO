use super::*;
use onlinerpg_shared::skills::{SkillId, Skills};

async fn learner(game: &GameState, name: &str) -> (PlayerId, DirectRx) {
    let (id, rx) = make_angler(game, name).await;
    game.register_player_skills(&id, Skills::default()).await;
    (id, rx)
}

async fn teacher(game: &GameState) -> (PlayerId, DirectRx) {
    let (id, rx) = learner(game, "Tobin").await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .is_official_npc = true;
    (id, rx)
}

async fn land_cast(game: &GameState, id: &PlayerId, rx: &mut DirectRx) {
    advance_until_bite(game, rx).await;
    {
        let mut sessions = game.fishing_sessions.write().await;
        let fish = sessions.get_mut(id).unwrap().rolled_fish.as_mut().unwrap();
        fish.item_def_id = "raw_minnow".into();
        fish.rarity = 1;
        fish.trophy = false;
    }
    game.respond_fishing(id, FishingAction::Hook).await;
    let (outcome, _) = flow_tests::fight_to_the_end(game, id, rx, auto_stance).await;
    assert!(matches!(outcome, FishingOutcome::Caught { .. }));
}

#[tokio::test(start_paused = true)]
async fn watching_tobin_unlocks_fishing_once_and_survives_reload() {
    let game = make_test_game_state("fishing_lesson");
    let (tobin, mut tobin_rx) = teacher(&game).await;
    let (student, mut rx) = learner(&game, "student").await;

    game.start_fishing(&student, water_target()).await;
    assert!(!game.fishing_sessions.read().await.contains_key(&student));
    assert!(drain(&mut rx).iter().any(|m| matches!(m,
        ServerMessage::FishingError { message } if message.contains("Watch Tobin")
    )));

    let rod = game
        .inventories
        .write()
        .await
        .get_mut(&student)
        .unwrap()
        .equipped
        .remove(&EquipSlot::MainHand)
        .unwrap();
    game.start_fishing(&tobin, water_target()).await;
    assert!(!game.has_skill(&student, SkillId::Fishing).await);
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert!(game.has_skill(&student, SkillId::Fishing).await);
    let messages = drain(&mut rx);
    assert!(messages.iter().any(|m| matches!(m,
        ServerMessage::SkillsUpdate { skills } if skills.has(SkillId::Fishing)
    )));
    assert!(messages.iter().any(|m| matches!(m,
        ServerMessage::SystemMessage { message, .. } if message.contains("learned Fishing")
    )));
    assert!(game.dirty_skills.read().await.contains(&student));

    let (_, rows) = game.take_player_skills(&student).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].skill_id, "fishing");
    game.register_player_skills(&student, crate::game_state::skills::skills_from_rows(&rows))
        .await;
    assert!(game.has_skill(&student, SkillId::Fishing).await);

    game.start_fishing(&tobin, water_target()).await;
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert!(!drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::SkillsUpdate { .. } | ServerMessage::SystemMessage { .. }
    )));
    assert!(!game.dirty_skills.read().await.contains(&student));
    game.inventories
        .write()
        .await
        .get_mut(&student)
        .unwrap()
        .equipped
        .insert(EquipSlot::MainHand, rod);
    game.start_fishing(&student, water_target()).await;
    assert!(game.fishing_sessions.read().await.contains_key(&student));
}

#[tokio::test(start_paused = true)]
async fn a_lesson_requires_staying_nearby_alive_on_the_same_floor_for_the_whole_cast() {
    let game = make_test_game_state("fishing_lesson_observers");
    let (tobin, mut tobin_rx) = teacher(&game).await;
    let mut students = Vec::new();
    for name in ["near", "left", "late", "upstairs", "dead", "above", "npc"] {
        students.push(learner(&game, name).await.0);
    }
    {
        let mut players = game.players.write().await;
        players.get_mut(&pid("near")).unwrap().position.x += 6.0;
        players.get_mut(&pid("late")).unwrap().position.x += 20.0;
        players.get_mut(&pid("upstairs")).unwrap().floor_level = 1;
        players.get_mut(&pid("dead")).unwrap().health = 0;
        players.get_mut(&pid("above")).unwrap().position.y += 10.0;
        players.get_mut(&pid("npc")).unwrap().is_official_npc = true;
    }
    game.start_fishing(&tobin, water_target()).await;
    game.players
        .write()
        .await
        .get_mut(&pid("left"))
        .unwrap()
        .position
        .x += 20.0;
    advance_with_ticks(&game, 250).await;
    {
        let mut players = game.players.write().await;
        players.get_mut(&pid("left")).unwrap().position.x -= 20.0;
        players.get_mut(&pid("late")).unwrap().position.x -= 20.0;
        players.get_mut(&pid("upstairs")).unwrap().floor_level = 0;
        players.get_mut(&pid("dead")).unwrap().health = 100;
    }
    land_cast(&game, &tobin, &mut tobin_rx).await;
    for id in students {
        assert_eq!(
            game.has_skill(&id, SkillId::Fishing).await,
            id == pid("near")
        );
    }
    game.start_fishing(&tobin, water_target()).await;
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert!(game.has_skill(&pid("left"), SkillId::Fishing).await);
    assert!(game.has_skill(&pid("late"), SkillId::Fishing).await);
}

#[tokio::test(start_paused = true)]
async fn aborted_escaped_and_impostor_fishing_do_not_teach() {
    let game = make_test_game_state("fishing_lesson_invalid");
    let (tobin, mut tobin_rx) = teacher(&game).await;
    let (student, _rx) = learner(&game, "student").await;
    game.start_fishing(&tobin, water_target()).await;
    game.stop_fishing(&tobin).await;
    assert!(!game.has_skill(&student, SkillId::Fishing).await);

    game.start_fishing(&tobin, water_target()).await;
    advance_until_bite(&game, &mut tobin_rx).await;
    advance_with_ticks(
        &game,
        u64::from(BITE_WINDOW_MS + 2 * LATENCY_GRACE_MS) + 250,
    )
    .await;
    assert!(!game.has_skill(&student, SkillId::Fishing).await);
    assert!(!game.fishing_sessions.read().await.contains_key(&tobin));

    game.learn_skill(&tobin, SkillId::Fishing).await;
    game.players
        .write()
        .await
        .get_mut(&tobin)
        .unwrap()
        .is_official_npc = false;
    game.start_fishing(&tobin, water_target()).await;
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert!(!game.has_skill(&student, SkillId::Fishing).await);

    {
        let mut players = game.players.write().await;
        let angler = players.get_mut(&tobin).unwrap();
        angler.is_official_npc = true;
        angler.name = "Another angler".into();
    }
    game.start_fishing(&tobin, water_target()).await;
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert!(!game.has_skill(&student, SkillId::Fishing).await);
}

#[tokio::test(start_paused = true)]
async fn existing_anglers_keep_access_and_learned_status_after_watching() {
    let game = make_test_game_state("fishing_lesson_existing_angler");
    let (tobin, mut tobin_rx) = teacher(&game).await;
    let (angler, mut rx) = make_angler(&game, "experienced_angler").await;
    let skills = crate::game_state::skills::skills_from_rows(&[crate::auth::SkillRow {
        skill_id: "fishing".into(),
    }]);
    game.register_player_skills(&angler, skills.clone()).await;

    game.start_fishing(&angler, water_target()).await;
    assert!(game.fishing_sessions.read().await.contains_key(&angler));
    game.stop_fishing(&angler).await;
    drain(&mut rx);
    drain(&mut tobin_rx);

    game.start_fishing(&tobin, water_target()).await;
    land_cast(&game, &tobin, &mut tobin_rx).await;
    assert_eq!(game.player_skills.read().await[&angler], skills);
    assert!(!game.dirty_skills.read().await.contains(&angler));
    assert!(!drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::SkillsUpdate { .. } | ServerMessage::SystemMessage { .. }
    )));
}
