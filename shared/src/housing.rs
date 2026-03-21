use serde::{Deserialize, Serialize};

use crate::Position;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RoomType {
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "stairwell")]
    Stairwell,
}

impl Default for RoomType {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RoofType {
    #[serde(rename = "flat")]
    Flat,
    #[serde(rename = "gabled")]
    Gabled,
    #[serde(rename = "steep")]
    Steep,
}

impl Default for RoofType {
    fn default() -> Self {
        Self::Flat
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RoofRidgeDir {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "x")]
    X,
    #[serde(rename = "z")]
    Z,
}

impl Default for RoofRidgeDir {
    fn default() -> Self {
        Self::Auto
    }
}

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
#[serde(rename_all = "camelCase")]
pub struct RoomData {
    #[serde(default)]
    pub room_type: RoomType,
    #[serde(default)]
    pub roof_type: RoofType,
    #[serde(default)]
    pub roof_ridge_dir: RoofRidgeDir,
    pub local_x: i32,
    pub local_z: i32,
    pub size_x: u8,
    pub size_z: u8,
    pub floor_level: u8,
    pub floor_texture: u8,
    pub roof_texture: u8,
    pub wall_height: f32,
    /// 1m segments: north wall (length = size_x)
    pub wall_north: Vec<WallConfig>,
    /// 1m segments: south wall (length = size_x)
    pub wall_south: Vec<WallConfig>,
    /// 1m segments: east wall (length = size_z)
    pub wall_east: Vec<WallConfig>,
    /// 1m segments: west wall (length = size_z)
    pub wall_west: Vec<WallConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseData {
    pub id: String,
    pub owner_id: String,
    pub origin: Position,
    pub rooms: Vec<RoomData>,
}
