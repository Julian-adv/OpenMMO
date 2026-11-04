//! Mine floors: long corridors between small chambers. Every locked floor
//! but the last forks at a junction into three work rooms, and only one of
//! them holds the way down.

use std::ops::{Range, RangeInclusive};

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::{
    cell_is_corridor, corridor_cells, corridor_intersects, finish_floor, pick_index,
    place_room_around_shaft, place_room_free, shaft_room_size, wall_yaw, Corridor, FloorPlan,
    PropPlacer, Spacing, SHAFT_ROOM_AXIAL,
};
use crate::dungeon::{
    is_locked_depth, FloorLayout, PropKind, PropSpec, Room, SpawnEntry, StairShaft, DIRS, GRID,
};

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
    let mut layout = finish_floor(rng, depth, is_last, up_shaft, table, plan)?;
    let decor = [timber_posts(&layout), rails(&layout)].concat();
    layout.props.extend(decor);
    let work_rooms = if fork { 2..2 + BRANCHES } else { 0..0 };
    let fixtures = room_fixtures(rng, &layout, work_rooms);
    layout.props.extend(fixtures);
    Some(layout)
}

/// Straight two-wide corridor stretches, sampled every `every` cells along
/// them: `(x, z, along_x)` where the cell pair from `(x, z)` across the
/// corridor has rock on both sides for each step in `span` along it, so
/// nothing lands at a bend or a room mouth.
fn straight_runs(
    layout: &FloorLayout,
    every: i32,
    span: RangeInclusive<i32>,
) -> Vec<(i32, i32, bool)> {
    let corridor = |x, z| cell_is_corridor(layout, x, z);
    let rock = |x, z| !layout.is_carved(x, z);
    let straight = |x: i32, z: i32, along_x: bool| {
        let ((dx, dz), (sx, sz)) = if along_x {
            ((1, 0), (0, 1))
        } else {
            ((0, 1), (1, 0))
        };
        span.clone().all(|k| {
            let (cx, cz) = (x + k * dx, z + k * dz);
            corridor(cx, cz)
                && corridor(cx + sx, cz + sz)
                && rock(cx - sx, cz - sz)
                && rock(cx + 2 * sx, cz + 2 * sz)
        })
    };
    let mut runs = Vec::new();
    for z in 1..GRID - 2 {
        for x in 1..GRID - 2 {
            for along_x in [true, false] {
                let step = if along_x { x } else { z };
                if step % every == 0 && straight(x, z, along_x) {
                    runs.push((x, z, along_x));
                }
            }
        }
    }
    runs
}

/// Floors whose corridors carry ore-cart rails.
pub(in crate::dungeon) const RAIL_DEPTHS: RangeInclusive<u8> = 6..=15;

/// Rail pieces down the middle of straight corridor stretches, one per two
/// cells so they join end to end. Each piece covers the 2×2 cells from its
/// own `(x, z)`.
fn rails(layout: &FloorLayout) -> Vec<PropSpec> {
    if !RAIL_DEPTHS.contains(&layout.depth) {
        return Vec::new();
    }
    straight_runs(layout, 2, -1..=2)
        .into_iter()
        .map(|(x, z, along_x)| PropSpec {
            x,
            z,
            kind: PropKind::MineRail,
            stack: 1,
            rotation: if along_x { 90 } else { 0 },
        })
        .collect()
}

/// A solid prop some rooms on a band of floors get one of.
pub(in crate::dungeon) struct Fixture {
    pub kind: PropKind,
    pub depths: RangeInclusive<u8>,
    room_pct: u32,
}

const fn fixture(kind: PropKind, depths: RangeInclusive<u8>, room_pct: u32) -> Fixture {
    Fixture {
        kind,
        depths,
        room_pct,
    }
}

/// Crystals roll up to three times per room, the later rolls only deeper.
pub(in crate::dungeon) const FIXTURES: [Fixture; 7] = [
    fixture(PropKind::MineToolRack, 1..=10, 30),
    fixture(PropKind::MineOreHeap, 1..=20, 35),
    fixture(PropKind::MineCart, 6..=15, 35),
    fixture(PropKind::MineRubble, 11..=20, 40),
    fixture(PropKind::MineCrystalCluster, 6..=25, 30),
    fixture(PropKind::MineCrystalCluster, 13..=25, 35),
    fixture(PropKind::MineCrystalCluster, 20..=25, 45),
];

/// A fork floor's three work rooms take these in a shuffled order: tool
/// store, ore yard, and a rest area kept free of mine fixtures.
const WORK_ROOM_THEMES: [&[(PropKind, usize)]; 3] = [
    &[(PropKind::MineToolRack, 3), (PropKind::MineCart, 1)],
    &[(PropKind::MineOreHeap, 3), (PropKind::MineCart, 2)],
    &[],
];

/// Solid mine props in the rooms, after the barrels and crates. Rooms in
/// `work_rooms` get a theme instead of the depth-banded fixtures.
fn room_fixtures(
    rng: &mut ChaCha8Rng,
    layout: &FloorLayout,
    work_rooms: Range<usize>,
) -> Vec<PropSpec> {
    let mut placer = PropPlacer::new(layout);
    for fixture in FIXTURES.iter().filter(|f| f.depths.contains(&layout.depth)) {
        for (i, room) in layout.rooms.iter().enumerate() {
            if work_rooms.contains(&i) || rng.gen_range(0..100u32) >= fixture.room_pct {
                continue;
            }
            place_one(rng, &mut placer, layout, room, fixture.kind);
        }
    }
    if !work_rooms.is_empty() {
        let offset = pick_index(rng, WORK_ROOM_THEMES.len());
        for (n, i) in work_rooms.enumerate() {
            let theme = WORK_ROOM_THEMES[(n + offset) % WORK_ROOM_THEMES.len()];
            for &(kind, count) in theme {
                for _ in 0..count {
                    place_one(rng, &mut placer, layout, &layout.rooms[i], kind);
                }
            }
        }
    }
    placer.placed
}

/// Places one `kind` at a random free spot in `room`, turned at random where
/// its shape allows.
fn place_one(
    rng: &mut ChaCha8Rng,
    placer: &mut PropPlacer,
    layout: &FloorLayout,
    room: &Room,
    kind: PropKind,
) {
    let spots: Vec<PropSpec> = match kind {
        PropKind::MineCart => cart_spots(layout, room),
        PropKind::MineToolRack => tool_rack_spots(layout, room),
        _ => square_spots(layout, room, kind),
    }
    .into_iter()
    .filter(|s| placer.fits(s))
    .collect();
    if spots.is_empty() {
        return;
    }
    let mut spec = spots[pick_index(rng, spots.len())];
    let turn = match kind {
        PropKind::MineCart => 180,
        PropKind::MineToolRack => 0,
        _ => 90,
    };
    if turn > 0 {
        spec.rotation += turn * rng.gen_range(0..u32::from(360 / turn)) as u16;
    }
    placer.place(spec);
}

fn room_cells(room: &Room) -> impl Iterator<Item = (i32, i32)> + '_ {
    (room.z..room.z + room.d).flat_map(move |z| (room.x..room.x + room.w).map(move |x| (x, z)))
}

fn spec(kind: PropKind, (x, z): (i32, i32), rotation: u16) -> PropSpec {
    PropSpec {
        x,
        z,
        kind,
        stack: 1,
        rotation,
    }
}

/// Carts lying along a wall of `room`, both cells against it.
fn cart_spots(layout: &FloorLayout, room: &Room) -> Vec<PropSpec> {
    let rock = |x, z| !layout.is_carved(x, z);
    let mut spots = Vec::new();
    for (x, z) in room_cells(room) {
        let sides = [-1, 1];
        if room.contains(x, z + 1) && sides.iter().any(|&s| rock(x + s, z) && rock(x + s, z + 1)) {
            spots.push(spec(PropKind::MineCart, (x, z), 0));
        }
        if room.contains(x + 1, z) && sides.iter().any(|&s| rock(x, z + s) && rock(x + 1, z + s)) {
            spots.push(spec(PropKind::MineCart, (x, z), 90));
        }
    }
    spots
}

/// `kind` over 2×2 room cells with one side against a wall.
fn square_spots(layout: &FloorLayout, room: &Room, kind: PropKind) -> Vec<PropSpec> {
    let rock = |x, z| !layout.is_carved(x, z);
    room_cells(room)
        .filter(|&(x, z)| room.contains(x + 1, z + 1))
        .filter(|&(x, z)| {
            (rock(x, z - 1) && rock(x + 1, z - 1))
                || (rock(x, z + 2) && rock(x + 1, z + 2))
                || (rock(x - 1, z) && rock(x - 1, z + 1))
                || (rock(x + 2, z) && rock(x + 2, z + 1))
        })
        .map(|c| spec(kind, c, 0))
        .collect()
}

/// Tool racks backed onto the north or east wall of `room`, the walls the
/// camera looks at.
fn tool_rack_spots(layout: &FloorLayout, room: &Room) -> Vec<PropSpec> {
    let rock = |x, z| !layout.is_carved(x, z);
    let mut spots = Vec::new();
    for (x, z) in room_cells(room) {
        if room.contains(x + 1, z) && rock(x, z - 1) && rock(x + 1, z - 1) {
            spots.push(spec(PropKind::MineToolRack, (x, z), wall_yaw((0, -1))));
        }
        if room.contains(x, z + 1) && rock(x + 1, z) && rock(x + 1, z + 1) {
            spots.push(spec(PropKind::MineToolRack, (x, z), wall_yaw((1, 0))));
        }
    }
    spots
}

/// Cells between support post pairs; the deeper, the fewer human traces.
fn post_spacing(depth: u8) -> Option<i32> {
    match depth {
        1..=10 => Some(6),
        11..=15 => Some(9),
        16..=20 => Some(14),
        _ => None,
    }
}

/// Facing pairs of support posts on straight corridor stretches, one on each
/// wall, every `post_spacing` cells.
fn timber_posts(layout: &FloorLayout) -> Vec<PropSpec> {
    let Some(spacing) = post_spacing(layout.depth) else {
        return Vec::new();
    };
    let post = |x, z, back| PropSpec {
        x,
        z,
        kind: PropKind::MineTimberPost,
        stack: 1,
        rotation: wall_yaw(back),
    };
    straight_runs(layout, spacing, -1..=1)
        .into_iter()
        .flat_map(|(x, z, along_x)| {
            if along_x {
                [post(x, z, (0, -1)), post(x, z + 1, (0, 1))]
            } else {
                [post(x, z, (-1, 0)), post(x + 1, z, (1, 0))]
            }
        })
        .collect()
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
    for _ in 0..rng.gen_range(2..=3_i32) {
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
    let way_down = pick_index(rng, BRANCHES);
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
