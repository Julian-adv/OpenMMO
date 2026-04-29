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
use super::super::noise::PerlinNoise3D;
use super::super::rivers::RiverMap;
use super::super::roads::RoadNetwork;
use super::super::vector_features::{
    cell_coord_passthrough, cell_index_to_center, chaikin_smooth, polyline_to_world,
    river_chaikin_smooth, river_polyline_to_world, RiverWorldPolyline, WorldPolyline,
};
use super::constants::{
    COAST_CHAIKIN_ITERATIONS, MOUTH_ISLAND_ANGLE_JITTER_RAD, MOUTH_ISLAND_APEX_ELEV_M,
    MOUTH_ISLAND_COUNT_MAX, MOUTH_ISLAND_COUNT_MIN, MOUTH_ISLAND_END_ALONG_MAX_M,
    MOUTH_ISLAND_END_ALONG_MIN_M, MOUTH_ISLAND_FAN_HALF_ANGLE_RAD, MOUTH_ISLAND_LAND_HEIGHT_BOOST,
    MOUTH_ISLAND_PEAK_MAX_M, MOUTH_ISLAND_PEAK_MIN_M, MOUTH_ISLAND_RADIUS_MAX_M,
    MOUTH_ISLAND_RADIUS_MIN_M, MOUTH_ISLAND_TIP_ALONG_MAX_M, MOUTH_ISLAND_WIDEST_AXIS_T,
    RIVER_CHAIKIN_ITERATIONS, RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M, RIVER_MOUTH_FAN_BASE_HIGH_M,
    RIVER_MOUTH_FAN_BASE_LOW_M, RIVER_MOUTH_FAN_EXTRA, ROAD_CHAIKIN_ITERATIONS,
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
    pub coasts_world: Vec<WorldPolyline>,
    /// Procedural sandy finger-islands placed inside each river's
    /// estuary fan. Pre-filtered per tile by `mouth_islands_near_tile`.
    pub(super) mouth_islands: Vec<MouthIsland>,
}

/// One sandy finger-island in a river-mouth delta. The heightmap bump is
/// a teardrop capsule centered on `center`, long axis along `tangent`.
#[derive(Clone, Copy, Debug)]
pub(super) struct MouthIsland {
    pub(super) center: [f32; 2],
    pub(super) tangent: [f32; 2],
    pub(super) half_len: f32,
    pub(super) radius: f32,
    pub(super) peak_m: f32,
    /// Precomputed `half_len + radius` for bbox culling; constant per island.
    pub(super) reach_m: f32,
}

impl MouthIsland {
    /// Height bump contribution at world `(wx, wz)`. Zero outside the
    /// teardrop. Along the axis `u ∈ [0, 1]` (0 = upstream tip, 1 =
    /// downstream tip) the effective radius sweeps from 0 → full at
    /// `u = MOUTH_ISLAND_WIDEST_AXIS_T` and back down to 0. Perpendicular
    /// distance then uses a quartic bell `(1 − (d/r)²)²` so the surface
    /// rolls smoothly to 0 at the edge.
    #[inline]
    pub(super) fn bump_m(&self, wx: f32, wz: f32) -> f32 {
        let dx = wx - self.center[0];
        let dz = wz - self.center[1];
        let along_raw = dx * self.tangent[0] + dz * self.tangent[1];
        if along_raw.abs() > self.half_len {
            return 0.0;
        }
        let u = (along_raw / self.half_len + 1.0) * 0.5;
        let u_peak = MOUTH_ISLAND_WIDEST_AXIS_T;
        // Quadratic rise (pointed on land side), quarter-circle arc
        // fall (rounded on sea side) — teardrop with a sharp flow-facing
        // head and a circular tail silhouette.
        let r_scale = if u <= u_peak {
            let t = u / u_peak;
            t * t
        } else {
            let t = (u - u_peak) / (1.0 - u_peak);
            (1.0 - t * t).sqrt()
        };
        let r_at_u = self.radius * r_scale;
        if r_at_u <= 1e-3 {
            return 0.0;
        }
        let px = self.center[0] + self.tangent[0] * along_raw;
        let pz = self.center[1] + self.tangent[1] * along_raw;
        let perp_dx = wx - px;
        let perp_dz = wz - pz;
        let d_sq = perp_dx * perp_dx + perp_dz * perp_dz;
        let r_sq = r_at_u * r_at_u;
        if d_sq >= r_sq {
            return 0.0;
        }
        let t_sq = d_sq / r_sq;
        let one_minus_tsq = 1.0 - t_sq;
        let s = one_minus_tsq * one_minus_tsq;
        let height_scale = 1.0 + MOUTH_ISLAND_LAND_HEIGHT_BOOST * (1.0 - u);
        self.peak_m * height_scale * s
    }
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
        let mouth_islands = generate_mouth_islands(&rivers_world, map, &dist_to_land);
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
            mouth_islands,
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

/// For each river polyline that enters the sea, scatter a handful of
/// elongated capsule "finger-islands" inside the fanned estuary. All
/// islands in one mouth share the apex tangent so they read as parallel
/// bars (matches real-world delta morphology). Seed is derived per
/// polyline so the scatter is stable across tile bakes that both touch
/// the same mouth.
fn generate_mouth_islands(
    rivers_world: &[RiverWorldPolyline],
    map: &GlobalMap,
    dist_to_land: &[u16],
) -> Vec<MouthIsland> {
    let mut islands = Vec::new();
    let master_seed = map.config.seed;

    for (poly_idx, poly) in rivers_world.iter().enumerate() {
        if poly.points.len() < 2 {
            continue;
        }
        let tip = poly.points[poly.points.len() - 1];
        if sample_base_elevation(map, dist_to_land, tip[0], tip[1]) >= MOUTH_ISLAND_APEX_ELEV_M {
            continue;
        }
        let apex = match poly.points.iter().rposition(|p| {
            sample_base_elevation(map, dist_to_land, p[0], p[1]) >= MOUTH_ISLAND_APEX_ELEV_M
        }) {
            Some(i) if i + 1 < poly.points.len() => i,
            _ => continue,
        };

        // Tangent averaged across the segment that crosses the coast.
        let tan_a = apex.saturating_sub(1);
        let tan_b = (apex + 2).min(poly.points.len() - 1);
        let a = poly.points[tan_a];
        let b = poly.points[tan_b];
        let tx = b[0] - a[0];
        let tz = b[1] - a[1];
        let tlen = (tx * tx + tz * tz).sqrt().max(1e-6);
        let tangent = [tx / tlen, tz / tlen];
        let normal = [-tangent[1], tangent[0]];

        let apex_pt = poly.points[apex];

        let mut rng = SmallRng::seed_from_u64(
            master_seed ^ (poly_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        let count = rng.gen_range(MOUTH_ISLAND_COUNT_MIN..=MOUTH_ISLAND_COUNT_MAX);
        for i in 0..count {
            let slot_t = if count <= 1 {
                0.5
            } else {
                i as f32 / (count as f32 - 1.0)
            };
            let base_angle = (slot_t * 2.0 - 1.0) * MOUTH_ISLAND_FAN_HALF_ANGLE_RAD;
            let jitter = (rng.gen::<f32>() * 2.0 - 1.0) * MOUTH_ISLAND_ANGLE_JITTER_RAD;
            let theta = base_angle + jitter;
            let cos_t = theta.cos();
            let sin_t = theta.sin();
            let fan_x = tangent[0] * cos_t + normal[0] * sin_t;
            let fan_z = tangent[1] * cos_t + normal[1] * sin_t;

            let tip_along = rng.gen::<f32>() * MOUTH_ISLAND_TIP_ALONG_MAX_M;
            let tip_x = apex_pt[0] + fan_x * tip_along;
            let tip_z = apex_pt[1] + fan_z * tip_along;

            let end_along =
                rng.gen_range(MOUTH_ISLAND_END_ALONG_MIN_M..MOUTH_ISLAND_END_ALONG_MAX_M);
            let end_x = apex_pt[0] + fan_x * end_along;
            let end_z = apex_pt[1] + fan_z * end_along;

            let axis_x = end_x - tip_x;
            let axis_z = end_z - tip_z;
            let length = (axis_x * axis_x + axis_z * axis_z).sqrt();
            if length < 1e-3 {
                continue;
            }
            let island_tangent = [axis_x / length, axis_z / length];
            let center = [(tip_x + end_x) * 0.5, (tip_z + end_z) * 0.5];

            let radius = rng.gen_range(MOUTH_ISLAND_RADIUS_MIN_M..MOUTH_ISLAND_RADIUS_MAX_M);
            let peak = rng.gen_range(MOUTH_ISLAND_PEAK_MIN_M..MOUTH_ISLAND_PEAK_MAX_M);
            let half_len = length * 0.5;
            islands.push(MouthIsland {
                center,
                tangent: island_tangent,
                half_len,
                radius,
                peak_m: peak,
                reach_m: half_len + radius,
            });
        }
    }
    islands
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
    let span = RIVER_MOUTH_FAN_BASE_HIGH_M - RIVER_MOUTH_FAN_BASE_LOW_M;
    for poly in rivers_world.iter_mut() {
        for i in 0..poly.points.len() {
            let base =
                sample_base_elevation(map, dist_to_land, poly.points[i][0], poly.points[i][1]);
            // J-curve: `(1-t)^2` with only the upper bound clamped, so
            // underwater polyline vertices (`t < 0`) push `s > 1` and
            // the multiplier accelerates monotonically with no plateau.
            // Lower clamp would saturate every below-sea-level vertex
            // at the same peak — visible as a constant-width band along
            // the last few cells before the coastline.
            let t = ((base - RIVER_MOUTH_FAN_BASE_LOW_M) / span).min(1.0);
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
