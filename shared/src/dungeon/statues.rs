//! Where a floor stands the statues of those turned to stone on it
//! (doc/STONE_GARDEN.md 돌이 된 플레이어의 석상).

use super::gen::{cell_in_any_room, cell_in_any_shaft, cell_in_stair_room, wall_sides};
use super::{FloorLayout, GRID};

const STATUES_PER_FLOOR: usize = 4;
const STATUES_IN_HALL: usize = 12;

/// How many statues a floor keeps; older ones make way.
pub fn statue_capacity(layout: &FloorLayout) -> usize {
    if layout.chest.is_some() {
        STATUES_IN_HALL
    } else {
        STATUES_PER_FLOOR
    }
}

/// The floor's statue cells, spread along its room walls: the same cells for
/// the same layout, clear of stairs, props, the chest and the boss.
pub fn statue_cells(layout: &FloorLayout) -> Vec<(i32, i32)> {
    let near = |cell: (i32, i32), x: i32, z: i32, reach: i32| {
        (cell.0 - x).abs() <= reach && (cell.1 - z).abs() <= reach
    };
    let taken = |x: i32, z: i32| {
        layout.props.iter().any(|p| {
            let (w, d) = p.size();
            (p.x - 1..p.x + w + 1).contains(&x) && (p.z - 1..p.z + d + 1).contains(&z)
        }) || layout.chest.is_some_and(|c| near(c, x, z, 2))
            || layout.spawns.iter().any(|s| near((s.x, s.z), x, z, 1))
    };
    let mut cells = Vec::new();
    for z in 0..GRID {
        for x in 0..GRID {
            if layout.is_carved(x, z)
                && cell_in_any_room(layout, x, z)
                && !cell_in_stair_room(layout, x, z)
                && !cell_in_any_shaft(layout, x, z)
                && wall_sides(layout, x, z) == 1
                && !taken(x, z)
            {
                cells.push((x, z));
            }
        }
    }
    let count = statue_capacity(layout).min(cells.len());
    // Evenly through the scan, started a little further in on each floor.
    let offset = usize::from(layout.depth) * 7;
    (0..count)
        .map(|i| cells[(offset + i * cells.len() / count) % cells.len()])
        .collect()
}
