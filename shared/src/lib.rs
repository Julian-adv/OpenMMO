use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub position: Position,
    pub rotation: f32,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    pub id: String,
    pub monster_type: String,
    pub position: Position,
    pub rotation: f32,
    pub state: String,
    pub owner_id: Option<String>,
    pub health: u32,
    pub max_health: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
    Join {
        player_name: String,
        password_hash: String,
        create_account: bool,
    },
    PlayerMove {
        position: Position,
        rotation: f32,
    },
    ChatMessage {
        message: String,
    },
    RequestSpawnMonster {
        monster_type: String,
        position: Position,
        rotation: f32,
    },
    MonsterMove {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: String,
        target_position: Position,
    },
    PlayerAttack {
        monster_id: String,
    },
    MonsterAttack {
        monster_id: String,
        target_player_id: String,
    },
    RequestRespawn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    JoinSuccess {
        player: Player,
    },
    AuthError {
        message: String,
    },
    PlayerJoined {
        player: Player,
    },
    PlayerLeft {
        player_id: String,
    },
    PlayerMoved {
        player_id: String,
        position: Position,
        rotation: f32,
    },
    ChatMessage {
        player_id: String,
        message: String,
    },
    GameState {
        players: HashMap<String, Player>,
        monsters: HashMap<String, Monster>,
    },
    MonsterSpawned {
        monster: Monster,
    },
    MonsterMoved {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: String,
        target_position: Position,
        owner_id: Option<String>,
    },
    MonsterRemoved {
        monster_id: String,
    },
    MonsterDead {
        monster_id: String,
    },
    PlayerAttacked {
        player_id: String,
        monster_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
    },
    MonsterAttackedPlayer {
        monster_id: String,
        player_id: String,
        hit: bool,
        roll: u8,
        damage: u32,
    },
    PlayerDead {
        player_id: String,
    },
    PlayerRespawned {
        player: Player,
    },
    Kicked {
        player_id: String,
        reason: String,
    },
}

pub type PlayerId = String;

// Serialization helpers (used by both server and wasm)
pub fn serialize_client_msg(msg: &ClientMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

pub fn deserialize_client_msg(bytes: &[u8]) -> Result<ClientMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

pub fn serialize_server_msg(msg: &ServerMessage) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(msg)
}

pub fn deserialize_server_msg(bytes: &[u8]) -> Result<ServerMessage, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

#[cfg(target_arch = "wasm32")]
mod wasm_api {
    use super::*;
    use serde::Serialize;
    use wasm_bindgen::prelude::*;

    fn to_js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
        let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
        value
            .serialize(&serializer)
            .map_err(|e| JsError::new(&format!("JS conversion failed: {e}")))
    }

    #[wasm_bindgen]
    pub fn serialize_client_message(val: JsValue) -> Result<Vec<u8>, JsError> {
        let msg: ClientMessage = serde_wasm_bindgen::from_value(val)
            .map_err(|e| JsError::new(&format!("Invalid client message: {e}")))?;
        rmp_serde::to_vec(&msg)
            .map_err(|e| JsError::new(&format!("Serialization failed: {e}")))
    }

    #[wasm_bindgen]
    pub fn deserialize_server_message(bytes: &[u8]) -> Result<JsValue, JsError> {
        let msg: ServerMessage = rmp_serde::from_slice(bytes)
            .map_err(|e| JsError::new(&format!("Deserialization failed: {e}")))?;
        to_js(&msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_client_message() {
        let msg = ClientMessage::PlayerMove {
            position: Position {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            rotation: 1.5,
        };
        let bytes = serialize_client_msg(&msg).unwrap();
        let decoded = deserialize_client_msg(&bytes).unwrap();
        match decoded {
            ClientMessage::PlayerMove { position, rotation } => {
                assert_eq!(position.x, 1.0);
                assert_eq!(position.y, 2.0);
                assert_eq!(position.z, 3.0);
                assert_eq!(rotation, 1.5);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn roundtrip_unit_variant() {
        let msg = ClientMessage::RequestRespawn;
        let bytes = serialize_client_msg(&msg).unwrap();
        let decoded = deserialize_client_msg(&bytes).unwrap();
        assert!(matches!(decoded, ClientMessage::RequestRespawn));
    }

    #[test]
    fn roundtrip_server_message_with_hashmap() {
        let mut players = HashMap::new();
        players.insert(
            "p1".to_string(),
            Player {
                id: "p1".to_string(),
                name: "Test".to_string(),
                position: Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation: 0.0,
                level: 1,
                health: 10,
                max_health: 10,
            },
        );
        let msg = ServerMessage::GameState {
            players,
            monsters: HashMap::new(),
        };
        let bytes = serialize_server_msg(&msg).unwrap();
        let decoded = deserialize_server_msg(&bytes).unwrap();
        match decoded {
            ServerMessage::GameState {
                players, monsters, ..
            } => {
                assert!(players.contains_key("p1"));
                assert!(monsters.is_empty());
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn roundtrip_all_server_messages() {
        let messages = vec![
            ServerMessage::AuthError {
                message: "bad".to_string(),
            },
            ServerMessage::PlayerLeft {
                player_id: "p1".to_string(),
            },
            ServerMessage::MonsterDead {
                monster_id: "m1".to_string(),
            },
            ServerMessage::PlayerAttacked {
                player_id: "p1".to_string(),
                monster_id: "m1".to_string(),
                hit: true,
                roll: 18,
                damage: 5,
            },
            ServerMessage::Kicked {
                player_id: "p1".to_string(),
                reason: "test".to_string(),
            },
        ];
        for msg in messages {
            let bytes = serialize_server_msg(&msg).unwrap();
            let decoded = deserialize_server_msg(&bytes).unwrap();
            // Just verify it roundtrips without error
            assert!(format!("{:?}", decoded).len() > 0);
        }
    }
}
