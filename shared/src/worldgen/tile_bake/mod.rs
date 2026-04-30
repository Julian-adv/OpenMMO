//! Phase 7: high-resolution tile baking.
//!
//! Consumes the low-resolution `GlobalMap` (elevation + land mask), the
//! flow/river map from Phase 4, and the road network from Phase 6, and
//! produces per-tile binary artifacts that match the runtime
//! `terrain::TerrainIO` layout:
//!
//! * 65×65 uint16 heightmap (`defaults::HEIGHTMAP_SIZE = 8,450` bytes)
//! * 64×64×4 byte V2 splatmap (`defaults::SPLATMAP_SIZE = 16,384` bytes)
//!
//! The global map lives at `meters_per_cell` m/cell (typically 8). Tile
//! vertices are 1 m apart, so each vertex sample is a bilinear interpolation
//! of 2×2 global cells plus a high-frequency detail noise term. Rivers and
//! roads are handled as world-space polylines (Chaikin-smoothed) and queried
//! by point-to-segment distance during bake. The splat layer uses a fixed
//! five-slot region palette; primary/secondary indices and the `blend` byte
//! are chosen per-cell by a priority ladder (river > road > sea > cliff >
//! alpine > slope > coast > plain). River bed wins over road so road→river
//! crossings keep their wet-pebble texture for a future bridge mesh; cliff
//! wins over alpine so a vertical marble face on a snowy peak still reads
//! as bare rock.

pub mod bridges;
mod constants;
mod context;
mod heightmap;
mod rivers_bin;
mod splatmap;

use serde::{Deserialize, Serialize};

use super::global_map::GlobalMap;
use super::vector_features::{river_segments_near_tile, segments_near_tile};

pub use constants::{
    HEIGHT_BIAS, HEIGHT_STEP, PAL_CLIFF, PAL_DIRT, PAL_GROUND, PAL_RIVER_BED, PAL_ROAD, PAL_SAND,
    PAL_SNOW, RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M, TILE_DIM, VERTS_PER_SIDE,
};
pub use context::BakeContext;
use context::MouthIsland;
pub use rivers_bin::{
    bake_rivers_binary, bucket_river_segments_by_owner, RiverSegmentBuckets, RIVER_BIN_HEADER_SIZE,
    RIVER_BIN_MAGIC, RIVER_BIN_SEGMENT_SIZE, RIVER_BIN_VERSION,
};

use constants::{
    COAST_FADE_SPAN_M, COAST_SAND_M, RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M,
    RIVER_FADE_SPAN_M, RIVER_SAND_WIDTH_MULT, ROAD_FADE_SPAN_M, ROAD_HALF_WIDTH_M,
};
use heightmap::{encode_heightmap, sample_tile_heights};
use splatmap::bake_splatmap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakedTile {
    /// Row-major uint16 heightmap (little-endian), 65×65 × 2 bytes.
    pub heightmap: Vec<u8>,
    /// Row-major V2 splatmap, 64×64 × 4 bytes.
    pub splatmap: Vec<u8>,
}

/// Bake one tile at signed tile coordinate (tx, tz).
pub fn bake_tile(map: &GlobalMap, ctx: &BakeContext, tx: i32, tz: i32) -> BakedTile {
    bake_tile_with_bridges(map, ctx, tx, tz, &[])
}

/// Like `bake_tile` but applies a per-tile bridge flatten pass to the
/// heightmap (the splatmap is unchanged — bridge piers paint through their
/// model at runtime, not via splat). Caller obtains the per-tile flatten
/// list from `bridges::group_flattens_by_tile`.
pub fn bake_tile_with_bridges(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    bridge_flattens: &[bridges::BridgeFlatten],
) -> BakedTile {
    let tile_min_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_min_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_max_x = tile_min_x + TILE_DIM as f32;
    let tile_max_z = tile_min_z + TILE_DIM as f32;

    // Margin = largest radius at which any river segment can still affect
    // this tile (carve taper or splat fade). Computed from the global
    // maxima — a tile with only narrow source streams still uses the
    // worldwide reach so neighbors agree on the shared cells.
    let max_half_width = RIVER_MAX_WIDTH_M * 0.5;
    let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
    let max_sand_half_width = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
    let river_margin = (max_half_width + max_taper).max(max_sand_half_width + RIVER_FADE_SPAN_M);
    let river_segs = river_segments_near_tile(
        &ctx.rivers_world,
        tile_min_x,
        tile_min_z,
        tile_max_x,
        tile_max_z,
        river_margin,
    );
    // `* 2.0`: the plain branch reads road distance up to HALF + FADE*2 via
    // `road_fade`. Shrinking the margin would desynchronize the fade across
    // tile boundaries.
    let road_margin = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M * 2.0;
    let road_segs = segments_near_tile(
        &ctx.roads_world,
        tile_min_x,
        tile_min_z,
        tile_max_x,
        tile_max_z,
        road_margin,
    );
    // Coast margin must reach as far as any classification branch consults
    // it: sand-dominant out to COAST_SAND_M, then a slope-dirt fade out to
    // COAST_SAND_M + COAST_FADE_SPAN_M. A cell within the fade span that
    // doesn't see the segment would otherwise reach `min_distance_to_segments
    // = INFINITY` and resolve to full-dirt, popping at the tile boundary.
    let coast_margin = COAST_SAND_M + COAST_FADE_SPAN_M;
    let coast_segs = segments_near_tile(
        &ctx.coasts_world,
        tile_min_x,
        tile_min_z,
        tile_max_x,
        tile_max_z,
        coast_margin,
    );
    // Extra margin so the heightmap-smoothing pass's 2-vertex
    // out-of-tile ring can still see any island whose bump reaches into
    // that ring — otherwise a tile edge drawn across the end of an
    // adjacent tile's island would blur against a bump-less ghost and
    // reintroduce a seam.
    const ISLAND_BLUR_MARGIN_M: f32 = 2.0;
    let mouth_islands = mouth_islands_near_tile(
        &ctx.mouth_islands,
        tile_min_x - ISLAND_BLUR_MARGIN_M,
        tile_min_z - ISLAND_BLUR_MARGIN_M,
        tile_max_x + ISLAND_BLUR_MARGIN_M,
        tile_max_z + ISLAND_BLUR_MARGIN_M,
    );

    let mut heights = sample_tile_heights(map, ctx, tx, tz, &river_segs, &mouth_islands);
    if !bridge_flattens.is_empty() {
        bridges::apply_bridge_flatten(&mut heights, tile_min_x, tile_min_z, bridge_flattens);
    }
    let heightmap = encode_heightmap(&heights);
    let splatmap = bake_splatmap(
        map,
        ctx,
        tx,
        tz,
        &heights,
        &river_segs,
        &road_segs,
        &coast_segs,
    );
    BakedTile {
        heightmap,
        splatmap,
    }
}

/// AABB-cull `MouthIsland`s against a tile's world-space bounds so the
/// per-vertex bump loop iterates only the local handful. Vertex-level
/// bumps inside `sample_elevation_m` still do their own bbox rejection.
fn mouth_islands_near_tile(
    islands: &[MouthIsland],
    tile_min_x: f32,
    tile_min_z: f32,
    tile_max_x: f32,
    tile_max_z: f32,
) -> Vec<MouthIsland> {
    islands
        .iter()
        .filter(|island| {
            let r = island.reach_m;
            let cx = island.center[0];
            let cz = island.center[1];
            cx + r >= tile_min_x
                && cx - r <= tile_max_x
                && cz + r >= tile_min_z
                && cz - r <= tile_max_z
        })
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation, rivers, roads, settlements};
    use super::constants::{HEIGHT_BIAS, HEIGHT_STEP};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;

    fn small_config() -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF_7777,
            world_size_m: 1024,
            global_res: 128,
            reference_res: 128,
            sea_ratio: 0.35,
            continent_frequency: 1.0 / 64.0,
            continent_seed_count: 3,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 1,
            continent_gap_cells: 0,
            small_island_count: 0,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            max_isthmus_width_cells: 0,
            erosion_droplet_count: 0,
            settlement_target_count: 3,
            settlement_min_spacing_cells: 10,
            settlement_inland_buffer_cells: 0,
            settlement_river_flow_threshold: 20.0,
            settlement_along_road_count: 0,
            y_border_wall_cells: 0,
            y_border_wall_height_m: 0.0,
            ..WorldGenConfig::default()
        }
    }

    fn build_context() -> (GlobalMap, BakeContext) {
        let cfg = small_config();
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut rm, 50.0, 4);
        let s = settlements::place_settlements(&map, &rm);
        let net = roads::compute_roads(&map, &s, &rm);
        let coast_polys =
            super::super::coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
        let ctx = BakeContext::new(&map, &rm, &net, &coast_polys);
        (map, ctx)
    }

    #[test]
    fn output_byte_sizes_match_terrain_io() {
        let (map, ctx) = build_context();
        let baked = bake_tile(&map, &ctx, 0, 0);
        // These constants duplicate `terrain::defaults::{HEIGHTMAP_SIZE,
        // SPLATMAP_SIZE}` on purpose — the shared crate can't depend on
        // the terrain crate, so the fixed sizes are asserted here as a
        // contract pin.
        assert_eq!(baked.heightmap.len(), VERTS_PER_SIDE * VERTS_PER_SIDE * 2);
        assert_eq!(baked.splatmap.len(), TILE_DIM * TILE_DIM * 4);
    }

    #[test]
    fn deterministic_for_same_seed() {
        let (a_map, a_ctx) = build_context();
        let (b_map, b_ctx) = build_context();
        for &(tx, tz) in &[(0, 0), (-1, 1), (3, -2)] {
            let a = bake_tile(&a_map, &a_ctx, tx, tz);
            let b = bake_tile(&b_map, &b_ctx, tx, tz);
            assert_eq!(a.heightmap, b.heightmap);
            assert_eq!(a.splatmap, b.splatmap);
        }
    }

    #[test]
    fn sea_tiles_encode_below_sea_level() {
        // Pick a tile far inside the sea (corner of the small test world) and
        // verify the uint16 values decode to negative meters.
        let (map, ctx) = build_context();
        let world_size = map.config.world_size_m as i32;
        let tile_edge = world_size / (TILE_DIM as i32) / 2 - 1;
        let baked = bake_tile(&map, &ctx, tile_edge, tile_edge);
        let mut any_below = false;
        for chunk in baked.heightmap.chunks_exact(2) {
            let v = u16::from_le_bytes([chunk[0], chunk[1]]);
            let meters = v as f32 * HEIGHT_STEP - HEIGHT_BIAS;
            if meters < 0.0 {
                any_below = true;
                break;
            }
        }
        // Not every seed puts sea at the edge, but for this config we expect
        // at least some sub-zero vertices in the ocean corner tile.
        assert!(
            any_below,
            "expected some sub-zero vertices in an offshore tile"
        );
    }

    #[test]
    fn splat_bytes_reference_valid_palette_slots() {
        let (map, ctx) = build_context();
        let baked = bake_tile(&map, &ctx, 0, 0);
        for chunk in baked.splatmap.chunks_exact(4) {
            let primary = (chunk[0] >> 4) & 0x0F;
            let secondary = chunk[0] & 0x0F;
            assert!(
                primary <= PAL_RIVER_BED,
                "primary slot {} out of palette",
                primary
            );
            assert!(
                secondary <= PAL_RIVER_BED,
                "secondary slot {} out of palette",
                secondary
            );
            // veg byte is either 0, short grass (230..=239), or tall grass
            // (240..=249). The patch field chooses tall/short per-patch, so
            // both ranges can appear in the same tile.
            let veg = chunk[3];
            assert!(
                veg == 0 || (230..=249).contains(&veg),
                "unexpected veg byte {}",
                veg
            );
        }
    }
}
