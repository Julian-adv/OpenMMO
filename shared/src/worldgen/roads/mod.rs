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
//!
//! Submodule layout:
//! - `graph`: Prim MST, K-nearest extras, parallel-fork redirect.
//! - `astar`: per-edge A* with slope/river/buffer cost terms.
//! - `merge`: post-pass that fuses near-parallel road pairs into shared
//!   trunks (endpoint-anchored and interior).
//! - `snap`: post-pass that snaps road↔river crossings to grid axes so
//!   bridge meshes drop in cleanly.
//! - `axis`: shared snap-axis classification used by both A* and snap.

mod astar;
mod axis;
mod graph;
mod merge;
mod snap;

pub use merge::{merge_parallel_interiors, merge_parallel_runs};
pub use snap::snap_crossings_to_grid;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use super::global_map::GlobalMap;
use super::rivers::RiverMap;
use super::settlements::Settlement;

use astar::{AStarScratch, RiverField, a_star};
use graph::{canonical, euclidean_sq, pair_cos_at, prim_mst, redirect_parallel_forks};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Road {
    pub points: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoadNetwork {
    pub roads: Vec<Road>,
}

pub fn compute_roads(
    map: &GlobalMap,
    settlements: &[Settlement],
    river_map: &RiverMap,
) -> RoadNetwork {
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
                let too_parallel = neighbors[i]
                    .iter()
                    .any(|&k| pair_cos_at(i, j, k, settlements, res_f) > MIN_ANGLE_COS);
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

    // MST has no parallel-fork rejection of its own (it just minimizes total
    // length), so two cities downstream of the same hub can both sit in the
    // hub's adjacency at near-parallel angles. Real road builders would never
    // lay redundant pavement next to an existing trunk; redirect those forks
    // through the closer city so the network reads as a Y-junction.
    redirect_parallel_forks(&mut edge_set, settlements, res_f);

    // Longest edges first so trunks form before branches and later
    // branches snap onto the trunk via `road_mask`.
    let mut edges: Vec<(usize, usize)> = edge_set.into_iter().collect();
    edges.sort_by(|&(a1, b1), &(a2, b2)| {
        let d1 = euclidean_sq(&settlements[a1], &settlements[b1], res_f);
        let d2 = euclidean_sq(&settlements[a2], &settlements[b2], res_f);
        d2.total_cmp(&d1).then((a1, b1).cmp(&(a2, b2)))
    });

    // Pre-allocate A* scratch buffers once and reset per call instead of
    // re-allocating 3× res² vectors for every edge. At 4096² this avoids
    // gigabytes of allocation traffic over the N-edge road loop.
    let res_usize = map.config.global_res as usize;
    let total = res_usize.pow(2);
    let mut scratch = AStarScratch::new(total);
    let river_field = RiverField::from_polylines(&river_map.rivers, res_usize);
    let mut road_mask = vec![0u8; total];
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
            &river_field,
            &road_mask,
        ) {
            for &(x, y) in &path {
                road_mask[(y as usize) * res_usize + x as usize] = 1;
            }
            roads.push(Road { points: path });
        }
    }
    RoadNetwork { roads }
}

/// Fold a raw X delta into the shorter wrap direction (≤ res/2 in magnitude).
/// Shared by `merge` and `snap` for X-wrap-aware cell-distance math.
#[inline]
pub(super) fn fold_x_delta(mut d: i32, res_i: i32) -> i32 {
    if d > res_i / 2 {
        d -= res_i;
    } else if d < -res_i / 2 {
        d += res_i;
    }
    d
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
        let net = compute_roads(&map, &s, &rm);
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
        let net = compute_roads(&map, &s, &rm);
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
            compute_roads(&map, &s, &rm)
        };
        let a = build();
        let b = build();
        assert_eq!(a.roads.len(), b.roads.len());
        for (ra, rb) in a.roads.iter().zip(b.roads.iter()) {
            assert_eq!(ra.points, rb.points);
        }
    }
}
