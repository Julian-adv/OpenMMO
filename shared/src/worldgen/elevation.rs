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

use super::config::{ElevationHotspot, WorldGenConfig};
use super::global_map::GlobalMap;
use super::grid::{bfs_distance_extend_from_cell, bfs_distance_from};
use super::noise::{
    fbm_wrap_x, fbm_wrap_x_damped, smoothstep, PerlinNoise, PerlinNoise3D, ValueNoise3D,
};
use super::rivers::{RiverMap, RIVER_PEAK_ELEVATION_FRAC};
use super::vector_features::project_point_to_segment;

const LACUNARITY: f32 = 2.0;
/// Damped fBm self-attenuates further detail in steep regions, so a 7th
/// octave can ride along without producing the "noisy hills on hills" look
/// that vanilla fBm gives at gain 0.5.
const DETAIL_OCTAVES: u32 = 7;
const DETAIL_GAIN: f32 = 0.5;
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
    let dist_land = bfs_distance_from(&map.land_mask, res, 0, None);
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
    // Detail noise carries the analytic-derivative damped fBm: erosion-shaped
    // ridges/valleys via per-octave gradient damping (Iñigo Quílez, "morenoise").
    let detail_noise = ValueNoise3D::new(seed ^ 0xE1_E_E1_E_E1_E_E1_E_u64);
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

            // Detail: derivative-damped fBm in [-1, 1] approximately.
            let detail = fbm_wrap_x_damped(
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

    apply_elevation_hotspots(map, &mut elevation);
    apply_river_carve_paths(map, &mut elevation);

    map.elevation_m = elevation;
}

/// Hotspot disk-boundary "lobes" per radius. ~1.5 keeps the disk lopsided
/// without reading as a periodic ringing.
const HOTSPOT_SHAPE_LOBES: f32 = 1.5;
/// Inner sub-summit count per radius. ~5 places 2-3 visible bumps inside.
const HOTSPOT_DETAIL_LOBES: f32 = 5.0;
/// ± fraction of nominal radius perturbed by the shape noise.
const HOTSPOT_RADIUS_PERTURB: f32 = 0.35;
/// ± fraction of `peak_m` modulated by the detail noise (scaled by `t`).
const HOTSPOT_DETAIL_AMPLITUDE: f32 = 0.3;

fn apply_river_carve_paths(map: &GlobalMap, elevation: &mut [f32]) {
    if map.config.river_carve_paths.is_empty() {
        return;
    }
    let res = map.config.global_res as usize;
    let mpc = map.config.meters_per_cell();

    for path in &map.config.river_carve_paths {
        let (ax, ay) = map.config.world_m_to_cell(path.start_x_m, path.start_y_m);
        let (bx, by) = map.config.world_m_to_cell(path.end_x_m, path.end_y_m);
        let half_w_cells = (path.width_m * 0.5 / mpc).max(0.5);
        let half_w_sq = half_w_cells * half_w_cells;
        let pad = half_w_cells.ceil() as i32 + 1;
        let bb_x_min = (ax.min(bx) as i32) - pad;
        let bb_x_max = (ax.max(bx) as i32) + pad;
        let bb_y_min = ((ay.min(by) as i32) - pad).max(0) as usize;
        let bb_y_max = ((ay.max(by) as i32) + pad).min(res as i32 - 1) as usize;

        for y in bb_y_min..=bb_y_max {
            for xi in bb_x_min..=bb_x_max {
                let x = xi.rem_euclid(res as i32) as usize;
                let i = y * res + x;
                if map.land_mask[i] == 0 {
                    continue;
                }
                let (d_sq, t) = project_point_to_segment(xi as f32, y as f32, ax, ay, bx, by);
                if d_sq > half_w_sq {
                    continue;
                }
                let target = path.start_elev_m * (1.0 - t) + path.end_elev_m * t;
                if elevation[i] > target {
                    elevation[i] = target;
                }
            }
        }
    }
}

fn apply_elevation_hotspots(map: &GlobalMap, elevation: &mut [f32]) {
    apply_hotspots_to(
        &map.config,
        &map.land_mask,
        elevation,
        &map.config.elevation_hotspots,
    );
}

/// Stack a slice of hotspots onto an elevation buffer. Shared between Phase 2's
/// config-driven application and the river-gap fill pass which generates
/// hotspots algorithmically. No-op for an empty slice.
pub(crate) fn apply_hotspots_to(
    cfg: &WorldGenConfig,
    land_mask: &[u8],
    elevation: &mut [f32],
    hotspots: &[ElevationHotspot],
) {
    if hotspots.is_empty() {
        return;
    }
    let res = cfg.global_res as usize;
    let mpc = cfg.meters_per_cell();
    let max_h = cfg.max_elevation_m;
    // Two independent fields: shape perturbs the disk boundary; detail adds
    // sub-summits. Seeded apart so they don't ringfit at scaled frequencies.
    let shape_noise = PerlinNoise::new(cfg.seed ^ 0xC0FFEE_C0FFEE);
    let detail_noise = PerlinNoise::new(cfg.seed ^ 0xDEADBEEF_DEADBEEF);

    for spot in hotspots {
        let (cx, cy) = cfg.world_m_to_cell(spot.center_x_m, spot.center_y_m);
        let r_cells = (spot.radius_m / mpc).max(1.0);
        let shape_freq = HOTSPOT_SHAPE_LOBES / r_cells;
        let detail_freq = HOTSPOT_DETAIL_LOBES / r_cells;
        let r_max = r_cells * (1.0 + HOTSPOT_RADIUS_PERTURB);
        let pad = r_max.ceil() as i32 + 1;
        let y_min = ((cy as i32) - pad).max(0) as usize;
        let y_max = ((cy as i32) + pad).min(res as i32 - 1) as usize;
        let x_min_i = (cx as i32) - pad;
        let x_max_i = (cx as i32) + pad;

        for y in y_min..=y_max {
            for xi in x_min_i..=x_max_i {
                let x = xi.rem_euclid(res as i32) as usize;
                let i = y * res + x;
                if land_mask[i] == 0 {
                    continue;
                }
                let dx = xi as f32 - cx;
                let dy = y as f32 - cy;
                let d_sq = dx * dx + dy * dy;
                if d_sq >= r_max * r_max {
                    continue;
                }
                let s = shape_noise.sample(x as f32 * shape_freq, y as f32 * shape_freq);
                let eff_r = r_cells * (1.0 + HOTSPOT_RADIUS_PERTURB * s);
                if d_sq >= eff_r * eff_r {
                    continue;
                }
                let t = 1.0 - d_sq.sqrt() / eff_r;
                let falloff = t * t;
                let n_detail = detail_noise.sample(x as f32 * detail_freq, y as f32 * detail_freq);
                let modulation = 1.0 + HOTSPOT_DETAIL_AMPLITUDE * n_detail * t;
                let boost = spot.peak_m * falloff * modulation;
                if boost > 0.0 {
                    let mut h = (elevation[i] + boost).min(max_h);
                    if let Some(cap) = spot.cap_elev_m {
                        h = h.min(cap);
                    }
                    elevation[i] = h;
                }
            }
        }
    }
}

/// Peak ratio for gap-fill hotspots. Sits comfortably above
/// `RIVER_PEAK_ELEVATION_FRAC` so the seeded summit clears the river
/// extraction threshold even after the hotspot's detail noise dips.
const RIVER_GAP_PEAK_FRAC: f32 = 0.4;
/// Hotspot disk radius in meters. 2 km keeps the seeded mountain reading
/// as a low hill rather than a spike at typical `peak_m` values.
const RIVER_GAP_RADIUS_M: f32 = 2000.0;
/// Lowland filter for gap-fill candidates: cells already above this
/// fraction of `max_elevation_m` are mountain-grade and either drain
/// rivers already or would clip against the elevation cap if a hotspot
/// were stacked there.
const RIVER_GAP_LOWLAND_FRAC: f32 = 0.4;
const _: () = assert!(
    RIVER_GAP_PEAK_FRAC > RIVER_PEAK_ELEVATION_FRAC,
    "gap-fill peak must clear the river extraction threshold"
);

/// Phase 4 follow-up: drop low mountains in lowlands far from any river so
/// the next river-extraction pass spawns fresh streams there. Returns the
/// list of added hotspots and mutates `map.elevation_m` in place. No-op
/// when `cfg.river_gap_max_m == 0.0`.
///
/// Caller is expected to re-run `rivers::compute_flow` + `extract_rivers`
/// after this function so the new peaks materialize as polylines.
pub fn seed_river_gap_mountains(
    map: &mut GlobalMap,
    river_map: &RiverMap,
) -> Vec<ElevationHotspot> {
    let max_gap_m = map.config.river_gap_max_m;
    if max_gap_m <= 0.0 {
        return Vec::new();
    }
    let res = map.config.global_res as usize;
    let total = res * res;
    let mpc = map.config.meters_per_cell();
    let max_gap_cells = (max_gap_m / mpc).round() as u16;
    let peak_m = map.config.max_elevation_m * RIVER_GAP_PEAK_FRAC;
    let lowland_thresh = map.config.max_elevation_m * RIVER_GAP_LOWLAND_FRAC;
    // Match the wall margin `extract_rivers` uses to exclude peaks; a hotspot
    // placed inside it wouldn't seed anything.
    let wall_margin = map
        .config
        .scaled_cells_usize(map.config.y_border_wall_cells)
        * 2;
    // Initial river-source mask: only cells on an extracted polyline. Using
    // raw flow accumulation would catch every micro-drainage in a lowland
    // (rain channeled through a single cell exceeds the default 100-flow
    // threshold easily), making the entire continent read as "river covered"
    // from the gap-fill's perspective. Polyline-based matches what's visible
    // in the rivers PNG — the user-facing definition of "where a river is".
    let mut river_mask = vec![0u8; total];
    for poly in &river_map.rivers {
        for &(x, y) in &poly.points {
            let i = (y as usize) * res + x as usize;
            if map.land_mask[i] == 1 {
                river_mask[i] = 1;
            }
        }
    }
    let mut dist = bfs_distance_from(&river_mask, res, 1, Some(&map.land_mask));

    // Habitable for gap-fill: lowland land cell outside the Y-border wall
    // exclusion. Slope/coast filters from settlements aren't relevant — we
    // only care whether the neighborhood lacks a river.
    let mut habitable = vec![0u8; total];
    for i in 0..total {
        if map.land_mask[i] != 1 {
            continue;
        }
        if map.elevation_m[i] >= lowland_thresh {
            continue;
        }
        let iy = i / res;
        if iy < wall_margin || iy + wall_margin >= res {
            continue;
        }
        habitable[i] = 1;
    }

    let mut added: Vec<ElevationHotspot> = Vec::new();
    let origin = map.config.world_size_m as f32 * 0.5;
    loop {
        let mut farthest_idx: Option<usize> = None;
        let mut farthest_d: u16 = 0;
        for i in 0..total {
            if habitable[i] == 0 {
                continue;
            }
            let d = dist[i];
            if d <= max_gap_cells || d == u16::MAX {
                continue;
            }
            if d > farthest_d {
                farthest_d = d;
                farthest_idx = Some(i);
            }
        }
        let Some(idx) = farthest_idx else {
            break;
        };
        let cell_x = (idx % res) as u32;
        let cell_y = (idx / res) as u32;
        let world_x = (cell_x as f32 + 0.5) * mpc - origin;
        let world_y = (cell_y as f32 + 0.5) * mpc - origin;
        added.push(ElevationHotspot {
            center_x_m: world_x,
            center_y_m: world_y,
            radius_m: RIVER_GAP_RADIUS_M,
            peak_m,
            cap_elev_m: None,
        });

        // Treat the new center as a future river source. The actual stream
        // produced by the hotspot will run *near* this cell, so using the
        // center as a BFS seed is a coverage estimate — accurate enough that
        // subsequent gap picks don't pile up around the same valley.
        bfs_distance_extend_from_cell(&mut dist, res, idx, Some(&map.land_mask));
    }

    apply_hotspots_to(&map.config, &map.land_mask, &mut map.elevation_m, &added);
    added
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
        let dist = bfs_distance_from(&map.land_mask, res, 0, None);
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

    #[test]
    fn river_gap_pass_disabled_is_noop() {
        let mut cfg = test_config(128);
        cfg.river_gap_max_m = 0.0;
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        let mut river_map = super::super::rivers::compute_flow(&map);
        super::super::rivers::extract_rivers(&map, &mut river_map, 50.0, 4);
        let pre = map.elevation_m.clone();
        let added = seed_river_gap_mountains(&mut map, &river_map);
        assert!(added.is_empty());
        assert_eq!(
            map.elevation_m, pre,
            "elevation must not change when disabled"
        );
    }

    #[test]
    fn river_gap_pass_seeds_low_mountains_in_riverless_lowland() {
        // Lowland continent with a single synthetic polyline at one edge —
        // the rest of the map should register as "far from rivers" and the
        // gap-fill must place at least one hotspot reaching the seed threshold.
        let mut cfg = test_config(256);
        cfg.sea_ratio = 0.2;
        cfg.target_continent_count = 1;
        cfg.continent_gap_cells = 0;
        cfg.mountain_ratio = 0.0;
        cfg.mountain_amplitude_m = 0.0;
        cfg.y_border_wall_cells = 0;
        cfg.y_border_wall_height_m = 0.0;
        // mpc=16 at res=256, world=4096. 1024 m gap → 64 cells.
        cfg.river_gap_max_m = 1024.0;
        let mut map = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut map);
        // Build a minimal RiverMap: flow vec the right size, one polyline in
        // a single cell at the world's NW corner (still inside the continent
        // for typical seeds). Most of the map ends up far from this point.
        let res = cfg.global_res as usize;
        let total = res * res;
        let mut river_map = super::super::rivers::RiverMap {
            downstream: vec![None; total],
            flow: vec![0.0; total],
            rivers: Vec::new(),
        };
        // Pick the first land cell as the "river"; the test only needs one.
        let one_river = (0..total)
            .find(|&i| map.land_mask[i] == 1)
            .expect("test map should have at least one land cell");
        river_map.rivers.push(super::super::rivers::Polyline {
            points: vec![((one_river % res) as u32, (one_river / res) as u32)],
            flow: vec![1.0],
        });

        let added = seed_river_gap_mountains(&mut map, &river_map);
        assert!(!added.is_empty(), "expected at least one gap-fill hotspot");
        let post_max = map.elevation_m.iter().copied().fold(0.0f32, f32::max);
        // `mountain_amplitude_m=0` guarantees pre-pass elevation stays well
        // below this threshold, so a single check on the post-pass max is
        // sufficient to prove the gap-fill ran and seeded a mountain.
        let seed_thresh = cfg.max_elevation_m * super::super::rivers::RIVER_PEAK_ELEVATION_FRAC;
        assert!(
            post_max >= seed_thresh,
            "gap-fill should raise some cell above {seed_thresh:.0} m (got {post_max:.0})"
        );
    }
}
