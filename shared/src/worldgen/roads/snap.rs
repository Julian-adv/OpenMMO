//! Grid-snap road↔river crossings into matched axis-aligned strips so
//! 90°-grid bridge meshes drop in cleanly across both polylines. Bridges
//! in the runtime support 4 grid orientations (H / V / NW-SE / NE-SW); the
//! pass picks the river's local snap axis and forces the road onto the
//! perpendicular partner over a small window centred on the crossing.
//! First/last index of each polyline is preserved so settlement /
//! river-source / river-mouth anchors don't drift.

use super::super::grid::fold_delta;
use super::super::rivers::RiverMap;
use super::axis::{pick_river_axis, SnapAxis};
use super::RoadNetwork;

/// Number of cells on each side of a road↔river crossing forced into a
/// single cardinal axis. Sized so two rounds of Chaikin smoothing in
/// `BakeContext::new` still leave a colinear strip across the crossing
/// (otherwise the smoothed kink at the snap-window boundary leaks into
/// the bridge footprint). With Chaikin moving each interior point by ¼ of
/// each adjacent segment, ±3 cells gives ~5 cells of post-smoothing
/// straight strip — enough for a grid-aligned bridge mesh to drop in.
const GRID_SNAP_HALF_WINDOW: usize = 3;

/// Run once after `compute_roads`, before tile baking. Mutates both road
/// and river polylines in place.
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
    let dx = fold_delta(qx as i32 - px as i32, res_i);
    let dy = fold_delta(qy as i32 - py as i32, res_i);
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
    let dx_lo = fold_delta(x_lo as i32 - cx_i, res_i);
    let dy_lo = fold_delta(y_lo as i32 - cy_i, res_i);
    let dx_hi = fold_delta(x_hi as i32 - cx_i, res_i);
    let dy_hi = fold_delta(y_hi as i32 - cy_i, res_i);
    let inv_len_sq = 1.0 / len_sq as f32;
    let s_lo = (dx_lo * ux + dy_lo * uy) as f32 * inv_len_sq;
    let s_hi = (dx_hi * ux + dy_hi * uy) as f32 * inv_len_sq;
    for k in 0..=len {
        let t = (k as f32 + 1.0) / span;
        let s = (s_lo + (s_hi - s_lo) * t).round() as i32;
        let x = (cx_i + s * ux).rem_euclid(res_i) as u32;
        let y = (cy_i + s * uy).rem_euclid(res_i) as u32;
        points[i_start + k] = (x, y);
    }
    i_end
}

#[cfg(test)]
mod tests {
    use super::super::Road;
    use super::*;
    use crate::worldgen::rivers::Polyline;

    #[test]
    #[allow(clippy::needless_range_loop)]
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
    #[allow(clippy::needless_range_loop)]
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
