//! Coast polyline extraction via Marching Squares.
//!
//! Converts the binary `land_mask` raster into vector coastline polylines.
//! Each 2×2 cell window of the land mask is classified into one of 16
//! Marching Squares cases, emitting 0–2 line segments inside the window.
//! Adjacent segments share endpoints exactly (cell-edge midpoints), so
//! chaining them by endpoint yields closed loops. On the torus every
//! contour closes — there is no world border for a chain to terminate at.
//!
//! Output vertex coordinates are in **cell-coord half-integer space** —
//! every vertex sits on the midpoint between two adjacent cell centers, so
//! its X or Y component is always either an integer or `n + 0.5`. Both
//! axes are wrap-aware: a polyline edge whose two endpoints differ by more
//! than `res/2` in X or Y crosses a world seam (downstream consumers
//! handle the seam-split themselves).
//!
//! Saddle cases (5, 10) are resolved as **disjoint** contours — the two
//! diagonally-opposite land cells stay topologically separate. Either
//! interpretation is locally plausible at 8 m cell resolution; "disjoint"
//! avoids accidentally connecting two unrelated landmasses through a single
//! diagonal cell.

use std::collections::HashMap;

/// One coastline segment chain in cell-coord half-integer space.
#[derive(Debug, Clone)]
pub struct CoastPolyline {
    pub points: Vec<[f32; 2]>,
    /// `true` if the chain is a closed loop (e.g. an island), in which case
    /// `points.first() == points.last()`. On the torus every well-formed
    /// contour closes; `false` survives only as a defensive marker for
    /// degenerate input.
    pub closed: bool,
}

#[inline]
fn vkey(vx_2x: i32, vy_2x: i32) -> i64 {
    ((vx_2x as i64) << 32) | (vy_2x as u32 as i64)
}

#[inline]
fn key_to_pt(k: i64) -> [f32; 2] {
    let vx = (k >> 32) as i32;
    let vy = k as i32;
    [vx as f32 * 0.5, vy as f32 * 0.5]
}

fn push_segment(segs: &mut Vec<[i64; 2]>, adj: &mut HashMap<i64, Vec<u32>>, a: i64, b: i64) {
    let idx = segs.len() as u32;
    segs.push([a, b]);
    adj.entry(a).or_default().push(idx);
    adj.entry(b).or_default().push(idx);
}

/// Walk the adjacency graph from `start_key` along `start_seg`, marking each
/// segment visited. Returns the ordered list of vertex keys in the chain.
/// Stops at a vertex with no unvisited incident segments (terminal endpoint
/// or, for closed loops, the start vertex once we've come full circle).
fn trace_chain(
    start_key: i64,
    start_seg: u32,
    segs: &[[i64; 2]],
    adj: &HashMap<i64, Vec<u32>>,
    visited: &mut [bool],
) -> Vec<i64> {
    let mut chain: Vec<i64> = vec![start_key];
    let mut cur_key = start_key;
    let mut cur_seg = start_seg;
    loop {
        visited[cur_seg as usize] = true;
        let [a, b] = segs[cur_seg as usize];
        let next_key = if a == cur_key { b } else { a };
        chain.push(next_key);
        cur_key = next_key;
        let Some(neigh) = adj.get(&cur_key) else {
            break;
        };
        let mut next_seg: Option<u32> = None;
        for &s in neigh {
            if !visited[s as usize] {
                next_seg = Some(s);
                break;
            }
        }
        let Some(s) = next_seg else {
            break;
        };
        cur_seg = s;
    }
    chain
}

/// Extract coastline polylines from a binary land mask.
///
/// `land_mask[y * res + x]` is `1` for land, `0` for sea. Both axes wrap.
/// Returns one `CoastPolyline` per maximal chain of segments; every chain
/// is a closed loop around an island, a continent, or the world itself.
pub fn extract_coasts(land_mask: &[u8], res: usize) -> Vec<CoastPolyline> {
    if res < 2 {
        return Vec::new();
    }
    let two_res = (res as i32) * 2;

    let mut segs: Vec<[i64; 2]> = Vec::new();
    let mut adj: HashMap<i64, Vec<u32>> = HashMap::new();

    for gy in 0..res {
        let gy_i = gy as i32;
        let gy1 = if gy + 1 == res { 0 } else { gy + 1 };
        for gx in 0..res {
            let gx_i = gx as i32;
            let gx1 = if gx + 1 == res { 0 } else { gx + 1 };
            let tl = land_mask[gy * res + gx] & 1;
            let tr = land_mask[gy * res + gx1] & 1;
            let bl = land_mask[gy1 * res + gx] & 1;
            let br = land_mask[gy1 * res + gx1] & 1;
            let case = (tl << 3) | (tr << 2) | (br << 1) | bl;

            // Edge midpoint vertex keys (vx_2x, vy_2x), both axes wrapped.
            let v_t = vkey((gx_i * 2 + 1).rem_euclid(two_res), gy_i * 2);
            let v_r = vkey(
                (gx_i * 2 + 2).rem_euclid(two_res),
                (gy_i * 2 + 1).rem_euclid(two_res),
            );
            let v_b = vkey(
                (gx_i * 2 + 1).rem_euclid(two_res),
                (gy_i * 2 + 2).rem_euclid(two_res),
            );
            let v_l = vkey(
                (gx_i * 2).rem_euclid(two_res),
                (gy_i * 2 + 1).rem_euclid(two_res),
            );

            match case {
                0 | 15 => {}
                1 => push_segment(&mut segs, &mut adj, v_l, v_b),
                2 => push_segment(&mut segs, &mut adj, v_b, v_r),
                3 => push_segment(&mut segs, &mut adj, v_l, v_r),
                4 => push_segment(&mut segs, &mut adj, v_t, v_r),
                5 => {
                    // Saddle (TR + BL land): two disjoint contours.
                    push_segment(&mut segs, &mut adj, v_l, v_b);
                    push_segment(&mut segs, &mut adj, v_t, v_r);
                }
                6 => push_segment(&mut segs, &mut adj, v_t, v_b),
                7 => push_segment(&mut segs, &mut adj, v_t, v_l),
                8 => push_segment(&mut segs, &mut adj, v_t, v_l),
                9 => push_segment(&mut segs, &mut adj, v_t, v_b),
                10 => {
                    // Saddle (TL + BR land): two disjoint contours.
                    push_segment(&mut segs, &mut adj, v_t, v_l);
                    push_segment(&mut segs, &mut adj, v_b, v_r);
                }
                11 => push_segment(&mut segs, &mut adj, v_t, v_r),
                12 => push_segment(&mut segs, &mut adj, v_l, v_r),
                13 => push_segment(&mut segs, &mut adj, v_b, v_r),
                14 => push_segment(&mut segs, &mut adj, v_l, v_b),
                _ => unreachable!(),
            }
        }
    }

    let mut visited = vec![false; segs.len()];
    let mut out: Vec<CoastPolyline> = Vec::new();

    // 1) Open chains first: start at any vertex with valence 1. On the
    // torus this shouldn't occur (every contour closes), but the pass stays
    // as a cheap defensive net for degenerate input. Sort the start keys
    // for determinism — HashMap iteration is otherwise hash-randomized.
    let mut open_starts: Vec<i64> = adj
        .iter()
        .filter_map(|(&k, v)| if v.len() == 1 { Some(k) } else { None })
        .collect();
    open_starts.sort();
    for start_key in open_starts {
        let Some(neigh) = adj.get(&start_key) else {
            continue;
        };
        let Some(&start_seg) = neigh.iter().find(|&&s| !visited[s as usize]) else {
            continue;
        };
        let chain = trace_chain(start_key, start_seg, &segs, &adj, &mut visited);
        if chain.len() >= 2 {
            out.push(CoastPolyline {
                points: chain.iter().map(|&k| key_to_pt(k)).collect(),
                closed: false,
            });
        }
    }

    // 2) Closed loops: any remaining unvisited segment starts one. Iterate
    // segments in index order (deterministic) rather than via HashMap.
    for s in 0..segs.len() {
        if visited[s] {
            continue;
        }
        let [a, _b] = segs[s];
        let chain = trace_chain(a, s as u32, &segs, &adj, &mut visited);
        if chain.len() >= 2 {
            out.push(CoastPolyline {
                points: chain.iter().map(|&k| key_to_pt(k)).collect(),
                closed: true,
            });
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4×4 mask with a 2×2 land block in the middle:
    ///   . . . .
    ///   . # # .
    ///   . # # .
    ///   . . . .
    /// Expected: one closed loop of length 8 segments around the block.
    #[test]
    fn small_island_closes_into_loop() {
        let res = 4;
        #[rustfmt::skip]
        let mask: Vec<u8> = vec![
            0,0,0,0,
            0,1,1,0,
            0,1,1,0,
            0,0,0,0,
        ];
        let coasts = extract_coasts(&mask, res);
        assert_eq!(coasts.len(), 1, "expected exactly one coast loop");
        let poly = &coasts[0];
        assert!(poly.closed, "block-island contour should be closed");
        // Closed chain repeats start vertex at end → 9 vertices for an 8-seg loop.
        assert_eq!(poly.points.len(), 9, "expected 8 segments around 2×2 block");
        assert_eq!(
            poly.points.first(),
            poly.points.last(),
            "closed loop must start == end"
        );
    }

    /// A block touching row res-1 wraps around to row 0's sea and closes:
    ///   . . . .
    ///   . . . .
    ///   . # # .
    ///   . # # .   <-- bottom row wraps to the top row's sea
    #[test]
    fn block_at_y_seam_still_closes() {
        let res = 4;
        #[rustfmt::skip]
        let mask: Vec<u8> = vec![
            0,0,0,0,
            0,0,0,0,
            0,1,1,0,
            0,1,1,0,
        ];
        let coasts = extract_coasts(&mask, res);
        assert_eq!(coasts.len(), 1);
        let poly = &coasts[0];
        assert!(
            poly.closed,
            "block at the Y seam must close across the wrap"
        );
        assert_eq!(
            poly.points.first(),
            poly.points.last(),
            "closed loop must start == end"
        );
    }

    /// Y-wrap: a vertical land strip wrapping the seam should produce two
    /// closed loops (left and right edges of the strip), each going around
    /// the world in Y.
    #[test]
    fn y_wrap_strip_closes_around_world() {
        let res = 4;
        // Middle two columns are all land; left and right columns all sea.
        #[rustfmt::skip]
        let mask: Vec<u8> = vec![
            0,1,1,0,
            0,1,1,0,
            0,1,1,0,
            0,1,1,0,
        ];
        let coasts = extract_coasts(&mask, res);
        assert_eq!(coasts.len(), 2, "vertical strip → 2 closed loops");
        for poly in &coasts {
            assert!(poly.closed);
            // 4 segments around the world (one per Y cell), closed → 5 points.
            assert_eq!(poly.points.len(), 5);
        }
    }

    /// X-wrap: a horizontal land strip wrapping the seam should produce two
    /// closed loops (top and bottom edges of the strip), each a closed loop
    /// going around the world in X.
    #[test]
    fn x_wrap_strip_closes_around_world() {
        let res = 4;
        // Middle two rows are all land; top and bottom rows all sea.
        #[rustfmt::skip]
        let mask: Vec<u8> = vec![
            0,0,0,0,
            1,1,1,1,
            1,1,1,1,
            0,0,0,0,
        ];
        let coasts = extract_coasts(&mask, res);
        assert_eq!(coasts.len(), 2, "horizontal strip → 2 closed loops");
        for poly in &coasts {
            assert!(poly.closed);
            // 4 segments around the world (one per X cell), closed → 5 points.
            assert_eq!(poly.points.len(), 5);
        }
    }

    #[test]
    fn empty_mask_produces_no_coasts() {
        let res = 8;
        let mask = vec![0u8; res * res];
        assert!(extract_coasts(&mask, res).is_empty());

        let mask_all_land = vec![1u8; res * res];
        assert!(extract_coasts(&mask_all_land, res).is_empty());
    }

    #[test]
    fn deterministic_for_same_input() {
        // Diagonally-asymmetric mask to exercise both the open-chain and
        // closed-loop branches plus a saddle cell.
        let res = 6;
        #[rustfmt::skip]
        let mask: Vec<u8> = vec![
            1,0,0,0,1,0,
            0,1,0,1,0,0,
            0,0,1,0,0,0,
            0,1,0,0,1,1,
            0,0,1,0,1,0,
            1,0,0,1,0,0,
        ];
        let a = extract_coasts(&mask, res);
        let b = extract_coasts(&mask, res);
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.points, pb.points);
            assert_eq!(pa.closed, pb.closed);
        }
    }
}
