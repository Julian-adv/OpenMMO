use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

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
pub fn get_floor_at_position(cache: &PassabilityCache, x: f32, z: f32, y: f32) -> u8 {
    let cx = x.floor() as i32;
    let cz = z.floor() as i32;
    for rp in cache.values() {
        if x < rp.min_x || x > rp.max_x || z < rp.min_z || z > rp.max_z {
            continue;
        }
        for floor in &rp.floors {
            if y < floor.y_base - 0.5 || y >= floor.y_base + floor.wall_height {
                continue;
            }
            let gx = cx - rp.house_origin_x.floor() as i32 - floor.origin_x;
            let gz = cz - rp.house_origin_z.floor() as i32 - floor.origin_z;
            if gx >= 0 && gx < floor.width as i32 && gz >= 0 && gz < floor.depth as i32 {
                return floor.floor_level;
            }
        }
    }
    0
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

#[derive(Clone)]
struct AStarNode {
    x: i32,
    z: i32,
    floor: u8,
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
    parent_x: i32,
    parent_z: i32,
    parent_floor: u8,
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

    let h = |x: i32, z: i32, floor: u8| -> u32 {
        ((x - gx).unsigned_abs()
            + (z - gz).unsigned_abs()
            + (floor as i32 - goal_floor as i32).unsigned_abs() * 2) as u32
    };

    let mut open = BinaryHeap::new();
    let mut closed: HashMap<(i32, i32, u8), ClosedEntry> = HashMap::new();

    let start_h = h(sx, sz, start_floor);
    open.push(Reverse(AStarNode {
        x: sx,
        z: sz,
        floor: start_floor,
        g: 0,
        f: start_h,
    }));
    closed.insert(
        (sx, sz, start_floor),
        ClosedEntry {
            g: 0,
            parent_x: sx,
            parent_z: sz,
            parent_floor: start_floor,
        },
    );

    let mut best_h = start_h;
    let mut best_x = sx;
    let mut best_z = sz;
    let mut best_floor = start_floor;
    let mut expanded = 0;

    while let Some(Reverse(cur)) = open.pop() {
        if expanded >= max_nodes {
            break;
        }
        expanded += 1;

        if cur.x == gx && cur.z == gz && cur.floor == goal_floor {
            return PathResult {
                waypoints: reconstruct_path(
                    &closed,
                    sx,
                    sz,
                    start_floor,
                    gx,
                    gz,
                    goal_floor,
                    goal_x,
                    goal_z,
                ),
                found: true,
            };
        }

        if let Some(entry) = closed.get(&(cur.x, cur.z, cur.floor)) {
            if cur.g > entry.g {
                continue;
            }
        }

        // Cardinal neighbors
        for &(dx, dz) in &DIRS {
            let nx = cur.x + dx;
            let nz = cur.z + dz;
            let new_g = cur.g + 1;

            if let Some(existing) = closed.get(&(nx, nz, cur.floor)) {
                if existing.g <= new_g {
                    continue;
                }
            }

            if is_cardinal_move_blocked(cache, cur.x, cur.z, dx, dz, cur.floor) {
                continue;
            }

            closed.insert(
                (nx, nz, cur.floor),
                ClosedEntry {
                    g: new_g,
                    parent_x: cur.x,
                    parent_z: cur.z,
                    parent_floor: cur.floor,
                },
            );
            let nh = h(nx, nz, cur.floor);
            open.push(Reverse(AStarNode {
                x: nx,
                z: nz,
                floor: cur.floor,
                g: new_g,
                f: new_g + nh,
            }));

            if nh < best_h {
                best_h = nh;
                best_x = nx;
                best_z = nz;
                best_floor = cur.floor;
            }
        }

        // Stairwell transitions
        for rp in cache.values() {
            let cx_f = cur.x as f32;
            let cz_f = cur.z as f32;
            if cx_f < rp.min_x || cx_f >= rp.max_x || cz_f < rp.min_z || cz_f >= rp.max_z {
                continue;
            }
            for stair in &rp.stairwells {
                let local_x = cur.x - rp.house_origin_x.floor() as i32;
                let local_z = cur.z - rp.house_origin_z.floor() as i32;
                if local_x < stair.local_min_x
                    || local_x >= stair.local_max_x
                    || local_z < stair.local_min_z
                    || local_z >= stair.local_max_z
                {
                    continue;
                }

                let target_floor = if cur.floor == stair.lower_floor {
                    Some(stair.upper_floor)
                } else if cur.floor == stair.upper_floor {
                    Some(stair.lower_floor)
                } else {
                    None
                };

                let Some(target_floor) = target_floor else {
                    continue;
                };

                let new_g = cur.g + 2;
                if let Some(existing) = closed.get(&(cur.x, cur.z, target_floor)) {
                    if existing.g <= new_g {
                        continue;
                    }
                }

                closed.insert(
                    (cur.x, cur.z, target_floor),
                    ClosedEntry {
                        g: new_g,
                        parent_x: cur.x,
                        parent_z: cur.z,
                        parent_floor: cur.floor,
                    },
                );
                let nh = h(cur.x, cur.z, target_floor);
                open.push(Reverse(AStarNode {
                    x: cur.x,
                    z: cur.z,
                    floor: target_floor,
                    g: new_g,
                    f: new_g + nh,
                }));

                if nh < best_h {
                    best_h = nh;
                    best_x = cur.x;
                    best_z = cur.z;
                    best_floor = target_floor;
                }
            }
        }
    }

    // Partial path to closest node
    if best_x != sx || best_z != sz || best_floor != start_floor {
        return PathResult {
            waypoints: reconstruct_path(
                &closed,
                sx,
                sz,
                start_floor,
                best_x,
                best_z,
                best_floor,
                best_x as f32 + 0.5,
                best_z as f32 + 0.5,
            ),
            found: false,
        };
    }

    PathResult {
        waypoints: Vec::new(),
        found: false,
    }
}

fn reconstruct_path(
    closed: &HashMap<(i32, i32, u8), ClosedEntry>,
    sx: i32,
    sz: i32,
    s_floor: u8,
    ex: i32,
    ez: i32,
    e_floor: u8,
    final_x: f32,
    final_z: f32,
) -> Vec<PathWaypoint> {
    let mut path = Vec::new();
    let mut cx = ex;
    let mut cz = ez;
    let mut cf = e_floor;

    while cx != sx || cz != sz || cf != s_floor {
        path.push(PathWaypoint {
            x: cx as f32 + 0.5,
            z: cz as f32 + 0.5,
            floor: cf,
        });
        let entry = match closed.get(&(cx, cz, cf)) {
            Some(e) => e,
            None => break,
        };
        cx = entry.parent_x;
        cz = entry.parent_z;
        cf = entry.parent_floor;
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
pub fn smooth_path(waypoints: &[PathWaypoint], cache: &PassabilityCache) -> Vec<PathWaypoint> {
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

    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        let mx = from.x + dx * t;
        let mz = from.z + dz * t;
        let cx = mx.floor() as i32;
        let cz = mz.floor() as i32;

        if cx != prev_cx || cz != prev_cz {
            // Check each axis crossing separately
            if cx != prev_cx {
                let step_x = if cx > prev_cx { 1 } else { -1 };
                if is_cardinal_move_blocked(cache, prev_cx, prev_cz, step_x, 0, floor) {
                    return false;
                }
            }
            if cz != prev_cz {
                let check_x = if cx != prev_cx { cx } else { prev_cx };
                let step_z = if cz > prev_cz { 1 } else { -1 };
                if is_cardinal_move_blocked(cache, check_x, prev_cz, 0, step_z, floor) {
                    return false;
                }
            }
            prev_cx = cx;
            prev_cz = cz;
        }
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
    PathResult {
        waypoints: smooth_path(&result.waypoints, cache),
        found: result.found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_simple_house() -> (String, RuntimePassability) {
        // 3x3 house with all walls: a fully enclosed room
        let mut cells = vec![0u8; 9];
        // North wall (row 0, all 3 cells)
        cells[0] |= EDGE_N;
        cells[1] |= EDGE_N;
        cells[2] |= EDGE_N;
        // South wall (row 2, all 3 cells)
        cells[6] |= EDGE_S;
        cells[7] |= EDGE_S;
        cells[8] |= EDGE_S;
        // West wall (col 0, all 3 rows)
        cells[0] |= EDGE_W;
        cells[3] |= EDGE_W;
        cells[6] |= EDGE_W;
        // East wall (col 2, all 3 rows)
        cells[2] |= EDGE_E;
        cells[5] |= EDGE_E;
        cells[8] |= EDGE_E;

        let rp = RuntimePassability {
            house_origin_x: 10.0,
            house_origin_z: 10.0,
            min_x: 10.0,
            max_x: 13.0,
            min_z: 10.0,
            max_z: 13.0,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 3,
                depth: 3,
                y_base: 0.0,
                wall_height: 3.0,
                cells,
            }],
            stairwells: vec![],
        };
        ("house1".to_string(), rp)
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
}
