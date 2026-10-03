//! Pathfinding against the server's passability cache or a test provider.

use crate::pathfinding::{self, PathResult};

pub trait PathProvider {
    fn find_path(
        &self,
        start_x: f32,
        start_z: f32,
        start_floor: u8,
        goal_x: f32,
        goal_z: f32,
        goal_floor: u8,
    ) -> PathResult;

    /// Whether the attack line crosses a wall or closed door.
    fn attack_line_blocked(
        &self,
        from_x: f32,
        from_z: f32,
        to_x: f32,
        to_z: f32,
        floor: u8,
    ) -> bool;

    /// Reject sealed standing cells before searching for a route.
    fn cell_passable(&self, _x: f32, _z: f32, _floor: u8) -> bool {
        true
    }

    /// `find_path` keeping out of `blocked` cells (standing monsters) within
    /// `max_nodes`. Default: the plain path, refused if it crosses one.
    #[allow(clippy::too_many_arguments)]
    fn find_path_avoiding(
        &self,
        start_x: f32,
        start_z: f32,
        start_floor: u8,
        goal_x: f32,
        goal_z: f32,
        goal_floor: u8,
        blocked: &[(i32, i32)],
        _max_nodes: usize,
    ) -> PathResult {
        let result = self.find_path(start_x, start_z, start_floor, goal_x, goal_z, goal_floor);
        if super::leg_crosses_occupied(start_x, start_z, &result.waypoints, blocked) {
            return PathResult {
                waypoints: vec![],
                found: false,
                termination: crate::pathfinding::PathTermination::Unreachable,
            };
        }
        result
    }
}

/// PathProvider backed by a reference to PassabilityCache (for native Rust).
pub struct CachePathProvider<'a> {
    pub cache: &'a pathfinding::PassabilityCache,
}

impl<'a> PathProvider for CachePathProvider<'a> {
    fn find_path(
        &self,
        start_x: f32,
        start_z: f32,
        start_floor: u8,
        goal_x: f32,
        goal_z: f32,
        goal_floor: u8,
    ) -> PathResult {
        pathfinding::find_and_smooth_path(
            start_x,
            start_z,
            start_floor,
            goal_x,
            goal_z,
            goal_floor,
            self.cache,
            crate::dungeon::path_max_nodes(start_floor, goal_floor),
        )
    }

    fn attack_line_blocked(
        &self,
        from_x: f32,
        from_z: f32,
        to_x: f32,
        to_z: f32,
        floor: u8,
    ) -> bool {
        pathfinding::attack_line_blocked(self.cache, from_x, from_z, to_x, to_z, floor)
    }

    fn cell_passable(&self, x: f32, z: f32, floor: u8) -> bool {
        !pathfinding::is_cell_sealed(self.cache, x, z, floor, None)
    }

    fn find_path_avoiding(
        &self,
        start_x: f32,
        start_z: f32,
        start_floor: u8,
        goal_x: f32,
        goal_z: f32,
        goal_floor: u8,
        blocked: &[(i32, i32)],
        max_nodes: usize,
    ) -> PathResult {
        pathfinding::find_and_smooth_path_avoiding(
            start_x,
            start_z,
            start_floor,
            goal_x,
            goal_z,
            goal_floor,
            self.cache,
            max_nodes,
            blocked,
        )
    }
}
