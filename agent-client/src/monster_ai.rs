//! Server-assigned monster AI: deterministic state-machine behavior.
//!
//! Ported from client/src/lib/managers/monsterManager.ts `updateMonsterAI`.
//! Each monster gets a `MonsterBrain` that ticks at ~20Hz and sends
//! `MonsterMove` / `MonsterAttack` messages via the shared command channel.

use onlinerpg_shared::pathfinding::{self, PassabilityCache, PathWaypoint};
use onlinerpg_shared::{ClientMessage, Monster, MonsterState, Player, Position};
use rand::Rng;
use std::collections::HashMap;
use std::time::Instant;
use tracing::info;

// Constants matching client/src/lib/managers/monsterManager.ts
const MIN_MOVE_DIST: f32 = 2.0;
const MAX_MOVE_DIST: f32 = 10.0;
/// How often to check idle -> patrol transition (ms)
const IDLE_CHECK_MS: f32 = 1000.0;
/// Probability of transitioning from idle to move
const IDLE_MOVE_CHANCE: f32 = 0.3;
/// Duration of hit stagger animation (ms)
const HIT_STAGGER_MS: f32 = 800.0;
/// How often to recompute chase path (ms)
const PATH_RECALC_MS: f32 = 3000.0;
/// Target movement threshold for repath (units)
const TARGET_MOVE_THRESHOLD: f32 = 3.0;
/// Flee when health ratio drops below this
const FLEE_HEALTH_RATIO: f32 = 0.3;
/// Probability of fleeing when health is below threshold (per hit check)
const DEFAULT_FLEE_CHANCE: f32 = 0.5;
/// Probability of returning to spawn when disengaging
const DEFAULT_RETURN_CHANCE: f32 = 0.7;
/// How long to flee before transitioning to return (ms)
const FLEE_DURATION_MS: f32 = 3000.0;
/// How close to spawn point before considered "returned"
const RETURN_ARRIVE_DIST: f32 = 5.0;
/// Max distance from spawn before leash kicks in
const DEFAULT_LEASH_RANGE: f32 = 50.0;
/// Default values when monster_defs not available
const DEFAULT_WALK_SPEED: f32 = 1.0;
const DEFAULT_RUN_SPEED: f32 = 8.0;
const DEFAULT_ATTACK_RANGE: f32 = 2.0;
const DEFAULT_CHASE_RANGE: f32 = 25.0;
const DEFAULT_ATTACK_COOLDOWN_MS: f32 = 1500.0;

/// Runtime definition for a monster type (loaded from monsters.json).
#[derive(Debug, Clone)]
pub struct MonsterDef {
    pub walk_speed: f32,
    pub run_speed: f32,
    pub attack_range: f32,
    pub chase_range: f32,
    pub attack_cooldown_ms: f32,
    pub flee_health_ratio: f32,
    pub flee_chance: f32,
    pub return_chance: f32,
}

impl Default for MonsterDef {
    fn default() -> Self {
        Self {
            walk_speed: DEFAULT_WALK_SPEED,
            run_speed: DEFAULT_RUN_SPEED,
            attack_range: DEFAULT_ATTACK_RANGE,
            chase_range: DEFAULT_CHASE_RANGE,
            attack_cooldown_ms: DEFAULT_ATTACK_COOLDOWN_MS,
            flee_health_ratio: FLEE_HEALTH_RATIO,
            flee_chance: DEFAULT_FLEE_CHANCE,
            return_chance: DEFAULT_RETURN_CHANCE,
        }
    }
}

/// Per-monster AI brain.
pub struct MonsterBrain {
    pub monster_id: String,
    pub position: Position,
    pub rotation: f32,
    pub health: u32,
    state: AiState,
    state_timer_ms: f32,
    target_player_id: Option<String>,
    move_speed: f32,
    target_position: Option<Position>,
    waypoints: Vec<PathWaypoint>,
    current_waypoint_idx: usize,
    last_path_time: Instant,
    last_known_target_pos: Option<Position>,
    spawn_position: Position,
    flee_health_threshold: u32,
    def: MonsterDef,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AiState {
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
    fn to_monster_state(self) -> MonsterState {
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

impl MonsterBrain {
    pub fn new(monster: &Monster, defs: &HashMap<String, MonsterDef>) -> Self {
        let def = defs.get(&monster.monster_type).cloned().unwrap_or_default();
        Self {
            monster_id: monster.id.clone(),
            position: monster.position.clone(),
            rotation: monster.rotation,
            health: monster.health,
            state: AiState::Idle,
            state_timer_ms: 0.0,
            target_player_id: None,
            move_speed: def.walk_speed,
            target_position: None,
            waypoints: Vec::new(),
            current_waypoint_idx: 0,
            last_path_time: Instant::now(),
            last_known_target_pos: None,
            spawn_position: monster.position.clone(),
            flee_health_threshold: (monster.max_health as f32 * def.flee_health_ratio) as u32,
            def,
        }
    }

    /// Main tick. Returns a list of commands to send to the server.
    pub fn tick(
        &mut self,
        delta_ms: f32,
        nearby_players: &HashMap<String, Player>,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        if self.state == AiState::Dead || self.health == 0 {
            return vec![];
        }

        self.state_timer_ms += delta_ms;
        let mut commands = Vec::new();

        match self.state {
            AiState::Idle => {
                self.tick_idle(&mut commands, passability_cache);
            }
            AiState::Walk | AiState::Run => {
                self.tick_patrol(delta_ms, &mut commands, passability_cache);
            }
            AiState::Hit => {
                self.tick_hit(&mut commands, passability_cache);
            }
            AiState::Attack => {
                self.tick_attack(delta_ms, nearby_players, &mut commands, passability_cache);
            }
            AiState::Flee => {
                self.tick_flee(delta_ms, &mut commands, passability_cache);
            }
            AiState::Return => {
                self.tick_return(delta_ms, &mut commands, passability_cache);
            }
            AiState::Dead => {}
        }

        commands
    }

    fn tick_idle(
        &mut self,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        if self.state_timer_ms < IDLE_CHECK_MS {
            return;
        }
        self.state_timer_ms = 0.0;

        let mut rng = rand::thread_rng();
        if rng.gen::<f32>() < IDLE_MOVE_CHANCE {
            self.transition_to_move(commands, passability_cache);
        }
    }

    fn tick_patrol(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        if self.target_position.is_none() {
            self.transition_to_idle(commands);
            return;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            let mut rng = rand::thread_rng();
            if rng.gen::<f32>() < 0.5 {
                self.transition_to_idle(commands);
            } else {
                self.transition_to_move(commands, passability_cache);
            }
        }
    }

    fn tick_hit(
        &mut self,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        if self.state_timer_ms >= HIT_STAGGER_MS {
            let flee_threshold = self.flee_health_threshold;
            let mut rng = rand::thread_rng();
            if self.health <= flee_threshold && rng.gen::<f32>() < self.def.flee_chance {
                self.transition_to_flee(commands, passability_cache);
            } else {
                self.state = AiState::Attack;
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_msg());
            }
        }
    }

    fn tick_attack(
        &mut self,
        delta_ms: f32,
        nearby_players: &HashMap<String, Player>,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        let target_id = match &self.target_player_id {
            Some(id) => id.clone(),
            None => {
                self.transition_to_idle(commands);
                return;
            }
        };

        // Find target player
        let target = match nearby_players.get(&target_id) {
            Some(p) if p.health > 0 => p,
            _ => {
                // Target dead or gone
                self.target_player_id = None;
                self.transition_to_idle(commands);
                return;
            }
        };

        let dx = target.position.x - self.position.x;
        let dz = target.position.z - self.position.z;
        let dist_sq = dx * dx + dz * dz;
        let chase_range_sq = self.def.chase_range * self.def.chase_range;

        // Leash: return to spawn if too far from home
        let spawn_dx = self.position.x - self.spawn_position.x;
        let spawn_dz = self.position.z - self.spawn_position.z;
        let dist_from_spawn_sq = spawn_dx * spawn_dx + spawn_dz * spawn_dz;
        if dist_from_spawn_sq > DEFAULT_LEASH_RANGE * DEFAULT_LEASH_RANGE {
            self.target_player_id = None;
            self.transition_to_return(commands, passability_cache);
            return;
        }

        // Give up if target too far
        if dist_sq > chase_range_sq {
            self.target_player_id = None;
            self.transition_to_return(commands, passability_cache);
            return;
        }

        let attack_range_sq = self.def.attack_range * self.def.attack_range;

        if dist_sq <= attack_range_sq {
            // In attack range: face target and attack on cooldown
            self.rotation = dz.atan2(dx);

            if self.state_timer_ms >= self.def.attack_cooldown_ms {
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_msg());
                commands.push(ClientMessage::MonsterAttack {
                    monster_id: self.monster_id.clone(),
                    target_player_id: target_id,
                });
            }
        } else {
            // Chase: pathfind to target
            self.move_speed = self.def.run_speed;
            let target_pos = &target.position;

            let needs_repath = self.waypoints.is_empty()
                || self.current_waypoint_idx >= self.waypoints.len()
                || self.last_path_time.elapsed().as_millis() > PATH_RECALC_MS as u128
                || self.target_moved_significantly(target_pos);

            if needs_repath {
                self.compute_path(target_pos.x, target_pos.z, passability_cache);
                self.last_known_target_pos = Some(target_pos.clone());
            }

            let reached = self.follow_path(delta_ms);
            if reached && dist_sq > attack_range_sq {
                // Stuck and still not in range — give up
                self.target_player_id = None;
                self.transition_to_idle(commands);
                return;
            }

            commands.push(ClientMessage::MonsterMove {
                monster_id: self.monster_id.clone(),
                position: self.position.clone(),
                rotation: self.rotation,
                state: MonsterState::Attack,
                target_position: target_pos.clone(),
            });
        }
    }

    /// Called when this monster is hit by a player attack.
    pub fn handle_hit(
        &mut self,
        attacker_id: &str,
        hit: bool,
        damage: u32,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        if self.state == AiState::Dead {
            return vec![];
        }

        self.health = self.health.saturating_sub(if hit { damage } else { 0 });
        self.target_player_id = Some(attacker_id.to_string());
        self.move_speed = self.def.run_speed;

        if self.health == 0 {
            self.state = AiState::Dead;
            return vec![];
        }

        // Flee if health is low (probabilistic)
        let flee_threshold = self.flee_health_threshold;
        let mut rng = rand::thread_rng();
        if self.health <= flee_threshold && rng.gen::<f32>() < self.def.flee_chance {
            let mut commands = Vec::new();
            if hit {
                // Stagger first, then flee after stagger
                self.state = AiState::Hit;
                self.state_timer_ms = 0.0;
                commands.push(self.make_move_msg());
            } else {
                self.transition_to_flee(&mut commands, passability_cache);
            }
            return commands;
        }

        let mut commands = Vec::new();
        if hit {
            self.state = AiState::Hit;
            self.state_timer_ms = 0.0;
            commands.push(self.make_move_msg());
        } else {
            // Miss: go straight to attack (no stagger)
            self.state = AiState::Attack;
            self.state_timer_ms = 0.0;
            commands.push(self.make_move_msg());
        }
        commands
    }

    /// Called when the monster dies (server confirms death).
    pub fn handle_death(&mut self) {
        self.state = AiState::Dead;
        self.health = 0;
    }

    // --- Private helpers ---

    fn tick_flee(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        if self.state_timer_ms >= FLEE_DURATION_MS {
            self.target_player_id = None;
            self.transition_to_return(commands, passability_cache);
            return;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            // Flee path exhausted, start returning
            self.target_player_id = None;
            self.transition_to_return(commands, passability_cache);
            return;
        }

        commands.push(self.make_move_msg());
    }

    fn tick_return(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        let dx = self.spawn_position.x - self.position.x;
        let dz = self.spawn_position.z - self.position.z;
        let dist_sq = dx * dx + dz * dz;

        if dist_sq <= RETURN_ARRIVE_DIST * RETURN_ARRIVE_DIST {
            self.transition_to_idle(commands);
            return;
        }

        // Repath if needed
        if self.waypoints.is_empty() || self.current_waypoint_idx >= self.waypoints.len() {
            self.compute_path(
                self.spawn_position.x,
                self.spawn_position.z,
                passability_cache,
            );
            if self.waypoints.is_empty() {
                // Can't path home, just idle here
                self.transition_to_idle(commands);
                return;
            }
        }

        self.follow_path(delta_ms);
        commands.push(self.make_move_msg());
    }

    fn transition_to_flee(
        &mut self,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        self.state = AiState::Flee;
        self.state_timer_ms = 0.0;
        self.move_speed = self.def.run_speed;

        self.compute_path(
            self.spawn_position.x,
            self.spawn_position.z,
            passability_cache,
        );

        if self.waypoints.is_empty() {
            // Can't path, just idle
            self.state = AiState::Idle;
            self.state_timer_ms = 0.0;
            return;
        }

        if let Some(wp) = self.waypoints.first() {
            let dx = wp.x - self.position.x;
            let dz = wp.z - self.position.z;
            self.rotation = dz.atan2(dx);
        }

        commands.push(self.make_move_msg());
    }

    fn transition_to_return(
        &mut self,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        let mut rng = rand::thread_rng();
        if rng.gen::<f32>() >= self.def.return_chance {
            self.transition_to_idle(commands);
            return;
        }

        self.state = AiState::Return;
        self.state_timer_ms = 0.0;
        self.move_speed = self.def.walk_speed;
        self.target_position = Some(self.spawn_position.clone());

        self.compute_path(
            self.spawn_position.x,
            self.spawn_position.z,
            passability_cache,
        );

        if self.waypoints.is_empty() {
            // Can't path home, just idle
            self.transition_to_idle(commands);
            return;
        }

        if let Some(wp) = self.waypoints.first() {
            let dx = wp.x - self.position.x;
            let dz = wp.z - self.position.z;
            self.rotation = dz.atan2(dx);
        }

        commands.push(self.make_move_msg());
    }

    fn transition_to_idle(&mut self, commands: &mut Vec<ClientMessage>) {
        self.state = AiState::Idle;
        self.state_timer_ms = 0.0;
        self.target_position = None;
        self.waypoints.clear();
        self.current_waypoint_idx = 0;
        commands.push(self.make_move_msg());
    }

    fn transition_to_move(
        &mut self,
        commands: &mut Vec<ClientMessage>,
        passability_cache: &PassabilityCache,
    ) {
        let mut rng = rand::thread_rng();
        let angle: f32 = rng.gen_range(0.0..std::f32::consts::TAU);
        let dist: f32 = rng.gen_range(MIN_MOVE_DIST..MAX_MOVE_DIST);

        let target_x = self.position.x + angle.cos() * dist;
        let target_z = self.position.z + angle.sin() * dist;

        // Walk vs run probability based on distance
        let walk_prob = (-0.075 * dist + 0.95).clamp(0.0, 1.0);
        let is_walk = rng.gen::<f32>() < walk_prob;

        if is_walk {
            self.state = AiState::Walk;
            self.move_speed = self.def.walk_speed;
        } else {
            self.state = AiState::Run;
            self.move_speed = self.def.run_speed;
        }

        self.state_timer_ms = 0.0;
        self.target_position = Some(Position {
            x: target_x,
            y: self.position.y,
            z: target_z,
        });

        self.compute_path(target_x, target_z, passability_cache);

        if self.waypoints.is_empty() {
            // Pathfinding failed, stay idle
            self.state = AiState::Idle;
            self.target_position = None;
            return;
        }

        // Look at first waypoint
        if let Some(wp) = self.waypoints.first() {
            let dx = wp.x - self.position.x;
            let dz = wp.z - self.position.z;
            self.rotation = dz.atan2(dx);
        }

        commands.push(ClientMessage::MonsterMove {
            monster_id: self.monster_id.clone(),
            position: self.position.clone(),
            rotation: self.rotation,
            state: self.state.to_monster_state(),
            target_position: self
                .target_position
                .clone()
                .unwrap_or(self.position.clone()),
        });
    }

    fn compute_path(&mut self, goal_x: f32, goal_z: f32, passability_cache: &PassabilityCache) {
        let result = pathfinding::find_and_smooth_path(
            self.position.x,
            self.position.z,
            0, // floor level
            goal_x,
            goal_z,
            0,
            passability_cache,
            pathfinding::DEFAULT_MAX_NODES,
        );
        self.waypoints = result.waypoints;
        self.current_waypoint_idx = 0;
        self.last_path_time = Instant::now();
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
            // Reached waypoint
            self.position.x = wp.x;
            self.position.z = wp.z;
            self.current_waypoint_idx += 1;

            if self.current_waypoint_idx >= self.waypoints.len() {
                return true;
            }

            // Look at next waypoint
            let next = &self.waypoints[self.current_waypoint_idx];
            let ndx = next.x - self.position.x;
            let ndz = next.z - self.position.z;
            self.rotation = ndz.atan2(ndx);
        } else {
            // Move toward waypoint
            let nx = dx / dist;
            let nz = dz / dist;
            self.position.x += nx * step;
            self.position.z += nz * step;
            self.rotation = dz.atan2(dx);
        }

        false
    }

    fn target_moved_significantly(&self, target_pos: &Position) -> bool {
        match &self.last_known_target_pos {
            None => true,
            Some(last) => {
                let dx = target_pos.x - last.x;
                let dz = target_pos.z - last.z;
                (dx * dx + dz * dz) > TARGET_MOVE_THRESHOLD * TARGET_MOVE_THRESHOLD
            }
        }
    }

    fn make_move_msg(&self) -> ClientMessage {
        ClientMessage::MonsterMove {
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

/// Manages all monster brains assigned to this client.
pub struct MonsterAiManager {
    brains: HashMap<String, MonsterBrain>,
    defs: HashMap<String, MonsterDef>,
}

impl MonsterAiManager {
    pub fn new() -> Self {
        Self {
            brains: HashMap::new(),
            defs: HashMap::new(),
        }
    }

    /// Load monster definitions from JSON (same format as server's monsters.json).
    pub fn load_defs_from_json(json_str: &str) -> HashMap<String, MonsterDef> {
        #[derive(serde::Deserialize)]
        struct RawDef {
            #[serde(rename = "walkSpeed", default = "default_walk")]
            walk_speed: f32,
            #[serde(rename = "runSpeed", default = "default_run")]
            run_speed: f32,
            #[serde(rename = "attackRange", default = "default_attack_range")]
            attack_range: f32,
            #[serde(rename = "chaseRange", default = "default_chase_range")]
            chase_range: f32,
            #[serde(rename = "attackCooldown", default = "default_cooldown")]
            attack_cooldown: u32,
            #[serde(rename = "fleeHealthRatio", default = "default_flee_ratio")]
            flee_health_ratio: f32,
            #[serde(rename = "fleeChance", default = "default_flee_chance")]
            flee_chance: f32,
            #[serde(rename = "returnChance", default = "default_return_chance")]
            return_chance: f32,
        }

        fn default_walk() -> f32 {
            DEFAULT_WALK_SPEED
        }
        fn default_run() -> f32 {
            DEFAULT_RUN_SPEED
        }
        fn default_attack_range() -> f32 {
            DEFAULT_ATTACK_RANGE
        }
        fn default_chase_range() -> f32 {
            DEFAULT_CHASE_RANGE
        }
        fn default_cooldown() -> u32 {
            DEFAULT_ATTACK_COOLDOWN_MS as u32
        }
        fn default_flee_ratio() -> f32 {
            FLEE_HEALTH_RATIO
        }
        fn default_flee_chance() -> f32 {
            DEFAULT_FLEE_CHANCE
        }
        fn default_return_chance() -> f32 {
            DEFAULT_RETURN_CHANCE
        }

        let raw: HashMap<String, RawDef> = serde_json::from_str(json_str).unwrap_or_default();
        raw.into_iter()
            .map(|(id, r)| {
                (
                    id,
                    MonsterDef {
                        walk_speed: r.walk_speed,
                        run_speed: r.run_speed,
                        attack_range: r.attack_range,
                        chase_range: r.chase_range,
                        attack_cooldown_ms: r.attack_cooldown as f32,
                        flee_health_ratio: r.flee_health_ratio,
                        flee_chance: r.flee_chance,
                        return_chance: r.return_chance,
                    },
                )
            })
            .collect()
    }

    pub fn set_defs(&mut self, defs: HashMap<String, MonsterDef>) {
        self.defs = defs;
    }

    /// Register a newly assigned monster.
    pub fn add_monster(&mut self, monster: &Monster) {
        info!(
            "Monster AI: managing {} (type={})",
            monster.id, monster.monster_type
        );
        let brain = MonsterBrain::new(monster, &self.defs);
        self.brains.insert(monster.id.clone(), brain);
    }

    /// Remove a monster (died or removed).
    pub fn remove_monster(&mut self, monster_id: &str) {
        if self.brains.remove(monster_id).is_some() {
            info!("Monster AI: stopped managing {}", monster_id);
        }
    }

    /// Notify that a monster was hit by a player.
    pub fn handle_monster_hit(
        &mut self,
        monster_id: &str,
        attacker_id: &str,
        hit: bool,
        damage: u32,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        if let Some(brain) = self.brains.get_mut(monster_id) {
            brain.handle_hit(attacker_id, hit, damage, passability_cache)
        } else {
            vec![]
        }
    }

    /// Notify that a monster died.
    pub fn handle_monster_dead(&mut self, monster_id: &str) {
        if let Some(brain) = self.brains.get_mut(monster_id) {
            brain.handle_death();
        }
        // Don't remove yet — will be removed on MonsterRemoved
    }

    /// Tick all managed monster brains. Returns commands to send.
    pub fn tick_all(
        &mut self,
        delta_ms: f32,
        nearby_players: &HashMap<String, Player>,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        let mut all_commands = Vec::new();
        for brain in self.brains.values_mut() {
            let cmds = brain.tick(delta_ms, nearby_players, passability_cache);
            all_commands.extend(cmds);
        }
        all_commands
    }

    /// Check if we manage a given monster.
    pub fn manages(&self, monster_id: &str) -> bool {
        self.brains.contains_key(monster_id)
    }
}
