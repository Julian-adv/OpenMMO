//! World primitives: 3D position, axis-aligned no-spawn rectangles, and
//! the in-game calendar/clock value the server broadcasts. Tiny but
//! shared by virtually every other type, so they live in one place that
//! has no dependencies on the rest of the crate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Axis-aligned rectangular zone where monsters must not spawn (e.g. towns).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoSpawnZone {
    pub min_x: f32,
    pub min_z: f32,
    pub max_x: f32,
    pub max_z: f32,
}

impl NoSpawnZone {
    pub fn contains(&self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameDateTime {
    pub year: u32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
}
