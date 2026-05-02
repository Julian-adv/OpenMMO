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

/// Linear penalty per unit grade applied to every road step, scaled by the
/// step's horizontal length in cells. At a 5 % grade this adds
/// `0.05 * SLOPE_WEIGHT_LIN` cells of cost per cell of travel — gentle
/// background bias that bends roads slightly toward the contour line on
/// rolling hills without introducing detours on truly flat ground.
const SLOPE_WEIGHT_LIN: f32 = 0.4;
/// Grade above which the quadratic steep-slope penalty kicks in
/// (≈10 %, the steep edge of comfortable highway grades). Below this only
/// the linear term contributes.
const SLOPE_STEEP_THRESHOLD: f32 = 0.10;
/// Quadratic weight on `(grade - SLOPE_STEEP_THRESHOLD)²` above the
/// threshold, in cells of cost per cell of horizontal travel. Tuned so a
/// 20 % grade pays ~0.7 cells/cell, 30 % ~2.5, 40 % ~5.5, 50 % ~10 —
/// large enough that A* prefers contour-following detours of tens of cells
/// over taking a steep face head-on, naturally bending roads around steep
/// hillsides instead of climbing them. (True switchbacks would need
/// direction-aware A* state and aren't modeled.)
const SLOPE_QUAD_WEIGHT: f32 = 60.0;

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
    /// Snap-axis class of each river cell, derived from `tangent`. Cached
    /// at construction so the per-step A* perpendicularity gate is a byte
    /// load instead of 4 muls + 4 compares per river-touching neighbour.
    axis: Vec<SnapAxis>,
    near_river: Vec<u8>,
}

impl RiverField {
    fn from_polylines(rivers: &[Polyline], res: usize) -> Self {
        let total = res * res;
        let mut mask = vec![0u8; total];
        let mut tangent = vec![(0.0f32, 0.0f32); total];
        let mut axis = vec![SnapAxis::Horizontal; total];
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
                let tx = dx / len;
                let ty = dy / len;
                tangent[idx] = (tx, ty);
                axis[idx] = pick_river_axis(tx, ty);
            }
        }
        let near_river = chebyshev_dilate(&mask, res);
        Self {
            mask,
            tangent,
            axis,
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
    let meters_per_cell = map.config.meters_per_cell();
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
                let ci_river = river_field.mask[ci] != 0;
                let ni_river = river_field.mask[ni] != 0;
                // Bridges always sit at 90° to the river but support 4 grid
                // orientations (H / V / NW-SE / NE-SW), so any river-touching
                // step must be on the perpendicular of the river's local
                // snap-axis class — non-perpendicular crossings are
                // rejected outright.
                if ci_river || ni_river {
                    let endpoint = if ni_river { ni } else { ci };
                    if step_axis(dx, dy) != river_field.axis[endpoint].perpendicular() {
                        continue;
                    }
                } else if is_diag {
                    // Pure-land diagonal: reject corner-cuts where a
                    // shoulder is river (would skim past a 1-cell channel).
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
                // Grade is per cell of horizontal travel (so diagonals
                // benefit fairly: same dh over √2 cells reads as a gentler
                // slope). Quadratic excess past `SLOPE_STEEP_THRESHOLD`
                // makes steep faces explode in cost so A* contours around
                // them instead of climbing.
                let step_length_m = base * meters_per_cell;
                let grade = dh / step_length_m;
                let excess = (grade - SLOPE_STEEP_THRESHOLD).max(0.0);
                let slope_cost =
                    base * (grade * SLOPE_WEIGHT_LIN + excess * excess * SLOPE_QUAD_WEIGHT);
                let cost = base + slope_cost + river_field.step_penalty(ni, sdx, sdy);
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

/// Axis used by the grid-snap pass. The road takes one axis at a
/// crossing; the river takes the perpendicular partner. Cardinals
/// (`Horizontal`/`Vertical`) pair with each other; diagonals
/// (`DiagonalNwSe`/`DiagonalNeSw`) pair with each other.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum SnapAxis {
    Horizontal,
    Vertical,
    /// `dy = dx` line (running NW↔SE).
    DiagonalNwSe,
    /// `dy = -dx` line (running NE↔SW).
    DiagonalNeSw,
}

impl SnapAxis {
    fn perpendicular(self) -> SnapAxis {
        match self {
            SnapAxis::Horizontal => SnapAxis::Vertical,
            SnapAxis::Vertical => SnapAxis::Horizontal,
            SnapAxis::DiagonalNwSe => SnapAxis::DiagonalNeSw,
            SnapAxis::DiagonalNeSw => SnapAxis::DiagonalNwSe,
        }
    }
}

/// Pick the snap axis whose unit direction is most aligned with `(dx, dy)`.
/// Compares squared projections — the four axes form a 45°-step rosette,
/// so the dominant projection wins by ≥ cos²(22.5°) margin in the generic
/// case. Used both to classify river tangents (A* perpendicularity gate)
/// and to drive the grid-snap pass.
fn pick_river_axis(dx: f32, dy: f32) -> SnapAxis {
    let h = dx * dx;
    let v = dy * dy;
    let nw_se = (dx + dy).powi(2) * 0.5;
    let ne_sw = (dx - dy).powi(2) * 0.5;
    if h >= v && h >= nw_se && h >= ne_sw {
        SnapAxis::Horizontal
    } else if v >= nw_se && v >= ne_sw {
        SnapAxis::Vertical
    } else if nw_se >= ne_sw {
        SnapAxis::DiagonalNwSe
    } else {
        SnapAxis::DiagonalNeSw
    }
}

/// Snap-axis classification of a single A* step. The 8-way step neighbourhood
/// maps onto the 4 axes: `(±1, 0) → Horizontal`, `(0, ±1) → Vertical`,
/// `(±1, ±1) same sign → NW-SE`, opposite sign `→ NE-SW`.
fn step_axis(dx: i32, dy: i32) -> SnapAxis {
    match (dx, dy) {
        (_, 0) => SnapAxis::Horizontal,
        (0, _) => SnapAxis::Vertical,
        (a, b) if a == b => SnapAxis::DiagonalNwSe,
        _ => SnapAxis::DiagonalNeSw,
    }
}

/// Bridges in the runtime are placed at 90° to the channel — supporting
/// both grid-aligned and 45°-rotated drops — so this pass rewrites a small
/// window of cells around every road↔river crossing into axis-aligned
/// strips: the road on one snap axis, the river on the perpendicular
/// partner. The chosen pair (cardinal-cardinal or diagonal-diagonal)
/// follows the river's local direction. Mutates both polylines in place;
/// first/last index of each polyline is preserved so settlement /
/// river-source / river-mouth anchors don't drift. Run once after
/// `compute_roads`, before tile baking.
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
            let river_axis = pick_river_axis(river_dir.0 as f32, river_dir.1 as f32);
            let road_axis = river_axis.perpendicular();

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
    let dx = fold_x_delta(qx as i32 - px as i32, res_i);
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
    axis: SnapAxis,
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
    let cx_i = cx as i32;
    let cy_i = cy as i32;
    let span = (len + 2) as f32;
    let hi_idx = (i_end + 1).min(n - 1);

    // Parameterise the snapped strip as `(cx, cy) + s * (ux, uy)`, where
    // `(ux, uy)` is the integer along-axis direction (unit length for
    // cardinals, √2 for diagonals — we divide by `len_sq` so `s` steps in
    // cells along the axis). The cross-axis component is implicitly 0:
    // points snap onto the line through the crossing cell, only the
    // along-axis offset interpolates between the anchor neighbours just
    // outside the window. This produces the same single-kink-at-boundary
    // join discipline for all four axes.
    let (ux, uy, len_sq) = match axis {
        SnapAxis::Horizontal => (1, 0, 1),
        SnapAxis::Vertical => (0, 1, 1),
        SnapAxis::DiagonalNwSe => (1, 1, 2),
        SnapAxis::DiagonalNeSw => (1, -1, 2),
    };
    let (x_lo, y_lo) = points[i_start - 1];
    let (x_hi, y_hi) = points[hi_idx];
    let dx_lo = fold_x_delta(x_lo as i32 - cx_i, res_i);
    let dy_lo = y_lo as i32 - cy_i;
    let dx_hi = fold_x_delta(x_hi as i32 - cx_i, res_i);
    let dy_hi = y_hi as i32 - cy_i;
    let inv_len_sq = 1.0 / len_sq as f32;
    let s_lo = (dx_lo * ux + dy_lo * uy) as f32 * inv_len_sq;
    let s_hi = (dx_hi * ux + dy_hi * uy) as f32 * inv_len_sq;
    for k in 0..=len {
        let t = (k as f32 + 1.0) / span;
        let s = (s_lo + (s_hi - s_lo) * t).round() as i32;
        let x = (cx_i + s * ux).rem_euclid(res_i) as u32;
        let y = (cy_i + s * uy).clamp(0, res_i - 1) as u32;
        points[i_start + k] = (x, y);
    }
    i_end
}

/// Fold a raw X delta into the shorter wrap direction (≤ res/2 in magnitude).
#[inline]
fn fold_x_delta(mut d: i32, res_i: i32) -> i32 {
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

    #[test]
    fn snap_picks_diagonal_axes_for_diagonal_river() {
        // Synthetic crossing: a NW-SE river meets a NE-SW road at one
        // shared cell. The river's local direction is (+1, +1) so snap
        // picks `river_axis = DiagonalNwSe`, and the road snaps to the
        // perpendicular `DiagonalNeSw` line through the crossing cell.
        let res = 64usize;
        let crossing_cell = (32u32, 32u32);

        // River along y = x (NW → SE) through the crossing cell.
        let river_pts: Vec<(u32, u32)> = (0..32).map(|i| (16 + i as u32, 16 + i as u32)).collect();
        let crossing_river_idx = river_pts
            .iter()
            .position(|&p| p == crossing_cell)
            .expect("river must pass through the crossing cell");

        // Road along y = -x + 64 (NE → SW) through the crossing cell.
        let road_pts: Vec<(u32, u32)> = (0..32).map(|i| (16 + i as u32, 48 - i as u32)).collect();
        let crossing_road_idx = road_pts
            .iter()
            .position(|&p| p == crossing_cell)
            .expect("road must pass through the crossing cell");

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

        // River window: every cell satisfies `dy = dx` relative to the
        // crossing — strictly on the NW-SE diagonal.
        let half = GRID_SNAP_HALF_WINDOW;
        for k in (crossing_river_idx - half)..=(crossing_river_idx + half) {
            let (x, y) = snapped_river[k];
            let dx = x as i32 - crossing_cell.0 as i32;
            let dy = y as i32 - crossing_cell.1 as i32;
            assert_eq!(dy, dx, "river point {k} not on NW-SE diagonal");
        }
        // Road window: every cell satisfies `dy = -dx` — on the NE-SW
        // diagonal perpendicular to the river.
        for k in (crossing_road_idx - half)..=(crossing_road_idx + half) {
            let (x, y) = snapped_road[k];
            let dx = x as i32 - crossing_cell.0 as i32;
            let dy = y as i32 - crossing_cell.1 as i32;
            assert_eq!(dy, -dx, "road point {k} not on NE-SW diagonal");
        }
        // Crossing cell still appears on both polylines so the bridge has
        // a coincident attach point.
        assert!(snapped_road.contains(&crossing_cell));
        assert!(snapped_river.contains(&crossing_cell));
    }
}
