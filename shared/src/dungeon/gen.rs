//! Floor layout generation. Everything here must stay deterministic
//! across native and wasm builds — ChaCha8 RNG only, integer math only,
//! fixed draw order (retries re-draw in the same code path, which is
//! fine; changing the *algorithm* shifts the stream and is gated by the
//! golden-hash test).

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use super::{
    FloorLayout, Room, SpawnSpec, StairShaft, BOSS_MONSTER_TYPE, GRID, MAX_DEPTH, MIN_DEPTH,
    SHAFT_LEN, SHAFT_W,
};

const ROOM_MIN: i32 = 5;
const ROOM_MAX: i32 = 12;
/// Axial size needed by a room that hosts a stair shaft (run + 1 margin each end).
const SHAFT_ROOM_AXIAL: i32 = SHAFT_LEN + 2;
const FLOOR_ATTEMPTS: u32 = 30;
const ROOM_PLACE_ATTEMPTS: u32 = 60;
const SPAWN_CLEAR_RADIUS: i32 = 3;

const DEPTH_SALT: u64 = 0x9e37_79b9_7f4a_7c15;

pub fn dungeon_depth(seed: u64) -> u8 {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    rng.gen_range(MIN_DEPTH..=MAX_DEPTH)
}

pub fn generate_dungeon(seed: u64) -> Vec<FloorLayout> {
    let mut meta = ChaCha8Rng::seed_from_u64(seed);
    let total = meta.gen_range(MIN_DEPTH..=MAX_DEPTH);
    let along_z = meta.gen_range(0..2) == 1;
    let reversed = meta.gen_range(0..2) == 1;

    // Entrance shaft: entry landing straddles the grid center so the
    // world-space entrance position sits on the top landing.
    let half = GRID / 2;
    let entrance_shaft = if along_z {
        StairShaft {
            x: half - 1,
            z: if reversed { half - SHAFT_LEN + 1 } else { half },
            along_z,
            reversed,
        }
    } else {
        StairShaft {
            x: if reversed { half - SHAFT_LEN + 1 } else { half },
            z: half - 1,
            along_z,
            reversed,
        }
    };

    let mut floors = Vec::with_capacity(total as usize);
    let mut up = entrance_shaft;
    for depth in 1..=total {
        let layout = generate_floor(seed, depth, total, up);
        let dead_end = layout.down_shaft.is_none();
        if let Some(down) = layout.down_shaft {
            up = down;
        }
        floors.push(layout);
        // A fallback floor that couldn't fit a down shaft promotes itself
        // to the final floor (chest + boss); stop descending.
        if dead_end {
            break;
        }
    }
    floors
}

fn generate_floor(seed: u64, depth: u8, total: u8, up_shaft: StairShaft) -> FloorLayout {
    let mut rng = ChaCha8Rng::seed_from_u64(seed ^ (depth as u64).wrapping_mul(DEPTH_SALT));
    let is_last = depth == total;

    for _ in 0..FLOOR_ATTEMPTS {
        if let Some(layout) = try_generate_floor(&mut rng, depth, is_last, up_shaft) {
            return layout;
        }
    }
    fallback_floor(&mut rng, depth, is_last, up_shaft)
}

fn try_generate_floor(
    rng: &mut ChaCha8Rng,
    depth: u8,
    is_last: bool,
    up_shaft: StairShaft,
) -> Option<FloorLayout> {
    let room_count = rng.gen_range(3..=5) as usize;

    // Room 0 wraps the up shaft (its exit landing is this floor's arrival).
    let mut rooms = vec![place_room_around_shaft(rng, &up_shaft)?];

    // Remaining rooms; the last one is sized to host the down shaft
    // (or the boss + chest on the final floor — same generous sizing).
    let down_along_z = rng.gen_range(0..2) == 1;
    while rooms.len() < room_count {
        let hosts_shaft = rooms.len() == room_count - 1;
        let (w, d) = if hosts_shaft {
            if down_along_z {
                (
                    rng.gen_range(ROOM_MIN..=ROOM_MAX),
                    rng.gen_range(SHAFT_ROOM_AXIAL..=ROOM_MAX),
                )
            } else {
                (
                    rng.gen_range(SHAFT_ROOM_AXIAL..=ROOM_MAX),
                    rng.gen_range(ROOM_MIN..=ROOM_MAX),
                )
            }
        } else {
            (
                rng.gen_range(ROOM_MIN..=ROOM_MAX),
                rng.gen_range(ROOM_MIN..=ROOM_MAX),
            )
        };
        let room = place_room_free(rng, w, d, &rooms, &up_shaft)?;
        rooms.push(room);
    }

    // Down shaft inside the last room.
    let down_shaft = if is_last {
        None
    } else {
        Some(place_shaft_in_room(
            rng,
            rooms.last().unwrap(),
            down_along_z,
        ))
    };

    // Carve rooms, shafts, then room-chain corridors.
    let mut carved = vec![false; (GRID * GRID) as usize];
    for room in &rooms {
        carve_rect(&mut carved, room);
    }
    carve_rect(&mut carved, &up_shaft.rect());
    if let Some(ref down) = down_shaft {
        carve_rect(&mut carved, &down.rect());
    }
    for i in 0..rooms.len() - 1 {
        carve_corridor(&mut carved, rooms[i].center(), rooms[i + 1].center());
    }

    let chest = if is_last {
        Some(rooms.last().unwrap().center())
    } else {
        None
    };

    let mut layout = FloorLayout {
        depth,
        rooms,
        carved,
        up_shaft,
        down_shaft,
        chest,
        spawns: Vec::new(),
    };

    if !floor_is_connected(&layout) {
        return None;
    }

    layout.spawns = roll_spawns(rng, &layout);
    Some(layout)
}

/// Room 0: random dims/position constrained to contain the up shaft with
/// a ≥1-cell margin on every side.
fn place_room_around_shaft(rng: &mut ChaCha8Rng, shaft: &StairShaft) -> Option<Room> {
    let r = shaft.rect();
    let (w, d) = if shaft.along_z {
        (
            rng.gen_range(ROOM_MIN..=ROOM_MAX),
            rng.gen_range(SHAFT_ROOM_AXIAL..=ROOM_MAX),
        )
    } else {
        (
            rng.gen_range(SHAFT_ROOM_AXIAL..=ROOM_MAX),
            rng.gen_range(ROOM_MIN..=ROOM_MAX),
        )
    };

    let x = range_pick(rng, (r.x + r.w + 1 - w).max(1), (r.x - 1).min(GRID - 1 - w))?;
    let z = range_pick(rng, (r.z + r.d + 1 - d).max(1), (r.z - 1).min(GRID - 1 - d))?;
    Some(Room { x, z, w, d })
}

fn place_room_free(
    rng: &mut ChaCha8Rng,
    w: i32,
    d: i32,
    rooms: &[Room],
    up_shaft: &StairShaft,
) -> Option<Room> {
    let shaft_zone = up_shaft.rect().expanded(1);
    for _ in 0..ROOM_PLACE_ATTEMPTS {
        let x = rng.gen_range(1..=GRID - 1 - w);
        let z = rng.gen_range(1..=GRID - 1 - d);
        let candidate = Room { x, z, w, d };
        let grown = candidate.expanded(1);
        if grown.intersects(&shaft_zone) {
            continue;
        }
        if rooms.iter().any(|r| grown.intersects(r)) {
            continue;
        }
        return Some(candidate);
    }
    None
}

fn place_shaft_in_room(rng: &mut ChaCha8Rng, room: &Room, along_z: bool) -> StairShaft {
    let (sw, sl) = if along_z {
        (SHAFT_W, SHAFT_LEN)
    } else {
        (SHAFT_LEN, SHAFT_W)
    };
    // Interior placement with a 1-cell margin; room sizing guarantees fit.
    let x = rng.gen_range(room.x + 1..=room.x + room.w - 1 - sw);
    let z = rng.gen_range(room.z + 1..=room.z + room.d - 1 - sl);
    StairShaft {
        x,
        z,
        along_z,
        reversed: rng.gen_range(0..2) == 1,
    }
}

fn range_pick(rng: &mut ChaCha8Rng, lo: i32, hi: i32) -> Option<i32> {
    if lo > hi {
        return None;
    }
    Some(rng.gen_range(lo..=hi))
}

fn carve_rect(carved: &mut [bool], r: &Room) {
    for z in r.z.max(0)..(r.z + r.d).min(GRID) {
        for x in r.x.max(0)..(r.x + r.w).min(GRID) {
            carved[(x + z * GRID) as usize] = true;
        }
    }
}

/// 2-cell-wide L corridor, X leg first then Z leg.
fn carve_corridor(carved: &mut [bool], from: (i32, i32), to: (i32, i32)) {
    let carve2 = |carved: &mut [bool], x: i32, z: i32, lateral_x: bool| {
        let (x2, z2) = if lateral_x { (x + 1, z) } else { (x, z + 1) };
        for (cx, cz) in [(x, z), (x2, z2)] {
            if cx >= 1 && cx < GRID - 1 && cz >= 1 && cz < GRID - 1 {
                carved[(cx + cz * GRID) as usize] = true;
            }
        }
    };
    let (x0, z0) = from;
    let (x1, z1) = to;
    for x in x0.min(x1)..=x0.max(x1) {
        carve2(carved, x, z0, false);
    }
    for z in z0.min(z1)..=z0.max(z1) {
        carve2(carved, x1, z, true);
    }
}

/// Edge-aware reachability check from the up-shaft exit landing. Walks
/// the same edge bitmasks real collision uses, treats stair-shaft
/// interior cells as blocked (they're only traversable vertically), and
/// requires every room center, the down-shaft entry, and the chest to be
/// reachable.
fn floor_is_connected(layout: &FloorLayout) -> bool {
    let cells = super::floor_passability_cells(layout);
    let start = layout.up_shaft.exit_cell();

    let is_interior = |x: i32, z: i32| -> bool {
        // Up shaft: only the exit row is a landing on this floor.
        if layout.up_shaft.contains(x, z) {
            let exit = layout.up_shaft.exit_cell();
            let on_exit_row = if layout.up_shaft.along_z {
                z == exit.1
            } else {
                x == exit.0
            };
            if !on_exit_row {
                return true;
            }
        }
        if let Some(ref down) = layout.down_shaft {
            if down.contains(x, z) {
                let entry = down.entry_cell();
                let on_entry_row = if down.along_z {
                    z == entry.1
                } else {
                    x == entry.0
                };
                if !on_entry_row {
                    return true;
                }
            }
        }
        false
    };

    let mut visited = vec![false; (GRID * GRID) as usize];
    let mut queue = std::collections::VecDeque::new();
    visited[(start.0 + start.1 * GRID) as usize] = true;
    queue.push_back(start);

    const EDGE_BITS: [(i32, i32, u8, u8); 4] = [
        (0, -1, super::EDGE_N, super::EDGE_S),
        (0, 1, super::EDGE_S, super::EDGE_N),
        (1, 0, super::EDGE_E, super::EDGE_W),
        (-1, 0, super::EDGE_W, super::EDGE_E),
    ];

    while let Some((x, z)) = queue.pop_front() {
        for (dx, dz, leave, enter) in EDGE_BITS {
            let nx = x + dx;
            let nz = z + dz;
            if nx < 0 || nx >= GRID || nz < 0 || nz >= GRID {
                continue;
            }
            let nidx = (nx + nz * GRID) as usize;
            if visited[nidx] || !layout.carved[nidx] {
                continue;
            }
            if cells[(x + z * GRID) as usize] & leave != 0 || cells[nidx] & enter != 0 {
                continue;
            }
            if is_interior(nx, nz) {
                continue;
            }
            visited[nidx] = true;
            queue.push_back((nx, nz));
        }
    }

    let reachable = |cell: (i32, i32)| visited[(cell.0 + cell.1 * GRID) as usize];

    for room in &layout.rooms {
        if !reachable(room.center()) {
            return false;
        }
    }
    if let Some(ref down) = layout.down_shaft {
        if !reachable(down.entry_cell()) {
            return false;
        }
    }
    if let Some(chest) = layout.chest {
        if !reachable(chest) {
            return false;
        }
    }
    true
}

fn roll_spawns(rng: &mut ChaCha8Rng, layout: &FloorLayout) -> Vec<SpawnSpec> {
    let exit = layout.up_shaft.exit_cell();
    let in_shaft = |x: i32, z: i32| {
        layout.up_shaft.contains(x, z)
            || layout.down_shaft.as_ref().is_some_and(|s| s.contains(x, z))
    };

    // Deterministic candidate order: row-major scan.
    let mut candidates: Vec<(i32, i32)> = Vec::new();
    for z in 0..GRID {
        for x in 0..GRID {
            if !layout.carved[(x + z * GRID) as usize] || in_shaft(x, z) {
                continue;
            }
            if (x - exit.0).abs().max((z - exit.1).abs()) <= SPAWN_CLEAR_RADIUS {
                continue;
            }
            if layout.chest == Some((x, z)) {
                continue;
            }
            candidates.push((x, z));
        }
    }

    let mut spawns = Vec::new();

    if let Some((cx, cz)) = layout.chest {
        // Boss guards the chest on the final floor.
        let boss_cell = [(cx + 2, cz), (cx - 2, cz), (cx, cz + 2), (cx, cz - 2)]
            .into_iter()
            .find(|&(x, z)| layout.is_carved(x, z) && !in_shaft(x, z))
            .unwrap_or((cx, cz));
        spawns.push(SpawnSpec {
            x: boss_cell.0,
            z: boss_cell.1,
            monster_type: BOSS_MONSTER_TYPE.to_string(),
            is_boss: true,
        });
    }

    let table = super::spawn_table(layout.depth);
    let total_weight: u32 = table.iter().map(|(_, w)| w).sum();
    let count = rng.gen_range(5..=9) as usize;

    for _ in 0..count.min(candidates.len()) {
        let idx = rng.gen_range(0..candidates.len());
        let (x, z) = candidates.swap_remove(idx);
        let mut roll = rng.gen_range(0..total_weight);
        let mut monster_type = table[0].0;
        for (ty, w) in table {
            if roll < *w {
                monster_type = ty;
                break;
            }
            roll -= w;
        }
        spawns.push(SpawnSpec {
            x,
            z,
            monster_type: monster_type.to_string(),
            is_boss: false,
        });
    }

    spawns
}

/// Guaranteed-valid degenerate layout: one large room wrapping the up
/// shaft (clamped to the grid), with the down shaft placed by
/// deterministic scan. Only reached if `FLOOR_ATTEMPTS` randomized
/// layouts all failed validation, which should be vanishingly rare.
fn fallback_floor(
    rng: &mut ChaCha8Rng,
    depth: u8,
    is_last: bool,
    up_shaft: StairShaft,
) -> FloorLayout {
    let r = up_shaft.rect();
    let x0 = (r.x - 8).max(1);
    let z0 = (r.z - 8).max(1);
    let x1 = (r.x + r.w + 8).min(GRID - 1);
    let z1 = (r.z + r.d + 8).min(GRID - 1);
    let room = Room {
        x: x0,
        z: z0,
        w: x1 - x0,
        d: z1 - z0,
    };

    let down_shaft = if is_last {
        None
    } else {
        let up_zone = r.expanded(1);
        let mut found = None;
        'scan: for z in room.z + 1..room.z + room.d - 1 - SHAFT_LEN {
            for x in room.x + 1..room.x + room.w - 1 - SHAFT_W {
                let cand = StairShaft {
                    x,
                    z,
                    along_z: true,
                    reversed: false,
                };
                if !cand.rect().expanded(1).intersects(&up_zone) {
                    found = Some(cand);
                    break 'scan;
                }
            }
        }
        // The wrap room is ≥17 cells on each axis around the shaft, so a
        // slot virtually always exists; if not, this floor promotes
        // itself to the final floor (generate_dungeon stops descending).
        found
    };

    let mut carved = vec![false; (GRID * GRID) as usize];
    carve_rect(&mut carved, &room);
    carve_rect(&mut carved, &up_shaft.rect());
    if let Some(ref down) = down_shaft {
        carve_rect(&mut carved, &down.rect());
    }

    let in_shaft = |x: i32, z: i32| {
        up_shaft.contains(x, z) || down_shaft.as_ref().is_some_and(|s| s.contains(x, z))
    };
    let chest = if is_last || down_shaft.is_none() {
        let (cx, cz) = room.center();
        let cell = [
            (cx, cz),
            (room.x + 2, room.z + 2),
            (room.x + room.w - 3, room.z + 2),
            (room.x + 2, room.z + room.d - 3),
            (room.x + room.w - 3, room.z + room.d - 3),
        ]
        .into_iter()
        .find(|&(x, z)| !in_shaft(x, z));
        cell
    } else {
        None
    };

    let mut layout = FloorLayout {
        depth,
        rooms: vec![room],
        carved,
        up_shaft,
        down_shaft,
        chest,
        spawns: Vec::new(),
    };
    layout.spawns = roll_spawns(rng, &layout);
    layout
}
