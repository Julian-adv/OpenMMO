//! Settlement adjacency graph: the road network's edge skeleton starts as
//! a Prim MST in Euclidean space, gets augmented with each city's K
//! nearest neighbours (parallel-edge filtered), then has near-parallel
//! forks redirected through the closer endpoint so two cities downstream
//! of the same hub chain instead of fanning into redundant pavement.

use std::collections::HashSet;

use super::super::settlements::Settlement;

/// Cosine threshold above which two incident edges at the same vertex are
/// considered too parallel to read as distinct roads. The longer one then
/// gets redirected to fork off the closer endpoint instead. cos(20°) ≈
/// 0.94 matches the KNN-extras filter — only catch the most obviously
/// parallel pairs. Wider angles (e.g. 30°) read as proper Y-junctions and
/// shouldn't be collapsed even if a midpoint city happens to be on the
/// way.
const FORK_REDIRECT_MIN_COS: f32 = 0.94;

/// Cap on the redirect's detour ratio: redirect (v→far) → (near→far) only
/// when (|v-near| + |near-far|) ≤ this × |v-far|. Without the guard, two
/// settlements at roughly equal distance from a hub get collapsed into a
/// chain even though the "through" route is much longer than the direct
/// road. 1.2× means the chain must be at most 20 % longer — i.e. `near`
/// genuinely sits along the way to `far`, not just somewhere in the
/// general direction.
const FORK_REDIRECT_MAX_DETOUR: f32 = 1.2;

pub(super) fn canonical(e: (usize, usize)) -> (usize, usize) {
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
pub(super) fn redirect_parallel_forks(
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
pub(super) fn prim_mst(settlements: &[Settlement], res_f: f32) -> Vec<(usize, usize)> {
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

pub(super) fn euclidean_sq(a: &Settlement, b: &Settlement, res_f: f32) -> f32 {
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
pub(super) fn pair_cos_at(
    v: usize,
    a: usize,
    b: usize,
    settlements: &[Settlement],
    res_f: f32,
) -> f32 {
    cos_angle(
        direction(&settlements[v], &settlements[a], res_f),
        direction(&settlements[v], &settlements[b], res_f),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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

        assert!(edge_set.contains(&canonical((0, 1))), "v→near must remain");
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
            s(2148, 2048),      // east
            s(1998, 2048 + 87), // 120° from east
            s(1998, 2048 - 87), // 240° from east
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
