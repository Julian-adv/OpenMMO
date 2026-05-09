//! Live in-game entity records: the per-frame state the server broadcasts
//! for every player and monster. `Player` and `Monster` are the snapshot
//! types embedded in `ServerMessage::GameState`; `MonsterState` is the
//! enum the client renders as an animation pose.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::character::{CharacterClass, Gender};
use crate::world::Position;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub name: String,
    pub position: Position,
    pub rotation: f32,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub class: CharacterClass,
    #[serde(default)]
    pub gender: Gender,
    #[serde(default)]
    pub is_npc: bool,
    #[serde(default)]
    pub torch_on: bool,
    #[serde(default)]
    pub floor_level: i8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    #[serde(skip)]
    pub object_id: Option<u32>,
    #[serde(skip)]
    pub last_combat_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonsterState {
    #[serde(rename = "idle")]
    Idle,
    #[serde(rename = "walk")]
    Walk,
    #[serde(rename = "run")]
    Run,
    #[serde(rename = "attack")]
    Attack,
    #[serde(rename = "hit")]
    Hit,
    #[serde(rename = "dead")]
    Dead,
}

impl fmt::Display for MonsterState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "idle"),
            Self::Walk => write!(f, "walk"),
            Self::Run => write!(f, "run"),
            Self::Attack => write!(f, "attack"),
            Self::Hit => write!(f, "hit"),
            Self::Dead => write!(f, "dead"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Monster {
    pub id: String,
    pub monster_type: String,
    pub position: Position,
    pub rotation: f32,
    pub state: MonsterState,
    pub owner_id: Option<String>,
    pub health: u32,
    pub max_health: u32,
    #[serde(skip)]
    pub last_attack_at: u64,
}
