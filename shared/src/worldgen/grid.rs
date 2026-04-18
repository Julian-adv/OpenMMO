//! Small grid-topology helpers shared across worldgen phases.
//!
//! The global map is X-periodic (wraps east-west) but Y is bounded, so all
//! neighborhood operations need this asymmetric treatment. Keeping these
//! helpers in one place avoids subtle divergence between phases.

use std::collections::VecDeque;

/// Multi-source 4-connected BFS over a binary mask, returning the cell-
/// distance from every cell to the nearest source cell. X wraps, Y doesn't.
/// Source cells (where `mask[i] == source_val`) have distance 0. Distances
/// are saturated to `u16::MAX`.
pub(crate) fn bfs_distance_from(mask: &[u8], res: usize, source_val: u8) -> Vec<u16> {
    let total = res * res;
    let mut dist = vec![u16::MAX; total];
    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &m) in mask.iter().enumerate() {
        if m == source_val {
            dist[i] = 0;
            queue.push_back(i);
        }
    }
    while let Some(i) = queue.pop_front() {
        let d = dist[i];
        let nd = d.saturating_add(1);
        let x = i % res;
        let y = i / res;
        let left = if x == 0 { res - 1 } else { x - 1 };
        let right = if x + 1 == res { 0 } else { x + 1 };
        let mut visit = |n: usize| {
            if dist[n] > nd {
                dist[n] = nd;
                queue.push_back(n);
            }
        };
        visit(y * res + left);
        visit(y * res + right);
        if y > 0 {
            visit((y - 1) * res + x);
        }
        if y + 1 < res {
            visit((y + 1) * res + x);
        }
    }
    dist
}
