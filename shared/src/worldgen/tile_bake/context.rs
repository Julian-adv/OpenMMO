//! Precomputed per-cell fields shared across every tile bake. Building these
//! once and reusing across all ~260k tiles is the difference between a
//! minute-long bake and something unusable.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use super::super::coasts::CoastPolyline;
use super::super::config::WorldGenConfig;
use super::super::global_map::GlobalMap;
use super::super::grass_patches::GrassPatchField;
use super::super::grid::bfs_distance_from;
use super::super::noise::{smoothstep, PerlinNoise3D};
use super::super::rivers::RiverMap;
use super::super::roads::RoadNetwork;
use super::super::vector_features::{
    cell_index_to_center, cell_node_to_center, chaikin_smooth, polyline_to_world,
    river_chaikin_smooth, river_polyline_to_world, RiverWorldPolyline, WorldPolyline,
};
use super::constants::{
    COAST_CHAIKIN_ITERATIONS, RIVER_CARVE_MIN_BED_Y_M, RIVER_CHAIKIN_ITERATIONS, RIVER_MAX_WIDTH_M,
    RIVER_MIN_WIDTH_M, RIVER_MOUTH_BRANCH_BED_Y_M, RIVER_MOUTH_BRANCH_COUNT_MAX,
    RIVER_MOUTH_BRANCH_COUNT_MIN, RIVER_MOUTH_BRANCH_END_JITTER_M,
    RIVER_MOUTH_BRANCH_END_WIDTH_MAX_M, RIVER_MOUTH_BRANCH_END_WIDTH_MIN_M,
    RIVER_MOUTH_BRANCH_END_WIDTH_SCALE, RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MAX,
    RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MIN, RIVER_MOUTH_BRANCH_MEANDER_M,
    RIVER_MOUTH_BRANCH_MEANDER_RAMP_T, RIVER_MOUTH_BRANCH_SAMPLES, RIVER_MOUTH_BRANCH_SPREAD_M,
    RIVER_MOUTH_FAN_ARC_CELLS, ROAD_CHAIKIN_ITERATIONS,
};
use super::heightmap::cell_elevation_m;

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
    /// Oriented land-on-the-LEFT of each chain's direction, so
    /// `signed_min_distance_to_segments` reads positive inland / negative
    /// seaward — the heightmap's shoreline blend depends on this.
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
        let dist_to_land = bfs_distance_from(&map.land_mask, res, 1, None);

        let mut rivers_world =
            smooth_river_polylines(river_map, &map.config, RIVER_CHAIKIN_ITERATIONS);
        // Split sea-bound mouths into several narrow distributaries. The
        // heightmap carve, splatmap, and river field all consume
        // `rivers_world`, so adding branches here keeps every baked layer
        // aligned.
        apply_mouth_distributaries(&mut rivers_world, map, &dist_to_land);
        let roads_world = smooth_polylines(
            road_net.roads.iter().map(|r| r.points.as_slice()),
            &map.config,
            ROAD_CHAIKIN_ITERATIONS,
            cell_index_to_center,
        );
        let mut coasts_world = smooth_polylines(
            coasts.iter().map(|c| c.points.as_slice()),
            &map.config,
            COAST_CHAIKIN_ITERATIONS,
            cell_node_to_center,
        );
        orient_coasts_land_left(&mut coasts_world, map, &dist_to_land);

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

/// Ensure every coast polyline runs with land on its LEFT, so the
/// heightmap's `signed_min_distance_to_segments` query reads positive
/// inland and negative seaward. Marching squares + chain tracing emit
/// chains in arbitrary direction, but a marching-squares contour of a
/// binary mask keeps land on one consistent side along its whole length
/// (the curve separates land from sea and never self-intersects — saddles
/// are resolved disjoint), so one land-side vote per chain suffices; the
/// vote still averages up to 16 segments to shrug off locally ambiguous
/// spots like river mouths and narrow spits. Each sampled segment offsets
/// its midpoint by ±half a cell along the left normal and compares
/// interpolated base elevation — the land side reads higher.
fn orient_coasts_land_left(coasts: &mut [WorldPolyline], map: &GlobalMap, dist_to_land: &[u16]) {
    let probe_m = map.config.meters_per_cell() * 0.5;
    for poly in coasts.iter_mut() {
        let n = poly.points.len();
        if n < 2 {
            continue;
        }
        let step = ((n - 1) / 16).max(1);
        let mut score = 0.0f64;
        for i in (0..n - 1).step_by(step) {
            let a = poly.points[i];
            let b = poly.points[i + 1];
            let dx = b[0] - a[0];
            let dz = b[1] - a[1];
            let len = (dx * dx + dz * dz).sqrt();
            if len < 1e-3 {
                continue;
            }
            // Left normal of a→b (positive-cross side, matching the sign
            // convention in `signed_min_distance_to_segments`).
            let nx = -dz / len;
            let nz = dx / len;
            let mx = (a[0] + b[0]) * 0.5;
            let mz = (a[1] + b[1]) * 0.5;
            let left =
                sample_base_elevation(map, dist_to_land, mx + nx * probe_m, mz + nz * probe_m);
            let right =
                sample_base_elevation(map, dist_to_land, mx - nx * probe_m, mz - nz * probe_m);
            score += (left - right) as f64;
        }
        if score < 0.0 {
            poly.points.reverse();
        }
    }
}

/// Bilinear sample of the coarse 4K base-elevation grid at a world
/// position. Shares `cell_elevation_m` with the hot-path bicubic sampler
/// so both evaluate "mouth-ness" against the same bathymetry curve.
fn sample_base_elevation(map: &GlobalMap, dist_to_land: &[u16], wx: f32, wz: f32) -> f32 {
    let res = map.config.global_res as i32;
    let mpc = map.config.meters_per_cell();
    let half = map.config.world_size_m as f32 * 0.5;
    let fx = (wx + half) / mpc - 0.5;
    let fz = (wz + half) / mpc - 0.5;
    let ix0 = fx.floor() as i32;
    let iz0 = fz.floor() as i32;
    let tx = fx - ix0 as f32;
    let tz = fz - iz0 as f32;
    let sample = |ix: i32, iz: i32| -> f32 {
        let cx = ix.rem_euclid(res) as usize;
        let cz = iz.clamp(0, res - 1) as usize;
        cell_elevation_m(map, dist_to_land, cz * res as usize + cx)
    };
    let e00 = sample(ix0, iz0);
    let e10 = sample(ix0 + 1, iz0);
    let e01 = sample(ix0, iz0 + 1);
    let e11 = sample(ix0 + 1, iz0 + 1);
    let e0 = e00 * (1.0 - tx) + e10 * tx;
    let e1 = e01 * (1.0 - tx) + e11 * tx;
    e0 * (1.0 - tz) + e1 * tz
}

/// Replace the sea-bound tail of each river with several narrow distributary
/// branches (`RIVER_MOUTH_BRANCH_COUNT_MIN..=MAX`). The split starts at the
/// same arc distance where the old mouth fan began widening; the original
/// polyline is truncated there and the generated branches carry the flow
/// into the sea with gentle S-curves.
fn apply_mouth_distributaries(
    rivers_world: &mut Vec<RiverWorldPolyline>,
    map: &GlobalMap,
    dist_to_land: &[u16],
) {
    let original_len = rivers_world.len();
    let mut branches = Vec::new();
    for poly_idx in 0..original_len {
        branches.extend(split_mouth_polyline(
            poly_idx,
            &mut rivers_world[poly_idx],
            map,
            dist_to_land,
        ));
    }
    rivers_world.extend(branches);
}

fn split_mouth_polyline(
    poly_idx: usize,
    poly: &mut RiverWorldPolyline,
    map: &GlobalMap,
    dist_to_land: &[u16],
) -> Vec<RiverWorldPolyline> {
    let n = poly.points.len();
    if n < 3 {
        return Vec::new();
    }
    let end = poly.points[n - 1];
    if sample_base_elevation(map, dist_to_land, end[0], end[1]) >= 0.0 {
        return Vec::new();
    }

    let arc_m = RIVER_MOUTH_FAN_ARC_CELLS * map.config.meters_per_cell();
    let (lens, total) = polyline_arc_lengths(&poly.points);
    let Some(apex_idx) = (0..n).rev().find(|&i| total - lens[i] >= arc_m) else {
        return Vec::new();
    };
    if apex_idx == 0 || apex_idx >= n - 1 {
        return Vec::new();
    }

    let apex = poly.points[apex_idx];
    let axis_x = end[0] - apex[0];
    let axis_z = end[1] - apex[1];
    let axis_len = (axis_x * axis_x + axis_z * axis_z).sqrt();
    if axis_len < 1e-3 {
        return Vec::new();
    }
    let tangent = [axis_x / axis_len, axis_z / axis_len];
    let normal = [-tangent[1], tangent[0]];
    let apex_flow = poly.flow_norm[apex_idx];
    let end_flow = poly.flow_norm[n - 1];
    let apex_width = poly.width[apex_idx].min(RIVER_MAX_WIDTH_M);
    let base_width = poly.width[n - 1].min(RIVER_MAX_WIDTH_M);

    let mut rng = SmallRng::seed_from_u64(
        map.config.seed
            ^ 0xD157_1B00_5EED_5EED
            ^ (poly_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    let count = rng.gen_range(RIVER_MOUTH_BRANCH_COUNT_MIN..=RIVER_MOUTH_BRANCH_COUNT_MAX);
    debug_assert!(count >= 2);
    let spread_half = RIVER_MOUTH_BRANCH_SPREAD_M.min(axis_len * 0.75);
    let spacing = spread_half * 2.0 / (count - 1) as f32;

    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let slot_t = i as f32 / (count - 1) as f32;
        let jitter_cap = RIVER_MOUTH_BRANCH_END_JITTER_M.min(spacing * 0.25);
        let edge_damp = if i == 0 || i + 1 == count { 0.45 } else { 1.0 };
        let jitter = rng.gen_range(-jitter_cap..=jitter_cap) * edge_damp;
        let lateral = (slot_t * 2.0 - 1.0) * spread_half + jitter;
        let mut end_pt = [end[0] + normal[0] * lateral, end[1] + normal[1] * lateral];
        end_pt = push_point_to_sea(end_pt, tangent, map, dist_to_land);
        let end_bed_floor = sample_base_elevation(map, dist_to_land, end_pt[0], end_pt[1])
            .min(RIVER_MOUTH_BRANCH_BED_Y_M);

        let width_jitter = rng.gen_range(0.9..=1.12);
        let end_width = (base_width * RIVER_MOUTH_BRANCH_END_WIDTH_SCALE * width_jitter).clamp(
            RIVER_MOUTH_BRANCH_END_WIDTH_MIN_M,
            RIVER_MOUTH_BRANCH_END_WIDTH_MAX_M,
        );
        let amp_cap = (spacing * 0.42).max(1.0).min(axis_len * 0.22);
        let amp = rng.gen_range(0.45..=1.0)
            * RIVER_MOUTH_BRANCH_MEANDER_M.min(amp_cap)
            * if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
        let cycles = rng.gen_range(
            RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MIN..RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MAX,
        );

        out.push(build_distributary_branch(
            apex,
            end_pt,
            apex_flow,
            end_flow,
            apex_width,
            end_width,
            end_bed_floor,
            amp,
            cycles,
        ));
    }

    poly.points.truncate(apex_idx + 1);
    poly.flow_norm.truncate(apex_idx + 1);
    poly.width.truncate(apex_idx + 1);

    out
}

fn polyline_arc_lengths(points: &[[f32; 2]]) -> (Vec<f32>, f32) {
    let mut lens = Vec::with_capacity(points.len());
    lens.push(0.0);
    let mut cumulative = 0.0f32;
    for i in 1..points.len() {
        let dx = points[i][0] - points[i - 1][0];
        let dz = points[i][1] - points[i - 1][1];
        cumulative += (dx * dx + dz * dz).sqrt();
        lens.push(cumulative);
    }
    (lens, cumulative)
}

const SEA_SEARCH_STEP_M: f32 = 8.0;
const SEA_SEARCH_MAX_FORWARD_STEPS: usize = 40;
const SEA_SEARCH_EXTRA_STEPS: usize = 4;
const SEA_SEARCH_LATERAL_STEPS: i32 = 4;
const SEA_SEARCH_TARGET_ELEVATION_M: f32 = -1.5;

fn push_point_to_sea(
    p: [f32; 2],
    tangent: [f32; 2],
    map: &GlobalMap,
    dist_to_land: &[u16],
) -> [f32; 2] {
    let start = clamp_world_point(p, map);
    let normal = [-tangent[1], tangent[0]];
    let mut best_sea: Option<([f32; 2], f32)> = None;

    for forward_step in 0..=SEA_SEARCH_MAX_FORWARD_STEPS {
        let forward_m = forward_step as f32 * SEA_SEARCH_STEP_M;
        let max_lateral_steps = ((forward_step as i32 + 1) / 3).min(SEA_SEARCH_LATERAL_STEPS);

        for lateral_step in 0..=max_lateral_steps {
            for sign in [-1.0f32, 1.0] {
                if lateral_step == 0 && sign < 0.0 {
                    continue;
                }
                let lateral_m = lateral_step as f32 * SEA_SEARCH_STEP_M * sign;
                let candidate = clamp_world_point(
                    [
                        start[0] + tangent[0] * forward_m + normal[0] * lateral_m,
                        start[1] + tangent[1] * forward_m + normal[1] * lateral_m,
                    ],
                    map,
                );
                let elevation =
                    sample_base_elevation(map, dist_to_land, candidate[0], candidate[1]);
                if elevation >= 0.0 {
                    continue;
                }
                if best_sea
                    .map(|(_, best_elevation)| elevation < best_elevation)
                    .unwrap_or(true)
                {
                    best_sea = Some((candidate, elevation));
                }
                if elevation <= SEA_SEARCH_TARGET_ELEVATION_M {
                    return push_point_deeper_into_sea(
                        candidate,
                        elevation,
                        tangent,
                        map,
                        dist_to_land,
                    );
                }
            }
        }
    }

    if let Some((candidate, elevation)) = best_sea {
        return push_point_deeper_into_sea(candidate, elevation, tangent, map, dist_to_land);
    }

    clamp_world_point(
        [
            start[0] + tangent[0] * SEA_SEARCH_STEP_M * SEA_SEARCH_MAX_FORWARD_STEPS as f32,
            start[1] + tangent[1] * SEA_SEARCH_STEP_M * SEA_SEARCH_MAX_FORWARD_STEPS as f32,
        ],
        map,
    )
}

fn push_point_deeper_into_sea(
    mut p: [f32; 2],
    start_elevation: f32,
    tangent: [f32; 2],
    map: &GlobalMap,
    dist_to_land: &[u16],
) -> [f32; 2] {
    let mut best = p;
    let mut best_elevation = start_elevation;

    for _ in 0..SEA_SEARCH_EXTRA_STEPS {
        p[0] += tangent[0] * SEA_SEARCH_STEP_M;
        p[1] += tangent[1] * SEA_SEARCH_STEP_M;
        p = clamp_world_point(p, map);
        let elevation = sample_base_elevation(map, dist_to_land, p[0], p[1]);
        if elevation < best_elevation {
            best = p;
            best_elevation = elevation;
        }
        if elevation >= 0.0 {
            break;
        }
    }

    best
}

fn clamp_world_point(mut p: [f32; 2], map: &GlobalMap) -> [f32; 2] {
    let half = map.config.world_size_m as f32 * 0.5;
    p[0] = p[0].clamp(-half, half);
    p[1] = p[1].clamp(-half, half);
    p
}

fn build_distributary_branch(
    apex: [f32; 2],
    end: [f32; 2],
    apex_flow: f32,
    end_flow: f32,
    apex_width: f32,
    end_width: f32,
    end_bed_floor: f32,
    meander_amp: f32,
    meander_cycles: f32,
) -> RiverWorldPolyline {
    let axis_x = end[0] - apex[0];
    let axis_z = end[1] - apex[1];
    let axis_len = (axis_x * axis_x + axis_z * axis_z).sqrt().max(1e-3);
    let normal = [-axis_z / axis_len, axis_x / axis_len];
    let bed_drop_t = (10.0 / axis_len).min(0.25);

    let mut points = Vec::with_capacity(RIVER_MOUTH_BRANCH_SAMPLES);
    let mut flow_norm = Vec::with_capacity(RIVER_MOUTH_BRANCH_SAMPLES);
    let mut widths = Vec::with_capacity(RIVER_MOUTH_BRANCH_SAMPLES);
    let mut bed_floor = Vec::with_capacity(RIVER_MOUTH_BRANCH_SAMPLES);
    for i in 0..RIVER_MOUTH_BRANCH_SAMPLES {
        let t = i as f32 / (RIVER_MOUTH_BRANCH_SAMPLES - 1) as f32;
        let meander_phase = meander_cycles * std::f32::consts::TAU * t;
        let meander_envelope = smoothstep(0.0, RIVER_MOUTH_BRANCH_MEANDER_RAMP_T, t);
        let s = meander_phase.sin() * meander_amp * meander_envelope;
        points.push([
            apex[0] + axis_x * t + normal[0] * s,
            apex[1] + axis_z * t + normal[1] * s,
        ]);
        flow_norm.push(apex_flow + (end_flow - apex_flow) * t);
        let width_t = smoothstep(0.0, RIVER_MOUTH_BRANCH_WIDTH_RAMP_T, t);
        widths.push(apex_width + (end_width - apex_width) * width_t);
        let initial_drop_t = smoothstep(0.0, bed_drop_t, t);
        let sea_drop_t = smoothstep(bed_drop_t, 1.0, t);
        let shallow_floor = RIVER_CARVE_MIN_BED_Y_M
            + (RIVER_MOUTH_BRANCH_BED_Y_M - RIVER_CARVE_MIN_BED_Y_M) * initial_drop_t;
        bed_floor.push(shallow_floor + (end_bed_floor - RIVER_MOUTH_BRANCH_BED_Y_M) * sea_drop_t);
    }

    river_chaikin_smooth(
        &RiverWorldPolyline {
            points,
            flow_norm,
            width: widths,
            bed_floor,
        },
        RIVER_CHAIKIN_ITERATIONS,
    )
}

/// Fraction of the branch length over which the carved width ramps from the
/// apex (full river) width down to the narrow sea-contact end width.
const RIVER_MOUTH_BRANCH_WIDTH_RAMP_T: f32 = 0.25;

/// Convert an iterator of cell-coord polylines into world-space polylines,
/// splitting at the X seam and Chaikin-smoothing each resulting piece.
/// `to_cell` maps each input vertex to its cell-coord position (see
/// `vector_features::polyline_to_world`); pass `cell_index_to_center` for
/// `(u32, u32)` rivers/roads, `cell_node_to_center` for `[f32; 2]`
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
