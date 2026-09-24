use super::*;
use onlinerpg_shared::mount::MountKind;

impl GameState {
    pub(super) async fn stop_route_at_entrance(
        &self,
        player: &Player,
        target: &Position,
        points: &mut Vec<PathWaypoint>,
    ) {
        if player.floor_level != 0 {
            return;
        }
        let house_id = self
            .passability_read()
            .iter()
            .find(|(_, entry)| {
                entry.is_ground
                    && target.x >= entry.min_x
                    && target.x <= entry.max_x
                    && target.z >= entry.min_z
                    && target.z <= entry.max_z
            })
            .map(|(id, _)| id.clone());
        let Some(id) = house_id else {
            return;
        };
        let Ok(Some(house)) = self.housing_io.find_house(&id).await else {
            return;
        };
        let inside = |x: f32, z: f32| {
            house
                .rooms
                .iter()
                .any(|room| room.floor_level == 0 && room.contains_xz(&house.origin, x, z))
        };
        if !inside(target.x, target.z) || inside(player.position.x, player.position.z) {
            return;
        }
        let mut previous = player.position;
        let mut entered = false;
        let mut remaining = 0.7;
        for index in 0..points.len() {
            let point = &points[index];
            let dx = shortest_world_delta_x(previous.x, point.x);
            let dz = point.z - previous.z;
            let length = dx.hypot(dz);
            let samples = (length / 0.05).ceil().max(1.0) as usize;
            for step in 1..=samples {
                let t = step as f32 / samples as f32;
                let x = wrap_world_x(previous.x + dx * t);
                let z = previous.z + dz * t;
                entered |= inside(x, z);
                if entered {
                    remaining -= length / samples as f32;
                }
                if entered && remaining <= 0.0 {
                    points[index].x = x;
                    points[index].z = z;
                    points.truncate(index + 1);
                    return;
                }
            }
            previous.x = point.x;
            previous.z = point.z;
        }
    }

    pub(super) async fn mount_route(
        &self,
        player: &Player,
        route: &[PathWaypoint],
        mount: MountKind,
    ) -> (Vec<MoveWaypoint>, Option<PathTermination>) {
        use onlinerpg_shared::mount_movement::{arc_step, STEP_SECONDS};
        let mut position = player.position;
        let mut rotation = player.rotation;
        let speed = PLAYER_MOVE_SPEED * mount.speed_mult();
        let mut points = Vec::new();
        for waypoint in route {
            if waypoint.floor != 0 {
                return (points, Some(PathTermination::Unreachable));
            }
            loop {
                let dx = shortest_world_delta_x(position.x, waypoint.x);
                let dz = waypoint.z - position.z;
                let distance = dx.hypot(dz);
                if distance < 0.02 {
                    break;
                }
                if points.len() >= 8192 {
                    return (points, Some(PathTermination::NodeLimit));
                }
                let seconds = STEP_SECONDS.min(distance / speed);
                let (ax, az, facing) = arc_step(
                    rotation,
                    dx.atan2(dz),
                    speed,
                    seconds,
                    mount.turn_radius().min(distance / 4.0),
                );
                let mut next = Position {
                    x: wrap_world_x(position.x + ax),
                    y: position.y,
                    z: position.z + az,
                };
                next.y = self
                    .surface_ground_y(0, &next, position.y, Some(mount))
                    .await;
                if swept_blocked(&self.passability_read(), position, ax, az, 0) {
                    return (points, Some(PathTermination::Unreachable));
                }
                points.push(MoveWaypoint {
                    position: next,
                    floor_level: 0,
                    rotation: Some(facing),
                    travel_seconds: Some(seconds),
                });
                position = next;
                rotation = facing;
            }
        }
        (points, None)
    }
}
