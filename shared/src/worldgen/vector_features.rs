//! Vector feature helpers shared by tile baking.
//!
//! The global map stores rivers, roads, and (eventually) coasts as polylines
//! whose vertices live on the 8 m global-cell grid. Rasterizing these into
//! per-cell masks and looking them up with nearest/bilinear sampling at bake
//! time produces 8 m staircase artifacts. Instead, we convert polylines into
//! world-space meters, smooth them with Chaikin corner-cutting, and query
//! point-to-segment distance directly during tile bake. That gives sub-meter
//! precision without raising the global map resolution.
//!
//! X-axis wrap: the world is cylindrical in X, so polyline segments whose
//! endpoints wrap across the seam are split into two half-segments ending at
//! the ±world/2 edges. Callers whose result must remain continuous across
//! that seam use `segments_near_tile_wrap_x`, which returns translated
//! periodic copies in the query's local coordinate frame.

use super::config::WorldGenConfig;

/// A polyline expressed in world-space meters (x, z).
#[derive(Debug, Clone, Default)]
pub struct WorldPolyline {
    pub points: Vec<[f32; 2]>,
}

/// River polyline in world-space meters with per-vertex scalars. Same
/// `points` semantics as `WorldPolyline`; `flow_norm`, `width`, and
/// `bed_floor` run parallel (same length). Used by tile bake to drive
/// flow-aware carve + sand band and to emit the per-tile `rivers/*.bin`
/// geometry.
#[derive(Debug, Clone, Default)]
pub struct RiverWorldPolyline {
    pub points: Vec<[f32; 2]>,
    /// Normalized flow accumulation in `[0, 1]`. 0 at river source, 1 at
    /// the globally-maximum mouth.
    pub flow_norm: Vec<f32>,
    /// River width in meters at this vertex.
    pub width: Vec<f32>,
    /// Minimum carved bed elevation target at this vertex. Natural rivers
    /// stay at sea level; mouth distributaries can ease below it.
    pub bed_floor: Vec<f32>,
}

/// A single river segment expressed as two endpoints plus per-vertex
/// metadata. All distance queries against rivers go through
/// `nearest_river_segment` so the caller can interpolate flow-dependent
/// carve parameters at the exact projection point `t`.
#[derive(Debug, Clone, Copy)]
pub struct RiverSegment {
    pub ax: f32,
    pub az: f32,
    pub bx: f32,
    pub bz: f32,
    pub flow_norm_a: f32,
    pub flow_norm_b: f32,
    pub width_a: f32,
    pub width_b: f32,
    pub bed_floor_a: f32,
    pub bed_floor_b: f32,
}

/// Convert a polyline to world-space meters by mapping each input point
/// through `to_cell` (returning cell-coord units, where (gx+0.5, gy+0.5) is
/// the center of cell (gx, gy)) and then applying the world transform
/// `pos * mpc - half`. Segments whose endpoints straddle the X seam are
/// split so each output polyline stays on one side of the seam. Source
/// vertices are assumed to be ≤ 1 cell apart in each axis — that's how
/// the seam-split detection (|dx| > half world width) distinguishes a wrap
/// from a long jump.
///
/// The closure form unifies cell-index polylines (rivers/roads, where
/// `to_cell = |(x,y)| [x+0.5, y+0.5]`) with cell-edge half-integer
/// polylines (coasts, where `to_cell = cell_node_to_center` adds the same
/// half-cell so both land in the cell-center frame).
pub fn polyline_to_world<P, F>(points: &[P], cfg: &WorldGenConfig, to_cell: F) -> Vec<WorldPolyline>
where
    F: Fn(&P) -> [f32; 2],
{
    let mpc = cfg.meters_per_cell();
    let half = cfg.world_size_m as f32 * 0.5;
    let to_world = |p: &P| -> [f32; 2] {
        let c = to_cell(p);
        [c[0] * mpc - half, c[1] * mpc - half]
    };

    let mut out: Vec<WorldPolyline> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::with_capacity(points.len());

    for raw in points {
        let p = to_world(raw);
        if let Some(&last) = current.last() {
            if (p[0] - last[0]).abs() > half {
                let edge_last = if last[0] > 0.0 { half } else { -half };
                let edge_p = -edge_last;
                // Interpolate the actual crossing in the unwrapped frame.
                // Reusing `last.z` on one edge and `p.z` on the other leaves
                // diagonal crossings as two open endpoints at different Z,
                // which makes signed-distance queries grow a false side
                // wedge between them.
                let world_size = half * 2.0;
                let p_unwrapped_x = if edge_last > 0.0 {
                    p[0] + world_size
                } else {
                    p[0] - world_size
                };
                let t = ((edge_last - last[0]) / (p_unwrapped_x - last[0])).clamp(0.0, 1.0);
                let edge_z = last[1] + (p[1] - last[1]) * t;
                current.push([edge_last, edge_z]);
                if current.len() >= 2 {
                    out.push(WorldPolyline {
                        points: std::mem::take(&mut current),
                    });
                } else {
                    current.clear();
                }
                current.push([edge_p, edge_z]);
            }
        }
        current.push(p);
    }

    if current.len() >= 2 {
        out.push(WorldPolyline { points: current });
    }
    out
}

/// Map a `(cell_x, cell_y)` index pair to its cell-center coordinate. Use
/// with `polyline_to_world` for river/road polylines whose vertices are
/// stored as integer cell indices.
#[inline]
pub fn cell_index_to_center(p: &(u32, u32)) -> [f32; 2] {
    [p.0 as f32 + 0.5, p.1 as f32 + 0.5]
}

/// Shift a marching-squares coast vertex into the cell-center coordinate
/// frame. `coasts::extract_coasts` emits vertices in the mask's grid-node
/// frame — mask cell `(gx, gy)` is treated as a point at the integer node
/// `(gx, gy)`, so an edge midpoint lands at e.g. `(gx+0.5, gy)`. Every other
/// consumer (the bicubic elevation sampler, `cell_index_to_center` for
/// rivers/roads) places cell `(gx, gy)` at its center `(gx+0.5, gy+0.5)`.
/// Adding half a cell reconciles the two, so the shoreline height blend pins
/// the waterline to the same contour the raw field crosses zero at instead of
/// half a cell (`meters_per_cell * 0.5`) diagonally off it.
#[inline]
pub fn cell_node_to_center(p: &[f32; 2]) -> [f32; 2] {
    [p[0] + 0.5, p[1] + 0.5]
}

/// Chaikin corner-cutting for open and closed polylines. Open polylines keep
/// their first and last vertices. Closed polylines are identified by a
/// repeated first/last vertex and treat the join as an ordinary cyclic edge,
/// so no single raw corner remains pinned in the smoothed loop. The repeated
/// closing vertex is restored in the result.
///
/// After `iterations` rounds, the line reads as a smooth curve — two rounds
/// are enough for 8 m source spacing to look visually curved at 1 m cells.
pub fn chaikin_smooth(poly: &WorldPolyline, iterations: u32) -> WorldPolyline {
    let mut pts = poly.points.clone();
    let closed = pts.len() >= 4 && pts.first() == pts.last();
    if closed {
        // Work on unique loop vertices. Preserving the duplicate endpoints
        // would make their shared raw lattice corner the only corner not cut.
        pts.pop();
    }

    for _ in 0..iterations {
        if pts.len() < 3 {
            break;
        }
        if closed {
            let mut next: Vec<[f32; 2]> = Vec::with_capacity(pts.len() * 2);
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                let q = [0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]];
                let r = [0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]];
                next.push(q);
                next.push(r);
            }
            pts = next;
            continue;
        }

        let mut next: Vec<[f32; 2]> = Vec::with_capacity(pts.len() * 2);
        next.push(pts[0]);
        for w in pts.windows(2) {
            let a = w[0];
            let b = w[1];
            let q = [0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]];
            let r = [0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]];
            next.push(q);
            next.push(r);
        }
        next.push(*pts.last().unwrap());
        pts = next;
    }

    if closed {
        pts.push(pts[0]);
    }
    WorldPolyline { points: pts }
}

/// A single segment expressed as `[ax, az, bx, bz]` in world meters. Flat
/// layout so per-tile segment lists are cache-friendly to scan.
pub type Segment = [f32; 4];

/// Convert a cell-coord river polyline (+ per-vertex flow values) into one
/// or more world-space polylines with normalized flow and per-vertex width.
///
/// Seam splitting follows `polyline_to_world`: when an edge crosses the ±X
/// seam, the sequence is cut and a synthetic endpoint is inserted at ±half
/// world-width. The synthetic endpoint inherits its scalar values (flow,
/// width) from the vertex on the same side of the seam it was inserted for
/// — visual seam artifacts on the cylindrical world are acceptable for
/// now.
///
/// `flow_raw` must have the same length as `points`. `max_flow` is the
/// world-wide maximum used to normalize; any value ≤ 0 degrades the output
/// to all-zeros.
pub fn river_polyline_to_world(
    points: &[(u32, u32)],
    flow_raw: &[f32],
    max_flow: f32,
    min_width_m: f32,
    max_width_m: f32,
    cfg: &WorldGenConfig,
) -> Vec<RiverWorldPolyline> {
    assert_eq!(
        points.len(),
        flow_raw.len(),
        "river polyline points and flow must be the same length"
    );
    let mpc = cfg.meters_per_cell();
    let half = cfg.world_size_m as f32 * 0.5;
    let inv_log_max = if max_flow > 1.0 {
        1.0 / max_flow.log2()
    } else {
        0.0
    };
    // log2 mapping compresses the ~10⁴ dynamic range of flow accumulation
    // into a perceptually even [0, 1]. Linear would stuff 99 % of cells
    // into the minimum-width bin.
    let flow_to_norm = |raw: f32| -> f32 {
        if inv_log_max <= 0.0 {
            return 0.0;
        }
        let r = raw.max(1.0);
        (r.log2() * inv_log_max).clamp(0.0, 1.0)
    };
    let norm_to_width = |t: f32| -> f32 { min_width_m + (max_width_m - min_width_m) * t };

    let to_world = |c: (u32, u32)| -> [f32; 2] {
        [
            (c.0 as f32 + 0.5) * mpc - half,
            (c.1 as f32 + 0.5) * mpc - half,
        ]
    };

    let mut out: Vec<RiverWorldPolyline> = Vec::new();
    let mut current = RiverWorldPolyline::default();

    for (raw, &fraw) in points.iter().zip(flow_raw.iter()) {
        let p = to_world(*raw);
        let fn_v = flow_to_norm(fraw);
        let w_v = norm_to_width(fn_v);
        if let Some(&last) = current.points.last() {
            if (p[0] - last[0]).abs() > half {
                let edge_last = if last[0] > 0.0 { half } else { -half };
                let edge_p = -edge_last;
                // Close current at the seam with the previous vertex's
                // scalar values.
                let last_fn = *current.flow_norm.last().unwrap();
                let last_w = *current.width.last().unwrap();
                let last_bed = *current.bed_floor.last().unwrap();
                current.points.push([edge_last, last[1]]);
                current.flow_norm.push(last_fn);
                current.width.push(last_w);
                current.bed_floor.push(last_bed);
                if current.points.len() >= 2 {
                    out.push(std::mem::take(&mut current));
                } else {
                    current = RiverWorldPolyline::default();
                }
                // Start a fresh polyline on the other side with the new
                // vertex's values.
                current.points.push([edge_p, p[1]]);
                current.flow_norm.push(fn_v);
                current.width.push(w_v);
                current.bed_floor.push(0.0);
            }
        }
        current.points.push(p);
        current.flow_norm.push(fn_v);
        current.width.push(w_v);
        current.bed_floor.push(0.0);
    }

    if current.points.len() >= 2 {
        out.push(current);
    }
    out
}

/// Chaikin corner-cutting for river polylines (points + scalars). Same
/// 25/75 weights as `chaikin_smooth`; scalar arrays are interpolated with
/// identical mix factors so per-vertex width/flow stays aligned with the
/// smoothed geometry.
pub fn river_chaikin_smooth(poly: &RiverWorldPolyline, iterations: u32) -> RiverWorldPolyline {
    assert_eq!(poly.points.len(), poly.flow_norm.len());
    assert_eq!(poly.points.len(), poly.width.len());
    assert_eq!(poly.points.len(), poly.bed_floor.len());
    let mut pts = poly.points.clone();
    let mut fns = poly.flow_norm.clone();
    let mut ws = poly.width.clone();
    let mut beds = poly.bed_floor.clone();
    for _ in 0..iterations {
        if pts.len() < 3 {
            break;
        }
        let mut np: Vec<[f32; 2]> = Vec::with_capacity(pts.len() * 2);
        let mut nf: Vec<f32> = Vec::with_capacity(pts.len() * 2);
        let mut nw: Vec<f32> = Vec::with_capacity(pts.len() * 2);
        let mut nb: Vec<f32> = Vec::with_capacity(pts.len() * 2);
        np.push(pts[0]);
        nf.push(fns[0]);
        nw.push(ws[0]);
        nb.push(beds[0]);
        for i in 0..pts.len() - 1 {
            let a = pts[i];
            let b = pts[i + 1];
            let fa = fns[i];
            let fb = fns[i + 1];
            let wa = ws[i];
            let wb = ws[i + 1];
            let ba = beds[i];
            let bb = beds[i + 1];
            np.push([0.75 * a[0] + 0.25 * b[0], 0.75 * a[1] + 0.25 * b[1]]);
            np.push([0.25 * a[0] + 0.75 * b[0], 0.25 * a[1] + 0.75 * b[1]]);
            nf.push(0.75 * fa + 0.25 * fb);
            nf.push(0.25 * fa + 0.75 * fb);
            nw.push(0.75 * wa + 0.25 * wb);
            nw.push(0.25 * wa + 0.75 * wb);
            nb.push(0.75 * ba + 0.25 * bb);
            nb.push(0.25 * ba + 0.75 * bb);
        }
        np.push(*pts.last().unwrap());
        nf.push(*fns.last().unwrap());
        nw.push(*ws.last().unwrap());
        nb.push(*beds.last().unwrap());
        pts = np;
        fns = nf;
        ws = nw;
        beds = nb;
    }
    RiverWorldPolyline {
        points: pts,
        flow_norm: fns,
        width: ws,
        bed_floor: beds,
    }
}

/// Filter river polylines to the subset of segments whose axis-aligned
/// bounding box intersects the tile bbox expanded by `margin`. Parallels
/// `segments_near_tile` but produces `RiverSegment` values with per-vertex
/// metadata attached.
pub fn river_segments_near_tile(
    polylines: &[RiverWorldPolyline],
    tile_min_x: f32,
    tile_min_z: f32,
    tile_max_x: f32,
    tile_max_z: f32,
    margin: f32,
) -> Vec<RiverSegment> {
    let qx0 = tile_min_x - margin;
    let qx1 = tile_max_x + margin;
    let qz0 = tile_min_z - margin;
    let qz1 = tile_max_z + margin;
    let mut out = Vec::new();
    for poly in polylines {
        if poly.points.len() < 2 {
            continue;
        }
        for i in 0..poly.points.len() - 1 {
            let a = poly.points[i];
            let b = poly.points[i + 1];
            let sx0 = a[0].min(b[0]);
            let sx1 = a[0].max(b[0]);
            let sz0 = a[1].min(b[1]);
            let sz1 = a[1].max(b[1]);
            if sx1 < qx0 || sx0 > qx1 || sz1 < qz0 || sz0 > qz1 {
                continue;
            }
            out.push(RiverSegment {
                ax: a[0],
                az: a[1],
                bx: b[0],
                bz: b[1],
                flow_norm_a: poly.flow_norm[i],
                flow_norm_b: poly.flow_norm[i + 1],
                width_a: poly.width[i],
                width_b: poly.width[i + 1],
                bed_floor_a: poly.bed_floor[i],
                bed_floor_b: poly.bed_floor[i + 1],
            });
        }
    }
    out
}

/// Nearest-river-segment query. Returns `(distance_m, seg_idx, t)` where
/// `t ∈ [0, 1]` is the projection parameter along the nearest segment —
/// the caller uses it to linearly interpolate per-vertex width / flow_norm
/// at the exact closest point. Returns `None` when `segs` is empty.
pub fn nearest_river_segment(px: f32, pz: f32, segs: &[RiverSegment]) -> Option<(f32, usize, f32)> {
    if segs.is_empty() {
        return None;
    }
    let mut best_sq = f32::INFINITY;
    let mut best_idx = 0usize;
    let mut best_t = 0.0f32;
    for (i, s) in segs.iter().enumerate() {
        let (d_sq, t) = project_point_to_segment(px, pz, s.ax, s.az, s.bx, s.bz);
        if d_sq < best_sq {
            best_sq = d_sq;
            best_idx = i;
            best_t = t;
        }
    }
    Some((best_sq.sqrt(), best_idx, best_t))
}

/// Filter `polylines` to the subset of segments whose axis-aligned bounding
/// box intersects the tile bounding box expanded by `margin`. Used to prune
/// the tens of thousands of world-wide segments down to the handful that can
/// influence a single tile.
pub fn segments_near_tile(
    polylines: &[WorldPolyline],
    tile_min_x: f32,
    tile_min_z: f32,
    tile_max_x: f32,
    tile_max_z: f32,
    margin: f32,
) -> Vec<Segment> {
    let qx0 = tile_min_x - margin;
    let qx1 = tile_max_x + margin;
    let qz0 = tile_min_z - margin;
    let qz1 = tile_max_z + margin;
    let mut out = Vec::new();
    for poly in polylines {
        for w in poly.points.windows(2) {
            let a = w[0];
            let b = w[1];
            let sx0 = a[0].min(b[0]);
            let sx1 = a[0].max(b[0]);
            let sz0 = a[1].min(b[1]);
            let sz1 = a[1].max(b[1]);
            if sx1 < qx0 || sx0 > qx1 || sz1 < qz0 || sz0 > qz1 {
                continue;
            }
            out.push([a[0], a[1], b[0], b[1]]);
        }
    }
    out
}

/// Wrap-aware counterpart to `segments_near_tile` for the cylindrical X
/// axis. Each source segment is considered at x offsets `-world_size`, `0`,
/// and `+world_size`; matching segments are returned with that offset already
/// applied, so ordinary Euclidean distance queries measure the shortest
/// periodic distance near the world seam.
///
/// Tile/sample coordinates are expected to stay within one circumference of
/// the canonical world range, as all current bake callers do.
pub fn segments_near_tile_wrap_x(
    polylines: &[WorldPolyline],
    tile_min_x: f32,
    tile_min_z: f32,
    tile_max_x: f32,
    tile_max_z: f32,
    margin: f32,
    world_size: f32,
) -> Vec<Segment> {
    debug_assert!(world_size > 0.0);
    let qx0 = tile_min_x - margin;
    let qx1 = tile_max_x + margin;
    let qz0 = tile_min_z - margin;
    let qz1 = tile_max_z + margin;
    let mut out = Vec::new();
    for poly in polylines {
        for w in poly.points.windows(2) {
            let a = w[0];
            let b = w[1];
            let sz0 = a[1].min(b[1]);
            let sz1 = a[1].max(b[1]);
            if sz1 < qz0 || sz0 > qz1 {
                continue;
            }
            for shift_x in [-world_size, 0.0, world_size] {
                let ax = a[0] + shift_x;
                let bx = b[0] + shift_x;
                let sx0 = ax.min(bx);
                let sx1 = ax.max(bx);
                if sx1 < qx0 || sx0 > qx1 {
                    continue;
                }
                out.push([ax, a[1], bx, b[1]]);
            }
        }
    }
    out
}

/// Project (px, pz) onto segment (ax,az)-(bx,bz) and return `(d_sq, t)`
/// where `t ∈ [0, 1]` is the clamped projection parameter. Shared by the
/// scalar distance helpers and the river-aware `nearest_river_segment`,
/// which needs `t` to interpolate per-vertex scalars at the exact
/// projection point.
#[inline]
pub(crate) fn project_point_to_segment(
    px: f32,
    pz: f32,
    ax: f32,
    az: f32,
    bx: f32,
    bz: f32,
) -> (f32, f32) {
    let dx = bx - ax;
    let dz = bz - az;
    let len_sq = dx * dx + dz * dz;
    let (cx, cz, t) = if len_sq <= 1e-12 {
        (ax, az, 0.0)
    } else {
        let t = (((px - ax) * dx + (pz - az) * dz) / len_sq).clamp(0.0, 1.0);
        (ax + t * dx, az + t * dz, t)
    };
    let ex = px - cx;
    let ez = pz - cz;
    (ex * ex + ez * ez, t)
}

/// Squared Euclidean distance from point (px, pz) to segment (ax,az)-(bx,bz).
#[inline]
pub fn point_segment_distance_sq(px: f32, pz: f32, seg: &Segment) -> f32 {
    project_point_to_segment(px, pz, seg[0], seg[1], seg[2], seg[3]).0
}

/// Euclidean distance from point (px, pz) to segment (ax,az)-(bx,bz).
#[inline]
pub fn point_segment_distance(px: f32, pz: f32, seg: &Segment) -> f32 {
    point_segment_distance_sq(px, pz, seg).sqrt()
}

/// Signed minimum distance from (px, pz) to `segs`. Positive means the
/// point lies on the LEFT of the nearest segment's a→b direction (cross
/// product `dir × (p − a) ≥ 0`). Coast polylines are oriented land-left
/// (see `BakeContext`), so positive = inland, negative = seaward.
///
/// At shared polyline endpoints two adjacent segments tie on distance but
/// can disagree on sign (the corner wedge); ties are resolved toward the
/// candidate whose projection is more perpendicular (larger |cross| per
/// unit length), which gives the wedge a stable, correct side. Returns
/// `None` when `segs` is empty.
pub fn signed_min_distance_to_segments(px: f32, pz: f32, segs: &[Segment]) -> Option<f32> {
    if segs.is_empty() {
        return None;
    }
    // m² slack for "same distance": shared-endpoint ties are exact in
    // arithmetic but drift a hair once the two projections are computed
    // through different segment endpoints.
    const TIE_EPS_SQ: f32 = 1e-3;
    let mut best_sq = f32::INFINITY;
    let mut best_perp = -1.0f32;
    let mut best_sign = 1.0f32;
    for seg in segs {
        let (d_sq, _t) = project_point_to_segment(px, pz, seg[0], seg[1], seg[2], seg[3]);
        if d_sq > best_sq + TIE_EPS_SQ {
            continue;
        }
        let dx = seg[2] - seg[0];
        let dz = seg[3] - seg[1];
        let len_sq = dx * dx + dz * dz;
        // A zero-length segment (duplicate consecutive vertices) has no
        // defined side; its perpendicular would be float noise amplified by
        // the 1/len division and could win the tie-break below and flip the
        // returned sign. Skip it — a real neighbor segment carries the
        // distance. (`orient_coasts_land_left`'s sibling vote skips the same.)
        if len_sq < 1e-12 {
            continue;
        }
        let len = len_sq.sqrt();
        let perp = (dx * (pz - seg[1]) - dz * (px - seg[0])) / len;
        if d_sq < best_sq - TIE_EPS_SQ || perp.abs() > best_perp {
            best_sq = best_sq.min(d_sq);
            best_perp = perp.abs();
            best_sign = if perp >= 0.0 { 1.0 } else { -1.0 };
        }
    }
    Some(best_sign * best_sq.sqrt())
}

/// Minimum distance from (px, pz) to any segment in `segs`, or `f32::INFINITY`
/// when the list is empty. Works in squared space so the hot loop issues one
/// `sqrt` at the end instead of one per segment.
#[inline]
pub fn min_distance_to_segments(px: f32, pz: f32, segs: &[Segment]) -> f32 {
    if segs.is_empty() {
        return f32::INFINITY;
    }
    let mut best_sq = f32::INFINITY;
    for seg in segs {
        let d_sq = point_segment_distance_sq(px, pz, seg);
        if d_sq < best_sq {
            best_sq = d_sq;
        }
    }
    best_sq.sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::config::WorldGenConfig;

    fn test_config(res: u32, world_m: u32) -> WorldGenConfig {
        WorldGenConfig {
            global_res: res,
            world_size_m: world_m,
            reference_res: res,
            ..WorldGenConfig::default()
        }
    }

    #[test]
    fn chaikin_preserves_endpoints() {
        let poly = WorldPolyline {
            points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [20.0, 10.0]],
        };
        let smoothed = chaikin_smooth(&poly, 2);
        assert_eq!(smoothed.points.first().unwrap(), &[0.0, 0.0]);
        assert_eq!(smoothed.points.last().unwrap(), &[20.0, 10.0]);
        assert!(smoothed.points.len() > poly.points.len());
    }

    #[test]
    fn chaikin_smooths_closed_loop_join_cyclically() {
        let raw_vertices = [[0.0, 0.0], [8.0, 0.0], [8.0, 8.0], [0.0, 8.0]];
        let poly = WorldPolyline {
            points: vec![
                raw_vertices[0],
                raw_vertices[1],
                raw_vertices[2],
                raw_vertices[3],
                raw_vertices[0],
            ],
        };

        let smoothed = chaikin_smooth(&poly, 1);

        assert_eq!(smoothed.points.first(), smoothed.points.last());
        assert_eq!(smoothed.points.len(), 9, "8 cut points plus loop closure");
        for raw in raw_vertices {
            assert!(
                !smoothed.points[..smoothed.points.len() - 1].contains(&raw),
                "closed Chaikin must cut raw corner {raw:?}"
            );
        }
    }

    #[test]
    fn chaikin_zero_iterations_preserves_closed_loop() {
        let poly = WorldPolyline {
            points: vec![[0.0, 0.0], [4.0, 0.0], [2.0, 4.0], [0.0, 0.0]],
        };
        assert_eq!(chaikin_smooth(&poly, 0).points, poly.points);
    }

    #[test]
    fn chaikin_grows_geometrically_with_iterations() {
        let poly = WorldPolyline {
            points: vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]],
        };
        let s0 = chaikin_smooth(&poly, 0);
        let s1 = chaikin_smooth(&poly, 1);
        let s2 = chaikin_smooth(&poly, 2);
        assert_eq!(s0.points.len(), 3);
        // One iteration: each of 2 edges splits → 4 interior points + 2 endpoints.
        assert_eq!(s1.points.len(), 6);
        // Two iterations grow roughly ×2 minus endpoint preservation.
        assert!(s2.points.len() >= s1.points.len() * 2 - 4);
    }

    #[test]
    fn chaikin_short_polyline_is_passthrough() {
        // Polylines with < 3 vertices can't be corner-cut; the helper must
        // leave them untouched rather than dropping points.
        let poly = WorldPolyline {
            points: vec![[0.0, 0.0], [10.0, 0.0]],
        };
        let smoothed = chaikin_smooth(&poly, 3);
        assert_eq!(smoothed.points, poly.points);
    }

    #[test]
    fn segment_distance_axis_aligned() {
        let seg: Segment = [0.0, 0.0, 10.0, 0.0];
        assert!((point_segment_distance(5.0, 3.0, &seg) - 3.0).abs() < 1e-4);
        assert!((point_segment_distance(-2.0, 0.0, &seg) - 2.0).abs() < 1e-4);
        assert!((point_segment_distance(12.0, 0.0, &seg) - 2.0).abs() < 1e-4);
    }

    #[test]
    fn segment_distance_degenerate_segment_uses_endpoint() {
        // Zero-length segment collapses to a point; distance is |p - a|.
        let seg: Segment = [3.0, 4.0, 3.0, 4.0];
        assert!((point_segment_distance(0.0, 0.0, &seg) - 5.0).abs() < 1e-4);
    }

    #[test]
    fn point_segment_distance_sq_matches_squared_distance() {
        let seg: Segment = [1.0, 2.0, 7.0, 6.0];
        for &(px, pz) in &[(3.0, 5.0), (-1.0, 2.0), (8.0, 6.0), (10.0, -10.0)] {
            let d = point_segment_distance(px, pz, &seg);
            let d_sq = point_segment_distance_sq(px, pz, &seg);
            assert!((d_sq - d * d).abs() < 1e-4);
        }
    }

    #[test]
    fn min_distance_returns_nearest_of_many_segments() {
        let segs: Vec<Segment> = vec![
            [100.0, 0.0, 110.0, 0.0],
            [0.0, 10.0, 10.0, 10.0],
            [0.0, 0.0, 10.0, 0.0],
        ];
        // (5, 3) is 3m above the third segment; the other two are ≥7m away.
        assert!((min_distance_to_segments(5.0, 3.0, &segs) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn min_distance_empty_list_returns_infinity() {
        assert!(min_distance_to_segments(0.0, 0.0, &[]).is_infinite());
    }

    #[test]
    fn signed_distance_sign_follows_segment_side() {
        // Segment pointing +x: left side is +z.
        let segs: Vec<Segment> = vec![[0.0, 0.0, 10.0, 0.0]];
        let left = signed_min_distance_to_segments(5.0, 3.0, &segs).unwrap();
        let right = signed_min_distance_to_segments(5.0, -3.0, &segs).unwrap();
        assert!((left - 3.0).abs() < 1e-4, "left side positive: {left}");
        assert!((right + 3.0).abs() < 1e-4, "right side negative: {right}");
        assert!(signed_min_distance_to_segments(0.0, 0.0, &[]).is_none());
    }

    #[test]
    fn signed_distance_corner_wedge_takes_stable_side() {
        // Two segments forming an L (both directed so the concave side is
        // left): a→(0,0)→b along +x then turning +z. A point in the convex
        // wedge outside the corner ties on distance to both segments; the
        // sign must resolve to negative (right side of both), not flip
        // depending on iteration order.
        let segs: Vec<Segment> = vec![[-10.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 10.0]];
        let fwd = signed_min_distance_to_segments(2.0, -2.0, &segs).unwrap();
        let rev: Vec<Segment> = segs.iter().rev().copied().collect();
        let bwd = signed_min_distance_to_segments(2.0, -2.0, &rev).unwrap();
        assert!(fwd < 0.0, "convex wedge point must be right-side: {fwd}");
        assert!(
            (fwd - bwd).abs() < 1e-4,
            "sign must not depend on segment order: {fwd} vs {bwd}"
        );
        // And a point clearly inside the L (concave side) reads positive.
        let inside = signed_min_distance_to_segments(-3.0, 2.0, &segs).unwrap();
        assert!(inside > 0.0, "concave side must be left/positive: {inside}");
    }

    #[test]
    fn segments_near_tile_filters_by_bbox() {
        let polys = vec![WorldPolyline {
            points: vec![[0.0, 0.0], [100.0, 0.0]],
        }];
        let near = segments_near_tile(&polys, 10.0, -5.0, 20.0, 5.0, 0.0);
        assert_eq!(near.len(), 1);
        let far = segments_near_tile(&polys, 200.0, 200.0, 210.0, 210.0, 0.0);
        assert!(far.is_empty());
    }

    #[test]
    fn segments_near_tile_respects_margin() {
        // A segment 10m north of the tile bbox is included with margin=15
        // but excluded with margin=5.
        let polys = vec![WorldPolyline {
            points: vec![[-5.0, -10.0], [5.0, -10.0]],
        }];
        assert_eq!(
            segments_near_tile(&polys, 0.0, 0.0, 10.0, 10.0, 15.0).len(),
            1,
            "margin=15m should include a segment 10m outside the tile"
        );
        assert!(
            segments_near_tile(&polys, 0.0, 0.0, 10.0, 10.0, 5.0).is_empty(),
            "margin=5m should exclude a segment 10m outside the tile"
        );
    }

    #[test]
    fn segments_near_tile_wrap_x_returns_translated_periodic_copy() {
        let polys = vec![WorldPolyline {
            // A coast 4 m inside the east edge of a 100 m world.
            points: vec![[46.0, -10.0], [46.0, 10.0]],
        }];
        let west = segments_near_tile_wrap_x(&polys, -50.0, -1.0, -49.0, 1.0, 8.0, 100.0);
        assert_eq!(west, vec![[-54.0, -10.0, -54.0, 10.0]]);

        // The translated segment gives the same signed distance at the two
        // coordinate representations of the seam itself.
        let east = segments_near_tile_wrap_x(&polys, 50.0, 0.0, 50.0, 0.0, 8.0, 100.0);
        let east_sd = signed_min_distance_to_segments(50.0, 0.0, &east).unwrap();
        let west_sd = signed_min_distance_to_segments(-50.0, 0.0, &west).unwrap();
        assert!((east_sd - west_sd).abs() < 1e-6, "{east_sd} vs {west_sd}");
    }

    #[test]
    fn polyline_to_world_converts_cell_centers_to_meters() {
        // 64 m world, 8 cells → 8 m/cell. Cell (0,0) center at world
        // (-32 + 4, -32 + 4) = (-28, -28); cell (4,4) at (+4, +4). The
        // seam-split heuristic assumes adjacent grid cells, so the test
        // walks through adjacent cells to avoid triggering a false split.
        let cfg = test_config(8, 64);
        let points: Vec<(u32, u32)> = vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)];
        let out = polyline_to_world(&points, &cfg, cell_index_to_center);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].points.len(), 5);
        assert!((out[0].points[0][0] - -28.0).abs() < 1e-4);
        assert!((out[0].points[0][1] - -28.0).abs() < 1e-4);
        assert!((out[0].points[4][0] - 4.0).abs() < 1e-4);
        assert!((out[0].points[4][1] - 4.0).abs() < 1e-4);
    }

    #[test]
    fn polyline_to_world_handles_half_integer_vertices() {
        // polyline_to_world applies `c * mpc - half` to whatever cell-coord
        // the mapping returns; half-integer inputs (as coast edge-midpoints
        // produce) must transform without rounding. Identity mapping isolates
        // the transform math: (0.5, 0.0) → 0.5 * 8 - 32 = -28.
        let cfg = test_config(8, 64);
        let points: Vec<[f32; 2]> = vec![[0.5, 0.0], [1.5, 0.0], [1.5, 1.0]];
        let out = polyline_to_world(&points, &cfg, |p: &[f32; 2]| *p);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].points.len(), 3);
        assert!((out[0].points[0][0] - -28.0).abs() < 1e-4);
        assert!((out[0].points[1][0] - -20.0).abs() < 1e-4);
        assert!((out[0].points[2][1] - -24.0).abs() < 1e-4);
    }

    #[test]
    fn cell_node_to_center_aligns_coast_with_cell_frame() {
        // A marching-squares coast vertex on the top edge of the window at
        // mask cell (gx, gy) is emitted at grid-node coord (gx+0.5, gy). After
        // recentering it must sit at (gx+1, gy+0.5) — the midpoint between the
        // cell-center-frame positions of cells (gx, gy) and (gx+1, gy), which
        // is where the bicubic field's zero-contour lives. For gx=gy=0 that is
        // (1.0, 0.5); the same half-cell `cell_index_to_center` adds to an
        // integer index.
        let node = cell_node_to_center(&[0.5, 0.0]);
        assert!((node[0] - 1.0).abs() < 1e-6, "{node:?}");
        assert!((node[1] - 0.5).abs() < 1e-6, "{node:?}");
        // Consistent with the river/road mapping's frame: index (0,0) and the
        // recentered node both add +0.5 to reach the center frame.
        assert_eq!(cell_index_to_center(&(0, 0)), [0.5, 0.5]);
    }

    #[test]
    fn polyline_to_world_splits_across_x_seam() {
        let cfg = test_config(8, 64);
        let half = (cfg.world_size_m as f32) * 0.5;
        let points: Vec<(u32, u32)> = vec![(6, 3), (7, 3), (0, 3), (1, 3)];
        let out = polyline_to_world(&points, &cfg, cell_index_to_center);
        assert_eq!(out.len(), 2, "seam-crossing polyline splits into 2");
        let first_end = out[0].points.last().unwrap();
        assert!((first_end[0] - half).abs() < 1e-3);
        let second_start = out[1].points.first().unwrap();
        assert!((second_start[0] + half).abs() < 1e-3);
    }

    #[test]
    fn polyline_to_world_diagonal_seam_split_shares_crossing_z() {
        let cfg = test_config(8, 64);
        let points: Vec<(u32, u32)> = vec![(7, 2), (0, 3)];
        let out = polyline_to_world(&points, &cfg, cell_index_to_center);
        assert_eq!(out.len(), 2);
        let east = out[0].points.last().unwrap();
        let west = out[1].points.first().unwrap();
        assert!(
            (east[1] - west[1]).abs() < 1e-6,
            "split endpoints must represent one cylindrical point: {east:?} vs {west:?}"
        );
        assert!(
            (east[1] - -8.0).abs() < 1e-6,
            "crossing z must be interpolated halfway: {east:?}"
        );
    }
}
