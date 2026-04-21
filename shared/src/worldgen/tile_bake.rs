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
//! are chosen per-cell by a priority ladder (road > river > sea > cliff >
//! alpine > slope > coast > plain). Cliff wins over alpine so a vertical
//! marble face on a snowy peak still reads as bare rock.

use serde::{Deserialize, Serialize};

use super::coasts::CoastPolyline;
use super::global_map::GlobalMap;
use super::grass_patches::{GrassPatchField, PatchSample};
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_x, smoothstep, PerlinNoise3D};
use super::rivers::RiverMap;
use super::roads::RoadNetwork;
use super::vector_features::{
    cell_coord_passthrough, cell_index_to_center, chaikin_smooth, min_distance_to_segments,
    nearest_river_segment, polyline_to_world, river_chaikin_smooth, river_polyline_to_world,
    river_segments_near_tile, segments_near_tile, RiverSegment, RiverWorldPolyline, Segment,
    WorldPolyline,
};

/// Cell-count side of the splatmap (64×64 cells per tile).
pub const TILE_DIM: usize = 64;
/// Vertex-count side of the heightmap (65×65, overlaps neighbor by 1).
pub const VERTS_PER_SIDE: usize = TILE_DIM + 1;

/// Heightmap encoding: 10000 → 0.0 m, step 0.05 m. Covers -500..+2776 m.
const HEIGHT_BIAS: f32 = 500.0;
const HEIGHT_STEP: f32 = 0.05;

/// Fixed palette slot indices used by this baker. Must match slot order in
/// `shared/palette.json`.
pub const PAL_GROUND: u8 = 0; // rocky_terrain_02 — general ground under grass
pub const PAL_SAND: u8 = 1; // sandy_gravel_02 — coast, river bed, shore
pub const PAL_DIRT: u8 = 2; // red_laterite — barren mid-altitude, gentle cliff base
pub const PAL_SNOW: u8 = 3; // snow_02 — alpine peaks
pub const PAL_ROAD: u8 = 4; // gravel_road — settlement road surfaces
pub const PAL_CLIFF: u8 = 5; // rocky_trail — exposed rock face on ≥45° slopes
pub const PAL_RIVER_BED: u8 = 6; // ganges_river_pebbles — wet rocky river bottom

/// Source of truth for the global terrain palette. Each slot: `texture`
/// (GLB under `client/public/textures/`), `tileScale` (m per repeat),
/// `minimapColor` (RGB 0..=255). The Rust baker only needs to know slot
/// order (via `PAL_*` constants) — the actual fields are consumed by the
/// client at bundle time. The embed here is just to keep the test below
/// honest about this file's schema.
#[cfg(test)]
const PALETTE_JSON: &str = include_str!("../../palette.json");

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

// --- Rolling hills layer -------------------------------------------------
// Universal hills applied to every land vertex, independent of the Phase 2
// plain/mountain classification. Lives in Phase 7 rather than Phase 2
// because Phase 3 erosion's 24 m brush blurs 60 m-wavelength features into
// flat plateaus before they ever reach the tile baker.
const HILLS_OCTAVES: u32 = 3;
const HILLS_GAIN: f32 = 0.5;
const HILLS_FREQUENCY: f32 = 1.0 / 60.0;
const HILLS_AMPLITUDE_M: f32 = 5.0;
/// Base elevation (m) over which the hills amplitude fades in from 0 to full.
/// At base = 0 m (sea level) the hills are zero, ramping linearly to full
/// amplitude at `HILLS_COASTAL_FADE_M`. Prevents the symmetric hills noise
/// from pulling coastal lowlands below sea level and creating lagoons /
/// standing-water pockets inland of the shoreline.
const HILLS_COASTAL_FADE_M: f32 = 3.0;

// --- River carve / splat ------------------------------------------------
// Width, taper, and carve depth all grow linearly in `flow_norm ∈ [0, 1]`.
// See RIVER_SYSTEM.md §2.4 / §2.5.
pub const RIVER_MIN_WIDTH_M: f32 = 1.5;
pub const RIVER_MAX_WIDTH_M: f32 = 10.0;
const RIVER_CARVE_TAPER_MIN_M: f32 = 3.0;
const RIVER_CARVE_TAPER_EXTRA_M: f32 = 7.0;
const RIVER_CARVE_DEPTH_MIN_M: f32 = 0.6;
const RIVER_CARVE_DEPTH_EXTRA_M: f32 = 1.4;
const RIVER_SAND_WIDTH_MULT: f32 = 1.25;
/// Two rounds smooth 8 m source vertices into a visible curve at 1 m tile
/// resolution.
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
/// Distance (m) from the coast polyline within which a land cell renders as
/// the sand band. Replaces the prior `COAST_SAND_CELLS = 1.33 cells × 8 m =
/// 10.67 m` threshold; equivalent radius, no longer locked to the 8 m
/// global-cell lattice so the sand line follows the smoothed polyline at
/// sub-meter precision.
const COAST_SAND_M: f32 = 10.0;
/// Distance (m) past the sand band over which the plain branch's slope-based
/// dirt fades in from 0. Width 0 at the band edge → full at `COAST_SAND_M +
/// COAST_FADE_SPAN_M`. Keeps the SAND→DIRT palette swap hidden (both sides
/// 100% GROUND at the swap point).
const COAST_FADE_SPAN_M: f32 = 16.0;
/// Chaikin iterations applied to each coast polyline. Marching-squares
/// emits axis-aligned segments at 8 m cell spacing; two rounds soften
/// those into a curve at 1 m tile resolution, matching rivers/roads.
const COAST_CHAIKIN_ITERATIONS: u32 = 2;
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
/// Slope at which bare marble cliff (PAL_CLIFF) takes over as primary. 1.0 ≈
/// tan(45°). Placed before alpine in the priority ladder, so a vertical face
/// on a snowy peak reads as rock rather than snow.
const CLIFF_SLOPE_THRESHOLD: f32 = 1.0;
/// Slope at which non-cliff land cells start tinting with CLIFF as their
/// secondary (secondary path for isolated steep ridges that don't cross the
/// cliff-primary threshold). Fade spans ≈ 35°→45°.
const CLIFF_FADE_START: f32 = 0.7;
/// Reach (m) of the cliff-proximity influence on non-cliff cells. Beyond
/// this the cliff texture contributes nothing.
const CLIFF_PROXIMITY_RADIUS_M: f32 = 5.0;
/// "Core" distance (m) within which non-cliff cells still render as 100%
/// cliff texture. The distance grid is quantized at 1 m so cells adjacent
/// to the cliff sit at d ≈ 1 — without this core zone a linear/smoothstep
/// falloff at d = 1 gives only ~75% cliff, which reads as a visible step
/// against the cliff-primary branch's 100%. 1.5 m covers the 8-way
/// neighborhood (diagonal ≈ 1.41 m) with a little slack.
const CLIFF_BLEND_CORE_M: f32 = 1.5;
/// Per-tile search radius (cells) for the nearest cliff when computing
/// proximity. Covers `CLIFF_PROXIMITY_RADIUS_M` plus a diagonal cell of
/// slack so boundary cells along diagonals still resolve correctly.
const CLIFF_PROXIMITY_SEARCH_CELLS: i32 = 6;
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
    /// Warped-Voronoi patch field that gates grass coverage. Each seed claims
    /// a circular territory (~22 m radius, jittered) with a per-patch tall/
    /// short flag; a domain warp gives the territories organic shapes. Cells
    /// outside every patch render as bare ground — the previous fBm+threshold
    /// mask produced near-uniform coverage even at tight thresholds because
    /// low-freq Perlin rarely dips far below zero.
    pub grass_patches: GrassPatchField,
    /// BFS distance from each cell to the nearest land cell. On sea this
    /// drives the offshore bathymetry curve; on land it is zero. Kept on
    /// the cell grid because the catmull-rom elevation sampler reads its
    /// 4×4 neighborhood per cell, not per world position — recomputing the
    /// distance per call against the coast polylines would dominate bake
    /// time.
    pub dist_to_land: Vec<u16>,
    /// River polylines in world-space meters, Chaikin-smoothed, with
    /// per-vertex flow_norm + width attached. `nearest_river_segment`
    /// interpolates width / flow / carve params at the exact projection
    /// point so geometry grows from source to mouth without lattice
    /// artifacts.
    pub rivers_world: Vec<RiverWorldPolyline>,
    /// Road polylines, same treatment as `rivers_world`. The previous
    /// rasterized `dist_to_road` BFS exposed the 8 m cell lattice as an
    /// axis-aligned staircase along every straight road segment.
    pub roads_world: Vec<WorldPolyline>,
    /// Coast polylines (output of marching squares + Chaikin smoothing) in
    /// world-space meters. The splat classifier queries point-to-segment
    /// distance against these to draw the sand band, replacing the prior
    /// bilinear-sampled `dist_to_sea` field whose 8 m lattice showed
    /// through as axis-aligned staircase artifacts at the shoreline.
    pub coasts_world: Vec<WorldPolyline>,
}

impl BakeContext {
    pub fn new(
        map: &GlobalMap,
        river_map: &RiverMap,
        road_net: &RoadNetwork,
        coasts: &[CoastPolyline],
    ) -> Self {
        let res = map.config.global_res as usize;

        // Bathymetry needs cell-granularity distance from sea cells to
        // their nearest land. Kept as a BFS field rather than a polyline
        // query because cell_elevation_m is called O(16 × 65² × n_tiles)
        // times during baking.
        let dist_to_land = bfs_distance_from(&map.land_mask, res, 1);

        let rivers_world = smooth_river_polylines(river_map, &map.config, RIVER_CHAIKIN_ITERATIONS);
        let roads_world = smooth_polylines(
            road_net.roads.iter().map(|r| r.points.as_slice()),
            &map.config,
            ROAD_CHAIKIN_ITERATIONS,
            cell_index_to_center,
        );
        let coasts_world = smooth_polylines(
            coasts.iter().map(|c| c.points.as_slice()),
            &map.config,
            COAST_CHAIKIN_ITERATIONS,
            cell_coord_passthrough,
        );

        let detail_noise = PerlinNoise3D::new(map.config.seed ^ 0xD1EA_C17E_0000_0007);
        let grass_patches = GrassPatchField::new(map.config.seed, map.config.world_size_m as f32);

        Self {
            detail_noise,
            grass_patches,
            dist_to_land,
            rivers_world,
            roads_world,
            coasts_world,
        }
    }
}

/// Convert an iterator of cell-coord polylines into world-space polylines,
/// splitting at the X seam and Chaikin-smoothing each resulting piece.
/// `to_cell` maps each input vertex to its cell-coord position (see
/// `vector_features::polyline_to_world`); pass `cell_index_to_center` for
/// `(u32, u32)` rivers/roads, `cell_coord_passthrough` for `[f32; 2]`
/// coasts.
fn smooth_polylines<'a, P, I, F>(
    polylines: I,
    cfg: &super::config::WorldGenConfig,
    iterations: u32,
    to_cell: F,
) -> Vec<WorldPolyline>
where
    P: 'a,
    I: IntoIterator<Item = &'a [P]>,
    F: Fn(&P) -> [f32; 2] + Copy,
{
    let mut out: Vec<WorldPolyline> = Vec::new();
    for pts in polylines {
        for wp in polyline_to_world(pts, cfg, to_cell) {
            if wp.points.len() >= 2 {
                out.push(chaikin_smooth(&wp, iterations));
            }
        }
    }
    out
}

/// River version of `smooth_polylines` that carries per-vertex flow/width
/// through the seam-split + Chaikin pass.
fn smooth_river_polylines(
    river_map: &RiverMap,
    cfg: &super::config::WorldGenConfig,
    iterations: u32,
) -> Vec<RiverWorldPolyline> {
    let max_flow = river_map.max_flow();
    let mut out: Vec<RiverWorldPolyline> = Vec::new();
    for poly in &river_map.rivers {
        let worlds = river_polyline_to_world(
            &poly.points,
            &poly.flow,
            max_flow,
            RIVER_MIN_WIDTH_M,
            RIVER_MAX_WIDTH_M,
            cfg,
        );
        for wp in worlds {
            if wp.points.len() >= 2 {
                out.push(river_chaikin_smooth(&wp, iterations));
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

/// River segments keyed by their midpoint-owning tile. Built once per bake
/// (amortizing the world-wide scan across all tiles); segments in the same
/// polyline share endpoints so neighboring tiles' ribbons still meet even
/// when they own different halves.
pub type RiverSegmentBuckets = std::collections::HashMap<(i32, i32), Vec<RiverSegment>>;

pub fn bucket_river_segments_by_owner(ctx: &BakeContext) -> RiverSegmentBuckets {
    let mut buckets: RiverSegmentBuckets = RiverSegmentBuckets::new();
    for poly in &ctx.rivers_world {
        if poly.points.len() < 2 {
            continue;
        }
        for i in 0..poly.points.len() - 1 {
            let a = poly.points[i];
            let b = poly.points[i + 1];
            let mx = (a[0] + b[0]) * 0.5;
            let mz = (a[1] + b[1]) * 0.5;
            // World → tile: tile 0 spans [-32, 32), tile 1 [32, 96), …
            let tx = ((mx + TILE_DIM as f32 * 0.5) / TILE_DIM as f32).floor() as i32;
            let tz = ((mz + TILE_DIM as f32 * 0.5) / TILE_DIM as f32).floor() as i32;
            buckets.entry((tx, tz)).or_default().push(RiverSegment {
                ax: a[0],
                az: a[1],
                bx: b[0],
                bz: b[1],
                flow_norm_a: poly.flow_norm[i],
                flow_norm_b: poly.flow_norm[i + 1],
                width_a: poly.width[i],
                width_b: poly.width[i + 1],
            });
        }
    }
    buckets
}

pub const RIVER_BIN_MAGIC: &[u8; 4] = b"RIV1";
pub const RIVER_BIN_VERSION: u16 = 1;
pub const RIVER_BIN_HEADER_SIZE: usize = 16;
pub const RIVER_BIN_SEGMENT_SIZE: usize = 32;

/// Serialize a per-tile river segment list to the on-disk binary format
/// documented in `RIVER_SYSTEM.md` §2.2. Returns `None` when the segment
/// list is empty so callers can skip writing a file for tiles with no
/// rivers (matches the vegetation pattern — missing file = none).
///
/// Layout:
///
/// ```text
/// header (16 bytes):
///   bytes 0..4   magic        b"RIV1"
///   bytes 4..6   u16          version (currently 1)
///   bytes 6..8   u16          segment_count
///   bytes 8..12  f32          reserved (0.0)
///   bytes 12..16 f32          reserved (0.0)
///
/// per-segment (32 bytes):
///   bytes  0..4   f32          ax  — world-space start x
///   bytes  4..8   f32          az  — world-space start z
///   bytes  8..12  f32          bx  — world-space end x
///   bytes 12..16  f32          bz  — world-space end z
///   bytes 16..20  f32          width_a      — surface width at vertex A
///   bytes 20..24  f32          width_b      — surface width at vertex B
///   bytes 24..28  f32          flow_norm_a  — normalized flow at A
///   bytes 28..32  f32          flow_norm_b  — normalized flow at B
/// ```
pub fn bake_rivers_binary(segments: &[RiverSegment]) -> Option<Vec<u8>> {
    if segments.is_empty() {
        return None;
    }
    let n = segments.len();
    assert!(
        n <= u16::MAX as usize,
        "river segment count {n} exceeds u16 capacity"
    );
    let mut out = Vec::with_capacity(RIVER_BIN_HEADER_SIZE + n * RIVER_BIN_SEGMENT_SIZE);
    out.extend_from_slice(RIVER_BIN_MAGIC);
    out.extend_from_slice(&RIVER_BIN_VERSION.to_le_bytes());
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    for s in segments {
        out.extend_from_slice(&s.ax.to_le_bytes());
        out.extend_from_slice(&s.az.to_le_bytes());
        out.extend_from_slice(&s.bx.to_le_bytes());
        out.extend_from_slice(&s.bz.to_le_bytes());
        out.extend_from_slice(&s.width_a.to_le_bytes());
        out.extend_from_slice(&s.width_b.to_le_bytes());
        out.extend_from_slice(&s.flow_norm_a.to_le_bytes());
        out.extend_from_slice(&s.flow_norm_b.to_le_bytes());
    }
    Some(out)
}

/// Bake one tile at signed tile coordinate (tx, tz).
pub fn bake_tile(map: &GlobalMap, ctx: &BakeContext, tx: i32, tz: i32) -> BakedTile {
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

    let heights = sample_tile_heights(map, ctx, tx, tz, &river_segs);
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

/// Generate the 65×65 f32 heightmap. Shared between the uint16 heightmap
/// output and the splatmap slope computation (so both read identical heights).
fn sample_tile_heights(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    river_segs: &[RiverSegment],
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
    river_segs: &[RiverSegment],
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

    // Universal rolling hills, land only — bathymetry should stay flat.
    // Amplitude fades in over the first `HILLS_COASTAL_FADE_M` meters of base
    // elevation so the symmetric noise can't pull 1-2 m coastal land below
    // sea level and trap water in lagoons inland of the shoreline.
    let hills = if base >= 0.0 {
        let hn = fbm_wrap_x(
            &ctx.detail_noise,
            world_x + world_size * 0.5,
            world_z + world_size * 0.5,
            world_size,
            HILLS_FREQUENCY,
            HILLS_OCTAVES,
            DETAIL_LACUNARITY,
            HILLS_GAIN,
        );
        let coastal_damp = (base / HILLS_COASTAL_FADE_M).clamp(0.0, 1.0);
        hn * HILLS_AMPLITUDE_M * coastal_damp
    } else {
        0.0
    };

    let carve = if let Some((d, idx, t)) = nearest_river_segment(world_x, world_z, river_segs) {
        let seg = &river_segs[idx];
        let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
        let width = lerp(seg.width_a, seg.width_b, t);
        let (half_width, taper, depth) = segment_carve_params(flow_norm, width);
        river_carve_m(d, half_width, taper, depth)
    } else {
        0.0
    };

    let max_cap = map.config.max_elevation_m;
    (base + detail + hills - carve).clamp(-HEIGHT_BIAS, max_cap)
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Carve geometry at a point on the river: `(half_width, taper, depth)`.
/// Flat bed matches the visible ribbon (`half_width = width * 0.5`) so the
/// water surface sits on a consistent floor. Taper and depth grow linearly
/// in flow so sources are shallow and mouths cut deeper. See
/// RIVER_SYSTEM.md §2.5.
#[inline]
fn segment_carve_params(flow_norm: f32, width_m: f32) -> (f32, f32, f32) {
    let half_width = width_m * 0.5;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let depth = RIVER_CARVE_DEPTH_MIN_M + RIVER_CARVE_DEPTH_EXTRA_M * flow_norm;
    (half_width, taper, depth)
}

/// River channel profile: flat floor within `half_width`, smoothstep taper
/// to zero over the next `taper` meters. Flat floor avoids a kink at the
/// bank.
#[inline]
fn river_carve_m(d_m: f32, half_width: f32, taper: f32, depth: f32) -> f32 {
    let total = half_width + taper;
    if d_m >= total {
        return 0.0;
    }
    if d_m <= half_width {
        return depth;
    }
    let t = (d_m - half_width) / taper.max(1e-3);
    let s = 1.0 - t * t * (3.0 - 2.0 * t);
    depth * s
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

/// Tall-grass vegMeta bytes live in 240..=249; pack a 0..=9 density there.
#[inline]
fn tall_grass_veg(density: u8) -> u8 {
    240 + density.min(9)
}

/// Euclidean distance (cells ≡ meters) from cell `(cx, cz)` to the nearest
/// `true` cell in the tile's cliff mask, clamped to
/// `CLIFF_PROXIMITY_RADIUS_M` when the search box contains no cliff. O(1)
/// per cell since the search box is bounded.
fn nearest_cliff_distance(mask: &[bool], cx: usize, cz: usize) -> f32 {
    let r = CLIFF_PROXIMITY_SEARCH_CELLS;
    let mut best_sq = (CLIFF_PROXIMITY_RADIUS_M * CLIFF_PROXIMITY_RADIUS_M) + 1.0;
    let x0 = (cx as i32 - r).max(0);
    let z0 = (cz as i32 - r).max(0);
    let x1 = (cx as i32 + r).min(TILE_DIM as i32 - 1);
    let z1 = (cz as i32 + r).min(TILE_DIM as i32 - 1);
    for nz in z0..=z1 {
        for nx in x0..=x1 {
            if !mask[nz as usize * TILE_DIM + nx as usize] {
                continue;
            }
            let dx = nx - cx as i32;
            let dz = nz - cz as i32;
            let d_sq = (dx * dx + dz * dz) as f32;
            if d_sq < best_sq {
                best_sq = d_sq;
            }
        }
    }
    best_sq.sqrt().min(CLIFF_PROXIMITY_RADIUS_M)
}

fn bake_splatmap(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    heights: &[f32],
    river_segs: &[RiverSegment],
    road_segs: &[Segment],
    coast_segs: &[Segment],
) -> Vec<u8> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let res = cfg.global_res as usize;

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    let mut out = vec![0u8; TILE_DIM * TILE_DIM * 4];

    // --- Pass 1: per-cell slope from the 4 tile vertices, plus the cliff
    // mask. Edge softness is carried entirely by pass 2's proximity blend,
    // so the mask stays faithful to the actually-steep terrain. -----------
    let mut slope_grid = vec![0.0f32; TILE_DIM * TILE_DIM];
    let mut cliff_mask = vec![false; TILE_DIM * TILE_DIM];
    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            let h00 = heights[cz * VERTS_PER_SIDE + cx];
            let h10 = heights[cz * VERTS_PER_SIDE + cx + 1];
            let h01 = heights[(cz + 1) * VERTS_PER_SIDE + cx];
            let h11 = heights[(cz + 1) * VERTS_PER_SIDE + cx + 1];
            let dzdx = ((h10 + h11) - (h00 + h01)) * 0.5;
            let dzdy = ((h01 + h11) - (h00 + h10)) * 0.5;
            let slope = (dzdx * dzdx + dzdy * dzdy).sqrt();
            let idx = cz * TILE_DIM + cx;
            slope_grid[idx] = slope;
            cliff_mask[idx] = slope >= CLIFF_SLOPE_THRESHOLD;
        }
    }

    // --- Pass 2: classification. For non-cliff cells, find distance to the
    // nearest cliff cell within `CLIFF_PROXIMITY_SEARCH_CELLS` and fold that
    // into the plain branch's blend so the cliff texture bleeds out by
    // `CLIFF_PROXIMITY_RADIUS_M` meters. -----------------------------------
    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            let wx = tile_origin_x + cx as f32 + 0.5;
            let wz = tile_origin_z + cz as f32 + 0.5;

            let gx = ((wx + world_size * 0.5) * inv_mpc).floor() as i32;
            let gy = ((wz + world_size * 0.5) * inv_mpc).floor() as i32;
            let gi =
                (gy.clamp(0, res as i32 - 1) as usize) * res + (gx.rem_euclid(res as i32) as usize);

            let h00 = heights[cz * VERTS_PER_SIDE + cx];
            let h10 = heights[cz * VERTS_PER_SIDE + cx + 1];
            let h01 = heights[(cz + 1) * VERTS_PER_SIDE + cx];
            let h11 = heights[(cz + 1) * VERTS_PER_SIDE + cx + 1];
            let h_center = (h00 + h10 + h01 + h11) * 0.25;

            let idx = cz * TILE_DIM + cx;
            let slope = slope_grid[idx];
            // 0.0 at cliff boundary (inclusive), CLIFF_PROXIMITY_RADIUS_M for
            // cells with no cliff in sight. Neighbor-tile cliffs aren't
            // visible — proximity near a tile edge stops at the edge, which
            // manifests as a slight softness asymmetry across seams. Fine at
            // the current scale.
            let cliff_proximity_m = if cliff_mask[idx] {
                0.0
            } else {
                nearest_cliff_distance(&cliff_mask, cx, cz)
            };

            let (river_d_m, river_width_m) = nearest_river_segment(wx, wz, river_segs)
                .map(|(d, idx, t)| {
                    let s = &river_segs[idx];
                    (d, lerp(s.width_a, s.width_b, t))
                })
                .unwrap_or((f32::INFINITY, 0.0));

            // `classify_splat` only consumes the patch sample in its plain
            // branch; passing a closure skips the warp + Voronoi query on
            // every sea / road / river / cliff / alpine / coast cell.
            let (primary, secondary, blend, veg) = classify_splat(
                SplatInputs {
                    is_sea: map.land_mask[gi] == 0,
                    river_d_m,
                    river_width_m,
                    road_d_m: min_distance_to_segments(wx, wz, road_segs),
                    h_center,
                    slope,
                    coast_d_m: min_distance_to_segments(wx, wz, coast_segs),
                    cliff_proximity_m,
                },
                || ctx.grass_patches.sample(wx, wz),
            );

            let off = (cz * TILE_DIM + cx) * 4;
            let bytes = pack_splat(primary, secondary, blend, veg);
            out[off..off + 4].copy_from_slice(&bytes);
        }
    }

    out
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

/// Catmull-Rom bicubic sample of a cell-indexed scalar field. X wraps,
/// Z clamps. Reads a 4×4 neighborhood around the fractional position, so
/// Y-border cells collapse shoulders onto the clamped row (still smooth,
/// degrades toward linear near the top/bottom edge of the world).
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

/// Per-cell inputs to `classify_splat`. Grouped so the priority ladder
/// doesn't become a 9-positional-arg call; also lets new biome factors
/// land as struct fields without churning every test call site.
#[derive(Debug, Clone, Copy)]
struct SplatInputs {
    is_sea: bool,
    river_d_m: f32,
    /// Width of the nearest river at its projection point (m). Zero when
    /// no river is in range — the priority ladder falls through naturally.
    river_width_m: f32,
    road_d_m: f32,
    h_center: f32,
    slope: f32,
    coast_d_m: f32,
    cliff_proximity_m: f32,
}

/// Splat priority ladder. Later branches only fire if earlier ones reject.
/// `patch` is invoked only when the plain branch fires, so non-plain cells
/// skip the warped-Voronoi query entirely.
fn classify_splat(inputs: SplatInputs, patch: impl FnOnce() -> PatchSample) -> (u8, u8, u8, u8) {
    let SplatInputs {
        is_sea,
        river_d_m,
        river_width_m,
        road_d_m,
        h_center,
        slope,
        coast_d_m,
        cliff_proximity_m,
    } = inputs;
    // 1 m floor on the sand band protects degenerate zero-width segments
    // from disappearing entirely.
    let river_sand_half_width_m = (river_width_m * RIVER_SAND_WIDTH_MULT).max(1.0);
    if road_d_m < ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M {
        // Roads override every biome so the network stays visible. Secondary
        // is PAL_GROUND so the fade outer edge (blend=255 → pure GROUND) meets
        // the plain branch's inner edge (also pure GROUND) without a
        // palette-swap pop.
        let t = ((road_d_m - ROAD_HALF_WIDTH_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        (PAL_ROAD, PAL_GROUND, blend, 0)
    } else if river_d_m < river_sand_half_width_m {
        // River bed: wet pebbles (PAL_RIVER_BED). Secondary switches to CLIFF
        // inside the cliff-proximity reach so the river-edge outer ring
        // resolves to 100% CLIFF on both sides of the palette-pair swap
        // against the adjacent plain cell; otherwise GROUND, meeting pure
        // grass outside the sand band.
        let t = (river_d_m / river_sand_half_width_m).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        let (secondary, veg) = if cliff_proximity_m < CLIFF_PROXIMITY_RADIUS_M {
            (PAL_CLIFF, 0)
        } else {
            let rocky = (slope / SLOPE_CLIFF_START).clamp(0.0, 1.0);
            let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
            let grass_t = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
            // `* t` fades veg density from 0 at the center to plain density
            // at the edge so the short-grass mesh doesn't pop on at the
            // boundary.
            let density = (grass_t * 9.0 * t).round().clamp(0.0, 9.0) as u8;
            let v = if density > 0 {
                short_grass_veg(density)
            } else {
                0
            };
            (PAL_GROUND, v)
        };
        (PAL_RIVER_BED, secondary, blend, veg)
    } else if is_sea {
        // Secondary = GROUND so the coast line shares a palette pair with
        // the land sand-band, keeping per-texture weights continuous
        // across the shoreline (a DIRT secondary here would abruptly
        // introduce laterite on every adjacent land cell).
        let depth = (-h_center).max(0.0);
        let blend = ((depth / SEA_MAX_DEPTH_FOR_BLEND).clamp(0.0, 1.0) * 255.0) as u8;
        (PAL_SAND, PAL_GROUND, blend, 0)
    } else if slope >= CLIFF_SLOPE_THRESHOLD {
        // ≥45° face: exposed marble cliff. Placed before alpine so steep
        // faces on snowy peaks still read as rock, not snow. Secondary =
        // GROUND keeps the palette pair consistent with the cliff-fade
        // branch below (just swapped primary/secondary), so there's no
        // texture discontinuity at the threshold — both sides resolve to
        // 100% CLIFF when slope is right at 1.0.
        (PAL_CLIFF, PAL_GROUND, 0, 0)
    } else if h_center > SNOW_ELEVATION_M {
        // Alpine: snow with cliff showing through on exposed slopes, so the
        // rocky blend matches the adjacent cliff patch color rather than
        // introducing a third ground texture.
        let t = ((h_center - SNOW_ELEVATION_M) / SNOW_FULL_SPAN_M).clamp(0.0, 1.0);
        let rocky = (slope / SLOPE_CLIFF_FULL).clamp(0.0, 1.0);
        let blend = (((1.0 - t) * 120.0).max(rocky * 200.0)) as u8;
        (PAL_SNOW, PAL_CLIFF, blend, 0)
    } else if !is_sea && coast_d_m <= COAST_SAND_M {
        // Quadratic blend keeps cells right at the water line near 100%
        // SAND so per-texture weights stay continuous with the adjacent
        // sea cell; a linear ramp would introduce ~25% GROUND right at the
        // shore and read as a staircase. Grass density has a floor of 1 so
        // the mesh fringe matches the adjacent plains' density of 9.
        // (`!is_sea` guard: distance alone has no sign, so a sea cell inside
        // a narrow inlet that's <COAST_SAND_M from the coast line would
        // otherwise compete for the sand band — land_mask is the source of
        // truth for the side.)
        const DENSITY_MIN: f32 = 1.0;
        let t = (coast_d_m / COAST_SAND_M).clamp(0.0, 1.0);
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
        // Plain / slope branch: GROUND primary, CLIFF secondary. Cliff bleeds
        // in via TWO channels and we take the max:
        //   (1) slope-based: smoothstep over [CLIFF_FADE_START, threshold].
        //       Fires on isolated steep ridges where the slope never quite
        //       reaches the cliff-primary threshold.
        //   (2) distance-based: proximity to a neighbor cell that DID cross
        //       the threshold, falling off linearly over
        //       CLIFF_PROXIMITY_RADIUS_M meters. This is what gives cliff
        //       edges a 2–3 cell gradient even when the actual slope
        //       transition is sharp enough that the slope-based channel
        //       barely fires.
        // Both channels meet the cliff-primary branch at 100% CLIFF (blend
        // 255 on this side, primary CLIFF on that side), so crossing the
        // threshold is a palette-pair swap with identical resolved colors.
        // `water_fade` holds blend near zero along coast / river / road
        // bands so banks stay clean grass.
        let rocky_slope = smoothstep(CLIFF_FADE_START, CLIFF_SLOPE_THRESHOLD, slope);
        // Dilation-then-smoothstep: cells within CORE_M stay at 1.0 so the
        // first ring around a cliff matches the cliff-primary branch's 100%
        // CLIFF exactly (no visible step), then fade smoothly to 0 over the
        // remaining `RADIUS - CORE` meters.
        let d_eff = (cliff_proximity_m - CLIFF_BLEND_CORE_M).max(0.0);
        let fade_span = (CLIFF_PROXIMITY_RADIUS_M - CLIFF_BLEND_CORE_M).max(1e-3);
        let rocky_proximity = 1.0 - smoothstep(0.0, fade_span, d_eff);
        let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
        // Water_fade is what keeps road/river/coast banks looking like clean
        // grass rather than picking up a slope tint from noise in the height
        // field. Applying it to `rocky_slope` is fine: a shallow-gradient
        // ridge near a river shouldn't read as rocky. But applying it to the
        // distance-to-cliff channel would erase cliff influence next to
        // actual cliffs that descend into water — e.g. the river cutting at
        // the base of a cliff. So proximity bypasses water_fade entirely.
        let coast_fade = ((coast_d_m - COAST_SAND_M) / COAST_FADE_SPAN_M).clamp(0.0, 1.0);
        let river_fade =
            ((river_d_m - river_sand_half_width_m) / RIVER_FADE_SPAN_M).clamp(0.0, 1.0);
        let road_fade =
            ((road_d_m - ROAD_HALF_WIDTH_M - ROAD_FADE_SPAN_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let water_fade = coast_fade.min(river_fade).min(road_fade);
        let rocky = (rocky_slope * water_fade).max(rocky_proximity);
        // Density combines hard exclusions (rocky, highland) with the
        // patch-field mask (0 outside patches, 1 inside, smooth fade at the
        // edge). The patch sample is only pulled here, so non-plain cells
        // skip the warp + Voronoi query entirely.
        let eligibility = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
        let patch = patch();
        let density = (patch.strength * eligibility * 9.0).round().clamp(0.0, 9.0) as u8;
        let veg = if density > 0 {
            if patch.is_tall {
                tall_grass_veg(density)
            } else {
                short_grass_veg(density)
            }
        } else {
            0
        };
        let blend = (rocky * 255.0) as u8;
        (PAL_GROUND, PAL_CLIFF, blend, veg)
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

    /// Flat plain far from any feature. Slope is 0 so the plain branch's
    /// rocky component is 0 and any blend value reflects only coastal fades,
    /// not terrain. Rivers and roads default to INFINITY so those branches
    /// are inactive.
    fn plain_inputs(coast_d_m: f32) -> SplatInputs {
        SplatInputs {
            is_sea: false,
            river_d_m: f32::INFINITY,
            river_width_m: 0.0,
            road_d_m: f32::INFINITY,
            h_center: 10.0,
            slope: 0.0,
            coast_d_m,
            cliff_proximity_m: CLIFF_PROXIMITY_RADIUS_M,
        }
    }

    /// Canonical width used by river-band tests. With `RIVER_SAND_WIDTH_MULT
    /// = 1.25`, a width of 4 m yields a 5 m sand band — the value the
    /// previous fixed `RIVER_SAND_HALF_WIDTH_M` constant used, so existing
    /// regression expectations carry over.
    const TEST_RIVER_WIDTH_M: f32 = 4.0;
    const TEST_RIVER_SAND_HALF_WIDTH_M: f32 = TEST_RIVER_WIDTH_M * RIVER_SAND_WIDTH_MULT;

    /// Full-coverage short-grass patch used by tests that don't care about
    /// veg output. Passed as `|| FULL_GRASS` where `classify_splat` wants a
    /// `FnOnce() -> PatchSample`.
    const FULL_GRASS: PatchSample = PatchSample {
        strength: 1.0,
        is_tall: false,
    };

    fn call_classify(inputs: SplatInputs) -> (u8, u8, u8, u8) {
        classify_splat(inputs, || FULL_GRASS)
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
        // `coast_d_m = COAST_SAND_M`, the coast branch emits blend=255 (100%
        // GROUND secondary) and the plain branch emits blend=0 (100% GROUND
        // primary). Both sides render pure GROUND, so the shader's
        // corner-sampled bilerp sees no texture discontinuity.
        let (cp, cs, c_blend, _) = call_classify(plain_inputs(COAST_SAND_M));
        assert_eq!((cp, cs), (PAL_SAND, PAL_GROUND));
        assert_eq!(c_blend, 255, "coast outer edge must be pure GROUND");

        let (pp, ps, p_blend, _) = call_classify(plain_inputs(COAST_SAND_M + 1e-4));
        assert_eq!((pp, ps), (PAL_GROUND, PAL_CLIFF));
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
            let d = COAST_SAND_M * (i as f32) / (steps as f32);
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
        let (p, s, blend, _) = call_classify(SplatInputs {
            is_sea: true,
            river_d_m: 1.0,
            river_width_m: TEST_RIVER_WIDTH_M,
            road_d_m: 0.0,
            h_center: -5.0,
            coast_d_m: 0.0,
            ..plain_inputs(0.0)
        });
        assert_eq!((p, s), (PAL_ROAD, PAL_GROUND));
        assert_eq!(blend, 0, "road center must be 100% PAL_ROAD");
    }

    #[test]
    fn priority_river_beats_sea() {
        // River bed uses (RIVER_BED, GROUND). Regression guard so an
        // accidental swap back to DIRT (red laterite) won't slip through.
        let (p, s, _, _) = call_classify(SplatInputs {
            is_sea: true,
            river_d_m: 0.0,
            river_width_m: TEST_RIVER_WIDTH_M,
            h_center: -1.0,
            coast_d_m: 0.0,
            ..plain_inputs(0.0)
        });
        assert_eq!((p, s), (PAL_RIVER_BED, PAL_GROUND));
    }

    #[test]
    fn river_outer_edge_meets_plain_seamlessly() {
        // Same continuity invariant as `coast_outer_edge_...` but for rivers.
        let at_edge = call_classify(SplatInputs {
            river_d_m: TEST_RIVER_SAND_HALF_WIDTH_M - 1e-4,
            river_width_m: TEST_RIVER_WIDTH_M,
            ..plain_inputs(100.0)
        });
        assert_eq!((at_edge.0, at_edge.1), (PAL_RIVER_BED, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "river edge must be near-pure GROUND");

        let past_edge = call_classify(SplatInputs {
            river_d_m: TEST_RIVER_SAND_HALF_WIDTH_M + 1e-4,
            river_width_m: TEST_RIVER_WIDTH_M,
            ..plain_inputs(100.0)
        });
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_CLIFF));
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
        let at_edge = call_classify(SplatInputs {
            road_d_m: ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M - 1e-4,
            ..plain_inputs(100.0)
        });
        assert_eq!((at_edge.0, at_edge.1), (PAL_ROAD, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "road edge must be near-pure GROUND");

        let past_edge = call_classify(SplatInputs {
            road_d_m: ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M + 1e-4,
            ..plain_inputs(100.0)
        });
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_CLIFF));
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
            let (primary, _, blend, _) = call_classify(SplatInputs {
                road_d_m: d,
                ..plain_inputs(100.0)
            });
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
        let at_margin = call_classify(SplatInputs {
            road_d_m: road_margin,
            slope: 0.5,
            ..plain_inputs(100.0)
        });
        let no_road = call_classify(SplatInputs {
            slope: 0.5,
            ..plain_inputs(100.0)
        });
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
        // matter to the splat water-fade (max-width sand band + fade span)
        // or the heightmap carve (max-width half-width + max taper). The
        // margin is computed from the global maxima so every tile sees
        // every segment that could influence it.
        let max_half_width = RIVER_MAX_WIDTH_M * 0.5;
        let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
        let max_sand = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
        let river_margin = (max_half_width + max_taper).max(max_sand + RIVER_FADE_SPAN_M);
        assert!(
            river_margin >= max_sand + RIVER_FADE_SPAN_M,
            "margin {} does not cover fade span {}",
            river_margin,
            max_sand + RIVER_FADE_SPAN_M
        );
        assert!(
            river_margin >= max_half_width + max_taper,
            "margin {} does not cover carve taper {}",
            river_margin,
            max_half_width + max_taper
        );
    }

    #[test]
    fn adjacent_tiles_see_same_nearby_river_segment() {
        // Build a synthetic world with a single river polyline straddling
        // the tile boundary at x = 0 (tile 0's right edge = tile 1's left
        // edge). The per-tile filter in `bake_tile` uses
        // `river_segments_near_tile` with the `river_margin` constant —
        // both adjacent tiles must see the segment so their splat
        // classification agrees at the boundary.
        use super::super::vector_features::{river_segments_near_tile, RiverWorldPolyline};
        let polys = vec![RiverWorldPolyline {
            points: vec![[0.5, -10.0], [0.5, 10.0]],
            flow_norm: vec![0.5, 0.5],
            width: vec![4.0, 4.0],
        }];
        let max_half_width = RIVER_MAX_WIDTH_M * 0.5;
        let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
        let max_sand = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
        let margin = (max_half_width + max_taper).max(max_sand + RIVER_FADE_SPAN_M);
        let near_tile_0 = river_segments_near_tile(&polys, -32.0, -16.0, 32.0, 16.0, margin);
        assert_eq!(near_tile_0.len(), 1, "tile 0 must see segment at x=0.5");
        let polys2 = vec![RiverWorldPolyline {
            points: vec![
                [32.0 - (margin - 1.0), -10.0],
                [32.0 - (margin - 1.0), 10.0],
            ],
            flow_norm: vec![0.5, 0.5],
            width: vec![4.0, 4.0],
        }];
        let near_tile_1 = river_segments_near_tile(&polys2, 32.0, -16.0, 96.0, 16.0, margin);
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
    fn non_plain_branches_do_not_sample_patch() {
        // Efficiency regression guard: the warped-Voronoi query is ~the
        // dominant cost of splat classification, so every non-plain branch
        // MUST NOT invoke the patch closure. A panic-on-call closure catches
        // any branch that accidentally starts pulling the sample eagerly.
        let trip = || -> PatchSample { panic!("patch must not be sampled") };

        // Road
        classify_splat(
            SplatInputs {
                road_d_m: 0.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // River (before sea check — river wins even in sea)
        classify_splat(
            SplatInputs {
                is_sea: true,
                river_d_m: 0.0,
                river_width_m: TEST_RIVER_WIDTH_M,
                h_center: -1.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Sea
        classify_splat(
            SplatInputs {
                is_sea: true,
                h_center: -5.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Cliff (slope ≥ threshold)
        classify_splat(
            SplatInputs {
                slope: CLIFF_SLOPE_THRESHOLD + 0.1,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Alpine (elevation > snow line, slope below cliff threshold)
        classify_splat(
            SplatInputs {
                h_center: SNOW_ELEVATION_M + 100.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Coast sand band
        classify_splat(plain_inputs(COAST_SAND_M * 0.5), trip);
    }

    #[test]
    fn river_binary_round_trip() {
        // Empty segment list returns None so the baker can skip writing a
        // file for tiles without rivers.
        assert!(bake_rivers_binary(&[]).is_none());

        let segs = vec![
            RiverSegment {
                ax: -10.5,
                az: 7.25,
                bx: 20.0,
                bz: -8.0,
                flow_norm_a: 0.125,
                flow_norm_b: 0.875,
                width_a: 2.0,
                width_b: 8.0,
            },
            RiverSegment {
                ax: 20.0,
                az: -8.0,
                bx: 22.5,
                bz: -10.0,
                flow_norm_a: 0.875,
                flow_norm_b: 1.0,
                width_a: 8.0,
                width_b: 10.0,
            },
        ];
        let bytes = bake_rivers_binary(&segs).expect("non-empty segments encode");
        assert_eq!(
            bytes.len(),
            RIVER_BIN_HEADER_SIZE + segs.len() * RIVER_BIN_SEGMENT_SIZE
        );
        // Magic + version + segment_count.
        assert_eq!(&bytes[0..4], RIVER_BIN_MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), RIVER_BIN_VERSION);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), segs.len() as u16);

        // First segment payload (offset 16).
        let read_f32 = |off: usize| -> f32 {
            f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
        };
        assert_eq!(read_f32(16), segs[0].ax);
        assert_eq!(read_f32(20), segs[0].az);
        assert_eq!(read_f32(24), segs[0].bx);
        assert_eq!(read_f32(28), segs[0].bz);
        assert_eq!(read_f32(32), segs[0].width_a);
        assert_eq!(read_f32(36), segs[0].width_b);
        assert_eq!(read_f32(40), segs[0].flow_norm_a);
        assert_eq!(read_f32(44), segs[0].flow_norm_b);
        // Second segment starts at 16 + 32 = 48.
        assert_eq!(read_f32(48), segs[1].ax);
    }

    #[test]
    fn plain_branch_samples_patch() {
        // Inverse guard: if the plain branch ever stops reading the patch
        // sample, every eligible land cell loses its veg byte silently.
        use std::cell::Cell;
        let called = Cell::new(false);
        classify_splat(plain_inputs(COAST_SAND_M + 1.0), || {
            called.set(true);
            FULL_GRASS
        });
        assert!(called.get(), "plain branch must sample patch");
    }

    #[test]
    fn palette_json_schema_matches_constants() {
        let meta: serde_json::Value =
            serde_json::from_str(PALETTE_JSON).expect("shared/palette.json is valid JSON");
        let layers = meta
            .get("layers")
            .and_then(|l| l.as_array())
            .expect("layers array");
        assert_eq!(layers.len(), PAL_RIVER_BED as usize + 1);
        for layer in layers {
            assert!(layer.get("texture").and_then(|t| t.as_str()).is_some());
            assert!(layer.get("tileScale").and_then(|t| t.as_f64()).is_some());
            let color = layer
                .get("minimapColor")
                .and_then(|c| c.as_array())
                .expect("minimapColor array");
            assert_eq!(color.len(), 3);
            for c in color {
                let v = c.as_u64().expect("minimapColor channel is u8");
                assert!(v <= 255);
            }
        }
    }
}
