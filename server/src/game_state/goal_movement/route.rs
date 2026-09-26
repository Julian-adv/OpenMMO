use super::*;
use onlinerpg_shared::mount::MountKind;

// Click pivots turn at the pace the horse turn clips are authored for (timeScale 1), so the
// hooves do not slide; keyboard turns keep the faster shared steering rate.
const PIVOT_TURN_RATE: f32 = std::f32::consts::FRAC_PI_2;
const MIN_PIVOT_SECONDS: f32 = 0.5;
const PIVOT_LOOKAHEAD: usize = 8;
const MIN_PIVOT_ANGLE: f32 = 10_f32.to_radians();
const MAX_MOUNT_SAMPLES: usize = 8192;

pub(super) fn line_points(
    from: Position,
    dx: f32,
    dz: f32,
) -> impl ExactSizeIterator<Item = Position> {
    let steps = dx.hypot(dz).ceil().max(1.0) as usize;
    (1..steps + 1).map(move |step| {
        let t = step as f32 / steps as f32;
        Position {
            x: wrap_world_x(from.x + dx * t),
            y: from.y,
            z: from.z + dz * t,
        }
    })
}

fn straight_clear(cache: &PassabilityCache, from: Position, dx: f32, dz: f32) -> bool {
    let mut previous = from;
    line_points(from, dx, dz).all(|point| {
        let (sx, sz) = (
            shortest_world_delta_x(previous.x, point.x),
            point.z - previous.z,
        );
        let blocked = swept_blocked(cache, previous, sx, sz, 0);
        previous = point;
        !blocked
    })
}

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
        let arc = self.mount_arc_route(player, route, mount).await;
        if arc.1 != Some(PathTermination::Unreachable) {
            return arc;
        }
        // Arcs drift off cell centres, so a rider against a fence or entering a narrow gap
        // falls back to turning in place and riding straight.
        let pivot = self.mount_pivot_route(player, route, mount).await;
        if pivot.1.is_none() {
            pivot
        } else {
            arc
        }
    }

    async fn mount_pivot_route(
        &self,
        player: &Player,
        route: &[PathWaypoint],
        mount: MountKind,
    ) -> (Vec<MoveWaypoint>, Option<PathTermination>) {
        use onlinerpg_shared::mount_movement::angle_delta;
        let speed = PLAYER_MOVE_SPEED * mount.speed_mult();
        let mut position = player.position;
        let mut rotation = player.rotation;
        let mut points = Vec::new();
        if route.iter().any(|w| w.floor != 0) {
            return (points, Some(PathTermination::Unreachable));
        }
        let mut next = 0;
        while next < route.len() {
            // Aim at the farthest waypoint in straight sight so the horse turns once, not per cell.
            let last = (next + PIVOT_LOOKAHEAD).min(route.len()) - 1;
            let found = {
                let cache = self.passability_read();
                (next..=last).rev().find_map(|index| {
                    let dx = shortest_world_delta_x(position.x, route[index].x);
                    let dz = route[index].z - position.z;
                    straight_clear(&cache, position, dx, dz).then_some((index, dx, dz))
                })
            };
            let Some((index, dx, dz)) = found else {
                return (points, Some(PathTermination::Unreachable));
            };
            next = index + 1;
            let distance = dx.hypot(dz);
            if distance < 0.02 {
                continue;
            }
            let heading = dx.atan2(dz);
            let turn = angle_delta(rotation, heading);
            rotation = heading;
            if turn.abs() > MIN_PIVOT_ANGLE {
                points.push(MoveWaypoint {
                    position,
                    floor_level: 0,
                    rotation: Some(rotation),
                    travel_seconds: Some((turn.abs() / PIVOT_TURN_RATE).max(MIN_PIVOT_SECONDS)),
                });
            }
            let samples = line_points(position, dx, dz);
            if points.len() + samples.len() > MAX_MOUNT_SAMPLES {
                return (points, Some(PathTermination::NodeLimit));
            }
            let step_seconds = distance / samples.len() as f32 / speed;
            for mut sample in samples {
                sample.y = self
                    .surface_ground_y(0, &sample, position.y, Some(mount))
                    .await;
                points.push(MoveWaypoint {
                    position: sample,
                    floor_level: 0,
                    rotation: Some(rotation),
                    travel_seconds: Some(step_seconds),
                });
                position = sample;
            }
        }
        (points, None)
    }

    async fn mount_arc_route(
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
                if points.len() >= MAX_MOUNT_SAMPLES {
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
                if swept_blocked(&self.passability_read(), position, ax, az, 0) {
                    return (points, Some(PathTermination::Unreachable));
                }
                let mut next = Position {
                    x: wrap_world_x(position.x + ax),
                    y: position.y,
                    z: position.z + az,
                };
                next.y = self
                    .surface_ground_y(0, &next, position.y, Some(mount))
                    .await;
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
