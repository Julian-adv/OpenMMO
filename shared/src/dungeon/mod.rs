//! Procedural nethack-style dungeon generation, shared verbatim between
//! the native server and the WASM web client. Layouts are fully
//! deterministic from a seed (derived from the entrance id), so neither
//! side ever sends geometry over the wire — both regenerate the same
//! rooms, corridors and stair shafts and only entity state is networked.
//!
//! Determinism rules (enforced by the golden-hash test in `tests`):
//! - RNG is `ChaCha8Rng` only. `SmallRng` is a different algorithm on
//!   wasm32 vs 64-bit native and must never be used here.
//! - No `HashMap`/`HashSet` iteration, no platform-dependent float math
//!   in anything that influences the layout.
//!
//! Coordinate model: each floor is a `GRID`×`GRID` field of 1m cells
//! centered on the dungeon entrance. Depth `d` (1-based) lives at world
//! `y = entrance.y - d * DUNGEON_FLOOR_HEIGHT` and registers in the
//! passability cache as floor index `passability_floor_for_depth(d)`.
//! The offset keeps dungeon floors clear of housing floor levels 0-3 —
//! `is_cardinal_move_blocked` matches by floor index alone, so reusing
//! 0..3 would make dungeon walls block players walking on the surface
//! above the dungeon footprint.
//!
//! Floors connect through 2×`SHAFT_LEN` stair shafts that occupy the
//! same cells on both adjacent floors; the entry landing belongs to the
//! shallower floor, the exit landing to the deeper one, and A* walks
//! them via the existing housing stairwell intermediate-key machinery.

mod gen;
#[cfg(test)]
mod tests;

use serde::Serialize;

use crate::pathfinding::{
    RuntimeFloorGrid, RuntimePassability, StairwellInfo, EDGE_E, EDGE_N, EDGE_S, EDGE_W,
};
use crate::world::Position;

/// Side length of a dungeon floor in 1m cells.
pub const GRID: i32 = 56;
const HALF_GRID: i32 = GRID / 2;

/// Vertical distance between consecutive dungeon floors.
pub const DUNGEON_FLOOR_HEIGHT: f32 = 4.0;

/// Collision wall height registered in the passability grids. Kept below
/// `DUNGEON_FLOOR_HEIGHT` so the depth-1 Y window tops out 1m under the
/// entrance and never captures players walking on the surface above.
pub const DUNGEON_WALL_HEIGHT: f32 = 3.0;

/// Passability floor index of depth 1. Housing uses 0..=3; dungeon depths
/// map to 4..=23 so the two systems can never collide in floor-keyed
/// collision queries.
pub const DUNGEON_FLOOR_INDEX_BASE: u8 = 4;

pub const MIN_DEPTH: u8 = 5;
pub const MAX_DEPTH: u8 = 20;

/// Stair shaft footprint: `SHAFT_W` cells wide, `SHAFT_LEN` along the run.
/// The run must stay ≤ 16 cells — stairwell intermediate floor keys get
/// `FLOOR_SCALE = 16` slots between two regular floors.
pub const SHAFT_W: i32 = 2;
pub const SHAFT_LEN: i32 = 8;

/// Monster type spawned on the final floor next to the treasure chest.
pub const BOSS_MONSTER_TYPE: &str = "orc_boss";

/// A* node budget for long in-dungeon path queries. Maze floors plus the
/// open-surface leak through the entrance stairwell can exhaust the
/// housing default (2000) on cross-floor routes; short chase paths are
/// unaffected. Searches that exhaust the budget still return a partial
/// path toward the goal.
pub const DUNGEON_PATH_MAX_NODES: usize = 20000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Room {
    pub x: i32,
    pub z: i32,
    pub w: i32,
    pub d: i32,
}

impl Room {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.w / 2, self.z + self.d / 2)
    }

    fn expanded(&self, by: i32) -> Room {
        Room {
            x: self.x - by,
            z: self.z - by,
            w: self.w + by * 2,
            d: self.d + by * 2,
        }
    }

    fn intersects(&self, other: &Room) -> bool {
        self.x < other.x + other.w
            && self.x + self.w > other.x
            && self.z < other.z + other.d
            && self.z + self.d > other.z
    }
}

/// A vertical stair shaft connecting two adjacent floors. The footprint
/// is identical on both floors; `reversed` selects which physical end is
/// the entry landing (on the shallower floor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StairShaft {
    /// Min-corner cell of the footprint.
    pub x: i32,
    pub z: i32,
    /// Run axis: true = along +Z, false = along +X.
    pub along_z: bool,
    /// false = entry (shallow) landing at the min end, true = at the max end.
    pub reversed: bool,
}

impl StairShaft {
    pub fn rect(&self) -> Room {
        if self.along_z {
            Room {
                x: self.x,
                z: self.z,
                w: SHAFT_W,
                d: SHAFT_LEN,
            }
        } else {
            Room {
                x: self.x,
                z: self.z,
                w: SHAFT_LEN,
                d: SHAFT_W,
            }
        }
    }

    pub fn contains(&self, x: i32, z: i32) -> bool {
        let r = self.rect();
        x >= r.x && x < r.x + r.w && z >= r.z && z < r.z + r.d
    }

    /// Cell at run position `i` (0 = entry end), lateral offset `w`.
    pub fn step_cell(&self, i: i32, w: i32) -> (i32, i32) {
        let run = if self.reversed { SHAFT_LEN - 1 - i } else { i };
        if self.along_z {
            (self.x + w, self.z + run)
        } else {
            (self.x + run, self.z + w)
        }
    }

    /// Entry landing cell (shallower floor), first lateral column.
    pub fn entry_cell(&self) -> (i32, i32) {
        self.step_cell(0, 0)
    }

    /// Exit landing cell (deeper floor), first lateral column.
    pub fn exit_cell(&self) -> (i32, i32) {
        self.step_cell(SHAFT_LEN - 1, 0)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnSpec {
    pub x: i32,
    pub z: i32,
    pub monster_type: String,
    pub is_boss: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorLayout {
    /// 1-based depth below the surface.
    pub depth: u8,
    pub rooms: Vec<Room>,
    /// GRID*GRID row-major walkability (rooms + corridors + shafts).
    pub carved: Vec<bool>,
    /// Shaft arriving from the floor above (or from the surface at depth 1).
    pub up_shaft: StairShaft,
    /// Shaft descending to the next floor; `None` on the final floor.
    pub down_shaft: Option<StairShaft>,
    /// Treasure chest cell, only on the final floor.
    pub chest: Option<(i32, i32)>,
    pub spawns: Vec<SpawnSpec>,
}

impl FloorLayout {
    pub fn is_carved(&self, x: i32, z: i32) -> bool {
        x >= 0 && x < GRID && z >= 0 && z < GRID && self.carved[(x + z * GRID) as usize]
    }
}

/// FNV-1a 64 over the entrance id. Implemented inline because
/// `DefaultHasher` is not stable across Rust releases and the seed must
/// match between independently-built server and client binaries.
pub fn dungeon_seed(entrance_id: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in entrance_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Total floor count for a dungeon, 5..=20, derived from the seed.
pub fn dungeon_depth(seed: u64) -> u8 {
    gen::dungeon_depth(seed)
}

/// Generate every floor of the dungeon. Cheap enough (≤20 grids of 56×56
/// cells) that callers always generate the full dungeon and index into it.
pub fn generate_dungeon(seed: u64) -> Vec<FloorLayout> {
    gen::generate_dungeon(seed)
}

pub fn passability_floor_for_depth(depth: u8) -> u8 {
    DUNGEON_FLOOR_INDEX_BASE + depth - 1
}

pub fn floor_world_y(entrance_y: f32, depth: u8) -> f32 {
    entrance_y - depth as f32 * DUNGEON_FLOOR_HEIGHT
}

/// World min-corner of the cell grid. Floored so cell edges sit on
/// integer world coordinates like housing grids do.
pub fn dungeon_origin(entrance_x: f32, entrance_z: f32) -> (f32, f32) {
    (
        entrance_x.floor() - HALF_GRID as f32,
        entrance_z.floor() - HALF_GRID as f32,
    )
}

/// World-space center of a grid cell.
pub fn cell_center(entrance: &Position, depth: u8, cell: (i32, i32)) -> Position {
    let (ox, oz) = dungeon_origin(entrance.x, entrance.z);
    Position {
        x: ox + cell.0 as f32 + 0.5,
        y: floor_world_y(entrance.y, depth),
        z: oz + cell.1 as f32 + 0.5,
    }
}

/// Passability cache key for a dungeon (one entry covers every floor).
pub fn dungeon_cache_key(entrance_id: &str) -> String {
    format!("dungeon:{entrance_id}")
}

/// Weighted monster table per depth. Types must exist in monsters.csv.
pub fn spawn_table(depth: u8) -> &'static [(&'static str, u32)] {
    match depth {
        1..=2 => &[("kobold", 1)],
        3..=4 => &[("kobold", 2), ("goblin", 3)],
        5..=8 => &[("goblin", 3), ("scp939", 2), ("orc", 2), ("orc_female", 1)],
        _ => &[("orc", 3), ("orc_female", 2), ("scp939", 1)],
    }
}

/// Effective monster level at a given depth. Shallow floors use the
/// definition level untouched; below depth 4 monsters gain +1 level per
/// two floors, capped at 20.
pub fn monster_level_for_depth(def_level: u8, depth: u8) -> u8 {
    if depth <= 4 {
        def_level
    } else {
        (def_level as u32 + (depth as u32 - 4) / 2).min(20) as u8
    }
}

/// Edge-bitmask cells for one floor, derived from its carved mask plus
/// explicit side walls along stair shafts (so descending players can't
/// step sideways off the stairs into a flush room).
pub fn floor_passability_cells(layout: &FloorLayout) -> Vec<u8> {
    let mut cells = vec![0u8; (GRID * GRID) as usize];

    for z in 0..GRID {
        for x in 0..GRID {
            if !layout.is_carved(x, z) {
                continue;
            }
            let idx = (x + z * GRID) as usize;
            if !layout.is_carved(x, z - 1) {
                cells[idx] |= EDGE_N;
            }
            if !layout.is_carved(x, z + 1) {
                cells[idx] |= EDGE_S;
            }
            if !layout.is_carved(x + 1, z) {
                cells[idx] |= EDGE_E;
            }
            if !layout.is_carved(x - 1, z) {
                cells[idx] |= EDGE_W;
            }
        }
    }

    let mut wall_shaft_sides = |shaft: &StairShaft| {
        let r = shaft.rect();
        let set = |cells: &mut Vec<u8>, x: i32, z: i32, bit: u8| {
            if x >= 0 && x < GRID && z >= 0 && z < GRID {
                cells[(x + z * GRID) as usize] |= bit;
            }
        };
        // Skip the deep (lower) landing's run cell: it sits at floor level
        // inside the room that wraps the shaft, so the arriving player steps
        // sideways off the stairs there. Walling it (like the steps above)
        // would trap them at the bottom. The shallow end and all steps stay
        // walled so descending players can't step into a flush room mid-run.
        if shaft.along_z {
            let exit_z = if shaft.reversed { r.z } else { r.z + r.d - 1 };
            for z in r.z..r.z + r.d {
                if z == exit_z {
                    continue;
                }
                set(&mut cells, r.x, z, EDGE_W);
                set(&mut cells, r.x - 1, z, EDGE_E);
                set(&mut cells, r.x + r.w - 1, z, EDGE_E);
                set(&mut cells, r.x + r.w, z, EDGE_W);
            }
        } else {
            let exit_x = if shaft.reversed { r.x } else { r.x + r.w - 1 };
            for x in r.x..r.x + r.w {
                if x == exit_x {
                    continue;
                }
                set(&mut cells, x, r.z, EDGE_N);
                set(&mut cells, x, r.z - 1, EDGE_S);
                set(&mut cells, x, r.z + r.d - 1, EDGE_S);
                set(&mut cells, x, r.z + r.d, EDGE_N);
            }
        }
    };

    wall_shaft_sides(&layout.up_shaft);
    if let Some(ref down) = layout.down_shaft {
        wall_shaft_sides(down);
    }

    cells
}

/// Build the runtime passability entry covering every floor of the
/// dungeon, including the surface-entrance stairwell (floor 0 → depth 1)
/// and one stairwell per inter-floor shaft. Register it under
/// `dungeon_cache_key(..)` in the same cache houses live in; all existing
/// collision/A* queries then work unchanged.
pub fn dungeon_passability(entrance: &Position, layouts: &[FloorLayout]) -> RuntimePassability {
    let (ox, oz) = dungeon_origin(entrance.x, entrance.z);

    let floors: Vec<RuntimeFloorGrid> = layouts
        .iter()
        .map(|layout| RuntimeFloorGrid {
            floor_level: passability_floor_for_depth(layout.depth),
            origin_x: 0,
            origin_z: 0,
            width: GRID as u8,
            depth: GRID as u8,
            y_base: floor_world_y(entrance.y, layout.depth),
            wall_height: DUNGEON_WALL_HEIGHT,
            cells: floor_passability_cells(layout),
        })
        .collect();

    let shaft_info = |shaft: &StairShaft, lower_floor: u8, upper_floor: u8| {
        let r = shaft.rect();
        StairwellInfo {
            local_min_x: r.x,
            local_min_z: r.z,
            local_max_x: r.x + r.w,
            local_max_z: r.z + r.d,
            lower_floor,
            upper_floor,
            along_z: shaft.along_z,
            reversed: shaft.reversed,
        }
    };

    let mut stairwells = Vec::new();
    if let Some(first) = layouts.first() {
        // Surface (floor 0) down to depth 1. In the stairwell encoding
        // "lower" is the entry end: i=0 lands on the surface.
        stairwells.push(shaft_info(
            &first.up_shaft,
            0,
            passability_floor_for_depth(1),
        ));
    }
    for layout in layouts {
        if let Some(ref down) = layout.down_shaft {
            stairwells.push(shaft_info(
                down,
                passability_floor_for_depth(layout.depth),
                passability_floor_for_depth(layout.depth + 1),
            ));
        }
    }

    RuntimePassability {
        house_origin_x: ox,
        house_origin_z: oz,
        min_x: ox,
        max_x: ox + GRID as f32,
        min_z: oz,
        max_z: oz + GRID as f32,
        floors,
        stairwells,
    }
}
