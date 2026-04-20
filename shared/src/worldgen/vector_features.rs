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
//! the ±world/2 edges. Per-tile query code does not do wrap-aware distance;
//! visible glitches exactly at the seam are acceptable for now.

use super::config::WorldGenConfig;

/// A polyline expressed in world-space meters (x, z).
#[derive(Debug, Clone, Default)]
pub struct WorldPolyline {
    pub points: Vec<[f32; 2]>,
}

/// Convert a cell-index polyline to world-space meters. Segments whose
/// endpoints straddle the X seam are split so each output polyline stays on
/// one side of the seam.
pub fn polyline_to_world(points: &[(u32, u32)], cfg: &WorldGenConfig) -> Vec<WorldPolyline> {
    let mpc = cfg.meters_per_cell();
    let half = cfg.world_size_m as f32 * 0.5;

    let to_world = |p: &(u32, u32)| -> [f32; 2] {
        let x = (p.0 as f32 + 0.5) * mpc - half;
        let z = (p.1 as f32 + 0.5) * mpc - half;
        [x, z]
    };

    let mut out: Vec<WorldPolyline> = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();

    for i in 0..points.len() {
        let p = to_world(&points[i]);
        if let Some(&last) = current.last() {
            // Consecutive global cells differ by at most 1 cell in each axis,
            // so any dx exceeding half the world width must be an X-seam wrap.
            if (p[0] - last[0]).abs() > half {
                let edge_last = if last[0] > 0.0 { half } else { -half };
                let edge_p = -edge_last;
                current.push([edge_last, last[1]]);
                if current.len() >= 2 {
                    out.push(WorldPolyline {
                        points: std::mem::take(&mut current),
                    });
                } else {
                    current.clear();
                }
                current.push([edge_p, p[1]]);
            }
        }
        current.push(p);
    }

    if current.len() >= 2 {
        out.push(WorldPolyline { points: current });
    }
    out
}

/// Chaikin corner-cutting for open polylines. First and last vertices are
/// preserved; every interior edge contributes two points at 25% and 75%.
/// After `iterations` rounds, the line reads as a smooth curve — two rounds
/// are enough for 8 m source spacing to look visually curved at 1 m cells.
pub fn chaikin_smooth(poly: &WorldPolyline, iterations: u32) -> WorldPolyline {
    let mut pts = poly.points.clone();
    for _ in 0..iterations {
        if pts.len() < 3 {
            break;
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
    WorldPolyline { points: pts }
}

/// A single segment expressed as `[ax, az, bx, bz]` in world meters. Flat
/// layout so per-tile segment lists are cache-friendly to scan.
pub type Segment = [f32; 4];

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

/// Squared Euclidean distance from point (px, pz) to segment (ax,az)-(bx,bz).
#[inline]
pub fn point_segment_distance_sq(px: f32, pz: f32, seg: &Segment) -> f32 {
    let ax = seg[0];
    let az = seg[1];
    let dx = seg[2] - ax;
    let dz = seg[3] - az;
    let len_sq = dx * dx + dz * dz;
    let (cx, cz) = if len_sq <= 1e-12 {
        (ax, az)
    } else {
        let t = (((px - ax) * dx + (pz - az) * dz) / len_sq).clamp(0.0, 1.0);
        (ax + t * dx, az + t * dz)
    };
    let ex = px - cx;
    let ez = pz - cz;
    ex * ex + ez * ez
}

/// Euclidean distance from point (px, pz) to segment (ax,az)-(bx,bz).
#[inline]
pub fn point_segment_distance(px: f32, pz: f32, seg: &Segment) -> f32 {
    point_segment_distance_sq(px, pz, seg).sqrt()
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
    fn polyline_to_world_converts_cell_centers_to_meters() {
        // 64 m world, 8 cells → 8 m/cell. Cell (0,0) center at world
        // (-32 + 4, -32 + 4) = (-28, -28); cell (4,4) at (+4, +4). The
        // seam-split heuristic assumes adjacent grid cells, so the test
        // walks through adjacent cells to avoid triggering a false split.
        let cfg = test_config(8, 64);
        let points: Vec<(u32, u32)> = vec![(0, 0), (1, 1), (2, 2), (3, 3), (4, 4)];
        let out = polyline_to_world(&points, &cfg);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].points.len(), 5);
        assert!((out[0].points[0][0] - -28.0).abs() < 1e-4);
        assert!((out[0].points[0][1] - -28.0).abs() < 1e-4);
        assert!((out[0].points[4][0] - 4.0).abs() < 1e-4);
        assert!((out[0].points[4][1] - 4.0).abs() < 1e-4);
    }

    #[test]
    fn polyline_to_world_splits_across_x_seam() {
        // Cell indices that wrap the X seam (from rightmost to leftmost
        // column) should split into two polylines, each terminating at
        // the world's ±half boundary.
        let cfg = test_config(8, 64);
        let half = (cfg.world_size_m as f32) * 0.5;
        let points: Vec<(u32, u32)> = vec![(6, 3), (7, 3), (0, 3), (1, 3)];
        let out = polyline_to_world(&points, &cfg);
        assert_eq!(out.len(), 2, "seam-crossing polyline splits into 2");
        // First half ends at +half (seam nearest the eastern cells).
        let first_end = out[0].points.last().unwrap();
        assert!((first_end[0] - half).abs() < 1e-3);
        // Second half starts at -half.
        let second_start = out[1].points.first().unwrap();
        assert!((second_start[0] + half).abs() < 1e-3);
    }
}
