use super::*;
use onlinerpg_shared::mount_movement::{self, STEP_SECONDS};

const INPUT_LEASE: Duration = Duration::from_millis(500);

#[derive(Clone)]
pub(super) struct Direction {
    request_id: u32,
    rotation: f32,
    forward: i8,
    turn: i8,
    sprinting: bool,
    speed: f32,
    height_revision: u64,
    pub(super) advanced_at: Instant,
    pub(super) expires_at: Instant,
}

impl GameState {
    pub(crate) async fn request_move_direction(
        &self,
        id: PlayerId,
        request_id: u32,
        rotation: f32,
        forward: i8,
        turn: i8,
        sprinting: bool,
    ) {
        if !rotation.is_finite() || !(-1..=1).contains(&forward) || !(-1..=1).contains(&turn) {
            self.reject_move_input(id, request_id, "direction").await;
            return;
        }
        self.advance_goal_players(&[id]).await;
        if self
            .players
            .read()
            .await
            .get(&id)
            .is_some_and(|p| p.health > 0)
        {
            self.leave_pose_for_move(id, request_id).await;
        }
        let mut goals = self.goal_moves.lock().await;
        let Some(player) = self.players.read().await.get(&id).cloned() else {
            return;
        };
        if !eligible(&player) {
            return;
        }
        let state = goals
            .entry(id)
            .or_insert_with(|| GoalMovement::new(request_id));
        if let Some(direction) = state.direction.as_mut() {
            if direction.request_id == request_id {
                direction.expires_at = Instant::now() + INPUT_LEASE;
                return;
            }
        }
        let speed = state.direction.as_ref().map_or(0.0, |d| d.speed);
        if !state.accept(request_id) {
            return;
        }
        let now = Instant::now();
        let direction = Direction {
            request_id,
            rotation,
            forward,
            turn,
            sprinting: sprinting && forward > 0,
            speed,
            height_revision: self.height_sampler.revision().await,
            advanced_at: now,
            expires_at: now + INPUT_LEASE,
        };
        let profiles = self.hunger_movement_profiles_for(&[id]).await;
        let (mult, allowed) = profiles.get(&id).copied().unwrap_or((1.0, true));
        self.send_direction_path(
            id,
            &player,
            &direction,
            mult,
            direction.sprinting && allowed,
        )
        .await;
        state.direction = Some(direction);
        *self
            .player_movement_versions
            .write()
            .await
            .entry(id)
            .or_default() += 1;
    }

    pub(crate) async fn face_player(&self, id: PlayerId, rotation: f32) {
        if !rotation.is_finite() {
            return;
        }
        self.advance_goal_players(&[id]).await;
        let goals = self.goal_moves.lock().await;
        if goals
            .get(&id)
            .is_some_and(|s| s.plan.is_some() || s.direction.is_some())
        {
            return;
        }
        let Some(player) = ({
            let mut players = self.players.write().await;
            players.get_mut(&id).filter(|p| p.health > 0).map(|p| {
                p.rotation = rotation;
                p.clone()
            })
        }) else {
            return;
        };
        drop(goals);
        self.publish_nearby(
            &player.position,
            player.floor_level,
            ServerMessage::PlayerMoved {
                player_id: id,
                position: player.position,
                rotation,
                floor_level: player.floor_level,
                sprinting: false,
            },
            None,
        )
        .await;
    }

    pub(crate) async fn relocate_npc(
        &self,
        id: PlayerId,
        position: Position,
        rotation: f32,
        floor_level: i8,
    ) {
        if !position.is_finite()
            || !rotation.is_finite()
            || floor_level < 0
            || floor_level > onlinerpg_shared::housing::MAX_FLOOR_LEVEL as i8
        {
            return;
        }
        if !self
            .players
            .read()
            .await
            .get(&id)
            .is_some_and(|p| p.is_official_npc)
        {
            return;
        }
        self.teleport_player(&id, position, rotation, floor_level)
            .await;
    }

    pub(super) async fn reject_move_input(
        &self,
        id: PlayerId,
        request_id: u32,
        reason: &'static str,
    ) {
        let mut goals = self.goal_moves.lock().await;
        let state = goals
            .entry(id)
            .or_insert_with(|| GoalMovement::new(request_id));
        state.invalid_requests = state.invalid_requests.saturating_add(1);
        let now = Instant::now();
        if state
            .last_diagnostic
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(10))
        {
            tracing::warn!(%id, request_id, reason, count = state.invalid_requests, "Invalid movement input");
            state.last_diagnostic = Some(now);
            state.invalid_requests = 0;
        }
        if let Some(player) = self.players.read().await.get(&id) {
            self.send_direct_message(
                &id,
                progress(player, request_id, 0, 0.0, MoveStatus::Rejected),
            )
            .await;
        }
    }

    pub(super) async fn advance_direction_players(&self, ids: &[PlayerId]) {
        let profiles = self.hunger_movement_profiles_for(ids).await;
        let mut goals = self.goal_moves.lock().await;
        let mut moved = Vec::new();
        let mut steps = Vec::new();
        let mut activities = Vec::new();
        for &id in ids {
            let Some(state) = goals.get_mut(&id) else {
                continue;
            };
            let Some(direction) = state.direction.as_mut() else {
                continue;
            };
            let Some(mut player) = self.players.read().await.get(&id).cloned() else {
                state.direction = None;
                continue;
            };
            let (old_position, old_rotation, old_floor) =
                (player.position, player.rotation, player.floor_level);
            let revision = self.height_sampler.revision().await;
            if direction.height_revision != revision && player.floor_level >= 0 {
                player.position.y = self
                    .surface_ground_y(
                        player.floor_level as u8,
                        &player.position,
                        player.position.y,
                        player.mount,
                    )
                    .await;
            }
            direction.height_revision = revision;
            let now = Instant::now();
            let dt = now
                .min(direction.expires_at)
                .saturating_duration_since(direction.advanced_at)
                .as_secs_f32();
            direction.advanced_at = now;
            let (mult, allowed) = profiles.get(&id).copied().unwrap_or((1.0, true));
            let sprinting = direction.sprinting && allowed;
            let (points, blocked) = self
                .simulate_direction(&mut player, direction, dt, mult, sprinting)
                .await;
            let travelled: f32 = points
                .iter()
                .scan(old_position, |from, point| {
                    let distance = from.dist_xz_sq(&point.position).sqrt();
                    *from = point.position;
                    Some(distance)
                })
                .sum();
            let (player, changed) = {
                let mut players = self.players.write().await;
                let Some(current) = players.get_mut(&id) else {
                    state.direction = None;
                    continue;
                };
                let changed = eligible(current)
                    && (player.position != old_position
                        || player.rotation != old_rotation
                        || player.floor_level != old_floor);
                if changed {
                    current.position = player.position;
                    current.rotation = player.rotation;
                    current.floor_level = player.floor_level;
                }
                (current.clone(), changed)
            };
            let status = if !eligible(&player) || now >= direction.expires_at {
                MoveStatus::Stopped
            } else if blocked {
                MoveStatus::Blocked
            } else {
                MoveStatus::Moving
            };
            if changed {
                steps.push(super::super::ambient_spawn::MoveStep {
                    player_id: id,
                    from: old_position,
                    to: player.position,
                    floor_level: player.floor_level,
                    is_official_npc: player.is_official_npc,
                    mount: player.mount,
                });
                activities.push((
                    id,
                    travelled / move_speed(mult, sprinting).max(0.01),
                    sprinting,
                ));
            }
            if status == MoveStatus::Moving {
                self.send_direction_path(id, &player, direction, mult, sprinting)
                    .await;
            } else {
                self.send_direct_message(
                    &id,
                    progress(&player, direction.request_id, 0, 0.0, status),
                )
                .await;
                state.direction = None;
            }
            if changed {
                moved.push((id, old_position, old_floor, player, sprinting));
            }
        }
        drop(goals);
        self.record_movement_activity(&activities).await;
        for (id, old_position, old_floor, player, sprinting) in moved {
            let message = ServerMessage::PlayerMoved {
                player_id: player.id,
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

    async fn send_direction_path(
        &self,
        id: PlayerId,
        player: &Player,
        direction: &Direction,
        mult: f32,
        sprinting: bool,
    ) {
        let mut forecast = player.clone();
        let mut input = direction.clone();
        let dt = direction
            .expires_at
            .saturating_duration_since(Instant::now())
            .as_secs_f32()
            .min(0.4);
        let (waypoints, _) = self
            .simulate_direction(&mut forecast, &mut input, dt, mult, sprinting)
            .await;
        self.send_direct_message(
            &id,
            ServerMessage::PlayerMovePath {
                request_id: direction.request_id,
                server_time_ms: Self::now_ms(),
                position: player.position,
                rotation: player.rotation,
                floor_level: player.floor_level,
                waypoints,
                speed: input.speed,
                termination: PathTermination::Reached,
            },
        )
        .await;
    }

    async fn simulate_direction(
        &self,
        player: &mut Player,
        input: &mut Direction,
        mut dt: f32,
        mult: f32,
        sprinting: bool,
    ) -> (Vec<MoveWaypoint>, bool) {
        let mut points = Vec::new();
        if !eligible(player) {
            return (points, true);
        }
        while dt > 1e-5 {
            let step = dt.min(STEP_SECONDS);
            dt -= step;
            let from = player.position;
            let mount = player.mount;
            let max_speed = if mount.is_some() && input.forward < 0 {
                mount_movement::BACKWARD_SPEED * mult
            } else {
                move_speed(mult, sprinting) * mount.map_or(1.0, |m| m.speed_mult())
            };
            input.speed = if input.forward == 0 {
                0.0
            } else {
                (input.speed + max_speed * 4.0 * step).min(max_speed)
            };
            let (dx, dz, rotation) = if let Some(mount) = mount {
                let desired = if input.turn != 0 {
                    player.rotation - input.turn as f32 * std::f32::consts::FRAC_PI_2 / 0.6 * step
                } else {
                    input.rotation
                };
                if input.forward == 0 {
                    (
                        0.0,
                        0.0,
                        mount_movement::keyboard_rotation(player.rotation, desired, step),
                    )
                } else {
                    let reverse = if input.forward < 0 {
                        std::f32::consts::PI
                    } else {
                        0.0
                    };
                    let (x, z, r) = mount_movement::arc_step(
                        player.rotation + reverse,
                        desired + reverse,
                        input.speed,
                        step,
                        mount.turn_radius(),
                    );
                    (x, z, r - reverse)
                }
            } else {
                (
                    input.rotation.sin() * input.speed * step * input.forward as f32,
                    input.rotation.cos() * input.speed * step * input.forward as f32,
                    input.rotation,
                )
            };
            let floor = {
                let cache = self.passability_read();
                pathfinding::start_floor_at(&cache, from.x, from.z, from.y)
            };
            let mut next = Position {
                x: wrap_world_x(from.x + dx),
                y: from.y,
                z: from.z + dz,
            };
            next.y = if player.floor_level < 0 {
                self.goal_ground_y(floor, next).await.unwrap_or(from.y)
            } else {
                self.surface_ground_y(floor, &next, from.y, mount).await
            };
            let blocked = {
                let cache = self.passability_read();
                swept_blocked(&cache, from, dx, dz, floor)
                    || (next.y - from.y > dx.hypot(dz) * 50_f32.to_radians().tan() + 0.01
                        && !pathfinding::in_stairwell_span(&cache, from.x, from.z, from.y)
                        && pathfinding::storey_ground_y(&cache, floor, from.x, from.z, from.y)
                            .is_none())
            };
            if blocked {
                input.speed = 0.0;
                return (points, true);
            }
            player.position = next;
            player.rotation = rotation;
            if let Some(entrance) = self.dungeon_defs.entrance_at(next.x, next.z) {
                if let Some(runtime) = self.dungeons.read().await.get(&entrance.id) {
                    player.floor_level = dungeon_floor_after_step(
                        player.floor_level,
                        &entrance.position(),
                        &runtime.layouts,
                        next,
                    );
                }
            } else if player.floor_level >= 0 {
                player.floor_level = pathfinding::get_floor_at_position(
                    &self.passability_read(),
                    next.x,
                    next.z,
                    next.y,
                ) as i8;
            }
            points.push(MoveWaypoint {
                position: next,
                floor_level: player.floor_level,
                rotation: Some(rotation),
                travel_seconds: Some(step),
            });
        }
        (points, false)
    }
}
