//! Shared monster AI FSM — used by both WASM (client) and native Rust (agent-client).
//!
//! The FSM is stateful per-monster via [`MonsterBrain`]. Each tick receives
//! external inputs (delta time, nearby players) and returns a list of
//! [`AiCommand`]s that the caller translates into network messages.

use crate::pathfinding::{self, PathResult, PathWaypoint};
use crate::{MonsterState, Position};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// AiTemplate — loaded from data/ai_templates.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTemplate {
    pub idle_move_chance: f32,
    pub idle_check_ms: f32,
    pub min_move_dist: f32,
    pub max_move_dist: f32,
    pub attack_range: f32,
    pub chase_range: f32,
    pub attack_cooldown_ms: f32,
    pub leash_range: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    pub hit_stagger_ms: f32,
    pub flee_health_ratio: f32,
    pub flee_chance: f32,
    pub flee_duration_ms: f32,
    pub return_chance: f32,
    pub return_arrive_dist: f32,
    pub path_recalc_ms: f32,
    pub target_move_threshold: f32,
}

impl Default for AiTemplate {
    fn default() -> Self {
        Self {
            idle_move_chance: 0.3,
            idle_check_ms: 1000.0,
            min_move_dist: 2.0,
            max_move_dist: 10.0,
            attack_range: 2.0,
            chase_range: 25.0,
            attack_cooldown_ms: 1500.0,
            leash_range: 50.0,
            walk_speed: 1.0,
            run_speed: 8.0,
            hit_stagger_ms: 800.0,
            flee_health_ratio: 0.3,
            flee_chance: 0.5,
            flee_duration_ms: 3000.0,
            return_chance: 0.7,
            return_arrive_dist: 5.0,
            path_recalc_ms: 500.0,
            target_move_threshold: 3.0,
        }
    }
}

/// Load AI templates from JSON string (data/ai_templates.json).
pub fn load_templates(json: &str) -> Result<HashMap<String, AiTemplate>, serde_json::Error> {
    serde_json::from_str(json)
}

// ---------------------------------------------------------------------------
// AiState — internal FSM state (superset of network MonsterState)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiState {
    Idle,
    Walk,
    Run,
    Attack,
    Hit,
    Dead,
    Flee,
    Return,
}

impl AiState {
    pub fn to_monster_state(self) -> MonsterState {
        match self {
            AiState::Idle => MonsterState::Idle,
            AiState::Walk => MonsterState::Walk,
            AiState::Run => MonsterState::Run,
            AiState::Attack => MonsterState::Attack,
            AiState::Hit => MonsterState::Hit,
            AiState::Dead => MonsterState::Dead,
            AiState::Flee => MonsterState::Run,
            AiState::Return => MonsterState::Walk,
        }
    }
}

// ---------------------------------------------------------------------------
// NearbyPlayer — minimal player projection for FSM input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyPlayer {
    pub id: String,
    pub position: Position,
    pub health: u32,
}

// ---------------------------------------------------------------------------
// AiCommand — FSM output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AiCommand {
    Move {
        monster_id: String,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
    },
    Attack {
        monster_id: String,
        target_player_id: String,
    },
}

/// Result of a single brain tick — always includes current position/rotation
/// so the caller can update the visual even when no commands are emitted.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickResult {
    pub commands: Vec<AiCommand>,
    pub position: Position,
    pub rotation: f32,
    pub state: MonsterState,
}

// ---------------------------------------------------------------------------
// PathProvider trait — abstracts pathfinding for WASM vs native
// ---------------------------------------------------------------------------

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
            pathfinding::DEFAULT_MAX_NODES,
        )
    }
}

// ---------------------------------------------------------------------------
// MonsterBrain — per-monster FSM instance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterBrain {
    pub monster_id: String,
    pub monster_type: String,
    pub position: Position,
    pub rotation: f32,
    pub health: u32,
    pub max_health: u32,
    state: AiState,
    state_timer_ms: f32,
    target_player_id: Option<String>,
    move_speed: f32,
    target_position: Option<Position>,
    waypoints: Vec<PathWaypoint>,
    current_waypoint_idx: usize,
    path_elapsed_ms: f32,
    last_known_target_pos: Option<Position>,
    spawn_position: Position,
    flee_health_threshold: u32,
}

impl MonsterBrain {
    pub fn new(
        monster_id: String,
        monster_type: String,
        position: Position,
        health: u32,
        max_health: u32,
        template: &AiTemplate,
    ) -> Self {
        Self {
            monster_id,
            monster_type,
            rotation: 0.0,
            health,
            max_health,
            state: AiState::Idle,
            state_timer_ms: 0.0,
            target_player_id: None,
            move_speed: template.walk_speed,
            target_position: None,
            waypoints: Vec::new(),
            current_waypoint_idx: 0,
            path_elapsed_ms: 0.0,
            last_known_target_pos: None,
            spawn_position: position.clone(),
            flee_health_threshold: (max_health as f32 * template.flee_health_ratio) as u32,
            position,
        }
    }

    pub fn state(&self) -> AiState {
        self.state
    }

    pub fn network_state(&self) -> MonsterState {
        self.state.to_monster_state()
    }

    pub fn is_dead(&self) -> bool {
        self.state == AiState::Dead
    }

    // =========================================================================
    // Main tick
    // =========================================================================

    pub fn tick(
        &mut self,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> TickResult {
        if self.state == AiState::Dead || self.health == 0 {
            return TickResult {
                commands: vec![],
                position: self.position.clone(),
                rotation: self.rotation,
                state: self.state.to_monster_state(),
            };
        }

        self.state_timer_ms += delta_ms;
        self.path_elapsed_ms += delta_ms;
        let mut commands = Vec::new();

        match self.state {
            AiState::Idle => self.tick_idle(&mut commands, template, path_provider, rng),
            AiState::Walk | AiState::Run => {
                self.tick_patrol(delta_ms, &mut commands, template, path_provider, rng)
            }
            AiState::Hit => self.tick_hit(&mut commands, template, path_provider, rng),
            AiState::Attack => self.tick_attack(
                delta_ms,
                nearby_players,
                &mut commands,
                template,
                path_provider,
                rng,
            ),
            AiState::Flee => self.tick_flee(delta_ms, &mut commands, template, path_provider, rng),
            AiState::Return => self.tick_return(delta_ms, &mut commands, template, path_provider),
            AiState::Dead => {}
        }

        TickResult {
            commands,
            position: self.position.clone(),
            rotation: self.rotation,
            state: self.state.to_monster_state(),
        }
    }

    // =========================================================================
    // Event handlers
    // =========================================================================

    pub fn handle_hit(
        &mut self,
        attacker_id: &str,
        hit: bool,
        damage: u32,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> Vec<AiCommand> {
        if self.state == AiState::Dead {
            return vec![];
        }

        self.health = self.health.saturating_sub(if hit { damage } else { 0 });
        self.target_player_id = Some(attacker_id.to_string());
        self.move_speed = template.run_speed;

        if self.health == 0 {
            self.state = AiState::Dead;
            return vec![];
        }

        let should_flee =
            self.health <= self.flee_health_threshold && rng.gen::<f32>() < template.flee_chance;

        let mut commands = Vec::new();
        if should_flee {
            if hit {
                // Stagger first, then flee after stagger
                self.state = AiState::Hit;
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_cmd());
            } else {
                self.transition_to_flee(&mut commands, template, path_provider);
            }
        } else if hit {
            self.state = AiState::Hit;
            self.state_timer_ms = 0.0;
            commands.push(self.make_move_cmd());
        } else {
            // Miss: go straight to attack (no stagger)
            self.state = AiState::Attack;
            self.state_timer_ms = 0.0;
            commands.push(self.make_move_cmd());
        }
        commands
    }

    pub fn handle_death(&mut self) {
        self.state = AiState::Dead;
        self.health = 0;
    }

    // =========================================================================
    // State tick methods
    // =========================================================================

    fn tick_idle(
        &mut self,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if self.state_timer_ms < template.idle_check_ms {
            return;
        }
        self.state_timer_ms = 0.0;

        if rng.gen::<f32>() < template.idle_move_chance {
            self.transition_to_move(commands, template, path_provider, rng);
        }
    }

    fn tick_patrol(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if self.target_position.is_none() {
            self.transition_to_idle(commands);
            return;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            if rng.gen::<f32>() < 0.5 {
                self.transition_to_idle(commands);
            } else {
                self.transition_to_move(commands, template, path_provider, rng);
            }
        }
    }

    fn tick_hit(
        &mut self,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if self.state_timer_ms >= template.hit_stagger_ms {
            if self.health <= self.flee_health_threshold && rng.gen::<f32>() < template.flee_chance
            {
                self.transition_to_flee(commands, template, path_provider);
            } else {
                self.state = AiState::Attack;
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_cmd());
            }
        }
    }

    fn tick_attack(
        &mut self,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        let target_id = match &self.target_player_id {
            Some(id) => id.as_str(),
            None => {
                self.transition_to_idle(commands);
                return;
            }
        };

        let target = match nearby_players.iter().find(|p| p.id == target_id) {
            Some(p) if p.health > 0 => p,
            _ => {
                self.target_player_id = None;
                self.transition_to_idle(commands);
                return;
            }
        };

        let dx = target.position.x - self.position.x;
        let dz = target.position.z - self.position.z;
        let dist_sq = dx * dx + dz * dz;
        let chase_range_sq = template.chase_range * template.chase_range;

        // Leash: return to spawn if too far from home
        let spawn_dx = self.position.x - self.spawn_position.x;
        let spawn_dz = self.position.z - self.spawn_position.z;
        let dist_from_spawn_sq = spawn_dx * spawn_dx + spawn_dz * spawn_dz;
        if dist_from_spawn_sq > template.leash_range * template.leash_range {
            self.target_player_id = None;
            self.transition_to_return(commands, template, path_provider, rng);
            return;
        }

        // Give up if target too far
        if dist_sq > chase_range_sq {
            self.target_player_id = None;
            self.transition_to_return(commands, template, path_provider, rng);
            return;
        }

        let attack_range_sq = template.attack_range * template.attack_range;

        if dist_sq <= attack_range_sq {
            self.rotation = dx.atan2(dz);

            if self.state_timer_ms >= template.attack_cooldown_ms {
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_cmd());
                commands.push(AiCommand::Attack {
                    monster_id: self.monster_id.clone(),
                    target_player_id: target_id.to_string(),
                });
            }
        } else {
            self.move_speed = template.run_speed;
            let target_pos = &target.position;

            let needs_repath = self.waypoints.is_empty()
                || self.current_waypoint_idx >= self.waypoints.len()
                || self.path_elapsed_ms > template.path_recalc_ms
                || self.target_moved_significantly(target_pos, template);

            if needs_repath {
                self.compute_path(target_pos.x, target_pos.z, path_provider);
                self.last_known_target_pos = Some(target_pos.clone());
            }

            let reached = self.follow_path(delta_ms);
            if reached && dist_sq > attack_range_sq {
                // Stuck and still not in range — give up
                self.target_player_id = None;
                self.transition_to_idle(commands);
                return;
            }

            commands.push(AiCommand::Move {
                monster_id: self.monster_id.clone(),
                position: self.position.clone(),
                rotation: self.rotation,
                state: MonsterState::Attack,
                target_position: target_pos.clone(),
            });
        }
    }

    fn tick_flee(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if self.state_timer_ms >= template.flee_duration_ms {
            self.target_player_id = None;
            self.transition_to_return(commands, template, path_provider, rng);
            return;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            self.target_player_id = None;
            self.transition_to_return(commands, template, path_provider, rng);
            return;
        }

        commands.push(self.make_move_cmd());
    }

    fn tick_return(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
    ) {
        let dx = self.spawn_position.x - self.position.x;
        let dz = self.spawn_position.z - self.position.z;
        let dist_sq = dx * dx + dz * dz;

        if dist_sq <= template.return_arrive_dist * template.return_arrive_dist {
            self.transition_to_idle(commands);
            return;
        }

        // Repath if needed
        if self.waypoints.is_empty() || self.current_waypoint_idx >= self.waypoints.len() {
            self.compute_path(self.spawn_position.x, self.spawn_position.z, path_provider);
            if self.waypoints.is_empty() {
                self.transition_to_idle(commands);
                return;
            }
        }

        self.follow_path(delta_ms);
        commands.push(self.make_move_cmd());
    }

    // =========================================================================
    // Transition helpers
    // =========================================================================

    fn transition_to_idle(&mut self, commands: &mut Vec<AiCommand>) {
        self.state = AiState::Idle;
        self.state_timer_ms = 0.0;
        self.target_position = None;
        self.waypoints.clear();
        self.current_waypoint_idx = 0;
        commands.push(self.make_move_cmd());
    }

    fn transition_to_move(
        &mut self,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        let dist: f32 = rng.gen_range(template.min_move_dist..template.max_move_dist);

        let target_x = self.position.x + angle.cos() * dist;
        let target_z = self.position.z + angle.sin() * dist;

        // Walk vs run probability based on distance
        let walk_prob = (-0.075 * dist + 0.95).clamp(0.0, 1.0);
        let is_walk = rng.gen::<f32>() < walk_prob;

        if is_walk {
            self.state = AiState::Walk;
            self.move_speed = template.walk_speed;
        } else {
            self.state = AiState::Run;
            self.move_speed = template.run_speed;
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

    fn transition_to_flee(
        &mut self,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
    ) {
        self.state = AiState::Flee;
        self.state_timer_ms = 0.0;
        self.move_speed = template.run_speed;

        self.compute_path(self.spawn_position.x, self.spawn_position.z, path_provider);

        if self.waypoints.is_empty() {
            self.state = AiState::Idle;
            self.state_timer_ms = 0.0;
            return;
        }

        self.face_first_waypoint();

        commands.push(self.make_move_cmd());
    }

    fn transition_to_return(
        &mut self,
        commands: &mut Vec<AiCommand>,
        template: &AiTemplate,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if rng.gen::<f32>() >= template.return_chance {
            self.transition_to_idle(commands);
            return;
        }

        self.state = AiState::Return;
        self.state_timer_ms = 0.0;
        self.move_speed = template.walk_speed;
        self.target_position = Some(self.spawn_position.clone());

        self.compute_path(self.spawn_position.x, self.spawn_position.z, path_provider);

        if self.waypoints.is_empty() {
            self.transition_to_idle(commands);
            return;
        }

        self.face_first_waypoint();

        commands.push(self.make_move_cmd());
    }

    // =========================================================================
    // Movement helpers
    // =========================================================================

    fn face_first_waypoint(&mut self) {
        if let Some(wp) = self.waypoints.first() {
            let wdx = wp.x - self.position.x;
            let wdz = wp.z - self.position.z;
            self.rotation = wdx.atan2(wdz);
        }
    }

    fn compute_path(&mut self, goal_x: f32, goal_z: f32, path_provider: &dyn PathProvider) {
        let result =
            path_provider.find_path(self.position.x, self.position.z, 0, goal_x, goal_z, 0);
        self.waypoints = result.waypoints;
        self.current_waypoint_idx = 0;
        self.path_elapsed_ms = 0.0;
    }

    /// Follow waypoints. Returns true if path is exhausted.
    fn follow_path(&mut self, delta_ms: f32) -> bool {
        if self.current_waypoint_idx >= self.waypoints.len() {
            return true;
        }

        let wp = &self.waypoints[self.current_waypoint_idx];
        let dx = wp.x - self.position.x;
        let dz = wp.z - self.position.z;
        let dist = (dx * dx + dz * dz).sqrt();
        let step = self.move_speed * delta_ms / 1000.0;

        if dist <= step {
            self.position.x = wp.x;
            self.position.z = wp.z;
            self.current_waypoint_idx += 1;

            if self.current_waypoint_idx >= self.waypoints.len() {
                return true;
            }

            let next = &self.waypoints[self.current_waypoint_idx];
            let ndx = next.x - self.position.x;
            let ndz = next.z - self.position.z;
            self.rotation = ndx.atan2(ndz);
        } else {
            let nx = dx / dist;
            let nz = dz / dist;
            self.position.x += nx * step;
            self.position.z += nz * step;
            self.rotation = dx.atan2(dz);
        }

        false
    }

    fn target_moved_significantly(&self, target_pos: &Position, template: &AiTemplate) -> bool {
        match &self.last_known_target_pos {
            None => true,
            Some(last) => {
                let dx = target_pos.x - last.x;
                let dz = target_pos.z - last.z;
                (dx * dx + dz * dz)
                    > template.target_move_threshold * template.target_move_threshold
            }
        }
    }

    fn make_move_cmd(&self) -> AiCommand {
        AiCommand::Move {
            monster_id: self.monster_id.clone(),
            position: self.position.clone(),
            rotation: self.rotation,
            state: self.state.to_monster_state(),
            target_position: self
                .target_position
                .clone()
                .unwrap_or(self.position.clone()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

    /// Dummy PathProvider that always returns an empty path.
    struct NoPath;
    impl PathProvider for NoPath {
        fn find_path(
            &self,
            _sx: f32,
            _sz: f32,
            _sf: u8,
            _gx: f32,
            _gz: f32,
            _gf: u8,
        ) -> PathResult {
            PathResult {
                waypoints: vec![],
                found: false,
            }
        }
    }

    /// PathProvider that returns a straight-line path to the goal.
    struct DirectPath;
    impl PathProvider for DirectPath {
        fn find_path(&self, _sx: f32, _sz: f32, _sf: u8, gx: f32, gz: f32, gf: u8) -> PathResult {
            PathResult {
                waypoints: vec![PathWaypoint {
                    x: gx,
                    z: gz,
                    floor: gf,
                }],
                found: true,
            }
        }
    }

    fn make_brain(template: &AiTemplate) -> MonsterBrain {
        MonsterBrain::new(
            "test_m1".into(),
            "scp939".into(),
            Position {
                x: 10.0,
                y: 0.0,
                z: 10.0,
            },
            10,
            10,
            template,
        )
    }

    #[test]
    fn brain_starts_idle() {
        let t = AiTemplate::default();
        let brain = make_brain(&t);
        assert_eq!(brain.state(), AiState::Idle);
        assert_eq!(brain.network_state(), MonsterState::Idle);
    }

    #[test]
    fn idle_does_not_transition_before_check_interval() {
        let t = AiTemplate::default();
        let mut brain = make_brain(&t);
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick(500.0, &[], &t, &DirectPath, &mut rng);
        assert!(result.commands.is_empty());
        assert_eq!(brain.state(), AiState::Idle);
    }

    #[test]
    fn idle_can_transition_to_move() {
        let mut t = AiTemplate::default();
        t.idle_move_chance = 1.0; // always move
        let mut brain = make_brain(&t);
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick(1001.0, &[], &t, &DirectPath, &mut rng);
        assert!(!result.commands.is_empty());
        assert!(brain.state() == AiState::Walk || brain.state() == AiState::Run);
    }

    #[test]
    fn handle_hit_transitions_to_hit_state() {
        let t = AiTemplate::default();
        let mut brain = make_brain(&t);
        let mut rng = SmallRng::seed_from_u64(42);

        let cmds = brain.handle_hit("player1", true, 3, &t, &DirectPath, &mut rng);
        assert!(!cmds.is_empty());
        assert_eq!(brain.state(), AiState::Hit);
        assert_eq!(brain.health, 7);
    }

    #[test]
    fn handle_hit_death() {
        let t = AiTemplate::default();
        let mut brain = make_brain(&t);
        let mut rng = SmallRng::seed_from_u64(42);

        let cmds = brain.handle_hit("player1", true, 100, &t, &DirectPath, &mut rng);
        assert!(cmds.is_empty()); // dead returns empty
        assert!(brain.is_dead());
        assert_eq!(brain.health, 0);
    }

    #[test]
    fn load_templates_parses_json() {
        let json = r#"{
      "default": {
        "idleMoveChance": 0.3,
        "idleCheckMs": 1000.0,
        "minMoveDist": 2.0,
        "maxMoveDist": 10.0,
        "attackRange": 2.0,
        "chaseRange": 25.0,
        "attackCooldownMs": 1500.0,
        "leashRange": 50.0,
        "walkSpeed": 1.0,
        "runSpeed": 8.0,
        "hitStaggerMs": 800.0,
        "fleeHealthRatio": 0.3,
        "fleeChance": 0.5,
        "fleeDurationMs": 3000.0,
        "returnChance": 0.7,
        "returnArriveDist": 5.0,
        "pathRecalcMs": 500.0,
        "targetMoveThreshold": 3.0
      }
    }"#;

        let templates = load_templates(json).unwrap();
        assert!(templates.contains_key("default"));
        let t = &templates["default"];
        assert_eq!(t.idle_move_chance, 0.3);
        assert_eq!(t.leash_range, 50.0);
    }

    #[test]
    fn attack_chases_nearby_player() {
        let t = AiTemplate::default();
        let mut brain = make_brain(&t);
        let mut rng = SmallRng::seed_from_u64(42);

        // Put brain in attack state with a target
        brain.state = AiState::Attack;
        brain.target_player_id = Some("p1".into());
        brain.move_speed = t.run_speed;

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 15.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let result = brain.tick(50.0, &players, &t, &DirectPath, &mut rng);
        assert!(result
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Move { .. })));
    }
}
