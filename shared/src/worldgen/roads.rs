//! Phase 6: road network.
//!
//! Each settlement is connected into a minimum spanning tree in Euclidean
//! space (X-wrap aware), and each MST edge is resolved on the terrain grid
//! via A* with cost = base distance + slope penalty. Sea cells are
//! forbidden — the network has to stay on land, implying ferries/bridges
//! aren't modeled.
//!
//! The result is a set of road polylines. Later phases use these both for
//! splatmap painting and for seeding extra villages along the routes.

use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashSet};

use super::global_map::GlobalMap;
use super::grid::MinF32;
use super::settlements::Settlement;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Road {
    pub points: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoadNetwork {
    pub roads: Vec<Road>,
}

/// Each meter of elevation change along an A* step adds this many cells of
/// cost. Tuned so roads route around real hills but don't detour absurdly
/// far to avoid a modest incline.
const SLOPE_WEIGHT: f32 = 0.04;

pub fn compute_roads(map: &GlobalMap, settlements: &[Settlement]) -> RoadNetwork {
    if settlements.len() < 2 {
        return RoadNetwork::default();
    }
    let res_f = map.config.global_res as f32;
    let extra = map.config.road_extra_neighbors as usize;

    // Base connectivity from the MST, then augment with each city's K
    // nearest neighbors so some towns become multi-degree hubs. New edges
    // are rejected if they run too close to the direction of an existing
    // incident edge (avoids parallel road-pairs from the same junction).
    let mst_edges: Vec<(usize, usize)> = prim_mst(settlements, res_f);
    let mut edge_set: HashSet<(usize, usize)> = mst_edges.iter().copied().map(canonical).collect();
    if extra > 0 {
        let n = settlements.len();
        let mut neighbors: Vec<Vec<usize>> = vec![Vec::new(); n];
        for &(a, b) in &mst_edges {
            neighbors[a].push(b);
            neighbors[b].push(a);
        }
        // Reject candidate if angle to any existing incident edge is below
        // this cosine threshold. cos(20°) ≈ 0.94 — below 20° they read as
        // parallel on the rendered map.
        const MIN_ANGLE_COS: f32 = 0.94;
        for i in 0..n {
            let mut dists: Vec<(f32, usize)> = (0..n)
                .filter(|&j| j != i)
                .map(|j| (euclidean_sq(&settlements[i], &settlements[j], res_f), j))
                .collect();
            dists.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut added = 0;
            for &(_, j) in dists.iter() {
                if added >= extra {
                    break;
                }
                if edge_set.contains(&canonical((i, j))) {
                    continue;
                }
                let dir_j = direction(&settlements[i], &settlements[j], res_f);
                let too_parallel = neighbors[i].iter().any(|&k| {
                    let dir_k = direction(&settlements[i], &settlements[k], res_f);
                    cos_angle(dir_j, dir_k) > MIN_ANGLE_COS
                });
                if too_parallel {
                    continue;
                }
                edge_set.insert(canonical((i, j)));
                neighbors[i].push(j);
                neighbors[j].push(i);
                added += 1;
            }
        }
    }

    let mut edges: Vec<(usize, usize)> = edge_set.into_iter().collect();
    edges.sort_unstable();

    // Pre-allocate A* scratch buffers once and reset per call instead of
    // re-allocating 3× res² vectors for every edge. At 4096² this avoids
    // gigabytes of allocation traffic over the N-edge road loop.
    let total = (map.config.global_res as usize).pow(2);
    let mut scratch = AStarScratch::new(total);
    let mut roads = Vec::with_capacity(edges.len());
    for (a, b) in edges {
        let sa = &settlements[a];
        let sb = &settlements[b];
        scratch.reset();
        if let Some(path) = a_star(
            map,
            sa.cell_x as usize,
            sa.cell_y as usize,
            sb.cell_x as usize,
            sb.cell_y as usize,
            &mut scratch,
        ) {
            roads.push(Road { points: path });
        }
    }
    RoadNetwork { roads }
}

struct AStarScratch {
    g_score: Vec<f32>,
    came_from: Vec<u32>,
    closed: Vec<bool>,
    open: BinaryHeap<MinF32>,
}

impl AStarScratch {
    fn new(total: usize) -> Self {
        Self {
            g_score: vec![f32::INFINITY; total],
            came_from: vec![u32::MAX; total],
            closed: vec![false; total],
            open: BinaryHeap::new(),
        }
    }
    fn reset(&mut self) {
        self.g_score.fill(f32::INFINITY);
        self.came_from.fill(u32::MAX);
        self.closed.fill(false);
        self.open.clear();
    }
}

fn canonical(e: (usize, usize)) -> (usize, usize) {
    if e.0 < e.1 {
        e
    } else {
        (e.1, e.0)
    }
}

/// Classical Prim's MST on settlement positions, with X-wrap-aware squared
/// Euclidean distance. `O(n²)` — fine for hundreds of cities.
fn prim_mst(settlements: &[Settlement], res_f: f32) -> Vec<(usize, usize)> {
    let n = settlements.len();
    let mut in_tree = vec![false; n];
    let mut min_dist = vec![f32::INFINITY; n];
    let mut closest = vec![0usize; n];
    in_tree[0] = true;
    for j in 1..n {
        min_dist[j] = euclidean_sq(&settlements[0], &settlements[j], res_f);
    }
    let mut edges = Vec::with_capacity(n.saturating_sub(1));
    for _ in 1..n {
        let mut best = usize::MAX;
        let mut best_d = f32::INFINITY;
        for (j, &d) in min_dist.iter().enumerate() {
            if !in_tree[j] && d < best_d {
                best_d = d;
                best = j;
            }
        }
        if best == usize::MAX {
            break;
        }
        edges.push((closest[best], best));
        in_tree[best] = true;
        for j in 0..n {
            if !in_tree[j] {
                let d = euclidean_sq(&settlements[best], &settlements[j], res_f);
                if d < min_dist[j] {
                    min_dist[j] = d;
                    closest[j] = best;
                }
            }
        }
    }
    edges
}

fn euclidean_sq(a: &Settlement, b: &Settlement, res_f: f32) -> f32 {
    let dx_raw = (a.cell_x as f32 - b.cell_x as f32).abs();
    let dx = dx_raw.min(res_f - dx_raw);
    let dy = a.cell_y as f32 - b.cell_y as f32;
    dx * dx + dy * dy
}

/// Unit direction vector from `a` to `b`, with X-wrap handled by picking
/// the shorter horizontal side.
fn direction(a: &Settlement, b: &Settlement, res_f: f32) -> (f32, f32) {
    let dx_raw = b.cell_x as f32 - a.cell_x as f32;
    let dx = if dx_raw.abs() > res_f * 0.5 {
        if dx_raw > 0.0 {
            dx_raw - res_f
        } else {
            dx_raw + res_f
        }
    } else {
        dx_raw
    };
    let dy = b.cell_y as f32 - a.cell_y as f32;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    (dx / len, dy / len)
}

fn cos_angle(a: (f32, f32), b: (f32, f32)) -> f32 {
    a.0 * b.0 + a.1 * b.1
}

fn a_star(
    map: &GlobalMap,
    sx: usize,
    sy: usize,
    gx: usize,
    gy: usize,
    scratch: &mut AStarScratch,
) -> Option<Vec<(u32, u32)>> {
    let res = map.config.global_res as usize;
    let res_i = res as i32;
    let elev = &map.elevation_m;
    let mask = &map.land_mask;

    let start = sy * res + sx;
    let goal = gy * res + gx;
    if mask[start] == 0 || mask[goal] == 0 {
        return None;
    }

    scratch.g_score[start] = 0.0;
    scratch
        .open
        .push(MinF32(heuristic(sx, sy, gx, gy, res), start as u32));

    while let Some(MinF32(_, cur)) = scratch.open.pop() {
        let ci = cur as usize;
        if scratch.closed[ci] {
            continue;
        }
        scratch.closed[ci] = true;
        if ci == goal {
            return Some(reconstruct(&scratch.came_from, start, goal, res));
        }
        let cx = (ci % res) as i32;
        let cy = (ci / res) as i32;
        let h = elev[ci];

        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (cx + dx).rem_euclid(res_i) as usize;
                let ny = cy + dy;
                if ny < 0 || ny >= res_i {
                    continue;
                }
                let ni = ny as usize * res + nx;
                if mask[ni] == 0 || scratch.closed[ni] {
                    continue;
                }
                let base = if dx.abs() + dy.abs() == 2 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                let dh = (elev[ni] - h).abs();
                let cost = base + dh * SLOPE_WEIGHT;
                let tentative = scratch.g_score[ci] + cost;
                if tentative < scratch.g_score[ni] {
                    scratch.g_score[ni] = tentative;
                    scratch.came_from[ni] = cur;
                    let f = tentative + heuristic(nx, ny as usize, gx, gy, res);
                    scratch.open.push(MinF32(f, ni as u32));
                }
            }
        }
    }
    None
}

fn reconstruct(came_from: &[u32], start: usize, goal: usize, res: usize) -> Vec<(u32, u32)> {
    let mut path = Vec::new();
    let mut c = goal;
    loop {
        let y = (c / res) as u32;
        let x = (c % res) as u32;
        path.push((x, y));
        if c == start {
            break;
        }
        if came_from[c] == u32::MAX {
            break;
        }
        c = came_from[c] as usize;
    }
    path.reverse();
    path
}

fn heuristic(sx: usize, sy: usize, gx: usize, gy: usize, res: usize) -> f32 {
    let dx_raw = (sx as f32 - gx as f32).abs();
    let dx = dx_raw.min(res as f32 - dx_raw);
    let dy = sy as f32 - gy as f32;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation, rivers, settlements};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;

    fn test_config(res: u32) -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF,
            global_res: res,
            reference_res: res,
            sea_ratio: 0.35,
            settlement_target_count: 8,
            settlement_min_spacing_cells: (res / 10).max(4),
            settlement_river_flow_threshold: 20.0,
            ..WorldGenConfig::default()
        }
    }

    #[test]
    fn roads_have_reasonable_count() {
        let cfg = test_config(128);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut rm, 50.0, 4);
        let s = settlements::place_settlements(&map, &rm);
        let net = compute_roads(&map, &s);
        let n = s.len();
        let max_possible = n * (n - 1) / 2;
        assert!(
            net.roads.len() <= max_possible,
            "roads {} exceeds complete-graph bound {}",
            net.roads.len(),
            max_possible
        );
        for r in &net.roads {
            assert!(r.points.len() >= 2, "road too short");
        }
    }

    #[test]
    fn roads_stay_on_land() {
        let cfg = test_config(128);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut rm, 50.0, 4);
        let s = settlements::place_settlements(&map, &rm);
        let net = compute_roads(&map, &s);
        let res = cfg.global_res as usize;
        for r in &net.roads {
            for &(x, y) in &r.points {
                let i = (y as usize) * res + x as usize;
                assert_eq!(map.land_mask[i], 1, "road crosses sea at ({x}, {y})");
            }
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(128);
        let build = || {
            let mut map = continent::generate_continent_mask(&cfg);
            elevation::generate_elevation(&mut map);
            let mut rm = rivers::compute_flow(&map);
            rivers::extract_rivers(&map, &mut rm, 50.0, 4);
            let s = settlements::place_settlements(&map, &rm);
            compute_roads(&map, &s)
        };
        let a = build();
        let b = build();
        assert_eq!(a.roads.len(), b.roads.len());
        for (ra, rb) in a.roads.iter().zip(b.roads.iter()) {
            assert_eq!(ra.points, rb.points);
        }
    }
}
