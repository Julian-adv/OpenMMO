use std::{collections::HashMap, sync::OnceLock};

use crate::{
    furniture::FurniturePlacement,
    pathfinding::{RuntimeFloorGrid, RuntimePassability, EDGE_E, EDGE_N, EDGE_S, EDGE_W},
};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Perimeter {
    width: f32,
    depth: f32,
    height: f32,
    entrance_width: f32,
    /// Height of a built floor above the placement; movers stand on it
    /// instead of the terrain.
    #[serde(default)]
    floor: Option<f32>,
    /// Solid band outside the walkable rectangles, so no one stands in the wall.
    #[serde(default)]
    wall: f32,
    #[serde(skip)]
    door: bool,
    #[serde(skip)]
    blocks: Vec<Block>,
    #[serde(skip)]
    annexes: Vec<Annex>,
}

/// Walkable local rectangle joined to the perimeter through an opening of
/// `opening` metres centred on its own x span, such as a chancel behind an arch.
#[derive(serde::Deserialize)]
struct Annex {
    x: [f32; 2],
    z: [f32; 2],
    opening: f32,
    #[serde(default)]
    floor: f32,
}

/// Solid local rectangle outside the perimeter, such as a tower.
#[derive(serde::Deserialize)]
struct Block {
    x: [f32; 2],
    z: [f32; 2],
}

#[derive(serde::Deserialize)]
struct Entry {
    id: String,
    perimeter: Option<Perimeter>,
    #[serde(rename = "swingDoor")]
    swing_door: Option<serde::de::IgnoredAny>,
    #[serde(default)]
    blocks: Vec<Block>,
    #[serde(default)]
    annexes: Vec<Annex>,
}

fn perimeters() -> &'static HashMap<String, Perimeter> {
    static TABLE: OnceLock<HashMap<String, Perimeter>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<Vec<Entry>>(include_str!("../../data/object_catalog.json"))
            .expect("object_catalog.json is malformed")
            .into_iter()
            .filter_map(|entry| {
                let mut perimeter = entry.perimeter?;
                perimeter.door = entry.swing_door.is_some();
                perimeter.blocks = entry.blocks;
                perimeter.annexes = entry.annexes;
                Some((entry.id, perimeter))
            })
            .collect()
    })
}

/// World bounds `(min_x, max_x, min_z, max_z)` of a local rectangle.
fn world_aabb(placement: &FurniturePlacement, x: [f32; 2], z: [f32; 2]) -> (f32, f32, f32, f32) {
    let (sin, cos) = placement.rotation_deg.to_radians().sin_cos();
    let mut bounds = (
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
    );
    for lx in x {
        for lz in z {
            let wx = placement.x + lx * cos + lz * sin;
            let wz = placement.z - lx * sin + lz * cos;
            bounds = (
                bounds.0.min(wx),
                bounds.1.max(wx),
                bounds.2.min(wz),
                bounds.3.max(wz),
            );
        }
    }
    bounds
}

fn grid(placement: &FurniturePlacement, perimeter: &Perimeter) -> Option<RuntimeFloorGrid> {
    let (hw, hd) = (perimeter.width / 2.0, perimeter.depth / 2.0);
    if !(hw > 0.0
        && hd > 0.0
        && perimeter.height > 0.0
        && perimeter.entrance_width > 0.0
        && perimeter.entrance_width <= perimeter.width)
    {
        return None;
    }
    let (sin, cos) = placement.rotation_deg.to_radians().sin_cos();
    let blocks = &perimeter.blocks;
    let (mut x0, mut x1, mut z0, mut z1) = (-hw, hw, -hd, hd);
    let annexes = &perimeter.annexes;
    let spans = blocks
        .iter()
        .map(|b| (b.x, b.z))
        .chain(annexes.iter().map(|a| (a.x, a.z)));
    for (x, z) in spans {
        x0 = x0.min(x[0]);
        x1 = x1.max(x[1]);
        z0 = z0.min(z[0]);
        z1 = z1.max(z[1]);
    }
    let wall = perimeter.wall;
    let (wx0, wx1, wz0, wz1) =
        world_aabb(placement, [x0 - wall, x1 + wall], [z0 - wall, z1 + wall]);
    let min_x = wx0.floor() as i32 - 1;
    let min_z = wz0.floor() as i32 - 1;
    let width = wx1.ceil() as i32 + 1 - min_x;
    let depth = wz1.ceil() as i32 + 1 - min_z;
    if width <= 0 || depth <= 0 || width > 255 || depth > 255 {
        return None;
    }
    let local = |x: i32, z: i32| {
        let dx = x as f32 + 0.5 - placement.x;
        let dz = z as f32 + 0.5 - placement.z;
        (dx * cos - dz * sin, dx * sin + dz * cos)
    };
    let inside = |x: f32, z: f32| x.abs() < hw && z.abs() < hd;
    let blocked = |x: f32, z: f32| {
        blocks
            .iter()
            .any(|b| x > b.x[0] && x < b.x[1] && z > b.z[0] && z < b.z[1])
    };
    let annex = |x: f32, z: f32| {
        annexes
            .iter()
            .find(|a| x > a.x[0] && x < a.x[1] && z > a.z[0] && z < a.z[1])
    };
    let half_entrance = perimeter.entrance_width / 2.0;
    let solid = |x: f32, z: f32| {
        blocked(x, z)
            || (wall > 0.0
                && !inside(x, z)
                && annex(x, z).is_none()
                && ((x.abs() < hw + wall
                    && z.abs() < hd + wall
                    && !(z > 0.0 && x.abs() < half_entrance))
                    || annexes.iter().any(|a| {
                        x > a.x[0] - wall
                            && x < a.x[1] + wall
                            && z > a.z[0] - wall
                            && z < a.z[1] + wall
                    })))
    };
    let through_opening = |a: &Annex, x: f32| (x - (a.x[0] + a.x[1]) / 2.0).abs() < a.opening / 2.0;
    const NEIGHBOURS: [(i32, i32, u8, u8); 4] = [
        (1, 0, EDGE_E, EDGE_W),
        (-1, 0, EDGE_W, EDGE_E),
        (0, 1, EDGE_S, EDGE_N),
        (0, -1, EDGE_N, EDGE_S),
    ];
    let mut cells = vec![0; (width * depth) as usize];
    for z in 1..depth - 1 {
        for x in 1..width - 1 {
            let (lx, lz) = local(min_x + x, min_z + z);
            let in_nave = inside(lx, lz);
            let in_annex = if in_nave { None } else { annex(lx, lz) };
            if !in_nave && in_annex.is_none() && !solid(lx, lz) {
                continue;
            }
            for (dx, dz, edge, opposite) in NEIGHBOURS {
                let (nx, nz) = local(min_x + x + dx, min_z + z + dz);
                let open = if in_nave {
                    if inside(nx, nz) {
                        true
                    } else if let Some(a) = annex(nx, nz) {
                        through_opening(a, (lx + nx) / 2.0)
                    } else {
                        let front_t = if nz > lz {
                            (hd - lz) / (nz - lz)
                        } else {
                            f32::INFINITY
                        };
                        let side_t = if nx > lx {
                            (hw - lx) / (nx - lx)
                        } else if nx < lx {
                            (-hw - lx) / (nx - lx)
                        } else {
                            f32::INFINITY
                        };
                        let entrance_x = lx + front_t * (nx - lx);
                        front_t <= 1.0
                            && front_t < side_t
                            && entrance_x.abs() < half_entrance
                            && (!perimeter.door || placement.door_open)
                    }
                } else if let Some(a) = in_annex {
                    if inside(nx, nz) {
                        through_opening(a, (lx + nx) / 2.0)
                    } else {
                        annex(nx, nz).is_some()
                    }
                } else {
                    solid(nx, nz)
                };
                if !open {
                    cells[(x + z * width) as usize] |= edge;
                    cells[(x + dx + (z + dz) * width) as usize] |= opposite;
                }
            }
        }
    }
    Some(RuntimeFloorGrid {
        floor_level: placement.floor_level,
        origin_x: min_x,
        origin_z: min_z,
        width: width as u8,
        depth: depth as u8,
        y_base: placement.y,
        wall_height: perimeter.height,
        cells,
    })
}

/// World segment `[ax, az, bx, bz]` across the entrance of a door object.
pub fn doorway(placement: &FurniturePlacement) -> Option<[f32; 4]> {
    let perimeter = perimeters().get(&placement.type_id).filter(|p| p.door)?;
    let (sin, cos) = placement.rotation_deg.to_radians().sin_cos();
    let (hw, hd) = (perimeter.entrance_width / 2.0, perimeter.depth / 2.0);
    let world = |x: f32| {
        (
            placement.x + x * cos + hd * sin,
            placement.z - x * sin + hd * cos,
        )
    };
    let ((ax, az), (bx, bz)) = (world(-hw), world(hw));
    Some([ax, az, bx, bz])
}

pub fn build_passability(placements: &[FurniturePlacement]) -> Option<RuntimePassability> {
    let floors: Vec<_> = placements
        .iter()
        .filter_map(|placement| grid(placement, perimeters().get(&placement.type_id)?))
        .collect();
    if floors.is_empty() {
        return None;
    }
    let (min_x, max_x, min_z, max_z) = floors.iter().fold(
        (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ),
        |(min_x, max_x, min_z, max_z), f| {
            let (x, z) = (f.origin_x as f32, f.origin_z as f32);
            (
                min_x.min(x),
                max_x.max(x + f.width as f32),
                min_z.min(z),
                max_z.max(z + f.depth as f32),
            )
        },
    );
    Some(RuntimePassability {
        house_origin_x: 0.0,
        house_origin_z: 0.0,
        min_x,
        max_x,
        min_z,
        max_z,
        floors,
        stairwells: Vec::new(),
        yields_to_trapped_mover: false,
        allows_projectiles: false,
        is_ground: false,
    })
}

/// One level grid over the cells whose centres fall in a local rectangle.
fn floor_grid(
    placement: &FurniturePlacement,
    x: [f32; 2],
    z: [f32; 2],
    y_base: f32,
    wall_height: f32,
) -> Option<RuntimeFloorGrid> {
    let (x0, x1, z0, z1) = world_aabb(placement, x, z);
    let origin_x = (x0 - 0.5).ceil() as i32;
    let origin_z = (z0 - 0.5).ceil() as i32;
    let width = (x1 - 0.5).ceil() as i32 - origin_x;
    let depth = (z1 - 0.5).ceil() as i32 - origin_z;
    (width > 0 && depth > 0 && width <= 255 && depth <= 255).then(|| RuntimeFloorGrid {
        floor_level: placement.floor_level,
        origin_x,
        origin_z,
        width: width as u8,
        depth: depth as u8,
        y_base,
        wall_height,
        cells: vec![0; (width * depth) as usize],
    })
}

/// Built floors of placed objects, read like a house's ground storey.
pub fn build_ground(placements: &[FurniturePlacement]) -> Option<RuntimePassability> {
    let mut floors = Vec::new();
    for placement in placements {
        let Some(perimeter) = perimeters().get(&placement.type_id) else {
            continue;
        };
        let Some(floor) = perimeter.floor else {
            continue;
        };
        let (hw, hd) = (perimeter.width / 2.0, perimeter.depth / 2.0);
        floors.extend(floor_grid(
            placement,
            [-hw, hw],
            [-hd, hd],
            placement.y + floor,
            perimeter.height,
        ));
        for annex in &perimeter.annexes {
            floors.extend(floor_grid(
                placement,
                annex.x,
                annex.z,
                placement.y + floor + annex.floor,
                perimeter.height,
            ));
        }
    }
    if floors.is_empty() {
        return None;
    }
    let bound = |pick: fn(&RuntimeFloorGrid) -> (f32, f32)| {
        floors
            .iter()
            .map(pick)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (a, b)| {
                (lo.min(a), hi.max(b))
            })
    };
    let (min_x, max_x) = bound(|f| (f.origin_x as f32, f.origin_x as f32 + f.width as f32));
    let (min_z, max_z) = bound(|f| (f.origin_z as f32, f.origin_z as f32 + f.depth as f32));
    Some(RuntimePassability {
        house_origin_x: 0.0,
        house_origin_z: 0.0,
        min_x,
        max_x,
        min_z,
        max_z,
        floors,
        stairwells: Vec::new(),
        yields_to_trapped_mover: false,
        allows_projectiles: true,
        is_ground: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{furniture, pathfinding::*};

    fn tent(x: f32, z: f32, yaw: f32) -> FurniturePlacement {
        FurniturePlacement {
            id: 1,
            type_id: "ysolde_tent".into(),
            x,
            y: 6.65,
            z,
            rotation_deg: yaw,
            floor_level: 0,
            door_open: false,
        }
    }

    fn world(p: &FurniturePlacement, x: f32, z: f32) -> (f32, f32) {
        let yaw = p.rotation_deg.to_radians();
        (
            p.x + x * yaw.cos() + z * yaw.sin(),
            p.z - x * yaw.sin() + z * yaw.cos(),
        )
    }

    fn cache(p: &FurniturePlacement) -> PassabilityCache {
        let mut cache = PassabilityCache::new();
        furniture::sync_passability(&mut cache, "furniture:test", std::slice::from_ref(p));
        cache
    }

    #[test]
    fn perimeter_blocks_sides_and_rear_but_opens_front_in_both_directions() {
        for yaw in [0.0, 90.0, 180.0, 270.0, 45.0] {
            for offset in [0.0, 0.17, 0.3, 0.5, 0.95] {
                let p = tent(10.0 + offset, 20.0 + offset, yaw);
                let cache = cache(&p);
                let (x, z) = world(&p, 0.0, 0.0);
                for (lx, lz) in [(-3.0, 0.0), (3.0, 0.0), (0.0, -3.0)] {
                    let (tx, tz) = world(&p, lx, lz);
                    assert!(
                        is_movement_blocked(&cache, x, z, tx, tz, 0, Some(p.y)),
                        "yaw {yaw} offset {offset} side {lx},{lz}"
                    );
                    assert!(is_movement_blocked(&cache, tx, tz, x, z, 0, Some(p.y)));
                }
                let (fx, fz) = world(&p, 0.0, 3.0);
                assert!(
                    !is_movement_blocked(&cache, x, z, fx, fz, 0, Some(p.y)),
                    "front yaw {yaw} offset {offset}"
                );
                assert!(!is_movement_blocked(&cache, fx, fz, x, z, 0, Some(p.y)));
                for step in 0..61 {
                    let (cx, cz) = world(&p, 0.0, step as f32 / 20.0);
                    assert!(
                        !is_circle_blocked_on_floor(&cache, cx, cz, 0.31, 0, Some(p.y)),
                        "body yaw {yaw} offset {offset} at {step}: {cx},{cz}"
                    );
                }
                assert!(!is_movement_blocked(&cache, x, z, fx, fz, 1, Some(p.y)));
                let (rx, rz) = world(&p, 0.0, -3.0);
                assert!(!is_movement_blocked(
                    &cache,
                    x,
                    z,
                    rx,
                    rz,
                    0,
                    Some(p.y + 3.1)
                ));
            }
        }
    }

    #[test]
    fn union_hall_walls_block_and_door_gates_the_portal() {
        let p = FurniturePlacement {
            id: 121,
            type_id: "union_hall".into(),
            x: -1445.0658,
            y: 1.0128,
            z: 4797.372,
            rotation_deg: 270.0,
            floor_level: 0,
            door_open: false,
        };
        let (x0, z0) = world(&p, 0.0, 7.5);
        let (x1, z1) = world(&p, 0.0, 9.5);
        let shut = cache(&p);
        assert!(is_movement_blocked(&shut, x0, z0, x1, z1, 0, Some(p.y)));
        assert!(is_movement_blocked(&shut, x1, z1, x0, z0, 0, Some(p.y)));
        let [ax, az, bx, bz] = doorway(&p).unwrap();
        let (mx, mz) = world(&p, 0.0, 8.5);
        assert!(((ax + bx) / 2.0 - mx).abs() < 1e-3 && ((az + bz) / 2.0 - mz).abs() < 1e-3);
        assert!(((ax - bx).hypot(az - bz) - 3.8).abs() < 1e-3);
        let p = FurniturePlacement {
            door_open: true,
            ..p
        };
        let cache = cache(&p);
        for (ax, az, bx, bz) in [
            (5.0, 0.0, 7.0, 0.0),
            (-5.0, 3.0, -7.0, 3.0),
            (5.0, -7.5, 5.0, -9.5),
            (4.0, 7.5, 4.0, 9.5),
        ] {
            let (x0, z0) = world(&p, ax, az);
            let (x1, z1) = world(&p, bx, bz);
            assert!(
                is_movement_blocked(&cache, x0, z0, x1, z1, 0, Some(p.y)),
                "out {ax},{az}"
            );
            assert!(
                is_movement_blocked(&cache, x1, z1, x0, z0, 0, Some(p.y)),
                "in {ax},{az}"
            );
        }
        assert!(!is_movement_blocked(&cache, x0, z0, x1, z1, 0, Some(p.y)));
        assert!(!is_movement_blocked(&cache, x1, z1, x0, z0, 0, Some(p.y)));
    }

    #[test]
    fn union_hall_tower_is_solid_and_chancel_opens_through_the_arch() {
        for yaw in [0.0, 90.0, 270.0] {
            let p = FurniturePlacement {
                id: 121,
                type_id: "union_hall".into(),
                x: -1445.0658,
                y: 1.0128,
                z: 4797.372,
                rotation_deg: yaw,
                floor_level: 0,
                door_open: false,
            };
            let cache = cache(&p);
            let crosses = |a: (f32, f32), b: (f32, f32)| {
                let (x0, z0) = world(&p, a.0, a.1);
                let (x1, z1) = world(&p, b.0, b.1);
                is_movement_blocked(&cache, x0, z0, x1, z1, 0, Some(p.y))
            };
            assert!(crosses((-13.0, 7.0), (-9.0, 7.0)), "into tower, yaw {yaw}");
            assert!(
                crosses((-9.0, 11.0), (-9.0, 7.0)),
                "into tower front, yaw {yaw}"
            );
            assert!(
                crosses((0.0, -17.0), (0.0, -12.0)),
                "into chancel, yaw {yaw}"
            );
            assert!(
                crosses((7.0, -12.0), (2.0, -12.0)),
                "into chancel side, yaw {yaw}"
            );
            assert!(
                !crosses((0.0, -7.5), (0.0, -9.5)),
                "through the arch, yaw {yaw}"
            );
            assert!(
                !crosses((0.0, -9.5), (0.0, -7.5)),
                "back through the arch, yaw {yaw}"
            );
            assert!(
                crosses((4.5, -7.5), (4.5, -9.5)),
                "beside the arch, yaw {yaw}"
            );
            assert!(
                crosses((3.5, -9.5), (3.5, -7.5)),
                "arch pier from behind, yaw {yaw}"
            );
            assert!(
                !crosses((-3.0, -10.0), (3.0, -13.0)),
                "inside the chancel, yaw {yaw}"
            );
            assert!(
                crosses((8.5, 0.0), (6.2, 0.0)),
                "into the south wall, yaw {yaw}"
            );
            assert!(
                crosses((4.0, 11.0), (4.0, 8.7)),
                "into the facade, yaw {yaw}"
            );
            assert!(
                crosses((6.5, -12.0), (4.2, -12.0)),
                "into the chancel wall, yaw {yaw}"
            );
            assert!(
                !crosses((0.0, 11.0), (0.0, 9.0)),
                "up to the doors, yaw {yaw}"
            );
            assert!(
                !crosses((8.5, -3.0), (8.5, 3.0)),
                "along the south wall, yaw {yaw}"
            );
            assert!(
                crosses((9.5, 1.7), (7.3, 1.7)),
                "into a buttress, yaw {yaw}"
            );
            assert!(
                !crosses((9.5, 0.0), (7.6, 0.0)),
                "between buttresses, yaw {yaw}"
            );
            assert!(
                !crosses((-13.0, 7.0), (-13.0, 2.0)),
                "past tower, yaw {yaw}"
            );
            assert!(
                !crosses((-9.0, 11.5), (-13.0, 11.5)),
                "before tower, yaw {yaw}"
            );
            assert!(
                !crosses((7.0, -16.5), (-7.0, -16.5)),
                "behind chancel, yaw {yaw}"
            );
        }
    }

    #[test]
    fn union_hall_floor_and_dais_carry_their_own_height() {
        for yaw in [0.0, 90.0, 270.0] {
            let p = FurniturePlacement {
                id: 121,
                type_id: "union_hall".into(),
                x: -1445.0658,
                y: 1.0128,
                z: 4797.372,
                rotation_deg: yaw,
                floor_level: 0,
                door_open: false,
            };
            let cache = cache(&p);
            let ground = |x: f32, z: f32| {
                let (wx, wz) = world(&p, x, z);
                storey_ground_y(&cache, 0, wx, wz, p.y)
            };
            let near = |got: Option<f32>, want: f32| got.is_some_and(|y| (y - want).abs() < 1e-3);
            assert!(near(ground(0.0, 0.0), p.y), "nave, yaw {yaw}");
            assert!(near(ground(4.0, 6.0), p.y), "nave corner, yaw {yaw}");
            assert!(near(ground(0.0, -11.0), p.y + 0.45), "dais, yaw {yaw}");
            assert!(ground(9.0, 0.0).is_none(), "outside, yaw {yaw}");
            assert!(ground(-9.0, 7.0).is_none(), "tower, yaw {yaw}");
        }
        let tent = tent(10.5, 20.5, 0.0);
        let cache = cache(&tent);
        assert!(storey_ground_y(&cache, 0, 10.5, 20.5, tent.y).is_none());
    }

    #[test]
    fn pathfinding_detours_through_front_instead_of_cutting_through_canvas() {
        for yaw in [0.0, 90.0, 180.0, 270.0, 45.0] {
            let p = tent(-1881.977, 5517.6353, yaw);
            let cache = cache(&p);
            let (sx, sz) = world(&p, 0.0, -4.0);
            let result = find_and_smooth_path(sx, sz, 0, p.x, p.z, 0, &cache, 2000);
            assert!(result.found, "yaw {yaw}: {result:?}");
            let mut previous = (sx, sz);
            let mut through_front = false;
            for point in result.waypoints {
                assert!(
                    !is_movement_blocked(
                        &cache,
                        previous.0,
                        previous.1,
                        point.x,
                        point.z,
                        0,
                        Some(p.y)
                    ),
                    "yaw {yaw}"
                );
                let angle = yaw.to_radians();
                let local_z = (point.x - p.x) * angle.sin() + (point.z - p.z) * angle.cos();
                through_front |= local_z > 0.8;
                previous = (point.x, point.z);
            }
            assert!(through_front, "yaw {yaw}");
        }
    }

    #[test]
    fn tent_and_cot_keep_separate_collision_rules_and_clear_together() {
        let p = tent(0.5, 0.5, 0.0);
        let cot = FurniturePlacement {
            type_id: "ysolde_field_cot".into(),
            id: 2,
            ..p.clone()
        };
        let mut cache = PassabilityCache::new();
        furniture::sync_passability(&mut cache, "furniture:test", &[p.clone(), cot]);
        assert_eq!(cache.len(), 2);
        assert!(cache["furniture:test"].yields_to_trapped_mover);
        assert!(!cache["furniture:test:perimeter"].yields_to_trapped_mover);
        assert!(is_movement_blocked_for_mover(
            &cache,
            0.5,
            0.5,
            2.5,
            0.5,
            0,
            Some(p.y)
        ));
        furniture::sync_passability(&mut cache, "furniture:test", &[p]);
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key("furniture:test:perimeter"));
        furniture::remove_passability(&mut cache, "furniture:test");
        assert!(cache.is_empty());
    }
}
