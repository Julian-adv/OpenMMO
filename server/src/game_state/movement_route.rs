use super::{passability, player::MAX_QUEUED_WAYPOINTS, GameState};
use crate::types::Position;
use onlinerpg_shared::{dungeon, pathfinding, shortest_world_delta_x};

impl GameState {
    pub(super) async fn replacement_waypoints(
        &self,
        from: Position,
        target: Position,
        target_floor: i8,
    ) -> Result<Vec<(Position, i8)>, ()> {
        let waypoints = {
            let cache = self.passability_read();
            let collision_floor = passability::authoritative_floor(&cache, &from);
            let target_x = from.x + shortest_world_delta_x(from.x, target.x);
            if passability::wrapped_block_info(
                &cache,
                from.x,
                from.z,
                target_x,
                target.z,
                collision_floor,
                from.y,
            )
            .is_none()
            {
                return Ok(Vec::new());
            }
            let start_floor = pathfinding::start_floor_at(&cache, from.x, from.z, from.y);
            let goal_floor = if pathfinding::in_stairwell_span(&cache, target.x, target.z, target.y)
            {
                pathfinding::start_floor_at(&cache, target.x, target.z, target.y)
            } else {
                dungeon::passability_floor_for_level(target_floor)
            };
            let mut path = pathfinding::find_and_smooth_path(
                from.x,
                from.z,
                start_floor,
                target.x,
                target.z,
                goal_floor,
                &cache,
                dungeon::path_max_nodes(start_floor, goal_floor),
            );
            if !path.found
                || path.waypoints.len() < 2
                || path.waypoints.len() > MAX_QUEUED_WAYPOINTS
            {
                return Err(());
            }
            let Some(last) = path.waypoints.pop() else {
                return Err(());
            };
            if shortest_world_delta_x(last.x, target.x).abs() > 0.001
                || (last.z - target.z).abs() > 0.001
            {
                return Err(());
            }
            path.waypoints
        };
        let mut result = Vec::with_capacity(waypoints.len());
        let mut previous_y = from.y;
        for waypoint in waypoints {
            let floor = dungeon::floor_level_for_passability(waypoint.floor);
            let mut position = Position {
                x: waypoint.x,
                y: previous_y,
                z: waypoint.z,
            };
            position.y = if floor < 0 {
                let entrance = self
                    .dungeon_defs
                    .entrance_at(position.x, position.z)
                    .ok_or(())?;
                self.ensure_dungeon_runtime(&entrance.id).await;
                let dungeons = self.dungeons.read().await;
                let runtime = dungeons.get(&entrance.id).ok_or(())?;
                dungeon::floor_height_at(
                    &entrance.position(),
                    &runtime.layouts,
                    floor.unsigned_abs(),
                    position.x,
                    position.z,
                )
                .ok_or(())?
            } else {
                self.surface_ground_y(waypoint.floor, &position, previous_y, None)
                    .await
            };
            previous_y = position.y;
            result.push((position, floor));
        }
        Ok(result)
    }
}
