//! Small grid-topology helpers shared across worldgen phases.
//!
//! The global map is X-periodic (wraps east-west) but Y is bounded, so all
//! neighborhood operations need this asymmetric treatment. Keeping these
//! helpers in one place avoids subtle divergence between phases.

use std::cmp::Ordering;
use std::collections::VecDeque;

/// Min-heap entry for f32 priorities in `BinaryHeap`. Ordering uses
/// `f32::total_cmp` (NaN triggers the usual total-order rules), then
/// secondary ordering by the u32 tag for full determinism. Reverses the
/// comparison so the default max-heap pops the lowest priority first.
#[derive(Copy, Clone, PartialEq)]
pub(crate) struct MinF32(pub f32, pub u32);

impl Eq for MinF32 {}
impl Ord for MinF32 {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .0
            .total_cmp(&self.0)
            .then_with(|| other.1.cmp(&self.1))
    }
}
impl PartialOrd for MinF32 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

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
