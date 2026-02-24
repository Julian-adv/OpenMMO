use super::*;
use crate::monster_defs::MonsterDefs;
use crate::types::CharacterClass;
use std::path::PathBuf;
use tokio::sync::broadcast::error::TryRecvError;

fn make_test_db_path(test_name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("onlinerpg_{test_name}_{}.db", uuid::Uuid::new_v4()))
}

fn make_test_game_state(test_name: &str) -> (GameState, PathBuf) {
    let db_path = make_test_db_path(test_name);
    let auth_service = Arc::new(
        AuthService::new(db_path.clone()).expect("Failed to initialize test auth service"),
    );
    let game_state = GameState::new(
        MonsterDefs::load(),
        GameState::default_start_datetime(),
        auth_service,
    );
    (game_state, db_path)
}

fn cleanup_test_db(db_path: &PathBuf) {
    let _ = std::fs::remove_file(db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
}

#[tokio::test]
async fn respawn_player_revives_dead_player_only() {
    let (game_state, db_path) = make_test_game_state("respawn_dead");

    let player = Player {
        id: "player_dead".to_string(),
        name: "DeadPlayer".to_string(),
        position: Position {
            x: 12.0,
            y: 0.0,
            z: -4.0,
        },
        rotation: 1.25,
        level: 3,
        health: 0,
        max_health: 30,
        class: CharacterClass::Knight,
    };
    let player_id = player.id.clone();
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let revived = players
        .get(&player_id)
        .expect("Player should still exist after respawn");
    assert_eq!(revived.health, revived.max_health);
    assert_eq!(revived.position.x, 0.0);
    assert_eq!(revived.position.y, 0.0);
    assert_eq!(revived.position.z, 0.0);
    assert_eq!(revived.rotation, 0.0);

    match rx.try_recv() {
        Ok(ServerMessage::PlayerRespawned { player }) => {
            assert_eq!(player.id, player_id);
            assert_eq!(player.health, player.max_health);
        }
        Ok(other) => panic!("Expected PlayerRespawned, got {:?}", other),
        Err(err) => panic!("Expected PlayerRespawned broadcast, got {:?}", err),
    }

    cleanup_test_db(&db_path);
}

#[tokio::test]
async fn respawn_player_ignores_alive_player() {
    let (game_state, db_path) = make_test_game_state("respawn_alive");

    let player = Player {
        id: "player_alive".to_string(),
        name: "AlivePlayer".to_string(),
        position: Position {
            x: 5.0,
            y: 0.0,
            z: 6.0,
        },
        rotation: 0.75,
        level: 2,
        health: 18,
        max_health: 20,
        class: CharacterClass::Knight,
    };
    let player_id = player.id.clone();
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let unchanged = players
        .get(&player_id)
        .expect("Player should still exist after ignored respawn");
    assert_eq!(unchanged.health, 18);
    assert_eq!(unchanged.position.x, 5.0);
    assert_eq!(unchanged.position.y, 0.0);
    assert_eq!(unchanged.position.z, 6.0);
    assert_eq!(unchanged.rotation, 0.75);

    match rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        Ok(other) => panic!("Expected no broadcast for alive respawn, got {:?}", other),
        Err(err) => panic!("Expected empty channel, got {:?}", err),
    }

    cleanup_test_db(&db_path);
}
