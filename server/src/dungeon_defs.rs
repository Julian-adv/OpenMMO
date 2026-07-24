//! Dungeon entrance registry, embedded at compile time from
//! data/dungeons.json (generated from data-src/dungeons.csv by the cargo
//! build script). The entrance id seeds the deterministic layout
//! generator in the shared crate; the client embeds the same JSON at vite
//! build, so both sides agree on entrances without any network exchange.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Deserialize;
use tracing::info;

use onlinerpg_shared::dungeon::{dungeon_origin, GRID};

#[derive(Debug, Clone, Deserialize)]
pub struct DungeonEntranceDef {
    pub id: String,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Entrance orientation, part of the shared dungeons.json schema. Consumed by
    /// the client to face the entrance building; the server only needs position.
    #[serde(default)]
    #[allow(dead_code)]
    pub rotation: f32,
    /// Item definition ids the final-floor treasure chest always yields.
    #[serde(
        rename = "chestDrops",
        default,
        deserialize_with = "crate::semicolon_list::deserialize"
    )]
    pub chest_drops: Vec<String>,
}

impl DungeonEntranceDef {
    pub fn position(&self) -> onlinerpg_shared::Position {
        onlinerpg_shared::Position {
            x: self.x,
            y: self.y,
            z: self.z,
        }
    }

    /// Whether (x, z) lies inside this dungeon's grid footprint.
    pub fn footprint_contains(&self, x: f32, z: f32) -> bool {
        let (ox, oz) = dungeon_origin(self.x, self.z);
        x >= ox && x < ox + GRID as f32 && z >= oz && z < oz + GRID as f32
    }
}

#[derive(Debug, Clone)]
pub struct DungeonDefs {
    defs: Arc<HashMap<String, DungeonEntranceDef>>,
}

impl DungeonDefs {
    /// Load and validate against `item_defs`: every `chestDrops` entry must
    /// name a real item; a typo'd entry panics at startup rather than silently
    /// handing out a broken item (mirrors the world-drop table).
    pub fn load(item_defs: &crate::item_defs::ItemDefs) -> Self {
        let data = include_str!("../../data/dungeons.json");
        let defs: HashMap<String, DungeonEntranceDef> =
            serde_json::from_str(data).expect("Failed to parse dungeons.json");
        info!("Loaded {} dungeon entrances", defs.len());
        for def in defs.values() {
            info!(
                "  {} \"{}\" at ({:.1}, {:.1}, {:.1})",
                def.id, def.name, def.x, def.y, def.z
            );
            for chest_drop in &def.chest_drops {
                assert!(
                    item_defs.get(chest_drop).is_some(),
                    "dungeon '{}' chestDrops entry '{}' has no matching item definition",
                    def.id,
                    chest_drop
                );
            }
        }
        Self {
            defs: Arc::new(defs),
        }
    }

    pub fn get(&self, id: &str) -> Option<&DungeonEntranceDef> {
        self.defs.get(id)
    }

    pub fn all(&self) -> impl Iterator<Item = &DungeonEntranceDef> {
        self.defs.values()
    }

    /// Entrance whose grid footprint contains the given XZ position.
    pub fn entrance_at(&self, x: f32, z: f32) -> Option<&DungeonEntranceDef> {
        self.defs.values().find(|d| d.footprint_contains(x, z))
    }
}
