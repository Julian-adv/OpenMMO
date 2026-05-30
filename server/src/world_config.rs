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
    /// Monster types that spawn dynamically around players (no fixed zones).
    #[serde(rename = "ambientSpawns", default)]
    pub ambient_spawns: Vec<AmbientSpawnRule>,
}

fn default_max_monsters_total() -> u32 {
    1000
}

fn default_max_distance() -> f32 {
    60.0
}

/// A monster type that spawns dynamically near players, instead of within a
/// hand-authored rectangle. The client picks the actual position (grassland,
/// not water, away from towns); the server only enforces caps and validates.
#[derive(Debug, Clone, Deserialize)]
pub struct AmbientSpawnRule {
    #[serde(rename = "monsterType")]
    pub monster_type: String,
    /// Max alive monsters of this type each player may own at once.
    #[serde(rename = "maxPerPlayer")]
    pub max_per_player: u32,
    /// Server-side sanity bound: a requested spawn must be within this many
    /// meters of the requesting player.
    #[serde(rename = "maxDistance", default = "default_max_distance")]
    pub max_distance: f32,
}

#[derive(Debug, Deserialize)]
pub struct SpawnPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation: f32,
}

static WORLD_CONFIG: LazyLock<WorldConfig> = LazyLock::new(|| {
    let data = include_str!("../../data-src/world.json");
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

/// Load no-spawn zones (towns, safe areas) from all per-region zone files.
/// Monster spawn areas are no longer authored per-region — see `ambientSpawns`
/// in world.json.
pub async fn load_no_spawn_zones_from_regions(terrain_io: &TerrainIO) -> Vec<NoSpawnZone> {
    let mut no_spawn_zones = Vec::new();

    let regions = match terrain_io.list_zone_regions().await {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to list zone regions: {e}");
            return no_spawn_zones;
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

        if let Some(zones) = json.get("noSpawnZones") {
            match serde_json::from_value::<Vec<NoSpawnZone>>(zones.clone()) {
                Ok(parsed) => no_spawn_zones.extend(parsed),
                Err(e) => warn!("Bad noSpawnZones in r{rx:+03}_{rz:+03}: {e}"),
            }
        }
    }

    info!(
        "Loaded {} no-spawn zones from region files",
        no_spawn_zones.len()
    );
    no_spawn_zones
}
