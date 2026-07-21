//! Phase 2: pre-erosion elevation layering.
//!
//! Erosion is what carves drainage networks and ridge tendrils, so this
//! pass only has to lay down the raw clay: a single FBM heightmap on land
//! at `base_elevation_m ± initial_relief_amp · max_elevation_m`. Everything
//! else stays out of the way:
//!
//! * Sea cells are pinned at 0 m.
//! * Config-driven `elevation_hotspots` and `river_carve_paths` are
//!   applied here so that art-directed peaks/channels are present in the
//!   pre-erosion clay (and thus get eroded into shape rather than appearing
//!   as raw bumps in the final terrain).

use super::config::{ElevationHotspot, WorldGenConfig};
use super::global_map::GlobalMap;
use super::grid::{bfs_distance_extend_from_cell, bfs_distance_from, fold_delta_f32};
use super::noise::{fbm_wrap_xy, PerlinNoise, PerlinNoise4D};
use super::rivers::{RiverMap, RIVER_PEAK_ELEVATION_FRAC};
use super::vector_features::project_point_to_segment;

const LACUNARITY: f32 = 2.0;

/// Run Phase 2: fill `map.elevation_m` from a single FBM noise on land.
pub fn generate_elevation(map: &mut GlobalMap) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let world_width = res as f32;

    let seed = map.config.seed;
    let noise = PerlinNoise4D::new(seed ^ 0xE1EE_1EE1_EE1E_u64);
    let base_freq = map
        .config
        .scaled_freq(1.0 / map.config.initial_relief_wavelength_cells.max(1.0));
    let octaves = map.config.initial_relief_octaves.max(1);
    let gain = map.config.initial_relief_gain.clamp(0.0, 1.0);

    let max_h = map.config.max_elevation_m.max(1.0);
    let base_h = map.config.base_elevation_m.clamp(0.0, max_h);
    let relief = (map.config.initial_relief_amp.max(0.0)) * max_h;

    let mut elevation = vec![0.0f32; total];
    for y in 0..res {
        for x in 0..res {
            let i = y * res + x;
            if map.land_mask[i] == 0 {
                continue;
            }
            let n = fbm_wrap_xy(
                &noise,
                x as f32,
                y as f32,
                world_width,
                base_freq,
                octaves,
                LACUNARITY,
                gain,
            );
            let h = base_h + n * relief;
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
    let shape_noise = PerlinNoise::new(cfg.seed ^ 0xC0FFEE_C0FFEE);
    let detail_noise = PerlinNoise::new(cfg.seed ^ 0xDEADBEEF_DEADBEEF);

    for spot in hotspots {
        let (cx, cy) = cfg.world_m_to_cell(spot.center_x_m, spot.center_y_m);
        let r_cells = (spot.radius_m / mpc).max(1.0);
        let shape_freq = HOTSPOT_SHAPE_LOBES / r_cells;
        let detail_freq = HOTSPOT_DETAIL_LOBES / r_cells;
        let r_max = r_cells * (1.0 + HOTSPOT_RADIUS_PERTURB);
        let pad = r_max.ceil() as i32 + 1;
        let y_min_i = (cy as i32) - pad;
        let y_max_i = (cy as i32) + pad;
        let x_min_i = (cx as i32) - pad;
        let x_max_i = (cx as i32) + pad;

        // Unwrapped loop coords, wrapped array indices: the disk is evaluated
        // in one continuous pass, so its local noise stays seam-free without
        // needing periodicity.
        for yi in y_min_i..=y_max_i {
            let y = yi.rem_euclid(res as i32) as usize;
            for xi in x_min_i..=x_max_i {
                let x = xi.rem_euclid(res as i32) as usize;
                let i = y * res + x;
                if land_mask[i] == 0 {
                    continue;
                }
                let dx = xi as f32 - cx;
                let dy = yi as f32 - cy;
                let d_sq = dx * dx + dy * dy;
                if d_sq >= r_max * r_max {
                    continue;
                }
                let s = shape_noise.sample(xi as f32 * shape_freq, yi as f32 * shape_freq);
                let eff_r = r_cells * (1.0 + HOTSPOT_RADIUS_PERTURB * s);
                if d_sq >= eff_r * eff_r {
                    continue;
                }
                let t = 1.0 - d_sq.sqrt() / eff_r;
                let falloff = t * t;
                let n_detail =
                    detail_noise.sample(xi as f32 * detail_freq, yi as f32 * detail_freq);
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

    let coast_dist = bfs_distance_from(&map.land_mask, res, 0, None);
    let coast_buffer_cells = (RIVER_GAP_RADIUS_M / mpc).round() as u16;

    let mut habitable = vec![0u8; total];
    for i in 0..total {
        if map.land_mask[i] != 1 {
            continue;
        }
        if map.elevation_m[i] >= lowland_thresh {
            continue;
        }
        if coast_dist[i] < coast_buffer_cells {
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

        bfs_distance_extend_from_cell(&mut dist, res, idx, Some(&map.land_mask));
    }

    apply_hotspots_to(&map.config, &map.land_mask, &mut map.elevation_m, &added);
    added
}

/// Phase 3c: stamp a gentle hill on every small land component so
/// that scattered islands read as islands rather than featureless
/// flat discs.
///
/// The erosion sim drains all of a small island's accumulated water
/// (and the sediment it carries) into the surrounding sea every
/// iteration, so what enters Phase 4 is a sub-pivot pancake that the
/// `plain_band_compression` then squashes essentially to the
/// `min_land_height_m` floor. Adding a hotspot per small component
/// here gives each island a visible crown without changing how
/// continents are shaped.
///
/// Output of [`seed_small_island_hills`]: the per-island hotspots
/// stamped onto `map.elevation_m`, plus the flat list of cell
/// indices that belong to any small-island component. The cell list
/// lets downstream passes (e.g. river extraction) iterate just the
/// island cells without re-scanning the full `res²` grid.
pub struct SmallIslandHillsOut {
    pub hotspots: Vec<ElevationHotspot>,
    pub island_cells: Vec<u32>,
}

/// No-op when `small_island_hill_peak_m == 0` or
/// `small_island_hill_max_cells == 0`. See [`SmallIslandHillsOut`].
pub fn seed_small_island_hills(map: &mut GlobalMap) -> SmallIslandHillsOut {
    let peak_m = map.config.small_island_hill_peak_m;
    let max_ref_cells = map.config.small_island_hill_max_cells;
    if peak_m <= 0.0 || max_ref_cells == 0 {
        return SmallIslandHillsOut {
            hotspots: Vec::new(),
            island_cells: Vec::new(),
        };
    }
    let max_cells = map.config.scaled_area_cells(max_ref_cells) as usize;
    let radius_frac = map.config.small_island_hill_radius_frac.clamp(0.05, 1.0);
    let res = map.config.global_res as usize;
    let total = res * res;
    let mpc = map.config.meters_per_cell();
    let origin = map.config.world_size_m as f32 * 0.5;

    let coast_dist = bfs_distance_from(&map.land_mask, res, 0, None);

    let mut visited = vec![false; total];
    let mut component: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut added: Vec<ElevationHotspot> = Vec::new();
    let mut island_cells: Vec<u32> = Vec::new();
    let res_i = res as i32;
    let res_f = res as f32;

    for start in 0..total {
        if map.land_mask[start] == 0 || visited[start] {
            continue;
        }
        component.clear();
        stack.clear();
        stack.push(start);
        visited[start] = true;
        let mut over_cap = false;

        while let Some(i) = stack.pop() {
            if !over_cap {
                component.push(i);
                if component.len() > max_cells {
                    over_cap = true;
                    component.clear();
                }
            }
            let x = i % res;
            let y = i / res;
            let left = if x == 0 { res - 1 } else { x - 1 };
            let right = if x + 1 == res { 0 } else { x + 1 };
            let mut try_push = |n: usize| {
                if map.land_mask[n] == 1 && !visited[n] {
                    visited[n] = true;
                    stack.push(n);
                }
            };
            try_push(y * res + left);
            try_push(y * res + right);
            if y > 0 {
                try_push((y - 1) * res + x);
            }
            if y + 1 < res {
                try_push((y + 1) * res + x);
            }
        }
        if over_cap {
            continue;
        }

        // X-wrap-aware centroid: anchor on the first cell and add
        // each subsequent cell's shortest-periodic offset, so an
        // island straddling x=0 still ends up with a sensible mean.
        let anchor = component[0];
        let anchor_x = (anchor % res) as f32;
        let anchor_y = (anchor / res) as f32;
        let mut sum_dx = 0.0f32;
        let mut sum_dy = 0.0f32;
        let mut max_d: u16 = 0;
        for &i in &component {
            island_cells.push(i as u32);
            sum_dx += fold_delta_f32((i % res) as f32 - anchor_x, res_f);
            sum_dy += (i / res) as f32 - anchor_y;
            let d = coast_dist[i];
            if d != u16::MAX && d > max_d {
                max_d = d;
            }
        }
        if max_d == 0 {
            continue;
        }
        let n = component.len() as f32;
        let mean_x = anchor_x + sum_dx / n;
        let cx = ((mean_x.round() as i32).rem_euclid(res_i)) as f32;
        let cy = (anchor_y + sum_dy / n).round().clamp(0.0, (res - 1) as f32);
        let world_x = (cx + 0.5) * mpc - origin;
        let world_y = (cy + 0.5) * mpc - origin;
        let radius_m = max_d as f32 * mpc * radius_frac;
        added.push(ElevationHotspot {
            center_x_m: world_x,
            center_y_m: world_y,
            radius_m,
            peak_m,
            cap_elev_m: None,
        });
    }

    apply_hotspots_to(&map.config, &map.land_mask, &mut map.elevation_m, &added);
    SmallIslandHillsOut {
        hotspots: added,
        island_cells,
    }
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
            continent_frequency: 1.0 / 64.0,
            min_island_cells: 0,
            min_strait_width_cells: 0,
            continent_seed_count: 5,
            continent_seed_min_distance_cells: 20,
            target_continent_count: 3,
            continent_gap_cells: 10,
            small_island_count: 0,
            river_gap_max_m: 0.0,
            initial_relief_wavelength_cells: (res as f32 / 4.0).max(8.0),
            ..Default::default()
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
    fn deterministic_for_same_seed() {
        let cfg = test_config(64);
        let mut a = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        generate_elevation(&mut b);
        assert_eq!(a.elevation_m, b.elevation_m);
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
}
