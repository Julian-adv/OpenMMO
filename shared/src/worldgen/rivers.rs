//! Phase 4: flow accumulation and river extraction.
//!
//! On a heightmap, water flows from high elevation toward low. Each land
//! cell has a single "downstream" neighbor (the 8-connected neighbor with
//! the steepest descent). Flow accumulation counts, for every cell, how
//! many upstream cells drain into it — i.e. how much rain reaches that
//! point along the gradient field.
//!
//! Cells whose accumulation exceeds a threshold are rivers. Tracing each
//! such cell's downstream chain to the sea yields river polylines, which
//! subsequent phases (splatmap, vegetation) use to paint riverbeds and
//! tint vegetation density.

use std::collections::BinaryHeap;

use super::global_map::GlobalMap;
use super::grid::MinF32;

/// Peak elevation threshold (as a fraction of `max_elevation_m`) for
/// `extract_rivers` to treat a local maximum as a river source. Shared
/// between the bake and preview pipelines so the same map produces the
/// same polylines in both, and used by Phase 4b's gap-fill to size its
/// seeded mountains comfortably above this bar.
pub const RIVER_PEAK_ELEVATION_FRAC: f32 = 0.3;

/// Nearby rivers flowing in nearly the same direction are visually read as
/// duplicate channels. Let the first traced main stem claim a small corridor;
/// later traces that enter it snap to the existing stem and stop there.
const PARALLEL_MERGE_RADIUS_FRAC: f32 = 0.003;
const PARALLEL_MERGE_RADIUS_MIN: i32 = 2;
const PARALLEL_MERGE_RADIUS_MAX: i32 = 12;
const PARALLEL_MERGE_MIN_DOT: f32 = 0.7;

#[derive(Clone, Copy)]
struct MergeClaim {
    target: u32,
    dir_x: i8,
    dir_y: i8,
    dist_sq: u16,
}

/// Priority-queue-based pit fill (Barnes et al. 2014). Starting from the
/// sea / Y-border cells, flood inward; each cell is raised just above the
/// highest point on the least-costly path back to an outlet, guaranteeing
/// that every land cell has a downhill path to the boundary.
fn fill_pits(elev: &[f32], mask: &[u8], res: usize) -> Vec<f32> {
    let total = res * res;
    let mut filled = elev.to_vec();
    let mut visited = vec![false; total];
    let mut pq: BinaryHeap<MinF32> = BinaryHeap::new();

    for i in 0..total {
        let y = i / res;
        let is_border = y == 0 || y == res - 1;
        if mask[i] == 0 || is_border {
            pq.push(MinF32(filled[i], i as u32));
            visited[i] = true;
        }
    }

    while let Some(MinF32(hi, iu)) = pq.pop() {
        let i = iu as usize;
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx).rem_euclid(res as i32) as usize;
                let ny = y + dy;
                if ny < 0 || ny >= res as i32 {
                    continue;
                }
                let ni = ny as usize * res + nx;
                if visited[ni] {
                    continue;
                }
                visited[ni] = true;
                // Raise neighbor's filled height to hi + tiny increment so
                // it drains back toward the outlet we just came from.
                let raised = elev[ni].max(hi + 1e-3);
                filled[ni] = raised;
                pq.push(MinF32(raised, ni as u32));
            }
        }
    }
    filled
}

fn parallel_merge_radius_cells(res: usize) -> i32 {
    ((res as f32 * PARALLEL_MERGE_RADIUS_FRAC).round() as i32)
        .clamp(PARALLEL_MERGE_RADIUS_MIN, PARALLEL_MERGE_RADIUS_MAX)
}

fn step_dir(from: (u32, u32), to: (u32, u32), res: usize) -> Option<(i8, i8)> {
    let mut dx = to.0 as i32 - from.0 as i32;
    let half = res as i32 / 2;
    if dx > half {
        dx -= res as i32;
    } else if dx < -half {
        dx += res as i32;
    }
    let dy = to.1 as i32 - from.1 as i32;
    if dx == 0 && dy == 0 {
        None
    } else {
        Some((dx.signum() as i8, dy.signum() as i8))
    }
}

fn downstream_dir(cell: usize, downstream: &[Option<u32>], res: usize) -> Option<(i8, i8)> {
    let target = downstream[cell]? as usize;
    let from = ((cell % res) as u32, (cell / res) as u32);
    let to = ((target % res) as u32, (target / res) as u32);
    step_dir(from, to, res)
}

fn polyline_dir(points: &[(u32, u32)], pi: usize, res: usize) -> Option<(i8, i8)> {
    if pi + 1 < points.len() {
        step_dir(points[pi], points[pi + 1], res)
    } else if pi > 0 {
        step_dir(points[pi - 1], points[pi], res)
    } else {
        None
    }
}

fn directions_parallel_downstream(a: (i8, i8), b: (i8, i8)) -> bool {
    let ax = a.0 as f32;
    let ay = a.1 as f32;
    let bx = b.0 as f32;
    let by = b.1 as f32;
    let dot = ax * bx + ay * by;
    if dot <= 0.0 {
        return false;
    }
    let len = (ax * ax + ay * ay).sqrt() * (bx * bx + by * by).sqrt();
    len > 0.0 && dot / len >= PARALLEL_MERGE_MIN_DOT
}

fn parallel_merge_target(
    claims: &[Option<MergeClaim>],
    cell: usize,
    dir: Option<(i8, i8)>,
) -> Option<u32> {
    let claim = claims[cell]?;
    let dir = dir?;
    if directions_parallel_downstream(dir, (claim.dir_x, claim.dir_y)) {
        Some(claim.target)
    } else {
        None
    }
}

fn add_parallel_merge_claims(
    claims: &mut [Option<MergeClaim>],
    points: &[(u32, u32)],
    downstream: &[Option<u32>],
    res: usize,
    radius: i32,
) {
    let radius_sq = radius * radius;
    for (pi, &(x, y)) in points.iter().enumerate() {
        let cell = y as usize * res + x as usize;
        let Some((dir_x, dir_y)) =
            downstream_dir(cell, downstream, res).or_else(|| polyline_dir(points, pi, res))
        else {
            continue;
        };

        for dy in -radius..=radius {
            let ny = y as i32 + dy;
            if ny < 0 || ny >= res as i32 {
                continue;
            }
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > radius_sq {
                    continue;
                }
                let nx = (x as i32 + dx).rem_euclid(res as i32) as usize;
                let ni = ny as usize * res + nx;
                let claim = MergeClaim {
                    target: cell as u32,
                    dir_x,
                    dir_y,
                    dist_sq: dist_sq as u16,
                };
                match claims[ni] {
                    Some(existing) if existing.dist_sq <= claim.dist_sq => {}
                    _ => claims[ni] = Some(claim),
                }
            }
        }
    }
}

pub struct RiverMap {
    /// For each cell, the index of its downstream neighbor, or `None` if it
    /// has no descent (sink / sea cell). Length = res².
    pub downstream: Vec<Option<u32>>,

    /// Flow accumulation in arbitrary cell-rain units. Higher = more water
    /// passes through. Length = res².
    pub flow: Vec<f32>,

    /// Extracted river polylines — each is a sequence of cell coordinates
    /// from the source (highest upstream point) to the mouth (sea or sink).
    pub rivers: Vec<Polyline>,
}

impl RiverMap {
    /// Maximum per-vertex flow across every extracted polyline, clamped to
    /// ≥ 1 so log-normalization downstream never divides by zero. Recomputed
    /// on demand (rivers vector mutates rarely, callers are offline bake /
    /// preview).
    pub fn max_flow(&self) -> f32 {
        let mut m = 1.0f32;
        for poly in &self.rivers {
            for &f in &poly.flow {
                if f > m {
                    m = f;
                }
            }
        }
        m
    }
}

#[derive(Debug, Clone)]
pub struct Polyline {
    pub points: Vec<(u32, u32)>,
    /// Per-vertex flow accumulation (raw units, same scale as `RiverMap.flow`).
    /// Same length as `points`. Drives downstream width growth.
    pub flow: Vec<f32>,
}

/// Compute downstream pointers + flow accumulation for every land cell.
/// Fills pits first so every land cell has a downhill path to the ocean.
pub fn compute_flow(map: &GlobalMap) -> RiverMap {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mask = &map.land_mask;
    // Pit-filled elevation for flow computation. Original elevation is
    // preserved in `map.elevation_m`; flow pretends pits are already full.
    let filled = fill_pits(&map.elevation_m, mask, res);
    let elev = &filled;

    // 8-connected offsets with their Euclidean distance (for slope calc).
    const OFFSETS: [(i32, i32, f32); 8] = [
        (-1, -1, std::f32::consts::SQRT_2),
        (0, -1, 1.0),
        (1, -1, std::f32::consts::SQRT_2),
        (-1, 0, 1.0),
        (1, 0, 1.0),
        (-1, 1, std::f32::consts::SQRT_2),
        (0, 1, 1.0),
        (1, 1, std::f32::consts::SQRT_2),
    ];

    // --- Downstream pointer per land cell, on the pit-filled surface.
    let mut downstream: Vec<Option<u32>> = vec![None; total];
    for i in 0..total {
        if mask[i] == 0 {
            continue;
        }
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        let h = elev[i];
        let mut best_slope = 0.0f32;
        let mut best: Option<u32> = None;
        for &(dx, dy, dist) in &OFFSETS {
            let nx = (x + dx).rem_euclid(res as i32) as usize;
            let ny = y + dy;
            if ny < 0 || ny >= res as i32 {
                continue;
            }
            let ni = ny as usize * res + nx;
            let dh = h - elev[ni];
            if dh > 0.0 {
                let slope = dh / dist;
                if slope > best_slope {
                    best_slope = slope;
                    best = Some(ni as u32);
                }
            }
        }
        downstream[i] = best;
    }

    // --- Flow accumulation.
    // Sort cells by elevation (descending). Each land cell contributes 1
    // unit of rain to itself, and then passes its total downstream. By
    // processing high-to-low we guarantee each cell is finalized before
    // its downstream is visited.
    let mut order: Vec<u32> = (0..total as u32).collect();
    order.sort_by(|&a, &b| elev[b as usize].total_cmp(&elev[a as usize]));

    let mut flow = vec![0.0f32; total];
    for &iu in &order {
        let i = iu as usize;
        if mask[i] == 0 {
            continue;
        }
        flow[i] += 1.0; // rain on this cell
        if let Some(d) = downstream[i] {
            flow[d as usize] += flow[i];
        }
    }

    RiverMap {
        downstream,
        flow,
        rivers: Vec::new(),
    }
}

/// Extract river polylines. Sources are **local elevation peaks** above
/// `min_peak_elevation` — i.e. cells whose elevation is strictly greater
/// than all 8 neighbors. Peaks are processed from highest to lowest; each
/// traces downstream until it either reaches sea, hits a sink, or merges
/// into a previously-traced river (giving natural tree structure).
///
/// Also carves water upstream from every "sharp sea inlet" — sea cells that
/// jut far into land become river mouths by extending the existing flow
/// path to reach them. (This catches the visual association users make
/// between pointy coastal inlets and river mouths.)
///
/// `min_length` drops very short chains so the preview stays readable.
pub fn extract_rivers(
    map: &GlobalMap,
    rivers: &mut RiverMap,
    min_peak_elevation: f32,
    min_length: usize,
) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mask = &map.land_mask;
    let elev = &map.elevation_m;

    rivers.rivers.clear();

    // --- 1. Gather candidate sources: local elevation maxima above
    // threshold. Candidates are filtered by a minimum-spacing pass so the
    // resulting river network has a handful of main stems rather than one
    // "river" per every rocky bump.
    // Exclude ~2× the wall band from peak candidacy. The wall's uniform
    // southward slope generates parallel peaks that trace as straight-line
    // rivers; the 2× cushion catches peaks just past the wall where the
    // wall-to-natural-terrain transition still produces a uniform gradient.
    let wall_margin = map
        .config
        .scaled_cells_usize(map.config.y_border_wall_cells)
        * 2;
    let mut candidates: Vec<(u32, f32)> = Vec::new();
    for i in 0..total {
        if mask[i] == 0 || elev[i] < min_peak_elevation {
            continue;
        }
        let iy = i / res;
        if iy < wall_margin || iy + wall_margin >= res {
            continue;
        }
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        let h = elev[i];
        let mut is_peak = true;
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx).rem_euclid(res as i32) as usize;
                let ny = y + dy;
                if ny < 0 || ny >= res as i32 {
                    continue;
                }
                let ni = ny as usize * res + nx;
                if elev[ni] >= h {
                    is_peak = false;
                    break;
                }
            }
            if !is_peak {
                break;
            }
        }
        if is_peak {
            candidates.push((i as u32, h));
        }
    }

    // Sort by elevation descending — tallest peaks claim main stems first,
    // shorter peaks become tributaries at junctions.
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Spatial filter: keep a peak only if it's at least `min_peak_spacing`
    // cells from every already-kept peak (X-wrap aware). Prevents dozens of
    // near-identical tributaries originating on the same massif.
    let min_peak_spacing_sq = ((res as f32 * 0.015).max(20.0).powi(2)) as i64;
    let res_i = res as i64;
    let mut peaks: Vec<(u32, f32)> = Vec::with_capacity(candidates.len().min(100));
    for (idx, h) in candidates {
        let px = (idx as usize % res) as i64;
        let py = (idx as usize / res) as i64;
        let ok = peaks.iter().all(|&(qidx, _)| {
            let qx = (qidx as usize % res) as i64;
            let qy = (qidx as usize / res) as i64;
            let dx = (px - qx).abs();
            let dx = dx.min(res_i - dx); // X-wrap
            let dy = py - qy;
            dx * dx + dy * dy >= min_peak_spacing_sq
        });
        if ok {
            peaks.push((idx, h));
        }
    }

    // --- 2. Trace each peak downstream until sea / sink / merge.
    let mut visited = vec![false; total];
    let merge_radius = parallel_merge_radius_cells(res);
    let mut merge_claims: Vec<Option<MergeClaim>> = vec![None; total];
    for (peak_idx, _) in peaks {
        let start = peak_idx as usize;
        if visited[start] {
            continue;
        }
        let mut points: Vec<(u32, u32)> = Vec::new();
        let mut flow_vals: Vec<f32> = Vec::new();
        let mut cur: Option<u32> = Some(start as u32);
        while let Some(ci32) = cur {
            let ci = ci32 as usize;
            let x = (ci % res) as u32;
            let y = (ci / res) as u32;
            points.push((x, y));
            flow_vals.push(rivers.flow[ci]);
            if visited[ci] {
                // Merge into an earlier-traced polyline — include this
                // junction point so the tributary visibly connects.
                break;
            }
            if let Some(target) = parallel_merge_target(
                &merge_claims,
                ci,
                downstream_dir(ci, &rivers.downstream, res),
            ) {
                visited[ci] = true;
                let ti = target as usize;
                let target_point = ((ti % res) as u32, (ti / res) as u32);
                if points.last().copied() != Some(target_point) {
                    points.push(target_point);
                    flow_vals.push(rivers.flow[ti]);
                }
                break;
            }
            visited[ci] = true;
            cur = rivers.downstream[ci];
        }
        if points.len() >= min_length {
            add_parallel_merge_claims(
                &mut merge_claims,
                &points,
                &rivers.downstream,
                res,
                merge_radius,
            );
            rivers.rivers.push(Polyline {
                points,
                flow: flow_vals,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;
    use crate::worldgen::global_map::GlobalMap;

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
            mountain_inland_buffer_m: 0.0,
            y_border_wall_cells: 0,
            y_border_wall_height_m: 0.0,
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

    fn cell_idx(res: u32, x: u32, y: u32) -> usize {
        y as usize * res as usize + x as usize
    }

    fn blank_map(res: u32) -> GlobalMap {
        let cfg = test_config(res);
        let total = res as usize * res as usize;
        GlobalMap {
            config: cfg,
            continent_potential: vec![1.0; total],
            land_mask: vec![1; total],
            sea_level_potential: 0.0,
            elevation_m: vec![0.0; total],
        }
    }

    fn blank_river_map(res: u32) -> RiverMap {
        let total = res as usize * res as usize;
        RiverMap {
            downstream: vec![None; total],
            flow: vec![1.0; total],
            rivers: Vec::new(),
        }
    }

    fn seed_manual_river(
        map: &mut GlobalMap,
        rivers: &mut RiverMap,
        points: &[(u32, u32)],
        source_elev: f32,
    ) {
        let res = map.config.global_res;
        for (pi, &(x, y)) in points.iter().enumerate() {
            let idx = cell_idx(res, x, y);
            map.elevation_m[idx] = source_elev - pi as f32;
            rivers.flow[idx] = (pi + 1) as f32;
            if let Some(&(nx, ny)) = points.get(pi + 1) {
                rivers.downstream[idx] = Some(cell_idx(res, nx, ny) as u32);
            }
        }
    }

    #[test]
    fn flow_accumulates_downhill() {
        // On a land cell with a strictly downhill neighbor, flow at the
        // source should equal 1 (its own rain) and flow at the downstream
        // cell should be strictly greater.
        let cfg = test_config(64);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let rm = compute_flow(&map);
        let total = rm.flow.len();
        for i in 0..total {
            if map.land_mask[i] == 0 {
                continue;
            }
            let Some(d) = rm.downstream[i] else {
                continue;
            };
            let di = d as usize;
            assert!(
                rm.flow[di] >= rm.flow[i],
                "downstream flow {} not ≥ upstream {}",
                rm.flow[di],
                rm.flow[i]
            );
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(64);
        let mut a = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut b);
        let ra = compute_flow(&a);
        let rb = compute_flow(&b);
        assert_eq!(ra.flow, rb.flow);
        assert_eq!(ra.downstream, rb.downstream);
    }

    #[test]
    fn rivers_extracted() {
        let cfg = test_config(128);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = compute_flow(&map);
        // min_peak_elevation low so small test maps still produce sources.
        extract_rivers(&map, &mut rm, 50.0, 4);
        assert!(!rm.rivers.is_empty(), "no rivers extracted");
        for r in &rm.rivers {
            assert!(r.points.len() >= 4);
        }
    }

    #[test]
    fn near_parallel_rivers_merge_into_existing_stem() {
        let res = 128;
        let mut map = blank_map(res);
        let mut rm = blank_river_map(res);

        let mut main: Vec<(u32, u32)> = (10u32..=44).map(|y| (30, y)).collect();
        main.extend((1u32..=14).map(|k| (30 + k, 44 + k)));
        main.extend((59u32..=100).map(|y| (44, y)));

        let mut parallel: Vec<(u32, u32)> = (10u32..=44).map(|y| (70, y)).collect();
        parallel.extend((1u32..=24).map(|k| (70 - k, 44 + k)));
        parallel.extend((69u32..=100).map(|y| (46, y)));

        seed_manual_river(&mut map, &mut rm, &main, 240.0);
        seed_manual_river(&mut map, &mut rm, &parallel, 220.0);

        extract_rivers(&map, &mut rm, 50.0, 4);

        let main_poly = rm
            .rivers
            .iter()
            .find(|poly| poly.points.first() == Some(&(30, 10)))
            .expect("main river was not extracted");
        assert_eq!(main_poly.points.last(), Some(&(44, 100)));

        let merged_poly = rm
            .rivers
            .iter()
            .find(|poly| poly.points.first() == Some(&(70, 10)))
            .expect("parallel river was not extracted");
        assert_eq!(merged_poly.points.last(), Some(&(44, 68)));
        assert!(
            !merged_poly.points.contains(&(46, 90)),
            "parallel river should stop once it joins the main stem"
        );
    }
}
