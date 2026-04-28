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
use super::rivers::{Polyline, RiverMap};
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

/// Flat penalty (in cells of A* cost) for stepping into a river cell. Keeps
/// roads slightly biased toward the dry-land path even when a perpendicular
/// crossing is the only thing left, but small enough that A* won't reroute
/// hundreds of meters around a single 1-cell stream when a clean ford is
/// available. Pairs with `RIVER_PARALLEL_PENALTY` to push the chosen
/// crossing toward right-angles to the flow.
const RIVER_CROSS_PENALTY: f32 = 2.0;

/// Anisotropic penalty (in cells of A* cost) scaled by the squared cosine
/// of the angle between the step direction and the local river tangent.
/// Perpendicular crossings (cos² ≈ 0) pay almost nothing on top of
/// `RIVER_CROSS_PENALTY`; parallel-along-river steps (cos² ≈ 1) pay the
/// full value, making it cheaper for A* to detour around the river than to
/// follow it. Squared (rather than linear) so the "near-perpendicular"
/// region is a wide cheap basin while only sharply angled crossings get
/// punished — keeps the network from over-bending for trivial misalignment.
const RIVER_PARALLEL_PENALTY: f32 = 50.0;

/// Per-step penalty (cells of A* cost) for entering a non-river cell that
/// sits in the river's Chebyshev-distance-1 ring (any of the 8 neighbours
/// of a river cell). Slightly larger than the cardinal-step base of 1.0
/// so A* is willing to detour by one cell to escape the buffer rather
/// than hug the bank — the requested ~2–3 m breathing room between the
/// road's outer edge and the river's sand band, expressed at cell
/// granularity. Real perpendicular crossings still happen: a single ford
/// transit pays at most twice this penalty, well under the
/// detour-around-the-river alternative.
const RIVER_BUFFER_PENALTY: f32 = 1.5;

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
    let river_field = RiverField::from_polylines(&river_map.rivers, map.config.global_res as usize);
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
        ) {
            roads.push(Road { points: path });
        }
    }
    RoadNetwork { roads }
}

/// Per-cell river overlay used by A*. `mask[i] != 0` marks cells the road
/// should treat as on-river; `tangent[i]` is the unit downstream direction
/// at that cell, used to score how parallel each candidate step is to the
/// flow; `near_river[i] != 0` flags cells inside the Chebyshev-1 ring of
/// any river cell (i.e. any of the eight neighbours), driving the
/// breathing-room buffer penalty. Built once per `compute_roads` call from
/// the already-extracted river polylines.
struct RiverField {
    mask: Vec<u8>,
    tangent: Vec<(f32, f32)>,
    near_river: Vec<u8>,
}

impl RiverField {
    fn from_polylines(rivers: &[Polyline], res: usize) -> Self {
        let total = res * res;
        let mut mask = vec![0u8; total];
        let mut tangent = vec![(0.0f32, 0.0f32); total];
        let res_f = res as f32;
        for poly in rivers {
            let pts = &poly.points;
            if pts.len() < 2 {
                continue;
            }
            for i in 0..pts.len() {
                let (x, y) = pts[i];
                let idx = (y as usize) * res + (x as usize);
                mask[idx] = 1;
                // Central difference where available, one-sided at the
                // ends. X-wrap: when consecutive samples land on opposite
                // sides of the seam (≥ res/2 apart) the raw delta has the
                // wrong sign — fold it to the shorter side.
                let prev = if i == 0 { pts[i] } else { pts[i - 1] };
                let next = if i + 1 >= pts.len() {
                    pts[i]
                } else {
                    pts[i + 1]
                };
                let mut dx = next.0 as f32 - prev.0 as f32;
                let dy = next.1 as f32 - prev.1 as f32;
                if dx > res_f * 0.5 {
                    dx -= res_f;
                } else if dx < -res_f * 0.5 {
                    dx += res_f;
                }
                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                tangent[idx] = (dx / len, dy / len);
            }
        }
        let near_river = chebyshev_dilate(&mask, res);
        Self {
            mask,
            tangent,
            near_river,
        }
    }

    /// Extra A* cost (in cells) for stepping into cell index `ni` along
    /// unit step `(sdx, sdy)`. On-river cells use the squared-cosine
    /// crossing/parallel penalty so perpendicular fords stay cheap while
    /// parallel-along steps pay close to the full
    /// `RIVER_PARALLEL_PENALTY`. Cells in the Chebyshev-1 buffer ring pay
    /// `RIVER_BUFFER_PENALTY` so roads keep ~1 cell of breathing room
    /// from the bank when running parallel.
    #[inline]
    fn step_penalty(&self, ni: usize, sdx: f32, sdy: f32) -> f32 {
        if self.mask[ni] != 0 {
            let (tx, ty) = self.tangent[ni];
            let par = sdx * tx + sdy * ty;
            let par_sq = par * par;
            return RIVER_CROSS_PENALTY + RIVER_PARALLEL_PENALTY * par_sq;
        }
        if self.near_river[ni] != 0 {
            return RIVER_BUFFER_PENALTY;
        }
        0.0
    }
}

/// One-step Chebyshev (8-connected) dilation of `mask`. Output `out[i] != 0`
/// iff some 8-neighbour of cell `i` is set in `mask`, with `i` itself
/// excluded. X-wraps; Y is bounded. Used to build the river-buffer flag —
/// a "right next to the river but not on it" mask.
fn chebyshev_dilate(mask: &[u8], res: usize) -> Vec<u8> {
    let total = res * res;
    let mut out = vec![0u8; total];
    let res_i = res as i32;
    for i in 0..total {
        if mask[i] == 0 {
            continue;
        }
        let cx = (i % res) as i32;
        let cy = (i / res) as i32;
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
                let ni = (ny as usize) * res + nx;
                if mask[ni] == 0 {
                    out[ni] = 1;
                }
            }
        }
    }
    out
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
    river_field: &RiverField,
) -> Option<Vec<(u32, u32)>> {
    let res = map.config.global_res as usize;
    let res_i = res as i32;
    let elev = &map.elevation_m;
    let mask = &map.land_mask;
    debug_assert_eq!(river_field.mask.len(), res * res);

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
                let is_diag = dx.abs() + dy.abs() == 2;
                // Bridges sit on the 90° grid, so the road must enter and
                // leave every river cell cardinally — reject diagonals
                // that touch the river at `ni`, `ci`, or either off-diagonal
                // shoulder (which would skim past the channel without
                // landing on it).
                if is_diag {
                    if river_field.mask[ni] != 0 || river_field.mask[ci] != 0 {
                        continue;
                    }
                    let sh1 = (cy as usize) * res + (cx + dx).rem_euclid(res_i) as usize;
                    let sh2 = (cy + dy) as usize * res + cx as usize;
                    if river_field.mask[sh1] != 0 || river_field.mask[sh2] != 0 {
                        continue;
                    }
                }
                // Step direction normalised so the dot-product against the
                // unit river tangent in `step_penalty` stays in [-1, 1] —
                // diagonals scale by 1/√2 to match the SQRT_2 step length.
                let (base, sdx, sdy) = if is_diag {
                    (
                        std::f32::consts::SQRT_2,
                        dx as f32 * std::f32::consts::FRAC_1_SQRT_2,
                        dy as f32 * std::f32::consts::FRAC_1_SQRT_2,
                    )
                } else {
                    (1.0, dx as f32, dy as f32)
                };
                let dh = (elev[ni] - h).abs();
                let cost = base + dh * SLOPE_WEIGHT + river_field.step_penalty(ni, sdx, sdy);
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

/// Number of cells on each side of a road↔river crossing forced into a
/// single cardinal axis. Sized so two rounds of Chaikin smoothing in
/// `BakeContext::new` still leave a colinear strip across the crossing
/// (otherwise the smoothed kink at the snap-window boundary leaks into
/// the bridge footprint). With Chaikin moving each interior point by ¼ of
/// each adjacent segment, ±3 cells gives ~5 cells of post-smoothing
/// straight strip — enough for a grid-aligned bridge mesh to drop in.
const GRID_SNAP_HALF_WINDOW: usize = 3;

/// Cardinal axis used by the grid-snap pass. The road takes one axis at a
/// crossing; the river takes the other.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum CardinalAxis {
    Horizontal,
    Vertical,
}

/// Bridges in the runtime are placed grid-aligned at 90°, so this pass
/// rewrites a small window of cells around every road↔river crossing into
/// axis-aligned strips: the road on one cardinal, the river on the other.
/// Mutates both polylines in place; first/last index of each polyline is
/// preserved so settlement / river-source / river-mouth anchors don't
/// drift. Run once after `compute_roads`, before tile baking.
pub fn snap_crossings_to_grid(road_net: &mut RoadNetwork, river_map: &mut RiverMap, res: usize) {
    let total = res * res;
    // Cell → (river_idx, point_idx). First river to claim a cell wins; later
    // tributaries that merge into the same cell are ignored for snap targeting
    // (the crossing still lands on the same physical position).
    let mut river_cell: Vec<Option<(u32, u32)>> = vec![None; total];
    for (ri, poly) in river_map.rivers.iter().enumerate() {
        for (pi, &(x, y)) in poly.points.iter().enumerate() {
            let idx = (y as usize) * res + (x as usize);
            if river_cell[idx].is_none() {
                river_cell[idx] = Some((ri as u32, pi as u32));
            }
        }
    }

    for road_idx in 0..road_net.roads.len() {
        let n = road_net.roads[road_idx].points.len();
        if n < 3 {
            continue;
        }
        // Walk interior road points only — skip the first and last so the
        // settlement endpoints never drift.
        let mut pi = 1;
        while pi + 1 < n {
            let (rx, ry) = road_net.roads[road_idx].points[pi];
            let cell = (ry as usize) * res + (rx as usize);
            let Some((ri, river_pi_u32)) = river_cell[cell] else {
                pi += 1;
                continue;
            };
            let ri = ri as usize;
            let river_pi = river_pi_u32 as usize;

            // Axes come from the river's local direction, not the road's:
            // A* may still leave the road on a diagonal trend even though
            // its entry into the crossing is cardinal, so snapping
            // perpendicular to the road can disagree with the river's
            // actual flow.
            let river_dir = local_dir(
                &river_map.rivers[ri].points,
                river_pi,
                GRID_SNAP_HALF_WINDOW,
                res,
            );
            let (river_axis, road_axis) = if river_dir.0.abs() >= river_dir.1.abs() {
                (CardinalAxis::Horizontal, CardinalAxis::Vertical)
            } else {
                (CardinalAxis::Vertical, CardinalAxis::Horizontal)
            };

            let snapped_road_end = snap_polyline_window(
                &mut road_net.roads[road_idx].points,
                pi,
                GRID_SNAP_HALF_WINDOW,
                road_axis,
                res,
            );
            // Per-vertex flow on the river polyline keeps its index
            // alignment, so width / carve depth still attach to the same
            // logical vertex after the snap.
            let river_poly = &mut river_map.rivers[ri];
            snap_polyline_window(
                &mut river_poly.points,
                river_pi,
                GRID_SNAP_HALF_WINDOW,
                river_axis,
                res,
            );

            // Skip past the just-snapped road window so we don't re-snap
            // adjacent points landing on the same crossing's tail cells.
            pi = snapped_road_end + 1;
        }
    }
}

/// Mean direction across a ±`half_w` slice of a cell-coord polyline. Returns
/// `(dx, dy)` of the chord between the two window endpoints, with X-wrap
/// folded to the shorter side. Used only to pick a cardinal axis, so
/// magnitudes don't need to be normalised.
fn local_dir(points: &[(u32, u32)], idx: usize, half_w: usize, res: usize) -> (i32, i32) {
    let n = points.len();
    let i_lo = idx.saturating_sub(half_w);
    let i_hi = (idx + half_w).min(n - 1);
    let (px, py) = points[i_lo];
    let (qx, qy) = points[i_hi];
    let res_i = res as i32;
    let mut dx = qx as i32 - px as i32;
    if dx > res_i / 2 {
        dx -= res_i;
    } else if dx < -res_i / 2 {
        dx += res_i;
    }
    let dy = qy as i32 - py as i32;
    (dx, dy)
}

/// Replace `points[i_start..=i_end]` (clamped to leave the first / last
/// vertex of the polyline anchored) with cells lying on a single cardinal
/// line through `(cx, cy)`. The along-axis coordinate steps linearly from
/// the unchanged neighbour-outside-the-window value to the other side, so
/// the only kinks introduced are right at the window boundaries — within
/// the window the polyline is strictly axis-aligned.
///
/// Returns the highest index actually overwritten so the caller can resume
/// scanning past the snapped span.
fn snap_polyline_window(
    points: &mut [(u32, u32)],
    idx: usize,
    half_w: usize,
    axis: CardinalAxis,
    res: usize,
) -> usize {
    let n = points.len();
    if n < 3 {
        return idx;
    }
    // Endpoint guard: first/last index always preserved (anchors on
    // settlement / river source / river mouth).
    let i_start = idx.saturating_sub(half_w).max(1);
    let i_end = (idx + half_w).min(n - 2);
    if i_start > i_end {
        return idx;
    }
    let len = i_end - i_start;
    let res_i = res as i32;
    let (cx, cy) = points[idx];

    match axis {
        CardinalAxis::Horizontal => {
            // Anchor along-axis (X) at the unchanged neighbours just outside
            // the window so the snapped strip joins the rest of the polyline
            // without a sudden along-axis jump (only the cross-axis Y bends).
            let x_lo = points[i_start - 1].0 as i32;
            let x_hi = points[(i_end + 1).min(n - 1)].0 as i32;
            let mut delta = x_hi - x_lo;
            if delta > res_i / 2 {
                delta -= res_i;
            } else if delta < -res_i / 2 {
                delta += res_i;
            }
            let span = (len + 2) as f32;
            for k in 0..=len {
                let t = (k as f32 + 1.0) / span;
                let x = x_lo + (delta as f32 * t).round() as i32;
                points[i_start + k] = (x.rem_euclid(res_i) as u32, cy);
            }
        }
        CardinalAxis::Vertical => {
            let y_lo = points[i_start - 1].1 as i32;
            let y_hi = points[(i_end + 1).min(n - 1)].1 as i32;
            let delta = y_hi - y_lo;
            let span = (len + 2) as f32;
            for k in 0..=len {
                let t = (k as f32 + 1.0) / span;
                let y = y_lo + (delta as f32 * t).round() as i32;
                points[i_start + k] = (cx, y.clamp(0, res_i - 1) as u32);
            }
        }
    }
    i_end
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

    #[test]
    fn snap_aligns_road_and_river_at_crossing() {
        // Synthetic crossing: a diagonal road meets an N-S river at one
        // shared cell. The river's local direction (vertical) drives the
        // axis choice — river snaps to a single column, road snaps to a
        // single row — so a 90°-grid bridge mesh fits across both
        // polylines.
        let res = 32usize;
        let road_pts: Vec<(u32, u32)> = (0..16).map(|i| (8 + i, 8 + i)).collect();
        let crossing_road_idx = 8; // Cell (16, 16) on the diagonal road.
        let crossing_cell = road_pts[crossing_road_idx];

        // River runs strictly N-S through the crossing cell. With
        // |dy| > |dx|, snap picks `river_axis = Vertical`, so the river
        // stays on its column and the road snaps to row y=16.
        let river_pts: Vec<(u32, u32)> = (0..16).map(|i| (crossing_cell.0, 8 + i)).collect();
        let crossing_river_idx = river_pts
            .iter()
            .position(|&p| p == crossing_cell)
            .expect("river must pass through the crossing cell");

        let mut net = RoadNetwork {
            roads: vec![Road {
                points: road_pts.clone(),
            }],
        };
        let mut river_map = RiverMap {
            downstream: Vec::new(),
            flow: Vec::new(),
            rivers: vec![Polyline {
                points: river_pts.clone(),
                flow: vec![1.0; river_pts.len()],
            }],
        };
        snap_crossings_to_grid(&mut net, &mut river_map, res);

        let snapped_road = &net.roads[0].points;
        let snapped_river = &river_map.rivers[0].points;
        // Endpoint anchors must survive the snap.
        assert_eq!(snapped_road.first(), Some(&road_pts[0]));
        assert_eq!(snapped_road.last(), Some(&road_pts[road_pts.len() - 1]));
        assert_eq!(snapped_river.first(), Some(&river_pts[0]));
        assert_eq!(snapped_river.last(), Some(&river_pts[river_pts.len() - 1]));

        // Road window around the crossing must share Y — strictly
        // axis-aligned, perpendicular to the river's flow direction.
        let half = GRID_SNAP_HALF_WINDOW;
        for k in (crossing_road_idx - half)..=(crossing_road_idx + half) {
            assert_eq!(
                snapped_road[k].1, crossing_cell.1,
                "road point {} not on snap row at crossing",
                k
            );
        }
        // River window must share X (already true here, but the snap
        // should leave it unchanged on its own column).
        for k in (crossing_river_idx - half)..=(crossing_river_idx + half) {
            assert_eq!(
                snapped_river[k].0, crossing_cell.0,
                "river point {} not on snap column at crossing",
                k
            );
        }
        // Crossing cell still appears on both polylines so the bridge has
        // a coincident attach point.
        assert!(snapped_road.contains(&crossing_cell));
        assert!(snapped_river.contains(&crossing_cell));
    }
}
