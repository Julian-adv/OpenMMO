//! Shared river-width derivation: flow → baked width, plus the mouth-fan
//! curve used by the renderer/per-pixel width math. Bake-time polylines are
//! NOT widened by the fan factor — the tile baker splits each mouth into
//! distributary branches instead (`context::apply_mouth_distributaries`).
//! Two delta thresholds split this concept:
//! [`RIVER_MOUTH_FAN_ARC_CELLS`] gates branch generation and settlement
//! habitability (so port towns sit just above the apex);
//! [`RIVER_DELTA_BUFFER_ARC_CELLS`] gates road A* and bridge placement so
//! the chosen crossing lands upstream of the visible fan.

use super::super::global_map::GlobalMap;
use super::super::grid::fold_delta_f32;
use super::super::rivers::{Polyline, RiverMap};
use super::constants::{
    RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M, RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M,
    RIVER_MOUTH_FAN_EXTRA, RIVER_MOUTH_FAN_SHARPNESS,
};

pub use super::constants::{RIVER_DELTA_BUFFER_ARC_CELLS, RIVER_MOUTH_FAN_ARC_CELLS};

/// Hard cap (visible water meters — `baked_width + carve_taper`) above
/// which no bridge is placed and road A* refuses to cross. Matches the wide
/// bridge model's deck length so the deck ends always land on solid bank
/// past the river's depth-fade contour.
pub const BRIDGE_MAX_VISIBLE_WIDTH_M: f32 = 28.0;

/// Predicted visible water width (m) at a river vertex: baked channel width
/// plus the natural-reach carve taper that grows with `flow_norm`. The
/// taper is added once (not 2×) because the `river_field` shader's
/// surface→bed smoothstep puts the *visible* water boundary near
/// `half_width + 0.5·taper` on each side, not at the alpha-zero contour
/// `half_width + taper`. At max flow this reads ~20 m visible.
///
/// Ignores the mouth-fan multiplier: bake-time replaces the post-apex tail
/// with narrower distributary branches (see
/// `context::apply_mouth_distributaries`), so the wide single-channel
/// envelope `mouth_fan_factor` describes never actually gets carved.
/// Distributary branches use the shorter [`RIVER_MOUTH_BRANCH_TAPER_M`]
/// (5 m fixed); for branches this remains an upper bound and bake-time
/// `segment_carve_taper_at` resolves the true width when the bridge is
/// placed.
#[inline]
pub fn predicted_visible_width(raw_flow: f32, inv_log_max: f32) -> f32 {
    let norm = if inv_log_max <= 0.0 {
        0.0
    } else {
        (raw_flow.max(1.0).log2() * inv_log_max).clamp(0.0, 1.0)
    };
    let baked = RIVER_MIN_WIDTH_M + (RIVER_MAX_WIDTH_M - RIVER_MIN_WIDTH_M) * norm;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * norm;
    baked + taper
}

// Pre-folded mouth-fan curve coefficients. `s(t) = (1/(k·t+1) - 1/(1+k)) · (1+k)/k`
// peaks at 1.0 at the mouth (t=0) and decays to 0 at t=1.
const MOUTH_FAN_K: f32 = RIVER_MOUTH_FAN_SHARPNESS;
const MOUTH_FAN_S_NORM: f32 = (1.0 + MOUTH_FAN_K) / MOUTH_FAN_K;
const MOUTH_FAN_INV_ONE_PLUS_K: f32 = 1.0 / (1.0 + MOUTH_FAN_K);

/// Pre-compute `1 / log2(max_flow)` once per polyline batch; pass into
/// [`flow_to_width`]. Returns 0 when `max_flow ≤ 1` so the width
/// degrades to `RIVER_MIN_WIDTH_M`.
#[inline]
pub fn flow_log_inv(max_flow: f32) -> f32 {
    if max_flow > 1.0 {
        1.0 / max_flow.log2()
    } else {
        0.0
    }
}

/// Map a raw flow accumulation to baked river width in meters using the
/// log-flow normalization. `inv_log_max` should come from
/// [`flow_log_inv`] hoisted out of the per-vertex loop.
#[inline]
pub fn flow_to_width(raw: f32, inv_log_max: f32) -> f32 {
    let norm = if inv_log_max <= 0.0 {
        0.0
    } else {
        (raw.max(1.0).log2() * inv_log_max).clamp(0.0, 1.0)
    };
    RIVER_MIN_WIDTH_M + (RIVER_MAX_WIDTH_M - RIVER_MIN_WIDTH_M) * norm
}

/// Mouth-fan multiplicative width boost at normalised arc-distance
/// `t = arc_remaining_to_mouth / arc_window`. `t = 0` at the mouth (peak
/// boost = `1 + EXTRA`), `t ≥ 1` outside the window (factor = 1).
#[inline]
pub fn mouth_fan_factor(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let s = (1.0 / (MOUTH_FAN_K * t + 1.0) - MOUTH_FAN_INV_ONE_PLUS_K) * MOUTH_FAN_S_NORM;
    1.0 + RIVER_MOUTH_FAN_EXTRA * s
}

/// Per-cell boolean mask: `true` where any river polyline vertex would
/// either (a) sit in the post-apex delta region of a sea-bound mouth (where
/// `apply_mouth_distributaries` replaces the natural channel with several
/// branches a single bridge can't span), or (b) carry a predicted visible
/// width above [`BRIDGE_MAX_VISIBLE_WIDTH_M`] on a non-delta reach. Used
/// by settlement habitability — the narrow [`RIVER_MOUTH_FAN_ARC_CELLS`]
/// gate lets port towns sit just above the apex.
pub fn wide_river_cell_mask(map: &GlobalMap, river_map: &RiverMap) -> Vec<bool> {
    let res = map.config.global_res as usize;
    let mut out = vec![false; res * res];
    let inv_log_max = flow_log_inv(river_map.max_flow());

    for (poly_idx, poly) in river_map.rivers.iter().enumerate() {
        let Some(arcs) = polyline_arcs(poly, map, poly_idx) else {
            continue;
        };
        for (i, &(x, y)) in poly.points.iter().enumerate() {
            let in_fan = arcs.mouth_in_sea && arcs.arc_to_mouth(i) < RIVER_MOUTH_FAN_ARC_CELLS;
            let too_wide =
                predicted_visible_width(poly.flow[i], inv_log_max) > BRIDGE_MAX_VISIBLE_WIDTH_M;
            if in_fan || too_wide {
                out[(y as usize) * res + (x as usize)] = true;
            }
        }
    }
    out
}

/// Cumulative arc length (in global cells) along a polyline, with X-wrap
/// folded so seam-crossing pieces measure their on-grid distance instead of
/// wrapping the long way around. `out[0] = 0`, `out[i]` = sum of segment
/// lengths up to and including the i-th vertex.
pub fn polyline_arc_lengths_cells(pts: &[(u32, u32)], res_f: f32) -> Vec<f32> {
    let n = pts.len();
    let mut lens = Vec::with_capacity(n);
    lens.push(0.0f32);
    let mut cumulative = 0.0f32;
    for i in 1..n {
        let (px, py) = pts[i - 1];
        let (qx, qy) = pts[i];
        let dx = fold_delta_f32(qx as f32 - px as f32, res_f);
        let dy = fold_delta_f32(qy as f32 - py as f32, res_f);
        cumulative += (dx * dx + dy * dy).sqrt();
        lens.push(cumulative);
    }
    lens
}

/// Per-polyline arc-length table + sea-mouth flag, computed once and
/// reused by the three consumers that gate on delta arc-distance
/// ([`wide_river_cell_mask`], `RiverField::from_river_map`,
/// `detect_bridges`). Returned by [`polyline_arcs`].
pub struct PolylineArcs {
    pub lens: Vec<f32>,
    pub total_arc: f32,
    /// `true` when the polyline's downstream end sits on a sea cell — the
    /// precondition for any delta gating. Inland tributaries get
    /// `mouth_in_sea = false` and bypass the gates entirely.
    pub mouth_in_sea: bool,
    /// Index into `river_map.rivers`. Stored so callers building a lookup
    /// table can `collect` straight into an indexed `Vec<Option<_>>`.
    pub poly_idx: usize,
}

impl PolylineArcs {
    /// Arc length from vertex `i` to the polyline's downstream end, in
    /// global cells. Used to compare against
    /// [`RIVER_MOUTH_FAN_ARC_CELLS`] or [`RIVER_DELTA_BUFFER_ARC_CELLS`].
    #[inline]
    pub fn arc_to_mouth(&self, i: usize) -> f32 {
        self.total_arc - self.lens[i]
    }
}

/// Build the arc-length table for one polyline plus its sea-mouth flag, or
/// return `None` for polylines too short to meaningfully sample.
pub fn polyline_arcs(poly: &Polyline, map: &GlobalMap, poly_idx: usize) -> Option<PolylineArcs> {
    let n = poly.points.len();
    if n < 2 {
        return None;
    }
    let res = map.config.global_res as usize;
    let (end_x, end_y) = poly.points[n - 1];
    let mouth_in_sea = map.land_mask[(end_y as usize) * res + (end_x as usize)] == 0;
    let lens = polyline_arc_lengths_cells(&poly.points, res as f32);
    let total_arc = *lens.last().unwrap();
    Some(PolylineArcs {
        lens,
        total_arc,
        mouth_in_sea,
        poly_idx,
    })
}
