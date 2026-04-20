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
//! are chosen per-cell by a priority ladder (road > river > sea > alpine >
//! cliff > coast > plain).

use serde::{Deserialize, Serialize};

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_x, PerlinNoise3D};
use super::rivers::RiverMap;
use super::roads::RoadNetwork;
use super::vector_features::{
    chaikin_smooth, min_distance_to_segments, polyline_to_world, segments_near_tile, Segment,
    WorldPolyline,
};

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

/// Region metadata (`meta/r{rx}_{rz}.json`) the baker writes. Same palette
/// for every region so the atlas cache is shared across the world.
pub fn default_palette_meta() -> serde_json::Value {
    serde_json::json!({
        "layers": [
            { "texture": "rocky_terrain_02_1k",        "tileScale": 8.0 },
            { "texture": "sandy_gravel_02_1k",         "tileScale": 8.0 },
            { "texture": "red_laterite_soil_stones_1k","tileScale": 10.0 },
            { "texture": "snow_02_1k",                 "tileScale": 4.0 },
            { "texture": "gravel_road_1k",             "tileScale": 8.0 }
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

// --- River carve / splat ------------------------------------------------
/// Half-width (m) of the flat river-bed floor. Points within this distance of
/// the river polyline are carved to the full channel depth.
const RIVER_CARVE_HALF_WIDTH_M: f32 = 2.5;
/// Taper (m) beyond the flat floor at which the carve smoothly reaches zero.
/// Total carve radius = HALF_WIDTH + TAPER.
const RIVER_CARVE_TAPER_M: f32 = 10.0;
/// Depth (m) removed from base elevation at the river center.
const RIVER_CARVE_DEPTH_M: f32 = 2.0;
/// Half-width (m) of the sandy-bank splat band around the river center.
const RIVER_SAND_HALF_WIDTH_M: f32 = 5.0;
/// Chaikin iterations applied to each river polyline before bake. Source
/// vertices are at 8 m global-cell spacing; two rounds smooth that into a
/// visible curve at 1 m tile resolution.
const RIVER_CHAIKIN_ITERATIONS: u32 = 2;

// --- Road splat ---------------------------------------------------------
/// Half-width (m) of the pure road surface. Points within this distance of the
/// road polyline render as 100% PAL_ROAD.
const ROAD_HALF_WIDTH_M: f32 = 2.0;
/// Distance (m) past the pure-road band over which the splat fades to pure
/// GROUND. Matches the plain branch's inner edge so crossing the outer edge is
/// a weight shift, not a palette swap.
const ROAD_FADE_SPAN_M: f32 = 2.0;
const ROAD_CHAIKIN_ITERATIONS: u32 = 2;

// --- Splat classification thresholds -------------------------------------
/// Cells within this many global cells of the coast get a sand band. The
/// blend is applied with a quadratic (`t²`) curve so most of the sand
/// weight lives near the water line; sand-dominant extent ends up ~70% of
/// this width (≈7.5 m at 1.33 cells × 8 m/cell).
const COAST_SAND_CELLS: f32 = 1.33;
/// Distance (in global cells) past the sand band over which the plain
/// branch's slope-based dirt fades in from 0. Width 0 at the band edge →
/// full at `COAST_SAND_CELLS + COAST_FADE_SPAN_CELLS`. Keeps the SAND→DIRT
/// palette swap hidden (both sides 100% GROUND at the swap point).
const COAST_FADE_SPAN_CELLS: f32 = 2.0;
/// Distance (m) past the river sand band over which plain dirt fades in.
/// Matches the river carve taper so slope returns to plain baseline right
/// as the fade completes.
const RIVER_FADE_SPAN_M: f32 = 10.0;
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
    /// River polylines converted to world-space meters and Chaikin-smoothed.
    /// Tile bake queries point-to-segment distance against these instead of
    /// rasterizing them back into an 8 m mask; that preserves sub-meter
    /// precision in the final splat/height carve.
    pub rivers_world: Vec<WorldPolyline>,
    /// Road polylines, same treatment as `rivers_world`. The previous
    /// rasterized `dist_to_road` BFS exposed the 8 m cell lattice as an
    /// axis-aligned staircase along every straight road segment.
    pub roads_world: Vec<WorldPolyline>,
}

impl BakeContext {
    pub fn new(map: &GlobalMap, river_map: &RiverMap, road_net: &RoadNetwork) -> Self {
        let res = map.config.global_res as usize;

        // Coast distance fields in both directions. `land_mask == 0` is sea:
        // sources = sea → distance to sea. sources = land → distance to land.
        let dist_to_sea = bfs_distance_from(&map.land_mask, res, 0);
        let dist_to_land = bfs_distance_from(&map.land_mask, res, 1);

        let rivers_world = smooth_polylines(
            river_map.rivers.iter().map(|p| p.points.as_slice()),
            &map.config,
            RIVER_CHAIKIN_ITERATIONS,
        );
        let roads_world = smooth_polylines(
            road_net.roads.iter().map(|r| r.points.as_slice()),
            &map.config,
            ROAD_CHAIKIN_ITERATIONS,
        );

        let detail_noise = PerlinNoise3D::new(map.config.seed ^ 0xD1EA_C17E_0000_0007);

        Self {
            detail_noise,
            dist_to_sea,
            dist_to_land,
            rivers_world,
            roads_world,
        }
    }
}

/// Convert an iterator of cell-index polylines into world-space polylines,
/// splitting at the X seam and Chaikin-smoothing each resulting segment.
/// Shared between rivers and roads so both go through the exact same pipeline.
fn smooth_polylines<'a, I>(
    polylines: I,
    cfg: &super::config::WorldGenConfig,
    iterations: u32,
) -> Vec<WorldPolyline>
where
    I: IntoIterator<Item = &'a [(u32, u32)]>,
{
    let mut out: Vec<WorldPolyline> = Vec::new();
    for pts in polylines {
        for wp in polyline_to_world(pts, cfg) {
            if wp.points.len() >= 2 {
                out.push(chaikin_smooth(&wp, iterations));
            }
        }
    }
    out
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
    let tile_min_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_min_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_max_x = tile_min_x + TILE_DIM as f32;
    let tile_max_z = tile_min_z + TILE_DIM as f32;

    // Margin = largest radius at which any vector feature still affects
    // this tile. Must cover both the heightmap carve (sand + taper) and
    // the splat water-fade (sand + fade span) — whichever reaches further.
    // Cells within the fade span that don't see the segment would get
    // full-dirt instead of a partial fade, producing a sharp seam exactly
    // on the tile boundary when the river runs close to it.
    let river_margin = (RIVER_CARVE_HALF_WIDTH_M + RIVER_CARVE_TAPER_M)
        .max(RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M);
    let river_segs = segments_near_tile(
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

    let heights = sample_tile_heights(map, ctx, tx, tz, &river_segs);
    let heightmap = encode_heightmap(&heights);
    let splatmap = bake_splatmap(map, ctx, tx, tz, &heights, &river_segs, &road_segs);
    BakedTile {
        heightmap,
        splatmap,
    }
}

/// Generate the 65×65 f32 heightmap. Shared between the uint16 heightmap
/// output and the splatmap slope computation (so both read identical heights).
fn sample_tile_heights(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    river_segs: &[Segment],
) -> Vec<f32> {
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
                sample_elevation_m(map, ctx, world_x, world_z, world_size, inv_mpc, river_segs);
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
/// cells into a shallow bathymetry curve, add high-frequency detail, and
/// subtract a polyline-distance river carve.
fn sample_elevation_m(
    map: &GlobalMap,
    ctx: &BakeContext,
    world_x: f32,
    world_z: f32,
    world_size: f32,
    inv_mpc: f32,
    river_segs: &[Segment],
) -> f32 {
    // Catmull-Rom (C1-continuous bicubic) instead of bilinear here: the 8 m
    // global cells are too coarse to describe a smooth hill, and bilinear's
    // per-cell derivative jump makes isolated tall cells read as pyramidal
    // cones at 1 m tile resolution. Splat-side fields (coast distance) still
    // use bilinear; bicubic overshoot at a sharp land/sea transition would
    // distort the shoreline.
    let base = catmull_rom_wrap_x(map, world_x, world_z, world_size, inv_mpc, |i| {
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

    let river_d = min_distance_to_segments(world_x, world_z, river_segs);
    let carve = river_carve_m(river_d);

    let max_cap = map.config.max_elevation_m;
    (base + detail - carve).clamp(-HEIGHT_BIAS, max_cap)
}

/// River channel profile: flat floor within `RIVER_CARVE_HALF_WIDTH_M`, then
/// smoothstep taper to zero over the next `RIVER_CARVE_TAPER_M` meters. The
/// flat floor avoids a visible kink at the bank and gives the water surface a
/// consistent bed depth along the river.
#[inline]
fn river_carve_m(d_m: f32) -> f32 {
    let full = RIVER_CARVE_HALF_WIDTH_M;
    let total = full + RIVER_CARVE_TAPER_M;
    if d_m >= total {
        return 0.0;
    }
    if d_m <= full {
        return RIVER_CARVE_DEPTH_M;
    }
    let t = (d_m - full) / RIVER_CARVE_TAPER_M;
    let s = 1.0 - t * t * (3.0 - 2.0 * t);
    RIVER_CARVE_DEPTH_M * s
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

fn bake_splatmap(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    heights: &[f32],
    river_segs: &[Segment],
    road_segs: &[Segment],
) -> Vec<u8> {
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
            let river_d_m = min_distance_to_segments(wx, wz, river_segs);
            let road_d_m = min_distance_to_segments(wx, wz, road_segs);

            let (primary, secondary, blend, veg) =
                classify_splat(is_sea, river_d_m, road_d_m, h_center, slope, coast_d_cells);

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

/// One-axis Catmull-Rom basis at parameter `t ∈ [0, 1]` between `p1` and `p2`,
/// with `p0` and `p3` as shoulder samples. Passes through `p1` at t=0 and `p2`
/// at t=1 with matching tangents on either side, so stitching adjacent cells
/// is C1-continuous — no per-cell gradient jump.
#[inline]
fn catmull_rom_1d(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
    let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c = -0.5 * p0 + 0.5 * p2;
    let d = p1;
    ((a * t + b) * t + c) * t + d
}

/// Fractional global-cell coordinates for world position `(wx, wz)`: the
/// integer cell that contains it plus the sub-cell fractions `fx, fy ∈ [0, 1]`.
/// Y is clamped to `[0, res-1]` so top/bottom borders stay on-grid; X is
/// returned as a raw (possibly negative) `i32` since callers wrap it into the
/// cell array themselves via `rem_euclid(res)`. Shared by every fractional
/// sampler so the two must stay in lockstep — diverging on `- 0.5` or the
/// clamp between bilinear and bicubic would desync elevation from splat.
#[inline]
fn fractional_cell_coords(
    map: &GlobalMap,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
) -> (i32, i32, i32, f32, f32) {
    let res = map.config.global_res as i32;
    let res_f = res as f32;
    let gx_f = (wx + world_size * 0.5) * inv_mpc - 0.5;
    let gy_f = ((wz + world_size * 0.5) * inv_mpc - 0.5).clamp(0.0, res_f - 1.0);
    let gx0 = gx_f.floor() as i32;
    let gy0 = gy_f.floor() as i32;
    (res, gx0, gy0, gx_f - gx0 as f32, gy_f - gy0 as f32)
}

/// Catmull-Rom bicubic sample of a cell-indexed scalar field. Signature
/// matches `bilinear_wrap_x`: X wraps, Z clamps. Reads a 4×4 neighborhood
/// around the fractional position, so Y-border cells collapse shoulders
/// onto the clamped row (still smooth, just degrades toward linear near the
/// top/bottom edge of the world).
fn catmull_rom_wrap_x<F: Fn(usize) -> f32>(
    map: &GlobalMap,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
    f: F,
) -> f32 {
    let (res, gx0, gy0, fx, fy) = fractional_cell_coords(map, wx, wz, world_size, inv_mpc);
    let ix = |x: i32| x.rem_euclid(res) as usize;
    let iy = |y: i32| y.clamp(0, res - 1) as usize;
    let idx = |x: usize, y: usize| y * res as usize + x;
    let sample = |ox: i32, oy: i32| f(idx(ix(gx0 + ox), iy(gy0 + oy)));

    let mut rows = [0.0f32; 4];
    for (k, oy) in [-1i32, 0, 1, 2].into_iter().enumerate() {
        let p0 = sample(-1, oy);
        let p1 = sample(0, oy);
        let p2 = sample(1, oy);
        let p3 = sample(2, oy);
        rows[k] = catmull_rom_1d(p0, p1, p2, p3, fx);
    }
    catmull_rom_1d(rows[0], rows[1], rows[2], rows[3], fy)
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
    let (res, gx0, gy0, fx, fy) = fractional_cell_coords(map, wx, wz, world_size, inv_mpc);
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
    river_d_m: f32,
    road_d_m: f32,
    h_center: f32,
    slope: f32,
    coast_d_cells: f32,
) -> (u8, u8, u8, u8) {
    if road_d_m < ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M {
        // Roads override every biome so the network stays visible. Secondary
        // is PAL_GROUND so the fade outer edge (blend=255 → pure GROUND) meets
        // the plain branch's inner edge (also pure GROUND) without a
        // palette-swap pop.
        let t = ((road_d_m - ROAD_HALF_WIDTH_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        (PAL_ROAD, PAL_GROUND, blend, 0)
    } else if river_d_m < RIVER_SAND_HALF_WIDTH_M {
        // River bed uses DIRT (not SAND) so it shares the (GROUND, DIRT)
        // palette pair with the plain branch outside — crossing the band
        // edge is a weight shift instead of a palette swap that would
        // flicker across the boundary.
        let t = (river_d_m / RIVER_SAND_HALF_WIDTH_M).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        let rocky = (slope / SLOPE_CLIFF_START).clamp(0.0, 1.0);
        let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
        let grass_t = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
        // `* t` fades veg density from 0 at the center to plain density at
        // the edge so the short-grass mesh doesn't pop on at the boundary.
        let density = (grass_t * 9.0 * t).round().clamp(0.0, 9.0) as u8;
        let veg = if density > 0 {
            short_grass_veg(density)
        } else {
            0
        };
        (PAL_DIRT, PAL_GROUND, blend, veg)
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
        // `water_fade` is 0 at the coast/river sand-band outer edge and
        // ramps to 1 over `*_FADE_SPAN_*`, so `blend = 0` meets the sand
        // branch's `blend = 255` (both pure GROUND) with no texture pop.
        let rocky = (slope / SLOPE_CLIFF_START).clamp(0.0, 1.0);
        let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
        let grass_t = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
        let density = (grass_t * 9.0).round().clamp(0.0, 9.0) as u8;
        let veg = if density > 0 {
            short_grass_veg(density)
        } else {
            0
        };
        let coast_fade =
            ((coast_d_cells - COAST_SAND_CELLS) / COAST_FADE_SPAN_CELLS).clamp(0.0, 1.0);
        let river_fade =
            ((river_d_m - RIVER_SAND_HALF_WIDTH_M) / RIVER_FADE_SPAN_M).clamp(0.0, 1.0);
        let road_fade =
            ((road_d_m - ROAD_HALF_WIDTH_M - ROAD_FADE_SPAN_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let water_fade = coast_fade.min(river_fade).min(road_fade);
        let blend = (rocky * 180.0 * water_fade) as u8;
        (PAL_GROUND, PAL_DIRT, blend, veg)
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
                primary <= PAL_ROAD,
                "primary slot {} out of palette",
                primary
            );
            assert!(
                secondary <= PAL_ROAD,
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

    /// Flat plain far from any feature. Slope is 0 so the plain branch's
    /// rocky component is 0 and any blend value reflects only coastal fades,
    /// not terrain. Rivers and roads default to INFINITY so those branches
    /// are inactive.
    fn plain_inputs(coast_d_cells: f32) -> (bool, f32, f32, f32, f32, f32) {
        (
            false,
            f32::INFINITY,
            f32::INFINITY,
            10.0,
            0.0,
            coast_d_cells,
        )
    }

    fn call_classify(args: (bool, f32, f32, f32, f32, f32)) -> (u8, u8, u8, u8) {
        classify_splat(args.0, args.1, args.2, args.3, args.4, args.5)
    }

    #[test]
    fn coast_water_line_is_pure_sand() {
        // At coast_d=0 (adjacent to sea), the sand band must render 100% SAND
        // so it visually connects to the shoreline.
        let (p, s, blend, _) = call_classify(plain_inputs(0.0));
        assert_eq!((p, s), (PAL_SAND, PAL_GROUND));
        assert_eq!(blend, 0, "expected pure SAND primary at water line");
    }

    #[test]
    fn coast_outer_edge_meets_plain_branch_seamlessly() {
        // The design invariant that lets the palette pair swap
        // ((SAND,GROUND) → (GROUND,DIRT)) be visually invisible: at exactly
        // `coast_d_cells = COAST_SAND_CELLS`, the coast branch emits
        // blend=255 (100% GROUND secondary) and the plain branch emits
        // blend=0 (100% GROUND primary). Both sides render pure GROUND, so
        // the shader's corner-sampled bilerp sees no texture discontinuity.
        let (cp, cs, c_blend, _) = call_classify(plain_inputs(COAST_SAND_CELLS));
        assert_eq!((cp, cs), (PAL_SAND, PAL_GROUND));
        assert_eq!(c_blend, 255, "coast outer edge must be pure GROUND");

        let (pp, ps, p_blend, _) = call_classify(plain_inputs(COAST_SAND_CELLS + 1e-4));
        assert_eq!((pp, ps), (PAL_GROUND, PAL_DIRT));
        assert_eq!(p_blend, 0, "plain at band edge must be pure GROUND");
    }

    #[test]
    fn coast_blend_monotonic_across_band() {
        // Quadratic `t²` ramp: strictly non-decreasing from 0 at water to 255
        // at the outer edge. Any regression that breaks monotonicity would
        // reintroduce visible bands inside the beach.
        let steps = 16;
        let mut prev = -1i32;
        for i in 0..=steps {
            let d = COAST_SAND_CELLS * (i as f32) / (steps as f32);
            let (_, _, blend, _) = call_classify(plain_inputs(d));
            assert!(
                blend as i32 >= prev,
                "non-monotonic coast blend at d={}: {} < {}",
                d,
                blend,
                prev
            );
            prev = blend as i32;
        }
    }

    #[test]
    fn priority_road_beats_sea_and_river() {
        // Roads must always win so the settlement network stays visible.
        let (p, s, blend, _) = classify_splat(true, 1.0, 0.0, -5.0, 0.0, 0.0);
        assert_eq!((p, s), (PAL_ROAD, PAL_GROUND));
        assert_eq!(blend, 0, "road center must be 100% PAL_ROAD");
    }

    #[test]
    fn priority_river_beats_sea() {
        // River bed uses (DIRT, GROUND) not (SAND, GROUND) — this is what
        // keeps the river-edge → plain transition a weight shift rather than
        // a palette swap. Regression guard for that choice.
        let (p, s, _, _) = classify_splat(true, 0.0, f32::INFINITY, -1.0, 0.0, 0.0);
        assert_eq!((p, s), (PAL_DIRT, PAL_GROUND));
    }

    #[test]
    fn river_outer_edge_meets_plain_seamlessly() {
        // Same continuity invariant as `coast_outer_edge_...` but for rivers.
        let at_edge = classify_splat(
            false,
            RIVER_SAND_HALF_WIDTH_M - 1e-4,
            f32::INFINITY,
            10.0,
            0.0,
            100.0,
        );
        assert_eq!((at_edge.0, at_edge.1), (PAL_DIRT, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "river edge must be near-pure GROUND");

        let past_edge = classify_splat(
            false,
            RIVER_SAND_HALF_WIDTH_M + 1e-4,
            f32::INFINITY,
            10.0,
            0.0,
            100.0,
        );
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_DIRT));
        assert_eq!(
            past_edge.2, 0,
            "plain just past river edge must be pure GROUND"
        );
    }

    #[test]
    fn road_outer_edge_meets_plain_seamlessly() {
        // Continuity invariant for the road band: at the outer edge the road
        // branch must emit blend=255 (pure GROUND secondary), and the plain
        // branch just past must emit blend=0 (pure GROUND primary). If either
        // side drifts, a visible seam appears along every road.
        let at_edge = classify_splat(
            false,
            f32::INFINITY,
            ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M - 1e-4,
            10.0,
            0.0,
            100.0,
        );
        assert_eq!((at_edge.0, at_edge.1), (PAL_ROAD, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "road edge must be near-pure GROUND");

        let past_edge = classify_splat(
            false,
            f32::INFINITY,
            ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M + 1e-4,
            10.0,
            0.0,
            100.0,
        );
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_DIRT));
        assert_eq!(
            past_edge.2, 0,
            "plain just past road edge must be pure GROUND"
        );
    }

    #[test]
    fn road_blend_monotonic_across_band() {
        // The road branch's blend curve (t² over the fade region) must be
        // non-decreasing from 0 at the pure-road core to 255 at the outer
        // edge. Any regression that breaks monotonicity would resurrect
        // visible bands inside a single road. We sample strictly inside the
        // band — past the outer edge the palette pair swaps (ROAD,GROUND) →
        // (GROUND,DIRT) and raw blend bytes are no longer comparable.
        let steps = 24;
        let band_end = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M - 1e-3;
        let mut prev = -1i32;
        for i in 0..=steps {
            let d = band_end * (i as f32) / (steps as f32);
            let (primary, _, blend, _) = classify_splat(false, f32::INFINITY, d, 10.0, 0.0, 100.0);
            assert_eq!(
                primary, PAL_ROAD,
                "road branch must still be active at d={d}"
            );
            assert!(
                blend as i32 >= prev,
                "non-monotonic road blend at d={d}: {blend} < {prev}"
            );
            prev = blend as i32;
        }
    }

    #[test]
    fn road_margin_covers_plain_fade_span() {
        // At the road margin distance, the plain branch's splat output must
        // match the "no road in sight" output. Otherwise a tile whose filter
        // excludes a road segment just past its margin would render a
        // different blend than the neighbor tile that includes it — a hard
        // seam along the tile boundary.
        let road_margin = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M * 2.0;
        // Slope 0.5 < SLOPE_CLIFF_START so the plain branch fires; its rocky
        // component is non-zero so a mismatched water_fade would surface as a
        // blend diff.
        let at_margin = classify_splat(false, f32::INFINITY, road_margin, 10.0, 0.5, 100.0);
        let no_road = classify_splat(false, f32::INFINITY, f32::INFINITY, 10.0, 0.5, 100.0);
        assert_eq!(
            at_margin, no_road,
            "plain branch must match 'no road' output at road_margin"
        );
    }

    #[test]
    fn adjacent_tiles_see_same_nearby_road_segment() {
        // Same boundary regression guard as the river version but with the
        // (smaller) road margin. A road segment `road_margin - 1` m outside
        // a tile bbox must still appear in that tile's filter list so the
        // plain branch's road_fade matches across tile edges.
        use super::super::vector_features::{segments_near_tile, WorldPolyline};
        let road_margin = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M * 2.0;
        let polys = vec![WorldPolyline {
            points: vec![
                [32.0 - (road_margin - 1.0), -10.0],
                [32.0 - (road_margin - 1.0), 10.0],
            ],
        }];
        let near = segments_near_tile(&polys, 32.0, -16.0, 96.0, 16.0, road_margin);
        assert_eq!(
            near.len(),
            1,
            "tile must see road segment {} m west of its bbox",
            road_margin - 1.0
        );
    }

    #[test]
    fn river_margin_covers_water_fade_span() {
        // Regression guard: the per-tile segment filter in `bake_tile` must
        // include every segment whose distance to the tile bbox could still
        // matter to the splat water-fade (which ramps through 0 → 1 over
        // `RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M`). If the margin is
        // smaller, edge cells near a river see INFINITY distance while
        // neighbor-tile cells see a partial fade — a hard seam on the tile
        // boundary. See the margin computation in `bake_tile`.
        let river_margin = (RIVER_CARVE_HALF_WIDTH_M + RIVER_CARVE_TAPER_M)
            .max(RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M);
        assert!(
            river_margin >= RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M,
            "margin {} does not cover fade span {}",
            river_margin,
            RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M
        );
        assert!(
            river_margin >= RIVER_CARVE_HALF_WIDTH_M + RIVER_CARVE_TAPER_M,
            "margin {} does not cover carve taper {}",
            river_margin,
            RIVER_CARVE_HALF_WIDTH_M + RIVER_CARVE_TAPER_M
        );
    }

    #[test]
    fn adjacent_tiles_see_same_nearby_river_segment() {
        // Build a synthetic world with a single river polyline straddling
        // the tile boundary at x = 0 (tile 0's right edge = tile 1's left
        // edge). The per-tile filter in `bake_tile` uses
        // `segments_near_tile` with the `river_margin` constant — both
        // adjacent tiles must see the segment so their splat classification
        // agrees at the boundary.
        use super::super::vector_features::{segments_near_tile, WorldPolyline};
        let polys = vec![WorldPolyline {
            points: vec![[0.5, -10.0], [0.5, 10.0]],
        }];
        let margin = (RIVER_CARVE_HALF_WIDTH_M + RIVER_CARVE_TAPER_M)
            .max(RIVER_SAND_HALF_WIDTH_M + RIVER_FADE_SPAN_M);
        // Tile 0 covers [-32, 32] in X; tile 1 covers [32, 96]. The segment
        // at x=0.5 is inside tile 0, and 31.5 m from tile 1's left edge —
        // inside the margin (15 m) only when the segment is close enough,
        // so we place it 14 m outside tile 1 to exercise the boundary case.
        let near_tile_0 = segments_near_tile(&polys, -32.0, -16.0, 32.0, 16.0, margin);
        assert_eq!(near_tile_0.len(), 1, "tile 0 must see segment at x=0.5");
        // Now move the segment to lie `margin - 1` m west of tile 1's
        // bbox: it must still appear in tile 1's filter list.
        let polys2 = vec![WorldPolyline {
            points: vec![
                [32.0 - (margin - 1.0), -10.0],
                [32.0 - (margin - 1.0), 10.0],
            ],
        }];
        let near_tile_1 = segments_near_tile(&polys2, 32.0, -16.0, 96.0, 16.0, margin);
        assert_eq!(
            near_tile_1.len(),
            1,
            "tile 1 must see segment {} m west of its bbox",
            margin - 1.0
        );
    }

    #[test]
    fn catmull_rom_passes_through_control_points() {
        // At t=0 the basis must return p1 exactly; at t=1 it must return p2.
        // This is the property that lets adjacent cells stitch without a
        // value jump — losing it would create visible step artifacts along
        // every cell boundary.
        for (p0, p1, p2, p3) in [
            (0.0, 1.0, 2.0, 3.0),
            (-5.0, 10.0, -3.0, 7.5),
            (100.0, 100.0, 100.0, 100.0),
        ] {
            assert!((catmull_rom_1d(p0, p1, p2, p3, 0.0) - p1).abs() < 1e-5);
            assert!((catmull_rom_1d(p0, p1, p2, p3, 1.0) - p2).abs() < 1e-5);
        }
    }

    #[test]
    fn catmull_rom_preserves_constant_field() {
        // A constant 1D field must stay constant at any t — no overshoot from
        // floating-point drift in the basis coefficients.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = catmull_rom_1d(4.2, 4.2, 4.2, 4.2, t);
            assert!((v - 4.2).abs() < 1e-5, "constant field at t={t}: {v}");
        }
    }

    #[test]
    fn catmull_rom_reproduces_linear_ramp() {
        // Catmull-Rom through 4 samples of a line must reproduce the line
        // exactly (the cubic collapses to degree 1). If any basis coefficient
        // is off, a gentle slope in the global map would pick up spurious
        // wiggles at 1 m tile vertices — the opposite of what this change is
        // supposed to do.
        let a = 3.0;
        let b = 1.5;
        let (p0, p1, p2, p3) = (a - b, a, a + b, a + 2.0 * b);
        for t in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let expected = a + b * t;
            let got = catmull_rom_1d(p0, p1, p2, p3, t);
            assert!(
                (got - expected).abs() < 1e-4,
                "linear ramp at t={t}: got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn catmull_rom_basis_is_symmetric() {
        // Tension-0.5 Catmull-Rom is direction-agnostic:
        // `f(p0,p1,p2,p3,t) == f(p3,p2,p1,p0,1-t)`. The sampler feeds a splat
        // classifier that treats +X and -X the same; asymmetric basis would
        // silently bias elevation one way along world axes.
        for (p0, p1, p2, p3) in [(0.0, 1.0, 4.0, 9.0), (-3.0, 2.0, -1.0, 5.0)] {
            for t in [0.0, 0.3, 0.5, 0.7, 1.0] {
                let fwd = catmull_rom_1d(p0, p1, p2, p3, t);
                let bwd = catmull_rom_1d(p3, p2, p1, p0, 1.0 - t);
                assert!(
                    (fwd - bwd).abs() < 1e-5,
                    "asymmetric at t={t}: fwd={fwd} bwd={bwd}"
                );
            }
        }
    }

    #[test]
    fn catmull_rom_c1_continuous_across_windows() {
        // The motivation for switching from bilinear to bicubic: sliding the
        // 4-sample window by one cell must preserve the derivative at the
        // shared vertex (left window at t→1 ≡ right window at t→0). If this
        // regresses, per-cell slope jumps return and the 8 m lattice reads
        // as pyramidal hills again — the whole bug this change fixed.
        let samples = [0.0f32, 1.0, 3.0, 2.5, 4.0];
        let eps = 1e-3;
        let left = catmull_rom_1d(samples[0], samples[1], samples[2], samples[3], 1.0);
        let left_prev = catmull_rom_1d(samples[0], samples[1], samples[2], samples[3], 1.0 - eps);
        let right = catmull_rom_1d(samples[1], samples[2], samples[3], samples[4], 0.0);
        let right_next = catmull_rom_1d(samples[1], samples[2], samples[3], samples[4], eps);
        // Value continuity at the shared vertex (both = samples[3] = p2-of-left = p1-of-right).
        assert!(
            (left - right).abs() < 1e-5,
            "c0 value mismatch: {left} vs {right}"
        );
        // Derivative continuity via finite difference.
        let left_slope = (left - left_prev) / eps;
        let right_slope = (right_next - right) / eps;
        assert!(
            (left_slope - right_slope).abs() < 1e-2,
            "c1 slope mismatch: left={left_slope} right={right_slope}"
        );
    }

    #[test]
    fn palette_meta_layer_count_matches_constants() {
        let meta = default_palette_meta();
        let layers = meta
            .get("layers")
            .and_then(|l| l.as_array())
            .expect("layers array");
        assert_eq!(layers.len(), PAL_ROAD as usize + 1);
        for layer in layers {
            assert!(layer.get("texture").and_then(|t| t.as_str()).is_some());
            assert!(layer.get("tileScale").and_then(|t| t.as_f64()).is_some());
        }
    }
}
