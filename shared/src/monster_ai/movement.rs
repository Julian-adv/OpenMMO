//! State-transition and path-following helpers for [`MonsterBrain`]: entering
//! idle/move/flee states, computing and following waypoint paths, and facing
//! the next waypoint.

use super::{AiCommand, AiState, MonsterBrain, PathProvider};
use crate::world::{shortest_world_delta_x, shortest_world_delta_z, wrap_world_x, wrap_world_z};
use crate::Position;
use rand::Rng;

impl MonsterBrain {
    // =========================================================================
    // Transition helpers
    // =========================================================================

    pub(super) fn transition_to_idle(&mut self, commands: &mut Vec<AiCommand>) {
        self.state = AiState::Idle;
        self.state_timer_ms = 0.0;
        self.target_position = None;
        self.waypoints.clear();
        self.current_waypoint_idx = 0;
        commands.push(self.make_move_cmd());
    }

    pub(super) fn transition_to_move(
        &mut self,
        commands: &mut Vec<AiCommand>,
        min_move_dist: f32,
        max_move_dist: f32,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        let dist: f32 = rng.gen_range(min_move_dist..max_move_dist);

        let target_x = self.position.x + angle.cos() * dist;
        let target_z = self.position.z + angle.sin() * dist;

        // Walk vs run probability based on distance
        let walk_prob = (-0.075 * dist + 0.95).clamp(0.0, 1.0);
        let is_walk = rng.gen::<f32>() < walk_prob;

        if is_walk {
            self.state = AiState::Walk;
            self.move_speed = self.walk_speed;
        } else {
            self.state = AiState::Run;
            self.move_speed = self.run_speed;
        }

        self.state_timer_ms = 0.0;
        self.target_position = Some(Position {
            x: target_x,
            y: self.position.y,
            z: target_z,
        });

        self.compute_path(target_x, target_z, path_provider);

        if self.waypoints.is_empty() {
            self.state = AiState::Idle;
            self.target_position = None;
            return;
        }

        self.face_first_waypoint();

        // target_position was set above, safe to unwrap
        commands.push(AiCommand::Move {
            monster_id: self.monster_id.clone(),
            position: self.position,
            rotation: self.rotation,
            state: self.state.to_monster_state(),
            target_position: self.target_position.unwrap(),
        });
    }

    pub(super) fn transition_to_flee(
        &mut self,
        safe_dist: f32,
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
    ) {
        self.state = AiState::Flee;
        self.state_timer_ms = 0.0;
        self.move_speed = self.run_speed;

        self.start_flee_path(safe_dist, path_provider);

        if self.waypoints.is_empty() {
            self.state = AiState::Idle;
            self.state_timer_ms = 0.0;
            self.target_position = None;
            return;
        }

        commands.push(self.make_move_cmd());
    }

    /// Pick a flee leg pointing directly away from the last known threat
    /// position, long enough to end up outside `safe_dist`. Falls back to the
    /// spawn point when the threat position is unknown or the away path is
    /// blocked. Leaves `waypoints` empty when no path is available.
    pub(super) fn start_flee_path(&mut self, safe_dist: f32, path_provider: &dyn PathProvider) {
        if let Some(threat) = self.last_known_target_pos {
            let dx = shortest_world_delta_x(threat.x, self.position.x);
            let dz = shortest_world_delta_z(threat.z, self.position.z);
            let dist = (dx * dx + dz * dz).sqrt();
            if dist > f32::EPSILON {
                let dest_x = self.position.x + dx / dist * safe_dist;
                let dest_z = self.position.z + dz / dist * safe_dist;
                if self.try_path_to(dest_x, dest_z, path_provider) {
                    return;
                }
            }
        }

        if !self.try_path_to(self.spawn_position.x, self.spawn_position.z, path_provider) {
            self.target_position = None;
        }
    }

    /// Set `target_position`, path to it, and face the first waypoint.
    /// Returns false (leaving `waypoints` empty) when no path is available.
    fn try_path_to(&mut self, x: f32, z: f32, path_provider: &dyn PathProvider) -> bool {
        self.target_position = Some(Position {
            x,
            y: self.position.y,
            z,
        });
        self.compute_path(x, z, path_provider);
        if self.waypoints.is_empty() {
            return false;
        }
        self.face_first_waypoint();
        true
    }

    // =========================================================================
    // Movement helpers
    // =========================================================================

    pub(super) fn face_first_waypoint(&mut self) {
        if let Some(wp) = self.waypoints.first() {
            let wdx = shortest_world_delta_x(self.position.x, wp.x);
            let wdz = shortest_world_delta_z(self.position.z, wp.z);
            self.rotation = wdx.atan2(wdz);
        }
    }

    pub(super) fn compute_path(
        &mut self,
        goal_x: f32,
        goal_z: f32,
        path_provider: &dyn PathProvider,
    ) {
        // Unwrap the goal next to the monster so the non-periodic A* sees a
        // short local segment instead of a world-spanning one when the goal
        // sits across a seam.
        let goal_x = self.position.x + shortest_world_delta_x(self.position.x, goal_x);
        let goal_z = self.position.z + shortest_world_delta_z(self.position.z, goal_z);
        let result = path_provider.find_path(
            self.position.x,
            self.position.z,
            self.path_floor,
            goal_x,
            goal_z,
            self.path_floor,
        );
        self.waypoints = result.waypoints;
        self.current_waypoint_idx = 0;
        self.path_elapsed_ms = 0.0;
    }

    /// Follow waypoints. Returns true if path is exhausted.
    pub(super) fn follow_path(&mut self, delta_ms: f32) -> bool {
        if self.current_waypoint_idx >= self.waypoints.len() {
            return true;
        }

        let wp = &self.waypoints[self.current_waypoint_idx];
        let dx = shortest_world_delta_x(self.position.x, wp.x);
        let dz = shortest_world_delta_z(self.position.z, wp.z);
        let dist = (dx * dx + dz * dz).sqrt();
        let step = self.move_speed * delta_ms / 1000.0;

        if dist <= step {
            self.position.x = wrap_world_x(wp.x);
            self.position.z = wrap_world_z(wp.z);
            self.current_waypoint_idx += 1;

            if self.current_waypoint_idx >= self.waypoints.len() {
                return true;
            }

            let next = &self.waypoints[self.current_waypoint_idx];
            let ndx = shortest_world_delta_x(self.position.x, next.x);
            let ndz = shortest_world_delta_z(self.position.z, next.z);
            self.rotation = ndx.atan2(ndz);
        } else {
            let nx = dx / dist;
            let nz = dz / dist;
            self.position.x = wrap_world_x(self.position.x + nx * step);
            self.position.z = wrap_world_z(self.position.z + nz * step);
            self.rotation = dx.atan2(dz);
        }

        false
    }

    pub(super) fn target_moved_significantly_by(
        &self,
        target_pos: &Position,
        threshold: f32,
    ) -> bool {
        match &self.last_known_target_pos {
            None => true,
            Some(last) => {
                let dx = shortest_world_delta_x(last.x, target_pos.x);
                let dz = shortest_world_delta_z(last.z, target_pos.z);
                (dx * dx + dz * dz) > threshold * threshold
            }
        }
    }
}
