//! Temple floors: wide chambers set edge to edge with one wall between
//! them, each joined to the next by a doorway through it and lined with two
//! rows of columns. Some floors stand in two or three such suites with a long
//! passage from each to the next. The last floor ends in the pillared boss
//! hall.

use std::ops::RangeInclusive;

use rand::Rng;
use rand_chacha::ChaCha8Rng;

use super::{
    corridor_cells, corridor_legs, finish_floor, hall_pillars, place_room_around_shaft_in,
    place_room_free, range_pick, shaft_room_size, Corridor, FloorPlan, PropPlacer, Spacing,
    HALL_SIZE, ROOM_PLACE_ATTEMPTS,
};
use crate::dungeon::doors::{WALL_E, WALL_N, WALL_S, WALL_W};
use crate::dungeon::{
    is_locked_depth, FloorLayout, InteriorDoorSpec, PropKind, PropSpec, Room, SpawnEntry,
    StairShaft, GRID,
};

/// A chamber that finds no free side is redrawn with the whole floor.
pub(super) const FLOOR_ATTEMPTS: u32 = 500;
const CHAMBER: RangeInclusive<i32> = 15..=23;
const DOORWAY: i32 = 2;
/// A doorway opens at least this far from the corners of the chamber it enters.
const DOOR_INSET: i32 = 3;
/// Columns stand this far in from the walls, one every `COLUMN_STEP` cells.
const COLUMN_INSET: i32 = 2;
const COLUMN_STEP: i32 = 4;
/// Percent of chambers where one more column lies fallen.
const FALLEN_PCT: i32 = 35;
const FALLEN_ATTEMPTS: u32 = 8;
/// Percent of draws laid out as three suites of one or two chambers, and as
/// two suites of two or three; the rest are one suite of three to five. A
/// plan with passages fails and is redrawn more often, so fewer floors than
/// this come out split.
const THREE_SUITES_PCT: i32 = 35;
const TWO_SUITES_PCT: i32 = 35;
/// Least Manhattan distance between the centres of the chambers a passage
/// joins, the least run it shows between them, and the rock the far suite's
/// first chamber keeps from the near suite.
const PASSAGE_HOP: i32 = 34;
const PASSAGE_MIN: usize = 10;
const PASSAGE_GAP: i32 = 3;

pub(super) fn try_generate_floor(
    rng: &mut ChaCha8Rng,
    depth: u8,
    is_last: bool,
    up_shaft: StairShaft,
    table: &[SpawnEntry],
) -> Option<FloorLayout> {
    let roll = rng.gen_range(0..100);
    let mut size = |range: RangeInclusive<i32>| rng.gen_range(range) as usize;
    let suites = if roll < THREE_SUITES_PCT {
        vec![size(1..=2), size(1..=2), size(1..=2)]
    } else if roll < THREE_SUITES_PCT + TWO_SUITES_PCT {
        vec![size(2..=3), size(2..=3)]
    } else {
        vec![size(3..=5)]
    };
    let room_count: usize = suites.iter().sum();
    // The chambers that open a further suite, each across a passage.
    let across: Vec<usize> = suites[..suites.len() - 1]
        .iter()
        .scan(0, |at, n| {
            *at += n;
            Some(*at)
        })
        .collect();
    let mut rooms = vec![place_room_around_shaft_in(
        rng, &up_shaft, CHAMBER, CHAMBER,
    )?];
    let down_along_z = rng.gen_range(0..2) == 1;
    // Within a suite no rock lies between chambers: there these only keep
    // the columns off the way from one doorway to the next.
    let mut corridors: Vec<Corridor> = Vec::new();
    let mut doorways = Vec::new();
    let mut passages: Vec<Corridor> = Vec::new();
    let mut max_mouths = vec![0; room_count];
    while rooms.len() < room_count {
        let end = rooms.len() == room_count - 1;
        let (w, d) = if end && is_last {
            (HALL_SIZE, HALL_SIZE)
        } else if end {
            shaft_room_size(rng, down_along_z, CHAMBER, CHAMBER)
        } else {
            (rng.gen_range(CHAMBER), rng.gen_range(CHAMBER))
        };
        let from = rooms.len() - 1;
        if across.contains(&rooms.len()) {
            let spacing = Spacing {
                anchor: rooms[from].center(),
                min: PASSAGE_HOP,
                gap: PASSAGE_GAP,
            };
            let room = place_room_free(rng, w, d, &rooms, &up_shaft, spacing)?;
            let passage = (from, from + 1, rng.gen_range(0..2) == 1);
            passages.push(passage);
            corridors.push(passage);
            max_mouths[from] += 1;
            max_mouths[from + 1] += 1;
            rooms.push(room);
            continue;
        }
        let (room, wall) = adjoin(rng, w, d, &rooms)?;
        max_mouths[from] += 1;
        let (cx, cz) = rooms[from].center();
        let (lat0, wall_line) = match wall {
            WALL_E => (cz, room.x),
            WALL_W => (cz, rooms[from].x),
            WALL_S => (cx, room.z),
            _ => (cx, rooms[from].z),
        };
        doorways.push(InteriorDoorSpec::new(
            wall,
            lat0,
            DOORWAY,
            wall_line,
            from,
            is_locked_depth(depth),
        ));
        corridors.push((from, from + 1, wall == WALL_E || wall == WALL_W));
        rooms.push(room);
    }

    // A chamber set down later may have swallowed part of a passage.
    let short = |&(a, b, x_first): &Corridor| {
        corridor_cells(rooms[a].center(), rooms[b].center(), x_first)
            .filter(|&(x, z)| !rooms.iter().any(|r| r.contains(x, z)))
            .count()
            < 2 * PASSAGE_MIN
    };
    if passages.iter().any(short) {
        return None;
    }

    let ways: Vec<Room> = corridors
        .iter()
        .flat_map(|&(a, b, x_first)| corridor_legs(rooms[a].center(), rooms[b].center(), x_first))
        .collect();
    let end_room = rooms.len() - 1;
    // The hall keeps rock pillars, which break the boss's line of sight.
    let chambers = rooms.len() - usize::from(is_last);
    let pillars = if is_last {
        hall_pillars(&rooms[end_room]).collect()
    } else {
        Vec::new()
    };
    let plan = FloorPlan {
        rooms,
        corridors,
        end_room,
        down_along_z,
        extra_spawns: 0,
        max_mouths: Some(max_mouths),
        pillars,
        doorways,
    };
    let mut layout = finish_floor(rng, depth, is_last, up_shaft, table, plan)?;

    let shafts = [Some(up_shaft), layout.down_shaft].map(|s| s.map(|s| s.rect().expanded(1)));
    let mut taken: Vec<(i32, i32)> = layout
        .props
        .iter()
        .flat_map(PropSpec::cells)
        .chain(layout.spawns.iter().map(|s| (s.x, s.z)))
        .collect();
    // A column keeps a cell clear all round it.
    let crowded = |spec: &PropSpec, taken: &[(i32, i32)]| {
        let (w, d) = spec.size();
        let around = Room {
            x: spec.x,
            z: spec.z,
            w,
            d,
        }
        .expanded(1);
        shafts.iter().flatten().any(|s| around.intersects(s))
            || ways.iter().any(|leg| around.intersects(leg))
            || taken.iter().any(|&(x, z)| around.contains(x, z))
    };
    let column = |x, z, kind, rotation| PropSpec {
        x,
        z,
        kind,
        stack: 1,
        rotation,
    };
    let mut placer = PropPlacer::new(&layout);
    for (x, z) in layout.rooms[..chambers].iter().flat_map(colonnade) {
        let standing = column(x, z, PropKind::TempleColumn, 0);
        if !crowded(&standing, &taken) && placer.fits(&standing) && placer.place(standing) {
            taken.push((x, z));
        }
    }
    for room in &layout.rooms[..chambers] {
        if rng.gen_range(0..100) >= FALLEN_PCT {
            continue;
        }
        for _ in 0..FALLEN_ATTEMPTS {
            let rotation = if rng.gen_range(0..2) == 1 { 90 } else { 0 };
            let (w, d) = column(0, 0, PropKind::TempleFallenColumn, rotation).size();
            let fallen = column(
                rng.gen_range(room.x + 2..=room.x + room.w - 2 - w),
                rng.gen_range(room.z + 2..=room.z + room.d - 2 - d),
                PropKind::TempleFallenColumn,
                rotation,
            );
            if !crowded(&fallen, &taken) && placer.fits(&fallen) && placer.place(fallen) {
                break;
            }
        }
    }
    let columns = placer.placed;
    layout.props.extend(columns);
    Some(layout)
}

/// A `w`×`d` chamber against a free side of the last one, placed so the
/// straight run from that room's centre enters it clear of its corners.
/// Returns it with the wall of the last room it stands against.
fn adjoin(rng: &mut ChaCha8Rng, w: i32, d: i32, rooms: &[Room]) -> Option<(Room, u8)> {
    let from = rooms.last()?;
    let (cx, cz) = from.center();
    for _ in 0..ROOM_PLACE_ATTEMPTS {
        let side = rng.gen_range(0..4);
        // The 2-wide run sits on the centre line and the one after it.
        let mut slide = |centre: i32, size: i32| {
            range_pick(
                rng,
                centre + DOORWAY + DOOR_INSET - size,
                centre - DOOR_INSET,
            )
        };
        let (x, z, wall) = match side {
            0 => (from.x + from.w, slide(cz, d)?, WALL_E),
            1 => (from.x - w, slide(cz, d)?, WALL_W),
            2 => (slide(cx, w)?, from.z + from.d, WALL_S),
            _ => (slide(cx, w)?, from.z - d, WALL_N),
        };
        let room = Room { x, z, w, d };
        if x < 1 || z < 1 || x + w > GRID - 1 || z + d > GRID - 1 {
            continue;
        }
        if rooms.iter().any(|r| room.intersects(r)) {
            continue;
        }
        return Some((room, wall));
    }
    None
}

/// The cells of the columns lining a chamber's long sides.
fn colonnade(room: &Room) -> Vec<(i32, i32)> {
    let wide = room.w >= room.d;
    let (long, short) = if wide {
        (room.w, room.d)
    } else {
        (room.d, room.w)
    };
    let rows = [COLUMN_INSET, short - COLUMN_INSET - 1];
    (COLUMN_INSET..long - COLUMN_INSET)
        .step_by(COLUMN_STEP as usize)
        .flat_map(|along| rows.map(|across| (along, across)))
        .map(|(along, across)| {
            if wide {
                (room.x + along, room.z + across)
            } else {
                (room.x + across, room.z + along)
            }
        })
        .collect()
}
