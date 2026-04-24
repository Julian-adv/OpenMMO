//! Precomputed per-cell fields shared across every tile bake. Building these
//! once and reusing across all ~260k tiles is the difference between a
//! minute-long bake and something unusable.

use super::super::coasts::CoastPolyline;
use super::super::config::WorldGenConfig;
use super::super::global_map::GlobalMap;
use super::super::grass_patches::GrassPatchField;
use super::super::grid::bfs_distance_from;
use super::super::noise::PerlinNoise3D;
use super::super::rivers::RiverMap;
use super::super::roads::RoadNetwork;
use super::super::vector_features::{
    cell_coord_passthrough, cell_index_to_center, chaikin_smooth, polyline_to_world,
    river_chaikin_smooth, river_polyline_to_world, RiverWorldPolyline, WorldPolyline,
};
use super::constants::{
    COAST_CHAIKIN_ITERATIONS, RIVER_CHAIKIN_ITERATIONS, RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M,
    RIVER_MOUTH_FAN_BASE_HIGH_M, RIVER_MOUTH_FAN_BASE_LOW_M, RIVER_MOUTH_FAN_EXTRA,
    ROAD_CHAIKIN_ITERATIONS,
};

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

        let mut rivers_world =
            smooth_river_polylines(river_map, &map.config, RIVER_CHAIKIN_ITERATIONS);
        // Widen polyline widths near the coast so the estuary fans out into
        // a small delta. Heightmap carve, splatmap classification, and the
        // client ribbon all consume these per-vertex widths, so applying
        // the scale here keeps the three consistent — if this lived only
        // on the client, the water plane would overhang the carved banks.
        apply_mouth_fan_widths(&mut rivers_world, map, &dist_to_land);
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

/// Scale each polyline vertex's `width` by a factor that ramps up as the
/// surrounding (base) elevation falls toward sea level. See constants
/// `RIVER_MOUTH_FAN_*`. The base elevation is sampled from the coarse
/// 4K grid — sub-meter accuracy isn't needed since the fan scale is a
/// gentle smoothstep over several meters of Y.
fn apply_mouth_fan_widths(
    rivers_world: &mut [RiverWorldPolyline],
    map: &GlobalMap,
    dist_to_land: &[u16],
) {
    let res = map.config.global_res as usize;
    let mpc = map.config.meters_per_cell();
    let half = map.config.world_size_m as f32 * 0.5;

    // Bilinear sample of the coarse 4K base-elevation grid. A naive
    // floor-to-cell produced visible stair-steps in the mouth fan
    // because ~4 sub-meter polyline vertices fall inside one 8 m cell
    // and all got the same width multiplier — the fan widened in 8 m
    // jumps rather than smoothly. Interpolating across neighbours makes
    // the per-vertex base elevation (and therefore the fan ramp)
    // continuous along the polyline.
    let sample_cell = |ix: i32, iz: i32| -> f32 {
        // X wraps, Z clamps — mirrors `cell_elevation_m`'s rules.
        let cx = ix.rem_euclid(res as i32) as usize;
        let cz = iz.clamp(0, res as i32 - 1) as usize;
        let i = cz * res + cx;
        if map.land_mask[i] == 1 {
            map.elevation_m[i]
        } else {
            let d = dist_to_land[i] as f32;
            -(0.5 + d.min(40.0) * 0.25)
        }
    };
    let sample_base = |wx: f32, wz: f32| -> f32 {
        // Cell-center basis: fx=0 at cell 0's center, fx=1 at cell 1's.
        let fx = (wx + half) / mpc - 0.5;
        let fz = (wz + half) / mpc - 0.5;
        let ix0 = fx.floor() as i32;
        let iz0 = fz.floor() as i32;
        let tx = fx - ix0 as f32;
        let tz = fz - iz0 as f32;
        let e00 = sample_cell(ix0, iz0);
        let e10 = sample_cell(ix0 + 1, iz0);
        let e01 = sample_cell(ix0, iz0 + 1);
        let e11 = sample_cell(ix0 + 1, iz0 + 1);
        let e0 = e00 * (1.0 - tx) + e10 * tx;
        let e1 = e01 * (1.0 - tx) + e11 * tx;
        e0 * (1.0 - tz) + e1 * tz
    };

    let span = RIVER_MOUTH_FAN_BASE_HIGH_M - RIVER_MOUTH_FAN_BASE_LOW_M;
    for poly in rivers_world.iter_mut() {
        for i in 0..poly.points.len() {
            let base = sample_base(poly.points[i][0], poly.points[i][1]);
            let t = ((base - RIVER_MOUTH_FAN_BASE_LOW_M) / span).clamp(0.0, 1.0);
            // Quadratic ease-in (inverted): front-loads the widening
            // toward sea level so the fan reads as a localized delta
            // flare rather than a uniformly wider river, while staying
            // smooth enough to form a bell curve rather than the sharp
            // T-shape a cubic ramp produces.
            let one_minus_t = 1.0 - t;
            let s = one_minus_t * one_minus_t;
            poly.width[i] *= 1.0 + RIVER_MOUTH_FAN_EXTRA * s;
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
    cfg: &WorldGenConfig,
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
    cfg: &WorldGenConfig,
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
