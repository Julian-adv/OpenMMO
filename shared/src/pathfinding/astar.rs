//! Two-ended A* search over the virtual 1m world grid with floor-level awareness.
//! Combines two cell types: regular floor cells (cardinal expansion via
//! the cell-edge mask) and stairwell intermediate cells (axis-only
//! expansion via the precomputed stair map). The goal is reachable at
//! any floor key on the goal floor — A* doesn't have to walk back to a
//! landing first if the goal cell is mid-stairwell.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};

use super::query::{
    is_cardinal_move_blocked, is_doorway, is_movement_blocked_for_mover, snap_goal_into_floor,
};
use super::stair::{
    build_stair_cells, floor_to_key, is_regular_key, key_to_floor, AStarKey, StairCellExpansion,
};
use super::{PassabilityCache, PathResult, PathWaypoint, DIRS};
use crate::world::wrap_world_x;

/// Default max nodes for each direction of the search. Sized for multi-floor
/// house traversal.
pub const DEFAULT_MAX_NODES: usize = 2000;
/// Largest sealed pocket around a goal whose rim the search still aims for:
/// room for a shut building, not for a dungeon wing.
const SEALED_MAX_CELLS: usize = 1024;
/// With the goal proven out of reach, the walker stops where this many more
/// nodes bring it no closer.
const UNREACHABLE_STALL_NODES: usize = 256;
/// A short search still gets this long after its sides meet to find the
/// shortest join.
const MIN_NODES_PAST_MEETING: usize = 64;

#[derive(Clone)]
struct AStarNode {
    x: i32,
    z: i32,
    /// Floor key: regular floors are multiples of FLOOR_SCALE,
    /// intermediate stairwell cells use values in between.
    fk: u16,
    g: u32,
    f: u32,
}

impl Eq for AStarNode {}
impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}
impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        self.f.cmp(&other.f)
    }
}
impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct ClosedEntry {
    g: u32,
    parent: AStarKey,
}

/// Cells a search read; a cached failure stays valid while geometry around
/// this box is unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SearchExtent {
    pub(super) expanded: usize,
    pub min: (i32, i32),
    pub max: (i32, i32),
    floors: [u64; 4],
}

impl SearchExtent {
    fn at(x: i32, z: i32, floor: u8) -> Self {
        let mut extent = Self {
            min: (x, z),
            max: (x, z),
            ..Self::default()
        };
        extent.include(x, z, floor);
        extent
    }

    fn include(&mut self, x: i32, z: i32, floor: u8) {
        self.min = (self.min.0.min(x), self.min.1.min(z));
        self.max = (self.max.0.max(x), self.max.1.max(z));
        self.floors[usize::from(floor / 64)] |= 1 << (floor % 64);
    }

    pub fn has_floor(&self, floor: u8) -> bool {
        self.floors[usize::from(floor / 64)] & (1 << (floor % 64)) != 0
    }
}

/// Find a path on a virtual 1m world grid with floor-level awareness.
///
/// `start_floor`/`goal_floor` are floor *keys*, not "the floor nearest that Y":
/// an endpoint standing mid-stairwell belongs to the shallower connected floor
/// here. Resolve endpoints with [`super::start_floor_at`] rather than
/// [`super::get_floor_at_position`], or the search reaches the stairs only
/// through their far landing.
#[allow(clippy::too_many_arguments)]
pub fn find_path(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
) -> PathResult {
    find_path_avoiding(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes,
        &[],
    )
}

/// Whether the 1m cell `(x, z)`, in any periodic frame, is in `blocked`.
pub(super) fn cell_blocked(blocked: &[(i32, i32)], x: i32, z: i32) -> bool {
    !blocked.is_empty() && blocked.contains(&(wrap_world_x(x as f32 + 0.5).floor() as i32, z))
}

/// Whether a mover walking `(x0, z0)`→`(x1, z1)` enters any of `cells`
/// (canonical, see `monster_ai::cell_of`). Sampled at quarter-cell steps so
/// no 1m cell is skipped; the start cell is exempt. The chase gate, the
/// occupied-leg check and the smoother all use this one sweep so they agree
/// on which cells a leg touches.
pub fn segment_enters_cells(x0: f32, z0: f32, x1: f32, z1: f32, cells: &[(i32, i32)]) -> bool {
    if cells.is_empty() {
        return false;
    }
    let dx = crate::world::shortest_world_delta_x(x0, x1);
    let dz = z1 - z0;
    let steps = ((dx * dx + dz * dz).sqrt() / 0.25).ceil().max(1.0) as i32;
    let start = (x0.floor() as i32, z0.floor() as i32);
    (1..=steps).any(|i| {
        let t = i as f32 / steps as f32;
        let cell = ((x0 + dx * t).floor() as i32, (z0 + dz * t).floor() as i32);
        cell != start && cell_blocked(cells, cell.0, cell.1)
    })
}

/// [`find_path`] that also refuses to enter `blocked` cells — standing
/// monsters, so a chase can detour around a queue instead of waiting in it.
#[allow(clippy::too_many_arguments)]
pub fn find_path_avoiding(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
    blocked: &[(i32, i32)],
) -> PathResult {
    find_path_avoiding_counted(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes,
        max_nodes.saturating_mul(2),
        blocked,
        &mut SearchExtent::default(),
    )
}

/// One direction of the search: its frontier and the best cost found per key.
#[derive(Default)]
struct Side {
    open: BinaryHeap<Reverse<AStarNode>>,
    seen: HashMap<AStarKey, ClosedEntry>,
    expanded: usize,
}

impl Side {
    fn min_f(&self) -> Option<u32> {
        self.open.peek().map(|Reverse(node)| node.f)
    }

    /// Record `key` at cost `g` unless a cheaper way to it is already known.
    fn reach(&mut self, key: AStarKey, parent: AStarKey, g: u32, h: u32) -> bool {
        if self.seen.get(&key).is_some_and(|known| known.g <= g) {
            return false;
        }
        self.seen.insert(key, ClosedEntry { g, parent });
        self.open.push(Reverse(AStarNode {
            x: key.0,
            z: key.1,
            fk: key.2,
            g,
            f: g + h,
        }));
        true
    }

    /// Take the next node to expand, charged to `extent`; `None` for one a
    /// cheaper way has since replaced.
    fn pop(&mut self, extent: &mut SearchExtent) -> Option<(AStarKey, u32)> {
        let Reverse(node) = self.open.pop()?;
        self.expanded += 1;
        extent.expanded += 1;
        extent.include(node.x, node.z, key_to_floor(node.fk));
        let key = (node.x, node.z, node.fk);
        (self.seen.get(&key)?.g >= node.g).then_some((key, node.g))
    }
}

/// The moves of one search, read in either direction.
struct Moves<'a> {
    cache: &'a PassabilityCache,
    stair_cells: HashMap<AStarKey, StairCellExpansion>,
    stair_positions: HashSet<(i32, i32, u8)>,
    blocked: &'a [(i32, i32)],
    confine_to_floor: bool,
    start_key: AStarKey,
}

impl<'a> Moves<'a> {
    fn new(
        cache: &'a PassabilityCache,
        start_key: AStarKey,
        goal_key: AStarKey,
        blocked: &'a [(i32, i32)],
    ) -> Self {
        let stair_cells = build_stair_cells(cache);
        let stair_positions = stair_cells
            .keys()
            .map(|&(x, z, fk)| (x, z, key_to_floor(fk)))
            .collect();
        let mut moves = Self {
            cache,
            stair_cells,
            stair_positions,
            blocked,
            confine_to_floor: false,
            start_key,
        };
        // A same-floor goal never needs the stairs, unless either end stands
        // in a stairwell: mid-stair cells are keyed to the shallower floor.
        moves.confine_to_floor = start_key.2 == goal_key.2
            && !moves.stair_interior(start_key)
            && !moves.on_stairs(goal_key);
        moves
    }

    fn on_stairs(&self, key: AStarKey) -> bool {
        self.stair_positions
            .contains(&(key.0, key.1, key_to_floor(key.2)))
    }

    /// A stairwell cell that is not a landing at this key.
    fn stair_interior(&self, key: AStarKey) -> bool {
        self.on_stairs(key) && !self.stair_cells.contains_key(&key)
    }

    /// Whether a mover may come to stand here. Nobody enters the start cell,
    /// so it is never refused.
    fn enterable(&self, x: i32, z: i32) -> bool {
        (x, z) == (self.start_key.0, self.start_key.1) || !cell_blocked(self.blocked, x, z)
    }

    /// A mover parked on furniture steps out of its sealed start cell the way
    /// the movement validator lets it; walls never yield.
    fn step_open(&self, from: AStarKey, dx: i32, dz: i32) -> bool {
        let floor = key_to_floor(from.2);
        !is_cardinal_move_blocked(self.cache, from.0, from.1, dx, dz, floor)
            || (from == self.start_key && self.steps_out(from.0, from.1, dx, dz, floor))
    }

    fn steps_out(&self, x: i32, z: i32, dx: i32, dz: i32, floor: u8) -> bool {
        let (cx, cz) = (x as f32 + 0.5, z as f32 + 0.5);
        let (tx, tz) = (cx + dx as f32, cz + dz as f32);
        !is_movement_blocked_for_mover(self.cache, cx, cz, tx, tz, floor, None)
    }

    /// Keys one move after `cur`, or with `back` the keys one move before it.
    fn neighbors(&self, cur: AStarKey, back: bool, out: &mut Vec<AStarKey>) {
        out.clear();
        if !self.enterable(cur.0, cur.1) {
            return;
        }
        if is_regular_key(cur.2) && !self.stair_interior(cur) {
            for &(dx, dz) in &DIRS {
                let other = if back {
                    (cur.0 - dx, cur.1 - dz, cur.2)
                } else {
                    (cur.0 + dx, cur.1 + dz, cur.2)
                };
                let from = if back { other } else { cur };
                if self.enterable(other.0, other.1)
                    && !self.stair_interior(other)
                    && self.step_open(from, dx, dz)
                {
                    out.push(other);
                }
            }
        }
        if !self.confine_to_floor {
            // Stair links are mutual, so they read the same both ways.
            if let Some(stair) = self.stair_cells.get(&cur) {
                for link in [&stair.prev, &stair.next].into_iter().flatten() {
                    if self.enterable(link.x, link.z) {
                        out.push((link.x, link.z, link.fk));
                    }
                }
            }
        }
    }

    /// Where a walker stands to be at a sealed `pocket`: beside furniture,
    /// which seals only its own cell, else at its doors, else anywhere along
    /// the wall. Doors are all tried at once, so the nearest one to walk to wins.
    fn rim_of(&self, pocket: &HashSet<(i32, i32)>, fk: u16) -> (Vec<AStarKey>, bool) {
        let floor = key_to_floor(fk);
        let (mut beside, mut doors, mut along) = (Vec::new(), Vec::new(), Vec::new());
        for &(x, z) in pocket {
            for &(dx, dz) in &DIRS {
                let cell = (x + dx, z + dz, fk);
                if pocket.contains(&(cell.0, cell.1))
                    || !self.enterable(cell.0, cell.1)
                    || self.stair_interior(cell)
                {
                    continue;
                }
                if self.steps_out(x, z, dx, dz, floor) {
                    beside.push(cell);
                } else if is_doorway(self.cache, x, z, dx, dz, floor) {
                    doors.push(cell);
                } else {
                    along.push(cell);
                }
            }
        }
        if !beside.is_empty() {
            (beside, false)
        } else if !doors.is_empty() {
            (doors, true)
        } else {
            (along, false)
        }
    }
}

/// Searches from the start and from the goal at once and joins them where they
/// meet. `max_nodes` bounds each direction, `total_nodes` the two together.
/// A goal nobody can stand on ends the path at the nearest cell of its rim.
#[allow(clippy::too_many_arguments)]
pub(super) fn find_path_avoiding_counted(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
    total_nodes: usize,
    blocked: &[(i32, i32)],
    extent: &mut SearchExtent,
) -> PathResult {
    use crate::pathfinding::PathTermination;

    let (goal_x, goal_z) = snap_goal_into_floor(cache, goal_x, goal_z, goal_floor);
    let start_key: AStarKey = (
        start_x.floor() as i32,
        start_z.floor() as i32,
        floor_to_key(start_floor),
    );
    let goal_key: AStarKey = (
        goal_x.floor() as i32,
        goal_z.floor() as i32,
        floor_to_key(goal_floor),
    );
    *extent = SearchExtent::at(start_key.0, start_key.1, start_floor);
    extent.include(goal_key.0, goal_key.1, goal_floor);

    if start_key == goal_key {
        return PathResult {
            waypoints: vec![PathWaypoint {
                x: goal_x,
                z: goal_z,
                floor: goal_floor,
            }],
            found: true,
            termination: PathTermination::Reached,
        };
    }

    let moves = Moves::new(cache, start_key, goal_key, blocked);
    let distance = |a: AStarKey, b: AStarKey| -> u32 {
        (a.0 - b.0).unsigned_abs()
            + (a.1 - b.1).unsigned_abs()
            + (i32::from(key_to_floor(a.2)) - i32::from(key_to_floor(b.2))).unsigned_abs() * 2
    };
    let to_goal = |key: AStarKey| distance(key, goal_key);
    let to_start = |key: AStarKey| distance(key, start_key);

    let mut forward = Side::default();
    let mut backward = Side::default();
    forward.reach(start_key, start_key, 0, to_goal(start_key));
    backward.reach(goal_key, goal_key, 0, to_start(goal_key));
    // A stair cell holds a key per step, and either end may stand on any of them.
    if !moves.confine_to_floor {
        for &key in moves.stair_cells.keys() {
            if (key.0, key.1) == (start_key.0, start_key.1) && key_to_floor(key.2) == start_floor {
                forward.reach(key, key, 0, to_goal(key));
            }
            if (key.0, key.1) == (goal_key.0, goal_key.1) && key_to_floor(key.2) == goal_floor {
                backward.reach(key, key, 0, to_start(key));
            }
        }
    }

    // Cheapest known join of the two sides, and the nodes spent finding the first.
    let mut meeting: Option<(u32, AStarKey)> = None;
    let mut met_at: Option<usize> = None;
    let join = |meeting: &mut Option<(u32, AStarKey)>, key: AStarKey, cost: u32| {
        if meeting.is_none_or(|(best, _)| cost < best) {
            *meeting = Some((cost, key));
        }
    };
    // The start side's nearest key to the goal, and its nodes spent finding it.
    let mut closest = (to_goal(start_key), start_key);
    let mut closest_at = 0;
    // The goal's sealed pocket once its side has run dry, the rim cells not yet
    // tried (farthest from the goal first) and how far the ones being tried stand.
    let mut pocket: HashSet<(i32, i32)> = HashSet::new();
    let mut rim: Vec<AStarKey> = Vec::new();
    let mut rim_distance = 0;
    let mut rim_together = false;
    let mut backward_done = false;
    let mut neighbors = Vec::with_capacity(6);

    let termination = loop {
        if backward.open.is_empty() && !backward_done && meeting.is_none() {
            pocket.extend(backward.seen.keys().map(|key| (key.0, key.1)));
            backward.seen.clear();
            if rim.is_empty() && moves.confine_to_floor && pocket.len() <= SEALED_MAX_CELLS {
                (rim, rim_together) = moves.rim_of(&pocket, goal_key.2);
                rim.sort_unstable_by_key(|&cell| Reverse((to_goal(cell), cell)));
                rim.dedup();
            }
            rim.retain(|cell| !pocket.contains(&(cell.0, cell.1)));
            match (rim.first(), rim.last()) {
                (Some(&farthest), Some(&nearest)) => {
                    let tried = if rim_together { farthest } else { nearest };
                    rim_distance = to_goal(tried);
                    while let Some(cell) =
                        rim.pop_if(|cell| rim_together || to_goal(*cell) == rim_distance)
                    {
                        extent.include(cell.0, cell.1, goal_floor);
                        backward.reach(cell, cell, 0, to_start(cell));
                        if let Some(from_start) = forward.seen.get(&cell) {
                            join(&mut meeting, cell, from_start.g);
                        }
                    }
                }
                _ => backward_done = true,
            }
        }

        let spent = extent.expanded >= total_nodes;
        let forward_live = !spent && !forward.open.is_empty() && forward.expanded < max_nodes;
        let backward_live = !spent && !backward.open.is_empty() && backward.expanded < max_nodes;
        if let Some((cost, _)) = meeting {
            // No cheaper join remains once either frontier's best is no better.
            // The start side estimates to the goal, `rim_distance` past the rim.
            let bound = |live: bool, side: &Side, slack: u32| match side.min_f() {
                Some(f) if live => f.saturating_sub(slack),
                _ => u32::MAX,
            };
            let ahead = bound(forward_live, &forward, rim_distance);
            // Proving a join shortest costs as much as a one-sided search, so
            // look only as long again for a shorter one.
            let met = *met_at.get_or_insert(extent.expanded);
            if ahead.max(bound(backward_live, &backward, 0)) >= cost
                || extent.expanded - met >= met.max(MIN_NODES_PAST_MEETING)
            {
                break PathTermination::Reached;
            }
        }
        // A start side run dry has seen every cell the walker can reach.
        if forward.open.is_empty() {
            break PathTermination::Unreachable;
        }
        if !forward_live && !backward_live {
            break PathTermination::NodeLimit;
        }
        if backward_done
            && meeting.is_none()
            && forward.expanded - closest_at >= UNREACHABLE_STALL_NODES
        {
            break PathTermination::Unreachable;
        }

        if forward_live && (!backward_live || forward.open.len() <= backward.open.len()) {
            let Some((cur, g)) = forward.pop(extent) else {
                continue;
            };
            moves.neighbors(cur, false, &mut neighbors);
            for &next in &neighbors {
                let h = to_goal(next);
                if !forward.reach(next, cur, g + 1, h) {
                    continue;
                }
                if h < closest.0 {
                    closest = (h, next);
                    closest_at = forward.expanded;
                }
                if let Some(to_end) = backward.seen.get(&next) {
                    join(&mut meeting, next, g + 1 + to_end.g);
                }
            }
        } else {
            let Some((cur, g)) = backward.pop(extent) else {
                continue;
            };
            moves.neighbors(cur, true, &mut neighbors);
            for &prev in &neighbors {
                if !backward.reach(prev, cur, g + 1, to_start(prev)) {
                    continue;
                }
                if let Some(from_start) = forward.seen.get(&prev) {
                    join(&mut meeting, prev, from_start.g + g + 1);
                }
            }
        }
    };

    let Some((_, key)) = meeting else {
        let mut keys = chain(&forward.seen, closest.1);
        keys.reverse();
        return PathResult {
            waypoints: waypoints_along(&keys, None),
            found: false,
            termination,
        };
    };
    let mut keys = chain(&forward.seen, key);
    keys.reverse();
    keys.extend(chain(&backward.seen, key).into_iter().skip(1));
    // A join found around a sealed pocket ends at its rim, not at the goal.
    let reached = pocket.is_empty();
    PathResult {
        waypoints: waypoints_along(&keys, reached.then_some((goal_x, goal_z))),
        found: reached,
        termination: if reached {
            PathTermination::Reached
        } else {
            PathTermination::Unreachable
        },
    }
}

/// `from` and then each parent up to the root of its side.
fn chain(seen: &HashMap<AStarKey, ClosedEntry>, from: AStarKey) -> Vec<AStarKey> {
    let mut keys = vec![from];
    while let Some(entry) = seen.get(keys.last().unwrap_or(&from)) {
        if keys.last() == Some(&entry.parent) {
            break;
        }
        keys.push(entry.parent);
    }
    keys
}

/// Waypoints for the keys after the first, which is where the walker stands,
/// ending at `goal` or else the last cell's centre. Stair steps are
/// interpolated client-side; keep only landings and the endpoint.
fn waypoints_along(keys: &[AStarKey], goal: Option<(f32, f32)>) -> Vec<PathWaypoint> {
    let last = keys.len().saturating_sub(1);
    keys.iter()
        .enumerate()
        .skip(1)
        .filter(|&(index, key)| index == last || is_regular_key(key.2))
        .map(|(index, &(x, z, fk))| {
            let (x, z) = match goal {
                Some(goal) if index == last => goal,
                _ => (x as f32 + 0.5, z as f32 + 0.5),
            };
            PathWaypoint {
                x,
                z,
                floor: key_to_floor(fk),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pathfinding::tests::perimeter_walls;
    use crate::pathfinding::{
        PathTermination, RuntimeFloorGrid, RuntimePassability, DOORWAY_SHIFT, EDGE_S,
    };
    use rand::{rngs::SmallRng, Rng, SeedableRng};

    /// Prod house r-30_+81_7, a 6×4 room whose door is the +Z wall's fourth
    /// segment; a monster waited outside it on 10/26 for a player shut inside.
    fn house(door_open: bool) -> PassabilityCache {
        let mut cells = perimeter_walls(6, 4);
        cells[3 * 6 + 3] |= EDGE_S << DOORWAY_SHIFT;
        if door_open {
            cells[3 * 6 + 3] &= !EDGE_S;
        }
        let mut cache = PassabilityCache::new();
        cache.insert(
            "r-30_+81_7".into(),
            RuntimePassability {
                house_origin_x: -1882.0,
                house_origin_z: 5212.0,
                min_x: -1882.0,
                max_x: -1876.0,
                min_z: 5212.0,
                max_z: 5216.0,
                floors: vec![RuntimeFloorGrid {
                    floor_level: 0,
                    origin_x: 0,
                    origin_z: 0,
                    width: 6,
                    depth: 4,
                    y_base: 1.15,
                    wall_height: 3.0,
                    cells,
                }],
                stairwells: vec![],
                yields_to_trapped_mover: false,
                allows_projectiles: false,
                is_ground: true,
            },
        );
        cache
    }

    fn path_into_house(
        cache: &PassabilityCache,
        start: (f32, f32),
        blocked: &[(i32, i32)],
    ) -> (PathResult, SearchExtent) {
        let mut extent = SearchExtent::default();
        let result = find_path_avoiding_counted(
            start.0,
            start.1,
            0,
            -1876.5,
            5215.5,
            0,
            cache,
            DEFAULT_MAX_NODES,
            DEFAULT_MAX_NODES * 2,
            blocked,
            &mut extent,
        );
        (result, extent)
    }

    #[test]
    fn a_closed_house_goal_stops_early_at_its_door() {
        let cache = house(false);
        for start in [(-1876.5, 5216.5), (-1878.5, 5240.5)] {
            let (result, extent) = path_into_house(&cache, start, &[]);
            assert!(!result.found);
            assert_eq!(result.termination, PathTermination::Unreachable);
            assert!(extent.expanded < 300, "expanded {}", extent.expanded);
            let last = result.waypoints.last().expect("a leg to the door");
            assert_eq!((last.x, last.z), (-1878.5, 5216.5));
            assert!(extent.min.1 <= 5212 && extent.max.0 >= -1877);
        }
        let (at_the_door, _) = path_into_house(&cache, (-1878.5, 5216.5), &[]);
        assert!(at_the_door.waypoints.is_empty());
    }

    #[test]
    fn a_monster_in_the_doorway_sends_the_walker_to_the_wall() {
        let (result, extent) = path_into_house(&house(false), (-1878.5, 5240.5), &[(-1879, 5216)]);
        assert_eq!(result.termination, PathTermination::Unreachable);
        assert!(extent.expanded < 300, "expanded {}", extent.expanded);
        let last = result.waypoints.last().unwrap();
        assert_eq!((last.x, last.z), (-1876.5, 5216.5));
    }

    /// Prod 10/29: a fence pen walled off the nearest outside cells.
    #[test]
    fn a_fenced_off_wall_is_not_the_cell_to_approach() {
        use crate::fence::{sync_passability, Fence, FenceAxis, FenceEdge};
        let mut cache = house(false);
        let fence = |x, z, axis| Fence {
            edge: FenceEdge { x, z, axis },
            y: 1.15,
            owner_id: 0,
        };
        // The pen runs along the house's east and south walls.
        let pen: Vec<Fence> = (-1876..=-1874)
            .map(|x| fence(x, 5212, FenceAxis::X))
            .chain((-1882..=-1874).map(|x| fence(x, 5219, FenceAxis::X)))
            .chain((5212..=5218).map(|z| fence(-1873, z, FenceAxis::Z)))
            .chain((5216..=5218).map(|z| fence(-1882, z, FenceAxis::Z)))
            .collect();
        sync_passability(&mut cache, "fences", &pen);

        let (result, extent) = path_into_house(&cache, (-1879.5, 5208.5), &[]);
        assert_eq!(result.termination, PathTermination::Unreachable);
        assert!(extent.expanded < 300, "expanded {}", extent.expanded);
        let last = result.waypoints.last().unwrap();
        assert_eq!((last.x, last.z), (-1876.5, 5211.5));
    }

    /// A walled arena with random inner edges, sealed furniture cells and
    /// standers, so every search ends by exhaustion.
    fn arena(seed: u64, size: u8) -> (PassabilityCache, Vec<(i32, i32)>, SmallRng) {
        use crate::pathfinding::{build_furniture_passability, FurniturePiece};
        let mut rng = SmallRng::seed_from_u64(seed);
        let side = i32::from(size);
        let mut cells = perimeter_walls(size, size);
        for cell in &mut cells {
            if rng.gen_ratio(55, 100) {
                *cell |= (1 << rng.gen_range(0..4)) | (1 << rng.gen_range(0..4));
            }
        }
        let mut cache = PassabilityCache::new();
        cache.insert(
            "arena".into(),
            RuntimePassability {
                house_origin_x: 0.0,
                house_origin_z: 0.0,
                min_x: 0.0,
                max_x: f32::from(size),
                min_z: 0.0,
                max_z: f32::from(size),
                floors: vec![RuntimeFloorGrid {
                    floor_level: 0,
                    origin_x: 0,
                    origin_z: 0,
                    width: size,
                    depth: size,
                    y_base: 0.0,
                    wall_height: 3.0,
                    cells,
                }],
                stairwells: vec![],
                yields_to_trapped_mover: false,
                allows_projectiles: false,
                is_ground: true,
            },
        );
        let pieces: Vec<FurniturePiece> = (0..rng.gen_range(0..14))
            .map(|_| FurniturePiece {
                cells: vec![(rng.gen_range(0..side), rng.gen_range(0..side))],
                floor_level: 0,
                y_base: 0.0,
                wall_height: 1.0,
            })
            .collect();
        if let Some(furniture) = build_furniture_passability(&pieces) {
            cache.insert("furniture".into(), furniture);
        }
        let standers = (0..rng.gen_range(0..4))
            .map(|_| (rng.gen_range(0..side), rng.gen_range(0..side)))
            .collect();
        (cache, standers, rng)
    }

    /// Steps of the shortest way by plain breadth-first search over the
    /// forward moves, or `None` when `cap` keys hold no way.
    fn shortest(
        cache: &PassabilityCache,
        start: (i32, i32, u8),
        goal: (i32, i32, u8),
        standers: &[(i32, i32)],
        cap: usize,
    ) -> Option<usize> {
        let start_key = (start.0, start.1, floor_to_key(start.2));
        let goal_key = (goal.0, goal.1, floor_to_key(goal.2));
        if start == goal {
            return Some(1);
        }
        let moves = Moves::new(cache, start_key, goal_key, standers);
        let mut steps: HashMap<AStarKey, usize> = HashMap::from([(start_key, 0)]);
        if !moves.confine_to_floor {
            for &key in moves.stair_cells.keys() {
                if (key.0, key.1, key_to_floor(key.2)) == start {
                    steps.insert(key, 0);
                }
            }
        }
        let mut queue: std::collections::VecDeque<AStarKey> = steps.keys().copied().collect();
        let mut next = Vec::new();
        while let Some(cur) = queue.pop_front() {
            if (cur.0, cur.1, key_to_floor(cur.2)) == goal {
                return Some(steps[&cur]);
            }
            if steps.len() > cap {
                return None;
            }
            moves.neighbors(cur, false, &mut next);
            for &key in &next {
                if !steps.contains_key(&key) {
                    steps.insert(key, steps[&cur] + 1);
                    queue.push_back(key);
                }
            }
        }
        None
    }

    /// Every step of `path` from `start` is one the movement rules allow.
    fn assert_walkable(
        cache: &PassabilityCache,
        start: (i32, i32),
        path: &[PathWaypoint],
        standers: &[(i32, i32)],
        case: u64,
    ) {
        use crate::pathfinding::query::{is_cardinal_move_blocked, is_movement_blocked_for_mover};
        let mut at = start;
        for waypoint in path {
            let to = (waypoint.x.floor() as i32, waypoint.z.floor() as i32);
            let (dx, dz) = (to.0 - at.0, to.1 - at.1);
            if to == start && path.len() == 1 {
                return;
            }
            assert_eq!(dx.abs() + dz.abs(), 1, "case {case}: {at:?} -> {to:?}");
            assert!(
                !cell_blocked(standers, to.0, to.1),
                "case {case}: stander at {to:?}"
            );
            let (cx, cz) = (at.0 as f32 + 0.5, at.1 as f32 + 0.5);
            let (tx, tz) = (cx + dx as f32, cz + dz as f32);
            let open = !is_cardinal_move_blocked(cache, at.0, at.1, dx, dz, 0)
                || (at == start && !is_movement_blocked_for_mover(cache, cx, cz, tx, tz, 0, None));
            assert!(open, "case {case}: wall between {at:?} and {to:?}");
            at = to;
        }
    }

    fn search(
        cache: &PassabilityCache,
        start: (i32, i32, u8),
        goal: (i32, i32, u8),
        standers: &[(i32, i32)],
        max_nodes: usize,
    ) -> (PathResult, SearchExtent) {
        let mut extent = SearchExtent::default();
        let result = find_path_avoiding_counted(
            start.0 as f32 + 0.5,
            start.1 as f32 + 0.5,
            start.2,
            goal.0 as f32 + 0.5,
            goal.1 as f32 + 0.5,
            goal.2,
            cache,
            max_nodes,
            max_nodes * 2,
            standers,
            &mut extent,
        );
        (result, extent)
    }

    #[test]
    fn it_finds_the_shortest_way_wherever_there_is_one() {
        let (mut reached, mut refused) = (0, 0);
        for case in 0..600u64 {
            let (cache, standers, mut rng) = arena(case, 12);
            let mut point = || (rng.gen_range(0..12), rng.gen_range(0..12), 0);
            let (start, goal) = (point(), point());
            let steps = shortest(&cache, start, goal, &standers, usize::MAX);
            let (path, extent) = search(&cache, start, goal, &standers, 100_000);
            assert_eq!(
                path.found,
                steps.is_some(),
                "case {case}: {start:?} -> {goal:?}"
            );
            assert_walkable(&cache, (start.0, start.1), &path.waypoints, &standers, case);
            match steps {
                Some(steps) => {
                    reached += 1;
                    assert_eq!(path.waypoints.len(), steps, "case {case}");
                    assert_eq!(path.termination, PathTermination::Reached);
                    let last = path.waypoints.last().unwrap();
                    assert_eq!((last.x, last.z), (goal.0 as f32 + 0.5, goal.1 as f32 + 0.5));
                }
                None => {
                    refused += 1;
                    assert_eq!(
                        path.termination,
                        PathTermination::Unreachable,
                        "case {case}"
                    );
                }
            }
            for waypoint in &path.waypoints {
                let (x, z) = (waypoint.x.floor() as i32, waypoint.z.floor() as i32);
                assert!(
                    (extent.min.0..=extent.max.0).contains(&x)
                        && (extent.min.1..=extent.max.1).contains(&z),
                    "case {case}: the extent misses a cell on the path"
                );
            }
        }
        assert!(
            reached > 150 && refused > 150,
            "{reached} reached, {refused} refused"
        );
    }

    /// Past the point where the search stops looking for a shorter join.
    #[test]
    fn a_long_way_round_is_rarely_longer_than_the_shortest() {
        let (mut compared, mut longer) = (0, 0);
        for case in 0..200u64 {
            let (cache, _, mut rng) = arena(case, 48);
            let mut point = || (rng.gen_range(0..48), rng.gen_range(0..48), 0);
            let (start, goal) = (point(), point());
            let steps = shortest(&cache, start, goal, &[], usize::MAX);
            let (path, _) = search(&cache, start, goal, &[], 100_000);
            assert_eq!(path.found, steps.is_some(), "case {case}");
            assert_walkable(&cache, (start.0, start.1), &path.waypoints, &[], case);
            let Some(steps) = steps else {
                continue;
            };
            compared += 1;
            let taken = path.waypoints.len();
            assert!(taken >= steps, "case {case}: {taken} < {steps}");
            assert!(
                taken <= steps + steps / 8 + 1,
                "case {case}: {taken} for {steps}"
            );
            longer += usize::from(taken > steps);
        }
        assert!(compared > 80, "{compared} compared");
        assert!(longer * 20 <= compared, "{longer} of {compared} longer");
    }

    #[test]
    fn it_finds_every_way_across_a_stairwell() {
        use crate::pathfinding::tests::make_two_floor_stairwell;
        let (id, rp) = make_two_floor_stairwell();
        let cache = PassabilityCache::from([(id, rp)]);
        let spots = |floor: u8| (0..3).flat_map(move |x| (0..4).map(move |z| (x, z, floor)));
        for start in spots(0).chain(spots(1)) {
            for goal in spots(0).chain(spots(1)) {
                // Outside the house the ground is open without end.
                if shortest(&cache, start, goal, &[], 400).is_some() {
                    let (path, _) = search(&cache, start, goal, &[], 400);
                    assert!(path.found, "{start:?} -> {goal:?}");
                }
            }
        }
    }

    #[test]
    fn an_open_door_still_leads_inside() {
        let (result, _) = path_into_house(&house(true), (-1878.5, 5240.5), &[]);
        assert!(result.found);
        let last = result.waypoints.last().unwrap();
        assert_eq!((last.x, last.z), (-1876.5, 5215.5));
    }
}
