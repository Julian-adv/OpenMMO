use super::*;
use crate::auth::{AuthService, CharacterRecord};

const DUNGEON: &str = "skeleton_crypt";

async fn stage_delver(test_name: &str) -> (GameState, AuthService, String, i64, PlayerId) {
    let game = make_test_game_state(test_name);
    game.debug_set_time(12, 0);
    game.tick_dungeon_reset().await;
    game.ensure_dungeon_runtime(DUNGEON).await;
    let entrance = game.dungeon_defs.get(DUNGEON).unwrap().position();
    let chest = game.dungeons.read().await[DUNGEON].layouts[19]
        .chest
        .unwrap();
    let position = cell_center(&entrance, 20, chest);
    let auth = make_test_auth(test_name);
    let account = auth.login_npc("npc_dungeon_reconnect").unwrap();
    let character = create_test_character(&auth, &account, "Delver");
    let mut player = make_player("Delver", position.x, position.z);
    player.position = position;
    player.floor_level = -20;
    player.health = 7;
    let id = player.id;
    game.add_player(player).await;
    game.register_player_character(&id, character.id, 0, attrs_with_cha(12), 123, None)
        .await;
    game.handle_player_floor_change(&id, 0, -20, &position, &position)
        .await;
    (game, auth, account, character.id, id)
}

fn saved_player(record: &CharacterRecord) -> Player {
    let mut player = make_player(&record.name, record.last_x, record.last_z);
    player.position.y = record.last_y;
    player.rotation = record.last_rotation;
    player.floor_level = record.floor_level;
    player.health = record.health.unwrap();
    player
}

#[tokio::test]
async fn dungeon_reconnect_in_same_night_preserves_position_after_restart() {
    let (game, auth, account, character_id, player_id) = stage_delver("dungeon_rejoin").await;
    game.persist_shutdown_snapshot(&auth).await;
    let record = auth
        .get_character_for_account(&account, character_id)
        .unwrap();
    assert_eq!(record.dungeon_epoch, Some(game.dungeon_save_epoch().await));

    let restarted = make_test_game_state("dungeon_rejoin_restarted");
    restarted.debug_set_datetime(&auth.load_world_time().unwrap().unwrap());
    restarted.tick_dungeon_reset().await;
    let mut player = saved_player(&record);
    assert!(
        restarted
            .rehydrate_dungeon_player(&mut player, record.dungeon_epoch)
            .await
    );
    assert_eq!(
        player.position,
        game.players.read().await[&player_id].position
    );
    assert_eq!(player.floor_level, -20);
    assert_eq!(player.health, 7);
}

#[tokio::test]
async fn dungeon_reconnect_after_offline_reset_returns_to_own_entrance() {
    let (game, auth, account, character_id, player_id) = stage_delver("dungeon_offline").await;
    game.persist_and_detach_player(&player_id, &auth).await;
    game.remove_player(&player_id).await;
    let record = auth
        .get_character_for_account(&account, character_id)
        .unwrap();

    for _ in 0..2 {
        game.debug_set_time(23, 0);
        game.tick_dungeon_reset().await;
    }

    let mut player = saved_player(&record);
    assert!(
        game.rehydrate_dungeon_player(&mut player, record.dungeon_epoch)
            .await
    );
    assert_eq!(player.floor_level, 0);
    assert_eq!(
        player.position,
        game.dungeon_defs.get(DUNGEON).unwrap().position()
    );
    assert_eq!(player.health, 7);
    assert_eq!(record.gold, 123);
    assert!(game.dungeons.read().await[DUNGEON].floors[&20]
        .players
        .is_empty());
}

#[tokio::test(start_paused = true)]
async fn dungeon_logout_during_reset_warning_keeps_the_expired_epoch() {
    let (game, auth, account, character_id, player_id) =
        stage_delver("dungeon_warning_logout").await;
    let old_epoch = game.dungeon_save_epoch().await;
    let mut rx = game.register_direct_channel(&player_id).await;
    game.debug_set_time(23, 0);
    let reset_game = game.clone();
    let reset = tokio::spawn(async move { reset_game.tick_dungeon_reset().await });
    tokio::task::yield_now().await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::DungeonReset)));

    game.persist_and_detach_player(&player_id, &auth).await;
    game.remove_player(&player_id).await;
    let record = auth
        .get_character_for_account(&account, character_id)
        .unwrap();
    assert_eq!(record.dungeon_epoch, Some(old_epoch));
    assert_eq!(record.floor_level, -20);
    reset.await.unwrap();

    let mut player = saved_player(&record);
    assert!(
        game.rehydrate_dungeon_player(&mut player, record.dungeon_epoch)
            .await
    );
    assert_eq!(player.floor_level, 0);
    assert_eq!(
        player.position,
        game.dungeon_defs.get(DUNGEON).unwrap().position()
    );
}

#[tokio::test]
async fn dungeon_reconnect_without_legacy_epoch_returns_to_entrance() {
    let game = make_test_game_state("dungeon_legacy_reconnect");
    let entrance = game.dungeon_defs.get(DUNGEON).unwrap().position();
    let mut player = make_player("Legacy", entrance.x, entrance.z);
    player.floor_level = -20;
    player.health = 0;
    assert!(game.rehydrate_dungeon_player(&mut player, None).await);
    assert_eq!(player.floor_level, 0);
    assert_eq!(player.position, entrance);
    assert_eq!(player.health, 0);
}

#[tokio::test]
async fn dungeon_reset_waits_for_an_inflight_login_to_register_occupancy() {
    let game = make_test_game_state("dungeon_reset_during_login");
    game.debug_set_time(12, 0);
    game.tick_dungeon_reset().await;
    let epoch = game.dungeon_save_epoch().await;
    let entrance = game.dungeon_defs.get(DUNGEON).unwrap().position();
    let mut player = make_player("Joining", entrance.x, entrance.z);
    player.position.y = onlinerpg_shared::dungeon::floor_world_y(entrance.y, 20);
    player.floor_level = -20;
    let sessions = game.lock_character_sessions().await;
    assert!(
        game.rehydrate_dungeon_player(&mut player, Some(epoch))
            .await
    );

    game.debug_set_time(23, 0);
    let reset_game = game.clone();
    let reset = tokio::spawn(async move { reset_game.tick_dungeon_reset().await });
    tokio::task::yield_now().await;
    assert!(!reset.is_finished());

    let id = player.id;
    let position = player.position;
    game.add_player(player).await;
    game.handle_player_floor_change(&id, 0, -20, &position, &position)
        .await;
    drop(sessions);
    reset.await.unwrap();
    let players = game.players.read().await;
    assert_eq!(players[&id].floor_level, 0);
    assert_eq!(players[&id].position, entrance);
}

#[tokio::test]
async fn dungeon_periodic_save_tracks_completed_resets_and_clears_surface_epoch() {
    let (game, auth, account, character_id, player_id) = stage_delver("dungeon_epoch_save").await;
    game.mark_dirty(&player_id).await;
    game.flush_dirty_saves(&auth).await;
    let record = auth
        .get_character_for_account(&account, character_id)
        .unwrap();
    assert_eq!(record.dungeon_epoch, Some(game.dungeon_save_epoch().await));

    let entrance = game.dungeon_defs.get(DUNGEON).unwrap().position();
    game.teleport_player(&player_id, entrance, 0.0, 0).await;
    game.flush_dirty_saves(&auth).await;
    let record = auth
        .get_character_for_account(&account, character_id)
        .unwrap();
    assert_eq!(record.floor_level, 0);
    assert_eq!(record.dungeon_epoch, None);
}
