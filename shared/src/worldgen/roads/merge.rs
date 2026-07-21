//! Two passes that fuse pairs of A* paths whose visual shapes overlap but
//! whose cell sequences differ. Both preserve every road's endpoints —
//! only interior cells get rewritten — so settlement / river-crossing
//! anchors stay put. Run after `compute_roads`, before
//! `snap_crossings_to_grid`. Bridge dedup, splat min-distance, and
//! along-road village dedup are all idempotent under shared cells, so the
//! merged output flows through the rest of the bake unchanged.

use std::collections::HashMap;

use super::super::grid::fold_delta;
use super::RoadNetwork;

/// Cell-distance threshold for treating two road points as "co-located"
/// during the parallel-run merge. ~3 cells (~24 m at the default 8 m/cell
/// scale) keeps the merge tight so only roads that read as the same trunk
/// visually get fused — wider would start eating distinct routes.
const PARALLEL_MERGE_THRESHOLD_CELLS: f32 = 3.0;
/// Minimum number of co-located cells in a row before two roads are merged
/// into a shared trunk. ~30 cells (~240 m) is the "오랜 구간" the player would
/// read as a single road that Y-forks at the end. Below this the divergence
/// happens quickly enough that two separate roads still read correctly.
const PARALLEL_MERGE_MIN_LEN_CELLS: usize = 30;
/// Forward look-ahead in B when scanning for the closest cell to A[i] under
/// monotone matching. Bounds the per-pair cost at O(|A| · LOOKAHEAD); 8 cells
/// covers the realistic step-count drift between two A* paths to the same
/// approximate region (one going diagonal, the other cardinal-heavy).
const PARALLEL_MERGE_LOOKAHEAD: usize = 8;

/// Cell-distance threshold for treating two interior road points as
/// co-located. Slightly looser than the endpoint-anchored threshold because
/// A* paths drift more in the middle than near forced-pass-through cells —
/// roads that share a corridor (same valley, same coastline run) often
/// hover ~4–5 cells apart even when the player perceives them as one
/// trunk.
const INTERIOR_MERGE_THRESHOLD_CELLS: f32 = 5.0;
/// Minimum shared run for the interior pass. Higher than the endpoint
/// version's 30 because mid-polyline splices replace the *middle* of a
/// road, which is a more disruptive edit; we want a clear visual payoff.
const INTERIOR_MERGE_MIN_LEN_CELLS: usize = 60;
/// Bin edge for the spatial hash used to find candidate alignment points.
/// Large enough that any pair of points within `THRESHOLD_CELLS` lands in
/// the same or an adjacent bin.
const INTERIOR_MERGE_BIN_CELLS: i32 = 6;

/// Fuse pairs of roads that share an endpoint and run nearly parallel for a
/// long stretch before diverging: replace the follower's prefix with the
/// trunk's cells so they share an identical run up to a Y-fork point.
/// Operates on both polyline ends since A* paths are directionless.
pub fn merge_parallel_runs(road_net: &mut RoadNetwork, res: usize) {
    if road_net.roads.len() < 2 {
        return;
    }
    let res_i = res as i32;
    let threshold_sq = PARALLEL_MERGE_THRESHOLD_CELLS * PARALLEL_MERGE_THRESHOLD_CELLS;
    let min_len = PARALLEL_MERGE_MIN_LEN_CELLS;

    // Endpoint cell → roads anchored at that cell. A road whose start and
    // end coincide registers once so we never self-pair it.
    let mut by_endpoint: HashMap<u64, Vec<(usize, EndKind)>> = HashMap::new();
    for (idx, road) in road_net.roads.iter().enumerate() {
        let n = road.points.len();
        if n < 2 {
            continue;
        }
        let s = encode_cell(road.points[0]);
        let e = encode_cell(road.points[n - 1]);
        by_endpoint
            .entry(s)
            .or_default()
            .push((idx, EndKind::Start));
        if e != s {
            by_endpoint.entry(e).or_default().push((idx, EndKind::End));
        }
    }

    // Take ownership of the map and sort by key so the merge order is
    // deterministic for a given seed (HashMap iteration order isn't).
    let mut entries: Vec<(u64, Vec<(usize, EndKind)>)> = by_endpoint.into_iter().collect();
    entries.sort_unstable_by_key(|&(k, _)| k);

    for (_k, list) in entries {
        if list.len() < 2 {
            continue;
        }
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                let (ra, ea) = list[i];
                let (rb, eb) = list[j];
                if ra == rb {
                    continue;
                }
                let a_len = road_net.roads[ra].points.len();
                let b_len = road_net.roads[rb].points.len();
                if a_len < min_len + 2 || b_len < min_len + 2 {
                    continue;
                }
                let (i_split, j_split) = match_prefix_lengths(
                    &road_net.roads[ra].points,
                    ea,
                    &road_net.roads[rb].points,
                    eb,
                    threshold_sq,
                    res_i,
                );
                // Reject merges that consume the entire follower (would
                // erase the road instead of Y-forking it) or fall short of
                // the minimum shared length.
                if i_split + 1 < min_len
                    || j_split + 1 < min_len
                    || i_split + 1 >= a_len
                    || j_split + 1 >= b_len
                {
                    continue;
                }
                // Trunk = lower-indexed road for stable, deterministic
                // output across runs. Both prefixes are within threshold,
                // so the visual choice is symmetric.
                let (trunk_idx, trunk_end, trunk_split, follower_idx, follower_end, follower_split) =
                    if ra < rb {
                        (ra, ea, i_split, rb, eb, j_split)
                    } else {
                        (rb, eb, j_split, ra, ea, i_split)
                    };
                // Materialize the trunk's oriented prefix once — the splice
                // mutably borrows follower's vec, so we can't keep a slice
                // view into trunk's points across the call.
                let trunk_prefix =
                    oriented_prefix(&road_net.roads[trunk_idx].points, trunk_end, trunk_split);
                splice_prefix(
                    &mut road_net.roads[follower_idx].points,
                    follower_end,
                    follower_split,
                    trunk_prefix,
                );
            }
        }
    }
}

/// Fuse pairs of roads that DON'T share an endpoint (or whose shared
/// endpoint diverges instantly) but run nearly parallel for a long
/// *interior* stretch — replace the higher-indexed road's matching segment
/// with the lower-indexed road's cells so the rendered network reads as a
/// single trunk with two Y-forks instead of two adjacent ribbons.
///
/// Run after `merge_parallel_runs` so the endpoint-anchored merges already
/// happened. Each road participates in at most one splice per pass to keep
/// the index math sane: a road that's a trunk for match A keeps its
/// geometry, a road that's a follower for match B has its mid-section
/// rewritten, but we never let a road be both in the same pass (cascading
/// edits would invalidate the alignment indices recorded for B).
pub fn merge_parallel_interiors(road_net: &mut RoadNetwork, res: usize) {
    if road_net.roads.len() < 2 {
        return;
    }
    let res_i = res as i32;
    let threshold_sq = INTERIOR_MERGE_THRESHOLD_CELLS * INTERIOR_MERGE_THRESHOLD_CELLS;
    let min_len = INTERIOR_MERGE_MIN_LEN_CELLS;

    // Spatial hash: bin coords → list of (road_idx, point_idx).
    let mut bins: HashMap<(i32, i32), Vec<(u32, u32)>> = HashMap::new();
    for (ri, road) in road_net.roads.iter().enumerate() {
        if road.points.len() < min_len + 1 {
            continue;
        }
        for (pi, &(x, y)) in road.points.iter().enumerate() {
            let key = (
                x as i32 / INTERIOR_MERGE_BIN_CELLS,
                y as i32 / INTERIOR_MERGE_BIN_CELLS,
            );
            bins.entry(key).or_default().push((ri as u32, pi as u32));
        }
    }

    // Best alignment per (lo, hi) road pair, where lo < hi.
    let mut best: HashMap<(usize, usize), (usize, Alignment)> = HashMap::new();
    // Per-pair `i_lo` ranges already discovered by extend_run. Disjoint
    // runs are still found — their seed `i_lo` falls outside every
    // recorded range.
    let mut covered_lo: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();

    let mut bin_keys: Vec<(i32, i32)> = bins.keys().copied().collect();
    bin_keys.sort_unstable();
    // Two parallel polylines within `threshold` cells of each other can
    // straddle a 6-cell bin boundary, so each seed point checks the 3×3
    // bin neighborhood instead of just its own bin. `best.entry((lo, hi))`
    // collapses the duplicate visits a pair gets across overlapping bins.
    for key in &bin_keys {
        let pts = &bins[key];
        for &(ra_u32, ia_u32) in pts {
            let (ra, ia) = (ra_u32 as usize, ia_u32 as usize);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let nbr_key = (key.0 + dx, key.1 + dy);
                    let Some(nbr_pts) = bins.get(&nbr_key) else {
                        continue;
                    };
                    for &(rb_u32, ib_u32) in nbr_pts {
                        let (rb, ib) = (rb_u32 as usize, ib_u32 as usize);
                        if ra >= rb {
                            // Skip self-pairs and the (b, a) ordering of
                            // any pair we'll see (or have seen) as (a, b).
                            continue;
                        }
                        let (lo, lo_idx, hi, hi_idx) = (ra, ia, rb, ib);
                        let pair = (lo, hi);
                        if let Some(ranges) = covered_lo.get(&pair) {
                            if ranges.iter().any(|&(s, e)| lo_idx >= s && lo_idx <= e) {
                                continue;
                            }
                        }
                        let a = &road_net.roads[lo].points;
                        let b = &road_net.roads[hi].points;
                        if shares_endpoint(a, b) {
                            continue;
                        }
                        if cell_dist_sq(a[lo_idx], b[hi_idx], res_i) > threshold_sq {
                            continue;
                        }

                        // Forward alignment (both walked in the same direction).
                        let (e_lo_f, e_hi_f) =
                            extend_run(a, b, lo_idx, hi_idx, 1, 1, res_i, threshold_sq);
                        let (s_lo_f, s_hi_f) =
                            extend_run(a, b, lo_idx, hi_idx, -1, -1, res_i, threshold_sq);
                        let len_f = e_lo_f - s_lo_f;

                        // Reverse alignment (b walked opposite direction).
                        // Forward half walks i↑ / j↓ and lands at
                        // (i_hi, j_lo); backward half walks i↓ / j↑ and
                        // lands at (i_lo, j_hi).
                        let (e_lo_r, j_lo_r) =
                            extend_run(a, b, lo_idx, hi_idx, 1, -1, res_i, threshold_sq);
                        let (s_lo_r, j_hi_r) =
                            extend_run(a, b, lo_idx, hi_idx, -1, 1, res_i, threshold_sq);
                        let len_r = e_lo_r - s_lo_r;

                        let (best_len, alignment) = if len_f >= len_r {
                            (
                                len_f,
                                Alignment {
                                    lo_start: s_lo_f,
                                    lo_end: e_lo_f,
                                    hi_start: s_hi_f,
                                    hi_end: e_hi_f,
                                    hi_descending: false,
                                },
                            )
                        } else {
                            (
                                len_r,
                                Alignment {
                                    lo_start: s_lo_r,
                                    lo_end: e_lo_r,
                                    hi_start: j_lo_r,
                                    hi_end: j_hi_r,
                                    hi_descending: true,
                                },
                            )
                        };

                        if best_len < min_len {
                            continue;
                        }

                        covered_lo
                            .entry(pair)
                            .or_default()
                            .push((alignment.lo_start, alignment.lo_end));

                        let entry = best.entry(pair).or_insert((0, Alignment::default()));
                        if best_len > entry.0 {
                            *entry = (best_len, alignment);
                        }
                    }
                }
            }
        }
    }

    // Apply splices longest-first; tiebreak by pair indices for
    // determinism. Each road participates at most once.
    let mut matches: Vec<((usize, usize), usize, Alignment)> = best
        .into_iter()
        .map(|(pair, (len, a))| (pair, len, a))
        .collect();
    matches.sort_by(|x, y| y.1.cmp(&x.1).then_with(|| x.0.cmp(&y.0)));

    let mut claimed = vec![false; road_net.roads.len()];
    let mut applied = 0usize;
    let mut total_cells = 0usize;
    for ((lo, hi), len, a) in matches {
        if claimed[lo] || claimed[hi] {
            continue;
        }
        if a.lo_start >= a.lo_end || a.hi_start >= a.hi_end {
            continue;
        }
        let trunk_segment: Vec<(u32, u32)> =
            road_net.roads[lo].points[a.lo_start..=a.lo_end].to_vec();
        let segment: Vec<(u32, u32)> = if a.hi_descending {
            trunk_segment.into_iter().rev().collect()
        } else {
            trunk_segment
        };
        road_net.roads[hi]
            .points
            .splice(a.hi_start..=a.hi_end, segment);
        claimed[lo] = true;
        claimed[hi] = true;
        applied += 1;
        total_cells += len;
    }
    eprintln!(
        "  interior parallel merge: {} pairs fused, {} cells of shared trunk",
        applied, total_cells
    );
}

/// Identifies which polyline end touches a shared cell (start = index 0,
/// end = index n−1). Lets the merge pass walk a polyline from the shared
/// endpoint outward regardless of which end anchors it.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum EndKind {
    Start,
    End,
}

/// Matched parallel run between two roads (lo = trunk, hi = follower) for
/// the interior merge pass. Indices are inclusive into each road's
/// `points`. `hi_descending` is set when the alignment was best in the
/// reverse direction — the follower's segment must be reversed before the
/// splice so the seam directions agree.
#[derive(Copy, Clone, Debug, Default)]
struct Alignment {
    lo_start: usize,
    lo_end: usize,
    hi_start: usize,
    hi_end: usize,
    hi_descending: bool,
}

#[inline]
fn encode_cell(c: (u32, u32)) -> u64 {
    ((c.1 as u64) << 32) | (c.0 as u64)
}

/// Cell at oriented-view index `i` for `points` anchored at `from` — index 0
/// is the shared endpoint, increasing indices walk outward.
#[inline]
fn view_get(points: &[(u32, u32)], from: EndKind, i: usize) -> (u32, u32) {
    match from {
        EndKind::Start => points[i],
        EndKind::End => points[points.len() - 1 - i],
    }
}

/// Greedy two-pointer scan: walk both polylines from their shared endpoints
/// and find the largest `(i_split, j_split)` such that the prefixes stay
/// within `threshold_sq` cells of each other under monotone matching. The
/// forward window in `b` is bounded by `PARALLEL_MERGE_LOOKAHEAD` so the
/// scan stays linear even when one A* path takes more cells than the other
/// over the same physical distance.
fn match_prefix_lengths(
    a: &[(u32, u32)],
    a_end: EndKind,
    b: &[(u32, u32)],
    b_end: EndKind,
    threshold_sq: f32,
    res_i: i32,
) -> (usize, usize) {
    let mut j = 0usize;
    let mut last_i = 0usize;
    let mut last_j = 0usize;
    for i in 1..a.len() {
        let p = view_get(a, a_end, i);
        let lookahead_max = (j + PARALLEL_MERGE_LOOKAHEAD + 1).min(b.len());
        let mut best_j = j;
        let mut best_d = f32::INFINITY;
        for k in j..lookahead_max {
            let d = cell_dist_sq(p, view_get(b, b_end, k), res_i);
            if d < best_d {
                best_d = d;
                best_j = k;
            }
        }
        if best_d > threshold_sq {
            break;
        }
        last_i = i;
        last_j = best_j;
        j = best_j;
    }
    (last_i, last_j)
}

#[inline]
fn cell_dist_sq(a: (u32, u32), b: (u32, u32), res_i: i32) -> f32 {
    let dx = fold_delta(a.0 as i32 - b.0 as i32, res_i) as f32;
    let dy = fold_delta(a.1 as i32 - b.1 as i32, res_i) as f32;
    dx * dx + dy * dy
}

/// True if either polyline endpoint of `a` coincides with either endpoint
/// of `b`. Endpoint-shared pairs are handled by `merge_parallel_runs`; the
/// interior pass skips them so it doesn't fight the endpoint splice.
fn shares_endpoint(a: &[(u32, u32)], b: &[(u32, u32)]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let a0 = a[0];
    let a_n = a[a.len() - 1];
    let b0 = b[0];
    let b_n = b[b.len() - 1];
    a0 == b0 || a0 == b_n || a_n == b0 || a_n == b_n
}

/// Walk both polylines from `(i0, j0)` along directions `(di, dj)` (each
/// ±1), greedily aligning a's next cell to its closest match in b within
/// `LOOKAHEAD` steps. Returns the last `(i, j)` that stayed within the
/// distance threshold. Used to extend a candidate alignment found by the
/// spatial bin scan into the longest contiguous parallel run on either
/// side of the seed.
#[allow(clippy::too_many_arguments)]
fn extend_run(
    a: &[(u32, u32)],
    b: &[(u32, u32)],
    i0: usize,
    j0: usize,
    di: i32,
    dj: i32,
    res_i: i32,
    threshold_sq: f32,
) -> (usize, usize) {
    let n_a = a.len() as i32;
    let n_b = b.len() as i32;
    let lookahead = PARALLEL_MERGE_LOOKAHEAD as i32;
    let mut i = i0 as i32;
    let mut j = j0 as i32;
    let mut last_i = i;
    let mut last_j = j;
    loop {
        let ni = i + di;
        if ni < 0 || ni >= n_a {
            break;
        }
        let mut best_j = j;
        let mut best_d = f32::INFINITY;
        for k in 0..=lookahead {
            let cand = j + k * dj;
            if cand < 0 || cand >= n_b {
                break;
            }
            let d = cell_dist_sq(a[ni as usize], b[cand as usize], res_i);
            if d < best_d {
                best_d = d;
                best_j = cand;
            }
        }
        if best_d > threshold_sq {
            break;
        }
        i = ni;
        j = best_j;
        last_i = i;
        last_j = j;
    }
    (last_i as usize, last_j as usize)
}

/// Build the trunk's first `split_idx + 1` cells in oriented-view order
/// (shared endpoint first), as an owned Vec.
fn oriented_prefix(points: &[(u32, u32)], from: EndKind, split_idx: usize) -> Vec<(u32, u32)> {
    let len = split_idx + 1;
    match from {
        EndKind::Start => points[..len].to_vec(),
        EndKind::End => points[points.len() - len..].iter().rev().copied().collect(),
    }
}

/// Replace the follower's prefix `[0..=follower_split_idx]` in oriented-view
/// space with `trunk_prefix` (also oriented, shared cell first). Splices in
/// place; for end-anchored polylines the trunk prefix is reversed so the
/// shared cell lands back on the polyline's tail.
fn splice_prefix(
    follower: &mut Vec<(u32, u32)>,
    follower_end: EndKind,
    follower_split_idx: usize,
    trunk_prefix: Vec<(u32, u32)>,
) {
    if follower.len() < 2 || trunk_prefix.is_empty() {
        return;
    }
    match follower_end {
        EndKind::Start => {
            follower.splice(0..=follower_split_idx, trunk_prefix);
        }
        EndKind::End => {
            let tail_start = follower.len() - 1 - follower_split_idx;
            follower.splice(tail_start.., trunk_prefix.into_iter().rev());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Road;
    use super::*;

    #[test]
    fn merge_y_fork_snaps_follower_prefix_to_trunk() {
        // Two roads share a starting cell, run nearly parallel for ~50 cells
        // (always within 1 cell of each other), then peel apart into
        // distinct ends. The merge should overwrite the second road's
        // prefix with the first road's cells so both polylines share an
        // identical run before Y-forking at the divergence.
        let res = 256usize;
        let shared = (40u32, 40u32);
        // Trunk: walks straight east (40, 40) → (90, 40), then bends south.
        let mut a_pts: Vec<(u32, u32)> = (0..50).map(|i| (40 + i, 40)).collect();
        for k in 1..30 {
            a_pts.push((90, 40 + k));
        }
        // Follower: starts at the shared cell, walks east at y=41 (one cell
        // off the trunk) for 50 cells, then peels north.
        let mut b_pts: Vec<(u32, u32)> = vec![shared];
        for i in 1..50 {
            b_pts.push((40 + i, 41));
        }
        for k in 1..30 {
            b_pts.push((90, 41 - k.min(40)));
        }

        let mut net = RoadNetwork {
            roads: vec![
                Road {
                    points: a_pts.clone(),
                },
                Road {
                    points: b_pts.clone(),
                },
            ],
        };
        merge_parallel_runs(&mut net, res);

        let merged_a = &net.roads[0].points;
        let merged_b = &net.roads[1].points;
        // Trunk untouched.
        assert_eq!(merged_a, &a_pts);
        // Follower's start anchor preserved.
        assert_eq!(merged_b.first(), Some(&shared));
        // Follower now shares some non-trivial number of cells with the
        // trunk's prefix — at least the merge's minimum length.
        let mut shared_run = 0usize;
        while shared_run < merged_a.len()
            && shared_run < merged_b.len()
            && merged_a[shared_run] == merged_b[shared_run]
        {
            shared_run += 1;
        }
        assert!(
            shared_run >= PARALLEL_MERGE_MIN_LEN_CELLS,
            "shared trunk only {shared_run} cells, expected at least {}",
            PARALLEL_MERGE_MIN_LEN_CELLS
        );
        // Follower must still diverge — its tail is the original peel-off,
        // so the last cell shouldn't equal the trunk's last cell.
        assert_ne!(merged_b.last(), merged_a.last());
    }

    #[test]
    fn merge_skipped_when_roads_diverge_immediately() {
        // Two roads share a start but pull apart on the very first step.
        // The merge pass must not snap them — there's no "long run" to fuse.
        let res = 128usize;
        let shared = (20u32, 20u32);
        let a_pts: Vec<(u32, u32)> = std::iter::once(shared)
            .chain((1..40).map(|i| (20 + i, 20)))
            .collect();
        let b_pts: Vec<(u32, u32)> = std::iter::once(shared)
            .chain((1..40).map(|i| (20, 20 + i)))
            .collect();

        let mut net = RoadNetwork {
            roads: vec![
                Road {
                    points: a_pts.clone(),
                },
                Road {
                    points: b_pts.clone(),
                },
            ],
        };
        merge_parallel_runs(&mut net, res);

        // Both polylines unchanged.
        assert_eq!(net.roads[0].points, a_pts);
        assert_eq!(net.roads[1].points, b_pts);
    }

    #[test]
    fn merge_handles_shared_end_cell() {
        // Two roads end at the same cell after running near-parallel for
        // their final stretch. The merge must orient itself from the shared
        // end inward and snap the follower's tail onto the trunk's tail
        // (preserving each road's distinct start).
        let res = 256usize;
        let shared_end = (200u32, 200u32);
        // Trunk arrives from the west: (140..200, 200), entering the shared
        // cell at the end.
        let mut a_pts: Vec<(u32, u32)> = (0..30).map(|k| (200, 170 + k)).collect();
        a_pts.extend((0..50).map(|i| (150 + i, 200)));
        a_pts.push(shared_end);
        // Follower runs at y=201 (one cell off) for the same final stretch.
        let mut b_pts: Vec<(u32, u32)> = (0..30).map(|k| (160 - k, 230 - k)).collect();
        b_pts.extend((0..50).map(|i| (150 + i, 201)));
        b_pts.push(shared_end);

        let original_a_start = a_pts[0];
        let original_b_start = b_pts[0];

        let mut net = RoadNetwork {
            roads: vec![
                Road {
                    points: a_pts.clone(),
                },
                Road {
                    points: b_pts.clone(),
                },
            ],
        };
        merge_parallel_runs(&mut net, res);

        let merged_a = &net.roads[0].points;
        let merged_b = &net.roads[1].points;
        // Each road's distinct start anchor is preserved.
        assert_eq!(merged_a.first(), Some(&original_a_start));
        assert_eq!(merged_b.first(), Some(&original_b_start));
        // Both still arrive at the shared end.
        assert_eq!(merged_a.last(), Some(&shared_end));
        assert_eq!(merged_b.last(), Some(&shared_end));
        // The trailing run is shared cell-for-cell.
        let mut shared_run = 0usize;
        while shared_run < merged_a.len()
            && shared_run < merged_b.len()
            && merged_a[merged_a.len() - 1 - shared_run]
                == merged_b[merged_b.len() - 1 - shared_run]
        {
            shared_run += 1;
        }
        assert!(
            shared_run >= PARALLEL_MERGE_MIN_LEN_CELLS,
            "shared trailing run only {shared_run} cells, expected at least {}",
            PARALLEL_MERGE_MIN_LEN_CELLS
        );
    }
}
