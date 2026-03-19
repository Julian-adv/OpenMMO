use serde::{Deserialize, Serialize};

use crate::Position;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WallVariant {
    #[serde(rename = "solid")]
    Solid,
    #[serde(rename = "door")]
    WithDoor,
    #[serde(rename = "window")]
    WithWindow,
    #[serde(rename = "open")]
    Open,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallConfig {
    pub variant: WallVariant,
    pub texture: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomData {
    pub local_x: i32,
    pub local_z: i32,
    pub size_x: u8,
    pub size_z: u8,
    pub floor_level: u8,
    pub floor_texture: u8,
    pub roof_texture: u8,
    pub wall_height: f32,
    pub wall_north: WallConfig,
    pub wall_south: WallConfig,
    pub wall_east: WallConfig,
    pub wall_west: WallConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HouseData {
    pub id: String,
    pub owner_id: String,
    pub origin: Position,
    pub rooms: Vec<RoomData>,
}
