//! A* over the global cell grid for a single road edge. Step cost combines
//! base distance, slope penalty (linear plus a quadratic excess past the
//! steep threshold), and river orientation/buffer penalties sourced from a
//! per-call `RiverField` overlay. Existing road cells from earlier edges
//! in the same `compute_roads` pass are heavily discounted so trunks form
//! before branches share them. `AStarScratch` lets the search reuse its
//! O(res²) buffers across all edges in one road-network build.

use std::collections::BinaryHeap;

use super::super::global_map::GlobalMap;
use super::super::grid::{fold_delta_f32, MinF32};
use super::super::rivers::RiverMap;
use super::super::tile_bake::river_geom::{
    flow_log_inv, polyline_arcs, predicted_visible_width, BRIDGE_MAX_VISIBLE_WIDTH_M,
    RIVER_DELTA_BUFFER_ARC_CELLS,
};
use super::axis::{pick_river_axis, step_axis, SnapAxis};

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

/// Cost multiplier for an A* step that lands on a cell already covered by
/// an earlier road in the same `compute_roads` pass. Slope, river-crossing
/// orientation, and detour cost have already been "paid" by whoever laid
/// the trunk, so following it is essentially free — A* should funnel
/// toward existing pavement and only break new ground when the detour
/// would be much longer than the direct route. Edges are processed
/// longest-first so trunks form before branches; 0.5× balances merging
/// (so two cities heading the same way don't lay parallel pavement) with
/// preserving genuine alternate routes (e.g. a mountain pass shortcut
/// shouldn't get sucked onto a long valley trunk just because the trunk
/// exists).
const EXISTING_ROAD_FACTOR: f32 = 0.5;

/// Per-cell river overlay used by A*. `mask[i]` is 0 for non-river,
/// [`MASK_RIVER`] for normal river cells, and [`MASK_WIDE`] for cells that
/// either sit inside a sea-bound river's post-apex delta region (where
/// `apply_mouth_distributaries` would replace one channel with several
/// branches a single bridge can't span coherently) or carry a predicted
/// natural visible width above [`BRIDGE_MAX_VISIBLE_WIDTH_M`]. Road A*
/// refuses to step into wide cells (except as start/goal) so roads detour
/// **upstream of the apex** to find a single-channel crossing.
/// `tangent` / `axis` / `near_river` describe geometry around the river
/// cells for the perpendicular-cross gate and breathing-room buffer.
pub(super) struct RiverField {
    mask: Vec<u8>,
    tangent: Vec<(f32, f32)>,
    /// Snap-axis class of each river cell, derived from `tangent`. Cached
    /// at construction so the per-step A* perpendicularity gate is a byte
    /// load instead of 4 muls + 4 compares per river-touching neighbour.
    axis: Vec<SnapAxis>,
    near_river: Vec<u8>,
}

const MASK_RIVER: u8 = 1;
const MASK_WIDE: u8 = 2;

impl RiverField {
    pub(super) fn from_river_map(river_map: &RiverMap, map: &GlobalMap) -> Self {
        let res = map.config.global_res as usize;
        let total = res * res;
        let mut mask = vec![0u8; total];
        let mut tangent = vec![(0.0f32, 0.0f32); total];
        let mut axis = vec![SnapAxis::Horizontal; total];
        let res_f = res as f32;
        let inv_log_max = flow_log_inv(river_map.max_flow());

        for (poly_idx, poly) in river_map.rivers.iter().enumerate() {
            let Some(arcs) = polyline_arcs(poly, map, poly_idx) else {
                continue;
            };
            let pts = &poly.points;
            let n = pts.len();

            for (i, &(x, y)) in pts.iter().enumerate() {
                let idx = (y as usize) * res + (x as usize);
                let prev = if i == 0 { pts[i] } else { pts[i - 1] };
                let next = if i + 1 >= n { pts[i] } else { pts[i + 1] };
                let dx = fold_delta_f32(next.0 as f32 - prev.0 as f32, res_f);
                let dy = fold_delta_f32(next.1 as f32 - prev.1 as f32, res_f);
                let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                tangent[idx] = (dx / len, dy / len);
                axis[idx] = pick_river_axis(dx / len, dy / len);

                // Multiple polylines can touch the same cell; widen-wins
                // so the wide flag survives a narrower polyline overwrite.
                // The buffer (wider than the fan apex) keeps A* detoured
                // past the visible distributary fan, not just past apex.
                let in_buffer =
                    arcs.mouth_in_sea && arcs.arc_to_mouth(i) < RIVER_DELTA_BUFFER_ARC_CELLS;
                let is_wide = in_buffer
                    || predicted_visible_width(poly.flow[i], inv_log_max)
                        > BRIDGE_MAX_VISIBLE_WIDTH_M;
                let new_mark = if is_wide { MASK_WIDE } else { MASK_RIVER };
                if new_mark > mask[idx] {
                    mask[idx] = new_mark;
                }
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
/// excluded. Both axes wrap. Used to build the river-buffer flag —
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
                let ny = (cy + dy).rem_euclid(res_i) as usize;
                let ni = ny * res + nx;
                if mask[ni] == 0 {
                    out[ni] = 1;
                }
            }
        }
    }
    out
}

pub(super) struct AStarScratch {
    g_score: Vec<f32>,
    came_from: Vec<u32>,
    closed: Vec<bool>,
    open: BinaryHeap<MinF32>,
    /// Cells touched by the previous run, so reset() only revisits them
    /// instead of fill()-ing all res² entries every edge.
    touched: Vec<u32>,
}

impl AStarScratch {
    pub(super) fn new(total: usize) -> Self {
        Self {
            g_score: vec![f32::INFINITY; total],
            came_from: vec![u32::MAX; total],
            closed: vec![false; total],
            open: BinaryHeap::new(),
            touched: Vec::new(),
        }
    }
    pub(super) fn reset(&mut self) {
        for &i in &self.touched {
            let idx = i as usize;
            self.g_score[idx] = f32::INFINITY;
            self.came_from[idx] = u32::MAX;
            self.closed[idx] = false;
        }
        self.touched.clear();
        self.open.clear();
    }
    /// Add `idx` to the reset list if its g_score is still at the
    /// untouched sentinel (infinity). Idempotent — safe to call on every
    /// neighbor relaxation; only the first call per cell pushes.
    #[inline]
    fn touch_if_new(&mut self, idx: usize) {
        if self.g_score[idx].is_infinite() {
            self.touched.push(idx as u32);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn a_star(
    map: &GlobalMap,
    sx: usize,
    sy: usize,
    gx: usize,
    gy: usize,
    scratch: &mut AStarScratch,
    river_field: &RiverField,
    road_mask: &[u8],
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

    scratch.touch_if_new(start);
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
                let ny = (cy + dy).rem_euclid(res_i) as usize;
                let ni = ny * res + nx;
                if mask[ni] == 0 || scratch.closed[ni] {
                    continue;
                }
                // Wide cells are impassable except as start/goal so a
                // settlement that lands on a wide-marked cell can still
                // terminate a road.
                if ni != start && ni != goal && river_field.mask[ni] == MASK_WIDE {
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
                    let sh2 = (cy + dy).rem_euclid(res_i) as usize * res + cx as usize;
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
                // Existing-road cells: a previous edge already laid this
                // pavement, so re-using it skips slope/river penalties
                // entirely (see EXISTING_ROAD_FACTOR for the trade-off).
                let cost = if road_mask[ni] != 0 {
                    base * EXISTING_ROAD_FACTOR
                } else {
                    let dh = (elev[ni] - h).abs();
                    // Grade is per cell of horizontal travel so diagonals
                    // benefit fairly. Quadratic excess past the steep
                    // threshold makes A* contour around steep faces
                    // instead of climbing them.
                    let step_length_m = base * meters_per_cell;
                    let grade = dh / step_length_m;
                    let excess = (grade - SLOPE_STEEP_THRESHOLD).max(0.0);
                    let slope_cost =
                        base * (grade * SLOPE_WEIGHT_LIN + excess * excess * SLOPE_QUAD_WEIGHT);
                    base + slope_cost + river_field.step_penalty(ni, sdx, sdy)
                };
                let tentative = scratch.g_score[ci] + cost;
                if tentative < scratch.g_score[ni] {
                    scratch.touch_if_new(ni);
                    scratch.g_score[ni] = tentative;
                    scratch.came_from[ni] = cur;
                    let f = tentative + heuristic(nx, ny, gx, gy, res);
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
    let dy_raw = (sy as f32 - gy as f32).abs();
    let dy = dy_raw.min(res as f32 - dy_raw);
    (dx * dx + dy * dy).sqrt()
}
