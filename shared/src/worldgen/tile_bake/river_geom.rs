//! Shared river-width derivation: flow → baked width plus the mouth-fan
//! factor. Note that bake-time polylines are NOT widened by the fan factor
//! — the tile baker splits each mouth into distributary branches instead
//! (`context::apply_mouth_distributaries`). The fan factor lives on only to
//! predict where bridges would land too wide so road A* can detour around
//! delta mouths before bake.

use super::super::global_map::GlobalMap;
use super::super::grid::fold_x_delta_f32;
use super::super::rivers::RiverMap;
use super::constants::{
    RIVER_MAX_WIDTH_M, RIVER_MIN_WIDTH_M, RIVER_MOUTH_FAN_EXTRA, RIVER_MOUTH_FAN_SHARPNESS,
};

pub use super::constants::RIVER_MOUTH_FAN_ARC_CELLS;

/// Render-side ribbon adjustments — keep in sync with `river-geometry.ts`'s
/// `RIVER_WIDTH_SCALE` and `RIVER_WIDTH_PAD_M`.
pub const RIVER_RIBBON_WIDTH_SCALE: f32 = 1.5;
pub const RIVER_RIBBON_WIDTH_PAD_M: f32 = 1.0;

/// Hard cap (rendered ribbon meters) above which no bridge is placed and
/// road A* refuses to cross. Wider crossings are visually implausible for
/// the catalog's stone bridge models and almost always sit in the
/// mouth-fan / delta zone.
pub const BRIDGE_MAX_VISIBLE_WIDTH_M: f32 = 29.0;

/// Baked-width equivalent of [`BRIDGE_MAX_VISIBLE_WIDTH_M`], used by
/// callers that only see baked widths (road A* prediction, before the
/// ribbon expansion).
pub const BRIDGE_MAX_BAKED_WIDTH_M: f32 =
    (BRIDGE_MAX_VISIBLE_WIDTH_M - RIVER_RIBBON_WIDTH_PAD_M * 2.0) / RIVER_RIBBON_WIDTH_SCALE;

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

/// Convert a baked river width (m) to its rendered ribbon width.
#[inline]
pub fn baked_to_visible_width(baked: f32) -> f32 {
    baked * RIVER_RIBBON_WIDTH_SCALE + RIVER_RIBBON_WIDTH_PAD_M * 2.0
}

/// Per-cell boolean mask: `true` where any river polyline's predicted baked
/// width (flow → width with the mouth-fan multiplier applied) exceeds
/// [`BRIDGE_MAX_BAKED_WIDTH_M`]. Mirrors the gate road A* applies in
/// [`crate::worldgen::roads::astar::RiverField`], so consumers like settlement
/// placement can refuse to seat a village on the same cell road A* would
/// detour around.
pub fn wide_river_cell_mask(map: &GlobalMap, river_map: &RiverMap) -> Vec<bool> {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mut out = vec![false; total];
    let res_f = res as f32;
    let inv_log_max = flow_log_inv(river_map.max_flow());
    let inv_arc = 1.0 / RIVER_MOUTH_FAN_ARC_CELLS.max(1e-3);

    for poly in &river_map.rivers {
        let pts = &poly.points;
        let n = pts.len();
        if n < 2 {
            continue;
        }
        let mut lens: Vec<f32> = Vec::with_capacity(n);
        lens.push(0.0);
        let mut cumulative = 0.0f32;
        for i in 1..n {
            let (px, py) = pts[i - 1];
            let (qx, qy) = pts[i];
            let dx = fold_x_delta_f32(qx as f32 - px as f32, res_f);
            let dy = qy as f32 - py as f32;
            cumulative += (dx * dx + dy * dy).sqrt();
            lens.push(cumulative);
        }
        let total_arc = cumulative;
        let (end_x, end_y) = pts[n - 1];
        let mouth_in_sea = map.land_mask[(end_y as usize) * res + (end_x as usize)] == 0;

        for i in 0..n {
            let (x, y) = pts[i];
            let idx = (y as usize) * res + (x as usize);
            let base_w = flow_to_width(poly.flow[i], inv_log_max);
            let mouth_factor = if mouth_in_sea {
                mouth_fan_factor((total_arc - lens[i]) * inv_arc)
            } else {
                1.0
            };
            if base_w * mouth_factor > BRIDGE_MAX_BAKED_WIDTH_M {
                out[idx] = true;
            }
        }
    }
    out
}
