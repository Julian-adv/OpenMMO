use super::*;
use crate::housing::HousingIO;
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterClass, Gender, Position, ServerMessage};
use crate::world_config::world_config;
use tokio::sync::broadcast::error::TryRecvError;

fn make_test_game_state(test_name: &str) -> GameState {
    let housing_dir = std::env::temp_dir().join(format!(
        "onlinerpg_{test_name}_housing_{}",
        uuid::Uuid::new_v4()
    ));
    let housing_io = Arc::new(HousingIO::new(housing_dir));
    GameState::new(
        MonsterDefs::load(),
        GameState::default_start_datetime(),
        housing_io,
        vec![],
        vec![],
    )
}

#[tokio::test]
async fn respawn_player_revives_dead_player_only() {
    let game_state = make_test_game_state("respawn_dead");

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
        gender: Gender::default(),
        is_npc: false,
        torch_on: false,
        floor_level: 0,
        furniture_type: None,
        furniture_id: None,
        last_combat_at: 0,
    };
    let player_id = player.id.clone();
    game_state.add_player(player).await;

    let mut rx = game_state.subscribe();
    game_state.respawn_player(&player_id).await;

    let players = game_state.get_all_players().await;
    let revived = players
        .get(&player_id)
        .expect("Player should still exist after respawn");
    let spawn = &world_config().spawn_position;
    assert_eq!(revived.health, revived.max_health);
    assert_eq!(revived.position.x, spawn.x);
    assert_eq!(revived.position.y, spawn.y);
    assert_eq!(revived.position.z, spawn.z);
    assert_eq!(revived.rotation, spawn.rotation);

    match rx.try_recv() {
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            match server_msg {
                ServerMessage::PlayerRespawned { player } => {
                    assert_eq!(player.id, player_id);
                    assert_eq!(player.health, player.max_health);
                }
                other => panic!("Expected PlayerRespawned, got {:?}", other),
            }
        }
        Err(err) => panic!("Expected PlayerRespawned broadcast, got {:?}", err),
    }
}

#[tokio::test]
async fn respawn_player_ignores_alive_player() {
    let game_state = make_test_game_state("respawn_alive");

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
        gender: Gender::default(),
        is_npc: false,
        torch_on: false,
        floor_level: 0,
        furniture_type: None,
        furniture_id: None,
        last_combat_at: 0,
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
        Ok(msg) => {
            let server_msg: ServerMessage =
                rmp_serde::from_slice(&msg.bytes).expect("Failed to deserialize broadcast");
            panic!(
                "Expected no broadcast for alive respawn, got {:?}",
                server_msg
            );
        }
        Err(err) => panic!("Expected empty channel, got {:?}", err),
    }
}
