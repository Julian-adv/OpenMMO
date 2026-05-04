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
use std::collections::{BinaryHeap, HashMap, HashSet};

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

/// Cosine threshold above which two incident edges at the same vertex are
/// considered too parallel to read as distinct roads. The longer one then
/// gets redirected to fork off the closer endpoint instead. cos(20°) ≈
/// 0.94 matches the KNN-extras filter — only catch the most obviously
/// parallel pairs. Wider angles (e.g. 30°) read as proper Y-junctions and
/// shouldn't be collapsed even if a midpoint city happens to be on the
/// way.
const FORK_REDIRECT_MIN_COS: f32 = 0.94;

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

/// Cap on the redirect's detour ratio: redirect (v→far) → (near→far) only
/// when (|v-near| + |near-far|) ≤ this × |v-far|. Without the guard, two
/// settlements at roughly equal distance from a hub get collapsed into a
/// chain even though the "through" route is much longer than the direct
/// road. 1.2× means the chain must be at most 20 % longer — i.e. `near`
/// genuinely sits along the way to `far`, not just somewhere in the
/// general direction.
const FORK_REDIRECT_MAX_DETOUR: f32 = 1.2;

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
                let too_parallel = neighbors[i]
                    .iter()
                    .any(|&k| pair_cos_at(i, j, k, settlements, res_f) > MIN_ANGLE_COS);
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

    // MST has no parallel-fork rejection of its own (it just minimizes total
    // length), so two cities downstream of the same hub can both sit in the
    // hub's adjacency at near-parallel angles. Real road builders would never
    // lay redundant pavement next to an existing trunk; redirect those forks
    // through the closer city so the network reads as a Y-junction.
    redirect_parallel_forks(&mut edge_set, settlements, res_f);

    // Longest edges first so trunks form before branches and later
    // branches snap onto the trunk via `road_mask`.
    let mut edges: Vec<(usize, usize)> = edge_set.into_iter().collect();
    edges.sort_by(|&(a1, b1), &(a2, b2)| {
        let d1 = euclidean_sq(&settlements[a1], &settlements[b1], res_f);
        let d2 = euclidean_sq(&settlements[a2], &settlements[b2], res_f);
        d2.total_cmp(&d1).then((a1, b1).cmp(&(a2, b2)))
    });

    // Pre-allocate A* scratch buffers once and reset per call instead of
    // re-allocating 3× res² vectors for every edge. At 4096² this avoids
    // gigabytes of allocation traffic over the N-edge road loop.
    let res_usize = map.config.global_res as usize;
    let total = res_usize.pow(2);
    let mut scratch = AStarScratch::new(total);
    let river_field = RiverField::from_polylines(&river_map.rivers, res_usize);
    let mut road_mask = vec![0u8; total];
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
            &road_mask,
        ) {
            for &(x, y) in &path {
                road_mask[(y as usize) * res_usize + x as usize] = 1;
            }
            roads.push(Road { points: path });
        }
    }
    RoadNetwork { roads }
}

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

/// Fuse pairs of roads that share an endpoint and run nearly parallel for a
/// long stretch before diverging: replace the follower's prefix with the
/// trunk's cells so they share an identical run up to a Y-fork point.
/// Operates on both polyline ends since A* paths are directionless. Run
/// after `compute_roads`, before `snap_crossings_to_grid`. Bridge dedup,
/// splat min-distance, and along-road village dedup are all idempotent
/// under shared cells, so the merged output flows through the rest of the
/// bake unchanged.
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

/// Identifies which polyline end touches a shared cell (start = index 0,
/// end = index n−1). Lets the merge pass walk a polyline from the shared
/// endpoint outward regardless of which end anchors it.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
enum EndKind {
    Start,
    End,
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
    let dx = fold_x_delta(a.0 as i32 - b.0 as i32, res_i) as f32;
    let dy = a.1 as i32 - b.1 as i32;
    dx * dx + (dy as f32).powi(2)
}

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
            let key = (x as i32 / INTERIOR_MERGE_BIN_CELLS, y as i32 / INTERIOR_MERGE_BIN_CELLS);
            bins.entry(key).or_default().push((ri as u32, pi as u32));
        }
    }

    // Best alignment per (lo, hi) road pair, where lo < hi.
    // Stored as (length, i_lo, i_hi, j_lo, j_hi, j_descending).
    type Alignment = (usize, usize, usize, usize, bool);
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
                            (len_f, (s_lo_f, e_lo_f, s_hi_f, e_hi_f, false))
                        } else {
                            (len_r, (s_lo_r, e_lo_r, j_lo_r, j_hi_r, true))
                        };

                        if best_len < min_len {
                            continue;
                        }

                        covered_lo
                            .entry(pair)
                            .or_default()
                            .push((alignment.0, alignment.1));

                        let entry = best.entry(pair).or_insert((0, (0, 0, 0, 0, false)));
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
    for ((lo, hi), len, (i_lo, i_hi, j_lo, j_hi, j_descending)) in matches {
        if claimed[lo] || claimed[hi] {
            continue;
        }
        if i_lo >= i_hi || j_lo >= j_hi {
            continue;
        }
        let trunk_segment: Vec<(u32, u32)> =
            road_net.roads[lo].points[i_lo..=i_hi].to_vec();
        let segment: Vec<(u32, u32)> = if j_descending {
            trunk_segment.into_iter().rev().collect()
        } else {
            trunk_segment
        };
        road_net.roads[hi].points.splice(j_lo..=j_hi, segment);
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
    /// Cells touched by the previous run, so reset() only revisits them
    /// instead of fill()-ing all res² entries every edge.
    touched: Vec<u32>,
}

impl AStarScratch {
    fn new(total: usize) -> Self {
        Self {
            g_score: vec![f32::INFINITY; total],
            came_from: vec![u32::MAX; total],
            closed: vec![false; total],
            open: BinaryHeap::new(),
            touched: Vec::new(),
        }
    }
    fn reset(&mut self) {
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

fn canonical(e: (usize, usize)) -> (usize, usize) {
    if e.0 < e.1 {
        e
    } else {
        (e.1, e.0)
    }
}

/// Iteratively redirect near-parallel forks: at any vertex where two
/// incident edges (v→a, v→b) leave at less than the FORK_REDIRECT angle,
/// drop the longer one (v→b) and reroute it through the closer endpoint
/// (insert a→b). The graph stays connected because b was reachable through
/// v and is now reachable via v→a→b. Convergence: each redirect either
/// drops a duplicate edge or strictly shortens total edge length (a sits
/// roughly between v and b along the shared direction, so |a-b| < |v-b|),
/// so this terminates after at most O(E) redirects in practice.
fn redirect_parallel_forks(
    edge_set: &mut HashSet<(usize, usize)>,
    settlements: &[Settlement],
    res_f: f32,
) {
    let n = settlements.len();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut sorted_edges: Vec<(usize, usize)> = edge_set.iter().copied().collect();
    sorted_edges.sort_unstable();
    for (a, b) in sorted_edges {
        adj[a].push(b);
        adj[b].push(a);
    }
    let mut redirects = 0usize;
    loop {
        // (v, near, far, cos): drop edge (v, far), insert (near, far).
        let mut redirect: Option<(usize, usize, usize, f32)> = None;
        'scan: for v in 0..n {
            // Tie-break by index so the redirect choice doesn't depend on
            // HashSet iteration order.
            let mut nbrs: Vec<(f32, usize)> = adj[v]
                .iter()
                .map(|&u| (euclidean_sq(&settlements[v], &settlements[u], res_f), u))
                .collect();
            nbrs.sort_by(|x, y| x.0.total_cmp(&y.0).then_with(|| x.1.cmp(&y.1)));
            for i in 0..nbrs.len() {
                for j in (i + 1)..nbrs.len() {
                    let near = nbrs[i].1;
                    let far = nbrs[j].1;
                    let c = pair_cos_at(v, near, far, settlements, res_f);
                    if c <= FORK_REDIRECT_MIN_COS {
                        continue;
                    }
                    let d_v_near = nbrs[i].0.sqrt();
                    let d_v_far = nbrs[j].0.sqrt();
                    let d_near_far =
                        euclidean_sq(&settlements[near], &settlements[far], res_f).sqrt();
                    if (d_v_near + d_near_far) > FORK_REDIRECT_MAX_DETOUR * d_v_far {
                        continue;
                    }
                    redirect = Some((v, near, far, c));
                    break 'scan;
                }
            }
        }
        match redirect {
            None => break,
            Some((v, near, far, _c)) => {
                edge_set.remove(&canonical((v, far)));
                adj[v].retain(|&x| x != far);
                adj[far].retain(|&x| x != v);
                if near != far && edge_set.insert(canonical((near, far))) {
                    adj[near].push(far);
                    adj[far].push(near);
                }
                redirects += 1;
            }
        }
    }
    if redirects > 0 {
        eprintln!("fork-redirect: {redirects} edge(s) redirected");
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

/// Cosine of the angle between the two rays leaving `v` toward `a` and `b`.
/// Wraps the `cos_angle(direction, direction)` pattern that both the
/// KNN-extras filter and the fork-redirect pass share.
fn pair_cos_at(v: usize, a: usize, b: usize, settlements: &[Settlement], res_f: f32) -> f32 {
    cos_angle(
        direction(&settlements[v], &settlements[a], res_f),
        direction(&settlements[v], &settlements[b], res_f),
    )
}

fn a_star(
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
            roads: vec![Road { points: road_pts.clone() }],
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
            roads: vec![Road { points: road_pts.clone() }],
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
                Road { points: a_pts.clone() },
                Road { points: b_pts.clone() },
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
                Road { points: a_pts.clone() },
                Road { points: b_pts.clone() },
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
                Road { points: a_pts.clone() },
                Road { points: b_pts.clone() },
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

    fn s(x: u32, y: u32) -> Settlement {
        Settlement {
            cell_x: x,
            cell_y: y,
            score: 0.0,
        }
    }

    #[test]
    fn fork_redirect_reroutes_parallel_pair_through_closer_city() {
        // v at origin, near at (100, 0), far at (200, 5). v→near and v→far
        // share a near-zero angle (cos ≈ 0.9997 ≫ threshold), so v→far must
        // be redirected to near→far.
        let settlements = [s(0, 0), s(100, 0), s(200, 5)];
        let res_f = 4096.0;
        let mut edge_set: HashSet<(usize, usize)> = HashSet::new();
        edge_set.insert(canonical((0, 1)));
        edge_set.insert(canonical((0, 2)));

        redirect_parallel_forks(&mut edge_set, &settlements, res_f);

        assert!(
            edge_set.contains(&canonical((0, 1))),
            "v→near must remain"
        );
        assert!(
            !edge_set.contains(&canonical((0, 2))),
            "v→far must be removed"
        );
        assert!(
            edge_set.contains(&canonical((1, 2))),
            "near→far must be added"
        );
    }

    #[test]
    fn fork_redirect_keeps_well_separated_edges() {
        // Three edges from v fanning out at 120°. None are parallel —
        // edge_set must be unchanged.
        let settlements = [
            s(2048, 2048),
            s(2148, 2048),                // east
            s(1998, 2048 + 87),           // 120° from east
            s(1998, 2048 - 87),           // 240° from east
        ];
        let res_f = 4096.0;
        let mut edge_set: HashSet<(usize, usize)> = HashSet::new();
        edge_set.insert(canonical((0, 1)));
        edge_set.insert(canonical((0, 2)));
        edge_set.insert(canonical((0, 3)));
        let before = edge_set.clone();

        redirect_parallel_forks(&mut edge_set, &settlements, res_f);

        assert_eq!(edge_set, before);
    }

    #[test]
    fn fork_redirect_chains_through_collinear_cities() {
        // Four collinear cities at 0, 100, 200, 300 — v has direct edges to
        // all three downstream cities. After redirect they should chain
        // 0→1→2→3 instead of fanning from 0.
        let settlements = [s(0, 0), s(100, 0), s(200, 0), s(300, 0)];
        let res_f = 4096.0;
        let mut edge_set: HashSet<(usize, usize)> = HashSet::new();
        edge_set.insert(canonical((0, 1)));
        edge_set.insert(canonical((0, 2)));
        edge_set.insert(canonical((0, 3)));

        redirect_parallel_forks(&mut edge_set, &settlements, res_f);

        assert!(edge_set.contains(&canonical((0, 1))));
        assert!(edge_set.contains(&canonical((1, 2))));
        assert!(edge_set.contains(&canonical((2, 3))));
        assert!(!edge_set.contains(&canonical((0, 2))));
        assert!(!edge_set.contains(&canonical((0, 3))));
    }
}
