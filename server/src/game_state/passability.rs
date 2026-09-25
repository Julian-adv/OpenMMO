//! Server-side passability cache: the same shared `PassabilityCache` the
//! browser (wasm) and agent-client build, fed from the server's own data
//! (housing files, region objects, dungeon layouts). `tick_player_movement`
//! checks untrusted simulated steps against it so players can't walk
//! through walls. Dungeon interior doors seal their corridor mouth while
//! shut — `interior_doors` derives the same door list (and ids) the client
//! renders, and every toggle rebuilds the floor's cells.

use crate::terrain::io::TerrainIO;
use onlinerpg_shared::dungeon::{
    dungeon_cache_key, dungeon_passability, floor_cells, generate_dungeon_for, set_floor_cells,
};
use onlinerpg_shared::furniture::{self, FurniturePlacement};
use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::pathfinding;
use onlinerpg_shared::{WORLD_MAX_X, WORLD_MIN_X, WORLD_WIDTH_X};
use onlinerpg_terrain::coords::{tile_to_region, world_to_tile};
use serde::Deserialize;
use std::collections::HashMap;
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
///
/// See `pathfinding::blocking_entry_for_mover` for why a sealed-in player is
/// let out.
pub(super) fn wrapped_block_info<'a>(
    cache: &'a pathfinding::PassabilityCache,
    from_x: f32,
    from_z: f32,
    to_x: f32,
    to_z: f32,
    floor_level: u8,
    y: f32,
) -> Option<pathfinding::BlockInfo<'a>> {
    let on_stairs = floor_level <= onlinerpg_shared::housing::MAX_FLOOR_LEVEL
        && (pathfinding::in_stairwell_span(cache, from_x, from_z, y)
            || pathfinding::in_stairwell_span(cache, to_x, to_z, y));
    let (dx, dz) = (to_x - from_x, to_z - from_z);
    let steps = if on_stairs {
        // A server tick can rise above furniture before crossing its cell edge.
        (dx.hypot(dz) / 0.1).ceil().max(1.0) as usize
    } else {
        1
    };
    let (mut x, mut z) = (from_x, from_z);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let (next_x, next_z) = (from_x + dx * t, from_z + dz * t);
        let step_y = collision_y(cache, x, z, next_x, next_z, floor_level, y);
        if let Some(info) = wrapped_block_info_at(cache, x, z, next_x, next_z, floor_level, step_y)
        {
            return Some(info);
        }
        (x, z) = (next_x, next_z);
    }
    None
}

/// Derive collision height from the floor or stair ramp; retain Y off-grid.
fn collision_y(
    cache: &pathfinding::PassabilityCache,
    from_x: f32,
    from_z: f32,
    to_x: f32,
    to_z: f32,
    floor_level: u8,
    reported: f32,
) -> f32 {
    let Some(floor_y) =
        pathfinding::supporting_floor_y(cache, from_x, from_z, to_x, to_z, floor_level, reported)
    else {
        return reported;
    };
    if pathfinding::in_stairwell_span(cache, from_x, from_z, reported) {
        return pathfinding::storey_ground_y(cache, floor_level, from_x, from_z, reported)
            .unwrap_or(reported);
    }
    floor_y
}

/// Check both seam representations at the sampled collision height.
fn wrapped_block_info_at<'a>(
    cache: &'a pathfinding::PassabilityCache,
    from_x: f32,
    from_z: f32,
    to_x: f32,
    to_z: f32,
    floor_level: u8,
    y: f32,
) -> Option<pathfinding::BlockInfo<'a>> {
    if let Some(info) = pathfinding::blocking_entry_for_mover(
        cache,
        from_x,
        from_z,
        to_x,
        to_z,
        floor_level,
        Some(y),
    ) {
        return Some(info);
    }

    let seam_offset = if to_x >= WORLD_MAX_X {
        -WORLD_WIDTH_X
    } else if to_x < WORLD_MIN_X {
        WORLD_WIDTH_X
    } else {
        return None;
    };
    pathfinding::blocking_entry_for_mover(
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
/// Underground and in a house the position is server-owned in all three axes
/// (floor changes only on stairs, Y from the storey's own height — see
/// `surface_ground_y`), so this agrees with the validated floor.
pub(super) fn authoritative_floor(
    cache: &pathfinding::PassabilityCache,
    position: &crate::types::Position,
) -> u8 {
    pathfinding::get_floor_at_position(cache, position.x, position.z, position.y)
}

impl super::GameState {
    /// Authoritative ground or floating surface Y. `ref_y` selects stacked
    /// bridge decks; unknown terrain or storeys retain the reported Y.
    pub async fn surface_ground_y(
        &self,
        floor: u8,
        to: &crate::types::Position,
        ref_y: f32,
        mount: Option<onlinerpg_shared::mount::MountKind>,
    ) -> f32 {
        if floor == 0 {
            if mount.is_some_and(onlinerpg_shared::mount::MountKind::floats) {
                let wx = onlinerpg_shared::wrap_world_x(to.x);
                if let Some((bed, depth)) = self.ground_and_depth_at(wx, to.z).await {
                    if depth > 0.0 {
                        return bed + depth;
                    }
                }
            }
            if let Some(entrance) = self.dungeon_defs.entrance_at(to.x, to.z) {
                let dungeons = self.dungeons.read().await;
                let ramp_y = dungeons.get(&entrance.id).and_then(|rt| {
                    onlinerpg_shared::dungeon::ground_y_for_floor(
                        &entrance.position(),
                        &rt.layouts,
                        floor,
                        to.x,
                        to.z,
                    )
                });
                if let Some(y) = ramp_y {
                    return y;
                }
            }
        }
        let storey_y = {
            let cache = self.passability_read();
            pathfinding::storey_ground_y(&cache, floor, to.x, to.z, to.y)
        };
        if let Some(y) = storey_y {
            return y;
        }
        if floor != 0 {
            return to.y;
        }
        let wx = onlinerpg_shared::wrap_world_x(to.x);
        if let Some(y) = bridge_deck_y(&self.bridge_decks_read(), wx, to.z, ref_y) {
            return y;
        }
        self.height_sampler
            .sample_height(wx, to.z)
            .await
            .unwrap_or(to.y)
    }

    pub(super) fn passability_read(&self) -> super::passability_snapshot::Read<'_> {
        self.passability.read()
    }

    pub(super) fn passability_write(&self) -> super::passability_snapshot::Write<'_> {
        self.passability.write()
    }

    /// Build the boot-time cache: every house, every region's solid
    /// furniture and every dungeon layout. Read failures propagate — an entry
    /// silently missing from this cache is a wall players can walk through.
    pub async fn init_passability(&self, terrain_io: &TerrainIO) -> std::io::Result<()> {
        let houses = self.housing_io.read_all_houses().await?;
        for house in &houses {
            self.passability_add_house(house).await;
        }
        let regions = self.load_region_furniture(terrain_io).await?;
        let mut dungeons = 0usize;
        for def in self.dungeon_defs.all() {
            let layouts = generate_dungeon_for(&def.id);
            self.interest_lock()
                .seed_dungeon(&def.id, &def.position(), &layouts);
            let rp = dungeon_passability(&def.position(), &layouts);
            self.passability_write()
                .insert(dungeon_cache_key(&def.id), rp);
            dungeons += 1;
        }
        // Counts are cache entries, not files scanned: most region files hold
        // only decorative objects and seal nothing.
        info!(
            "Passability cache ready: {} entries ({} houses, {} furniture regions, {} dungeons)",
            houses.len() + regions + dungeons,
            houses.len(),
            regions,
            dungeons
        );
        Ok(())
    }

    async fn load_region_furniture(&self, terrain_io: &TerrainIO) -> std::io::Result<usize> {
        let mut count = 0;
        for (rx, rz) in terrain_io.list_object_regions().await? {
            let json = terrain_io.read_object(rx, rz).await?;
            let objs = serde_json::from_value::<RegionObjects>(json).map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("bad region objects r{rx:+03}_{rz:+03}: {e}"),
                )
            })?;
            if self.sync_region_furniture(rx, rz, &objs.placements) {
                count += 1;
            }
        }
        Ok(count)
    }

    fn sync_region_bridges(&self, rx: i32, rz: i32, placements: &[FurniturePlacement]) {
        let decks = onlinerpg_shared::bridge::placed_decks(placements);
        let mut index = self.bridge_decks.write().unwrap_or_else(|e| e.into_inner());
        if decks.is_empty() {
            index.remove(&(rx, rz));
        } else {
            index.insert((rx, rz), decks);
        }
    }

    fn sync_respawn_beds(&self, rx: i32, rz: i32, placements: &[FurniturePlacement]) {
        let respawn = &crate::world_config::world_config().respawn;
        if respawn.region() != (rx, rz) {
            return;
        }
        let beds: Vec<FurniturePlacement> = respawn
            .bed_ids
            .iter()
            .filter_map(|id| placements.iter().find(|p| p.id == *id))
            .cloned()
            .collect();
        if beds.len() != respawn.bed_ids.len() {
            warn!(
                "Respawn beds: {} of {} configured ids found in r{rx:+03}_{rz:+03}",
                beds.len(),
                respawn.bed_ids.len()
            );
        }
        *self.respawn_beds.write().unwrap_or_else(|e| e.into_inner()) = beds;
    }

    pub(super) fn respawn_beds(&self) -> Vec<FurniturePlacement> {
        self.respawn_beds
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub(super) fn bridge_decks_read(&self) -> std::sync::RwLockReadGuard<'_, BridgeDeckIndex> {
        self.bridge_decks.read().unwrap_or_else(|e| e.into_inner())
    }
}

/// The 3×3 region neighbourhood around a world point.
pub(super) fn regions_around(wx: f32, z: f32) -> impl Iterator<Item = (i32, i32)> {
    let rx = tile_to_region(world_to_tile(wx));
    let rz = tile_to_region(world_to_tile(z));
    (rx - 1..=rx + 1).flat_map(move |x| (rz - 1..=rz + 1).map(move |zz| (x, zz)))
}

/// Bridge decks by owning region.
pub(super) type BridgeDeckIndex = HashMap<(i32, i32), Vec<onlinerpg_shared::bridge::PlacedDeck>>;

/// Deck-top Y for a mover at server height `ref_y` on a bridge over the
/// wrapped-x surface point, if any (`PlacedDeck::stand_y`). Decks reach at
/// most a few metres past their region's edge, so the 3×3 neighbourhood
/// covers every candidate.
pub(super) fn bridge_deck_y(index: &BridgeDeckIndex, wx: f32, z: f32, ref_y: f32) -> Option<f32> {
    regions_around(wx, z)
        .filter_map(|key| index.get(&key))
        .flatten()
        .find_map(|d| d.stand_y(wx, z, ref_y))
}

impl super::GameState {
    /// Insert or replace a house's cache entry: base grids plus the door
    /// overlays persisted in its data. The house's live door state is re-seeded
    /// from the data's `is_open` flags.
    pub async fn passability_add_house(&self, house: &HouseData) {
        self.reset_open_doors_for_house(house).await;
        self.sync_rain_shelters(house);
        let rp = pathfinding::build_runtime_passability(house);
        let mut cache = self.passability_write();
        cache.insert(house.id.clone(), rp);
        pathfinding::apply_door_overlays(&mut cache, house);
        drop(cache);
        self.publish_house(house);
    }

    pub async fn passability_remove_house(&self, house_id: &str) {
        self.remove_house_subject(house_id);
        self.clear_open_doors_for_house(house_id).await;
        self.passability_write().remove(house_id);
        self.rain_shelters
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .remove(house_id);
    }

    /// Mirror of the client's `passability_set_furniture` for one region:
    /// solid placements become sealed cells, empty regions clear the entry.
    /// Returns whether the region left an entry behind — most regions hold only
    /// decorative objects and contribute nothing.
    pub fn sync_region_furniture(
        &self,
        rx: i32,
        rz: i32,
        placements: &[FurniturePlacement],
    ) -> bool {
        {
            let mut index = self
                .interaction_furniture
                .write()
                .unwrap_or_else(|e| e.into_inner());
            if placements.is_empty() {
                index.remove(&(rx, rz));
            } else {
                index.insert((rx, rz), placements.to_vec());
            }
        }
        self.sync_region_bridges(rx, rz, placements);
        self.sync_respawn_beds(rx, rz, placements);
        self.sync_beds(rx, rz, placements);
        self.sync_dining(rx, rz, placements);
        let key = furniture::region_cache_key(rx, rz);
        let mut cache = self.passability_write();
        match furniture::build_furniture_passability_for_placements(placements) {
            Some(rp) => {
                cache.insert(key, rp);
                true
            }
            None => {
                cache.remove(&key);
                false
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

    /// Re-derive one dungeon floor's cells from its current dynamic state
    /// (shared `dungeon::floor_cells`); this is the adapter that hands it the
    /// server's own live door/prop state.
    ///
    /// The cells are computed before the passability write lock is taken:
    /// `tick_player_movement` holds that lock read-side for every moving
    /// player, so a 6400-cell rebuild under it would stall the whole tick.
    pub(super) async fn rebuild_dungeon_floor_passability(&self, entrance_id: &str, depth: u8) {
        let cells = {
            let dungeons = self.dungeons.read().await;
            let Some(rt) = dungeons.get(entrance_id) else {
                return;
            };
            let broken: Vec<u32> = rt
                .broken_props
                .get(&depth)
                .map(|s| s.iter().copied().collect())
                .unwrap_or_default();
            floor_cells(&rt.layouts, depth, &broken, rt.open_doors.get(&depth))
        };
        let Some(cells) = cells else {
            return;
        };
        set_floor_cells(&mut self.passability_write(), entrance_id, depth, cells);
    }
}

#[cfg(test)]
mod tests {
    use super::{collision_y, wrapped_block_info};
    use onlinerpg_shared::pathfinding::{
        build_furniture_passability, FurniturePiece, PassabilityCache, RuntimeFloorGrid,
        RuntimePassability, StairwellInfo,
    };

    /// One storey at y_base 0 over cells x 0..2, z 0..4, with a stairwell up to
    /// a second storey at 3.1 occupying x 0..1.
    fn two_storey_cache() -> PassabilityCache {
        let grid = |floor_level, y_base| RuntimeFloorGrid {
            floor_level,
            origin_x: 0,
            origin_z: 0,
            width: 2,
            depth: 4,
            y_base,
            wall_height: 3.0,
            cells: vec![0u8; 8],
        };
        let mut cache = PassabilityCache::new();
        cache.insert(
            "house".to_string(),
            RuntimePassability {
                house_origin_x: 0.0,
                house_origin_z: 0.0,
                min_x: 0.0,
                max_x: 2.0,
                min_z: 0.0,
                max_z: 4.0,
                floors: vec![grid(0, 0.0), grid(1, 3.1)],
                stairwells: vec![StairwellInfo {
                    local_min_x: 0,
                    local_min_z: 0,
                    local_max_x: 1,
                    local_max_z: 4,
                    lower_floor: 0,
                    upper_floor: 1,
                    along_z: true,
                    reversed: false,
                }],
                yields_to_trapped_mover: false,
                allows_projectiles: false,
                is_ground: true,
            },
        );
        cache
    }

    #[test]
    fn a_forged_height_is_pulled_down_to_the_storey_it_claims() {
        let cache = two_storey_cache();
        // Off the stairwell (x 1.5), so nothing holds the mover up: both a
        // hand's breadth over the wall tops and an absurd claim collapse to
        // the storey's own floor height.
        assert_eq!(collision_y(&cache, 1.5, 0.5, 1.5, 1.5, 0, 3.5), 0.0);
        assert_eq!(collision_y(&cache, 1.5, 0.5, 1.5, 1.5, 0, 1000.0), 0.0);
        // Keyed to the upper storey, it is held to that one instead.
        assert_eq!(collision_y(&cache, 1.5, 0.5, 1.5, 1.5, 1, 1000.0), 3.1);
        // An honest Y survives unchanged.
        assert_eq!(collision_y(&cache, 1.5, 0.5, 1.5, 1.5, 0, 0.0), 0.0);
    }

    #[test]
    fn a_climber_mid_flight_uses_the_ramp_height() {
        let cache = two_storey_cache();
        assert_eq!(collision_y(&cache, 0.5, 2.0, 0.5, 2.5, 0, 2.0), 1.55);
        // Above the flight entirely: no flight to be on.
        assert_eq!(collision_y(&cache, 0.5, 0.5, 0.5, 1.5, 0, 1000.0), 0.0);
    }

    #[test]
    fn a_climber_clears_furniture_after_rising_during_a_step() {
        let mut cache = two_storey_cache();
        let furniture = build_furniture_passability(&[FurniturePiece {
            cells: vec![(0, 2)],
            floor_level: 0,
            y_base: 0.0,
            wall_height: 1.0,
        }])
        .unwrap();
        cache.insert("furniture".into(), furniture);

        assert!(wrapped_block_info(&cache, 0.5, 1.4, 0.5, 2.3, 0, 0.9).is_none());
        assert!(wrapped_block_info(&cache, 0.5, 2.3, 0.5, 1.4, 0, 1.9).is_none());
    }

    #[test]
    fn stair_furniture_still_blocks_before_the_climber_rises_above_it() {
        let mut cache = two_storey_cache();
        let furniture = build_furniture_passability(&[FurniturePiece {
            cells: vec![(0, 1)],
            floor_level: 0,
            y_base: 0.0,
            wall_height: 1.0,
        }])
        .unwrap();
        cache.insert("furniture".into(), furniture);

        assert!(wrapped_block_info(&cache, 0.5, 0.5, 0.5, 2.3, 0, 0.0).is_some());
    }

    #[test]
    fn a_leg_crossing_no_floor_keeps_its_reported_height() {
        let cache = two_storey_cache();
        assert_eq!(collision_y(&cache, 50.0, 50.0, 51.0, 50.0, 0, 7.0), 7.0);
    }
}
