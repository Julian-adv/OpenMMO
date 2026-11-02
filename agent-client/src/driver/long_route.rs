//! Route finding past the server's per-request reach: A* over 8m sectors
//! split into their connected parts, refined into legs the server can walk.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use onlinerpg_shared::pathfinding::{self, PassabilityCache, DIRS};
use onlinerpg_shared::{EVENT_DELIVERY_RADIUS, MAX_MOVE_TARGET_DISTANCE};

pub(super) const MAX_LEG: f32 = MAX_MOVE_TARGET_DISTANCE * 0.8;
const SECTOR: i32 = 8;
const CELLS: usize = (SECTOR * SECTOR) as usize;
// Fences arrive only within this radius, so nearer sectors are re-read on every plan.
const FRESH_RADIUS: f32 = EVENT_DELIVERY_RADIUS - SECTOR as f32;
const SEARCH_MARGIN: i32 = 32;
const MAX_NODES: usize = 20_000;
const MAX_FAILED_LEGS: usize = 8;
const CROSSINGS: [i32; 8] = [3, 4, 2, 5, 1, 6, 0, 7];

#[derive(Clone, Copy)]
struct Sector {
    part: [u8; CELLS],
    open: [u8; 4],
}

const OPEN: Sector = Sector {
    part: [0; CELLS],
    open: [u8::MAX; 4],
};

type Node = ((i32, i32), u8);

#[derive(Default)]
pub(super) struct LongRoute {
    sectors: HashMap<(i32, i32), Sector>,
    failed: HashSet<Node>,
    unreachable: bool,
}

impl LongRoute {
    /// The next point to hand the server on the way to `to`, or `None` when
    /// nothing we know of leads there.
    pub(super) fn leg(
        &mut self,
        cache: &PassabilityCache,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Option<(f32, f32)> {
        if self.unreachable || self.failed.len() >= MAX_FAILED_LEGS {
            return None;
        }
        if self.failed.is_empty()
            && !pathfinding::is_movement_blocked(cache, from.0, from.1, to.0, to.1, 0, None)
        {
            return Some(toward(from, to));
        }
        let Some(route) = self.plan(cache, from, to) else {
            self.unreachable = true;
            return None;
        };
        Some(pick_leg(cache, from, &route))
    }

    /// `leg` did not get through; plan around its sector from now on.
    pub(super) fn leg_failed(&mut self, cache: &PassabilityCache, leg: (f32, f32)) {
        let at = sector_of(leg);
        let sector = *self.sectors.entry(at).or_insert_with(|| analyze(cache, at));
        self.failed.insert((at, sector.part[cell_of(leg)]));
    }

    fn sector(
        &mut self,
        cache: &PassabilityCache,
        dirty: &HashSet<(i32, i32)>,
        at: (i32, i32),
    ) -> Sector {
        if !dirty.contains(&at) && !self.sectors.contains_key(&at) {
            return OPEN;
        }
        *self.sectors.entry(at).or_insert_with(|| analyze(cache, at))
    }

    fn plan(
        &mut self,
        cache: &PassabilityCache,
        from: (f32, f32),
        to: (f32, f32),
    ) -> Option<Vec<(f32, f32)>> {
        let (start_sector, goal_sector) = (sector_of(from), sector_of(to));
        let lo = (
            start_sector.0.min(goal_sector.0) - SEARCH_MARGIN,
            start_sector.1.min(goal_sector.1) - SEARCH_MARGIN,
        );
        let hi = (
            start_sector.0.max(goal_sector.0) + SEARCH_MARGIN,
            start_sector.1.max(goal_sector.1) + SEARCH_MARGIN,
        );
        let dirty = dirty_sectors(cache, lo, hi);
        self.sectors.retain(|&at, _| {
            let (cx, cz) = sector_center(at);
            (cx - from.0).hypot(cz - from.1) > FRESH_RADIUS
        });

        let start = (
            start_sector,
            self.sector(cache, &dirty, start_sector).part[cell_of(from)],
        );
        let goal = (
            goal_sector,
            self.sector(cache, &dirty, goal_sector).part[cell_of(to)],
        );
        let h = |((sx, sz), _): Node| {
            ((sx - goal_sector.0).unsigned_abs() + (sz - goal_sector.1).unsigned_abs())
                * SECTOR as u32
        };
        let len = (to.0 - from.0).hypot(to.1 - from.1);
        let dir = ((to.0 - from.0) / len, (to.1 - from.1) / len);
        let off_line = |(at, _): Node| {
            let (cx, cz) = sector_center(at);
            ((cx - from.0) * dir.1 - (cz - from.1) * dir.0).abs() as u32
        };
        let mut open = BinaryHeap::new();
        let mut came: HashMap<Node, (Node, (f32, f32), u32)> = HashMap::new();
        let mut closed = HashSet::new();
        open.push(Reverse((h(start), off_line(start), 0u32, start)));
        while let Some(Reverse((_, _, g, node))) = open.pop() {
            if node == goal {
                let mut points = vec![to];
                let mut at = goal;
                while let Some(&(parent, point, _)) = came.get(&at) {
                    points.push(point);
                    at = parent;
                }
                points.reverse();
                return Some(points);
            }
            if !closed.insert(node) {
                continue;
            }
            if closed.len() > MAX_NODES {
                return None;
            }
            let here = self.sector(cache, &dirty, node.0);
            for (side, (dx, dz)) in DIRS.into_iter().enumerate() {
                let at = (node.0 .0 + dx, node.0 .1 + dz);
                if at.0 < lo.0 || at.0 > hi.0 || at.1 < lo.1 || at.1 > hi.1 {
                    continue;
                }
                let there = self.sector(cache, &dirty, at);
                let crossable = here.open[side] & there.open[side ^ 1];
                for i in CROSSINGS {
                    let (a, b) = (border(side, i), border(side ^ 1, i));
                    if crossable & (1 << i) == 0 || here.part[a] != node.1 {
                        continue;
                    }
                    let next = (at, there.part[b]);
                    if closed.contains(&next) || (next != goal && self.failed.contains(&next)) {
                        continue;
                    }
                    let g = g + SECTOR as u32;
                    if came.get(&next).is_some_and(|&(_, _, known)| known <= g) {
                        continue;
                    }
                    came.insert(next, (node, cell_center(at, b), g));
                    open.push(Reverse((g + h(next), off_line(next), g, next)));
                }
            }
        }
        None
    }
}

fn sector_of((x, z): (f32, f32)) -> (i32, i32) {
    (
        (x.floor() as i32).div_euclid(SECTOR),
        (z.floor() as i32).div_euclid(SECTOR),
    )
}

fn sector_center((sx, sz): (i32, i32)) -> (f32, f32) {
    let half = SECTOR as f32 / 2.0;
    ((sx * SECTOR) as f32 + half, (sz * SECTOR) as f32 + half)
}

fn cell(lx: i32, lz: i32) -> usize {
    (lx + lz * SECTOR) as usize
}

fn local(cell: usize) -> (i32, i32) {
    (cell as i32 % SECTOR, cell as i32 / SECTOR)
}

fn cell_of((x, z): (f32, f32)) -> usize {
    cell(
        (x.floor() as i32).rem_euclid(SECTOR),
        (z.floor() as i32).rem_euclid(SECTOR),
    )
}

fn cell_center((sx, sz): (i32, i32), at: usize) -> (f32, f32) {
    let (lx, lz) = local(at);
    (
        (sx * SECTOR + lx) as f32 + 0.5,
        (sz * SECTOR + lz) as f32 + 0.5,
    )
}

fn border(side: usize, i: i32) -> usize {
    match side {
        0 => cell(SECTOR - 1, i),
        1 => cell(0, i),
        2 => cell(i, SECTOR - 1),
        _ => cell(i, 0),
    }
}

/// Sectors any ground-floor grid reaches into; all others are open ground.
fn dirty_sectors(cache: &PassabilityCache, lo: (i32, i32), hi: (i32, i32)) -> HashSet<(i32, i32)> {
    let (min_x, min_z) = ((lo.0 * SECTOR) as f32, (lo.1 * SECTOR) as f32);
    let (max_x, max_z) = (((hi.0 + 1) * SECTOR) as f32, ((hi.1 + 1) * SECTOR) as f32);
    let mut dirty = HashSet::new();
    for rp in cache.values() {
        if rp.max_x < min_x || rp.min_x > max_x || rp.max_z < min_z || rp.min_z > max_z {
            continue;
        }
        for floor in rp.floors.iter().filter(|f| f.floor_level == 0) {
            let x = rp.house_origin_x.floor() as i32 + floor.origin_x;
            let z = rp.house_origin_z.floor() as i32 + floor.origin_z;
            let (x0, z0) = ((x - 1).div_euclid(SECTOR), (z - 1).div_euclid(SECTOR));
            let (x1, z1) = (
                (x + floor.width as i32).div_euclid(SECTOR),
                (z + floor.depth as i32).div_euclid(SECTOR),
            );
            for sx in x0.max(lo.0)..=x1.min(hi.0) {
                for sz in z0.max(lo.1)..=z1.min(hi.1) {
                    dirty.insert((sx, sz));
                }
            }
        }
    }
    dirty
}

fn analyze(cache: &PassabilityCache, (sx, sz): (i32, i32)) -> Sector {
    let blocked = |c: usize, dx: i32, dz: i32| {
        let (lx, lz) = local(c);
        pathfinding::is_cardinal_move_blocked(cache, sx * SECTOR + lx, sz * SECTOR + lz, dx, dz, 0)
    };
    let mut part = [u8::MAX; CELLS];
    let mut parts = 0u8;
    for seed in 0..CELLS {
        if part[seed] != u8::MAX {
            continue;
        }
        part[seed] = parts;
        let mut stack = vec![seed];
        while let Some(c) = stack.pop() {
            let (lx, lz) = local(c);
            for (dx, dz) in DIRS {
                let (nx, nz) = (lx + dx, lz + dz);
                if !(0..SECTOR).contains(&nx) || !(0..SECTOR).contains(&nz) {
                    continue;
                }
                let n = cell(nx, nz);
                if part[n] == u8::MAX && !blocked(c, dx, dz) {
                    part[n] = parts;
                    stack.push(n);
                }
            }
        }
        parts += 1;
    }
    let mut open = [0u8; 4];
    for (side, (dx, dz)) in DIRS.into_iter().enumerate() {
        for i in 0..SECTOR {
            if !blocked(border(side, i), dx, dz) {
                open[side] |= 1 << i;
            }
        }
    }
    Sector { part, open }
}

/// The farthest route point within one leg that we can walk straight to.
fn pick_leg(cache: &PassabilityCache, from: (f32, f32), route: &[(f32, f32)]) -> (f32, f32) {
    let mut walked = 0.0;
    let mut previous = from;
    let reach: Vec<_> = route
        .iter()
        .take_while(|p| {
            walked += (p.0 - previous.0).hypot(p.1 - previous.1);
            previous = **p;
            walked <= MAX_LEG * 2.0
        })
        .filter(|p| (p.0 - from.0).hypot(p.1 - from.1) <= MAX_LEG)
        .collect();
    reach
        .into_iter()
        .rev()
        .find(|p| !pathfinding::is_movement_blocked(cache, from.0, from.1, p.0, p.1, 0, None))
        .copied()
        .unwrap_or_else(|| toward(from, route[0]))
}

pub(super) fn toward(from: (f32, f32), to: (f32, f32)) -> (f32, f32) {
    let d = (to.0 - from.0).hypot(to.1 - from.1);
    if d <= MAX_LEG {
        return to;
    }
    (
        from.0 + (to.0 - from.0) * MAX_LEG / d,
        from.1 + (to.1 - from.1) * MAX_LEG / d,
    )
}

#[cfg(test)]
#[path = "long_route_tests.rs"]
mod tests;
