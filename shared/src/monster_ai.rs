//! Shared monster AI behavior tree runtime — used by both WASM (client) and native Rust (agent-client).
//!
//! The runtime is stateful per-monster via [`MonsterBrain`]. Each tick receives
//! external inputs (delta time, nearby players) and returns a list of
//! [`AiCommand`]s that the caller translates into network messages.

use crate::pathfinding::{self, PathResult, PathWaypoint};
use crate::{MonsterState, Position};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_IDLE_CHECK_MS: f32 = 1000.0;
const DEFAULT_MIN_MOVE_DIST: f32 = 2.0;
const DEFAULT_MAX_MOVE_DIST: f32 = 10.0;
pub const DEFAULT_WALK_SPEED: f32 = 1.0;
pub const DEFAULT_RUN_SPEED: f32 = 8.0;
pub const DEFAULT_ATTACK_RANGE: f32 = 2.0;
pub const DEFAULT_CHASE_RANGE: f32 = 25.0;
pub const DEFAULT_ATTACK_COOLDOWN_MS: f32 = 1500.0;
const DEFAULT_LEASH_RANGE: f32 = 50.0;
const DEFAULT_HIT_STAGGER_MS: f32 = 800.0;
const DEFAULT_FLEE_HEALTH_RATIO: f32 = 0.0;
const DEFAULT_FLEE_MAX_DURATION_MS: f32 = 15000.0;
const FLEE_SAFE_DIST_MARGIN: f32 = 5.0;
const DEFAULT_RETURN_ARRIVE_DIST: f32 = 5.0;
const DEFAULT_PATH_RECALC_MS: f32 = 500.0;
const DEFAULT_TARGET_MOVE_THRESHOLD: f32 = 3.0;
pub const DEFAULT_BEHAVIOR: &str = "brave";

// ---------------------------------------------------------------------------
// BehaviorTree — loaded from data-src/behavior_trees.json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BehaviorTreeFile {
    #[serde(default)]
    pub schema_version: u32,
    pub trees: HashMap<String, BehaviorTree>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BehaviorTree {
    #[serde(default)]
    pub description: Option<String>,
    pub root: BehaviorNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BehaviorNode {
    Selector {
        children: Vec<BehaviorNode>,
    },
    Sequence {
        children: Vec<BehaviorNode>,
    },
    Condition {
        name: String,
        #[serde(default)]
        params: HashMap<String, f32>,
    },
    Action {
        name: String,
        #[serde(default)]
        params: HashMap<String, f32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BehaviorStatus {
    Success,
    Failure,
    Running,
}

impl From<bool> for BehaviorStatus {
    fn from(passed: bool) -> Self {
        if passed {
            BehaviorStatus::Success
        } else {
            BehaviorStatus::Failure
        }
    }
}

/// Load behavior trees from JSON string (data-src/behavior_trees.json).
pub fn load_behavior_trees(json: &str) -> Result<HashMap<String, BehaviorTree>, serde_json::Error> {
    let file: BehaviorTreeFile = serde_json::from_str(json)?;
    Ok(file.trees)
}

pub fn behavior_tree_for<'a>(
    trees: &'a HashMap<String, BehaviorTree>,
    behavior: &str,
) -> Option<&'a BehaviorTree> {
    trees.get(behavior).or_else(|| trees.get(DEFAULT_BEHAVIOR))
}

// ---------------------------------------------------------------------------
// AiState — internal behavior state (superset of network MonsterState)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AiState {
    Idle,
    Walk,
    Run,
    Chase,
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
            AiState::Chase => MonsterState::Run,
            AiState::Attack => MonsterState::Attack,
            AiState::Hit => MonsterState::Hit,
            AiState::Dead => MonsterState::Dead,
            AiState::Flee => MonsterState::Run,
            AiState::Return => MonsterState::Walk,
        }
    }
}

// ---------------------------------------------------------------------------
// NearbyPlayer — minimal player projection for behavior input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NearbyPlayer {
    pub id: String,
    pub position: Position,
    pub health: u32,
}

// ---------------------------------------------------------------------------
// AiCommand — behavior output
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
// MonsterBrain — per-monster behavior tree instance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonsterBrain {
    pub monster_id: String,
    pub monster_type: String,
    pub behavior: String,
    pub position: Position,
    pub rotation: f32,
    pub health: u32,
    pub max_health: u32,
    state: AiState,
    state_timer_ms: f32,
    target_player_id: Option<String>,
    walk_speed: f32,
    run_speed: f32,
    attack_range: f32,
    chase_range: f32,
    attack_cooldown_ms: f32,
    move_speed: f32,
    target_position: Option<Position>,
    waypoints: Vec<PathWaypoint>,
    current_waypoint_idx: usize,
    path_elapsed_ms: f32,
    last_known_target_pos: Option<Position>,
    spawn_position: Position,
}

impl MonsterBrain {
    pub fn new(
        monster_id: String,
        monster_type: String,
        behavior: String,
        position: Position,
        health: u32,
        max_health: u32,
        walk_speed: f32,
        run_speed: f32,
        attack_range: f32,
        chase_range: f32,
        attack_cooldown_ms: f32,
    ) -> Self {
        Self {
            monster_id,
            monster_type,
            behavior,
            rotation: 0.0,
            health,
            max_health,
            state: AiState::Idle,
            state_timer_ms: 0.0,
            target_player_id: None,
            walk_speed,
            run_speed,
            attack_range: if attack_range > 0.0 {
                attack_range
            } else {
                DEFAULT_ATTACK_RANGE
            },
            chase_range: if chase_range > 0.0 {
                chase_range
            } else {
                DEFAULT_CHASE_RANGE
            },
            attack_cooldown_ms,
            move_speed: walk_speed,
            target_position: None,
            waypoints: Vec::new(),
            current_waypoint_idx: 0,
            path_elapsed_ms: 0.0,
            last_known_target_pos: None,
            spawn_position: position.clone(),
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

    /// Build a `TickResult` snapshot of the brain's current pose plus `commands`.
    fn tick_result(&self, commands: Vec<AiCommand>) -> TickResult {
        TickResult {
            commands,
            position: self.position.clone(),
            rotation: self.rotation,
            state: self.state.to_monster_state(),
        }
    }

    pub fn tick_with_behavior_tree(
        &mut self,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        behavior_tree: &BehaviorTree,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> TickResult {
        if self.state == AiState::Dead || self.health == 0 {
            return self.tick_result(vec![]);
        }

        self.state_timer_ms += delta_ms;
        self.path_elapsed_ms += delta_ms;
        let mut commands = Vec::new();

        if self.state == AiState::Hit {
            if self.state_timer_ms < DEFAULT_HIT_STAGGER_MS {
                return self.tick_result(commands);
            }
            self.state = AiState::Idle;
            self.state_timer_ms = 0.0;
        }

        if matches!(self.state, AiState::Walk | AiState::Run) {
            self.tick_patrol(delta_ms, &mut commands, path_provider, rng);
        } else {
            let status = self.eval_behavior_node(
                &behavior_tree.root,
                delta_ms,
                nearby_players,
                &mut commands,
                path_provider,
                rng,
            );

            if status == BehaviorStatus::Failure && self.state != AiState::Idle {
                self.transition_to_idle(&mut commands);
            }
        }

        self.tick_result(commands)
    }

    // =========================================================================
    // Event handlers
    // =========================================================================

    /// Apply incoming damage and acquire `attacker_id` as the target. Returns
    /// `false` if the brain is already dead or the hit was lethal (in which
    /// case no further reaction should be produced).
    fn apply_hit(&mut self, attacker_id: &str, hit: bool, damage: u32) -> bool {
        if self.state == AiState::Dead {
            return false;
        }

        self.health = self.health.saturating_sub(if hit { damage } else { 0 });
        self.target_player_id = Some(attacker_id.to_string());
        self.move_speed = self.run_speed;

        if self.health == 0 {
            self.state = AiState::Dead;
            return false;
        }

        true
    }

    pub fn handle_hit_with_behavior_tree(
        &mut self,
        attacker_id: &str,
        hit: bool,
        damage: u32,
    ) -> Vec<AiCommand> {
        if !self.apply_hit(attacker_id, hit, damage) {
            return vec![];
        }

        if hit {
            self.state = AiState::Hit;
            self.state_timer_ms = 0.0;
            vec![self.make_move_cmd()]
        } else {
            vec![]
        }
    }

    pub fn handle_death(&mut self) {
        self.state = AiState::Dead;
        self.health = 0;
    }

    // =========================================================================
    fn tick_patrol(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
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
                self.transition_to_move(
                    commands,
                    DEFAULT_MIN_MOVE_DIST,
                    DEFAULT_MAX_MOVE_DIST,
                    path_provider,
                    rng,
                );
            }
        }
    }

    // =========================================================================
    // Behavior tree execution
    // =========================================================================

    fn eval_behavior_node(
        &mut self,
        node: &BehaviorNode,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> BehaviorStatus {
        match node {
            BehaviorNode::Selector { children } => {
                for child in children {
                    match self.eval_behavior_node(
                        child,
                        delta_ms,
                        nearby_players,
                        commands,
                        path_provider,
                        rng,
                    ) {
                        BehaviorStatus::Failure => {}
                        status => return status,
                    }
                }
                BehaviorStatus::Failure
            }
            BehaviorNode::Sequence { children } => {
                for child in children {
                    match self.eval_behavior_node(
                        child,
                        delta_ms,
                        nearby_players,
                        commands,
                        path_provider,
                        rng,
                    ) {
                        BehaviorStatus::Success => {}
                        status => return status,
                    }
                }
                BehaviorStatus::Success
            }
            BehaviorNode::Condition { name, params } => {
                self.eval_condition(name, params, nearby_players, rng)
            }
            BehaviorNode::Action { name, params } => self.eval_action(
                name,
                params,
                delta_ms,
                nearby_players,
                commands,
                path_provider,
                rng,
            ),
        }
    }

    fn eval_condition(
        &mut self,
        name: &str,
        params: &HashMap<String, f32>,
        nearby_players: &[NearbyPlayer],
        rng: &mut impl Rng,
    ) -> BehaviorStatus {
        match name {
            "has_target" => self.current_target(nearby_players).is_some().into(),
            "target_in_range" => {
                let range = param(params, "range", self.chase_range);
                self.select_target_in_range(nearby_players, range).into()
            }
            "is_beyond_leash" => {
                let range = param(params, "range", DEFAULT_LEASH_RANGE);
                let dx = self.position.x - self.spawn_position.x;
                let dz = self.position.z - self.spawn_position.z;
                (self.state == AiState::Return || dx * dx + dz * dz > range * range).into()
            }
            "health_below_ratio" => {
                let ratio = param(params, "ratio", DEFAULT_FLEE_HEALTH_RATIO);
                let health_ratio = if self.max_health == 0 {
                    0.0
                } else {
                    self.health as f32 / self.max_health as f32
                };
                (self.state == AiState::Flee || health_ratio <= ratio).into()
            }
            "chance" => {
                let probability = param(params, "probability", 0.0).clamp(0.0, 1.0);
                (matches!(self.state, AiState::Flee | AiState::Return)
                    || rng.gen::<f32>() < probability)
                    .into()
            }
            _ => BehaviorStatus::Failure,
        }
    }

    fn eval_action(
        &mut self,
        name: &str,
        params: &HashMap<String, f32>,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> BehaviorStatus {
        match name {
            "idle" => {
                if self.state != AiState::Idle {
                    self.transition_to_idle(commands);
                }
                BehaviorStatus::Success
            }
            "wander" => self.bt_wander(params, commands, path_provider, rng),
            "return_to_spawn" => self.bt_return_to_spawn(params, delta_ms, commands, path_provider),
            "flee_from_target" => {
                self.bt_flee_from_target(params, delta_ms, nearby_players, commands, path_provider)
            }
            "attack_target" => self.bt_attack_target(params, nearby_players, commands),
            "chase_target" => {
                self.bt_chase_target(params, delta_ms, nearby_players, commands, path_provider)
            }
            _ => BehaviorStatus::Failure,
        }
    }

    fn bt_wander(
        &mut self,
        params: &HashMap<String, f32>,
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> BehaviorStatus {
        let check_ms = param(params, "checkMs", DEFAULT_IDLE_CHECK_MS);
        if self.state_timer_ms < check_ms {
            return BehaviorStatus::Failure;
        }

        let min_move_dist = param(params, "minMoveDist", DEFAULT_MIN_MOVE_DIST);
        let max_move_dist = param(params, "maxMoveDist", DEFAULT_MAX_MOVE_DIST);

        self.state_timer_ms = 0.0;
        self.transition_to_move(commands, min_move_dist, max_move_dist, path_provider, rng);
        if matches!(self.state, AiState::Walk | AiState::Run) {
            BehaviorStatus::Running
        } else {
            BehaviorStatus::Failure
        }
    }

    fn bt_return_to_spawn(
        &mut self,
        params: &HashMap<String, f32>,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
    ) -> BehaviorStatus {
        let arrive_dist = param(params, "arriveDist", DEFAULT_RETURN_ARRIVE_DIST);
        let dx = self.spawn_position.x - self.position.x;
        let dz = self.spawn_position.z - self.position.z;
        if dx * dx + dz * dz <= arrive_dist * arrive_dist {
            self.transition_to_idle(commands);
            return BehaviorStatus::Success;
        }

        if self.state != AiState::Return {
            self.state = AiState::Return;
            self.state_timer_ms = 0.0;
            self.move_speed = self.walk_speed;
            self.target_position = Some(self.spawn_position.clone());
            self.compute_path(self.spawn_position.x, self.spawn_position.z, path_provider);
            if self.waypoints.is_empty() {
                self.transition_to_idle(commands);
                return BehaviorStatus::Failure;
            }
            self.face_first_waypoint();
        }

        self.follow_path(delta_ms);
        commands.push(self.make_move_cmd());
        BehaviorStatus::Running
    }

    fn bt_flee_from_target(
        &mut self,
        params: &HashMap<String, f32>,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
    ) -> BehaviorStatus {
        if self.target_player_id.is_none() && self.state != AiState::Flee {
            return BehaviorStatus::Failure;
        }

        // Flee until the threat is outside its sight range, not for a fixed time.
        // maxDurationMs is only a failsafe against fleeing forever from a chaser.
        let safe_dist = param(params, "safeDist", self.chase_range + FLEE_SAFE_DIST_MARGIN);
        let max_duration_ms = param(params, "maxDurationMs", DEFAULT_FLEE_MAX_DURATION_MS);

        if let Some(target) = self.current_target(nearby_players) {
            self.last_known_target_pos = Some(target.position.clone());
        }

        if self.beyond_safe_dist(safe_dist) {
            self.finish_flee(commands);
            return BehaviorStatus::Success;
        }

        if self.state != AiState::Flee {
            self.transition_to_flee(safe_dist, commands, path_provider);
            if self.state != AiState::Flee {
                return BehaviorStatus::Failure;
            }
            return BehaviorStatus::Running;
        }

        if self.state_timer_ms >= max_duration_ms {
            self.finish_flee(commands);
            return BehaviorStatus::Success;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            if self.last_known_target_pos.is_none() || self.beyond_safe_dist(safe_dist) {
                self.finish_flee(commands);
                return BehaviorStatus::Success;
            }
            // Path ran out while the threat can still see us — start a new leg.
            self.start_flee_path(safe_dist, path_provider);
            if self.waypoints.is_empty() {
                self.finish_flee(commands);
                return BehaviorStatus::Success;
            }
        }

        commands.push(self.make_move_cmd());
        BehaviorStatus::Running
    }

    /// True when the last known threat position is far enough away to stop
    /// fleeing. Returns false while the threat position is unknown.
    fn beyond_safe_dist(&self, safe_dist: f32) -> bool {
        match &self.last_known_target_pos {
            Some(threat) => self.position.dist_xz_sq(threat) >= safe_dist * safe_dist,
            None => false,
        }
    }

    fn finish_flee(&mut self, commands: &mut Vec<AiCommand>) {
        self.target_player_id = None;
        self.last_known_target_pos = None;
        self.transition_to_idle(commands);
    }

    fn bt_attack_target(
        &mut self,
        params: &HashMap<String, f32>,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
    ) -> BehaviorStatus {
        let target = match self.current_target(nearby_players) {
            Some(target) => target,
            None => return BehaviorStatus::Failure,
        };

        let dx = target.position.x - self.position.x;
        let dz = target.position.z - self.position.z;
        let range = param(params, "range", self.attack_range);
        if dx * dx + dz * dz > range * range {
            return BehaviorStatus::Failure;
        }

        let target_id = target.id.clone();
        self.rotation = dx.atan2(dz);
        if self.state != AiState::Attack {
            self.state = AiState::Attack;
            self.state_timer_ms = self.attack_cooldown_ms;
            self.target_position = None;
            self.waypoints.clear();
            commands.push(self.make_move_cmd());
        }

        if self.state_timer_ms >= self.attack_cooldown_ms {
            self.state_timer_ms = 0.0;
            commands.push(self.make_move_cmd());
            commands.push(AiCommand::Attack {
                monster_id: self.monster_id.clone(),
                target_player_id: target_id,
            });
        }

        BehaviorStatus::Running
    }

    fn bt_chase_target(
        &mut self,
        params: &HashMap<String, f32>,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
    ) -> BehaviorStatus {
        let target = match self.current_target(nearby_players) {
            Some(target) => target,
            None => return BehaviorStatus::Failure,
        };

        let target_pos = target.position.clone();
        let path_recalc_ms = param(params, "pathRecalcMs", DEFAULT_PATH_RECALC_MS);
        let target_move_threshold =
            param(params, "targetMoveThreshold", DEFAULT_TARGET_MOVE_THRESHOLD);

        self.state = AiState::Chase;
        self.move_speed = self.run_speed;

        let needs_repath = self.waypoints.is_empty()
            || self.current_waypoint_idx >= self.waypoints.len()
            || self.path_elapsed_ms > path_recalc_ms
            || self.target_moved_significantly_by(&target_pos, target_move_threshold);

        if needs_repath {
            self.compute_path(target_pos.x, target_pos.z, path_provider);
            self.last_known_target_pos = Some(target_pos.clone());
            if self.waypoints.is_empty() {
                return BehaviorStatus::Failure;
            }
        }

        self.follow_path(delta_ms);
        commands.push(AiCommand::Move {
            monster_id: self.monster_id.clone(),
            position: self.position.clone(),
            rotation: self.rotation,
            state: MonsterState::Run,
            target_position: target_pos,
        });
        BehaviorStatus::Running
    }

    fn select_target_in_range(&mut self, nearby_players: &[NearbyPlayer], range: f32) -> bool {
        let range_sq = range * range;

        if let Some(target_id) = &self.target_player_id {
            return nearby_players.iter().any(|p| {
                p.id == *target_id
                    && p.health > 0
                    && self.position.dist_xz_sq(&p.position) <= range_sq
            });
        }

        let selected = nearby_players
            .iter()
            .filter_map(|p| {
                let dist_sq = self.position.dist_xz_sq(&p.position);
                (p.health > 0 && dist_sq <= range_sq).then_some((dist_sq, p))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, p)| p.id.clone());

        if let Some(id) = selected {
            self.target_player_id = Some(id);
            true
        } else {
            false
        }
    }

    fn current_target<'a>(&self, nearby_players: &'a [NearbyPlayer]) -> Option<&'a NearbyPlayer> {
        let target_id = self.target_player_id.as_ref()?;
        nearby_players
            .iter()
            .find(|p| p.id == *target_id && p.health > 0)
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

    fn transition_to_flee(
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
    fn start_flee_path(&mut self, safe_dist: f32, path_provider: &dyn PathProvider) {
        if let Some(threat) = self.last_known_target_pos {
            let dx = self.position.x - threat.x;
            let dz = self.position.z - threat.z;
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

    fn target_moved_significantly_by(&self, target_pos: &Position, threshold: f32) -> bool {
        match &self.last_known_target_pos {
            None => true,
            Some(last) => {
                let dx = target_pos.x - last.x;
                let dz = target_pos.z - last.z;
                (dx * dx + dz * dz) > threshold * threshold
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

fn param(params: &HashMap<String, f32>, name: &str, default: f32) -> f32 {
    params.get(name).copied().unwrap_or(default)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::SmallRng;
    use rand::SeedableRng;

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

    fn make_brain() -> MonsterBrain {
        MonsterBrain::new(
            "test_m1".into(),
            "scp939".into(),
            "default".into(),
            Position {
                x: 10.0,
                y: 0.0,
                z: 10.0,
            },
            10,
            10,
            1.0,
            8.0,
            DEFAULT_ATTACK_RANGE,
            DEFAULT_CHASE_RANGE,
            1500.0,
        )
    }

    #[test]
    fn brain_starts_idle() {
        let brain = make_brain();
        assert_eq!(brain.state(), AiState::Idle);
        assert_eq!(brain.network_state(), MonsterState::Idle);
    }

    #[test]
    fn idle_does_not_transition_before_check_interval() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Action {
                        name: "wander".into(),
                        params: HashMap::from([("checkMs".into(), 1000.0)]),
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick_with_behavior_tree(500.0, &[], &tree, &DirectPath, &mut rng);
        assert!(result.commands.is_empty());
        assert_eq!(brain.state(), AiState::Idle);
    }

    #[test]
    fn idle_can_transition_to_move() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Action {
                name: "wander".into(),
                params: HashMap::from([("checkMs".into(), 1000.0)]),
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick_with_behavior_tree(1001.0, &[], &tree, &DirectPath, &mut rng);
        assert!(!result.commands.is_empty());
        assert!(brain.state() == AiState::Walk || brain.state() == AiState::Run);
    }

    #[test]
    fn handle_hit_transitions_to_hit_state() {
        let mut brain = make_brain();

        let cmds = brain.handle_hit_with_behavior_tree("player1", true, 3);
        assert!(!cmds.is_empty());
        assert_eq!(brain.state(), AiState::Hit);
        assert_eq!(brain.health, 7);
    }

    #[test]
    fn handle_hit_death() {
        let mut brain = make_brain();

        let cmds = brain.handle_hit_with_behavior_tree("player1", true, 100);
        assert!(cmds.is_empty()); // dead returns empty
        assert!(brain.is_dead());
        assert_eq!(brain.health, 0);
    }

    #[test]
    fn load_behavior_trees_parses_json() {
        let trees = load_behavior_trees(include_str!("../../data-src/behavior_trees.json"))
            .expect("behavior_trees.json should parse");

        assert!(trees.contains_key("timid"));
        assert!(trees.contains_key("brave"));
        assert!(behavior_tree_for(&trees, "missing").is_some());
    }

    #[test]
    fn behavior_tree_attacks_target_in_range() {
        let mut brain = make_brain();
        brain.attack_cooldown_ms = 1000.0;
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 2.0)]),
                            },
                            BehaviorNode::Action {
                                name: "attack_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 11.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let result = brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);

        assert!(result
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));
        assert_eq!(brain.state(), AiState::Attack);
    }

    #[test]
    fn chase_to_attack_fires_without_waiting_full_cooldown() {
        let mut brain = make_brain();
        brain.state = AiState::Chase;
        brain.target_player_id = Some("p1".into());
        brain.attack_cooldown_ms = 4100.0;

        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "has_target".into(),
                                params: HashMap::new(),
                            },
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 2.0)]),
                            },
                            BehaviorNode::Action {
                                name: "attack_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 11.9,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let result = brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);

        assert!(result
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));
        assert_eq!(brain.state(), AiState::Attack);
    }

    #[test]
    fn behavior_tree_chases_target_in_range() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 25.0)]),
                            },
                            BehaviorNode::Action {
                                name: "chase_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 15.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let result = brain.tick_with_behavior_tree(50.0, &players, &tree, &DirectPath, &mut rng);

        assert!(result
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Move { .. })));
        assert!(result.commands.iter().any(|c| {
            matches!(
                c,
                AiCommand::Move {
                    state: MonsterState::Run,
                    ..
                }
            )
        }));
        assert_eq!(brain.state(), AiState::Chase);
    }

    fn attacker_at(x: f32, z: f32) -> Vec<NearbyPlayer> {
        vec![NearbyPlayer {
            id: "p1".into(),
            position: Position { x, y: 0.0, z },
            health: 10,
        }]
    }

    fn flee_tree() -> BehaviorTree {
        BehaviorTree {
            description: None,
            root: BehaviorNode::Sequence {
                children: vec![
                    BehaviorNode::Condition {
                        name: "health_below_ratio".into(),
                        params: HashMap::from([("ratio".into(), 0.3)]),
                    },
                    BehaviorNode::Action {
                        name: "flee_from_target".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        }
    }

    #[test]
    fn behavior_tree_flee_without_threat_position_runs_to_spawn() {
        let mut brain = make_brain();
        brain.position.x = 20.0;
        brain.target_player_id = Some("p1".into());
        brain.health = 2;
        let tree = flee_tree();
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick_with_behavior_tree(16.0, &[], &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Flee);
        assert!(result.commands.iter().any(|c| {
            matches!(
                c,
                AiCommand::Move {
                    state: MonsterState::Run,
                    target_position: Position {
                        x: 10.0,
                        z: 10.0,
                        ..
                    },
                    ..
                }
            )
        }));
    }

    #[test]
    fn behavior_tree_flee_runs_away_from_attacker_beyond_sight() {
        let mut brain = make_brain();
        brain.target_player_id = Some("p1".into());
        brain.health = 2;
        let tree = flee_tree();
        let mut rng = SmallRng::seed_from_u64(42);

        // Attacker just west of the monster — flee leg must point east, one
        // full safe distance (chase 25 + margin 5) away.
        let players = attacker_at(8.0, 10.0);

        let result = brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Flee);
        assert!(result.commands.iter().any(|c| {
            matches!(
                c,
                AiCommand::Move {
                    state: MonsterState::Run,
                    target_position: Position { x, z, .. },
                    ..
                } if (x - 40.0).abs() < 0.01 && (z - 10.0).abs() < 0.01
            )
        }));
    }

    #[test]
    fn behavior_tree_flee_stops_once_beyond_safe_distance() {
        let mut brain = make_brain();
        brain.target_player_id = Some("p1".into());
        brain.health = 2;
        let tree = flee_tree();
        let mut rng = SmallRng::seed_from_u64(42);

        let players = attacker_at(8.0, 10.0);

        brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);
        assert_eq!(brain.state(), AiState::Flee);

        // Large delta covers the whole flee leg: monster ends at x=40,
        // 32m from the attacker — beyond the 30m safe distance.
        brain.tick_with_behavior_tree(5000.0, &players, &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Idle);
        assert!(brain.target_player_id.is_none());
        assert!((brain.position.x - 40.0).abs() < 0.01);
    }

    #[test]
    fn behavior_tree_flee_repaths_when_attacker_keeps_chasing() {
        let mut brain = make_brain();
        brain.target_player_id = Some("p1".into());
        brain.health = 2;
        let tree = flee_tree();
        let mut rng = SmallRng::seed_from_u64(42);

        let players = attacker_at(8.0, 10.0);
        brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);
        assert_eq!(brain.state(), AiState::Flee);

        // Attacker chased to x=35 — when the first leg ends at x=40 the
        // monster is still within sight, so it must start another leg east.
        let chasing = attacker_at(35.0, 10.0);
        let result = brain.tick_with_behavior_tree(5000.0, &chasing, &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Flee);
        assert!(result.commands.iter().any(|c| {
            matches!(
                c,
                AiCommand::Move {
                    target_position: Position { x, .. },
                    ..
                } if (x - 70.0).abs() < 0.01
            )
        }));
    }

    #[test]
    fn behavior_tree_does_not_flee_without_target() {
        let mut brain = make_brain();
        brain.position.x = 20.0;
        brain.health = 2;
        let tree = flee_tree();
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick_with_behavior_tree(16.0, &[], &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Idle);
        assert!(result.commands.is_empty());
    }

    #[test]
    fn behavior_tree_return_sends_walk_target_to_spawn() {
        let mut brain = make_brain();
        brain.position.x = 70.0;
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Sequence {
                children: vec![
                    BehaviorNode::Condition {
                        name: "is_beyond_leash".into(),
                        params: HashMap::from([("range".into(), 30.0)]),
                    },
                    BehaviorNode::Action {
                        name: "return_to_spawn".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        let result = brain.tick_with_behavior_tree(16.0, &[], &tree, &DirectPath, &mut rng);

        assert_eq!(brain.state(), AiState::Return);
        assert!(result.commands.iter().any(|c| {
            matches!(
                c,
                AiCommand::Move {
                    state: MonsterState::Walk,
                    target_position: Position {
                        x: 10.0,
                        z: 10.0,
                        ..
                    },
                    ..
                }
            )
        }));
    }

    #[test]
    fn behavior_tree_requires_existing_target_before_attacking() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "has_target".into(),
                                params: HashMap::new(),
                            },
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 2.0)]),
                            },
                            BehaviorNode::Action {
                                name: "attack_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);
        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 11.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let peaceful = brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);
        assert!(!peaceful
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));
        assert_eq!(brain.state(), AiState::Idle);

        brain.handle_hit_with_behavior_tree("p1", false, 0);
        let provoked = brain.tick_with_behavior_tree(16.0, &players, &tree, &DirectPath, &mut rng);
        assert!(provoked
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));
    }

    #[test]
    fn attack_chases_nearby_player() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "has_target".into(),
                                params: HashMap::new(),
                            },
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 25.0)]),
                            },
                            BehaviorNode::Action {
                                name: "chase_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        brain.state = AiState::Attack;
        brain.target_player_id = Some("p1".into());
        brain.move_speed = brain.run_speed;

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 15.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let result = brain.tick_with_behavior_tree(50.0, &players, &tree, &DirectPath, &mut rng);
        assert!(result
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Move { .. })));
    }

    #[test]
    fn attack_command_uses_monster_cooldown() {
        let mut brain = make_brain();
        let tree = BehaviorTree {
            description: None,
            root: BehaviorNode::Selector {
                children: vec![
                    BehaviorNode::Sequence {
                        children: vec![
                            BehaviorNode::Condition {
                                name: "has_target".into(),
                                params: HashMap::new(),
                            },
                            BehaviorNode::Condition {
                                name: "target_in_range".into(),
                                params: HashMap::from([("range".into(), 2.0)]),
                            },
                            BehaviorNode::Action {
                                name: "attack_target".into(),
                                params: HashMap::new(),
                            },
                        ],
                    },
                    BehaviorNode::Action {
                        name: "idle".into(),
                        params: HashMap::new(),
                    },
                ],
            },
        };
        let mut rng = SmallRng::seed_from_u64(42);

        brain.state = AiState::Attack;
        brain.target_player_id = Some("p1".into());
        brain.attack_cooldown_ms = 1800.0;

        let players = vec![NearbyPlayer {
            id: "p1".into(),
            position: Position {
                x: 11.0,
                y: 0.0,
                z: 10.0,
            },
            health: 10,
        }];

        let before_cooldown =
            brain.tick_with_behavior_tree(1700.0, &players, &tree, &DirectPath, &mut rng);
        assert!(!before_cooldown
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));

        let after_cooldown =
            brain.tick_with_behavior_tree(100.0, &players, &tree, &DirectPath, &mut rng);
        assert!(after_cooldown
            .commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. })));
    }
}
