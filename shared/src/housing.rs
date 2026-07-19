use serde::{Deserialize, Serialize};

use crate::Position;

/// Highest housing floor level (0 = ground floor). Housing thus occupies
/// passability floor indices `0..=MAX_FLOOR_LEVEL`; dungeon depths start
/// just above this range (see `dungeon::DUNGEON_FLOOR_INDEX_BASE`), so the
/// two systems can never collide in floor-keyed collision queries. Raising
/// this is the single knob that grows housing — the dungeon base follows
/// automatically.
pub const MAX_FLOOR_LEVEL: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RoomType {
    #[serde(rename = "normal")]
    #[default]
    Normal,
    #[serde(rename = "stairwell")]
    Stairwell,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RoofType {
    #[serde(rename = "flat")]
    #[default]
    Flat,
    #[serde(rename = "gabled")]
    Gabled,
    #[serde(rename = "steep")]
    Steep,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum RoofRidgeDir {
    #[serde(rename = "auto")]
    #[default]
    Auto,
    #[serde(rename = "x")]
    X,
    #[serde(rename = "z")]
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WallDirection {
    #[serde(rename = "north")]
    North,
    #[serde(rename = "south")]
    South,
    #[serde(rename = "east")]
    East,
    #[serde(rename = "west")]
    West,
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

impl WallVariant {
    /// Segments players can toggle open/closed (doors and window shutters).
    pub fn is_openable(self) -> bool {
        matches!(self, WallVariant::WithDoor | WallVariant::WithWindow)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WallConfig {
    pub variant: WallVariant,
    pub texture: u8,
    #[serde(default)]
    pub is_open: bool,
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
    /// Stairwell ascends in reverse direction (180°/270° rotation)
    #[serde(default)]
    pub stair_reversed: bool,
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

impl RoomData {
    pub fn wall(&self, dir: WallDirection) -> &[WallConfig] {
        match dir {
            WallDirection::North => &self.wall_north,
            WallDirection::South => &self.wall_south,
            WallDirection::East => &self.wall_east,
            WallDirection::West => &self.wall_west,
        }
    }

    pub fn wall_mut(&mut self, dir: WallDirection) -> &mut [WallConfig] {
        match dir {
            WallDirection::North => &mut self.wall_north,
            WallDirection::South => &mut self.wall_south,
            WallDirection::East => &mut self.wall_east,
            WallDirection::West => &mut self.wall_west,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassabilityGrid {
    pub floor_level: u8,
    pub origin_x: i32,
    pub origin_z: i32,
    pub width: u8,
    pub depth: u8,
    /// Packed edge bits per cell (N=1, E=2, S=4, W=8). Length = width * depth.
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseData {
    pub id: String,
    pub owner_id: String,
    pub origin: Position,
    pub rooms: Vec<RoomData>,
    #[serde(default)]
    pub passability: Vec<PassabilityGrid>,
}
