use super::{passability, GameState};
use crate::types::{Player, PlayerId, Position, ServerMessage};
use onlinerpg_shared::messages::{MoveStatus, MoveWaypoint};
use onlinerpg_shared::pathfinding::{self, PassabilityCache, PathTermination, PathWaypoint};
use onlinerpg_shared::{
    dungeon, shortest_world_delta_x, wrap_world_x, MAX_MOVE_TARGET_DISTANCE, PLAYER_MOVE_SPEED,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;

const SEARCH_INTERVAL: Duration = Duration::from_millis(100);
const MAX_ROUTE_SAMPLES: usize = 1024;

#[derive(Clone, Copy)]
struct Goal {
    request_id: u32,
    x: f32,
    z: f32,
    sprinting: bool,
    stop_at_entrance: bool,
}

pub(super) struct GoalMovement {
    last_id: u32,
    generation: u64,
    owned: bool,
    pending: Option<Goal>,
    running: bool,
    next_search: Instant,
    plan: Option<Plan>,
    direction: Option<direction::Direction>,
    last_diagnostic: Option<Instant>,
    invalid_requests: u32,
}

struct Plan {
    goal: Goal,
    waypoints: Vec<MoveWaypoint>,
    next: usize,
    termination: PathTermination,
    advanced_at: Instant,
    height_revision: u64,
}

impl GoalMovement {
    fn new(request_id: u32) -> Self {
        Self {
            last_id: request_id.wrapping_sub(1),
            generation: 0,
            owned: false,
            pending: None,
            running: false,
            next_search: Instant::now(),
            plan: None,
            direction: None,
            last_diagnostic: None,
            invalid_requests: 0,
        }
    }

    fn newer(&self, id: u32) -> bool {
        let delta = id.wrapping_sub(self.last_id);
        delta != 0 && delta < 1 << 31
    }

    fn accept(&mut self, id: u32) -> bool {
        if !self.newer(id) {
            return false;
        }
        self.last_id = id;
        self.generation = self.generation.wrapping_add(1);
        self.pending = None;
        self.plan = None;
        self.direction = None;
        self.owned = true;
        true
    }
}

fn progress(
    player: &Player,
    request_id: u32,
    next: usize,
    speed: f32,
    status: MoveStatus,
) -> ServerMessage {
    ServerMessage::PlayerMoveProgress {
        request_id,
        server_time_ms: GameState::now_ms(),
        position: player.position,
        rotation: player.rotation,
        floor_level: player.floor_level,
        next_waypoint: next as u32,
        speed,
        status,
    }
}

fn eligible(player: &Player) -> bool {
    player.health > 0 && player.object_type.is_none()
}

fn move_speed(mult: f32, sprinting: bool) -> f32 {
    PLAYER_MOVE_SPEED * mult * onlinerpg_shared::hunger::sprint_move_mult(sprinting)
}

impl GameState {
    async fn leave_pose_for_move(&self, id: PlayerId, request_id: u32) {
        let fresh = self
            .goal_moves
            .lock()
            .await
            .get(&id)
            .is_none_or(|state| state.newer(request_id));
        if fresh {
            self.clear_pose_on_move(&id, "move").await;
        }
    }

    #[cfg(test)]
    pub(crate) async fn request_move_goal(
        &self,
        id: PlayerId,
        request_id: u32,
        x: f32,
        z: f32,
        sprinting: bool,
    ) {
        self.request_move_goal_with_entrance(id, request_id, x, z, sprinting, false)
            .await;
    }

    pub(crate) async fn request_move_goal_with_entrance(
        &self,
        id: PlayerId,
        request_id: u32,
        x: f32,
        z: f32,
        sprinting: bool,
        stop_at_entrance: bool,
    ) {
        if !x.is_finite() || !z.is_finite() {
            self.reject_move_input(id, request_id, "coordinates").await;
            return;
        }
        self.advance_goal_players(&[id]).await;
        let x = wrap_world_x(x);
        let valid = self.players.read().await.get(&id).is_some_and(|player| {
            player.health > 0
                && shortest_world_delta_x(player.position.x, x).hypot(z - player.position.z)
                    <= MAX_MOVE_TARGET_DISTANCE
        });
        if !valid {
            self.reject_move_input(id, request_id, "state_or_distance")
                .await;
            return;
        }
        self.leave_pose_for_move(id, request_id).await;
        let mut goals = self.goal_moves.lock().await;
        let players = self.players.read().await;
        let Some(player) = players.get(&id) else {
            return;
        };
        if !eligible(player)
            || shortest_world_delta_x(player.position.x, x).hypot(z - player.position.z)
                > MAX_MOVE_TARGET_DISTANCE
        {
            self.send_direct_message(
                &id,
                progress(player, request_id, 0, 0.0, MoveStatus::Rejected),
            )
            .await;
            return;
        }
        let state = goals
            .entry(id)
            .or_insert_with(|| GoalMovement::new(request_id));
        if !state.accept(request_id) {
            return;
        }
        let mut versions = self.player_movement_versions.write().await;
        *versions.entry(id).or_default() += 1;
        state.pending = Some(Goal {
            request_id,
            x,
            z,
            sprinting,
            stop_at_entrance,
        });
        self.send_direct_message(
            &id,
            progress(player, request_id, 0, 0.0, MoveStatus::Searching),
        )
        .await;
        if !state.running {
            state.running = true;
            let game = self.clone();
            tokio::spawn(async move {
                game.run_goal_search(id).await;
            });
        }
    }

    pub(crate) async fn stop_move_goal(&self, id: PlayerId, request_id: u32) {
        self.advance_goal_players(&[id]).await;
        let mut goals = self.goal_moves.lock().await;
        let state = goals
            .entry(id)
            .or_insert_with(|| GoalMovement::new(request_id));
        if !state.accept(request_id) {
            return;
        }
        let mut versions = self.player_movement_versions.write().await;
        *versions.entry(id).or_default() += 1;
        if let Some(player) = self.players.read().await.get(&id) {
            self.send_direct_message(
                &id,
                progress(player, request_id, 0, 0.0, MoveStatus::Stopped),
            )
            .await;
        }
    }

    pub(super) async fn cancel_goal_movement(&self, id: &PlayerId) {
        let mut goals = self.goal_moves.lock().await;
        if let Some(state) = goals.get_mut(id) {
            state.generation = state.generation.wrapping_add(1);
            state.pending = None;
            state.plan = None;
            state.direction = None;
            if state.owned {
                state.owned = false;
                if let Some(player) = self.players.read().await.get(id) {
                    self.send_direct_message(
                        id,
                        progress(player, state.last_id, 0, 0.0, MoveStatus::Stopped),
                    )
                    .await;
                }
            }
        }
    }

    async fn run_goal_search(&self, id: PlayerId) {
        loop {
            let deadline = {
                let mut goals = self.goal_moves.lock().await;
                let Some(state) = goals.get_mut(&id) else {
                    return;
                };
                if state.pending.is_none() {
                    state.running = false;
                    return;
                }
                state.next_search
            };
            tokio::time::sleep_until(deadline).await;
            let (goal, generation, player) = {
                let mut goals = self.goal_moves.lock().await;
                let Some(state) = goals.get_mut(&id) else {
                    return;
                };
                let Some(goal) = state.pending.take() else {
                    state.running = false;
                    return;
                };
                let Some(player) = self.players.read().await.get(&id).cloned() else {
                    goals.remove(&id);
                    return;
                };
                state.next_search = Instant::now() + SEARCH_INTERVAL;
                (goal, state.generation, player)
            };
            let height_revision = self.height_sampler.revision().await;
            let route = self.find_goal_route(&player, goal).await;
            let profile = self.hunger_movement_profiles_for(&[id]).await;
            let (mult, sprint_allowed) = profile.get(&id).copied().unwrap_or((1.0, true));
            let mut goals = self.goal_moves.lock().await;
            let Some(state) = goals.get_mut(&id) else {
                return;
            };
            if state.generation != generation {
                continue;
            }
            let players = self.players.read().await;
            let Some(current) = players.get(&id) else {
                goals.remove(&id);
                return;
            };
            if !eligible(current)
                || current.position != player.position
                || current.floor_level != player.floor_level
            {
                self.send_direct_message(
                    &id,
                    progress(current, goal.request_id, 0, 0.0, MoveStatus::Stopped),
                )
                .await;
                continue;
            }
            match route {
                Ok((waypoints, termination, snapshot))
                    if self.passability.is_current(&snapshot)
                        && height_revision == self.height_sampler.revision().await =>
                {
                    let speed = move_speed(mult, goal.sprinting && sprint_allowed)
                        * current.mount.map_or(1.0, |mount| mount.speed_mult());
                    self.send_direct_message(
                        &id,
                        ServerMessage::PlayerMovePath {
                            request_id: goal.request_id,
                            server_time_ms: Self::now_ms(),
                            position: current.position,
                            rotation: current.rotation,
                            floor_level: current.floor_level,
                            waypoints: display_waypoints(&waypoints, current, speed),
                            speed,
                            termination,
                        },
                    )
                    .await;
                    state.plan = Some(Plan {
                        goal,
                        waypoints,
                        next: 0,
                        termination,
                        advanced_at: Instant::now(),
                        height_revision,
                    });
                }
                result => {
                    let status = result.err().unwrap_or(MoveStatus::MapChanged);
                    self.send_direct_message(
                        &id,
                        progress(current, goal.request_id, 0, 0.0, status),
                    )
                    .await;
                }
            }
        }
    }

    async fn goal_ground_y(&self, floor: u8, position: Position) -> Option<f32> {
        if dungeon::floor_level_for_passability(floor) < 0 {
            let entrance = self.dungeon_defs.entrance_at(position.x, position.z)?;
            let dungeons = self.dungeons.read().await;
            let runtime = dungeons.get(&entrance.id)?;
            dungeon::ground_y_for_floor(
                &entrance.position(),
                &runtime.layouts,
                floor,
                position.x,
                position.z,
            )
        } else {
            Some(
                self.surface_ground_y(floor, &position, position.y, None)
                    .await,
            )
        }
    }

    async fn find_goal_route(
        &self,
        player: &Player,
        goal: Goal,
    ) -> Result<
        (
            Vec<MoveWaypoint>,
            PathTermination,
            std::sync::Weak<PassabilityCache>,
        ),
        MoveStatus,
    > {
        if let Some(entrance) = self
            .dungeon_defs
            .entrance_at(player.position.x, player.position.z)
        {
            self.ensure_dungeon_runtime(&entrance.id).await;
        }
        if let Some(entrance) = self.dungeon_defs.entrance_at(goal.x, goal.z) {
            self.ensure_dungeon_runtime(&entrance.id).await;
        }
        let floor = dungeon::passability_floor_for_level(player.floor_level);
        let mut target = Position {
            x: goal.x,
            y: player.position.y,
            z: goal.z,
        };
        target.y = self
            .goal_ground_y(floor, target)
            .await
            .ok_or(MoveStatus::Rejected)?;
        let snapshot = self.passability.snapshot();
        let token = Arc::downgrade(&snapshot);
        let start_floor = pathfinding::start_floor_at(
            &snapshot,
            player.position.x,
            player.position.z,
            player.position.y,
        );
        let goal_floor = if pathfinding::in_stairwell_span(&snapshot, target.x, target.z, target.y)
        {
            pathfinding::start_floor_at(&snapshot, target.x, target.z, target.y)
        } else {
            floor
        };
        let goal_x = player.position.x + shortest_world_delta_x(player.position.x, target.x);
        let snapshot = if (goal_x - target.x).abs() > 1.0 {
            let mut shifted = (*snapshot).clone();
            for entry in shifted.values_mut() {
                let center = (entry.min_x + entry.max_x) * 0.5;
                let offset =
                    player.position.x + shortest_world_delta_x(player.position.x, center) - center;
                entry.house_origin_x += offset;
                entry.min_x += offset;
                entry.max_x += offset;
            }
            Arc::new(shifted)
        } else {
            snapshot
        };
        let mut path = self
            .path_search
            .search(
                snapshot,
                PathWaypoint {
                    x: player.position.x,
                    z: player.position.z,
                    floor: start_floor,
                },
                PathWaypoint {
                    x: goal_x,
                    z: target.z,
                    floor: goal_floor,
                },
                dungeon::path_max_nodes(start_floor, goal_floor),
            )
            .await
            .map_err(|_| MoveStatus::Busy)?;
        if goal.stop_at_entrance {
            self.stop_route_at_entrance(player, &target, &mut path.waypoints)
                .await;
        }
        if let Some(mount) = player.mount {
            let (waypoints, termination) = self.mount_route(player, &path.waypoints, mount).await;
            return Ok((waypoints, termination.unwrap_or(path.termination), token));
        }
        let mut waypoints = Vec::new();
        let mut previous = player.position;
        for point in &path.waypoints {
            let dx = shortest_world_delta_x(previous.x, point.x);
            let dz = point.z - previous.z;
            let count = dx.hypot(dz).ceil().max(1.0) as usize;
            if waypoints.len() + count > MAX_ROUTE_SAMPLES {
                return Err(MoveStatus::NodeLimit);
            }
            let from = previous;
            for step in 1..=count {
                let fraction = step as f32 / count as f32;
                let mut position = Position {
                    x: wrap_world_x(from.x + dx * fraction),
                    y: previous.y,
                    z: from.z + dz * fraction,
                };
                position.y = self
                    .goal_ground_y(point.floor, position)
                    .await
                    .ok_or(MoveStatus::Rejected)?;
                waypoints.push(MoveWaypoint {
                    position,
                    floor_level: dungeon::floor_level_for_passability(point.floor),
                    rotation: None,
                    travel_seconds: None,
                });
                previous = position;
            }
        }
        Ok((waypoints, path.termination, token))
    }

    pub(super) async fn tick_goal_movement(&self) {
        let ids: Vec<_> = self
            .goal_moves
            .lock()
            .await
            .iter()
            .filter_map(|(id, s)| (s.plan.is_some() || s.direction.is_some()).then_some(*id))
            .collect();
        self.advance_goal_players(&ids).await;
    }

    pub(crate) async fn advance_goal_players(&self, ids: &[PlayerId]) {
        let _movement = self.movement_gate.lock().await;
        self.advance_goal_players_unlocked(ids).await;
    }

    pub(super) async fn advance_goal_players_unlocked(&self, ids: &[PlayerId]) {
        self.advance_direction_players(ids).await;
        if ids.is_empty() {
            return;
        }
        let profiles = self.hunger_movement_profiles_for(ids).await;
        let mut goals = self.goal_moves.lock().await;
        let height_revision = self.height_sampler.revision().await;
        for id in ids {
            let Some(plan) = goals.get_mut(id).and_then(|s| s.plan.as_mut()) else {
                continue;
            };
            if plan.height_revision == height_revision {
                continue;
            }
            let Some(mut player) = self.players.read().await.get(id).cloned() else {
                continue;
            };
            if player.floor_level >= 0 {
                player.position.y = self
                    .surface_ground_y(
                        player.floor_level as u8,
                        &player.position,
                        player.position.y,
                        player.mount,
                    )
                    .await;
                if let Some(current) = self.players.write().await.get_mut(id) {
                    current.position.y = player.position.y;
                }
            }
            for point in plan.waypoints.iter_mut().skip(plan.next) {
                if point.floor_level >= 0 {
                    point.position.y = self
                        .surface_ground_y(
                            point.floor_level as u8,
                            &point.position,
                            point.position.y,
                            player.mount,
                        )
                        .await;
                }
            }
            plan.height_revision = height_revision;
            plan.waypoints.drain(..plan.next);
            plan.next = 0;
            let (mult, allowed) = profiles.get(id).copied().unwrap_or((1.0, true));
            let speed = move_speed(mult, plan.goal.sprinting && allowed)
                * player.mount.map_or(1.0, |m| m.speed_mult());
            self.send_direct_message(
                id,
                ServerMessage::PlayerMovePath {
                    request_id: plan.goal.request_id,
                    server_time_ms: Self::now_ms(),
                    position: player.position,
                    rotation: player.rotation,
                    floor_level: player.floor_level,
                    waypoints: display_waypoints(&plan.waypoints, &player, speed),
                    speed,
                    termination: plan.termination,
                },
            )
            .await;
        }
        let dungeons = self.dungeons.read().await;
        let mut players = self.players.write().await;
        let mut moved = Vec::new();
        let mut activities = Vec::new();
        let mut steps = Vec::new();
        let mut updates = Vec::new();
        for &id in ids {
            let Some(state) = goals.get_mut(&id) else {
                continue;
            };
            let Some(plan) = state.plan.as_mut() else {
                continue;
            };
            let Some(player) = players.get_mut(&id) else {
                state.plan = None;
                continue;
            };
            let now = Instant::now();
            let dt = now.duration_since(plan.advanced_at).as_secs_f32();
            plan.advanced_at = now;
            let (mult, allowed) = profiles.get(&id).copied().unwrap_or((1.0, true));
            let sprinting = plan.goal.sprinting && allowed;
            let speed =
                move_speed(mult, sprinting) * player.mount.map_or(1.0, |mount| mount.speed_mult());
            let old_position = player.position;
            let old_floor = player.floor_level;
            let (status, travelled) = if !eligible(player) {
                (MoveStatus::Stopped, 0.0)
            } else {
                let cache = self.passability_read();
                advance_plan(player, plan, &cache, speed * dt, |player| {
                    if let Some(entrance) = self
                        .dungeon_defs
                        .entrance_at(player.position.x, player.position.z)
                    {
                        if let Some(runtime) = dungeons.get(&entrance.id) {
                            let floor = dungeon::passability_floor_for_level(player.floor_level);
                            if let Some(y) = dungeon::ground_y_for_floor(
                                &entrance.position(),
                                &runtime.layouts,
                                floor,
                                player.position.x,
                                player.position.z,
                            ) {
                                player.position.y = y;
                            }
                            player.floor_level = dungeon_floor_after_step(
                                player.floor_level,
                                &entrance.position(),
                                &runtime.layouts,
                                player.position,
                            );
                        }
                    }
                })
            };
            let update = if player.mount.is_some() && status == MoveStatus::Moving {
                ServerMessage::PlayerMovePath {
                    request_id: plan.goal.request_id,
                    server_time_ms: Self::now_ms(),
                    position: player.position,
                    rotation: player.rotation,
                    floor_level: player.floor_level,
                    waypoints: display_waypoints(&plan.waypoints[plan.next..], player, speed),
                    speed,
                    termination: plan.termination,
                }
            } else {
                progress(
                    player,
                    plan.goal.request_id,
                    plan.next,
                    if status == MoveStatus::Moving {
                        speed
                    } else {
                        0.0
                    },
                    status,
                )
            };
            updates.push((id, update));
            if player.position != old_position || player.floor_level != old_floor {
                activities.push((id, travelled / speed.max(f32::EPSILON), sprinting));
                steps.push(super::ambient_spawn::MoveStep {
                    player_id: id,
                    from: old_position,
                    to: player.position,
                    floor_level: player.floor_level,
                    is_official_npc: player.is_official_npc,
                    mount: player.mount,
                });
                moved.push((id, old_position, old_floor, player.clone(), sprinting));
            }
            if status != MoveStatus::Moving {
                state.plan = None;
            }
        }
        drop(players);
        drop(dungeons);
        for (id, message) in updates {
            self.send_direct_message(&id, message).await;
        }
        drop(goals);
        self.record_movement_activity(&activities).await;
        for (id, old_position, old_floor, player, sprinting) in moved {
            let message = ServerMessage::PlayerMoved {
                player_id: id,
                position: player.position,
                rotation: player.rotation,
                floor_level: player.floor_level,
                sprinting,
            };
            self.finish_position_update(&id, old_position, old_floor, player, message)
                .await;
        }
        self.spawn_along_movement(&steps).await;
        self.soak_movers(&steps).await;
    }
}

fn dungeon_floor_after_step(
    current: i8,
    entrance: &Position,
    layouts: &[dungeon::FloorLayout],
    position: Position,
) -> i8 {
    if current > 0 {
        return current;
    }
    let depth = current.unsigned_abs();
    let switch = dungeon::SHAFT_LEN as f32 * 0.2;
    if depth == 0 {
        return if layouts
            .first()
            .and_then(|l| dungeon::shaft_run_pos(entrance, &l.up_shaft, position.x, position.z))
            .is_some_and(|t| t > switch + 0.3)
        {
            -1
        } else {
            0
        };
    }
    if let Some(layout) = layouts.get(depth as usize - 1) {
        if dungeon::shaft_run_pos(entrance, &layout.up_shaft, position.x, position.z)
            .is_some_and(|t| t < switch - 0.3)
        {
            return current + 1;
        }
        if layout
            .down_shaft
            .as_ref()
            .and_then(|s| dungeon::shaft_run_pos(entrance, s, position.x, position.z))
            .is_some_and(|t| t > switch + 0.3)
        {
            return current - 1;
        }
    }
    current
}

fn display_waypoints(waypoints: &[MoveWaypoint], player: &Player, speed: f32) -> Vec<MoveWaypoint> {
    let base = PLAYER_MOVE_SPEED * player.mount.map_or(1.0, |m| m.speed_mult());
    waypoints
        .iter()
        .take(if player.mount.is_some() {
            128
        } else {
            usize::MAX
        })
        .cloned()
        .map(|mut point| {
            if let Some(seconds) = &mut point.travel_seconds {
                *seconds *= base / speed.max(0.01);
            }
            point
        })
        .collect()
}

fn advance_plan(
    player: &mut Player,
    plan: &mut Plan,
    cache: &PassabilityCache,
    mut distance: f32,
    mut settle: impl FnMut(&mut Player),
) -> (MoveStatus, f32) {
    let mut travelled = 0.0;
    while let Some(waypoint) = plan.waypoints.get_mut(plan.next) {
        let from = player.position;
        let dx = shortest_world_delta_x(from.x, waypoint.position.x);
        let dz = waypoint.position.z - from.z;
        let length = dx.hypot(dz);
        let cost = waypoint.travel_seconds.map_or(length, |seconds| {
            seconds * PLAYER_MOVE_SPEED * player.mount.map_or(1.0, |m| m.speed_mult())
        });
        if distance <= 0.0 && cost > 1e-5 {
            return (MoveStatus::Moving, travelled);
        }
        let fraction = if cost < 1e-5 {
            1.0
        } else {
            (distance / cost).min(1.0)
        };
        let floor = pathfinding::start_floor_at(cache, from.x, from.z, from.y);
        if floor == 0
            && waypoint.position.y - from.y > length * 50_f32.to_radians().tan()
            && !pathfinding::in_stairwell_span(cache, from.x, from.z, from.y)
            && pathfinding::storey_ground_y(cache, floor, from.x, from.z, from.y).is_none()
        {
            return (MoveStatus::Blocked, travelled);
        }
        let blocked = |t: f32| swept_blocked(cache, from, dx * t, dz * t, floor);
        let obstruction = blocked(fraction);
        let safe = if obstruction {
            let (mut lo, mut hi) = (0.0, fraction);
            for _ in 0..12 {
                let mid = (lo + hi) * 0.5;
                if blocked(mid) {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            lo
        } else {
            fraction
        };
        player.position = Position {
            x: wrap_world_x(from.x + dx * safe),
            y: from.y + (waypoint.position.y - from.y) * safe,
            z: from.z + dz * safe,
        };
        if let Some(rotation) = waypoint.rotation {
            player.rotation +=
                onlinerpg_shared::mount_movement::angle_delta(player.rotation, rotation) * safe;
        } else if length > 1e-5 {
            player.rotation = dx.atan2(dz);
        }
        if player.floor_level >= 0 && waypoint.floor_level >= 0 {
            player.floor_level = pathfinding::get_floor_at_position(
                cache,
                player.position.x,
                player.position.z,
                player.position.y,
            ) as i8;
        }
        travelled += length * safe;
        settle(player);
        if obstruction {
            return (MoveStatus::Blocked, travelled);
        }
        distance -= cost * fraction;
        if let Some(seconds) = &mut waypoint.travel_seconds {
            *seconds *= 1.0 - fraction;
        }
        if fraction < 1.0 {
            return (MoveStatus::Moving, travelled);
        }
        plan.next += 1;
    }
    let status = match plan.termination {
        PathTermination::Reached => MoveStatus::Arrived,
        PathTermination::Unreachable => MoveStatus::Partial,
        PathTermination::NodeLimit => MoveStatus::NodeLimit,
    };
    (status, travelled)
}

fn swept_blocked(cache: &PassabilityCache, from: Position, dx: f32, dz: f32, floor: u8) -> bool {
    if passability::wrapped_block_info(
        cache,
        from.x,
        from.z,
        from.x + dx,
        from.z + dz,
        floor,
        from.y,
    )
    .is_some()
    {
        return true;
    }
    let radius = 0.3;
    if pathfinding::is_circle_blocked_on_floor(cache, from.x, from.z, radius, floor, Some(from.y)) {
        return false;
    }
    if pathfinding::is_circle_blocked_on_floor(
        cache,
        wrap_world_x(from.x + dx),
        from.z + dz,
        radius,
        floor,
        Some(from.y),
    ) {
        return true;
    }
    let length = dx.hypot(dz);
    if length < 1e-5 {
        return false;
    }
    let (ox, oz) = (-dz / length * radius, dx / length * radius);
    [-1.0, 1.0].into_iter().any(|side| {
        passability::wrapped_block_info(
            cache,
            from.x + ox * side,
            from.z + oz * side,
            from.x + dx + ox * side,
            from.z + dz + oz * side,
            floor,
            from.y,
        )
        .is_some()
    })
}

mod direction;
mod doors;
mod route;

#[cfg(test)]
mod tests;
