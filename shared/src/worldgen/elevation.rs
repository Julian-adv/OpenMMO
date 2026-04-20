//! Phase 2: elevation layering.
//!
//! Converts the binary land mask from Phase 1 into a meter-scale heightmap:
//!   - **base gradient**: land cells rise smoothly with distance from the
//!     coast, saturating at `base_elevation_m` around 40% inland.
//!   - **mountain/plain mask**: a low-frequency fBm noise decides whether a
//!     region is mountainous or flat. Top `mountain_ratio` quantile among
//!     land cells = mountain; rest = plain.
//!   - **detail noise**: high-frequency fBm (octave-rich) provides local
//!     peaks/valleys on top of the base. Amplitude is `mountain_amplitude_m`
//!     in mountains and `plain_amplitude_m` on plains.
//!   - **Y-border mountain wall**: because the Y axis doesn't wrap, land
//!     within `y_border_wall_cells` of the north or south border is lifted
//!     toward `y_border_wall_height_m` to form an impassable range.
//!   - Sea cells always sit at 0 m.

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::noise::{fbm_wrap_x, PerlinNoise3D};

const LACUNARITY: f32 = 2.0;
const DETAIL_OCTAVES: u32 = 6;
/// Low gain so the finest 20 m octave stays ~4 m peak-to-peak in mountain
/// regions, avoiding clusters of small bumps on top of larger hills.
const DETAIL_GAIN: f32 = 0.29;
const MOUNTAIN_SELECTOR_OCTAVES: u32 = 3;
const MOUNTAIN_SELECTOR_GAIN: f32 = 0.55;

/// Run Phase 2: fill `map.elevation_m` based on the land mask and config.
pub fn generate_elevation(map: &mut GlobalMap) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let world_width = res as f32;

    // --- Distance from coast: normalized to 0..1 using a saturation depth.
    // Deeper than `coast_depth_cells` reads as "fully inland". 400 reference
    // cells ≈ 3.2km at the 8m/cell reference scale.
    //
    // The raw BFS field is 4-connected Manhattan, so near irregular
    // coastlines it grows axis-aligned ridges that surface as visible
    // 1-tile bumps on lowland slopes once the tile baker's Catmull-Rom
    // samples them. Blurring before the smoothstep erases the artifact
    // without shifting the macro coast → inland gradient.
    let dist_land = bfs_distance_from(&map.land_mask, res, 0);
    let dist_land_smooth = box_blur_2d(&dist_land, res, 10);
    let coast_depth_cells = map.config.scaled_cells(400.0);
    let mut coast_norm = vec![0.0f32; total];
    for i in 0..total {
        if map.land_mask[i] == 1 {
            coast_norm[i] = (dist_land_smooth[i] / coast_depth_cells).clamp(0.0, 1.0);
        }
    }

    // --- Noise fields (deterministic per master seed).
    let seed = map.config.seed;
    let detail_noise = PerlinNoise3D::new(seed ^ 0xE1_E_E1_E_E1_E_E1_E_u64);
    let mountain_noise = PerlinNoise3D::new(seed ^ 0xA1_A_A1_A_A1_A_A1_A_u64);
    let detail_freq = map
        .config
        .scaled_freq(1.0 / map.config.detail_wavelength_cells.max(1.0));
    let mountain_freq = map
        .config
        .scaled_freq(1.0 / map.config.mountain_selector_wavelength_cells.max(1.0));

    // --- Mountain selector: sample noise, then threshold at quantile so
    // exactly `mountain_ratio` fraction of land cells becomes mountain.
    let mut mountain_score = vec![0.0f32; total];
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] == 0 {
                continue;
            }
            mountain_score[i] = fbm_wrap_x(
                &mountain_noise,
                x as f32,
                y as f32,
                world_width,
                mountain_freq,
                MOUNTAIN_SELECTOR_OCTAVES,
                LACUNARITY,
                MOUNTAIN_SELECTOR_GAIN,
            );
        }
    }
    let mountain_threshold = land_quantile(
        &mountain_score,
        &map.land_mask,
        1.0 - map.config.mountain_ratio.clamp(0.0, 1.0),
    );

    // --- Main elevation loop.
    let base_h = map.config.base_elevation_m;
    let mtn_amp = map.config.mountain_amplitude_m;
    let pln_amp = map.config.plain_amplitude_m;
    let max_h = map.config.max_elevation_m;
    let wall_cells = map
        .config
        .scaled_cells_usize(map.config.y_border_wall_cells);
    let wall_h = map.config.y_border_wall_height_m;

    let mut elevation = vec![0.0f32; total];
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] == 0 {
                continue; // sea stays at 0
            }
            // Base: rises smoothly from coast to a plateau with distance.
            // Wider saturation (0.8) keeps the coast → inland transition
            // gentle so lowland tiles don't read as a single 40 m ridge.
            let base = smoothstep(0.0, 0.8, coast_norm[i]) * base_h;

            // Detail: high-frequency fBm, in [-1, 1] approximately.
            let detail = fbm_wrap_x(
                &detail_noise,
                x as f32,
                y as f32,
                world_width,
                detail_freq,
                DETAIL_OCTAVES,
                LACUNARITY,
                DETAIL_GAIN,
            );

            // Coast fade: dampen amplitude right at the coast so mountains
            // don't start at the water's edge. Ramps up with coast_norm.
            let fade = smoothstep(0.0, 0.15, coast_norm[i]);
            // Smooth blend between plain (symmetric low-amplitude noise) and
            // mountain (positive-bias high-amplitude noise) so the boundary
            // doesn't show a hard step in the heightmap. `mtn_factor`
            // ramps from 0 just below the quantile threshold to 1 well above.
            let mtn_band = 0.15; // ±15% of the selector noise range
            let mtn_factor = smoothstep(
                mountain_threshold - mtn_band,
                mountain_threshold + mtn_band,
                mountain_score[i],
            );
            let plain_part = detail * pln_amp;
            // Couple mountain amplitude to the base-elevation gradient so
            // lowland cells don't inherit the mountain fBm's 100 m+ swings.
            // Cubed so highlands (ratio ≈ 1) keep full amplitude while
            // lowlands get very aggressive damping (0.5 → 0.125).
            let base_frac = (base / base_h).clamp(0.0, 1.0).powi(3);
            let mountain_part = (detail * 0.5 + 0.5).clamp(0.0, 1.0) * mtn_amp * base_frac;
            let detail_contribution = plain_part * (1.0 - mtn_factor) + mountain_part * mtn_factor;
            let mut h = base + detail_contribution * fade;

            // Y-border mountain wall: take the max of current elevation and a
            // wall height that peaks at the border and falls off inward.
            // The detail fBm modulates wall height laterally (±40%) so the
            // wall isn't a uniform ramp — without this, flow accumulation
            // along the wall produces parallel straight-line rivers.
            if wall_cells > 0 {
                let d_border = y.min(res - 1 - y);
                if d_border < wall_cells {
                    let t = 1.0 - (d_border as f32 / wall_cells as f32);
                    let variation = 0.5 + 1.0 * (detail * 0.5 + 0.5);
                    let wall = smoothstep(0.0, 1.0, t) * wall_h * variation;
                    if wall > h {
                        h = wall;
                    }
                }
            }

            elevation[i] = h.clamp(0.0, max_h);
        }
    }

    map.elevation_m = elevation;
}

/// Return the value in `scores` at quantile `q`, considering only land
/// cells. Fine for Phase 2 (one-shot, global map resolution).
fn land_quantile(scores: &[f32], land_mask: &[u8], q: f32) -> f32 {
    let mut vals: Vec<f32> = scores
        .iter()
        .zip(land_mask.iter())
        .filter_map(|(&s, &m)| if m == 1 { Some(s) } else { None })
        .collect();
    if vals.is_empty() {
        return 0.0;
    }
    let idx = ((q.clamp(0.0, 1.0) * vals.len() as f32) as usize).min(vals.len() - 1);
    *vals.select_nth_unstable_by(idx, f32::total_cmp).1
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Separable 2-pass box blur of a `u16` field, returning `f32`. X wraps
/// (world is cylindrical on the X axis), Y clamps. Used to smooth the
/// Manhattan-distance coast field before it drives the base-elevation
/// ramp; see comment at the call site.
fn box_blur_2d(src: &[u16], res: usize, radius: usize) -> Vec<f32> {
    let total = res * res;
    let window = (2 * radius + 1) as f32;
    let mut tmp = vec![0.0f32; total];
    // Horizontal pass (X wraps).
    for y in 0..res {
        let row = y * res;
        for x in 0..res {
            let mut sum = 0.0f32;
            for dx in 0..=(2 * radius) {
                let xi = (x + res + dx).wrapping_sub(radius) % res;
                sum += src[row + xi] as f32;
            }
            tmp[row + x] = sum / window;
        }
    }
    // Vertical pass (Y clamps).
    let mut out = vec![0.0f32; total];
    for y in 0..res {
        for x in 0..res {
            let mut sum = 0.0f32;
            for dy in 0..=(2 * radius) {
                let yi = (y + dy).saturating_sub(radius).min(res - 1);
                sum += tmp[yi * res + x];
            }
            out[y * res + x] = sum / window;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::config::WorldGenConfig;
    use super::super::continent;
    use super::*;

    fn test_config(res: u32) -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF,
            world_size_m: 4096,
            global_res: res,
            reference_res: res,
            sea_ratio: 0.3,
            mountain_ratio: 0.2,
            continent_frequency: 1.0 / 64.0,
            continent_octaves: 4,
            continent_gain: 0.5,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            sea_channel_strength: 0.0,
            sea_channel_wavelength: 1000.0,
            max_isthmus_width_cells: 0,
            continent_seed_count: 5,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 3,
            continent_gap_cells: 10,
            small_island_count: 0,
            small_island_radius_cells: 10,
            small_island_min_clearance_cells: 20,
            max_elevation_m: 2500.0,
            base_elevation_m: 600.0,
            mountain_amplitude_m: 1500.0,
            plain_amplitude_m: 150.0,
            mountain_selector_wavelength_cells: 64.0,
            detail_wavelength_cells: 16.0,
            y_border_wall_cells: 8,
            y_border_wall_height_m: 2200.0,
            erosion_droplet_count: 0,
            erosion_max_steps: 50,
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
            road_extra_neighbors: 0,
        }
    }

    #[test]
    fn sea_cells_are_zero_elevation() {
        let cfg = test_config(64);
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        for i in 0..map.land_mask.len() {
            if map.land_mask[i] == 0 {
                assert_eq!(
                    map.elevation_m[i], 0.0,
                    "sea cell {i} has non-zero elevation"
                );
            }
        }
    }

    #[test]
    fn land_elevation_within_max_cap() {
        let cfg = test_config(64);
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        for &e in &map.elevation_m {
            assert!(
                e >= 0.0 && e <= cfg.max_elevation_m + 1e-3,
                "elevation {e} out of range"
            );
        }
    }

    #[test]
    fn y_border_land_reaches_wall_height() {
        // Force sea_ratio low and border wall tall so *some* land will exist
        // inside the border margin. Check that the tallest elevation there
        // is near the wall height.
        let mut cfg = test_config(128);
        cfg.sea_ratio = 0.1; // mostly land
        cfg.continent_gap_cells = 0;
        cfg.target_continent_count = 1;
        cfg.continent_seed_count = 2;
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        let res = cfg.global_res as usize;
        let margin = cfg.y_border_wall_cells as usize;
        let mut max_at_border = 0.0f32;
        for y in 0..margin {
            for x in 0..res {
                let i = y * res + x;
                if map.land_mask[i] == 1 && map.elevation_m[i] > max_at_border {
                    max_at_border = map.elevation_m[i];
                }
            }
        }
        assert!(
            max_at_border > cfg.y_border_wall_height_m * 0.5,
            "border wall not reaching height: max {max_at_border}"
        );
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(64);
        let mut a = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut b);
        assert_eq!(a.elevation_m, b.elevation_m);
    }

    #[test]
    fn interior_has_more_elevation_variance_than_coast() {
        // At the coast, `fade` damps detail noise so elevation is near 0;
        // inland cells can swing through mountain amplitudes. Compare max
        // elevations in each band rather than means (which are noise-driven).
        let mut cfg = test_config(256);
        cfg.continent_gap_cells = 0;
        cfg.sea_ratio = 0.2;
        cfg.target_continent_count = 1;
        // Disable the Y-border wall so it doesn't dominate "coast" max values
        // for coastal cells that happen to sit near the north/south edge.
        cfg.y_border_wall_cells = 0;
        cfg.y_border_wall_height_m = 0.0;
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        let res = cfg.global_res as usize;
        let dist = bfs_distance_from(&map.land_mask, res, 0);
        let mut coast_max: f32 = 0.0;
        let mut inland_max: f32 = 0.0;
        for i in 0..map.land_mask.len() {
            if map.land_mask[i] != 1 {
                continue;
            }
            if dist[i] <= 2 {
                coast_max = coast_max.max(map.elevation_m[i]);
            } else if dist[i] >= 40 {
                inland_max = inland_max.max(map.elevation_m[i]);
            }
        }
        if inland_max > 0.0 {
            assert!(
                inland_max > coast_max,
                "inland max {inland_max} not > coast max {coast_max} (fade should limit coastal peaks)"
            );
        }
    }
}
