//! Smooth A* paths from the player's continuous position using line of sight.
//! Anchor floor transitions to keep stairwell movement cardinal.

use super::astar::{find_path_avoiding_counted, segment_enters_cells};
use super::query::{
    is_cardinal_move_blocked, is_circle_blocked_on_floor, is_movement_blocked, segment_obstacles,
    SegmentObstacles,
};
use super::{PassabilityCache, PathResult, PathWaypoint};

/// Matches `PLAYER_RADIUS` in the client's `player-physics.ts`.
const PLAYER_RADIUS: f32 = 0.3;

/// Greedy line-of-sight path smoothing. Only smooths within the same floor level.
fn smooth_path(
    waypoints: &[PathWaypoint],
    cache: &PassabilityCache,
    blocked: &[(i32, i32)],
) -> Vec<PathWaypoint> {
    if waypoints.len() <= 2 {
        return waypoints.to_vec();
    }

    let mut result = vec![waypoints[0].clone()];
    let mut anchor = 0;

    while anchor < waypoints.len() - 1 {
        let mut farthest = anchor + 1;

        // Keep the first step after a floor change cardinal.
        let is_floor_transition =
            anchor > 0 && waypoints[anchor].floor != waypoints[anchor - 1].floor;

        if !is_floor_transition {
            for probe in anchor + 2..waypoints.len() {
                if waypoints[probe].floor != waypoints[anchor].floor {
                    break;
                }
                if line_passable_avoiding(&waypoints[anchor], &waypoints[probe], cache, blocked) {
                    farthest = probe;
                } else {
                    break;
                }
            }
        }

        result.push(waypoints[farthest].clone());
        anchor = farthest;
    }

    result
}

/// Check cell edges and body clearance along a smoothing segment.
pub(super) fn is_line_passable(
    from: &PathWaypoint,
    to: &PathWaypoint,
    cache: &PassabilityCache,
) -> bool {
    line_passable_avoiding(from, to, cache, &[])
}

/// `is_line_passable` that also refuses to cross a `blocked` cell.
fn line_passable_avoiding(
    from: &PathWaypoint,
    to: &PathWaypoint,
    cache: &PassabilityCache,
    blocked: &[(i32, i32)],
) -> bool {
    if !cells_line_passable(from, to, cache)
        || segment_enters_cells(from.x, from.z, to.x, to.z, blocked)
        || is_movement_blocked(cache, from.x, from.z, to.x, to.z, from.floor, None)
    {
        return false;
    }
    // Allow endpoints near walls, but only after ruling out actual edge crossings.
    let r = PLAYER_RADIUS;
    if is_circle_blocked_on_floor(cache, from.x, from.z, r, from.floor, None)
        || is_circle_blocked_on_floor(cache, to.x, to.z, r, from.floor, None)
    {
        return true;
    }
    !body_clips_wall(from, to, cache)
}

/// Check the body's swept edges and interior.
fn body_clips_wall(from: &PathWaypoint, to: &PathWaypoint, cache: &PassabilityCache) -> bool {
    let floor = from.floor;
    let r = PLAYER_RADIUS;
    let dx = to.x - from.x;
    let dz = to.z - from.z;
    let len = (dx * dx + dz * dz).sqrt();
    if len < 1e-5 {
        return false;
    }
    let (ox, oz) = (-dz / len * r, dx / len * r);
    if [-1.0, 1.0].into_iter().any(|side| {
        is_movement_blocked(
            cache,
            from.x + ox * side,
            from.z + oz * side,
            to.x + ox * side,
            to.z + oz * side,
            floor,
            None,
        )
    }) {
        return true;
    }
    // Step finer than the radius so a corner notch can't slip between samples.
    let steps = (len / (r * 0.5)).ceil() as i32;
    for i in 1..steps {
        let t = i as f32 / steps as f32;
        if is_circle_blocked_on_floor(cache, from.x + dx * t, from.z + dz * t, r, floor, None) {
            return true;
        }
    }
    false
}

/// Bresenham cell-edge line-of-sight: the original point-thickness check.
fn cells_line_passable(from: &PathWaypoint, to: &PathWaypoint, cache: &PassabilityCache) -> bool {
    let floor = from.floor;
    let x0 = from.x.floor() as i32;
    let z0 = from.z.floor() as i32;
    let x1 = to.x.floor() as i32;
    let z1 = to.z.floor() as i32;

    if x0 == x1 && z0 == z1 {
        return true;
    }

    let dx = (x1 - x0).abs();
    let dz = (z1 - z0).abs();
    let sx = (x1 - x0).signum();
    let sz = (z1 - z0).signum();

    let mut x = x0;
    let mut z = z0;
    let mut err = dx - dz;

    loop {
        if x == x1 && z == z1 {
            return true;
        }
        let e2 = 2 * err;
        let step_x = e2 > -dz;
        let step_z = e2 < dx;

        if step_x && step_z {
            // Diagonal: both L-paths must be clear
            if is_cardinal_move_blocked(cache, x, z, sx, 0, floor)
                || is_cardinal_move_blocked(cache, x + sx, z, 0, sz, floor)
            {
                return false;
            }
            if is_cardinal_move_blocked(cache, x, z, 0, sz, floor)
                || is_cardinal_move_blocked(cache, x, z + sz, sx, 0, floor)
            {
                return false;
            }
            x += sx;
            z += sz;
            err += dx - dz;
        } else if step_x {
            if is_cardinal_move_blocked(cache, x, z, sx, 0, floor) {
                return false;
            }
            x += sx;
            err -= dz;
        } else {
            if is_cardinal_move_blocked(cache, x, z, 0, sz, floor) {
                return false;
            }
            z += sz;
            err += dx;
        }
    }
}

/// Skip A* for clear same-floor segments; stairwell contact requires a search.
fn direct_line_ok(
    start_x: f32,
    start_z: f32,
    goal_x: f32,
    goal_z: f32,
    floor: u8,
    cache: &PassabilityCache,
    blocked: &[(i32, i32)],
) -> bool {
    if segment_enters_cells(start_x, start_z, goal_x, goal_z, blocked) {
        return false;
    }
    let min_x = start_x.min(goal_x);
    let max_x = start_x.max(goal_x);
    let min_z = start_z.min(goal_z);
    let max_z = start_z.max(goal_z);
    match segment_obstacles(cache, min_x, max_x, min_z, max_z, floor, PLAYER_RADIUS) {
        SegmentObstacles::Stairwell => false,
        SegmentObstacles::None => true,
        SegmentObstacles::Walls => {
            let from = PathWaypoint {
                x: start_x,
                z: start_z,
                floor,
            };
            let to = PathWaypoint {
                x: goal_x,
                z: goal_z,
                floor,
            };
            is_line_passable(&from, &to, cache)
        }
    }
}

/// Convenience: find path and smooth it in one call. Endpoint floors follow
/// [`find_path`]'s rule — resolve them with [`super::start_floor_at`].
#[allow(clippy::too_many_arguments)]
pub fn find_and_smooth_path(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
) -> PathResult {
    find_and_smooth_path_avoiding(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes,
        &[],
    )
}

/// Keep both search and smoothing out of `blocked` cells.
#[allow(clippy::too_many_arguments)]
pub fn find_and_smooth_path_avoiding(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
    blocked: &[(i32, i32)],
) -> PathResult {
    find_and_smooth_path_avoiding_with_budget(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes,
        blocked,
        &std::cell::Cell::new(max_nodes),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn find_and_smooth_path_avoiding_with_budget(
    start_x: f32,
    start_z: f32,
    start_floor: u8,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    cache: &PassabilityCache,
    max_nodes: usize,
    blocked: &[(i32, i32)],
    remaining: &std::cell::Cell<usize>,
) -> PathResult {
    if start_floor == goal_floor
        && direct_line_ok(
            start_x,
            start_z,
            goal_x,
            goal_z,
            start_floor,
            cache,
            blocked,
        )
    {
        return PathResult {
            waypoints: vec![PathWaypoint {
                x: goal_x,
                z: goal_z,
                floor: goal_floor,
            }],
            found: true,
            termination: crate::pathfinding::PathTermination::Reached,
        };
    }

    if remaining.get() == 0 {
        return PathResult {
            waypoints: vec![],
            found: false,
            termination: crate::pathfinding::PathTermination::NodeLimit,
        };
    }
    let mut expanded = 0;
    let result = find_path_avoiding_counted(
        start_x,
        start_z,
        start_floor,
        goal_x,
        goal_z,
        goal_floor,
        cache,
        max_nodes.min(remaining.get()),
        blocked,
        &mut expanded,
    );
    remaining.set(remaining.get().saturating_sub(expanded));
    if result.waypoints.is_empty() {
        return result;
    }
    let start = PathWaypoint {
        x: start_x,
        z: start_z,
        floor: start_floor,
    };
    let center = PathWaypoint {
        x: start_x.floor() + 0.5,
        z: start_z.floor() + 0.5,
        floor: start_floor,
    };
    let first = &result.waypoints[0];
    let mut full_path = Vec::with_capacity(result.waypoints.len() + 2);
    full_path.push(start.clone());
    if first.floor == start_floor
        && !line_passable_avoiding(&start, first, cache, blocked)
        && !is_circle_blocked_on_floor(cache, center.x, center.z, PLAYER_RADIUS, start_floor, None)
        && line_passable_avoiding(&start, &center, cache, blocked)
        && line_passable_avoiding(&center, first, cache, blocked)
    {
        full_path.push(center);
    }
    full_path.extend(result.waypoints);
    let mut smoothed = smooth_path(&full_path, cache, blocked);
    smoothed.remove(0);
    PathResult {
        waypoints: smoothed,
        found: result.found,
        termination: result.termination,
    }
}
