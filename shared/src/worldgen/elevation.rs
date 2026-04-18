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
const DETAIL_GAIN: f32 = 0.5;
const MOUNTAIN_SELECTOR_OCTAVES: u32 = 3;
const MOUNTAIN_SELECTOR_GAIN: f32 = 0.55;

/// Run Phase 2: fill `map.elevation_m` based on the land mask and config.
pub fn generate_elevation(map: &mut GlobalMap) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let world_width = res as f32;

    // --- Distance from coast: normalized to 0..1 using a saturation depth.
    // Deeper than `coast_depth_cells` reads as "fully inland".
    let dist_land = bfs_distance_from(&map.land_mask, res, 0);
    let coast_depth_cells = 400.0f32; // ~3.2km at 8m/cell — interior plateau
    let mut coast_norm = vec![0.0f32; total];
    for i in 0..total {
        if map.land_mask[i] == 1 {
            let d = dist_land[i] as f32;
            coast_norm[i] = (d / coast_depth_cells).clamp(0.0, 1.0);
        }
    }

    // --- Noise fields (deterministic per master seed).
    let seed = map.config.seed;
    let detail_noise = PerlinNoise3D::new(seed ^ 0xE1_E_E1_E_E1_E_E1_E_u64);
    let mountain_noise = PerlinNoise3D::new(seed ^ 0xA1_A_A1_A_A1_A_A1_A_u64);
    let detail_freq = 1.0 / map.config.detail_wavelength_cells.max(1.0);
    let mountain_freq = 1.0 / map.config.mountain_selector_wavelength_cells.max(1.0);

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
    let wall_cells = map.config.y_border_wall_cells as usize;
    let wall_h = map.config.y_border_wall_height_m;

    let mut elevation = vec![0.0f32; total];
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] == 0 {
                continue; // sea stays at 0
            }
            // Base: rises smoothly from coast to a plateau with distance.
            let base = smoothstep(0.0, 0.4, coast_norm[i]) * base_h;

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
            let mountain_part = (detail * 0.5 + 0.5).clamp(0.0, 1.0) * mtn_amp;
            let detail_contribution = plain_part * (1.0 - mtn_factor) + mountain_part * mtn_factor;
            let mut h = base + detail_contribution * fade;

            // Y-border mountain wall: take the max of current elevation and a
            // wall height that peaks at the border and falls off inward.
            if wall_cells > 0 {
                let d_border = y.min(res - 1 - y);
                if d_border < wall_cells {
                    // 1.0 at exact border, 0 at wall_cells deep.
                    let t = 1.0 - (d_border as f32 / wall_cells as f32);
                    let wall = smoothstep(0.0, 1.0, t) * wall_h;
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
