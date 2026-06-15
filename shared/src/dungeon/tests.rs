//! Generator invariants over many seeds, plus a golden hash. The golden
//! test makes any algorithm drift an explicit, reviewed event: client and
//! server regenerate layouts independently from the same seed, so a
//! silent change desyncs deployed builds.

use super::*;
use crate::pathfinding::{find_and_smooth_path, PassabilityCache};

fn fnv1a64_bytes(h: &mut u64, bytes: &[u8]) {
    for b in bytes {
        *h ^= *b as u64;
        *h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

fn layout_hash(floors: &[FloorLayout]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for f in floors {
        fnv1a64_bytes(&mut h, &format!("{f:?}").into_bytes());
    }
    h
}

fn test_entrance() -> Position {
    Position {
        x: 100.0,
        y: 10.0,
        z: 200.0,
    }
}

#[test]
fn depth_in_range() {
    for seed in 0..500u64 {
        let d = dungeon_depth(seed);
        assert!(
            (MIN_DEPTH..=MAX_DEPTH).contains(&d),
            "seed {seed}: depth {d}"
        );
    }
}

#[test]
fn determinism_two_runs_identical() {
    for seed in [0u64, 1, 42, 0xdead_beef, u64::MAX] {
        let a = generate_dungeon(seed);
        let b = generate_dungeon(seed);
        assert_eq!(layout_hash(&a), layout_hash(&b), "seed {seed}");
        assert_eq!(format!("{a:?}"), format!("{b:?}"), "seed {seed}");
    }
}

/// Golden hash: if this fails you changed the generation algorithm (or
/// the RNG/dependency versions feeding it). That desyncs every deployed
/// client against the server — bump deliberately and deploy both sides
/// together.
#[test]
fn golden_layout_hash() {
    let floors = generate_dungeon(dungeon_seed("old_crypt"));
    let h = layout_hash(&floors);
    assert_eq!(
        h, GOLDEN_OLD_CRYPT_HASH,
        "dungeon generation drifted: got {h:#x}"
    );
}

// Captured from the first blessed run; see golden_layout_hash. Re-blessed when
// the spawn table moved from depth-band arrays (dungeon_spawns.json) to the
// per-monster `dungeon*` columns of monsters.csv: same monster presence per
// depth, but weighted-selection order now follows csv row order, so the picks
// (and thus the hash) shift. Re-blessed again when floor 1 spawn count dropped
// to 3..=5 (from the uniform 5..=9): the smaller draw shifts depth-1 RNG order.
const GOLDEN_OLD_CRYPT_HASH: u64 = 0xa091_5202_fbc3_b743;

#[test]
fn structure_invariants_many_seeds() {
    for seed in 0..200u64 {
        let floors = generate_dungeon(seed);
        let total = floors.len() as u8;
        assert!((1..=MAX_DEPTH).contains(&total), "seed {seed}");

        for (i, f) in floors.iter().enumerate() {
            let depth = i as u8 + 1;
            assert_eq!(f.depth, depth, "seed {seed}");
            assert!(
                (1..=5).contains(&f.rooms.len()),
                "seed {seed} depth {depth}: {} rooms",
                f.rooms.len()
            );

            // Rooms in bounds, non-overlapping (fallback floors have 1 room).
            for r in &f.rooms {
                assert!(r.x >= 1 && r.z >= 1 && r.x + r.w <= GRID - 1 && r.z + r.d <= GRID - 1);
            }

            // Shaft alignment: this floor's down shaft is the next floor's up shaft.
            if let Some(down) = f.down_shaft {
                assert!(
                    depth < total,
                    "seed {seed}: down shaft on final floor {depth}"
                );
                assert_eq!(
                    floors[i + 1].up_shaft,
                    down,
                    "seed {seed} depth {depth}: shaft misaligned"
                );
            } else {
                assert_eq!(depth, total, "seed {seed}: dead end at {depth}/{total}");
                assert!(f.chest.is_some(), "seed {seed}: final floor missing chest");
                assert!(
                    f.spawns.iter().any(|s| s.is_boss),
                    "seed {seed}: final floor missing boss"
                );
            }

            // Shaft footprints carved on this floor.
            let mut shaft_cells = shaft_footprint(&f.up_shaft);
            if let Some(ref d) = f.down_shaft {
                shaft_cells.extend(shaft_footprint(d));
            }
            for (x, z) in &shaft_cells {
                assert!(
                    f.is_carved(*x, *z),
                    "seed {seed} depth {depth}: shaft cell uncarved"
                );
            }

            // Spawns on carved, non-shaft cells.
            for s in &f.spawns {
                assert!(
                    f.is_carved(s.x, s.z),
                    "seed {seed} depth {depth}: spawn off-grid"
                );
                if !s.is_boss {
                    assert!(
                        !f.up_shaft.contains(s.x, s.z)
                            && !f
                                .down_shaft
                                .as_ref()
                                .is_some_and(|sh| sh.contains(s.x, s.z)),
                        "seed {seed} depth {depth}: spawn in shaft"
                    );
                }
            }
            assert!(!f.spawns.is_empty(), "seed {seed} depth {depth}: no spawns");
        }

        // First floor's up shaft entry lands on the grid center (entrance).
        let entry = floors[0].up_shaft.entry_cell();
        let half = GRID / 2;
        assert!(
            (entry.0 - half).abs() <= 1 && (entry.1 - half).abs() <= 1,
            "seed {seed}: entrance entry cell {entry:?}"
        );
    }
}

fn shaft_footprint(s: &StairShaft) -> Vec<(i32, i32)> {
    let r = s.rect();
    let mut cells = Vec::new();
    for z in r.z..r.z + r.d {
        for x in r.x..r.x + r.w {
            cells.push((x, z));
        }
    }
    cells
}

/// End-to-end pathfinding through the real passability machinery: from
/// the surface entrance, walk down every floor to the chest.
#[test]
fn full_descent_path_through_passability() {
    let entrance = test_entrance();
    for seed in 0..40u64 {
        let floors = generate_dungeon(seed);
        let rp = dungeon_passability(&entrance, &floors);
        let mut cache = PassabilityCache::new();
        cache.insert(dungeon_cache_key("t"), rp);

        // Surface → depth 1 up-shaft exit.
        let surface = cell_center(&entrance, 0, floors[0].up_shaft.entry_cell());
        let arrival = cell_center(&entrance, 1, floors[0].up_shaft.exit_cell());
        let res = find_and_smooth_path(
            surface.x,
            surface.z,
            0,
            arrival.x,
            arrival.z,
            passability_floor_for_depth(1),
            &cache,
            DUNGEON_PATH_MAX_NODES,
        );
        assert!(res.found, "seed {seed}: surface → depth 1 path not found");

        for f in &floors {
            let from = cell_center(&entrance, f.depth, f.up_shaft.exit_cell());
            let floor = passability_floor_for_depth(f.depth);
            let goal_cell = match f.down_shaft {
                Some(ref d) => d.entry_cell(),
                None => f.chest.unwrap(),
            };
            let goal = cell_center(&entrance, f.depth, goal_cell);
            let res = find_and_smooth_path(
                from.x,
                from.z,
                floor,
                goal.x,
                goal.z,
                floor,
                &cache,
                DUNGEON_PATH_MAX_NODES,
            );
            assert!(
                res.found,
                "seed {seed} depth {}: arrival → {} unreachable",
                f.depth,
                if f.down_shaft.is_some() {
                    "down stairs"
                } else {
                    "chest"
                }
            );
        }
    }
}

/// Each stair shaft must be a dead-end on its floor: the only shaft cells that
/// open onto a room cell are this floor's own landing. Otherwise same-floor A*
/// treats the footprint (which sits inside a room) as a cut-through and marches
/// a monster across it onto steps that render at the adjacent floor's height.
/// The steps must *also* stay open along the run, or a descending player —
/// collision-checked against this floor once their Y drops into its range —
/// gets stuck mid-stairs. This guards both failure modes.
#[test]
fn shaft_opens_to_room_only_at_its_landing() {
    use super::{EDGE_E, EDGE_N, EDGE_S, EDGE_W};
    const EDGES: [(i32, i32, u8, u8); 4] = [
        (0, -1, EDGE_N, EDGE_S),
        (0, 1, EDGE_S, EDGE_N),
        (1, 0, EDGE_E, EDGE_W),
        (-1, 0, EDGE_W, EDGE_E),
    ];

    for seed in 0..60u64 {
        let floors = generate_dungeon(seed);
        for layout in &floors {
            let cells = floor_passability_cells(layout);
            let blocked = |x: i32, z: i32, leave: u8, nx: i32, nz: i32, enter: u8| {
                cells[(x + z * GRID) as usize] & leave != 0
                    || cells[(nx + nz * GRID) as usize] & enter != 0
            };

            // Footprint of every shaft on this floor and the live landing rows
            // (SHAFT_W wide) it legitimately stands on.
            let mut shaft = std::collections::HashSet::new();
            let mut landings = std::collections::HashSet::new();
            for (x, z) in shaft_footprint(&layout.up_shaft) {
                shaft.insert((x, z));
            }
            for w in 0..SHAFT_W {
                landings.insert(layout.up_shaft.step_cell(SHAFT_LEN - 1, w));
            }
            if let Some(ref d) = layout.down_shaft {
                for (x, z) in shaft_footprint(d) {
                    shaft.insert((x, z));
                }
                for w in 0..SHAFT_W {
                    landings.insert(d.step_cell(0, w));
                }
            }

            // A shaft cell may open onto a carved room cell only if it is a live
            // landing — that single opening is the one way onto the stairs.
            for &(x, z) in &shaft {
                for (dx, dz, leave, enter) in EDGES {
                    let (nx, nz) = (x + dx, z + dz);
                    if nx < 0 || nx >= GRID || nz < 0 || nz >= GRID {
                        continue;
                    }
                    if !layout.carved[(nx + nz * GRID) as usize] || shaft.contains(&(nx, nz)) {
                        continue; // wall or another shaft cell — not a room opening
                    }
                    if !blocked(x, z, leave, nx, nz, enter) {
                        assert!(
                            landings.contains(&(x, z)),
                            "seed {seed} depth {}: non-landing shaft cell ({x},{z}) \
                             opens into room cell ({nx},{nz})",
                            layout.depth
                        );
                    }
                }
            }

            // Steps must stay walkable along the run, or descent breaks: the
            // up-shaft exit landing must connect to the step just above it.
            let exit = layout.up_shaft.exit_cell();
            let step = layout.up_shaft.step_cell(SHAFT_LEN - 2, 0);
            let delta = (step.0 - exit.0, step.1 - exit.1);
            let &(_, _, leave, enter) = EDGES
                .iter()
                .find(|&&(dx, dz, _, _)| (dx, dz) == delta)
                .expect("adjacent steps differ by one cell");
            assert!(
                !blocked(exit.0, exit.1, leave, step.0, step.1, enter),
                "seed {seed} depth {}: up-shaft exit landing sealed off from its steps",
                layout.depth
            );
        }
    }
}

#[test]
fn passability_floor_mapping() {
    assert_eq!(passability_floor_for_depth(1), 4);
    assert_eq!(passability_floor_for_depth(MAX_DEPTH), 23);
    assert_eq!(floor_world_y(10.0, 1), 6.0);
    assert_eq!(floor_world_y(10.0, 20), -70.0);
}

#[test]
fn monster_level_scaling() {
    assert_eq!(monster_level_for_depth(1, 1), 1);
    assert_eq!(monster_level_for_depth(2, 4), 2);
    assert_eq!(monster_level_for_depth(2, 6), 3);
    assert_eq!(monster_level_for_depth(4, 20), 12);
    assert_eq!(monster_level_for_depth(19, 20), 20); // cap
    for depth in 1..=MAX_DEPTH {
        assert!(!spawn_table(depth).is_empty());
    }
}

#[test]
fn walkable_drop_keeps_a_carved_scatter_point() {
    // A scatter point that already lands on floor must be returned as-is
    // (only the Y is normalized to the floor surface).
    let entrance = test_entrance();
    let floors = generate_dungeon(dungeon_seed("old_crypt"));
    let layout = &floors[0];
    let room = layout.rooms[0];
    let death = cell_center(&entrance, layout.depth, room.center());
    let preferred = Position {
        x: death.x + 0.3,
        y: death.y,
        z: death.z - 0.2,
    };
    let drop = layout.walkable_drop_position(&entrance, &death, &preferred);
    assert_eq!(drop.x, preferred.x);
    assert_eq!(drop.z, preferred.z);
    assert_eq!(drop.y, floor_world_y(entrance.y, layout.depth));
}

#[test]
fn walkable_drop_never_lands_in_a_wall() {
    // For every carved cell that borders a wall, a drop scattered into that
    // wall must be relocated back onto carved floor across many seeds.
    let entrance = test_entrance();
    let mut checked = 0u32;
    for seed in 0..30u64 {
        let floors = generate_dungeon(seed);
        for layout in &floors {
            for z in 0..GRID {
                for x in 0..GRID {
                    if !layout.is_carved(x, z) {
                        continue;
                    }
                    // Find an uncarved (wall) neighbor to scatter toward.
                    let wall = [(1, 0), (-1, 0), (0, 1), (0, -1)]
                        .into_iter()
                        .map(|(dx, dz)| (x + dx, z + dz))
                        .find(|&(nx, nz)| !layout.is_carved(nx, nz));
                    let Some((wx, wz)) = wall else { continue };

                    let death = cell_center(&entrance, layout.depth, (x, z));
                    // Push the preferred point a full cell into the wall.
                    let preferred = cell_center(&entrance, layout.depth, (wx, wz));
                    let drop = layout.walkable_drop_position(&entrance, &death, &preferred);

                    let (dcx, dcz) = world_to_cell(&entrance, drop.x, drop.z);
                    assert!(
                        layout.is_carved(dcx, dcz),
                        "seed {seed} depth {} cell ({x},{z}) -> wall ({wx},{wz}) \
                         dropped into uncarved cell ({dcx},{dcz})",
                        layout.depth,
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0, "test never exercised a wall-bordering cell");
}

#[test]
fn seed_is_stable_fnv() {
    // FNV-1a 64 reference values; the seed must never change across
    // refactors (it is baked into every deployed entrance).
    assert_eq!(dungeon_seed(""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(dungeon_seed("a"), 0xaf63_dc4c_8601_ec8c);
}
