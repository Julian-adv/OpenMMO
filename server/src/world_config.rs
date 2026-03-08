use serde::Deserialize;
use std::sync::LazyLock;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct WorldConfig {
    #[serde(rename = "spawnPosition")]
    pub spawn_position: SpawnPosition,
}

#[derive(Debug, Deserialize)]
pub struct SpawnPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
}

static WORLD_CONFIG: LazyLock<WorldConfig> = LazyLock::new(|| {
    let data = include_str!("../../data/world.json");
    serde_json::from_str(data).expect("Failed to parse world.json")
});

pub fn world_config() -> &'static WorldConfig {
    &WORLD_CONFIG
}

pub fn log_world_config() {
    let cfg = world_config();
    info!(
        "Spawn position: ({}, {}, {}) rotation: {}",
        cfg.spawn_position.x,
        cfg.spawn_position.y,
        cfg.spawn_position.z,
        cfg.spawn_position.rotation
    );
}
