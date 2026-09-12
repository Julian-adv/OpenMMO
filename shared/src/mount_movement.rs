use std::f32::consts::{FRAC_PI_2, PI, TAU};

const TURN_RATE: f32 = FRAC_PI_2 / 0.6;
const REVERSE_RATE: f32 = FRAC_PI_2 / 0.4;
const MOVE_ANGLE: f32 = PI / 6.0;
pub const TURN_RADIUS: f32 = 0.65;
pub const STEP_SECONDS: f32 = 1.0 / 60.0;
pub const ARRIVAL_DISTANCE: f32 = 0.05;
pub const BACKWARD_SPEED: f32 = 1.5;

pub fn angle_delta(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

pub fn turn_duration(angle: f32) -> f32 {
    let angle = angle.abs();
    (angle - FRAC_PI_2).max(0.0) / REVERSE_RATE + angle.min(FRAC_PI_2) / TURN_RATE
}

fn movement_credit(angle: f32) -> f32 {
    let angle = angle.min(MOVE_ANGLE);
    (angle / 2.0 + MOVE_ANGLE * (PI * angle / MOVE_ANGLE).sin() / (2.0 * PI)) / TURN_RATE
}

/// Conservative turn and detour allowance for external-client movement timers.
pub fn turn_delay(from: f32, to: f32, speed: f32) -> f32 {
    let angle = angle_delta(from, to).abs();
    turn_duration(angle) + TURN_RADIUS * angle / speed.max(0.01)
}

/// Angular steering and the extra forward travel available while aligning.
pub fn steer(from: f32, to: f32, dt: f32) -> (f32, f32) {
    let dt = dt.max(0.0);
    let delta = angle_delta(from, to);
    let angle = delta.abs();
    let remaining = (turn_duration(angle) - dt).max(0.0);
    let slow_time = FRAC_PI_2 / TURN_RATE;
    let next_angle = if remaining > slow_time {
        FRAC_PI_2 + (remaining - slow_time) * REVERSE_RATE
    } else {
        remaining * TURN_RATE
    };
    let rotation = from + delta.signum() * (angle - next_angle);
    let travel_time =
        (dt - turn_duration(angle)).max(0.0) + movement_credit(angle) - movement_credit(next_angle);
    (rotation, travel_time.clamp(0.0, dt))
}

/// Integrate an arc along the horse's facing, then add forward acceleration.
pub fn arc_step(from: f32, to: f32, speed: f32, dt: f32, radius: f32) -> (f32, f32, f32) {
    let (rotation, travel) = steer(from, to, dt);
    let radius = radius.min(speed.max(0.0) / REVERSE_RATE);
    let signed_radius = radius * angle_delta(from, to).signum();
    let aligned_time = (dt - turn_duration(angle_delta(from, to))).max(0.0);
    let forward = (speed * travel - radius * TURN_RATE * (travel - aligned_time)).max(0.0);
    (
        signed_radius * (from.cos() - rotation.cos()) + to.sin() * forward,
        signed_radius * (rotation.sin() - from.sin()) + to.cos() * forward,
        rotation,
    )
}

/// Find room behind the horse for the first turn of a new route.
pub fn recovery_target(
    cache: &crate::pathfinding::PassabilityCache,
    start: crate::Position,
    rotation: f32,
    goal: crate::Position,
    speed: f32,
) -> Option<crate::Position> {
    use crate::{pathfinding, shortest_world_delta_x, wrap_world_x, Position};
    if !rotation.is_finite() || !speed.is_finite() || speed <= 0.0 {
        return None;
    }
    for n in 1..=8 {
        let distance = n as f32 * 0.25;
        let end = Position {
            x: wrap_world_x(start.x - rotation.sin() * distance),
            z: start.z - rotation.cos() * distance,
            y: start.y,
        };
        if pathfinding::is_movement_blocked_for_mover(
            cache,
            start.x,
            start.z,
            start.x + shortest_world_delta_x(start.x, end.x),
            end.z,
            0,
            Some(start.y),
        ) {
            break;
        }
        let route =
            pathfinding::find_and_smooth_path(end.x, end.z, 0, goal.x, goal.z, 0, cache, 2000);
        let Some(wp) = route
            .waypoints
            .iter()
            .find(|wp| shortest_world_delta_x(end.x, wp.x).hypot(wp.z - end.z) > ARRIVAL_DISTANCE)
        else {
            continue;
        };
        if wp.floor != 0 {
            continue;
        }
        let mut p = end;
        let mut facing = rotation;
        for _ in 0..600 {
            let dx = shortest_world_delta_x(p.x, wp.x);
            let dz = wp.z - p.z;
            let dist = dx.hypot(dz);
            let desired = dx.atan2(dz);
            let (ax, az, next_rotation) = arc_step(
                facing,
                desired,
                speed,
                STEP_SECONDS,
                TURN_RADIUS.min(dist / 4.0),
            );
            let snap = dist <= ARRIVAL_DISTANCE
                || (angle_delta(facing, desired).abs() < 1e-4 && speed * STEP_SECONDS >= dist);
            let next = if snap {
                Position {
                    x: p.x + dx,
                    z: wp.z,
                    y: p.y,
                }
            } else {
                Position {
                    x: p.x + ax,
                    z: p.z + az,
                    y: p.y,
                }
            };
            if pathfinding::is_movement_blocked_for_mover(
                cache,
                p.x,
                p.z,
                next.x,
                next.z,
                0,
                Some(p.y),
            ) {
                break;
            }
            if snap
                || (angle_delta(next_rotation, desired).abs() < 1e-4
                    && !pathfinding::is_movement_blocked_for_mover(
                        cache,
                        next.x,
                        next.z,
                        wp.x,
                        wp.z,
                        0,
                        Some(next.y),
                    ))
            {
                return Some(end);
            }
            p = next;
            facing = next_rotation;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_has_a_radius_and_mirrors_without_exceeding_movement_speed() {
        let right = arc_step(0.0, FRAC_PI_2, 6.0, 0.2, TURN_RADIUS);
        let left = arc_step(0.0, -FRAC_PI_2, 6.0, 0.2, TURN_RADIUS);
        assert!((right.0 - TURN_RADIUS * (1.0 - (PI / 6.0).cos())).abs() < 1e-5);
        assert!((right.1 - 0.325).abs() < 1e-5);
        assert!((right.0 + left.0).abs() < 1e-5);
        assert!((right.1 - left.1).abs() < 1e-5);
        for speed in [0.0, 0.5, 3.0, 6.0, 9.0] {
            for angle in [0.01, 0.2, 1.0, 2.0, PI] {
                let step = arc_step(0.0, angle, speed, STEP_SECONDS, TURN_RADIUS);
                assert!(step.0.hypot(step.1) <= speed * STEP_SECONDS + 1e-5);
            }
        }
    }

    #[test]
    fn turns_have_the_same_timing_in_both_directions() {
        for sign in [-1.0, 1.0] {
            let (rotation, travel) = steer(0.0, sign * FRAC_PI_2, 0.2);
            assert!((rotation - sign * PI / 6.0).abs() < 1e-5);
            assert!(travel < 1e-5);
            let (rotation, travel) = steer(0.0, sign * FRAC_PI_2, 0.6);
            assert!((rotation - sign * FRAC_PI_2).abs() < 1e-5);
            assert!((travel - 0.1).abs() < 1e-5);
        }
        assert!((turn_duration(PI) - 1.0).abs() < 1e-5);
        assert!((angle_delta(PI - 0.1, -PI + 0.1) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn travel_and_facing_are_independent_of_tick_size() {
        for target in [0.1, FRAC_PI_2, PI, -2.0] {
            let expected = steer(0.0, target, 1.2);
            let mut rotation = 0.0;
            let mut travel = 0.0;
            for _ in 0..120 {
                let step = steer(rotation, target, 0.01);
                rotation = step.0;
                travel += step.1;
            }
            assert!(angle_delta(rotation, expected.0).abs() < 1e-4);
            assert!((travel - expected.1).abs() < 1e-4);
        }
    }
}
