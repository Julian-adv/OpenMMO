use super::*;
use onlinerpg_shared::furniture::FurniturePlacement;
use tokio::time::{advance, Duration};

async fn sleeper(game: &GameState, kind: &str) -> PlayerId {
    let mut player = make_player("sleeper", 100.0, 50.0);
    player.position.y = 5.0;
    let id = player.id;
    game.register_mana(&player, 10, Some(0)).await;
    game.add_player(player).await;
    game.register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(500))
        .await;
    game.players.write().await.get_mut(&id).unwrap().max_health = 100;
    game.sync_region_furniture(
        0,
        0,
        &[FurniturePlacement {
            id: 7,
            type_id: kind.into(),
            x: 100.0,
            y: 5.0,
            z: 50.0,
            rotation_deg: 0.0,
            floor_level: 0,
        }],
    );
    advance(Duration::from_secs(26)).await;
    id
}

async fn vitals(game: &GameState, id: &PlayerId) -> (u32, u32) {
    (
        game.players.read().await[id].health,
        game.mana.read().await[id].mana,
    )
}

#[tokio::test(start_paused = true)]
async fn bed_rest_doubles_hp_and_mp_only_after_a_full_interval_and_stops_on_standing() {
    for kind in ["bed", "rustic_bed"] {
        let game = make_test_game_state(kind);
        let id = sleeper(&game, kind).await;
        game.set_player_interaction(&id, Some(kind.into()), Some(7))
            .await;
        advance(Duration::from_secs(15)).await;
        game.tick_regeneration().await;
        assert_eq!(vitals(&game, &id).await, (11, 1));
        advance(Duration::from_secs(1)).await;
        game.tick_regeneration().await;
        assert_eq!(vitals(&game, &id).await, (13, 3));
        game.set_player_interaction(&id, None, None).await;
        advance(Duration::from_secs(16)).await;
        game.tick_regeneration().await;
        assert_eq!(vitals(&game, &id).await, (14, 4));
    }
}

#[tokio::test(start_paused = true)]
async fn bed_rest_cannot_bank_time_across_standing_or_missing_beds() {
    let game = make_test_game_state("bed_rest_reset");
    let id = sleeper(&game, "bed").await;
    game.set_player_interaction(&id, Some("bed".into()), Some(7))
        .await;
    advance(Duration::from_secs(15)).await;
    game.set_player_interaction(&id, None, None).await;
    game.set_player_interaction(&id, Some("bed".into()), Some(7))
        .await;
    advance(Duration::from_secs(1)).await;
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (11, 1));
    advance(Duration::from_secs(15)).await;
    game.sync_region_furniture(0, 0, &[]);
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (12, 2));
    assert!(game.players.read().await[&id].object_type.is_none());
    assert!(!game.bed_rest_started.read().await.contains_key(&id));
}

#[tokio::test(start_paused = true)]
async fn bed_rest_rejects_fake_distant_wrong_floor_and_mounted_poses() {
    for case in ["fake", "distant", "floor", "mounted", "wrong_type"] {
        let game = make_test_game_state(case);
        let id = sleeper(&game, "bed").await;
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&id).unwrap();
            match case {
                "distant" => player.position.x += 10.0,
                "floor" => player.floor_level = 1,
                "mounted" => player.mount = Some(onlinerpg_shared::mount::MountKind::Horse),
                _ => {}
            }
        }
        game.set_player_interaction(
            &id,
            Some(
                if case == "wrong_type" {
                    "rustic_bed"
                } else {
                    "bed"
                }
                .into(),
            ),
            Some(if case == "fake" { 99 } else { 7 }),
        )
        .await;
        advance(Duration::from_secs(16)).await;
        game.tick_regeneration().await;
        assert_eq!(vitals(&game, &id).await, (11, 1), "{case}");
    }
}

#[tokio::test(start_paused = true)]
async fn bed_rest_keeps_hp_and_mp_independent_and_respects_recovery_limits() {
    let game = make_test_game_state("bed_rest_limits");
    let id = sleeper(&game, "bed").await;
    game.set_player_interaction(&id, Some("bed".into()), Some(7))
        .await;
    advance(Duration::from_secs(16)).await;
    game.players.write().await.get_mut(&id).unwrap().health = 100;
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (100, 2));
    game.players.write().await.get_mut(&id).unwrap().health = 10;
    game.register_hunger(&id, 50).await;
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (10, 4));
    game.register_hunger(&id, 500).await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (10, 4));
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    game.players.write().await.get_mut(&id).unwrap().health = 99;
    game.mana.write().await.get_mut(&id).unwrap().mana = 14;
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (100, 15));
    game.players.write().await.get_mut(&id).unwrap().health = 0;
    game.tick_regeneration().await;
    assert_eq!(vitals(&game, &id).await, (0, 15));
    assert!(game.players.read().await[&id].object_type.is_none());
}

#[tokio::test(start_paused = true)]
async fn bed_rest_ends_for_actions_and_disconnect() {
    use onlinerpg_shared::ability::AbilityId;
    for action in [
        "item",
        "ability",
        "attack",
        "teleport",
        "death",
        "disconnect",
    ] {
        let game = make_test_game_state(action);
        let id = sleeper(&game, "bed").await;
        game.set_player_interaction(&id, Some("bed".into()), Some(7))
            .await;
        advance(Duration::from_secs(16)).await;
        assert!(game.bed_rest_started.read().await.contains_key(&id));
        match action {
            "item" => game.use_item(&id, 999).await,
            "ability" => game.use_ability(&id, AbilityId::GuardianWard).await,
            "attack" => game.player_attack(&id, "missing".into(), None).await,
            "teleport" => {
                game.teleport_player(
                    &id,
                    Position {
                        x: 101.0,
                        y: 0.0,
                        z: 50.0,
                    },
                    0.0,
                    0,
                )
                .await
            }
            "death" => game.on_player_died(&id, "test").await,
            "disconnect" => game.remove_player(&id).await,
            _ => unreachable!(),
        }
        assert!(
            !game.bed_rest_started.read().await.contains_key(&id),
            "{action}"
        );
        assert!(
            game.players
                .read()
                .await
                .get(&id)
                .is_none_or(|p| p.object_type.is_none()),
            "{action}"
        );
    }
}
