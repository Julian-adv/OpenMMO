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
}

#[derive(serde::Deserialize)]
struct Entry {
    id: String,
    perimeter: Option<Perimeter>,
}

fn perimeters() -> &'static HashMap<String, Perimeter> {
    static TABLE: OnceLock<HashMap<String, Perimeter>> = OnceLock::new();
    TABLE.get_or_init(|| {
        serde_json::from_str::<Vec<Entry>>(include_str!("../../data/object_catalog.json"))
            .expect("object_catalog.json is malformed")
            .into_iter()
            .filter_map(|entry| Some((entry.id, entry.perimeter?)))
            .collect()
    })
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
    let ex = hw * cos.abs() + hd * sin.abs();
    let ez = hw * sin.abs() + hd * cos.abs();
    let min_x = (placement.x - ex).floor() as i32 - 1;
    let min_z = (placement.z - ez).floor() as i32 - 1;
    let width = (placement.x + ex).ceil() as i32 + 1 - min_x;
    let depth = (placement.z + ez).ceil() as i32 + 1 - min_z;
    if width <= 0 || depth <= 0 || width > 255 || depth > 255 {
        return None;
    }
    let local = |x: i32, z: i32| {
        let dx = x as f32 + 0.5 - placement.x;
        let dz = z as f32 + 0.5 - placement.z;
        (dx * cos - dz * sin, dx * sin + dz * cos)
    };
    let inside = |x: f32, z: f32| x.abs() < hw && z.abs() < hd;
    let mut cells = vec![0; (width * depth) as usize];
    for z in 1..depth - 1 {
        for x in 1..width - 1 {
            let (lx, lz) = local(min_x + x, min_z + z);
            if !inside(lx, lz) {
                continue;
            }
            for (dx, dz, edge, opposite) in [
                (1, 0, EDGE_E, EDGE_W),
                (-1, 0, EDGE_W, EDGE_E),
                (0, 1, EDGE_S, EDGE_N),
                (0, -1, EDGE_N, EDGE_S),
            ] {
                let (nx, nz) = local(min_x + x + dx, min_z + z + dz);
                if inside(nx, nz) {
                    continue;
                }
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
                if front_t <= 1.0
                    && front_t < side_t
                    && entrance_x.abs() < perimeter.entrance_width / 2.0
                {
                    continue;
                }
                cells[(x + z * width) as usize] |= edge;
                cells[(x + dx + (z + dz) * width) as usize] |= opposite;
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
    fn union_hall_walls_block_and_portal_opens() {
        let p = FurniturePlacement {
            id: 121,
            type_id: "union_hall".into(),
            x: -1445.0658,
            y: 1.0128,
            z: 4797.3719,
            rotation_deg: 270.0,
            floor_level: 0,
        };
        let cache = cache(&p);
        for (ax, az, bx, bz) in [
            (5.0, 0.0, 7.0, 0.0),
            (-5.0, 3.0, -7.0, 3.0),
            (0.0, -7.5, 0.0, -9.5),
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
        let (x0, z0) = world(&p, 0.0, 7.5);
        let (x1, z1) = world(&p, 0.0, 9.5);
        assert!(!is_movement_blocked(&cache, x0, z0, x1, z1, 0, Some(p.y)));
        assert!(!is_movement_blocked(&cache, x1, z1, x0, z0, 0, Some(p.y)));
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
