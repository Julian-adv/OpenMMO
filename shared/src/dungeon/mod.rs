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
use std::sync::LazyLock;

use crate::pathfinding::{
    RuntimeFloorGrid, RuntimePassability, StairwellInfo, EDGE_E, EDGE_N, EDGE_S, EDGE_W,
};
use crate::world::Position;

/// Side length of a dungeon floor in 1m cells.
pub const GRID: i32 = 80;
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

/// How far (in cells, Chebyshev distance) to search outward from a kill
/// for walkable floor when a loot drop would otherwise land in a wall.
/// Rooms are larger than this, so a carved cell is always found well
/// before the limit; it's only a guard against pathological geometry.
const DROP_SEARCH_RING_MAX: i32 = 4;

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
    /// Proactive (선공형) monster: attacks players on sight instead of only
    /// retaliating when hit. Designated per entry in [`spawn_table`].
    pub aggressive: bool,
}

/// Decorative clutter prop dropped into a room. Purely cosmetic — like the
/// treasure chest it carries no collision (it's never added to the passability
/// grid), so placement only has to read well, not gate movement. The string
/// variants match the object-catalog ids (`barrel`/`crate`/`chest`) the client
/// loads the GLB for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PropKind {
    Barrel,
    Crate,
    Chest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropSpec {
    pub x: i32,
    pub z: i32,
    pub kind: PropKind,
    /// How many of `kind` are stacked vertically (1 or 2). Chests never stack.
    pub stack: u8,
    /// Yaw in whole degrees (0..360); jitters the model for variety.
    pub rotation: u16,
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
    /// Decorative barrels/crates/chests clustered in room corners. Cosmetic
    /// only (no collision) — see [`PropSpec`].
    pub props: Vec<PropSpec>,
}

impl FloorLayout {
    pub fn is_carved(&self, x: i32, z: i32) -> bool {
        x >= 0 && x < GRID && z >= 0 && z < GRID && self.carved[(x + z * GRID) as usize]
    }

    /// Pick a walkable world position to drop loot near a monster's death
    /// spot, so the item never lands inside a wall. Pickup is a pure
    /// proximity check (no pathfinding), so an item in an uncarved cell can
    /// be unreachable — a player can never get close enough through the
    /// wall.
    ///
    /// `preferred` is the desired scatter point (the death position plus a
    /// random offset). If it already sits on carved floor it's kept as-is.
    /// Otherwise the search widens in Chebyshev rings around the *death*
    /// cell and snaps to the carved cell whose center is nearest `preferred`,
    /// finally falling back to the death cell itself — the monster stood
    /// there, so it is carved.
    pub fn walkable_drop_position(
        &self,
        entrance: &Position,
        death: &Position,
        preferred: &Position,
    ) -> Position {
        let surface_y = floor_world_y(entrance.y, self.depth);

        // 1. Keep the scattered point when it already lands on floor.
        let (px, pz) = world_to_cell(entrance, preferred.x, preferred.z);
        if self.is_carved(px, pz) {
            return Position {
                x: preferred.x,
                y: surface_y,
                z: preferred.z,
            };
        }

        // 2. Widen outward from the death cell; at each ring snap to the
        //    carved cell whose center is closest to the preferred point so
        //    the drop still trends in the scatter direction.
        let (dcx, dcz) = world_to_cell(entrance, death.x, death.z);
        for ring in 1..=DROP_SEARCH_RING_MAX {
            let mut best: Option<((i32, i32), f32)> = None;
            for cz in (dcz - ring)..=(dcz + ring) {
                for cx in (dcx - ring)..=(dcx + ring) {
                    // Only the perimeter cells are new at this ring.
                    if (cx - dcx).abs() != ring && (cz - dcz).abs() != ring {
                        continue;
                    }
                    if !self.is_carved(cx, cz) {
                        continue;
                    }
                    let c = cell_center(entrance, self.depth, (cx, cz));
                    let d2 = (c.x - preferred.x).powi(2) + (c.z - preferred.z).powi(2);
                    if best.is_none_or(|(_, b)| d2 < b) {
                        best = Some(((cx, cz), d2));
                    }
                }
            }
            if let Some((cell, _)) = best {
                return cell_center(entrance, self.depth, cell);
            }
        }

        // 3. Fallback: the death cell itself (the monster occupied it).
        cell_center(entrance, self.depth, (dcx, dcz))
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

/// Grid cell containing a world-space XZ position (inverse of `cell_center`).
pub fn world_to_cell(entrance: &Position, x: f32, z: f32) -> (i32, i32) {
    let (ox, oz) = dungeon_origin(entrance.x, entrance.z);
    ((x - ox).floor() as i32, (z - oz).floor() as i32)
}

/// Passability cache key for a dungeon (one entry covers every floor).
pub fn dungeon_cache_key(entrance_id: &str) -> String {
    format!("dungeon:{entrance_id}")
}

/// One weighted entry in a depth's spawn table. `aggressive` makes that dungeon
/// spawn proactive (선공형 — attacks on sight). Sourced per monster from the
/// `dungeonMinDepth`/`dungeonMaxDepth`/`dungeonWeight`/`dungeonAggressive`
/// columns of `data-src/monsters.csv`.
#[derive(Debug, Clone)]
pub struct SpawnEntry {
    pub monster_type: String,
    pub weight: u32,
    pub aggressive: bool,
}

/// Per-depth spawn tables indexed by depth (`0..=MAX_DEPTH`), built once from
/// the monster table. The dungeon generator runs in the shared crate on both
/// native (server) and wasm32 (client), so the data is baked in at compile
/// time via `include_str!` — runtime file IO would risk desync. We read the
/// SOURCE csv directly (not the generated `data/monsters.json`) because that
/// JSON is produced by a build script whose ordering relative to this crate
/// isn't guaranteed; reading the csv keeps a `cargo build` after a csv edit
/// self-consistent. Entries stay in csv row order — stable and identical on
/// both sides — which the weighted pick in `roll_spawns` relies on.
static SPAWN_TABLES: LazyLock<Vec<Vec<SpawnEntry>>> =
    LazyLock::new(|| build_spawn_tables(include_str!("../../../data-src/monsters.csv")));

fn build_spawn_tables(csv: &str) -> Vec<Vec<SpawnEntry>> {
    let mut tables: Vec<Vec<SpawnEntry>> = vec![Vec::new(); MAX_DEPTH as usize + 1];
    let mut lines = csv.lines();
    let Some(header) = lines.next() else {
        return tables;
    };
    let cols: Vec<&str> = header.split(',').map(str::trim).collect();
    let col = |name: &str| {
        cols.iter()
            .position(|c| *c == name)
            .unwrap_or_else(|| panic!("monsters.csv missing `{name}` column"))
    };
    let (id_col, min_col, max_col, weight_col, aggr_col) = (
        col("id"),
        col("dungeonMinDepth"),
        col("dungeonMaxDepth"),
        col("dungeonWeight"),
        col("dungeonAggressive"),
    );

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split(',').collect();
        let field = |i: usize| fields.get(i).map(|s| s.trim()).unwrap_or("");
        // Only rows with a min depth are dungeon spawns; the boss is placed
        // separately (see `roll_spawns`) and leaves these columns blank.
        let Ok(min) = field(min_col).parse::<u8>() else {
            continue;
        };
        // Blank (or anything past the deepest floor) means "down to the
        // bottom"; values above MAX_DEPTH clamp rather than extend it.
        let max = field(max_col)
            .parse::<u8>()
            .unwrap_or(MAX_DEPTH)
            .min(MAX_DEPTH);
        let weight = field(weight_col).parse::<u32>().unwrap_or(1).max(1);
        let entry = SpawnEntry {
            monster_type: field(id_col).to_string(),
            weight,
            aggressive: field(aggr_col) == "true",
        };
        for depth in min..=max {
            tables[depth as usize].push(entry.clone());
        }
    }
    tables
}

/// Weighted monster entries that can spawn at `depth`, in stable csv order, or
/// an empty slice if none cover it. Tune via the `dungeon*` columns of
/// monsters.csv.
pub fn spawn_table(depth: u8) -> &'static [SpawnEntry] {
    SPAWN_TABLES
        .get(depth as usize)
        .map(Vec::as_slice)
        .unwrap_or(&[])
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

/// All four edge bits set: a fully sealed, impassable cell. Used to turn a
/// decorative prop's cell into a 1×1 collision pillar (see `roll_props`).
pub(crate) const EDGE_ALL: u8 = EDGE_N | EDGE_E | EDGE_S | EDGE_W;

/// Edge-bitmask cells for one floor, derived from its carved mask plus walls
/// turning each stair shaft into a dead-end whose only opening on this floor is
/// the landing this floor stands on. The shaft footprint sits inside a room, so
/// without the far landing's run-end walled, A* treats it as a cut-through and
/// marches a same-floor monster across it onto steps that render at the *other*
/// floor's height; the side walls also keep a descending player from stepping
/// sideways off the stairs mid-run. The steps themselves stay open along the run
/// so a descending player (collision-checked against this floor once their Y
/// drops into range) still walks through.
pub fn floor_passability_cells(layout: &FloorLayout) -> Vec<u8> {
    floor_passability_cells_inner(layout, &[])
}

/// Like [`floor_passability_cells`] but leaves the cells of broken props open.
/// `broken` holds indices into `layout.props` whose 1×1 collision pillar has
/// been destroyed, restoring movement through them. Recomputing from the carved
/// mask (rather than clearing the cell's edges) keeps any genuine wall the prop
/// sat against intact. Used to rebuild a floor's passability after a prop breaks.
pub fn floor_passability_cells_with_broken(layout: &FloorLayout, broken: &[u32]) -> Vec<u8> {
    floor_passability_cells_inner(layout, broken)
}

fn floor_passability_cells_inner(layout: &FloorLayout, broken: &[u32]) -> Vec<u8> {
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

    // Wall a shaft so its footprint is a dead-end on this floor, opening only
    // at the landing this floor stands on. `legit_is_exit` is true for the
    // up-shaft (you arrive at and step off its deep/exit landing) and false for
    // the down-shaft (you step onto its shallow/entry landing to descend).
    //   * lateral side walls keep a descending player from sidestepping off the
    //     run into a flush room — the up-shaft exit landing is the only row left
    //     open sideways, so you can step out at the bottom;
    //   * the *far* landing (the one belonging to the other floor) gets its
    //     room-facing run-end walled too. That's the one new opening the old
    //     code left, and it let same-floor A* march a monster straight across
    //     the footprint and onto steps that render at the other floor's height.
    // The steps in between stay open along the run on purpose: a descending
    // player is collision-checked against *this* floor's grid once their Y
    // drops into its range (~a quarter of the way down, see `is_movement_blocked`
    // floor selection), so the run must stay walkable. Cross-floor pathfinding
    // is unaffected either way — it walks the shaft via the stairwell expansion,
    // which ignores these edge bits.
    let mut wall_shaft = |shaft: &StairShaft, legit_is_exit: bool| {
        let r = shaft.rect();
        let set = |cells: &mut Vec<u8>, x: i32, z: i32, bit: u8| {
            if x >= 0 && x < GRID && z >= 0 && z < GRID {
                cells[(x + z * GRID) as usize] |= bit;
            }
        };
        if shaft.along_z {
            let exit_z = shaft.exit_cell().1;
            let far_z = if legit_is_exit {
                shaft.entry_cell().1
            } else {
                exit_z
            };
            for z in r.z..r.z + r.d {
                if !(legit_is_exit && z == exit_z) {
                    set(&mut cells, r.x, z, EDGE_W);
                    set(&mut cells, r.x - 1, z, EDGE_E);
                    set(&mut cells, r.x + r.w - 1, z, EDGE_E);
                    set(&mut cells, r.x + r.w, z, EDGE_W);
                }
            }
            // Far landing's outer run-end (points away from the steps, into the
            // wrapping room).
            for x in r.x..r.x + r.w {
                if far_z == r.z {
                    set(&mut cells, x, far_z, EDGE_N);
                    set(&mut cells, x, far_z - 1, EDGE_S);
                } else {
                    set(&mut cells, x, far_z, EDGE_S);
                    set(&mut cells, x, far_z + 1, EDGE_N);
                }
            }
        } else {
            let exit_x = shaft.exit_cell().0;
            let far_x = if legit_is_exit {
                shaft.entry_cell().0
            } else {
                exit_x
            };
            for x in r.x..r.x + r.w {
                if !(legit_is_exit && x == exit_x) {
                    set(&mut cells, x, r.z, EDGE_N);
                    set(&mut cells, x, r.z - 1, EDGE_S);
                    set(&mut cells, x, r.z + r.d - 1, EDGE_S);
                    set(&mut cells, x, r.z + r.d, EDGE_N);
                }
            }
            for z in r.z..r.z + r.d {
                if far_x == r.x {
                    set(&mut cells, far_x, z, EDGE_W);
                    set(&mut cells, far_x - 1, z, EDGE_E);
                } else {
                    set(&mut cells, far_x, z, EDGE_E);
                    set(&mut cells, far_x + 1, z, EDGE_W);
                }
            }
        }
    };

    wall_shaft(&layout.up_shaft, true);
    if let Some(ref down) = layout.down_shaft {
        wall_shaft(down, false);
    }

    // Decorative props are solid obstacles: seal every edge of their cell so a
    // player or monster can neither enter nor leave it. Sealing the prop cell
    // alone is enough — the move check ORs both cells' edge bits, so it becomes
    // a 1×1 pillar. `prop_cell_ok` keeps props off corridor mouths and stair
    // landings, but those local rules can't guarantee global connectivity on
    // their own, so `roll_props` runs a reachability backstop (with each prop's
    // seal applied here) and rejects any prop that would close a route.
    for (i, p) in layout.props.iter().enumerate() {
        if broken.contains(&(i as u32)) {
            continue;
        }
        if p.x >= 0 && p.x < GRID && p.z >= 0 && p.z < GRID {
            cells[(p.x + p.z * GRID) as usize] |= EDGE_ALL;
        }
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
