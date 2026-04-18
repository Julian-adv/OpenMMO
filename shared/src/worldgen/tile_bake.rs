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
//! of 2×2 global cells plus a high-frequency detail noise term. The splat
//! layer uses a fixed six-slot region palette; primary/secondary indices and
//! the `blend` byte are chosen per-cell by a priority ladder (road > river >
//! sea > alpine > cliff > coast > plain).

use serde::{Deserialize, Serialize};

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_x, PerlinNoise3D};
use super::rivers::RiverMap;
use super::roads::RoadNetwork;

/// Cell-count side of the splatmap (64×64 cells per tile).
pub const TILE_DIM: usize = 64;
/// Vertex-count side of the heightmap (65×65, overlaps neighbor by 1).
pub const VERTS_PER_SIDE: usize = TILE_DIM + 1;

/// Heightmap encoding: 10000 → 0.0 m, step 0.05 m. Covers -500..+2776 m.
const HEIGHT_BIAS: f32 = 500.0;
const HEIGHT_STEP: f32 = 0.05;

/// Fixed palette slot indices used by this baker. The runtime `meta.json`
/// writer mirrors these positions via `default_palette_meta()`.
pub const PAL_GROUND: u8 = 0; // rocky_terrain_02 — general ground under grass
pub const PAL_SAND: u8 = 1; // sandy_gravel_02 — coast, river bed, shore
pub const PAL_DIRT: u8 = 2; // red_laterite — barren mid-altitude / cliffs
pub const PAL_SNOW: u8 = 3; // snow_02 — alpine peaks
pub const PAL_ROAD: u8 = 4; // gravel_road — settlement road surfaces
pub const PAL_PAVED: u8 = 5; // gravel_floor — paved squares / highlights

/// Region metadata (`meta/r{rx}_{rz}.json`) the baker writes. Same palette
/// for every region so the atlas cache is shared across the world.
pub fn default_palette_meta() -> serde_json::Value {
    serde_json::json!({
        "layers": [
            { "texture": "rocky_terrain_02_1k",        "tileScale": 8.0 },
            { "texture": "sandy_gravel_02_1k",         "tileScale": 8.0 },
            { "texture": "red_laterite_soil_stones_1k","tileScale": 10.0 },
            { "texture": "snow_02_1k",                 "tileScale": 4.0 },
            { "texture": "gravel_road_1k",             "tileScale": 8.0 },
            { "texture": "gravel_floor_1k",            "tileScale": 6.0 }
        ]
    })
}

// --- Detail noise tuning -------------------------------------------------
const DETAIL_OCTAVES: u32 = 4;
const DETAIL_LACUNARITY: f32 = 2.0;
const DETAIL_GAIN: f32 = 0.5;
/// Base frequency: cycles per meter. 1/16 = 16 m wavelength; with 4 octaves
/// the finest harmonic lands near 1 m, matching the tile vertex spacing.
const DETAIL_FREQUENCY: f32 = 1.0 / 16.0;
/// Max detail amplitude (m) on tall mountains.
const DETAIL_MAX_AMPLITUDE: f32 = 6.0;
/// Min detail amplitude (m) on lowland plains.
const DETAIL_MIN_AMPLITUDE: f32 = 0.4;

// --- Splat classification thresholds -------------------------------------
/// Cells within this many global cells of the coast get a sand band. The
/// blend is applied with a quadratic (`t²`) curve so most of the sand
/// weight lives near the water line; sand-dominant extent ends up ~70% of
/// this width (≈11 m at 2 cells × 8 m/cell).
const COAST_SAND_CELLS: f32 = 2.0;
/// Absolute elevation (m) at which the snow→rock blend starts fading in.
const SNOW_ELEVATION_M: f32 = 1800.0;
/// Elevation (m) above `SNOW_ELEVATION_M` at which snow is fully dominant.
const SNOW_FULL_SPAN_M: f32 = 400.0;
/// Slope (Δm per 1 m horizontal) at which rock starts to dominate plains.
const SLOPE_CLIFF_START: f32 = 0.9;
/// Slope (Δm per 1 m horizontal) at which rock is fully dominant.
const SLOPE_CLIFF_FULL: f32 = 2.5;
/// Max depth (m) used to map sea bathymetry blend 0..=255.
const SEA_MAX_DEPTH_FOR_BLEND: f32 = 10.0;
/// Elevation band (m) for grass-density falloff: grass thins toward this height.
const GRASS_FALLOFF_ELEVATION_M: f32 = 1600.0;

/// Precomputed per-cell fields reused across every tile bake. Building these
/// once and sharing across all ~260k tiles is the difference between a
/// minute-long bake and something unusable.
pub struct BakeContext {
    /// Deterministic detail-noise source seeded off the master seed.
    pub detail_noise: PerlinNoise3D,
    /// BFS distance from each cell to the nearest sea cell (u16 saturated).
    /// On land this is the classical "coast distance"; on sea it is zero.
    pub dist_to_sea: Vec<u16>,
    /// BFS distance from each cell to the nearest land cell. On sea this
    /// serves as an "offshore depth" driver; on land it is zero.
    pub dist_to_land: Vec<u16>,
    /// 255 = river cell (flow ≥ threshold AND land), 0 otherwise.
    pub river_mask: Vec<u8>,
    /// BFS distance from each cell to the nearest road cell.
    pub dist_to_road: Vec<u16>,
}

impl BakeContext {
    pub fn new(map: &GlobalMap, river_map: &RiverMap, road_net: &RoadNetwork) -> Self {
        let res = map.config.global_res as usize;
        let total = res * res;

        // Coast distance fields in both directions. `land_mask == 0` is sea:
        // sources = sea → distance to sea. sources = land → distance to land.
        let dist_to_sea = bfs_distance_from(&map.land_mask, res, 0);
        let dist_to_land = bfs_distance_from(&map.land_mask, res, 1);

        // River cells: rasterize only the extracted polylines. Using the
        // raw flow field would paint every micro-tributary as a river.
        let mut river_mask = vec![0u8; total];
        for poly in &river_map.rivers {
            for &(x, y) in &poly.points {
                let idx = (y as usize) * res + (x as usize);
                if map.land_mask[idx] == 1 {
                    river_mask[idx] = 255;
                }
            }
        }

        let mut road_bin = vec![0u8; total];
        for road in &road_net.roads {
            for &(x, y) in &road.points {
                road_bin[(y as usize) * res + (x as usize)] = 1;
            }
        }
        let dist_to_road = bfs_distance_from(&road_bin, res, 1);

        let detail_noise = PerlinNoise3D::new(map.config.seed ^ 0xD1EA_C17E_0000_0007);

        Self {
            detail_noise,
            dist_to_sea,
            dist_to_land,
            river_mask,
            dist_to_road,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BakedTile {
    /// Row-major uint16 heightmap (little-endian), 65×65 × 2 bytes.
    pub heightmap: Vec<u8>,
    /// Row-major V2 splatmap, 64×64 × 4 bytes.
    pub splatmap: Vec<u8>,
}

/// Bake one tile at signed tile coordinate (tx, tz).
pub fn bake_tile(map: &GlobalMap, ctx: &BakeContext, tx: i32, tz: i32) -> BakedTile {
    let heights = sample_tile_heights(map, ctx, tx, tz);
    let heightmap = encode_heightmap(&heights);
    let splatmap = bake_splatmap(map, ctx, tx, tz, &heights);
    BakedTile {
        heightmap,
        splatmap,
    }
}

/// Generate the 65×65 f32 heightmap. Shared between the uint16 heightmap
/// output and the splatmap slope computation (so both read identical heights).
fn sample_tile_heights(map: &GlobalMap, ctx: &BakeContext, tx: i32, tz: i32) -> Vec<f32> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let mut heights = vec![0.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let world_x = tile_origin_x + i as f32;
            let world_z = tile_origin_z + j as f32;
            heights[j * VERTS_PER_SIDE + i] =
                sample_elevation_m(map, ctx, world_x, world_z, world_size, inv_mpc);
        }
    }
    heights
}

fn encode_heightmap(heights: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(heights.len() * 2);
    for &h in heights {
        let v = ((h + HEIGHT_BIAS) / HEIGHT_STEP)
            .round()
            .clamp(0.0, 65535.0) as u16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Bilinear-sample the global elevation at a world position, convert sea
/// cells into a shallow bathymetry curve, and add high-frequency detail.
fn sample_elevation_m(
    map: &GlobalMap,
    ctx: &BakeContext,
    world_x: f32,
    world_z: f32,
    world_size: f32,
    inv_mpc: f32,
) -> f32 {
    let base = bilinear_wrap_x(map, world_x, world_z, world_size, inv_mpc, |i| {
        cell_elevation_m(map, ctx, i)
    });

    // Amplitude scales with relative elevation so plains stay calm and peaks
    // feel jagged. Underwater damped heavily so the water surface looks flat.
    let max_elev = map.config.max_elevation_m.max(1.0);
    let amp_t = (base.max(0.0) / max_elev).clamp(0.0, 1.0);
    let amp = DETAIL_MIN_AMPLITUDE + (DETAIL_MAX_AMPLITUDE - DETAIL_MIN_AMPLITUDE) * amp_t;
    let underwater_damp = if base < 0.0 { 0.15 } else { 1.0 };

    // Detail sampled with X-wrap so the seamless continent carries through.
    let n = fbm_wrap_x(
        &ctx.detail_noise,
        world_x + world_size * 0.5,
        world_z + world_size * 0.5,
        world_size,
        DETAIL_FREQUENCY,
        DETAIL_OCTAVES,
        DETAIL_LACUNARITY,
        DETAIL_GAIN,
    );
    let detail = n * amp * underwater_damp;

    let max_cap = map.config.max_elevation_m;
    (base + detail).clamp(-HEIGHT_BIAS, max_cap)
}

/// Map a single global cell to "effective elevation": the raw meters for
/// land, or a shallow negative bathymetry for sea (deeper offshore, capped).
fn cell_elevation_m(map: &GlobalMap, ctx: &BakeContext, i: usize) -> f32 {
    if map.land_mask[i] == 1 {
        map.elevation_m[i]
    } else {
        // Depth ramps 0.5 m at the shore up to ~10 m far offshore.
        let d = ctx.dist_to_land[i] as f32;
        -(0.5 + d.min(40.0) * 0.25)
    }
}

/// Pack one splat cell into 4 bytes following the V2 layout
/// (`doc/SPLATMAP_V2.md`).
#[inline]
fn pack_splat(primary: u8, secondary: u8, blend: u8, veg: u8) -> [u8; 4] {
    [
        ((primary & 0x0F) << 4) | (secondary & 0x0F),
        0, // reserved (byte 1)
        blend,
        veg,
    ]
}

/// Short-grass vegMeta bytes live in 230..=239; pack a 0..=9 density there.
#[inline]
fn short_grass_veg(density: u8) -> u8 {
    230 + density.min(9)
}

fn bake_splatmap(map: &GlobalMap, ctx: &BakeContext, tx: i32, tz: i32, heights: &[f32]) -> Vec<u8> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let res = cfg.global_res as usize;

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    let mut out = vec![0u8; TILE_DIM * TILE_DIM * 4];

    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            // Cell center in world units.
            let wx = tile_origin_x + cx as f32 + 0.5;
            let wz = tile_origin_z + cz as f32 + 0.5;

            // Nearest global cell for mask/field lookups.
            let gx = ((wx + world_size * 0.5) * inv_mpc).floor() as i32;
            let gy = ((wz + world_size * 0.5) * inv_mpc).floor() as i32;
            let gi =
                (gy.clamp(0, res as i32 - 1) as usize) * res + (gx.rem_euclid(res as i32) as usize);

            // Elevation at cell center = average of 4 surrounding vertices;
            // slope from the finite-difference gradient of the same quad.
            let h00 = heights[cz * VERTS_PER_SIDE + cx];
            let h10 = heights[cz * VERTS_PER_SIDE + cx + 1];
            let h01 = heights[(cz + 1) * VERTS_PER_SIDE + cx];
            let h11 = heights[(cz + 1) * VERTS_PER_SIDE + cx + 1];
            let h_center = (h00 + h10 + h01 + h11) * 0.25;
            let dzdx = ((h10 + h11) - (h00 + h01)) * 0.5;
            let dzdy = ((h01 + h11) - (h00 + h10)) * 0.5;
            let slope = (dzdx * dzdx + dzdy * dzdy).sqrt();

            let is_sea = map.land_mask[gi] == 0;
            // Bilinear + noise-jittered coast distance so the sand→grass
            // band doesn't reveal the 8 m global lattice as axis-aligned
            // staircase where d transitions from 2 to 3.
            let coast_d_cells = sample_coast_d_jittered(map, ctx, wx, wz, world_size, inv_mpc);
            let is_river = ctx.river_mask[gi] > 0;
            let on_road = ctx.dist_to_road[gi] == 0;

            let (primary, secondary, blend, veg) =
                classify_splat(is_sea, is_river, on_road, h_center, slope, coast_d_cells);

            let off = (cz * TILE_DIM + cx) * 4;
            let bytes = pack_splat(primary, secondary, blend, veg);
            out[off..off + 4].copy_from_slice(&bytes);
        }
    }

    out
}

/// Wavelength (m) of the coast-boundary jitter noise. Sub-global-cell
/// scale so the perturbation scrambles the 8 m lattice, not the continent
/// shape.
const COAST_JITTER_WAVELENGTH_M: f32 = 6.0;
/// Amplitude (global cells) of the coast-boundary jitter. Roughly ±1 cell
/// so a single splat cell can be pulled from d=1 into the sand band or
/// from d=3 out of it — breaks straight edges without drifting the overall
/// band width far from its designed 2 cells (≈16 m).
const COAST_JITTER_AMPLITUDE_CELLS: f32 = 1.0;

/// Bilinear sample the coast-distance field at an arbitrary world-space
/// position (global cells, X wraps and Y clamps), then add a fine-scale
/// fBm perturbation in cell units. Returns the jittered distance that the
/// splat classifier compares against `COAST_SAND_CELLS`.
fn sample_coast_d_jittered(
    map: &GlobalMap,
    ctx: &BakeContext,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
) -> f32 {
    let bilinear = bilinear_wrap_x(map, wx, wz, world_size, inv_mpc, |i| {
        ctx.dist_to_sea[i] as f32
    });
    // Meter-scale jitter so the bilinear pass doesn't reveal the 8 m cell
    // lattice as an axis-aligned staircase at the sand-band boundary.
    let jitter = fbm_wrap_x(
        &ctx.detail_noise,
        wx + world_size * 0.5,
        wz + world_size * 0.5,
        world_size,
        1.0 / COAST_JITTER_WAVELENGTH_M,
        3,
        2.0,
        0.5,
    ) * COAST_JITTER_AMPLITUDE_CELLS;
    (bilinear + jitter).max(0.0)
}

/// Bilinear sample a cell-indexed scalar field over the global-cell grid,
/// evaluating `f` at each corner. X wraps, Z clamps — matching the world
/// topology the rest of the worldgen pipeline assumes.
fn bilinear_wrap_x<F: Fn(usize) -> f32>(
    map: &GlobalMap,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
    f: F,
) -> f32 {
    let res = map.config.global_res as i32;
    let res_f = res as f32;
    let gx_f = (wx + world_size * 0.5) * inv_mpc - 0.5;
    let gy_f = ((wz + world_size * 0.5) * inv_mpc - 0.5).clamp(0.0, res_f - 1.0);
    let gx0 = gx_f.floor() as i32;
    let gy0 = gy_f.floor() as i32;
    let fx = gx_f - gx0 as f32;
    let fy = gy_f - gy0 as f32;
    let ix = |x: i32| x.rem_euclid(res) as usize;
    let iy = |y: i32| y.clamp(0, res - 1) as usize;
    let idx = |x: usize, y: usize| y * res as usize + x;
    let s00 = f(idx(ix(gx0), iy(gy0)));
    let s10 = f(idx(ix(gx0 + 1), iy(gy0)));
    let s01 = f(idx(ix(gx0), iy(gy0 + 1)));
    let s11 = f(idx(ix(gx0 + 1), iy(gy0 + 1)));
    let s0 = s00 + (s10 - s00) * fx;
    let s1 = s01 + (s11 - s01) * fx;
    s0 + (s1 - s0) * fy
}

/// Splat priority ladder. Later branches only fire if earlier ones reject.
fn classify_splat(
    is_sea: bool,
    is_river: bool,
    on_road: bool,
    h_center: f32,
    slope: f32,
    coast_d_cells: f32,
) -> (u8, u8, u8, u8) {
    if on_road {
        // Roads override every biome so the network is always visible.
        (PAL_ROAD, PAL_PAVED, 0, 0)
    } else if is_river {
        // River bed: sandy with ground fade at the edges (blend ≠ 0 so banks
        // read a touch greener where sampled at sub-pixel distance).
        (PAL_SAND, PAL_GROUND, 30, 0)
    } else if is_sea {
        // Secondary = GROUND so the coast line shares a palette pair with
        // the land sand-band, keeping per-texture weights continuous
        // across the shoreline (a DIRT secondary here would abruptly
        // introduce laterite on every adjacent land cell).
        let depth = (-h_center).max(0.0);
        let blend = ((depth / SEA_MAX_DEPTH_FOR_BLEND).clamp(0.0, 1.0) * 255.0) as u8;
        (PAL_SAND, PAL_GROUND, blend, 0)
    } else if h_center > SNOW_ELEVATION_M {
        // Alpine: snow with rock showing through on exposed slopes.
        let t = ((h_center - SNOW_ELEVATION_M) / SNOW_FULL_SPAN_M).clamp(0.0, 1.0);
        let rocky = (slope / SLOPE_CLIFF_FULL).clamp(0.0, 1.0);
        // Full snow = 0 blend. Secondary is ground (rock) so `blend` bumps up
        // where it's steep or just below the snow line.
        let blend = (((1.0 - t) * 120.0).max(rocky * 200.0)) as u8;
        (PAL_SNOW, PAL_GROUND, blend, 0)
    } else if slope > SLOPE_CLIFF_START {
        // Steep slopes: rocky ground with laterite showing on the steepest.
        let t =
            ((slope - SLOPE_CLIFF_START) / (SLOPE_CLIFF_FULL - SLOPE_CLIFF_START)).clamp(0.0, 1.0);
        (PAL_GROUND, PAL_DIRT, (t * 255.0) as u8, 0)
    } else if coast_d_cells <= COAST_SAND_CELLS {
        // Quadratic blend keeps the first land cell (coast BFS d ≈ 1)
        // near 100% SAND so per-texture weights stay continuous with
        // the adjacent sea cell; a linear ramp would introduce ~25%
        // GROUND on the first cell and read as a staircase. Grass
        // density has a floor of 1 so the mesh fringe matches the
        // adjacent plains' density of 9.
        const DENSITY_MIN: f32 = 1.0;
        let t = (coast_d_cells / COAST_SAND_CELLS).clamp(0.0, 1.0);
        let blend_f = t * t;
        let density = (DENSITY_MIN + (9.0 - DENSITY_MIN) * t)
            .round()
            .clamp(DENSITY_MIN, 9.0) as u8;
        (
            PAL_SAND,
            PAL_GROUND,
            (blend_f * 255.0) as u8,
            short_grass_veg(density),
        )
    } else {
        // Secondary = SAND (not DIRT) so plains share a palette pair with
        // the adjacent sand-band cells, keeping per-texture weights
        // continuous across the outer coast boundary. Grass density thins
        // with slope and altitude.
        let rocky = (slope / SLOPE_CLIFF_START).clamp(0.0, 1.0);
        let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
        let grass_t = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
        let density = (grass_t * 9.0).round().clamp(0.0, 9.0) as u8;
        let veg = if density > 0 {
            short_grass_veg(density)
        } else {
            0
        };
        (PAL_GROUND, PAL_SAND, (rocky * 180.0) as u8, veg)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation, rivers, roads, settlements};
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
        let net = roads::compute_roads(&map, &s);
        let ctx = BakeContext::new(&map, &rm, &net);
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
                primary <= PAL_PAVED,
                "primary slot {} out of palette",
                primary
            );
            assert!(
                secondary <= PAL_PAVED,
                "secondary slot {} out of palette",
                secondary
            );
            // veg byte is either 0 or a short-grass value in 230..=239.
            let veg = chunk[3];
            assert!(
                veg == 0 || (230..=239).contains(&veg),
                "unexpected veg byte {}",
                veg
            );
        }
    }

    #[test]
    fn palette_meta_has_six_layers() {
        let meta = default_palette_meta();
        let layers = meta
            .get("layers")
            .and_then(|l| l.as_array())
            .expect("layers array");
        assert_eq!(layers.len(), 6);
        for layer in layers {
            assert!(layer.get("texture").and_then(|t| t.as_str()).is_some());
            assert!(layer.get("tileScale").and_then(|t| t.as_f64()).is_some());
        }
    }
}
