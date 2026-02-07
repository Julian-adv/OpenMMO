use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

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

impl Player {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            position: Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: 0.0,
            level: 1,
            health: 100,
            max_health: 100,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    pub id: String,
    pub monster_type: String,
    pub position: Position,
    pub rotation: f32,
    pub state: String,
    pub owner_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientMessage {
    #[serde(rename = "join")]
    Join { player_name: String },
    #[serde(rename = "player_move")]
    PlayerMove { position: Position, rotation: f32 },
    #[serde(rename = "chat_message")]
    ChatMessage { message: String },
    #[serde(rename = "request_spawn_monster")]
    RequestSpawnMonster {
        monster_type: String,
        position: Position,
        rotation: f32,
    },
    #[serde(rename = "monster_move")]
    MonsterMove {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: String,
        target_position: Position,
    },
    #[serde(rename = "player_attack")]
    PlayerAttack { monster_id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ServerMessage {
    #[serde(rename = "join_success")]
    JoinSuccess { player: Player },
    #[serde(rename = "player_joined")]
    PlayerJoined { player: Player },
    #[serde(rename = "player_left")]
    PlayerLeft { player_id: String },
    #[serde(rename = "player_moved")]
    PlayerMoved {
        player_id: String,
        position: Position,
        rotation: f32,
    },
    #[serde(rename = "chat_message")]
    ChatMessage { player_id: String, message: String },
    #[serde(rename = "game_state")]
    GameState {
        players: HashMap<String, Player>,
        monsters: HashMap<String, Monster>,
    },
    #[serde(rename = "monster_spawned")]
    MonsterSpawned { monster: Monster },
    #[serde(rename = "monster_moved")]
    MonsterMoved {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: String,
        target_position: Position,
        owner_id: Option<String>,
    },
    #[serde(rename = "monster_removed")]
    MonsterRemoved { monster_id: String },
    #[serde(rename = "player_attacked")]
    PlayerAttacked {
        player_id: String,
        monster_id: String,
        hit: bool,
        roll: u8,
    },
}

pub type PlayerId = String;
