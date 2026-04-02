use crate::terrain::io::TerrainIO;
use onlinerpg_shared::NoSpawnZone;
use serde::Deserialize;
use std::sync::LazyLock;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct WorldConfig {
    #[serde(rename = "spawnPosition")]
    pub spawn_position: SpawnPosition,
    #[serde(rename = "maxMonstersTotal", default = "default_max_monsters_total")]
    pub max_monsters_total: u32,
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

/// Monster spawn rule loaded from per-region zone files.
#[derive(Debug, Clone, Deserialize)]
pub struct MonsterSpawnRule {
    #[serde(rename = "monsterType")]
    pub monster_type: String,
    #[serde(rename = "maxPerPlayer")]
    pub max_per_player: u32,
    #[allow(dead_code)]
    #[serde(rename = "maxTotal", default)]
    pub max_total: Option<u32>,
    #[allow(dead_code)]
    #[serde(rename = "spawnIntervalSecs")]
    pub spawn_interval_secs: u64,
    #[serde(rename = "minX")]
    pub min_x: f32,
    #[serde(rename = "minZ")]
    pub min_z: f32,
    #[serde(rename = "maxX")]
    pub max_x: f32,
    #[serde(rename = "maxZ")]
    pub max_z: f32,
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

/// Load spawn rules and no-spawn zones from all per-region zone files.
pub async fn load_spawn_config_from_regions(
    terrain_io: &TerrainIO,
) -> (Vec<MonsterSpawnRule>, Vec<NoSpawnZone>) {
    let mut spawn_rules = Vec::new();
    let mut no_spawn_zones = Vec::new();

    let regions = match terrain_io.list_zone_regions().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to list zone regions: {e}");
            return (spawn_rules, no_spawn_zones);
        }
    };

    for (rx, rz) in regions {
        let json = match terrain_io.read_zone(rx, rz).await {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to read zone r{rx:+03}_{rz:+03}: {e}");
                continue;
            }
        };

        if let Some(spawns) = json.get("monsterSpawns") {
            match serde_json::from_value::<Vec<MonsterSpawnRule>>(spawns.clone()) {
                Ok(rules) => spawn_rules.extend(rules),
                Err(e) => warn!("Bad monsterSpawns in r{rx:+03}_{rz:+03}: {e}"),
            }
        }

        if let Some(zones) = json.get("noSpawnZones") {
            match serde_json::from_value::<Vec<NoSpawnZone>>(zones.clone()) {
                Ok(parsed) => no_spawn_zones.extend(parsed),
                Err(e) => warn!("Bad noSpawnZones in r{rx:+03}_{rz:+03}: {e}"),
            }
        }
    }

    info!(
        "Loaded {} spawn rules, {} no-spawn zones from region files",
        spawn_rules.len(),
        no_spawn_zones.len()
    );
    (spawn_rules, no_spawn_zones)
}
