//! Phase 3: hydraulic erosion (dandrino simulation).
//!
//! Faithful port of <https://github.com/dandrino/terrain-erosion-3-ways>
//! `simulation.py`. Each iteration walks the whole grid through six
//! sequential steps:
//!
//! 1. **Rain** — `water += U(0, rain_rate)` on every land cell.
//! 2. **Gradient** — central-difference flow direction (downhill, unit
//!    length); flat cells get a random direction so they still drain.
//! 3. **Neighbor sample** — bilinear lookup of `terrain` at `cell + ĝ`;
//!    `Δh = current − neighbor` is the local drop along the flow.
//! 4. **Sediment exchange** — capacity = `max(Δh, ε)/cell · v · water · k`;
//!    below capacity → erode at `dissolving_rate`, above → deposit at
//!    `deposition_rate`; never erode more than the local terrain.
//! 5. **Advect** — water and sediment are forward-displaced along ĝ
//!    (mass-conserving bilinear distribution).
//! 6. **Slippage / velocity / evaporation** — gaussian-blur cells whose
//!    slope exceeds `repose_slope`; recompute the per-cell scalar velocity
//!    from the new Δh; multiply water by `1 − evaporation_rate`.
//!
//! Adapted to our world:
//!
//! * The sim runs at `cfg.erosion_sim_res` (downsampled and upsampled
//!   around the call), so a single preview pass costs minutes instead of
//!   hours at full `global_res`. The macro shape is what matters; per-meter
//!   detail comes from the tile baker's high-frequency noise downstream.
//! * Terrain is internally normalized to `[0, 1]` (= `[0, max_elevation_m]`)
//!   so dandrino's unit-less constants apply directly.
//! * `land_mask` keeps sea cells pinned at 0 throughout the run and drains
//!   any water/sediment that crosses the coast — rivers reach the sea and
//!   disappear, the ocean floor never rises.
//! * Post-sim the result is rescaled so its peak matches the pre-sim peak
//!   (in meters), then bilinearly upsampled back to `global_res`. This
//!   preserves world relief even when the sim flattened the absolute peak.
//! * A final Phase 3b shaping pass applies a peak-preserving lowland
//!   power curve and a noise-modulated coastal beach ramp. These are
//!   gameplay-driven adjustments — the sim is faithful, but it doesn't
//!   know we want plains to read as flat or coastlines to slope into the
//!   water rather than meeting it at a small cliff.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_xy, smoothstep, PerlinNoise4D};

const SLIPPAGE_BLUR_SIGMA: f32 = 1.5;
/// Subsystem salt mixed into the sim RNG so erosion's stream is independent
/// of other phases that share the master seed.
const EROSION_RNG_SALT: u64 = 0xDA7E_DA7E_DA7E_DA7E;

/// Run Phase 3. Mutates `map.elevation_m` in place.
pub fn erode_hydraulic(map: &mut GlobalMap) {
    let cfg = map.config.clone();
    let global_res = cfg.global_res as usize;
    if global_res < 4 {
        return;
    }
    let sim_res = if cfg.erosion_sim_res == 0 {
        global_res
    } else {
        (cfg.erosion_sim_res as usize).min(global_res)
    };
    if sim_res < 4 {
        return;
    }
    let iterations = if cfg.erosion_iterations == 0 {
        ((sim_res as f32 * 1.4).ceil() as usize).max(1)
    } else {
        cfg.erosion_iterations as usize
    };
    if iterations == 0 {
        return;
    }

    let max_elev = cfg.max_elevation_m.max(1.0);
    // Pre-sim peak in meters, so the post-sim normalization preserves the
    // pre-erosion dynamic range even when the sim shaved the global max.
    let pre_max = land_peak_m(&map.elevation_m, &map.land_mask);

    let (mut terrain, sim_land) = downsample(
        &map.elevation_m,
        &map.land_mask,
        global_res,
        sim_res,
        max_elev,
    );

    let sim_water = run_simulation(&mut terrain, &sim_land, sim_res, &cfg, iterations);

    upsample_into_elevation(
        &terrain,
        sim_res,
        global_res,
        max_elev,
        pre_max,
        &map.land_mask,
        &mut map.elevation_m,
    );

    map.water_after_erosion = upsample_water(&sim_water, sim_res, global_res, &map.land_mask);

    apply_post_erosion_shaping(map);
}

/// Phase 3b: post-erosion shaping. Four gameplay-driven passes layered
/// on top of the physically-faithful sim output:
///   1. Lowland expansion — peak-preserving power curve so plains read as
///      flat ground instead of a plateau near `base_elevation_m`.
///   2. Plain-band compression — piecewise squash of cells below a pivot
///      so the playable lowland reads as ankle-deep rather than waist-
///      high; cells ≥ pivot pass through unchanged so mountains keep
///      their mass.
///   3. Coast beach ramp — pulls coastal cells down to a configured
///      shoreline height, with a slow noise pattern reserving a fraction
///      of the coast as cliffs.
///   4. Min land floor — raises any remaining sub-floor land cells to
///      `min_land_height_m + small_noise` so river banks always clear
///      the river water surface (0.5 m) and the river mesh's depth-fade
///      can terminate cleanly against terrain.
fn apply_post_erosion_shaping(map: &mut GlobalMap) {
    apply_lowland_expansion(map);
    apply_plain_band_compression(map);
    apply_coast_beach_ramp(map);
    apply_min_land_floor(map);
}

/// Subsystem salt for the floor-bumpiness noise so it doesn't correlate
/// with the cliff or initial-relief patterns.
const MIN_LAND_FLOOR_NOISE_SALT: u64 = 0xF100_F100_F100_F100;

/// Raise every land cell to at least `min_land_height_m` plus a small
/// noise modulation. Runs LAST in Phase 3b so it overrides the beach
/// ramp's pull-down — coastal cells the ramp dropped to ~0.1 m get
/// lifted back up to ~1 m, giving river-mouth tiles the clearance their
/// banks need. Cells already above the floor are left alone, so
/// mountains and inland highs keep their natural shape.
fn apply_min_land_floor(map: &mut GlobalMap) {
    let cfg = &map.config;
    let min_h = cfg.min_land_height_m;
    if min_h <= 0.0 {
        return;
    }
    let amp = cfg.min_land_height_noise_amp_m.max(0.0);
    let wavelength = cfg
        .scaled_cells(cfg.min_land_height_noise_wavelength_cells)
        .max(1.0);
    let freq = 1.0 / wavelength;
    let noise = PerlinNoise4D::new(cfg.seed ^ MIN_LAND_FLOOR_NOISE_SALT);

    let res = cfg.global_res as usize;
    let world_width = res as f32;
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] != 1 {
                continue;
            }
            // Half-amplitude noise in [-1, 1] → modulation in [-amp/2, +amp/2]
            // so peak-to-peak is `amp` as documented.
            let n = fbm_wrap_xy(&noise, x as f32, y as f32, world_width, freq, 1, 2.0, 0.5);
            let floor = min_h + n * amp * 0.5;
            if map.elevation_m[i] < floor {
                map.elevation_m[i] = floor;
            }
        }
    }
}

/// Maximum land elevation in `elevation`, floored at 1e-6 so callers can
/// divide by it without an extra guard.
fn land_peak_m(elevation: &[f32], land_mask: &[u8]) -> f32 {
    elevation
        .iter()
        .copied()
        .zip(land_mask.iter().copied())
        .filter_map(|(h, m)| if m == 1 { Some(h) } else { None })
        .fold(0.0f32, f32::max)
        .max(1e-6)
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn apply_plain_band_compression(map: &mut GlobalMap) {
    let pivot = map.config.plain_band_pivot_m;
    let q = map.config.plain_band_power;
    if pivot <= 0.0 || !(q > 0.0) || q == 1.0 {
        return;
    }
    for (h, &m) in map.elevation_m.iter_mut().zip(map.land_mask.iter()) {
        if m != 1 || *h >= pivot {
            continue;
        }
        let t = (*h / pivot).max(0.0);
        *h = pivot * t.powf(q);
    }
}

#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn apply_lowland_expansion(map: &mut GlobalMap) {
    let p = map.config.lowland_expansion_power;
    if !(p > 0.0) || p == 1.0 {
        return;
    }
    let peak = land_peak_m(&map.elevation_m, &map.land_mask);
    for (h, &m) in map.elevation_m.iter_mut().zip(map.land_mask.iter()) {
        if m != 1 {
            continue;
        }
        let t = (*h / peak).clamp(0.0, 1.0);
        *h = peak * t.powf(p);
    }
}

const COAST_CLIFF_NOISE_SALT: u64 = 0xC11F_FACE_C11F_FACE;
/// Soft-threshold band on each side of the cliff cutoff. Wider = more
/// gradual transitions between cliff and beach sections.
const COAST_CLIFF_THRESHOLD_BAND: f32 = 0.12;

fn apply_coast_beach_ramp(map: &mut GlobalMap) {
    let cfg = &map.config;
    let beach_d_cells = cfg.scaled_cells(cfg.coast_beach_distance_cells as f32);
    if beach_d_cells <= 0.0 {
        return;
    }
    let beach_max = cfg.coast_beach_max_height_m.max(0.0);
    let cliff_frac = cfg.coast_cliff_fraction.clamp(0.0, 1.0);
    let cliff_wl = cfg.scaled_cells(cfg.coast_cliff_wavelength_cells).max(1.0);
    let cliff_freq = 1.0 / cliff_wl;
    let cliff_thr = 1.0 - cliff_frac;

    let res = cfg.global_res as usize;
    let dist = bfs_distance_from(&map.land_mask, res, 0, None);
    let cliff_noise = PerlinNoise4D::new(cfg.seed ^ COAST_CLIFF_NOISE_SALT);
    let world_width = res as f32;

    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] != 1 {
                continue;
            }
            let d = dist[i];
            if d == u16::MAX || (d as f32) >= beach_d_cells {
                continue;
            }
            // Slow X-wrapped noise on the world grid: clusters adjacent
            // coast cells into homogenous beach- or cliff-classified
            // stretches, so the pattern doesn't dither cell-by-cell.
            let n = fbm_wrap_xy(
                &cliff_noise,
                x as f32,
                y as f32,
                world_width,
                cliff_freq,
                1,
                2.0,
                0.5,
            );
            let nu = (n * 0.5 + 0.5).clamp(0.0, 1.0);
            let cliff_factor = smoothstep(
                cliff_thr - COAST_CLIFF_THRESHOLD_BAND,
                cliff_thr + COAST_CLIFF_THRESHOLD_BAND,
                nu,
            );
            let t = (d as f32 / beach_d_cells).clamp(0.0, 1.0);
            let ramp_t = smoothstep(0.0, 1.0, t);
            let current = map.elevation_m[i];
            let beach_target = beach_max + (current - beach_max) * ramp_t;
            let new_h = beach_target * (1.0 - cliff_factor) + current * cliff_factor;
            // Only lower; never raise. Ensures the ramp can't push a
            // cell above its natural elevation when the noise puts a
            // cliff section adjacent to a low-base cell.
            if new_h < current {
                map.elevation_m[i] = new_h.max(0.0);
            }
        }
    }
}

fn run_simulation(
    terrain: &mut [f32],
    land: &[u8],
    res: usize,
    cfg: &super::config::WorldGenConfig,
    iterations: usize,
) -> Vec<f32> {
    let cell_width = cfg.erosion_cell_width.max(1e-3);
    let cell_area = cell_width * cell_width;
    let rain_rate = cfg.erosion_rain_rate.max(0.0) * cell_area;
    let evap_rate = cfg.erosion_evaporation_rate.clamp(0.0, 1.0);
    let min_h_delta = cfg.erosion_min_height_delta.max(0.0);
    let repose_slope = cfg.erosion_repose_slope.max(0.0);
    let gravity = cfg.erosion_gravity.max(0.0);
    let capacity_k = cfg.erosion_sediment_capacity.max(0.0);
    let dissolving_rate = cfg.erosion_dissolving_rate.clamp(0.0, 1.0);
    let deposition_rate = cfg.erosion_deposition_rate.clamp(0.0, 1.0);

    let total = res * res;
    let mut sediment = vec![0.0f32; total];
    let mut water = vec![0.0f32; total];
    // Time-integrated water accumulator. Each iteration adds the post-evap
    // water field; channels visited by many advection passes accumulate
    // proportionally more than off-channel cells whose only inflow is the
    // single rain pulse per iter. Stable signal for downstream routing.
    let mut water_visits = vec![0.0f32; total];
    let warmup = iterations / 4;
    let mut velocity = vec![0.0f32; total];
    let mut gx = vec![0.0f32; total];
    let mut gy = vec![0.0f32; total];
    let mut next_water = vec![0.0f32; total];
    let mut next_sediment = vec![0.0f32; total];
    let mut blur_tmp = vec![0.0f32; total];
    let mut blur_out = vec![0.0f32; total];
    // Lift the slippage blur kernel out of the per-iter call site since
    // sigma is constant for the whole run.
    let blur_kernel = build_gaussian_kernel(SLIPPAGE_BLUR_SIGMA);

    let mut rng = SmallRng::seed_from_u64(cfg.seed ^ EROSION_RNG_SALT);
    let report_every = (iterations / 10).max(1);
    let mut next_report = report_every;

    for iter in 0..iterations {
        rain_step(&mut water, land, rain_rate, &mut rng);
        compute_gradient(terrain, land, res, &mut gx, &mut gy, &mut rng);
        sediment_exchange_step(
            terrain,
            land,
            res,
            cell_width,
            min_h_delta,
            capacity_k,
            dissolving_rate,
            deposition_rate,
            &water,
            &velocity,
            &mut sediment,
            &gx,
            &gy,
        );
        displace(&sediment, &gx, &gy, res, &mut next_sediment);
        displace(&water, &gx, &gy, res, &mut next_water);
        std::mem::swap(&mut sediment, &mut next_sediment);
        std::mem::swap(&mut water, &mut next_water);

        apply_slippage(
            terrain,
            res,
            cell_width,
            repose_slope,
            &blur_kernel,
            &mut blur_tmp,
            &mut blur_out,
        );
        update_velocity(
            terrain,
            land,
            res,
            cell_width,
            gravity,
            &gx,
            &gy,
            &mut velocity,
        );
        evaporate(&mut water, evap_rate);
        clear_sea(land, terrain, &mut water, &mut sediment, &mut velocity);

        if iter >= warmup {
            for (acc, &w) in water_visits.iter_mut().zip(water.iter()) {
                *acc += w;
            }
        }

        let done = iter + 1;
        if done == iterations || done >= next_report {
            let pct = done * 100 / iterations;
            eprintln!("    erosion: {pct:>3}% ({done}/{iterations})");
            while next_report <= done {
                next_report += report_every;
            }
        }
    }

    water_visits
}

// --- Sim steps ----------------------------------------------------------

fn rain_step(water: &mut [f32], land: &[u8], rate: f32, rng: &mut SmallRng) {
    if rate <= 0.0 {
        return;
    }
    for (w, &m) in water.iter_mut().zip(land.iter()) {
        if m == 1 {
            *w += rng.gen::<f32>() * rate;
        }
    }
}

fn compute_gradient(
    terrain: &[f32],
    land: &[u8],
    res: usize,
    gx: &mut [f32],
    gy: &mut [f32],
    rng: &mut SmallRng,
) {
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if land[i] == 0 {
                gx[i] = 0.0;
                gy[i] = 0.0;
                continue;
            }
            // (fx, fy) point in the +X / +Y direction when the left/up
            // neighbor is higher — i.e. *downhill* in our index space, so
            // water flows in this direction.
            let (mut fx, mut fy) = central_diff(terrain, res, x, y);
            let len = (fx * fx + fy * fy).sqrt();
            if len > 1e-10 {
                fx /= len;
                fy /= len;
            } else {
                // Flat cell: pick a random direction so accumulated water
                // still has somewhere to go (matches dandrino's exp(2πi·rand)).
                let a = rng.gen::<f32>() * std::f32::consts::TAU;
                fx = a.cos();
                fy = a.sin();
            }
            gx[i] = fx;
            gy[i] = fy;
        }
    }
}

/// Central-difference of `field` at `(x, y)`, both axes wrapping.
/// Returns the unscaled `(left-right, up-down)/2` so callers apply their
/// own per-cell-width or normalization step.
#[inline]
fn central_diff(field: &[f32], res: usize, x: usize, y: usize) -> (f32, f32) {
    let res_i = res as i32;
    let last = res - 1;
    let xl = ((x as i32 - 1).rem_euclid(res_i)) as usize;
    let xr = ((x as i32 + 1).rem_euclid(res_i)) as usize;
    let yu = if y == 0 { last } else { y - 1 };
    let yd = if y == last { 0 } else { y + 1 };
    let dx = 0.5 * (field[y * res + xl] - field[y * res + xr]);
    let dy = 0.5 * (field[yu * res + x] - field[yd * res + x]);
    (dx, dy)
}

#[allow(clippy::too_many_arguments)]
fn sediment_exchange_step(
    terrain: &mut [f32],
    land: &[u8],
    res: usize,
    cell_width: f32,
    min_h_delta: f32,
    capacity_k: f32,
    dissolving_rate: f32,
    deposition_rate: f32,
    water: &[f32],
    velocity: &[f32],
    sediment: &mut [f32],
    gx: &[f32],
    gy: &[f32],
) {
    let inv_cw = 1.0 / cell_width.max(1e-6);
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if land[i] == 0 {
                continue;
            }
            let nx = x as f32 + gx[i];
            let ny = y as f32 + gy[i];
            let neighbor = bilinear_sample(terrain, res, nx, ny);
            let h_delta = terrain[i] - neighbor;

            let drop_pos = h_delta.max(min_h_delta);
            let capacity = drop_pos * inv_cw * velocity[i] * water[i] * capacity_k;

            // dandrino's three-way select.
            //   h_delta < 0  → climbing: deposit min(|Δh|, sediment) (+).
            //   sed > cap    → over capacity: deposit fraction of excess (+).
            //   else         → erode fraction of capacity-deficit (−).
            let mut deposited = if h_delta < 0.0 {
                h_delta.min(sediment[i]) // negative or zero
            } else if sediment[i] > capacity {
                deposition_rate * (sediment[i] - capacity) // positive
            } else {
                dissolving_rate * (sediment[i] - capacity) // ≤ 0
            };
            // Don't erode more than the current Δh worth — the cell can't
            // sink below its downstream neighbor.
            if deposited < -h_delta {
                deposited = -h_delta;
            }
            sediment[i] -= deposited;
            terrain[i] += deposited;
            if terrain[i] < 0.0 {
                terrain[i] = 0.0;
            }
            if sediment[i] < 0.0 {
                sediment[i] = 0.0;
            }
        }
    }
}

/// Mass-conserving forward advection: distribute each cell's value into the
/// 4 cells overlapped by the unit-square shifted by `(gx, gy)`. dandrino's
/// `displace` written out as a 9-cell forward pass (most weights are zero).
fn displace(src: &[f32], gx: &[f32], gy: &[f32], res: usize, dst: &mut [f32]) {
    dst.fill(0.0);
    let res_i = res as i32;
    let last = res - 1;
    for y in 0..res {
        let ym = if y == 0 { last } else { y - 1 };
        let yp = if y == last { 0 } else { y + 1 };
        for x in 0..res {
            let i = y * res + x;
            let v = src[i];
            if v == 0.0 {
                continue;
            }
            let dx_f = gx[i];
            let dy_f = gy[i];
            let xm = ((x as i32 - 1).rem_euclid(res_i)) as usize;
            let xp = ((x as i32 + 1).rem_euclid(res_i)) as usize;
            // Bilinear forward-distribution into the 4 cells overlapped by
            // the unit square shifted by (gx, gy). Either xneg or xpos has
            // weight 0, same for y, so only ~4 of 9 combos do real work.
            let cols = [
                (xm, (-dx_f).max(0.0)),
                (x, (1.0 - dx_f.abs()).max(0.0)),
                (xp, dx_f.max(0.0)),
            ];
            let rows = [
                (ym, (-dy_f).max(0.0)),
                (y, (1.0 - dy_f.abs()).max(0.0)),
                (yp, dy_f.max(0.0)),
            ];
            for (ry, wy) in rows {
                if wy <= 0.0 {
                    continue;
                }
                let row = ry * res;
                let vy = v * wy;
                for (cx, wx) in cols {
                    if wx <= 0.0 {
                        continue;
                    }
                    dst[row + cx] += vy * wx;
                }
            }
        }
    }
}

fn apply_slippage(
    terrain: &mut [f32],
    res: usize,
    cell_width: f32,
    repose_slope: f32,
    blur_kernel: &[f32],
    blur_tmp: &mut [f32],
    blur_out: &mut [f32],
) {
    if repose_slope <= 0.0 {
        return;
    }
    gaussian_blur(terrain, res, blur_kernel, blur_tmp, blur_out);
    let inv_cw = 1.0 / cell_width.max(1e-6);
    // Squared comparison avoids a sqrt per cell.
    let thresh_sq = repose_slope * repose_slope;
    for y in 0..res {
        for x in 0..res {
            let (dx, dy) = central_diff(terrain, res, x, y);
            let slope_sq = (dx * dx + dy * dy) * inv_cw * inv_cw;
            if slope_sq > thresh_sq {
                let i = y * res + x;
                terrain[i] = blur_out[i];
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn update_velocity(
    terrain: &[f32],
    land: &[u8],
    res: usize,
    cell_width: f32,
    gravity: f32,
    gx: &[f32],
    gy: &[f32],
    velocity: &mut [f32],
) {
    // Re-sample the neighbor (rather than reusing Δh from `sediment_exchange`)
    // because `apply_slippage` runs in between and may have flattened the
    // local slope; velocity needs to reflect the post-slippage terrain.
    let inv_cw = 1.0 / cell_width.max(1e-6);
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if land[i] == 0 {
                velocity[i] = 0.0;
                continue;
            }
            let nx = x as f32 + gx[i];
            let ny = y as f32 + gy[i];
            let neighbor = bilinear_sample(terrain, res, nx, ny);
            let h_delta = terrain[i] - neighbor;
            velocity[i] = gravity * h_delta * inv_cw;
        }
    }
}

fn evaporate(water: &mut [f32], rate: f32) {
    if rate <= 0.0 {
        return;
    }
    let factor = 1.0 - rate;
    for w in water.iter_mut() {
        *w *= factor;
    }
}

fn clear_sea(
    land: &[u8],
    terrain: &mut [f32],
    water: &mut [f32],
    sediment: &mut [f32],
    velocity: &mut [f32],
) {
    for (i, &m) in land.iter().enumerate() {
        if m == 0 {
            terrain[i] = 0.0;
            water[i] = 0.0;
            sediment[i] = 0.0;
            velocity[i] = 0.0;
        }
    }
}

// --- Helpers ------------------------------------------------------------

#[inline]
fn bilinear_sample(field: &[f32], res: usize, fx: f32, fy: f32) -> f32 {
    let ix = fx.floor() as i32;
    let iy = fy.floor() as i32;
    let dx = fx - ix as f32;
    let dy = fy - iy as f32;
    let res_i = res as i32;
    let ix0 = ix.rem_euclid(res_i) as usize;
    let ix1 = (ix + 1).rem_euclid(res_i) as usize;
    let iy0 = iy.rem_euclid(res_i) as usize;
    let iy1 = (iy + 1).rem_euclid(res_i) as usize;
    let v00 = field[iy0 * res + ix0];
    let v10 = field[iy0 * res + ix1];
    let v01 = field[iy1 * res + ix0];
    let v11 = field[iy1 * res + ix1];
    let top = v00 * (1.0 - dx) + v10 * dx;
    let bot = v01 * (1.0 - dx) + v11 * dx;
    top * (1.0 - dy) + bot * dy
}

/// Build a normalized 1-D gaussian kernel with `radius = ceil(3·sigma)`,
/// suitable for the separable two-pass blur below.
fn build_gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil().max(1.0) as usize;
    let mut k = vec![0.0f32; 2 * radius + 1];
    let inv2sig2 = 1.0 / (2.0 * sigma * sigma);
    let mut sum = 0.0;
    for (i, slot) in k.iter_mut().enumerate() {
        let xc = i as f32 - radius as f32;
        *slot = (-xc * xc * inv2sig2).exp();
        sum += *slot;
    }
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn gaussian_blur(src: &[f32], res: usize, kernel: &[f32], tmp: &mut [f32], dst: &mut [f32]) {
    let radius = kernel.len() / 2;
    let res_i = res as i32;
    // Horizontal pass (X wraps).
    for y in 0..res {
        let row = y * res;
        for x in 0..res {
            let mut s = 0.0f32;
            for (k_idx, &w) in kernel.iter().enumerate() {
                let dx = k_idx as i32 - radius as i32;
                let xi = (x as i32 + dx).rem_euclid(res_i) as usize;
                s += w * src[row + xi];
            }
            tmp[row + x] = s;
        }
    }
    // Vertical pass (Y wraps).
    for y in 0..res {
        for x in 0..res {
            let mut s = 0.0f32;
            for (k_idx, &w) in kernel.iter().enumerate() {
                let dy = k_idx as i32 - radius as i32;
                let yi = (y as i32 + dy).rem_euclid(res_i) as usize;
                s += w * tmp[yi * res + x];
            }
            dst[y * res + x] = s;
        }
    }
}

fn downsample(
    elevation: &[f32],
    land: &[u8],
    src_res: usize,
    dst_res: usize,
    max_elev: f32,
) -> (Vec<f32>, Vec<u8>) {
    let total = dst_res * dst_res;
    let mut terrain = vec![0.0f32; total];
    let mut sim_land = vec![0u8; total];
    let scale = src_res as f32 / dst_res as f32;
    let src_i = src_res as i32;
    for dy in 0..dst_res {
        for dx in 0..dst_res {
            let sx_f = (dx as f32 + 0.5) * scale - 0.5;
            let sy_f = (dy as f32 + 0.5) * scale - 0.5;
            let h = bilinear_sample(elevation, src_res, sx_f, sy_f);
            terrain[dy * dst_res + dx] = (h / max_elev).clamp(0.0, 1.0);
            // Nearest-neighbor land mask. Bilinear of a binary field would
            // smudge coastlines into halves we'd then have to threshold; the
            // mask is already discrete, so just pick the closest source cell.
            let sx = (sx_f.round() as i32).rem_euclid(src_i) as usize;
            let sy = (sy_f.round() as i32).clamp(0, src_i - 1) as usize;
            sim_land[dy * dst_res + dx] = land[sy * src_res + sx];
        }
    }
    (terrain, sim_land)
}

/// Upsample the sim-resolution water field to `dst_res` and normalize to
/// `[0, 1]` against the 99.5th-percentile sim cell. Channels routinely sit
/// orders of magnitude above off-channel rain accumulation, so a hard
/// max-divide compresses every interesting cell into the bottom end of
/// the range; the high quantile keeps the river cells around 1.0. Sea
/// cells are zeroed (the sim drains coastal water but bilinear smear can
/// leak a touch across the coastline). Returns an empty Vec when the
/// sim produced no signal so callers can treat "no water field"
/// uniformly.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
fn upsample_water(sim_water: &[f32], sim_res: usize, dst_res: usize, dst_land: &[u8]) -> Vec<f32> {
    let max_w = sim_water.iter().copied().fold(0.0f32, f32::max);
    if !(max_w > 0.0) {
        return Vec::new();
    }
    let mut positives: Vec<f32> = sim_water.iter().copied().filter(|&w| w > 0.0).collect();
    if positives.is_empty() {
        return Vec::new();
    }
    let q_idx = ((positives.len() as f32) * 0.995) as usize;
    let q_idx = q_idx.min(positives.len() - 1);
    let q = (*positives.select_nth_unstable_by(q_idx, f32::total_cmp).1).max(1e-12);
    let inv_q = 1.0 / q;

    let mut out = vec![0.0f32; dst_res * dst_res];
    let scale = sim_res as f32 / dst_res as f32;
    for dy in 0..dst_res {
        for dx in 0..dst_res {
            let i = dy * dst_res + dx;
            if dst_land[i] == 0 {
                continue;
            }
            let sx_f = (dx as f32 + 0.5) * scale - 0.5;
            let sy_f = (dy as f32 + 0.5) * scale - 0.5;
            let w = bilinear_sample(sim_water, sim_res, sx_f, sy_f).max(0.0);
            out[i] = (w * inv_q).clamp(0.0, 1.0);
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn upsample_into_elevation(
    sim_terrain: &[f32],
    sim_res: usize,
    dst_res: usize,
    max_elev: f32,
    pre_max: f32,
    dst_land: &[u8],
    dst_elev: &mut [f32],
) {
    // Renormalize sim peak to the pre-erosion peak in meters so the world
    // doesn't shrink to a pancake when the sim shaved the absolute max.
    let post_max = sim_terrain.iter().copied().fold(0.0f32, f32::max).max(1e-6);
    let meter_scale = pre_max / post_max;
    let scale = sim_res as f32 / dst_res as f32;
    for dy in 0..dst_res {
        for dx in 0..dst_res {
            let i = dy * dst_res + dx;
            if dst_land[i] == 0 {
                dst_elev[i] = 0.0;
                continue;
            }
            let sx_f = (dx as f32 + 0.5) * scale - 0.5;
            let sy_f = (dy as f32 + 0.5) * scale - 0.5;
            let h = bilinear_sample(sim_terrain, sim_res, sx_f, sy_f);
            dst_elev[i] = (h * meter_scale).clamp(0.0, max_elev);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;

    fn test_config(res: u32) -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF,
            world_size_m: 4096,
            global_res: res,
            reference_res: res,
            continent_frequency: 1.0 / 64.0,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            continent_seed_count: 3,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 1,
            continent_gap_cells: 0,
            small_island_count: 0,
            // Run the sim at the test resolution (skip downsample) and use a
            // small iteration count so the test finishes quickly.
            erosion_sim_res: res,
            erosion_iterations: 24,
            initial_relief_wavelength_cells: (res as f32 / 4.0).max(8.0),
            ..Default::default()
        }
    }

    #[test]
    fn erosion_preserves_sea_at_zero() {
        let cfg = test_config(96);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        erode_hydraulic(&mut map);
        for i in 0..map.land_mask.len() {
            if map.land_mask[i] == 0 {
                assert_eq!(
                    map.elevation_m[i], 0.0,
                    "sea cell {i} elevated to {}",
                    map.elevation_m[i]
                );
            }
        }
    }

    #[test]
    fn erosion_does_not_exceed_max_elevation() {
        let cfg = test_config(96);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        erode_hydraulic(&mut map);
        let max_observed = map.elevation_m.iter().fold(0.0f32, |a, &b| a.max(b));
        assert!(
            max_observed <= cfg.max_elevation_m + 1e-3,
            "post-erosion max {max_observed} exceeds cap {}",
            cfg.max_elevation_m
        );
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(64);
        let mut a = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut a);
        erode_hydraulic(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut b);
        erode_hydraulic(&mut b);
        assert_eq!(a.elevation_m, b.elevation_m);
    }

    #[test]
    fn min_land_floor_lifts_all_land_above_threshold() {
        // Floor + half-amplitude noise → every land cell must clear
        // (min − amp/2). Coastal cells the beach ramp drops to 0.1 m
        // come back up to ≥ 0.75 m with the default config (1 m floor,
        // 0.5 m amplitude), comfortably above the river water surface.
        let cfg = test_config(96);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        erode_hydraulic(&mut map);
        let lower_bound = cfg.min_land_height_m - cfg.min_land_height_noise_amp_m * 0.5;
        for (i, &h) in map.elevation_m.iter().enumerate() {
            if map.land_mask[i] == 1 {
                assert!(
                    h >= lower_bound - 1e-3,
                    "land cell {i} below floor: {h} < {lower_bound}"
                );
            }
        }
    }

    #[test]
    fn min_land_floor_disabled_when_zero() {
        // `min_land_height_m = 0` must be a true no-op: cells that the
        // beach ramp pulled to 0.1 m stay at 0.1 m.
        let mut cfg = test_config(96);
        cfg.min_land_height_m = 0.0;
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        erode_hydraulic(&mut map);
        let any_below_floor = map
            .elevation_m
            .iter()
            .zip(map.land_mask.iter())
            .any(|(&h, &m)| m == 1 && h < 0.5);
        assert!(
            any_below_floor,
            "with floor disabled some coastal cells should remain below 0.5 m"
        );
    }

    #[test]
    fn displace_conserves_mass() {
        // Random gradient field, random source field. After one displace
        // pass, sums should match (modulo edge clamping in Y).
        let res = 32;
        let total = res * res;
        let mut rng = SmallRng::seed_from_u64(7);
        let src: Vec<f32> = (0..total).map(|_| rng.gen::<f32>()).collect();
        let mut gx = vec![0.0f32; total];
        let mut gy = vec![0.0f32; total];
        for i in 0..total {
            let a = rng.gen::<f32>() * std::f32::consts::TAU;
            gx[i] = a.cos() * 0.7;
            gy[i] = a.sin() * 0.7;
        }
        let mut dst = vec![0.0f32; total];
        displace(&src, &gx, &gy, res, &mut dst);
        let sum_src: f32 = src.iter().sum();
        let sum_dst: f32 = dst.iter().sum();
        assert!(
            (sum_src - sum_dst).abs() < 1e-3,
            "displace lost mass: src {sum_src} vs dst {sum_dst}"
        );
    }

    #[test]
    fn gaussian_blur_preserves_constant_field() {
        let res = 16;
        let total = res * res;
        let src = vec![0.7f32; total];
        let mut tmp = vec![0.0f32; total];
        let mut dst = vec![0.0f32; total];
        let kernel = build_gaussian_kernel(1.5);
        gaussian_blur(&src, res, &kernel, &mut tmp, &mut dst);
        for &v in &dst {
            assert!((v - 0.7).abs() < 1e-4, "constant got perturbed: {v}");
        }
    }
}
