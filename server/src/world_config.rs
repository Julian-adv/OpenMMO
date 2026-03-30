use serde::Deserialize;
use std::sync::LazyLock;
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct WorldConfig {
    #[serde(rename = "spawnPosition")]
    pub spawn_position: SpawnPosition,
    #[serde(rename = "maxMonstersTotal", default = "default_max_monsters_total")]
    pub max_monsters_total: u32,
    #[serde(rename = "monsterSpawns", default)]
    pub monster_spawns: Vec<MonsterSpawnRule>,
}

fn default_max_monsters_total() -> u32 {
    1000
}

#[derive(Debug, Deserialize)]
pub struct SpawnPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonsterSpawnRule {
    #[serde(rename = "monsterType")]
    pub monster_type: String,
    #[serde(rename = "maxPerPlayer")]
    pub max_per_player: u32,
    #[allow(dead_code)]
    #[serde(rename = "spawnIntervalSecs")]
    pub spawn_interval_secs: u64,
    #[serde(rename = "spawnCenter")]
    pub spawn_center: SpawnCenter,
    #[serde(rename = "spawnRadius")]
    pub spawn_radius: f32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpawnCenter {
    pub x: f32,
    pub z: f32,
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
