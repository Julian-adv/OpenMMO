//! Server-side passability cache: the same shared `PassabilityCache` the
//! browser (wasm) and agent-client build, fed from the server's own data
//! (housing files, region objects, dungeon layouts). `tick_player_movement`
//! checks untrusted simulated steps against it so players can't walk
//! through walls. Dungeon interior doors seal their corridor mouth while
//! shut — `interior_doors` derives the same door list (and ids) the client
//! renders, and every toggle rebuilds the floor's cells.

use std::path::Path;

use onlinerpg_shared::dungeon::{
    closed_door_segs, dungeon_cache_key, dungeon_passability, dungeon_seed,
    floor_passability_cells_full, generate_dungeon, passability_floor_for_depth,
};
use onlinerpg_shared::furniture::{self, FurniturePlacement};
use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::pathfinding;
use onlinerpg_shared::{WORLD_MAX_X, WORLD_MIN_X, WORLD_WIDTH_X};
use serde::Deserialize;
use tracing::{info, warn};

/// Region object file shape (`data/terrain/objects/r{rx}_{rz}.json`).
#[derive(Deserialize)]
struct RegionObjects {
    #[serde(default)]
    placements: Vec<FurniturePlacement>,
}

/// Query a short local movement sweep on both representations of the wrapped
/// X seam. The player's stored position is canonical, while a seam-crossing
/// step is deliberately left unwrapped so it remains a short segment. Shifting
/// that segment by one world width lets it see passability near the destination
/// edge as well as the source edge.
pub(super) fn wrapped_block_info<'a>(
    cache: &'a pathfinding::PassabilityCache,
    from_x: f32,
    from_z: f32,
    to_x: f32,
    to_z: f32,
    floor_level: u8,
    y: f32,
) -> Option<pathfinding::BlockInfo<'a>> {
    if let Some(info) =
        pathfinding::blocking_entry(cache, from_x, from_z, to_x, to_z, floor_level, Some(y))
    {
        return Some(info);
    }

    let seam_offset = if to_x >= WORLD_MAX_X {
        -WORLD_WIDTH_X
    } else if to_x < WORLD_MIN_X {
        WORLD_WIDTH_X
    } else {
        return None;
    };
    pathfinding::blocking_entry(
        cache,
        from_x + seam_offset,
        from_z,
        to_x + seam_offset,
        to_z,
        floor_level,
        Some(y),
    )
}

/// Cache floor index for a player, derived from the server's own position
/// rather than the floor the client reported.
///
/// `validated_dungeon_floor` waves through any non-negative floor, so a client
/// claiming floor 0 from three storeys underground would otherwise pick which
/// walls apply to it and walk straight through the dungeon. Position is
/// server-simulated, so deriving from it keeps collision authoritative.
pub(super) fn authoritative_floor(
    cache: &pathfinding::PassabilityCache,
    position: &crate::types::Position,
) -> u8 {
    pathfinding::get_floor_at_position(cache, position.x, position.z, position.y)
}

impl super::GameState {
    /// Cache guards recover from poisoning: a panic mid-update at worst
    /// leaves one stale entry, which must not take down the movement tick.
    pub(super) fn passability_read(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, pathfinding::PassabilityCache> {
        self.passability.read().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) fn passability_write(
        &self,
    ) -> std::sync::RwLockWriteGuard<'_, pathfinding::PassabilityCache> {
        self.passability.write().unwrap_or_else(|e| e.into_inner())
    }

    /// Build the boot-time cache: every house, every region's solid
    /// furniture and every dungeon layout.
    pub async fn init_passability(&self, terrain_dir: &str) {
        let houses = self.housing_io.read_all_houses().await;
        for house in &houses {
            self.passability_add_house(house).await;
        }
        let regions = self.load_region_furniture(terrain_dir).await;
        let mut dungeons = 0usize;
        for def in self.dungeon_defs.all() {
            let layouts = generate_dungeon(dungeon_seed(&def.id));
            let rp = dungeon_passability(&def.position(), &layouts);
            self.passability_write()
                .insert(dungeon_cache_key(&def.id), rp);
            dungeons += 1;
        }
        info!(
            "Passability cache ready: {} houses, {} furniture regions, {} dungeons",
            houses.len(),
            regions,
            dungeons
        );
    }

    async fn load_region_furniture(&self, terrain_dir: &str) -> usize {
        let dir = Path::new(terrain_dir).join("objects");
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
            Err(e) => {
                warn!("Failed to read region objects dir {:?}: {e}", dir);
                return 0;
            }
        };
        let mut count = 0;
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    warn!("Failed to enumerate region objects: {e}");
                    break;
                }
            };
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some((rx, rz)) = crate::housing::parse_chunk_from_id(stem) else {
                warn!("Skipping region objects file with odd name: {:?}", path);
                continue;
            };
            match tokio::fs::read_to_string(&path).await {
                Ok(content) => match serde_json::from_str::<RegionObjects>(&content) {
                    Ok(objs) => {
                        self.sync_region_furniture(rx, rz, &objs.placements);
                        count += 1;
                    }
                    Err(e) => warn!("Bad region objects file {:?}: {}", path, e),
                },
                Err(e) => warn!("Failed to read region objects {:?}: {}", path, e),
            }
        }
        count
    }

    /// Insert or replace a house's cache entry: base grids plus the door
    /// overlays persisted in its data. The in-memory open-door state for the
    /// house is reset — the incoming data is authoritative after an edit.
    pub async fn passability_add_house(&self, house: &HouseData) {
        self.clear_open_doors_for_house(&house.id).await;
        let rp = pathfinding::build_runtime_passability(house);
        let mut cache = self.passability_write();
        cache.insert(house.id.clone(), rp);
        pathfinding::apply_door_overlays(&mut cache, house);
    }

    pub async fn passability_remove_house(&self, house_id: &str) {
        self.clear_open_doors_for_house(house_id).await;
        self.passability_write().remove(house_id);
    }

    /// Mirror of the client's `passability_set_furniture` for one region:
    /// solid placements become sealed cells, empty regions clear the entry.
    pub fn sync_region_furniture(&self, rx: i32, rz: i32, placements: &[FurniturePlacement]) {
        let key = format!("furniture:{rx},{rz}");
        let mut cache = self.passability_write();
        match furniture::build_furniture_passability_for_placements(placements) {
            Some(rp) => {
                cache.insert(key, rp);
            }
            None => {
                cache.remove(&key);
            }
        }
    }

    /// Validate the map editor's region-object payload before it is persisted,
    /// returning only the fields needed by collision caching.
    pub(crate) fn parse_region_furniture(
        body: &serde_json::Value,
    ) -> Result<Vec<FurniturePlacement>, serde_json::Error> {
        RegionObjects::deserialize(body).map(|objects| objects.placements)
    }

    /// Re-derive one dungeon floor's cells from its current dynamic state:
    /// broken props open their cells, shut interior doors seal their mouth.
    /// Both route through this one call so neither clobbers the other.
    pub(super) async fn rebuild_dungeon_floor_passability(&self, entrance_id: &str, depth: u8) {
        if depth == 0 {
            return;
        }
        let cells = {
            let dungeons = self.dungeons.read().await;
            let Some(rt) = dungeons.get(entrance_id) else {
                return;
            };
            let Some(layout) = rt.layouts.get((depth - 1) as usize) else {
                return;
            };
            let broken: Vec<u32> = rt
                .broken_props
                .get(&depth)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            let closed = closed_door_segs(layout, rt.open_doors.get(&depth));
            floor_passability_cells_full(layout, &broken, &closed)
        };
        let floor_level = passability_floor_for_depth(depth);
        let mut cache = self.passability_write();
        if let Some(rp) = cache.get_mut(&dungeon_cache_key(entrance_id)) {
            if let Some(f) = rp.floors.iter_mut().find(|f| f.floor_level == floor_level) {
                f.cells = cells;
            }
        }
    }
}
