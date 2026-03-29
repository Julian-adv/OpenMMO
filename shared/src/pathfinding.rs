use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::housing::{HouseData, RoomData, RoomType, WallDirection, WallVariant};

// Edge bitmask constants (matches TypeScript EDGE_N/E/S/W)
const EDGE_N: u8 = 1; // -Z edge
const EDGE_E: u8 = 2; // +X edge
const EDGE_S: u8 = 4; // +Z edge
const EDGE_W: u8 = 8; // -X edge

const WALL_HALF_THICKNESS: f32 = 0.3;
const FLOOR_THICKNESS: f32 = 0.1;
const DEFAULT_WALL_HEIGHT: f32 = 3.0;

// --- Runtime data structures ---

#[derive(Debug, Clone)]
pub struct RuntimeFloorGrid {
    pub floor_level: u8,
    pub origin_x: i32,
    pub origin_z: i32,
    pub width: u8,
    pub depth: u8,
    pub y_base: f32,
    pub wall_height: f32,
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StairwellInfo {
    pub local_min_x: i32,
    pub local_min_z: i32,
    pub local_max_x: i32,
    pub local_max_z: i32,
    pub lower_floor: u8,
    pub upper_floor: u8,
    pub along_z: bool,
    pub reversed: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimePassability {
    pub house_origin_x: f32,
    pub house_origin_z: f32,
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub floors: Vec<RuntimeFloorGrid>,
    pub stairwells: Vec<StairwellInfo>,
}

#[derive(Debug, Clone)]
pub struct PathWaypoint {
    pub x: f32,
    pub z: f32,
    pub floor: u8,
}

#[derive(Debug, Clone)]
pub struct PathResult {
    pub waypoints: Vec<PathWaypoint>,
    pub found: bool,
}

/// Type alias for the passability cache used throughout the API.
pub type PassabilityCache = HashMap<String, RuntimePassability>;

// --- Build runtime passability ---

fn floor_y_base(floor_level: u8, wall_height: f32) -> f32 {
    floor_level as f32 * (wall_height + FLOOR_THICKNESS)
}

/// Build runtime passability data from a HouseData.
/// Expects pre-computed PassabilityGrid in house.passability.
/// The caller must ensure passability is computed before calling this.
pub fn build_runtime_passability(house: &HouseData) -> RuntimePassability {
    let grids = &house.passability;

    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_z = f32::NEG_INFINITY;

    let floors: Vec<RuntimeFloorGrid> = grids
        .iter()
        .map(|g| {
            let world_min_x = house.origin.x + g.origin_x as f32;
            let world_min_z = house.origin.z + g.origin_z as f32;
            let world_max_x = world_min_x + g.width as f32;
            let world_max_z = world_min_z + g.depth as f32;
            min_x = min_x.min(world_min_x);
            max_x = max_x.max(world_max_x);
            min_z = min_z.min(world_min_z);
            max_z = max_z.max(world_max_z);

            let mut wall_height = DEFAULT_WALL_HEIGHT;
            let mut y_base = house.origin.y;
            for room in &house.rooms {
                if room.floor_level == g.floor_level {
                    wall_height = room.wall_height;
                    y_base = house.origin.y + floor_y_base(room.floor_level, room.wall_height);
                    break;
                }
                if room.room_type == RoomType::Stairwell && g.floor_level == room.floor_level + 1 {
                    wall_height = room.wall_height;
                    y_base = house.origin.y + floor_y_base(g.floor_level, room.wall_height);
                    break;
                }
            }

            RuntimeFloorGrid {
                floor_level: g.floor_level,
                origin_x: g.origin_x,
                origin_z: g.origin_z,
                width: g.width,
                depth: g.depth,
                y_base,
                wall_height,
                cells: g.cells.clone(),
            }
        })
        .collect();

    let mut stairwells = Vec::new();
    for room in &house.rooms {
        if room.room_type == RoomType::Stairwell {
            stairwells.push(StairwellInfo {
                local_min_x: room.local_x,
                local_min_z: room.local_z,
                local_max_x: room.local_x + room.size_x as i32,
                local_max_z: room.local_z + room.size_z as i32,
                lower_floor: room.floor_level,
                upper_floor: room.floor_level + 1,
                along_z: room.size_z as i32 >= room.size_x as i32,
                reversed: room.stair_reversed,
            });
        }
    }

    RuntimePassability {
        house_origin_x: house.origin.x,
        house_origin_z: house.origin.z,
        min_x,
        max_x,
        min_z,
        max_z,
        floors,
        stairwells,
    }
}

// --- Cardinal move blocking (for A* expansion) ---

/// Check if a cardinal (1-cell) move is blocked on a specific floor level.
/// Matches by floor_level directly (no Y range check), no proximity buffer.
pub fn is_cardinal_move_blocked(
    cache: &PassabilityCache,
    cell_x: i32,
    cell_z: i32,
    dx: i32,
    dz: i32,
    floor_level: u8,
) -> bool {
    let nx = cell_x + dx;
    let nz = cell_z + dz;
    let (leave_bit, enter_bit) = match (dx, dz) {
        (1, 0) => (EDGE_E, EDGE_W),
        (-1, 0) => (EDGE_W, EDGE_E),
        (0, 1) => (EDGE_S, EDGE_N),
        (0, -1) => (EDGE_N, EDGE_S),
        _ => return false,
    };

    for rp in cache.values() {
        let cx_f = cell_x as f32;
        let nxf = nx as f32;
        let cz_f = cell_z as f32;
        let nzf = nz as f32;
        if cx_f < rp.min_x && nxf < rp.min_x {
            continue;
        }
        if cx_f > rp.max_x && nxf > rp.max_x {
            continue;
        }
        if cz_f < rp.min_z && nzf < rp.min_z {
            continue;
        }
        if cz_f > rp.max_z && nzf > rp.max_z {
            continue;
        }

        for floor in &rp.floors {
            if floor.floor_level != floor_level {
                continue;
            }
            let fx = rp.house_origin_x.floor() as i32 + floor.origin_x;
            let fz = rp.house_origin_z.floor() as i32 + floor.origin_z;
            let w = floor.width as i32;
            let d = floor.depth as i32;

            let gx = cell_x - fx;
            let gz = cell_z - fz;
            if gx >= 0 && gx < w && gz >= 0 && gz < d {
                if floor.cells[(gx + gz * w) as usize] & leave_bit != 0 {
                    return true;
                }
            }

            let ngx = nx - fx;
            let ngz = nz - fz;
            if ngx >= 0 && ngx < w && ngz >= 0 && ngz < d {
                if floor.cells[(ngx + ngz * w) as usize] & enter_bit != 0 {
                    return true;
                }
            }
        }
    }
    false
}

// --- Continuous movement blocking (for path smoothing and player movement) ---

/// Check if movement from→to is blocked by any cell edge.
/// Uses WALL_HALF_THICKNESS proximity buffer.
pub fn is_movement_blocked(
    cache: &PassabilityCache,
    from_x: f32,
    from_z: f32,
    to_x: f32,
    to_z: f32,
    y: f32,
) -> bool {
    let min_x = from_x.min(to_x) - WALL_HALF_THICKNESS;
    let max_x = from_x.max(to_x) + WALL_HALF_THICKNESS;
    let min_z = from_z.min(to_z) - WALL_HALF_THICKNESS;
    let max_z = from_z.max(to_z) + WALL_HALF_THICKNESS;

    for rp in cache.values() {
        if max_x < rp.min_x || min_x > rp.max_x || max_z < rp.min_z || min_z > rp.max_z {
            continue;
        }
        for floor in &rp.floors {
            if y < floor.y_base - 0.5 || y >= floor.y_base + floor.wall_height {
                continue;
            }
            let local_from_x = from_x - rp.house_origin_x - floor.origin_x as f32;
            let local_from_z = from_z - rp.house_origin_z - floor.origin_z as f32;
            let local_to_x = to_x - rp.house_origin_x - floor.origin_x as f32;
            let local_to_z = to_z - rp.house_origin_z - floor.origin_z as f32;

            if edge_blocks_axis(
                local_from_x,
                local_to_x,
                local_from_z,
                local_to_z,
                floor,
                true,
            ) {
                return true;
            }
            if edge_blocks_axis(
                local_from_z,
                local_to_z,
                local_from_x,
                local_to_x,
                floor,
                false,
            ) {
                return true;
            }
        }
    }
    false
}

fn edge_blocks_axis(
    from_a: f32,
    to_a: f32,
    from_b: f32,
    to_b: f32,
    floor: &RuntimeFloorGrid,
    x_axis: bool,
) -> bool {
    let size_a = if x_axis { floor.width } else { floor.depth } as i32;
    let size_b = if x_axis { floor.depth } else { floor.width } as i32;
    let w = floor.width as i32;
    let idx = |a: i32, b: i32| -> usize {
        if x_axis {
            (a + b * w) as usize
        } else {
            (b + a * w) as usize
        }
    };

    let from_cell = from_a.floor() as i32;
    let to_cell = to_a.floor() as i32;

    if from_cell != to_cell {
        let step: i32 = if to_cell > from_cell { 1 } else { -1 };
        let leave_bit = if step > 0 {
            if x_axis {
                EDGE_E
            } else {
                EDGE_S
            }
        } else {
            if x_axis {
                EDGE_W
            } else {
                EDGE_N
            }
        };
        let enter_bit = if step > 0 {
            if x_axis {
                EDGE_W
            } else {
                EDGE_N
            }
        } else {
            if x_axis {
                EDGE_E
            } else {
                EDGE_S
            }
        };

        let mut cell = from_cell;
        while cell != to_cell {
            let edge_coord = if step > 0 { cell + 1 } else { cell };
            let next_cell = cell + step;
            let denom = to_a - from_a;
            if denom.abs() > f32::EPSILON {
                let t = (edge_coord as f32 - from_a) / denom;
                let cell_b = (from_b + t * (to_b - from_b)).floor() as i32;
                if cell_b >= 0 && cell_b < size_b {
                    if cell >= 0 && cell < size_a {
                        if floor.cells[idx(cell, cell_b)] & leave_bit != 0 {
                            return true;
                        }
                    }
                    if next_cell >= 0 && next_cell < size_a {
                        if floor.cells[idx(next_cell, cell_b)] & enter_bit != 0 {
                            return true;
                        }
                    }
                }
            }
            cell += step;
        }
    }

    // Proximity check
    let nearest_edge = to_a.round() as i32;
    let to_dist = (to_a - nearest_edge as f32).abs();
    if to_dist < WALL_HALF_THICKNESS && to_dist < (from_a - nearest_edge as f32).abs() {
        let cell_b = to_b.floor() as i32;
        if cell_b < 0 || cell_b >= size_b {
            return false;
        }
        let cell_before = nearest_edge - 1;
        let cell_after = nearest_edge;
        if cell_before >= 0 && cell_before < size_a {
            let bit = if x_axis { EDGE_E } else { EDGE_S };
            if floor.cells[idx(cell_before, cell_b)] & bit != 0 {
                return true;
            }
        }
        if cell_after >= 0 && cell_after < size_a {
            let bit = if x_axis { EDGE_W } else { EDGE_N };
            if floor.cells[idx(cell_after, cell_b)] & bit != 0 {
                return true;
            }
        }
    }

    false
}

// --- Floor queries ---

/// Get the floor level at a world position based on Y height.
/// Returns 0 if outside any house.
/// Picks the floor whose y_base is closest to y among all floors whose
/// grid contains the cell — handles mid-stairwell clicks and overlapping
/// floor ranges at stairwell landings.
pub fn get_floor_at_position(cache: &PassabilityCache, x: f32, z: f32, y: f32) -> u8 {
    let cx = x.floor() as i32;
    let cz = z.floor() as i32;
    let mut best_floor: u8 = 0;
    let mut best_dist = f32::INFINITY;
    let mut found = false;

    for rp in cache.values() {
        if x < rp.min_x || x > rp.max_x || z < rp.min_z || z > rp.max_z {
            continue;
        }
        for floor in &rp.floors {
            let gx = cx - rp.house_origin_x.floor() as i32 - floor.origin_x;
            let gz = cz - rp.house_origin_z.floor() as i32 - floor.origin_z;
            if gx < 0 || gx >= floor.width as i32 || gz < 0 || gz >= floor.depth as i32 {
                continue;
            }
            // Cell is inside this floor's grid — pick the closest y_base
            let dist = (y - floor.y_base).abs();
            if dist < best_dist {
                best_dist = dist;
                best_floor = floor.floor_level;
                found = true;
            }
        }
    }

    if found {
        best_floor
    } else {
        0
    }
}

/// Get the yBase for a given floor level at a world position.
pub fn get_floor_y_base(cache: &PassabilityCache, x: f32, z: f32, floor_level: u8) -> Option<f32> {
    for rp in cache.values() {
        if x < rp.min_x || x > rp.max_x || z < rp.min_z || z > rp.max_z {
            continue;
        }
        for floor in &rp.floors {
            if floor.floor_level != floor_level {
                continue;
            }
            let gx = x.floor() as i32 - rp.house_origin_x.floor() as i32 - floor.origin_x;
            let gz = z.floor() as i32 - rp.house_origin_z.floor() as i32 - floor.origin_z;
            if gx >= 0 && gx < floor.width as i32 && gz >= 0 && gz < floor.depth as i32 {
                return Some(floor.y_base);
            }
        }
    }
    None
}

// --- Door edge update ---

/// Update passability edge bits when a door is opened or closed.
pub fn update_door_edge(
    cache: &mut PassabilityCache,
    house_id: &str,
    room: &RoomData,
    wall_dir: WallDirection,
    segment_index: usize,
    is_open: bool,
) {
    let rp = match cache.get_mut(house_id) {
        Some(rp) => rp,
        None => return,
    };

    let floor = match rp
        .floors
        .iter_mut()
        .find(|f| f.floor_level == room.floor_level)
    {
        Some(f) => f,
        None => return,
    };

    let rx = room.local_x - floor.origin_x;
    let rz = room.local_z - floor.origin_z;

    let (cx, cz, edge, adj_cx, adj_cz, adj_edge) = match wall_dir {
        WallDirection::North => {
            let cx = rx + segment_index as i32;
            (cx, rz, EDGE_N, cx, rz - 1, EDGE_S)
        }
        WallDirection::South => {
            let cx = rx + segment_index as i32;
            let cz = rz + room.size_z as i32 - 1;
            (cx, cz, EDGE_S, cx, cz + 1, EDGE_N)
        }
        WallDirection::West => {
            let cz = rz + segment_index as i32;
            (rx, cz, EDGE_W, rx - 1, cz, EDGE_E)
        }
        WallDirection::East => {
            let cx = rx + room.size_x as i32 - 1;
            let cz = rz + segment_index as i32;
            (cx, cz, EDGE_E, cx + 1, cz, EDGE_W)
        }
    };

    let w = floor.width as i32;
    let d = floor.depth as i32;

    let set_or_clear = |cells: &mut Vec<u8>, gx: i32, gz: i32, bit: u8| {
        if gx < 0 || gx >= w || gz < 0 || gz >= d {
            return;
        }
        let idx = (gx + gz * w) as usize;
        if is_open {
            cells[idx] &= !bit;
        } else {
            cells[idx] |= bit;
        }
    };

    set_or_clear(&mut floor.cells, cx, cz, edge);
    set_or_clear(&mut floor.cells, adj_cx, adj_cz, adj_edge);
}

// --- A* Pathfinding ---

const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Default max nodes for A* expansion. Sized for multi-floor house traversal.
pub const DEFAULT_MAX_NODES: usize = 2000;

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

type AStarKey = (i32, i32, u16);

struct ClosedEntry {
    g: u32,
    parent: AStarKey,
}

/// Multiplier: regular floor N has key N * FLOOR_SCALE.
/// Stairwell intermediate cells use keys between adjacent regular floors.
const FLOOR_SCALE: u16 = 16;

fn floor_to_key(f: u8) -> u16 {
    f as u16 * FLOOR_SCALE
}

fn key_to_floor(k: u16) -> u8 {
    (k / FLOOR_SCALE) as u8
}

fn is_regular_key(k: u16) -> bool {
    k % FLOOR_SCALE == 0
}

/// Precomputed stairwell cell neighbor info for the A* expansion.
struct StairNeighbor {
    x: i32,
    z: i32,
    fk: u16,
}
struct StairCellExpansion {
    prev: Option<StairNeighbor>,
    next: Option<StairNeighbor>,
}

/// Build the stairwell cell map for A* pathfinding.
/// Maps (x, z, floor_key) → expansion neighbors along the stair axis.
fn build_stair_cells(cache: &PassabilityCache) -> HashMap<AStarKey, StairCellExpansion> {
    let mut map = HashMap::new();

    for rp in cache.values() {
        let ox = rp.house_origin_x.floor() as i32;
        let oz = rp.house_origin_z.floor() as i32;

        for stair in &rp.stairwells {
            let lower_key = floor_to_key(stair.lower_floor);
            let upper_key = floor_to_key(stair.upper_floor);
            let n = if stair.along_z {
                stair.local_max_z - stair.local_min_z
            } else {
                stair.local_max_x - stair.local_min_x
            };
            let width = if stair.along_z {
                stair.local_max_x - stair.local_min_x
            } else {
                stair.local_max_z - stair.local_min_z
            };

            // Compute the floor key for step i.
            // step_pos already flips physical positions for reversed stairs,
            // so i=0 is always the entry (lower floor) end and i=n-1 is the
            // exit (upper floor) end regardless of reversed.
            let step_key = |i: i32| -> u16 {
                if i == 0 {
                    lower_key
                } else if i == n - 1 {
                    upper_key
                } else {
                    lower_key + i as u16
                }
            };

            // Compute world (x, z) for step i and lateral offset w
            let step_pos = |i: i32, w: i32| -> (i32, i32) {
                if stair.along_z {
                    let z = if stair.reversed {
                        stair.local_max_z - 1 - i
                    } else {
                        stair.local_min_z + i
                    };
                    (ox + stair.local_min_x + w, oz + z)
                } else {
                    let x = if stair.reversed {
                        stair.local_max_x - 1 - i
                    } else {
                        stair.local_min_x + i
                    };
                    (ox + x, oz + stair.local_min_z + w)
                }
            };

            for i in 0..n {
                let fk = step_key(i);
                let prev_fk = if i > 0 { Some(step_key(i - 1)) } else { None };
                let next_fk = if i < n - 1 {
                    Some(step_key(i + 1))
                } else {
                    None
                };

                for w in 0..width {
                    let (cx, cz) = step_pos(i, w);
                    let prev = prev_fk.map(|pk| {
                        let (px, pz) = step_pos(i - 1, w);
                        StairNeighbor {
                            x: px,
                            z: pz,
                            fk: pk,
                        }
                    });
                    let next = next_fk.map(|nk| {
                        let (nx, nz) = step_pos(i + 1, w);
                        StairNeighbor {
                            x: nx,
                            z: nz,
                            fk: nk,
                        }
                    });
                    map.insert((cx, cz, fk), StairCellExpansion { prev, next });
                }
            }
        }
    }

    map
}

/// Find a path on a virtual 1m world grid with floor-level awareness.
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
    let sx = start_x.floor() as i32;
    let sz = start_z.floor() as i32;
    let gx = goal_x.floor() as i32;
    let gz = goal_z.floor() as i32;

    if sx == gx && sz == gz && start_floor == goal_floor {
        return PathResult {
            waypoints: vec![PathWaypoint {
                x: goal_x,
                z: goal_z,
                floor: goal_floor,
            }],
            found: true,
        };
    }

    let stair_cells = build_stair_cells(cache);

    // Build set of (x, z, real_floor) for stairwell cells.
    // Used to block regular-floor cardinal moves into stairwell interior cells,
    // but only for floors that the stairwell actually connects.
    let mut stair_positions: HashSet<(i32, i32, u8)> = HashSet::new();
    for &(x, z, fk) in stair_cells.keys() {
        stair_positions.insert((x, z, key_to_floor(fk)));
    }

    let start_fk = floor_to_key(start_floor);

    let mut open = BinaryHeap::new();
    let mut closed: HashMap<AStarKey, ClosedEntry> = HashMap::new();

    let h = |x: i32, z: i32, fk: u16| -> u32 {
        let real_f = (fk / FLOOR_SCALE) as i32;
        let goal_f = goal_floor as i32;
        (x - gx).unsigned_abs()
            + (z - gz).unsigned_abs()
            + (real_f - goal_f).unsigned_abs() as u32 * 2
    };

    let start_h = h(sx, sz, start_fk);
    let start_key: AStarKey = (sx, sz, start_fk);
    open.push(Reverse(AStarNode {
        x: sx,
        z: sz,
        fk: start_fk,
        g: 0,
        f: start_h,
    }));
    closed.insert(
        start_key,
        ClosedEntry {
            g: 0,
            parent: start_key,
        },
    );

    // If start is on a stairwell intermediate cell, also seed that key
    // so the player doesn't have to walk back to entry landing first.
    for (&key, _) in &stair_cells {
        let (kx, kz, kfk) = key;
        if kx == sx && kz == sz && key_to_floor(kfk) == start_floor && kfk != start_fk {
            let sh = h(sx, sz, kfk);
            open.push(Reverse(AStarNode {
                x: sx,
                z: sz,
                fk: kfk,
                g: 0,
                f: sh,
            }));
            closed.insert(
                key,
                ClosedEntry {
                    g: 0,
                    parent: start_key,
                },
            );
        }
    }

    let mut best_h = start_h;
    let mut best_key = start_key;
    let mut expanded = 0;

    while let Some(Reverse(cur)) = open.pop() {
        if expanded >= max_nodes {
            break;
        }
        expanded += 1;

        let cur_key: AStarKey = (cur.x, cur.z, cur.fk);

        // Accept goal at exact fk or any intermediate stair fk on the same floor.
        // This handles clicking mid-stairwell where the cell is only reachable
        // via stair expansion with an intermediate fk, not the regular floor fk.
        if cur.x == gx && cur.z == gz && key_to_floor(cur.fk) == goal_floor {
            return PathResult {
                waypoints: reconstruct_path_vf(&closed, start_key, cur_key, goal_x, goal_z),
                found: true,
            };
        }

        if let Some(entry) = closed.get(&cur_key) {
            if cur.g > entry.g {
                continue;
            }
        }

        let on_regular = is_regular_key(cur.fk);
        let cur_floor = key_to_floor(cur.fk);

        // --- Regular floor expansion (cardinal neighbors) ---
        // Skip stairwell interior cells that aren't landings on this regular floor
        let is_stair_interior = stair_positions.contains(&(cur.x, cur.z, cur_floor))
            && !stair_cells.contains_key(&cur_key);
        if on_regular && !is_stair_interior {
            for &(dx, dz) in &DIRS {
                let nx = cur.x + dx;
                let nz = cur.z + dz;
                let new_g = cur.g + 1;

                if is_cardinal_move_blocked(cache, cur.x, cur.z, dx, dz, cur_floor) {
                    continue;
                }

                // Block moves into stairwell interior cells on regular floor.
                // A cell (nx, nz) is a stairwell interior if it belongs to a
                // stairwell but has no stair_cells entry at the current fk
                // (only landings match regular floor keys).
                if stair_positions.contains(&(nx, nz, cur_floor))
                    && !stair_cells.contains_key(&(nx, nz, cur.fk))
                {
                    continue;
                }

                let nkey: AStarKey = (nx, nz, cur.fk);
                if let Some(existing) = closed.get(&nkey) {
                    if existing.g <= new_g {
                        continue;
                    }
                }

                closed.insert(
                    nkey,
                    ClosedEntry {
                        g: new_g,
                        parent: cur_key,
                    },
                );
                let nh = h(nx, nz, cur.fk);
                open.push(Reverse(AStarNode {
                    x: nx,
                    z: nz,
                    fk: cur.fk,
                    g: new_g,
                    f: new_g + nh,
                }));
                if nh < best_h {
                    best_h = nh;
                    best_key = nkey;
                }
            }
        }

        // --- Stairwell axis expansion (prev/next along stairwell) ---
        if let Some(sc) = stair_cells.get(&cur_key) {
            for neighbor in [&sc.prev, &sc.next].into_iter().flatten() {
                let new_g = cur.g + 1;
                let nkey: AStarKey = (neighbor.x, neighbor.z, neighbor.fk);
                if let Some(existing) = closed.get(&nkey) {
                    if existing.g <= new_g {
                        continue;
                    }
                }
                closed.insert(
                    nkey,
                    ClosedEntry {
                        g: new_g,
                        parent: cur_key,
                    },
                );
                let nh = h(neighbor.x, neighbor.z, neighbor.fk);
                open.push(Reverse(AStarNode {
                    x: neighbor.x,
                    z: neighbor.z,
                    fk: neighbor.fk,
                    g: new_g,
                    f: new_g + nh,
                }));
                if nh < best_h {
                    best_h = nh;
                    best_key = nkey;
                }
            }
        }
    }

    // Partial path to closest node
    let (bx, bz, _) = best_key;
    if best_key != start_key {
        return PathResult {
            waypoints: reconstruct_path_vf(
                &closed,
                start_key,
                best_key,
                bx as f32 + 0.5,
                bz as f32 + 0.5,
            ),
            found: false,
        };
    }

    PathResult {
        waypoints: Vec::new(),
        found: false,
    }
}

fn reconstruct_path_vf(
    closed: &HashMap<AStarKey, ClosedEntry>,
    start: AStarKey,
    end: AStarKey,
    final_x: f32,
    final_z: f32,
) -> Vec<PathWaypoint> {
    let mut path = Vec::new();
    let mut key = end;

    while key != start {
        let (cx, cz, fk) = key;
        // Only emit waypoints at regular floor levels (entry/exit landings).
        // Intermediate stairwell cells are skipped — the client interpolates
        // between the entry and exit landings, and GameSceneHousingLayer
        // handles the Y offset based on stairwell position.
        if is_regular_key(fk) {
            path.push(PathWaypoint {
                x: cx as f32 + 0.5,
                z: cz as f32 + 0.5,
                floor: key_to_floor(fk),
            });
        }
        let entry = match closed.get(&key) {
            Some(e) => e,
            None => break,
        };
        key = entry.parent;
    }

    path.reverse();

    if let Some(last) = path.last_mut() {
        last.x = final_x;
        last.z = final_z;
    }

    path
}

// --- Path smoothing ---

/// Greedy line-of-sight path smoothing. Only smooths within the same floor level.
fn smooth_path(waypoints: &[PathWaypoint], cache: &PassabilityCache) -> Vec<PathWaypoint> {
    if waypoints.len() <= 2 {
        return waypoints.to_vec();
    }

    let mut result = vec![waypoints[0].clone()];
    let mut anchor = 0;

    while anchor < waypoints.len() - 1 {
        let mut farthest = anchor + 1;

        for probe in anchor + 2..waypoints.len() {
            if waypoints[probe].floor != waypoints[anchor].floor {
                break;
            }
            if is_line_passable(&waypoints[anchor], &waypoints[probe], cache) {
                farthest = probe;
            } else {
                break;
            }
        }

        result.push(waypoints[farthest].clone());
        anchor = farthest;
    }

    result
}

fn is_line_passable(from: &PathWaypoint, to: &PathWaypoint, cache: &PassabilityCache) -> bool {
    let floor = from.floor;
    let dx = to.x - from.x;
    let dz = to.z - from.z;
    let dist = (dx * dx + dz * dz).sqrt();
    let steps = (dist / 0.5).ceil() as usize;
    if steps == 0 {
        return true;
    }

    let mut prev_cx = from.x.floor() as i32;
    let mut prev_cz = from.z.floor() as i32;
    let mut prev_mx = from.x;
    let mut prev_mz = from.z;

    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let mx = from.x + dx * t;
        let mz = from.z + dz * t;
        let cx = mx.floor() as i32;
        let cz = mz.floor() as i32;

        if cx != prev_cx || cz != prev_cz {
            let step_x = cx - prev_cx;
            let step_z = cz - prev_cz;

            if step_x != 0 && step_z != 0 {
                // Diagonal cell crossing: block only if both L-shaped paths are impassable.
                let a_blocked = is_cardinal_move_blocked(cache, prev_cx, prev_cz, step_x, 0, floor)
                    || is_cardinal_move_blocked(cache, cx, prev_cz, 0, step_z, floor);
                if a_blocked {
                    let b_blocked =
                        is_cardinal_move_blocked(cache, prev_cx, prev_cz, 0, step_z, floor)
                            || is_cardinal_move_blocked(cache, prev_cx, cz, step_x, 0, floor);
                    if b_blocked {
                        return false;
                    }
                }
            } else if step_x != 0 {
                if is_cardinal_move_blocked(cache, prev_cx, prev_cz, step_x, 0, floor) {
                    return false;
                }
            } else {
                if is_cardinal_move_blocked(cache, prev_cx, prev_cz, 0, step_z, floor) {
                    return false;
                }
            }

            prev_cx = cx;
            prev_cz = cz;
        }

        // Proximity check matching is_movement_blocked's directional buffer.
        // Skip at the endpoint — it's the same destination for smoothed and
        // unsmoothed paths, so blocking here only prevents smoothing without
        // changing where the player actually stops.
        if i < steps {
            let nearest_x = mx.round() as i32;
            let to_dist_x = (mx - nearest_x as f32).abs();
            let from_dist_x = (prev_mx - nearest_x as f32).abs();
            if to_dist_x < WALL_HALF_THICKNESS && to_dist_x < from_dist_x {
                let bz = mz.floor() as i32;
                if is_cardinal_move_blocked(cache, nearest_x - 1, bz, 1, 0, floor) {
                    return false;
                }
            }

            let nearest_z = mz.round() as i32;
            let to_dist_z = (mz - nearest_z as f32).abs();
            let from_dist_z = (prev_mz - nearest_z as f32).abs();
            if to_dist_z < WALL_HALF_THICKNESS && to_dist_z < from_dist_z {
                let bx = mx.floor() as i32;
                if is_cardinal_move_blocked(cache, bx, nearest_z - 1, 0, 1, floor) {
                    return false;
                }
            }
        }

        prev_mx = mx;
        prev_mz = mz;
    }

    true
}

/// Apply open-door overlays from a HouseData to its runtime passability cache entry.
/// Should be called after build_runtime_passability to reflect doors that are already open.
pub fn apply_door_overlays(cache: &mut PassabilityCache, house: &HouseData) {
    for room in &house.rooms {
        for (dir, segs) in [
            (WallDirection::North, &room.wall_north),
            (WallDirection::South, &room.wall_south),
            (WallDirection::East, &room.wall_east),
            (WallDirection::West, &room.wall_west),
        ] {
            for (i, seg) in segs.iter().enumerate() {
                if seg.variant == WallVariant::WithDoor && seg.is_open {
                    update_door_edge(cache, &house.id, room, dir, i, true);
                }
            }
        }
    }
}

/// Convenience: find path and smooth it in one call.
pub fn find_and_smooth_path(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
) -> PathResult {
    let result = find_path(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes,
    );
    if result.waypoints.is_empty() {
        return result;
    }
    // Prepend the player's actual position so smoothing can optimize the
    // entire trajectory (start → goal), not just (first A* cell → goal).
    let mut full_path = Vec::with_capacity(result.waypoints.len() + 1);
    full_path.push(PathWaypoint {
        x: start_x,
        z: start_z,
        floor: start_floor,
    });
    full_path.extend(result.waypoints);
    let smoothed = smooth_path(&full_path, cache);
    PathResult {
        // Remove the start position — the client already knows where the player is
        // and uses the first waypoint as the movement target.
        waypoints: if smoothed.len() > 1 {
            smoothed[1..].to_vec()
        } else {
            smoothed
        },
        found: result.found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rect_room(width: u8, depth: u8) -> (String, RuntimePassability) {
        let w = width as usize;
        let d = depth as usize;
        let mut cells = vec![0u8; w * d];
        for x in 0..w {
            cells[x] |= EDGE_N;
            cells[x + (d - 1) * w] |= EDGE_S;
        }
        for z in 0..d {
            cells[z * w] |= EDGE_W;
            cells[z * w + w - 1] |= EDGE_E;
        }
        let rp = RuntimePassability {
            house_origin_x: 10.0,
            house_origin_z: 10.0,
            min_x: 10.0,
            max_x: 10.0 + width as f32,
            min_z: 10.0,
            max_z: 10.0 + depth as f32,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width,
                depth,
                y_base: 0.0,
                wall_height: 3.0,
                cells,
            }],
            stairwells: vec![],
        };
        ("house".to_string(), rp)
    }

    fn make_simple_house() -> (String, RuntimePassability) {
        make_rect_room(3, 3)
    }

    #[test]
    fn cardinal_move_blocked_by_wall() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Trying to move west from cell (10, 10) — blocked by west wall
        assert!(is_cardinal_move_blocked(&cache, 10, 10, -1, 0, 0));
        // Moving east from (10, 10) within the house — not blocked
        assert!(!is_cardinal_move_blocked(&cache, 10, 10, 1, 0, 0));
        // Moving east from (12, 10) — blocked by east wall
        assert!(is_cardinal_move_blocked(&cache, 12, 10, 1, 0, 0));
    }

    #[test]
    fn find_path_around_house() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Path from west of house to east of house
        let result = find_path(9.5, 11.5, 0, 13.5, 11.5, 0, &cache, 200);
        assert!(result.found);
        assert!(!result.waypoints.is_empty());
        // Path should go around the house, not through it
        assert!(result.waypoints.len() > 1);
    }

    #[test]
    fn path_in_open_terrain() {
        let cache = PassabilityCache::new(); // No houses
        let result = find_path(0.0, 0.0, 0, 5.0, 5.0, 0, &cache, 200);
        assert!(result.found);
    }

    #[test]
    fn smooth_path_does_not_cross_walls() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Diagonal line from NW corner to SE corner of house would cross walls
        let from = PathWaypoint {
            x: 9.5,
            z: 9.5,
            floor: 0,
        };
        let to = PathWaypoint {
            x: 13.5,
            z: 13.5,
            floor: 0,
        };
        assert!(!is_line_passable(&from, &to, &cache));

        // Line along the north side outside the house — should be passable
        let from2 = PathWaypoint {
            x: 9.5,
            z: 9.5,
            floor: 0,
        };
        let to2 = PathWaypoint {
            x: 13.5,
            z: 9.5,
            floor: 0,
        };
        assert!(is_line_passable(&from2, &to2, &cache));
    }

    #[test]
    fn smooth_path_preserves_endpoints() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Path around the house should be smoothed but still start and end correctly
        let result = find_and_smooth_path(9.5, 11.5, 0, 13.5, 11.5, 0, &cache, 200);
        assert!(result.found);
        assert!(!result.waypoints.is_empty());
        let first = &result.waypoints[0];
        let last = result.waypoints.last().unwrap();
        // First waypoint should be near start, last near goal
        assert!((first.x - 9.5).abs() < 1.0 || (first.x - 10.5).abs() < 1.0);
        assert!((last.x - 13.5).abs() < 0.01);
    }

    #[test]
    fn smooth_diagonal_inside_room() {
        let (id, rp) = make_rect_room(5, 5);
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Diagonal across the room interior (cell centers) — must be passable
        let from = PathWaypoint {
            x: 10.5,
            z: 10.5,
            floor: 0,
        };
        let to = PathWaypoint {
            x: 14.5,
            z: 14.5,
            floor: 0,
        };
        assert!(is_line_passable(&from, &to, &cache));

        // Walk parallel to north wall at z=10.2 — should be passable
        // (directional check: not approaching, just moving parallel)
        let from2 = PathWaypoint {
            x: 10.5,
            z: 10.2,
            floor: 0,
        };
        let to2 = PathWaypoint {
            x: 14.5,
            z: 10.2,
            floor: 0,
        };
        assert!(is_line_passable(&from2, &to2, &cache));

        // Goal near a wall corner — endpoint proximity shouldn't block smoothing
        let from3 = PathWaypoint {
            x: 10.5,
            z: 10.5,
            floor: 0,
        };
        let to3 = PathWaypoint {
            x: 14.8,
            z: 14.8,
            floor: 0,
        };
        assert!(is_line_passable(&from3, &to3, &cache));

        // Full find_and_smooth: diagonal should produce ≤2 waypoints (direct line)
        let result = find_and_smooth_path(10.5, 10.5, 0, 14.5, 14.5, 0, &cache, 500);
        assert!(result.found);
        assert!(
            result.waypoints.len() <= 2,
            "Expected smooth diagonal (≤2 waypoints), got {}",
            result.waypoints.len()
        );
    }

    #[test]
    fn smooth_diagonal_inside_rectangular_room() {
        // Wide rectangle: 8x3
        let (id, rp) = make_rect_room(8, 3);
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        let result = find_and_smooth_path(10.5, 10.5, 0, 17.5, 12.5, 0, &cache, 500);
        assert!(result.found);
        assert!(
            result.waypoints.len() == 1,
            "8x3 room: expected single goal waypoint (direct diagonal), got {} waypoints: {:?}",
            result.waypoints.len(),
            result
                .waypoints
                .iter()
                .map(|w| (w.x, w.z))
                .collect::<Vec<_>>()
        );

        // Tall rectangle: 3x8
        let (id2, rp2) = make_rect_room(3, 8);
        let mut cache2 = PassabilityCache::new();
        cache2.insert(id2, rp2);

        let result2 = find_and_smooth_path(10.5, 10.5, 0, 12.5, 17.5, 0, &cache2, 500);
        assert!(result2.found);
        assert!(
            result2.waypoints.len() == 1,
            "3x8 room: expected single goal waypoint (direct diagonal), got {} waypoints: {:?}",
            result2.waypoints.len(),
            result2
                .waypoints
                .iter()
                .map(|w| (w.x, w.z))
                .collect::<Vec<_>>()
        );
    }
}
