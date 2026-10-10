//! Server-driven monster brains (doc/SERVER_SIDE_MONSTER_AI.md).

use crate::types::{Monster, MonsterState, PlayerId, Position, ServerMessage};
use onlinerpg_shared::dungeon::passability_floor_for_level;
use onlinerpg_shared::monster_ai::{
    self, AiCommand, AiState, BehaviorTree, ChaseAim, MonsterBrain, NearbyMonster, NearbyPlayer,
    AGGRESSIVE_BEHAVIOR, DEFAULT_BEHAVIOR,
};
use onlinerpg_shared::pathfinding::{is_movement_blocked, segment_touches_box, PathTermination};
use onlinerpg_shared::shortest_world_delta_x;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

mod path_budget;
use path_budget::{CountingPath, FailedPaths};

/// Budget checked before each brain.
const TICK_BUDGET: Duration = Duration::from_millis(40);
const BRAIN_PATH_NODE_BUDGET: usize = 20_000;
const TICK_PATH_NODE_BUDGET: usize = 20_000;
/// Cap catch-up movement after skipped ticks.
const MAX_BRAIN_DELTA_MS: f32 = 1000.0;
const STATS_LOG_PERIOD: Duration = Duration::from_secs(30);
/// Keep targets briefly when corners obscure sight.
const SIGHT_MEMORY: Duration = Duration::from_secs(5);
/// Sense players within this range even behind walls.
const SENSE_RANGE: f32 = 5.0;

struct Entry {
    brain: MonsterBrain,
    failed_paths: RefCell<FailedPaths>,
    last_tick: Instant,
    /// Tick generation this brain last had a player in view.
    watched_gen: u64,
    /// When the brain's current target was last in plain sight.
    target_seen: Option<(PlayerId, Instant)>,
    closed_doors: Vec<[f32; 4]>,
    door_hidden_target: Option<PlayerId>,
}

impl Entry {
    fn new(brain: MonsterBrain, now: Instant) -> Self {
        Self {
            brain,
            failed_paths: RefCell::default(),
            last_tick: now,
            watched_gen: 0,
            target_seen: None,
            closed_doors: Vec::new(),
            door_hidden_target: None,
        }
    }

    /// Simulated time owed to the brain up to `now`, and mark it paid.
    fn owed_ms(&mut self, now: Instant, forced: Option<f32>) -> f32 {
        let delta = forced.unwrap_or_else(|| (now - self.last_tick).as_secs_f32() * 1000.0);
        self.last_tick = now;
        delta.min(MAX_BRAIN_DELTA_MS)
    }

    /// Mark the brain watched this tick. A brain nobody watched last tick
    /// owes nothing: paying that time back is a teleport to whoever looks.
    fn watch(&mut self, gen: u64, now: Instant) {
        if self.watched_gen + 1 != gen {
            self.last_tick = now;
        }
        self.watched_gen = gen;
    }

    /// The target still in view on memory alone.
    fn remembered(&self, target: Option<PlayerId>, now: Instant) -> Option<PlayerId> {
        self.target_seen
            .filter(|(id, at)| Some(*id) == target && now - *at < SIGHT_MEMORY)
            .map(|(id, _)| id)
    }
}

#[derive(Default)]
struct Stats {
    ticks: u32,
    ticked: u64,
    pathfinds: u64,
    expanded_nodes: usize,
    cache_hits: u64,
    deferred_brains: u64,
    node_budget_ticks: u32,
    commands: u64,
    over_budget: u32,
    worst_ms: f32,
    slowest: Option<BrainSample>,
}

struct BrainSample {
    monster_id: String,
    position: Position,
    floor_level: i8,
    behavior: String,
    state: AiState,
    target: Option<PlayerId>,
    observed_at: i64,
    elapsed_ms: f32,
    paths: PathDiagnostics,
}

impl BrainSample {
    fn log(&self) {
        let path = self.paths.slowest;
        info!(
            monster_id = self.monster_id,
            x = self.position.x, y = self.position.y, z = self.position.z,
            floor_level = self.floor_level,
            behavior = self.behavior, state = ?self.state,
            target_player_id = self.target.map(PlayerId::get),
            observed_at = self.observed_at, brain_ms = self.elapsed_ms,
            pathfinds = self.paths.count(),
            reached = self.paths.reached, unreachable = self.paths.unreachable,
            node_limit = self.paths.node_limit, partial = self.paths.partial,
            expanded_nodes = self.paths.expanded_nodes,
            cached_paths = self.paths.cache_hits,
            deferred = self.paths.deferred,
            path_ms = path.map(|p| p.elapsed_ms),
            path_start_x = path.map(|p| p.start.0), path_start_z = path.map(|p| p.start.1),
            path_start_floor = path.map(|p| p.start.2),
            path_goal_x = path.map(|p| p.goal.0), path_goal_z = path.map(|p| p.goal.1),
            path_goal_floor = path.map(|p| p.goal.2),
            path_termination = path.map(|p| tracing::field::debug(p.termination)),
            path_waypoints = path.map(|p| p.waypoints),
            path_expanded_nodes = path.map(|p| p.expanded_nodes),
            "monster ai slowest brain"
        );
    }
}

#[derive(Clone, Copy)]
struct PathSample {
    start: (f32, f32, u8),
    goal: (f32, f32, u8),
    elapsed_ms: f32,
    termination: PathTermination,
    waypoints: usize,
    expanded_nodes: usize,
}

#[derive(Default, Clone, Copy)]
struct PathDiagnostics {
    cache_hits: u64,
    deferred: bool,
    reached: u64,
    unreachable: u64,
    node_limit: u64,
    partial: u64,
    expanded_nodes: usize,
    slowest: Option<PathSample>,
}

impl PathDiagnostics {
    fn count(&self) -> u64 {
        self.reached + self.unreachable + self.node_limit
    }

    fn record(&mut self, sample: PathSample) {
        match sample.termination {
            PathTermination::Reached => self.reached += 1,
            PathTermination::Unreachable => self.unreachable += 1,
            PathTermination::NodeLimit => self.node_limit += 1,
        }
        self.partial +=
            u64::from(sample.termination != PathTermination::Reached && sample.waypoints > 0);
        self.expanded_nodes += sample.expanded_nodes;
        if self
            .slowest
            .is_none_or(|p| sample.elapsed_ms > p.elapsed_ms)
        {
            self.slowest = Some(sample);
        }
    }
}

pub(crate) struct ServerBrains {
    entries: HashMap<String, Entry>,
    trees: HashMap<String, BehaviorTree>,
    /// Round-robin start so budget starvation rotates instead of always
    /// hitting the same tail.
    cursor: usize,
    /// Counts ticks, for `Entry::watch`.
    tick_gen: u64,
    stats: Stats,
    stats_since: Instant,
}

impl ServerBrains {
    pub(crate) fn new() -> Self {
        let trees =
            monster_ai::load_behavior_trees(include_str!("../../../data-src/behavior_trees.json"))
                .expect("behavior_trees.json is malformed");
        Self {
            entries: HashMap::new(),
            trees,
            cursor: 0,
            tick_gen: 0,
            stats: Stats::default(),
            stats_since: Instant::now(),
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

fn door_blocks_sense(from: &Position, to: &Position, doors: &[[f32; 4]]) -> bool {
    let tx = from.x + shortest_world_delta_x(from.x, to.x);
    doors.iter().any(|&[ax, az, bx, bz]| {
        let ax = from.x + shortest_world_delta_x(from.x, ax);
        let bx = ax + shortest_world_delta_x(ax, bx);
        segment_touches_box(
            (ax.min(bx), ax.max(bx)),
            (az.min(bz), az.max(bz)),
            (from.x, from.z),
            (tx, to.z),
        )
    })
}

type Roster = HashMap<super::SpatialCell, Vec<(PlayerId, Position, u32, i8)>>;

/// A live monster someone can see this tick.
struct Active {
    id: String,
    floor_level: i8,
    position: Position,
    players: Vec<NearbyPlayer>,
}

impl super::GameState {
    #[cfg(test)]
    pub(crate) async fn brain_target(&self, monster_id: &str) -> Option<PlayerId> {
        let brains = self.monster_brains.lock().await;
        brains
            .entries
            .get(monster_id)
            .and_then(|e| e.brain.target_player_id())
    }

    #[cfg(test)]
    pub(crate) async fn brain_pathfind_count(&self) -> u64 {
        self.monster_brains.lock().await.stats.pathfinds
    }

    fn new_brain(&self, monster: &Monster) -> MonsterBrain {
        let def = self.monster_defs.get(&monster.monster_type);
        let behavior = if monster.aggressive {
            AGGRESSIVE_BEHAVIOR.to_string()
        } else {
            def.and_then(|d| d.behavior.clone())
                .unwrap_or_else(|| DEFAULT_BEHAVIOR.to_string())
        };
        let mut brain = MonsterBrain::new(
            monster.id.clone(),
            &monster.monster_type,
            behavior,
            monster.position,
            monster.health,
            monster.max_health,
            def.map_or(monster_ai::DEFAULT_WALK_SPEED, |d| d.walk_speed),
            def.map_or(monster_ai::DEFAULT_RUN_SPEED, |d| d.run_speed),
            def.map_or(monster_ai::DEFAULT_ATTACK_RANGE, |d| d.attack_range),
            def.map_or(monster_ai::DEFAULT_CHASE_RANGE, |d| d.chase_range),
            def.map_or(monster_ai::DEFAULT_ATTACK_COOLDOWN_MS, |d| {
                d.attack_cooldown as f32
            }),
        );
        brain.path_floor = passability_floor_for_level(monster.floor_level);
        brain.rotation = monster.rotation;
        brain
    }

    /// One 200ms step of every brain someone can see, within `TICK_BUDGET`.
    pub async fn tick_monster_ai(&self) {
        self.tick_monster_ai_with(None).await;
    }

    /// Tests drive simulated time explicitly instead of waiting it out.
    #[cfg(test)]
    pub(crate) async fn tick_monster_ai_by(&self, delta_ms: f32) {
        self.tick_monster_ai_with(Some(delta_ms)).await;
    }

    async fn tick_monster_ai_with(&self, forced_delta_ms: Option<f32>) {
        let started = Instant::now();
        let (mut roster, underground) = {
            let now = Self::now_ms();
            let players = self.players.read().await;
            let mut roster = Roster::default();
            let mut underground = Vec::new();
            for (id, p) in players.iter() {
                if !p.is_ready(now) {
                    continue;
                }
                if p.floor_level < 0 {
                    underground.push((*id, p.position, p.floor_level));
                }
                roster
                    .entry(super::SpatialCell::from_position(&p.position))
                    .or_default()
                    .push((*id, p.position, p.health, p.floor_level));
            }
            (roster, underground)
        };
        // Just arrived, or behind a shut locked door: out of every monster's sight.
        let sealed = self.players_hidden_in_stair_rooms(&underground).await;
        let closed_doors = {
            let dungeons = self.dungeons.read().await;
            let mut doors = HashMap::new();
            for (_, pos, level) in &underground {
                let Some(entrance) = self.dungeon_defs.entrance_at(pos.x, pos.z) else {
                    continue;
                };
                let Some(rt) = dungeons.get(&entrance.id) else {
                    continue;
                };
                let depth = level.unsigned_abs();
                let Some(layout) = rt.layouts.get(depth as usize - 1) else {
                    continue;
                };
                doors
                    .entry((entrance.id.as_str(), *level))
                    .or_insert_with(|| {
                        let (ox, oz) =
                            onlinerpg_shared::dungeon::dungeon_origin(entrance.x, entrance.z);
                        onlinerpg_shared::dungeon::closed_door_segs(
                            layout,
                            rt.open_doors.get(&depth),
                        )
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|&[ax, az, bx, bz]| {
                            [
                                ox + ax as f32,
                                oz + az as f32,
                                ox + bx as f32,
                                oz + bz as f32,
                            ]
                        })
                        .collect::<Vec<_>>()
                    });
            }
            doors
        };
        for entries in roster.values_mut() {
            entries.retain(|(id, ..)| !sealed.contains(id));
        }
        let radius = super::EVENT_DELIVERY_RADIUS;
        let radius_sq = radius * radius;
        let players_near = |position: &Position, floor: i8| -> Vec<NearbyPlayer> {
            let mut seen = HashSet::new();
            super::SpatialCell::within_radius(position, radius)
                .filter_map(|cell| roster.get(&cell))
                .flatten()
                .filter(|(id, p, _, f)| {
                    *f == floor && position.dist_xz_sq(p) <= radius_sq && seen.insert(*id)
                })
                .map(|(id, p, health, _)| NearbyPlayer {
                    id: *id,
                    position: *p,
                    health: *health,
                })
                .collect()
        };

        let mut brains = self.monster_brains.lock().await;
        // Reconcile against the registry and snapshot what to tick, in one
        // pass under the read lock.
        let (mut active, standing) = {
            let monsters = self.monsters.read().await;
            brains.entries.retain(|id, _| {
                monsters
                    .get(id)
                    .is_some_and(|m| m.state != MonsterState::Dead)
            });
            let mut active = Vec::new();
            let mut standing: HashMap<super::SpatialCell, Vec<NearbyMonster>> = HashMap::new();
            for m in monsters.values() {
                if m.state == MonsterState::Dead || m.health == 0 {
                    continue;
                }
                let players = players_near(&m.position, m.floor_level);
                if players.is_empty() {
                    continue;
                }
                if m.state.is_stationary() {
                    standing
                        .entry(super::SpatialCell::from_position(&m.position))
                        .or_default()
                        .push(NearbyMonster {
                            id: m.id.clone(),
                            position: m.position,
                            state: m.state,
                            path_floor: passability_floor_for_level(m.floor_level),
                        });
                }
                if !brains.entries.contains_key(&m.id) {
                    brains
                        .entries
                        .insert(m.id.clone(), Entry::new(self.new_brain(m), started));
                }
                active.push(Active {
                    id: m.id.clone(),
                    floor_level: m.floor_level,
                    position: m.position,
                    players,
                });
            }
            (active, standing)
        };
        let mut commands: Vec<(String, i8, AiCommand)> = Vec::new();
        // Closed doors block sensing and sight memory too.
        {
            let cache = self.passability_read();
            for a in active.iter_mut().filter(|a| a.floor_level < 0) {
                let Some(entrance) = self.dungeon_defs.entrance_at(a.position.x, a.position.z)
                else {
                    continue;
                };
                let Some(entry) = brains.entries.get_mut(&a.id) else {
                    continue;
                };
                let key = onlinerpg_shared::dungeon::dungeon_cache_key(&entrance.id);
                let floor = passability_floor_for_level(a.floor_level);
                let from = entry.brain.position;
                let target = entry.brain.target_player_id().or(entry.door_hidden_target);
                let remembered = entry.remembered(target, started);
                let doors = closed_doors
                    .get(&(entrance.id.as_str(), a.floor_level))
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                if entry.closed_doors != doors {
                    entry.closed_doors = doors.to_vec();
                    entry.brain.retry_chase();
                }
                let mut target_hidden = false;
                a.players.retain(|p| {
                    if door_blocks_sense(&from, &p.position, doors) {
                        if Some(p.id) == target {
                            target_hidden = true;
                            entry.target_seen = None;
                        }
                        return false;
                    }
                    let visible = from.dist_xz_sq(&p.position) <= SENSE_RANGE * SENSE_RANGE
                        || !onlinerpg_shared::pathfinding::attack_line_blocked_in(
                            &cache,
                            &key,
                            from.x,
                            from.z,
                            p.position.x,
                            p.position.z,
                            floor,
                        );
                    if visible && Some(p.id) == target {
                        entry.target_seen = Some((p.id, started));
                    }
                    visible || Some(p.id) == remembered
                });
                if target_hidden {
                    commands.extend(
                        entry
                            .brain
                            .pause_chase()
                            .into_iter()
                            .map(|c| (a.id.clone(), a.floor_level, c)),
                    );
                    entry.door_hidden_target = target;
                } else if let Some(hidden) = entry.door_hidden_target.take() {
                    if entry.brain.target_player_id().is_none()
                        && a.players.iter().any(|p| p.id == hidden)
                    {
                        entry.brain.resume_chase(hidden);
                    }
                }
            }
            active.retain(|a| !a.players.is_empty());
        }
        active.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        brains.tick_gen += 1;
        let gen = brains.tick_gen;
        for a in &active {
            if let Some(entry) = brains.entries.get_mut(&a.id) {
                entry.watch(gen, started);
            }
        }
        if active.is_empty() && commands.is_empty() {
            return;
        }

        {
            let cache = self.passability_read();
            let geometry = cache.token();
            let tick_remaining = Cell::new(TICK_PATH_NODE_BUDGET);
            let mut rng = rand::thread_rng();
            let mut ticked = 0usize;
            let n = active.len();
            let start = brains.cursor % n.max(1);
            brains.cursor = 0;
            let mut node_budget_exhausted = false;
            for i in 0..n {
                let brain_started = Instant::now();
                let out_of_nodes = tick_remaining.get() == 0;
                if out_of_nodes || brain_started - started > TICK_BUDGET {
                    node_budget_exhausted = out_of_nodes;
                    brains.cursor = (start + i) % n;
                    break;
                }
                let a = &active[(start + i) % n];
                let ServerBrains {
                    entries,
                    trees,
                    stats,
                    ..
                } = &mut *brains;
                let Some(entry) = entries.get_mut(&a.id) else {
                    continue;
                };
                let Some(tree) = monster_ai::behavior_tree_for(trees, &entry.brain.behavior) else {
                    continue;
                };
                if entry.failed_paths.borrow_mut().synchronize(&geometry) {
                    entry.brain.retry_chase();
                }
                // Only a brain that starts below the full budget can be deferred.
                let previous_brain =
                    (tick_remaining.get() < BRAIN_PATH_NODE_BUDGET).then(|| entry.brain.clone());
                let previous_tick = entry.last_tick;
                let delta_ms = entry.owed_ms(brain_started, forced_delta_ms);
                let monsters: Vec<NearbyMonster> =
                    super::SpatialCell::within_radius(&entry.brain.position, radius)
                        .filter_map(|cell| standing.get(&cell))
                        .flatten()
                        .filter(|m| m.id != a.id)
                        .cloned()
                        .collect();
                let path =
                    CountingPath::new(&cache, &tick_remaining, &entry.failed_paths, brain_started);
                let position = entry.brain.position;
                let result = entry.brain.tick_with_behavior_tree(
                    delta_ms, &a.players, &monsters, tree, &path, &mut rng,
                );
                let elapsed_ms = brain_started.elapsed().as_secs_f32() * 1000.0;
                let paths = path.diagnostics.get();
                stats.pathfinds += paths.count();
                stats.expanded_nodes += paths.expanded_nodes;
                stats.cache_hits += paths.cache_hits;
                if stats
                    .slowest
                    .as_ref()
                    .is_none_or(|s| elapsed_ms > s.elapsed_ms)
                {
                    stats.slowest = Some(BrainSample {
                        monster_id: a.id.clone(),
                        position,
                        floor_level: a.floor_level,
                        behavior: entry.brain.behavior.clone(),
                        state: entry.brain.state(),
                        target: entry.brain.target_player_id(),
                        observed_at: crate::auth::unix_now(),
                        elapsed_ms,
                        paths,
                    });
                }
                if paths.deferred {
                    // Discard speculative movement and failed-chase backoff.
                    entry.brain = previous_brain.expect("deferred below the full budget");
                    entry.last_tick = previous_tick;
                    stats.deferred_brains += 1;
                    node_budget_exhausted = true;
                    brains.cursor = (start + i) % n;
                    break;
                }
                ticked += 1;
                commands.extend(result.into_iter().map(|c| (a.id.clone(), a.floor_level, c)));
            }
            let tick_elapsed = started.elapsed();
            let s = &mut brains.stats;
            s.ticks += 1;
            s.ticked += ticked as u64;
            s.commands += commands.len() as u64;
            s.over_budget += (tick_elapsed > TICK_BUDGET) as u32;
            s.node_budget_ticks += node_budget_exhausted as u32;
            s.worst_ms = s.worst_ms.max(tick_elapsed.as_secs_f32() * 1000.0);
        }
        if brains.stats_since.elapsed() >= STATS_LOG_PERIOD {
            let s = std::mem::take(&mut brains.stats);
            info!(
                "monster ai: brains {} active {} ticks {} ticked/tick {:.0} pathfinds/s {:.1} commands/s {:.1} over_budget {} worst {:.1}ms expanded_nodes {} cached_paths {} deferred_brains {} node_budget_ticks {}",
                brains.entries.len(),
                active.len(),
                s.ticks,
                s.ticked as f32 / s.ticks.max(1) as f32,
                s.pathfinds as f32 / STATS_LOG_PERIOD.as_secs_f32(),
                s.commands as f32 / STATS_LOG_PERIOD.as_secs_f32(),
                s.over_budget,
                s.worst_ms,
                s.expanded_nodes,
                s.cache_hits,
                s.deferred_brains,
                s.node_budget_ticks
            );
            if let Some(slowest) = &s.slowest {
                slowest.log();
            }
            brains.stats_since = Instant::now();
        }
        drop(brains);

        for (monster_id, floor_level, command) in commands {
            self.apply_ai_command(&monster_id, floor_level, command)
                .await;
        }
    }

    async fn apply_ai_command(&self, monster_id: &str, floor_level: i8, command: AiCommand) {
        match command {
            AiCommand::Move {
                position,
                rotation,
                state,
                target_position,
                chasing,
                ..
            } => {
                self.apply_ai_move(
                    monster_id,
                    floor_level,
                    position,
                    rotation,
                    state,
                    target_position,
                    chasing,
                )
                .await
            }
            AiCommand::Attack {
                target_player_id, ..
            } => self.monster_attack(monster_id, &target_player_id).await,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn apply_ai_move(
        &self,
        monster_id: &str,
        floor_level: i8,
        position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
        chasing: Option<ChaseAim>,
    ) {
        if !position.is_finite() || !rotation.is_finite() || state == MonsterState::Dead {
            warn!("Brain emitted a bad move for {monster_id}: {position:?} {state:?}");
            return;
        }
        let mut position = position.wrapped_x();
        let target_position = target_position.wrapped_x();
        let from = {
            let monsters = self.monsters.read().await;
            match monsters.get(monster_id) {
                Some(m) if m.state != MonsterState::Dead => m.position,
                _ => return,
            }
        };
        // A brain step through a wall means its path and the cache disagree;
        // refuse it and pull the brain back so it re-plans from the truth.
        let blocked = (from.x != position.x || from.z != position.z) && {
            let cache = self.passability_read();
            let to_x = from.x + shortest_world_delta_x(from.x, position.x);
            let floor = passability_floor_for_level(floor_level);
            is_movement_blocked(&cache, from.x, from.z, to_x, position.z, floor, None)
        };
        if blocked {
            warn!(
                "Refused wall-crossing brain move for {monster_id} on floor {floor_level}: {from:?} -> {position:?}"
            );
            let mut brains = self.monster_brains.lock().await;
            if let Some(entry) = brains.entries.get_mut(monster_id) {
                entry.brain.apply_authoritative_position(from);
            }
            return;
        }
        // Brains keep their spawn Y; the ground is the server's to settle.
        let y = self
            .expected_monster_move_y(floor_level, from, position)
            .await;
        position.y = y.unwrap_or(from.y);
        let despawns_when_unattended = {
            let mut monsters = self.monsters.write().await;
            let Some(m) = monsters.get_mut(monster_id) else {
                return;
            };
            if m.state == MonsterState::Dead {
                return;
            }
            m.rotation = rotation;
            m.state = state;
            let Some(m) = monsters.set_position(monster_id, position) else {
                return;
            };
            self.interest_lock().publish_monster_movement(
                m,
                ServerMessage::MonsterMoved {
                    monster_id: monster_id.to_string(),
                    position,
                    rotation,
                    state,
                    target_position: Position {
                        y: position.y,
                        ..target_position
                    },
                    chasing,
                },
            );
            m.lifecycle.despawns_when_unattended()
        };
        {
            let mut brains = self.monster_brains.lock().await;
            if let Some(entry) = brains.entries.get_mut(monster_id) {
                entry.brain.position.y = position.y;
            }
        }
        if despawns_when_unattended {
            self.despawn_unwatched_monsters(&[monster_id.to_owned()])
                .await;
        }
    }

    /// Feed a player's swing (hit or miss — a miss still aggros) to the brain
    /// and act on what it decides.
    pub(super) async fn brain_hit(
        &self,
        monster_id: &str,
        attacker_id: &PlayerId,
        hit: bool,
        damage: u32,
    ) {
        let commands = {
            let mut brains = self.monster_brains.lock().await;
            let Some(entry) = brains.entries.get_mut(monster_id) else {
                return;
            };
            // A swing lands between ticks; the hit pose must not rewind the
            // monster to the last tick's step.
            let owed = entry.owed_ms(Instant::now(), None);
            entry.brain.catch_up(owed);
            entry
                .brain
                .handle_hit_with_behavior_tree(attacker_id, hit, damage)
        };
        if commands.is_empty() {
            return;
        }
        let floor_level = {
            let monsters = self.monsters.read().await;
            match monsters.get(monster_id) {
                Some(m) => m.floor_level,
                None => return,
            }
        };
        for command in commands {
            self.apply_ai_command(monster_id, floor_level, command)
                .await;
        }
    }

    /// Where the brain has the monster right now; the registry trails it by
    /// up to a sync interval while it runs.
    pub(super) async fn brain_position_now(&self, monster_id: &str) -> Option<Position> {
        let mut brains = self.monster_brains.lock().await;
        let entry = brains.entries.get_mut(monster_id)?;
        let owed = entry.owed_ms(Instant::now(), None);
        entry.brain.catch_up(owed);
        Some(entry.brain.position)
    }

    pub(super) async fn brain_death(&self, monster_id: &str) {
        let mut brains = self.monster_brains.lock().await;
        if brains.entries.remove(monster_id).is_some() {
            debug!("Brain dropped for dead monster {monster_id}");
        }
    }

    #[cfg(test)]
    pub(crate) async fn brain_count(&self) -> usize {
        self.monster_brains.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_brain_diagnostics_preserve_the_costliest_path_and_all_outcomes() {
        let mut paths = PathDiagnostics::default();
        let sample = PathSample {
            start: (-1036.5, 4265.0, 1),
            goal: (-1035.0, 4263.0, 1),
            elapsed_ms: 12.0,
            termination: PathTermination::NodeLimit,
            waypoints: 2,
            expanded_nodes: 20_000,
        };
        paths.record(sample);
        paths.record(PathSample {
            elapsed_ms: 1.0,
            termination: PathTermination::Unreachable,
            waypoints: 0,
            expanded_nodes: 50,
            ..sample
        });
        paths.record(PathSample {
            elapsed_ms: 0.1,
            termination: PathTermination::Reached,
            waypoints: 1,
            expanded_nodes: 0,
            ..sample
        });
        assert_eq!(paths.count(), 3);
        assert_eq!(
            (paths.reached, paths.unreachable, paths.node_limit),
            (1, 1, 1)
        );
        assert_eq!(paths.partial, 1);
        assert_eq!(paths.expanded_nodes, 20_050);
        assert_eq!(paths.slowest.unwrap().elapsed_ms, 12.0);
        let brain = BrainSample {
            monster_id: "m123".into(),
            position: Position {
                x: -1036.5,
                y: -4.0,
                z: 4265.0,
            },
            floor_level: -1,
            behavior: "aggressive".into(),
            state: AiState::Hold,
            target: Some(PlayerId::from(42)),
            observed_at: 123456,
            elapsed_ms: 13.5,
            paths,
        };
        let (subscriber, buffer) = crate::test_util::capture_logs();
        tracing::subscriber::with_default(subscriber, || brain.log());
        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        for field in [
            "monster ai slowest brain",
            "monster_id=\"m123\"",
            "x=-1036.5",
            "floor_level=-1",
            "state=Hold",
            "target_player_id=42",
            "pathfinds=3",
            "path_termination=NodeLimit",
            "path_goal_x=-1035",
            "path_expanded_nodes=20000",
        ] {
            assert!(logs.contains(field), "{logs}");
        }
        assert!(!logs.contains("Some("), "{logs}");
        assert!(!logs.contains("Position {"), "{logs}");
    }

    #[tokio::test]
    async fn slowest_brain_is_logged_once_per_stats_window_and_reset() {
        use tracing::instrument::WithSubscriber;

        let game = super::super::tests::make_flat_world_game_state("brain_stats");
        game.add_player(super::super::tests::make_player("prey", 0.0, 0.0))
            .await;
        let monster = game
            .spawn_monster(
                "goblin".into(),
                Position {
                    x: 12.0,
                    y: 0.0,
                    z: 0.0,
                },
                0.0,
                0,
                crate::types::MonsterLifecycle::Ambient,
                None,
                true,
            )
            .await
            .unwrap();
        game.tick_monster_ai_by(200.0).await;
        {
            let mut brains = game.monster_brains.lock().await;
            assert_eq!(
                brains.stats.slowest.as_ref().unwrap().monster_id,
                monster.id
            );
            brains.stats_since = Instant::now() - STATS_LOG_PERIOD;
        }
        let (subscriber, buffer) = crate::test_util::capture_logs();
        async {
            game.tick_monster_ai_by(200.0).await;
            {
                let brains = game.monster_brains.lock().await;
                assert!(brains.stats.slowest.is_none());
                assert_eq!(brains.stats.ticks, 0);
            }
            game.tick_monster_ai_by(200.0).await;
        }
        .with_subscriber(subscriber)
        .await;
        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert_eq!(logs.matches("monster ai:").count(), 1, "{logs}");
        assert_eq!(
            logs.matches("monster ai slowest brain").count(),
            1,
            "{logs}"
        );
        assert!(
            logs.contains(&format!("monster_id=\"{}\"", monster.id)),
            "{logs}"
        );
        assert!(logs.contains("floor_level=0"), "{logs}");
    }

    #[test]
    fn closed_door_sensing_checks_the_opening_span() {
        let from = Position {
            x: -2.0,
            y: 0.0,
            z: 0.5,
        };
        let to = Position {
            x: 2.0,
            y: 0.0,
            z: 0.5,
        };
        assert!(door_blocks_sense(&from, &to, &[[0.0, 0.0, 0.0, 1.0]]));
        assert!(!door_blocks_sense(&from, &to, &[[0.0, 1.0, 0.0, 2.0]]));
        assert!(!door_blocks_sense(&from, &to, &[]));
        let other_side = Position { x: -1.0, ..to };
        assert!(!door_blocks_sense(
            &from,
            &other_side,
            &[[0.0, 0.0, 0.0, 1.0]]
        ));
        let from = Position {
            x: 0.5,
            y: 0.0,
            z: -2.0,
        };
        let to = Position {
            x: 0.5,
            y: 0.0,
            z: 2.0,
        };
        assert!(door_blocks_sense(&from, &to, &[[0.0, 0.0, 1.0, 0.0]]));
    }
}

#[cfg(test)]
mod scheduling_tests;
