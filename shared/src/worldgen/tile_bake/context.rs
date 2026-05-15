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
    COAST_CHAIKIN_ITERATIONS, MOUTH_ISLAND_COUNT_MAX, MOUTH_ISLAND_COUNT_MIN,
    MOUTH_ISLAND_END_ALONG_FRAC_MAX, MOUTH_ISLAND_END_ALONG_FRAC_MIN,
    MOUTH_ISLAND_PEAK_MAX_M, MOUTH_ISLAND_PEAK_MIN_M,
    MOUTH_ISLAND_PERP_JITTER_M, MOUTH_ISLAND_RADIUS_MAX_M, MOUTH_ISLAND_RADIUS_MIN_M,
    MOUTH_ISLAND_SPACING_M, MOUTH_ISLAND_SPREAD_FRAC, MOUTH_ISLAND_TIP_ALONG_FRAC_MAX,
    MOUTH_ISLAND_TIP_ALONG_FRAC_MIN, MOUTH_ISLAND_TIP_LATERAL_FRAC,
    MOUTH_ISLAND_TIP_RADIUS_FRAC, MOUTH_ISLAND_WIDEST_AXIS_T, RIVER_CHAIKIN_ITERATIONS,
    RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M, RIVER_MOUTH_FAN_ARC_CELLS,
    RIVER_MOUTH_FAN_BANK_WOBBLE_M, RIVER_MOUTH_FAN_BANK_WOBBLE_WAVELENGTH_M,
    RIVER_MOUTH_FAN_EXTRA, RIVER_MOUTH_FAN_SHARPNESS, ROAD_CHAIKIN_ITERATIONS,
};
use super::heightmap::{cell_elevation_m, lerp};

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
        // Quarter-circle arc on both sides of the widest axis — vertical
        // tangent at each tip so radius grows fast away from u=0/u=1 and
        // the bar reads as a continuous ellipse at 1 m heightmap sampling.
        // A quadratic rise on the upstream side would collapse to a
        // sub-grid needle (e.g. radius·(0.1/0.75)² ≈ 0.05 m), only
        // catching the rare on-axis vertex and rendering the upstream
        // end as a chain of disconnected dots above the deep mouth-fan
        // bed. `t` is normalised so the two halves share the arc body.
        let t = if u <= u_peak {
            (u_peak - u) / u_peak
        } else {
            (u - u_peak) / (1.0 - u_peak)
        };
        let r_scale = (1.0 - t * t).sqrt();
        // Asymmetric width: the upstream half-ellipse is squeezed by
        // `TIP_RADIUS_FRAC` at u=0, ramping linearly back to 1.0 at the
        // widest axis so the seam at `u_peak` stays kink-free. The
        // downstream half keeps full radius. Real delta bars are
        // narrower on the flow-facing edge where current scours.
        let side_scale = if u <= u_peak {
            lerp(MOUTH_ISLAND_TIP_RADIUS_FRAC, 1.0, u / u_peak)
        } else {
            1.0
        };
        let r_at_u = self.radius * r_scale * side_scale;
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
        self.peak_m * s
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
        let dist_to_land = bfs_distance_from(&map.land_mask, res, 1, None);

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

    let fan_arc_m = RIVER_MOUTH_FAN_ARC_CELLS * map.config.meters_per_cell();

    for (poly_idx, poly) in rivers_world.iter().enumerate() {
        let n = poly.points.len();
        if n < 2 {
            continue;
        }
        let end_pt = poly.points[n - 1];
        if sample_base_elevation(map, dist_to_land, end_pt[0], end_pt[1]) >= 0.0 {
            continue;
        }

        // Fan apex: vertex at arc-distance >= fan_arc_m from the end. Islands
        // are anchored here so their span is parameterized in fan-zone
        // fractions, not in absolute meters — keeps the placement coherent
        // across world resolutions.
        let mut cumulative = 0.0f32;
        let mut lens: Vec<f32> = Vec::with_capacity(n);
        lens.push(0.0);
        for i in 1..n {
            let dx = poly.points[i][0] - poly.points[i - 1][0];
            let dy = poly.points[i][1] - poly.points[i - 1][1];
            cumulative += (dx * dx + dy * dy).sqrt();
            lens.push(cumulative);
        }
        let total = cumulative;
        let mut apex_idx = 0usize;
        for i in (0..n).rev() {
            if total - lens[i] >= fan_arc_m {
                apex_idx = i;
                break;
            }
        }
        if apex_idx == 0 || apex_idx >= n - 1 {
            continue;
        }
        let apex_pt = poly.points[apex_idx];

        let tx = end_pt[0] - apex_pt[0];
        let tz = end_pt[1] - apex_pt[1];
        let tlen = (tx * tx + tz * tz).sqrt().max(1e-6);
        let tangent = [tx / tlen, tz / tlen];
        let normal = [-tangent[1], tangent[0]];

        // Mouth width (post-`apply_mouth_fan_widths`) drives count and
        // lateral spread so bars distribute across the visible water plane.
        let mouth_width = poly.width[n - 1];
        let mouth_half = mouth_width * 0.5;
        let count = ((mouth_width / MOUTH_ISLAND_SPACING_M).round() as u32)
            .clamp(MOUTH_ISLAND_COUNT_MIN, MOUTH_ISLAND_COUNT_MAX);

        let mut rng = SmallRng::seed_from_u64(
            master_seed ^ (poly_idx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15),
        );
        for i in 0..count {
            let slot_t = if count <= 1 {
                0.5
            } else {
                i as f32 / (count as f32 - 1.0)
            };
            // Even perpendicular spread across the mouth, leaving a margin
            // near the banks via `SPREAD_FRAC`. Small jitter breaks the
            // perfect-lattice silhouette.
            let perp_offset = (slot_t * 2.0 - 1.0) * mouth_half * MOUTH_ISLAND_SPREAD_FRAC;
            let perp_jitter = (rng.gen::<f32>() * 2.0 - 1.0) * MOUTH_ISLAND_PERP_JITTER_M;
            let lateral = perp_offset + perp_jitter;

            // Tip sits around mid-fan, end near the mouth — fingers reach
            // halfway up the wedge before fanning out to the open mouth.
            let tip_frac = rng.gen_range(
                MOUTH_ISLAND_TIP_ALONG_FRAC_MIN..=MOUTH_ISLAND_TIP_ALONG_FRAC_MAX,
            );
            let end_frac = rng.gen_range(
                MOUTH_ISLAND_END_ALONG_FRAC_MIN..=MOUTH_ISLAND_END_ALONG_FRAC_MAX,
            );
            let tip_along = tip_frac * fan_arc_m;
            let end_along = end_frac * fan_arc_m;

            // Tip lateral is compressed toward the centerline so fingers
            // splay outward from apex to mouth rather than running as
            // parallel bars.
            let tip_lateral = lateral * MOUTH_ISLAND_TIP_LATERAL_FRAC;
            let tip_x = apex_pt[0] + tangent[0] * tip_along + normal[0] * tip_lateral;
            let tip_z = apex_pt[1] + tangent[1] * tip_along + normal[1] * tip_lateral;
            let end_x = apex_pt[0] + tangent[0] * end_along + normal[0] * lateral;
            let end_z = apex_pt[1] + tangent[1] * end_along + normal[1] * lateral;

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

/// Straighten the last `RIVER_MOUTH_FAN_ARC_CELLS` cells of arc of each
/// sea-bound polyline into a clean apex→mouth line, then scale per-vertex
/// `width` along the same window via a 1/x reciprocal curve (peak at the
/// mouth, 1× at the apex). Without straightening, the underlying 8-
/// connected cell trace leaves small kinks in the fan zone that the bake
/// carve faithfully reproduces — visible as dents in the wedge bank.
/// Both adjustments share the same arc-length walk so they stay aligned.
fn apply_mouth_fan_widths(
    rivers_world: &mut [RiverWorldPolyline],
    map: &GlobalMap,
    dist_to_land: &[u16],
) {
    let k = RIVER_MOUTH_FAN_SHARPNESS;
    let s_norm = (1.0 + k) / k;
    let inv_one_plus_k = 1.0 / (1.0 + k);
    let arc_m = RIVER_MOUTH_FAN_ARC_CELLS * map.config.meters_per_cell();
    let inv_arc = 1.0 / arc_m.max(1e-3);
    let bank_noise = PerlinNoise3D::new(map.config.seed ^ 0xB44E_5099_F1A8_C3D7);
    let wobble_freq = 1.0 / RIVER_MOUTH_FAN_BANK_WOBBLE_WAVELENGTH_M.max(1e-3);

    for poly in rivers_world.iter_mut() {
        let n = poly.points.len();
        if n < 2 {
            continue;
        }
        let end = poly.points[n - 1];
        if sample_base_elevation(map, dist_to_land, end[0], end[1]) >= 0.0 {
            continue;
        }

        // Cumulative arc length on the original (curvy) polyline. Captured
        // before straightening so `dist_from_end` matches what the river
        // map intended.
        let mut cumulative = 0.0f32;
        let mut lens: Vec<f32> = Vec::with_capacity(n);
        lens.push(0.0);
        for i in 1..n {
            let dx = poly.points[i][0] - poly.points[i - 1][0];
            let dy = poly.points[i][1] - poly.points[i - 1][1];
            cumulative += (dx * dx + dy * dy).sqrt();
            lens.push(cumulative);
        }
        let total = cumulative;

        // Locate the apex: the deepest interior vertex still outside the
        // fan window. Everything between it and the end gets snapped to the
        // apex→end chord (parameterized by arc fraction so vertex ordering
        // is preserved), then perturbed perpendicular to the axis by low-
        // frequency Perlin noise so the bank doesn't read as CG-perfect.
        let mut apex_idx: Option<usize> = None;
        for i in (0..n).rev() {
            if total - lens[i] >= arc_m {
                apex_idx = Some(i);
                break;
            }
        }
        if let Some(apex_idx) = apex_idx {
            if apex_idx + 1 < n - 1 {
                let apex = poly.points[apex_idx];
                let dx_axis = end[0] - apex[0];
                let dy_axis = end[1] - apex[1];
                let axis_len = (dx_axis * dx_axis + dy_axis * dy_axis).sqrt().max(1e-3);
                let perp_x = -dy_axis / axis_len;
                let perp_y = dx_axis / axis_len;
                let apex_arc = lens[apex_idx];
                let zone_arc = (total - apex_arc).max(1e-3);
                for i in (apex_idx + 1)..(n - 1) {
                    let frac = (lens[i] - apex_arc) / zone_arc;
                    let straight_x = apex[0] + dx_axis * frac;
                    let straight_y = apex[1] + dy_axis * frac;
                    let noise = bank_noise.sample(
                        straight_x * wobble_freq,
                        straight_y * wobble_freq,
                        0.0,
                    );
                    let wobble = noise * RIVER_MOUTH_FAN_BANK_WOBBLE_M * frac;
                    poly.points[i][0] = straight_x + perp_x * wobble;
                    poly.points[i][1] = straight_y + perp_y * wobble;
                }
            }
        }

        for i in 0..n {
            let t = ((total - lens[i]) * inv_arc).clamp(0.0, 1.0);
            let s = (1.0 / (k * t + 1.0) - inv_one_plus_k) * s_norm;
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
