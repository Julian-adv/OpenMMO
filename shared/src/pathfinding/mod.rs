//! Runtime passability cache + A* pathfinding for housing interiors.
//!
//! A house's `PassabilityGrid` (offline-computed walls, doors, room
//! boundaries) is converted at load-time into a `RuntimePassability`
//! cache entry. The cache is the single source of truth for both client
//! and server traversal queries: where the player can step, where a
//! cardinal A* expansion is allowed, what floor a Y coordinate maps to.
//!
//! Submodule layout:
//! - `cache`: build a cache entry from `HouseData`; mutate it as doors
//!   open and close.
//! - `query`: read-only collision and floor-lookup helpers used by both
//!   the search loop and continuous movement validation.
//! - `astar` / `stair`: A* search over a virtual 1m grid. Stairwell
//!   intermediate cells are encoded as floor-key values *between* two
//!   regular floors so the same machinery walks them.
//! - `smooth`: greedy line-of-sight smoothing applied on top of A* paths.

mod astar;
mod cache;
mod query;
mod smooth;
mod stair;

pub use astar::{find_path, DEFAULT_MAX_NODES};
pub use cache::{apply_door_overlays, build_runtime_passability, update_door_edge};
pub use query::{
    get_floor_at_position, get_floor_y_base, is_cardinal_move_blocked, is_circle_blocked,
    is_movement_blocked,
};
pub use smooth::find_and_smooth_path;

use std::collections::HashMap;

// Edge bitmask constants (matches TypeScript EDGE_N/E/S/W)
pub(super) const EDGE_N: u8 = 1; // -Z edge
pub(super) const EDGE_E: u8 = 2; // +X edge
pub(super) const EDGE_S: u8 = 4; // +Z edge
pub(super) const EDGE_W: u8 = 8; // -X edge

#[derive(Debug, Clone)]
pub struct RuntimeFloorGrid {
    pub floor_level: u8,
    pub origin_x: i32,
    pub origin_z: i32,
    pub width: u8,
    pub depth: u8,
    pub y_base: f32,
    pub wall_height: f32,
    pub cells: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StairwellInfo {
    pub local_min_x: i32,
    pub local_min_z: i32,
    pub local_max_x: i32,
    pub local_max_z: i32,
    pub lower_floor: u8,
    pub upper_floor: u8,
    pub along_z: bool,
    pub reversed: bool,
}

#[derive(Debug, Clone)]
pub struct RuntimePassability {
    pub house_origin_x: f32,
    pub house_origin_z: f32,
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    pub floors: Vec<RuntimeFloorGrid>,
    pub stairwells: Vec<StairwellInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathWaypoint {
    pub x: f32,
    pub z: f32,
    pub floor: u8,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PathResult {
    pub waypoints: Vec<PathWaypoint>,
    pub found: bool,
}

/// Type alias for the passability cache used throughout the API.
pub type PassabilityCache = HashMap<String, RuntimePassability>;

#[cfg(test)]
mod tests {
    use super::smooth::is_line_passable;
    use super::*;

    /// Edge-bitmask cells for a `width`×`depth` grid walled on its outer rim.
    fn perimeter_walls(width: u8, depth: u8) -> Vec<u8> {
        let w = width as usize;
        let d = depth as usize;
        let mut cells = vec![0u8; w * d];
        for x in 0..w {
            cells[x] |= EDGE_N;
            cells[x + (d - 1) * w] |= EDGE_S;
        }
        for z in 0..d {
            cells[z * w] |= EDGE_W;
            cells[z * w + w - 1] |= EDGE_E;
        }
        cells
    }

    fn make_rect_room(width: u8, depth: u8) -> (String, RuntimePassability) {
        let cells = perimeter_walls(width, depth);
        let rp = RuntimePassability {
            house_origin_x: 10.0,
            house_origin_z: 10.0,
            min_x: 10.0,
            max_x: 10.0 + width as f32,
            min_z: 10.0,
            max_z: 10.0 + depth as f32,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width,
                depth,
                y_base: 0.0,
                wall_height: 3.0,
                cells,
            }],
            stairwells: vec![],
        };
        ("house".to_string(), rp)
    }

    fn make_simple_house() -> (String, RuntimePassability) {
        make_rect_room(3, 3)
    }

    #[test]
    fn cardinal_move_blocked_by_wall() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Trying to move west from cell (10, 10) — blocked by west wall
        assert!(is_cardinal_move_blocked(&cache, 10, 10, -1, 0, 0));
        // Moving east from (10, 10) within the house — not blocked
        assert!(!is_cardinal_move_blocked(&cache, 10, 10, 1, 0, 0));
        // Moving east from (12, 10) — blocked by east wall
        assert!(is_cardinal_move_blocked(&cache, 12, 10, 1, 0, 0));
    }

    #[test]
    fn find_path_around_house() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Path from west of house to east of house
        let result = find_path(9.5, 11.5, 0, 13.5, 11.5, 0, &cache, 200);
        assert!(result.found);
        assert!(!result.waypoints.is_empty());
        // Path should go around the house, not through it
        assert!(result.waypoints.len() > 1);
    }

    #[test]
    fn path_in_open_terrain() {
        let cache = PassabilityCache::new(); // No houses
        let result = find_path(0.0, 0.0, 0, 5.0, 5.0, 0, &cache, 200);
        assert!(result.found);
    }

    #[test]
    fn smooth_path_does_not_cross_walls() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Diagonal line from NW corner to SE corner of house would cross walls
        let from = PathWaypoint {
            x: 9.5,
            z: 9.5,
            floor: 0,
        };
        let to = PathWaypoint {
            x: 13.5,
            z: 13.5,
            floor: 0,
        };
        assert!(!is_line_passable(&from, &to, &cache));

        // Line along the north side outside the house — should be passable
        let from2 = PathWaypoint {
            x: 9.5,
            z: 9.5,
            floor: 0,
        };
        let to2 = PathWaypoint {
            x: 13.5,
            z: 9.5,
            floor: 0,
        };
        assert!(is_line_passable(&from2, &to2, &cache));
    }

    #[test]
    fn smooth_path_preserves_endpoints() {
        let (id, rp) = make_simple_house();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Path around the house should be smoothed but still start and end correctly
        let result = find_and_smooth_path(9.5, 11.5, 0, 13.5, 11.5, 0, &cache, 200);
        assert!(result.found);
        assert!(!result.waypoints.is_empty());
        let first = &result.waypoints[0];
        let last = result.waypoints.last().unwrap();
        // First waypoint should be near start, last near goal
        assert!((first.x - 9.5).abs() < 1.0 || (first.x - 10.5).abs() < 1.0);
        assert!((last.x - 13.5).abs() < 0.01);
    }

    #[test]
    fn smooth_diagonal_inside_room() {
        let (id, rp) = make_rect_room(5, 5);
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Diagonal across the room interior (cell centers) — must be passable
        let from = PathWaypoint {
            x: 10.5,
            z: 10.5,
            floor: 0,
        };
        let to = PathWaypoint {
            x: 14.5,
            z: 14.5,
            floor: 0,
        };
        assert!(is_line_passable(&from, &to, &cache));

        // Walk parallel to north wall at z=10.2 — should be passable
        // (directional check: not approaching, just moving parallel)
        let from2 = PathWaypoint {
            x: 10.5,
            z: 10.2,
            floor: 0,
        };
        let to2 = PathWaypoint {
            x: 14.5,
            z: 10.2,
            floor: 0,
        };
        assert!(is_line_passable(&from2, &to2, &cache));

        // Goal near a wall corner — endpoint proximity shouldn't block smoothing
        let from3 = PathWaypoint {
            x: 10.5,
            z: 10.5,
            floor: 0,
        };
        let to3 = PathWaypoint {
            x: 14.8,
            z: 14.8,
            floor: 0,
        };
        assert!(is_line_passable(&from3, &to3, &cache));

        // Full find_and_smooth: diagonal should produce ≤2 waypoints (direct line)
        let result = find_and_smooth_path(10.5, 10.5, 0, 14.5, 14.5, 0, &cache, 500);
        assert!(result.found);
        assert!(
            result.waypoints.len() <= 2,
            "Expected smooth diagonal (≤2 waypoints), got {}",
            result.waypoints.len()
        );
    }

    #[test]
    fn smooth_diagonal_inside_rectangular_room() {
        // Wide rectangle: 8x3
        let (id, rp) = make_rect_room(8, 3);
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        let result = find_and_smooth_path(10.5, 10.5, 0, 17.5, 12.5, 0, &cache, 500);
        assert!(result.found);
        assert!(
            result.waypoints.len() == 1,
            "8x3 room: expected single goal waypoint (direct diagonal), got {} waypoints: {:?}",
            result.waypoints.len(),
            result
                .waypoints
                .iter()
                .map(|w| (w.x, w.z))
                .collect::<Vec<_>>()
        );

        // Tall rectangle: 3x8
        let (id2, rp2) = make_rect_room(3, 8);
        let mut cache2 = PassabilityCache::new();
        cache2.insert(id2, rp2);

        let result2 = find_and_smooth_path(10.5, 10.5, 0, 12.5, 17.5, 0, &cache2, 500);
        assert!(result2.found);
        assert!(
            result2.waypoints.len() == 1,
            "3x8 room: expected single goal waypoint (direct diagonal), got {} waypoints: {:?}",
            result2.waypoints.len(),
            result2
                .waypoints
                .iter()
                .map(|w| (w.x, w.z))
                .collect::<Vec<_>>()
        );
    }

    /// Two single-row floors joined by one stairwell column:
    ///   floor 0 (lower): world cells (0..3, z=0)
    ///   floor 1 (upper): world cells (0..3, z=3)
    ///   stairwell: x=0, z=0..4, connecting lower landing (0,0) to upper (0,3).
    /// House origin is (0,0); both rows are open in X with perimeter walls.
    fn make_two_floor_stairwell() -> (String, RuntimePassability) {
        // 3x1 open row walled on its rim: [W|N|S, N|S, E|N|S].
        let row = || perimeter_walls(3, 1);
        let rp = RuntimePassability {
            house_origin_x: 0.0,
            house_origin_z: 0.0,
            min_x: 0.0,
            max_x: 3.0,
            min_z: 0.0,
            max_z: 4.0,
            floors: vec![
                RuntimeFloorGrid {
                    floor_level: 0,
                    origin_x: 0,
                    origin_z: 0,
                    width: 3,
                    depth: 1,
                    y_base: 0.0,
                    wall_height: 3.0,
                    cells: row(),
                },
                RuntimeFloorGrid {
                    floor_level: 1,
                    origin_x: 0,
                    origin_z: 3,
                    width: 3,
                    depth: 1,
                    y_base: 3.1,
                    wall_height: 3.0,
                    cells: row(),
                },
            ],
            stairwells: vec![StairwellInfo {
                local_min_x: 0,
                local_min_z: 0,
                local_max_x: 1,
                local_max_z: 4,
                lower_floor: 0,
                upper_floor: 1,
                along_z: true,
                reversed: false,
            }],
        };
        ("two_floor".to_string(), rp)
    }

    #[test]
    fn cross_floor_query_descends_the_stairwell() {
        let (id, rp) = make_two_floor_stairwell();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Start on the upper floor (1), goal on the lower floor (0): differing
        // floors, so the stairwell is traversed.
        let result = find_path(2.5, 3.5, 1, 2.5, 0.5, 0, &cache, 500);
        assert!(result.found, "cross-floor path should be found");
        assert!(
            result.waypoints.iter().any(|w| w.floor == 0),
            "cross-floor path must reach the lower floor: {:?}",
            result.waypoints
        );
    }

    #[test]
    fn same_floor_query_never_leaves_its_floor() {
        let (id, rp) = make_two_floor_stairwell();
        let mut cache = PassabilityCache::new();
        cache.insert(id, rp);

        // Same start, but request the goal on the START floor (1). The target
        // cell only exists on floor 0, so without confinement A* would dive
        // down the stairwell to approach it. Confinement keeps every waypoint
        // on floor 1 — the fix that stops dungeon monsters using stairs.
        let result = find_path(2.5, 3.5, 1, 0.5, 0.5, 1, &cache, 500);
        assert!(
            !result.waypoints.is_empty(),
            "confined partial path should still advance along its own floor"
        );
        assert!(
            result.waypoints.iter().all(|w| w.floor == 1),
            "confined path must never descend the stairwell: {:?}",
            result.waypoints
        );
    }
}
