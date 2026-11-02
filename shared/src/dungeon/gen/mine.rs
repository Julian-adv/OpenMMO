//! Mine floors: long corridors between small chambers. Every locked floor
//! but the last forks at a junction into three work rooms, and only one of
//! them holds the way down.

use std::ops::RangeInclusive;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::{
    corridor_cells, corridor_intersects, finish_floor, place_room_around_shaft, place_room_free,
    shaft_room_size, Corridor, FloorPlan, Spacing, SHAFT_ROOM_AXIAL,
};
use crate::dungeon::{is_locked_depth, FloorLayout, Room, SpawnEntry, StairShaft, DIRS, GRID};

/// Plans are redrawn whole when a corridor cannot keep clear, so they fail
/// often.
pub(super) const FLOOR_ATTEMPTS: u32 = 500;
const CHAMBER: RangeInclusive<i32> = 5..=7;
const SHAFT_CHAMBER: RangeInclusive<i32> = 7..=9;
const WORK_ROOM: RangeInclusive<i32> = SHAFT_ROOM_AXIAL..=13;
const BOSS_ROOM: RangeInclusive<i32> = 15..=19;
/// Minimum Manhattan distance between linked room centers.
const HOP: i32 = 18;
const BRANCH_HOP: i32 = 10;
const BRANCHES: usize = 3;
/// How far a corridor keeps from rooms it does not link, and how close to
/// its own rooms it may meet another corridor.
pub(in crate::dungeon) const ISOLATION: i32 = 2;
/// Clear cells between rooms: no back-to-back doors, and every corridor
/// keeps a stretch of its own past the meeting zone beside each room.
pub(in crate::dungeon) const ROOM_GAP: i32 = 2 * ISOLATION + 2;
const FORK_EXTRA_SPAWNS: usize = 3;

pub(super) fn try_generate_floor(
    rng: &mut ChaCha8Rng,
    depth: u8,
    is_last: bool,
    up_shaft: StairShaft,
    table: &[SpawnEntry],
) -> Option<FloorLayout> {
    let mut rooms = vec![place_room_around_shaft(rng, &up_shaft)?];
    let down_along_z = rng.gen_range(0..2) == 1;
    let fork = is_locked_depth(depth) && !is_last;
    let (links, end_room) = if fork {
        fork_rooms(rng, &mut rooms, &up_shaft)?
    } else {
        tunnel_rooms(rng, &mut rooms, &up_shaft, down_along_z, is_last)?
    };
    let corridors = route(&rooms, &links)?;
    let mut max_mouths = vec![0; rooms.len()];
    for &(a, b) in &links {
        max_mouths[a] += 1;
        max_mouths[b] += 1;
    }
    let plan = FloorPlan {
        rooms,
        corridors,
        end_room,
        down_along_z,
        extra_spawns: if fork { FORK_EXTRA_SPAWNS } else { 0 },
        max_mouths: Some(max_mouths),
    };
    finish_floor(rng, depth, is_last, up_shaft, table, plan)
}

/// Two or three chambers, then the stair room (or the boss room), each a
/// long corridor from the one before.
fn tunnel_rooms(
    rng: &mut ChaCha8Rng,
    rooms: &mut Vec<Room>,
    up_shaft: &StairShaft,
    down_along_z: bool,
    is_last: bool,
) -> Option<(Vec<(usize, usize)>, usize)> {
    for _ in 0..rng.gen_range(2..=3) {
        let size = (rng.gen_range(CHAMBER), rng.gen_range(CHAMBER));
        push_far(rng, rooms, up_shaft, size, HOP)?;
    }
    let size = if is_last {
        (rng.gen_range(BOSS_ROOM), rng.gen_range(BOSS_ROOM))
    } else {
        let axial = SHAFT_ROOM_AXIAL..=SHAFT_ROOM_AXIAL + 2;
        shaft_room_size(rng, down_along_z, SHAFT_CHAMBER, axial)
    };
    push_far(rng, rooms, up_shaft, size, HOP)?;
    let end_room = rooms.len() - 1;
    Some(((0..end_room).map(|i| (i, i + 1)).collect(), end_room))
}

/// A junction chamber, then a work room per branch off the junction.
fn fork_rooms(
    rng: &mut ChaCha8Rng,
    rooms: &mut Vec<Room>,
    up_shaft: &StairShaft,
) -> Option<(Vec<(usize, usize)>, usize)> {
    let size = (rng.gen_range(CHAMBER), rng.gen_range(CHAMBER));
    push_far(rng, rooms, up_shaft, size, HOP)?;
    let way_down = rng.gen_range(0..BRANCHES);
    for _ in 0..BRANCHES {
        let size = (rng.gen_range(WORK_ROOM), rng.gen_range(WORK_ROOM));
        push_near(rng, rooms, up_shaft, size, 1, BRANCH_HOP)?;
    }
    let links = std::iter::once((0, 1))
        .chain((2..rooms.len()).map(|i| (1, i)))
        .collect();
    Some((links, 2 + way_down))
}

/// Places a room at least `min` cells from the last one.
fn push_far(
    rng: &mut ChaCha8Rng,
    rooms: &mut Vec<Room>,
    up_shaft: &StairShaft,
    size: (i32, i32),
    min: i32,
) -> Option<()> {
    push_near(rng, rooms, up_shaft, size, rooms.len() - 1, min)
}

/// Places a room at least `min` cells from room `from`.
fn push_near(
    rng: &mut ChaCha8Rng,
    rooms: &mut Vec<Room>,
    up_shaft: &StairShaft,
    (w, d): (i32, i32),
    from: usize,
    min: i32,
) -> Option<()> {
    let spacing = Spacing {
        anchor: rooms[from].center(),
        min,
        gap: ROOM_GAP,
    };
    let room = place_room_free(rng, w, d, rooms, up_shaft, spacing)?;
    rooms.push(room);
    Some(())
}

/// Routes each link clear of the other rooms. Corridors meet only within
/// `ISOLATION` cells of a room they share, so no crossroads form between
/// rooms.
fn route(rooms: &[Room], links: &[(usize, usize)]) -> Option<Vec<Corridor>> {
    const FREE: u8 = u8::MAX;
    let mut owner = vec![FREE; (GRID * GRID) as usize];
    let in_room = |(x, z): (i32, i32)| rooms.iter().any(|r| r.contains(x, z));
    let mut corridors = Vec::with_capacity(links.len());
    for (link, &(a, b)) in links.iter().enumerate() {
        let link = link as u8;
        let (from, to) = (rooms[a].center(), rooms[b].center());
        let others: Vec<Room> = (0..rooms.len())
            .filter(|&i| i != a && i != b)
            .map(|i| rooms[i].expanded(ISOLATION))
            .collect();
        let ends = [rooms[a].expanded(ISOLATION), rooms[b].expanded(ISOLATION)];
        let taken = |(x, z): (i32, i32)| {
            std::iter::once((0, 0)).chain(DIRS).any(|(dx, dz)| {
                let o = owner[(x + dx + (z + dz) * GRID) as usize];
                o != FREE && o != link
            })
        };
        let fits = |x_first| {
            !others
                .iter()
                .any(|r| corridor_intersects(from, to, x_first, r))
                && !corridor_cells(from, to, x_first)
                    .filter(|&(x, z)| !ends.iter().any(|r| r.contains(x, z)))
                    .any(taken)
        };
        let x_first = [true, false].into_iter().find(|&x| fits(x))?;
        for c in corridor_cells(from, to, x_first).filter(|&c| !in_room(c)) {
            owner[(c.0 + c.1 * GRID) as usize] = link;
        }
        corridors.push((a, b, x_first));
    }
    Some(corridors)
}
