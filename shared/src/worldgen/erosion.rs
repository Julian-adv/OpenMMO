//! Phase 3: hydraulic erosion (particle-based droplet simulation).
//!
//! Each droplet is a virtual water particle that starts at a random land
//! cell, flows downhill under gradient + inertia, carries sediment, and
//! either erodes or deposits material depending on its carrying capacity.
//! Over hundreds of thousands of droplets this carves realistic valleys
//! and drainage channels into the Phase 2 heightmap — the raw material
//! from which Phase 4 will extract river polylines via flow accumulation.
//!
//! Droplet step (per Sebastian Lague / Hans Beyer style):
//!   1. Sample height + gradient bilinearly at current position.
//!   2. Update velocity with inertia: `v = v·inertia − g·(1−inertia)`.
//!   3. Move one unit along the normalized velocity.
//!   4. Sediment capacity = `max(−Δh, min_slope) · |v| · water · factor`.
//!   5. If over capacity or climbing a hill → deposit (bilinear at old pos).
//!      Otherwise → erode around old pos with a radial brush; grow sediment.
//!   6. Evaporate water; terminate if out of map, in sea, or stalled.

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use super::global_map::GlobalMap;

/// Run Phase 3. Mutates `map.elevation_m` in place.
pub fn erode_hydraulic(map: &mut GlobalMap) {
    let cfg = map.config.clone();
    if cfg.erosion_droplet_count == 0 {
        return;
    }
    let res = cfg.global_res as usize;
    let res_f = res as f32;
    // Droplet budget scales with map area so per-cell erosion intensity stays
    // constant across resolutions.
    let droplet_count = cfg.scaled_area_cells(cfg.erosion_droplet_count) as usize;
    // Max steps scales linearly so a droplet traverses the same physical
    // distance before giving up. Floor at 5 to keep short droplets viable.
    let max_steps = cfg.scaled_cells(cfg.erosion_max_steps as f32).max(5.0) as usize;
    let inertia = cfg.erosion_inertia.clamp(0.0, 1.0);
    let capacity_factor = cfg.erosion_capacity_factor.max(0.0);
    let min_slope = cfg.erosion_min_slope.max(0.0);
    let erode_rate = cfg.erosion_rate.clamp(0.0, 1.0);
    let deposit_rate = cfg.erosion_deposition_rate.clamp(0.0, 1.0);
    let evap_rate = cfg.erosion_evaporation_rate.clamp(0.0, 1.0);
    let radius = cfg.scaled_cells(cfg.erosion_radius_cells as f32).max(1.0) as i32;

    // Precompute the erosion brush: (dx, dy, weight) entries within `radius`
    // cells, weighted by a linear falloff that sums to 1. Applied with the
    // cell at the droplet's position as the center.
    let brush = build_brush(radius);

    let mut rng = SmallRng::seed_from_u64(cfg.seed ^ 0xE0DE_E0DE_E0DE_E0DE_u64);
    let elev = &mut map.elevation_m;
    let mask = &map.land_mask;

    // Pre-collect land-cell indices for cheap random starts. The borders
    // (one cell) are excluded so bilinear sampling never reaches out of Y
    // bounds; X wraps.
    let mut land_indices: Vec<u32> = Vec::with_capacity((res * res) / 4);
    for y in 1..(res - 1) {
        for x in 0..res {
            if mask[y * res + x] == 1 {
                land_indices.push((y * res + x) as u32);
            }
        }
    }
    if land_indices.is_empty() {
        return;
    }

    for _ in 0..droplet_count {
        let seed_idx = land_indices[rng.gen_range(0..land_indices.len())] as usize;
        let sy = seed_idx / res;
        let sx = seed_idx % res;
        let mut x = sx as f32 + rng.gen::<f32>();
        let mut y = sy as f32 + rng.gen::<f32>();
        let mut vx = 0.0f32;
        let mut vy = 0.0f32;
        let mut water = 1.0f32;
        let mut sediment = 0.0f32;

        for _ in 0..max_steps {
            let (old_h, gx, gy) = sample_height_and_gradient(elev, res, x, y);

            // Update velocity toward the negative gradient, blending with
            // the droplet's existing momentum.
            vx = vx * inertia - gx * (1.0 - inertia);
            vy = vy * inertia - gy * (1.0 - inertia);
            let vlen = (vx * vx + vy * vy).sqrt();
            if vlen < 1e-6 {
                break;
            }
            let dx = vx / vlen;
            let dy = vy / vlen;

            let nx_raw = x + dx;
            let ny = y + dy;
            // Wrap X, bail on Y out-of-bounds.
            let nx = nx_raw.rem_euclid(res_f);
            if ny < 1.0 || ny >= res_f - 1.0 {
                break;
            }
            // If the droplet entered a sea cell, stop — the sea absorbs it.
            let nix = nx as usize;
            let niy = ny as usize;
            if mask[niy * res + nix] == 0 {
                break;
            }

            let new_h = sample_height(elev, res, nx, ny);
            let h_delta = new_h - old_h;

            // Sediment capacity scales with downhill slope, speed, water.
            let capacity = (-h_delta).max(min_slope) * vlen * water * capacity_factor;

            if h_delta > 0.0 {
                // Climbing — drop enough sediment to fill the hole (or all).
                let drop = h_delta.min(sediment);
                sediment -= drop;
                deposit_bilinear(elev, mask, res, x, y, drop);
            } else if sediment > capacity {
                // Over capacity — deposit the excess bilinearly.
                let drop = (sediment - capacity) * deposit_rate;
                sediment -= drop;
                deposit_bilinear(elev, mask, res, x, y, drop);
            } else {
                // Under capacity — erode around the current position.
                let take = ((capacity - sediment) * erode_rate).min(-h_delta);
                let taken = erode_with_brush(elev, mask, res, x, y, take, &brush);
                sediment += taken;
            }

            x = nx;
            y = ny;
            water *= 1.0 - evap_rate;
            if water < 1e-4 {
                break;
            }
        }
    }
}

/// Bilinear height sample at (fx, fy), X wrapping, Y clamped.
#[inline]
fn sample_height(elev: &[f32], res: usize, fx: f32, fy: f32) -> f32 {
    let ix = fx.floor() as i32;
    let iy = fy.floor() as i32;
    let dx = fx - ix as f32;
    let dy = fy - iy as f32;
    let ix0 = (ix.rem_euclid(res as i32)) as usize;
    let ix1 = ((ix + 1).rem_euclid(res as i32)) as usize;
    let iy0 = iy.clamp(0, res as i32 - 1) as usize;
    let iy1 = (iy + 1).clamp(0, res as i32 - 1) as usize;
    let h00 = elev[iy0 * res + ix0];
    let h10 = elev[iy0 * res + ix1];
    let h01 = elev[iy1 * res + ix0];
    let h11 = elev[iy1 * res + ix1];
    let top = h00 * (1.0 - dx) + h10 * dx;
    let bot = h01 * (1.0 - dx) + h11 * dx;
    top * (1.0 - dy) + bot * dy
}

/// Bilinear height + gradient at (fx, fy). Gradient is analytic from the
/// four corner heights of the surrounding cell.
#[inline]
fn sample_height_and_gradient(elev: &[f32], res: usize, fx: f32, fy: f32) -> (f32, f32, f32) {
    let ix = fx.floor() as i32;
    let iy = fy.floor() as i32;
    let dx = fx - ix as f32;
    let dy = fy - iy as f32;
    let ix0 = (ix.rem_euclid(res as i32)) as usize;
    let ix1 = ((ix + 1).rem_euclid(res as i32)) as usize;
    let iy0 = iy.clamp(0, res as i32 - 1) as usize;
    let iy1 = (iy + 1).clamp(0, res as i32 - 1) as usize;
    let h00 = elev[iy0 * res + ix0];
    let h10 = elev[iy0 * res + ix1];
    let h01 = elev[iy1 * res + ix0];
    let h11 = elev[iy1 * res + ix1];
    let top = h00 * (1.0 - dx) + h10 * dx;
    let bot = h01 * (1.0 - dx) + h11 * dx;
    let h = top * (1.0 - dy) + bot * dy;
    // Analytic gradient of the bilinear patch.
    let gx = (h10 - h00) * (1.0 - dy) + (h11 - h01) * dy;
    let gy = (h01 - h00) * (1.0 - dx) + (h11 - h10) * dx;
    (h, gx, gy)
}

/// Deposit `amount` of sediment at (fx, fy) using bilinear weights across
/// the 4 surrounding cells. Respects the land mask — sea cells don't
/// accumulate elevation (otherwise we'd raise ocean floor above 0).
#[inline]
fn deposit_bilinear(elev: &mut [f32], mask: &[u8], res: usize, fx: f32, fy: f32, amount: f32) {
    if amount == 0.0 {
        return;
    }
    let ix = fx.floor() as i32;
    let iy = fy.floor() as i32;
    let dx = fx - ix as f32;
    let dy = fy - iy as f32;
    let ix0 = (ix.rem_euclid(res as i32)) as usize;
    let ix1 = ((ix + 1).rem_euclid(res as i32)) as usize;
    let iy0 = iy.clamp(0, res as i32 - 1) as usize;
    let iy1 = (iy + 1).clamp(0, res as i32 - 1) as usize;
    let w00 = (1.0 - dx) * (1.0 - dy);
    let w10 = dx * (1.0 - dy);
    let w01 = (1.0 - dx) * dy;
    let w11 = dx * dy;
    add_if_land(elev, mask, iy0 * res + ix0, amount * w00);
    add_if_land(elev, mask, iy0 * res + ix1, amount * w10);
    add_if_land(elev, mask, iy1 * res + ix0, amount * w01);
    add_if_land(elev, mask, iy1 * res + ix1, amount * w11);
}

#[inline]
fn add_if_land(elev: &mut [f32], mask: &[u8], idx: usize, amount: f32) {
    if mask[idx] == 1 {
        elev[idx] = (elev[idx] + amount).max(0.0);
    }
}

/// Erode cells around (fx, fy) using a precomputed disk brush. Returns the
/// actual amount of material removed (may be less than `amount` if some
/// cells were sea / under-floor).
fn erode_with_brush(
    elev: &mut [f32],
    mask: &[u8],
    res: usize,
    fx: f32,
    fy: f32,
    amount: f32,
    brush: &[(i32, i32, f32)],
) -> f32 {
    if amount <= 0.0 {
        return 0.0;
    }
    let cx = fx as i32;
    let cy = fy as i32;
    let mut removed = 0.0f32;
    for &(dx, dy, w) in brush {
        let ny = cy + dy;
        if ny < 0 || ny >= res as i32 {
            continue;
        }
        let nx = (cx + dx).rem_euclid(res as i32) as usize;
        let idx = ny as usize * res + nx;
        if mask[idx] == 0 {
            continue;
        }
        let to_remove = (amount * w).min(elev[idx]);
        elev[idx] -= to_remove;
        removed += to_remove;
    }
    removed
}

/// Build a disk brush with linear falloff inside `radius` cells, normalized
/// so the weights sum to 1.
fn build_brush(radius: i32) -> Vec<(i32, i32, f32)> {
    let mut out: Vec<(i32, i32, f32)> = Vec::new();
    let rf = radius as f32;
    let mut total = 0.0f32;
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let d = ((dx * dx + dy * dy) as f32).sqrt();
            if d > rf {
                continue;
            }
            let w = 1.0 - d / rf;
            if w <= 0.0 {
                continue;
            }
            out.push((dx, dy, w));
            total += w;
        }
    }
    if total > 0.0 {
        for entry in &mut out {
            entry.2 /= total;
        }
    }
    out
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
            sea_ratio: 0.3,
            mountain_ratio: 0.3,
            continent_frequency: 1.0 / 64.0,
            continent_octaves: 4,
            continent_gain: 0.5,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            sea_channel_strength: 0.0,
            sea_channel_wavelength: 1000.0,
            max_isthmus_width_cells: 0,
            continent_seed_count: 3,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 1,
            continent_gap_cells: 0,
            small_island_count: 0,
            small_island_radius_cells: 10,
            small_island_min_clearance_cells: 20,
            max_elevation_m: 2500.0,
            base_elevation_m: 500.0,
            mountain_amplitude_m: 1800.0,
            plain_amplitude_m: 40.0,
            mountain_selector_wavelength_cells: 64.0,
            detail_wavelength_cells: 16.0,
            y_border_wall_cells: 0,
            y_border_wall_height_m: 0.0,
            erosion_droplet_count: 5_000,
            erosion_max_steps: 40,
            erosion_inertia: 0.05,
            erosion_capacity_factor: 4.0,
            erosion_min_slope: 0.01,
            erosion_rate: 0.3,
            erosion_deposition_rate: 0.3,
            erosion_evaporation_rate: 0.02,
            erosion_radius_cells: 3,
            settlement_target_count: 5,
            settlement_min_spacing_cells: 10,
            settlement_max_elevation_m: 1200.0,
            settlement_max_slope: 0.35,
            settlement_river_flow_threshold: 20.0,
            settlement_along_road_count: 0,
            settlement_inland_buffer_cells: 0,
            settlement_coastal_spacing_mult: 1.0,
            settlement_mouth_count: 0,
            settlement_phase_a_spacing_mult: 1.0,
            settlement_south_edge_exclusion_m: 0.0,
            settlement_max_gap_m: 0.0,
            river_gap_max_m: 0.0,
            road_extra_neighbors: 0,
            elevation_hotspots: Vec::new(),
            river_carve_paths: Vec::new(),
        }
    }

    #[test]
    fn erosion_preserves_sea_at_zero() {
        let cfg = test_config(128);
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
        let cfg = test_config(128);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        erode_hydraulic(&mut map);
        // Erosion could deposit on top of mountains if sediment is dropped
        // there; allow a little slack but require no runaway growth.
        let max_observed = map.elevation_m.iter().fold(0.0f32, |a, &b| a.max(b));
        assert!(
            max_observed <= cfg.max_elevation_m * 1.2,
            "post-erosion max {max_observed} wildly exceeds cap {}",
            cfg.max_elevation_m
        );
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(96);
        let mut a = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut a);
        erode_hydraulic(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut b);
        erode_hydraulic(&mut b);
        assert_eq!(a.elevation_m, b.elevation_m);
    }

    #[test]
    fn erosion_reduces_peak_height() {
        // Run erosion with a lot of droplets and check the tallest mountain
        // got knocked down (standard consequence of hydraulic erosion).
        let mut cfg = test_config(128);
        cfg.erosion_droplet_count = 30_000;
        let mut before = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut before);
        let peak_before = before.elevation_m.iter().fold(0.0f32, |a, &b| a.max(b));
        let mut after = before.clone();
        erode_hydraulic(&mut after);
        let peak_after = after.elevation_m.iter().fold(0.0f32, |a, &b| a.max(b));
        assert!(
            peak_after < peak_before,
            "erosion didn't reduce peak: before {peak_before}, after {peak_after}"
        );
    }
}
