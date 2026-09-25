use super::super::tests::{make_player, make_test_game_state};
use super::*;
use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};

fn corridor(closed: bool) -> RuntimePassability {
    let mut cells = vec![15; 24];
    for x in 1..7 {
        cells[8 + x] = 1 | 4;
    }
    cells[9] |= 8;
    cells[14] |= 2;
    if closed {
        cells[11] |= 2;
        cells[12] |= 8;
    }
    RuntimePassability {
        house_origin_x: 0.0,
        house_origin_z: 0.0,
        min_x: 0.0,
        max_x: 8.0,
        min_z: 0.0,
        max_z: 3.0,
        floors: vec![RuntimeFloorGrid {
            floor_level: 0,
            origin_x: 0,
            origin_z: 0,
            width: 8,
            depth: 3,
            y_base: 0.0,
            wall_height: 3.0,
            cells,
        }],
        stairwells: vec![],
        yields_to_trapped_mover: false,
        allows_projectiles: false,
        is_ground: true,
    }
}

async fn walker(
    name: &str,
    closed: bool,
) -> (
    GameState,
    PlayerId,
    tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>,
) {
    let game = make_test_game_state(name);
    game.passability_write()
        .insert("corridor".into(), corridor(closed));
    let player = make_player(name, 1.5, 1.5);
    let id = player.id;
    game.add_player(player).await;
    let rx = game.register_connection_channel(&id).await;
    (game, id, rx)
}

async fn next_path(rx: &mut tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>) -> ServerMessage {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let bytes = rx.recv().await.expect("open channel");
            let message = onlinerpg_shared::deserialize_server_msg(&bytes).expect("decode");
            if matches!(message, ServerMessage::PlayerMovePath { .. }) {
                return message;
            }
            if let ServerMessage::PlayerMoveProgress { status, .. } = message {
                assert_eq!(status, MoveStatus::Searching, "unexpected refusal");
            }
        }
    })
    .await
    .expect("path response without a movement tick")
}

async fn advance(game: &GameState, id: PlayerId, seconds: f32) {
    if let Some(plan) = game
        .goal_moves
        .lock()
        .await
        .get_mut(&id)
        .and_then(|s| s.plan.as_mut())
    {
        plan.advanced_at -= Duration::from_secs_f32(seconds);
    }
    game.tick_goal_movement().await;
}

async fn search_finished(game: &GameState, id: PlayerId) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while game
            .goal_moves
            .lock()
            .await
            .get(&id)
            .is_some_and(|s| s.running)
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("search finishes");
}

#[tokio::test]
async fn goal_is_approved_before_tick_and_uses_only_time_since_approval() {
    for (sprinting, expected_speed) in [(false, 3.0), (true, 4.5)] {
        let (game, id, mut rx) = walker(&format!("goal_clock_{sprinting}"), false).await;
        game.request_move_goal(id, 1, 6.5, 1.5, sprinting).await;
        let reply = next_path(&mut rx).await;
        assert!(matches!(
            reply,
            ServerMessage::PlayerMovePath {
                request_id: 1,
                termination: PathTermination::Reached,
                speed,
                ..
            } if speed == expected_speed
        ));
        assert_eq!(game.players.read().await[&id].position.x, 1.5);
        game.tick_player_movement(60.0).await;
        assert!(game.players.read().await[&id].position.x < 1.6);
        advance(&game, id, 0.5).await;
        let distance = game.players.read().await[&id].position.x - 1.5;
        assert!(
            (distance - expected_speed * 0.5).abs() < 0.1,
            "sprinting={sprinting}, actual elapsed distance: {distance}"
        );
    }
}

#[tokio::test]
async fn progress_delivery_releases_world_locks_but_preserves_goal_ordering() {
    let (game, id, mut rx) = walker("goal_delivery_locks", false).await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    next_path(&mut rx).await;
    search_finished(&game, id).await;
    let channels = game.direct_channels.write().await;
    let tick = advance(&game, id, 0.5);
    tokio::pin!(tick);
    assert!(futures_util::poll!(tick.as_mut()).is_pending());
    assert!(game.players.try_write().is_ok());
    assert!(game.dungeons.try_write().is_ok());
    assert!(game.goal_moves.try_lock().is_err());
    drop(channels);
    tick.await;
}

#[tokio::test]
async fn a_door_closed_after_approval_stops_the_body_before_the_edge() {
    let (game, id, mut rx) = walker("goal_door", false).await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    next_path(&mut rx).await;
    game.passability_write()
        .insert("corridor".into(), corridor(true));
    advance(&game, id, 3.0).await;
    let stopped = game.players.read().await[&id].position;
    assert!(stopped.x > 3.69 && stopped.x <= 3.701, "{stopped:?}");
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    game.passability_write()
        .insert("corridor".into(), corridor(false));
    advance(&game, id, 30.0).await;
    assert_eq!(game.players.read().await[&id].position, stopped);
}

#[tokio::test]
async fn unreachable_goal_approves_a_partial_route_without_claiming_arrival() {
    let (game, id, mut rx) = walker("goal_partial", true).await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    let reply = next_path(&mut rx).await;
    let ServerMessage::PlayerMovePath {
        termination,
        waypoints,
        ..
    } = reply
    else {
        unreachable!()
    };
    assert_eq!(termination, PathTermination::Unreachable);
    assert!(waypoints.last().unwrap().position.x < 4.0);
    advance(&game, id, 10.0).await;
    let mut terminal = None;
    while let Ok(bytes) = rx.try_recv() {
        if let ServerMessage::PlayerMoveProgress { status, .. } =
            onlinerpg_shared::deserialize_server_msg(&bytes).unwrap()
        {
            terminal = Some(status);
        }
    }
    assert_eq!(terminal, Some(MoveStatus::Partial));
}

#[tokio::test]
async fn latest_goal_wins_and_stop_invalidates_an_inflight_search() {
    let (game, id, mut rx) = walker("goal_replace", false).await;
    let workers = game.path_search.reserve_workers().await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    while game.goal_moves.lock().await[&id].pending.is_some() {
        tokio::task::yield_now().await;
    }
    for request in 2..=20 {
        game.request_move_goal(id, request, 5.5, 1.5, false).await;
    }
    assert_eq!(
        game.goal_moves.lock().await[&id]
            .pending
            .unwrap()
            .request_id,
        20
    );
    drop(workers);
    assert!(matches!(
        next_path(&mut rx).await,
        ServerMessage::PlayerMovePath { request_id: 20, .. }
    ));
    search_finished(&game, id).await;
    let workers = game.path_search.reserve_workers().await;
    game.request_move_goal(id, 21, 6.5, 1.5, false).await;
    game.stop_move_goal(id, 22).await;
    drop(workers);
    search_finished(&game, id).await;
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    assert!(game.goal_moves.lock().await[&id].pending.is_none());
    while let Ok(bytes) = rx.try_recv() {
        assert!(!matches!(
            onlinerpg_shared::deserialize_server_msg(&bytes).unwrap(),
            ServerMessage::PlayerMovePath { request_id: 21, .. }
        ));
    }
}

#[tokio::test]
async fn invalid_goal_preserves_a_valid_plan_and_direction_takes_over() {
    let (game, id, mut rx) = walker("goal_legacy", false).await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    next_path(&mut rx).await;
    game.request_move_goal(id, 2, 1000.0, 1.5, false).await;
    assert_eq!(
        game.goal_moves.lock().await[&id]
            .plan
            .as_ref()
            .unwrap()
            .goal
            .request_id,
        1
    );
    game.request_move_direction(id, 3, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    assert!(game.goal_moves.lock().await[&id].direction.is_some());
}

#[test]
fn client_coordinates_and_floor_reports_are_not_in_the_protocol() {
    for message in [
        r#"{"PlayerMove":{"position":{"x":1,"y":99,"z":1},"rotation":0,"floor_level":1}}"#,
        r#"{"PlayerFloorChanged":{"floor_level":1}}"#,
        r#"{"PlayerMovementSample":{"position":{"x":1,"y":99,"z":1},"rotation":0,"floor_level":1}}"#,
    ] {
        assert!(serde_json::from_str::<onlinerpg_shared::ClientMessage>(message).is_err());
    }
}

#[tokio::test]
async fn xz_stair_goals_infer_height_and_floor_in_both_directions() {
    let game = make_test_game_state("goal_stairs");
    let dungeon_id = "skeleton_crypt";
    game.ensure_dungeon_runtime(dungeon_id).await;
    let entrance = game.dungeon_defs.get(dungeon_id).unwrap().position();
    let layouts = game.dungeons.read().await[dungeon_id].layouts.clone();
    game.passability_write().insert(
        dungeon::dungeon_cache_key(dungeon_id),
        dungeon::dungeon_passability(&entrance, &layouts),
    );
    let shaft = layouts[0].down_shaft.as_ref().unwrap();
    let (ox, oz) = dungeon::dungeon_origin(entrance.x, entrance.z);
    let on_shaft = |run: f32| {
        let run = if shaft.reversed {
            dungeon::SHAFT_LEN as f32 - run
        } else {
            run
        };
        let (x, z) = if shaft.along_z {
            (ox + shaft.x as f32 + 1.5, oz + shaft.z as f32 + run)
        } else {
            (ox + shaft.x as f32 + run, oz + shaft.z as f32 + 1.5)
        };
        Position {
            x,
            y: dungeon::floor_height_at(&entrance, &layouts, 1, x, z).unwrap(),
            z,
        }
    };
    let top = on_shaft(0.5);
    let bottom = on_shaft(dungeon::SHAFT_LEN as f32 - 0.5);
    let mut player = make_player("stair_goal_walker", top.x, top.z);
    player.position = top;
    player.floor_level = -1;
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal(id, 1, bottom.x, bottom.z, false)
        .await;
    next_path(&mut rx).await;
    advance(&game, id, 10.0).await;
    let down = game.players.read().await[&id].clone();
    assert_eq!(down.floor_level, -2, "{down:?}");
    assert!(
        (down.position.y - bottom.y).abs() < 0.01,
        "{down:?}, {bottom:?}"
    );
    while rx.try_recv().is_ok() {}
    game.request_move_goal(id, 2, top.x, top.z, false).await;
    next_path(&mut rx).await;
    advance(&game, id, 10.0).await;
    let up = game.players.read().await[&id].clone();
    assert_eq!(up.floor_level, -1, "{up:?}");
    assert!((up.position.y - top.y).abs() < 0.01, "{up:?}, {top:?}");
}

#[tokio::test]
async fn map_edits_and_teleports_discard_search_results() {
    let (game, id, mut rx) = walker("goal_stale", false).await;
    let workers = game.path_search.reserve_workers().await;
    let snapshot = game.passability.snapshot();
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    while game.goal_moves.lock().await[&id].pending.is_some() {
        tokio::task::yield_now().await;
    }
    while Arc::strong_count(&snapshot) < 3 {
        tokio::task::yield_now().await;
    }
    game.passability_write()
        .insert("corridor".into(), corridor(true));
    drop(workers);
    search_finished(&game, id).await;
    let mut reason = None;
    while let Ok(bytes) = rx.try_recv() {
        if let ServerMessage::PlayerMoveProgress { status, .. } =
            onlinerpg_shared::deserialize_server_msg(&bytes).unwrap()
        {
            reason = Some(status);
        }
    }
    assert_eq!(reason, Some(MoveStatus::MapChanged));
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    game.request_move_goal(id, 2, 2.5, 1.5, false).await;
    game.teleport_player(
        &id,
        Position {
            x: 20.0,
            y: 0.0,
            z: 20.0,
        },
        0.0,
        0,
    )
    .await;
    search_finished(&game, id).await;
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    assert_eq!(game.players.read().await[&id].position.x, 20.0);
}

#[tokio::test]
async fn goal_crosses_the_world_seam_using_a_short_path() {
    let game = make_test_game_state("goal_seam");
    let mut player = make_player("goal_seam", onlinerpg_shared::WORLD_MAX_X - 1.5, 0.5);
    player.position.y = 5.0;
    let id = player.id;
    game.add_player(player).await;
    let goal_x = onlinerpg_shared::WORLD_MIN_X + 2.5;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal(id, 1, goal_x, 0.5, false).await;
    let ServerMessage::PlayerMovePath {
        waypoints,
        termination,
        ..
    } = next_path(&mut rx).await
    else {
        unreachable!()
    };
    assert_eq!(termination, PathTermination::Reached);
    assert!(waypoints.len() <= 5);
    advance(&game, id, 2.0).await;
    assert!((game.players.read().await[&id].position.x - goal_x).abs() < 0.01);
}

#[test]
fn request_order_accepts_wraparound_but_rejects_duplicates_and_old_packets() {
    let mut state = GoalMovement::new(u32::MAX);
    assert!(state.accept(u32::MAX));
    assert!(state.accept(0));
    assert!(!state.accept(u32::MAX));
    assert!(!state.accept(0));
}

impl GameState {
    pub(in crate::game_state) async fn request_test_move(
        &self,
        id: &PlayerId,
        command: super::super::tests::TestMove,
        _npc: bool,
    ) {
        let player = self.players.read().await[id].clone();
        if player.floor_level == 0 && player.position.y == 0.0 {
            if let Some(y) = self.goal_ground_y(0, player.position).await {
                self.players.write().await.get_mut(id).unwrap().position.y = y;
            }
        }
        let request_id = {
            let mut states = self.goal_moves.lock().await;
            if let Some(state) = states.get_mut(id) {
                state.next_search = Instant::now();
                state.last_id.wrapping_add(1)
            } else {
                1
            }
        };
        self.request_move_goal(
            *id,
            request_id,
            command.position.x,
            command.position.z,
            command.sprinting,
        )
        .await;
        search_finished(self, *id).await;
    }

    pub(in crate::game_state) async fn advance_test_movement(&self, seconds: f32) {
        let ids: Vec<_> = self.goal_moves.lock().await.keys().copied().collect();
        for id in &ids {
            search_finished(self, *id).await;
        }
        for state in self.goal_moves.lock().await.values_mut() {
            if let Some(plan) = state.plan.as_mut() {
                plan.advanced_at = Instant::now() - Duration::from_secs_f32(seconds);
            }
        }
        self.tick_player_movement(seconds).await;
    }
}

impl GameState {
    pub(in crate::game_state) async fn has_test_movement(&self, id: &PlayerId) -> bool {
        self.goal_moves
            .lock()
            .await
            .get(id)
            .is_some_and(|s| s.plan.is_some() || s.direction.is_some())
    }
}

async fn advance_direction(game: &GameState, id: PlayerId, seconds: f32) {
    {
        let mut states = game.goal_moves.lock().await;
        let direction = states.get_mut(&id).unwrap().direction.as_mut().unwrap();
        direction.advanced_at -= Duration::from_secs_f32(seconds);
        direction.expires_at = Instant::now() + Duration::from_millis(500);
    }
    game.advance_goal_players(&[id]).await;
}

struct PausedHeightTiles(Arc<tokio::sync::RwLock<()>>);

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for PausedHeightTiles {
    async fn read_heightmap(&self, _tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        let _guard = self.0.read().await;
        Ok(onlinerpg_terrain::height::encode_height(5.0)
            .to_le_bytes()
            .repeat(onlinerpg_terrain::defaults::VERTS_PER_SIDE.pow(2)))
    }
}

async fn paused_direction_walker(
    name: &str,
) -> (
    GameState,
    PlayerId,
    tokio::sync::mpsc::UnboundedReceiver<bytes::Bytes>,
    tokio::sync::OwnedRwLockWriteGuard<()>,
) {
    let mut game = make_test_game_state(name);
    let mut player = make_player(name, 100.0, 10.0);
    player.position.y = 5.0;
    player.health = 5;
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_direction(id, 1, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    next_path(&mut rx).await;
    let sampling = Arc::new(tokio::sync::RwLock::new(()));
    game.height_sampler = Arc::new(onlinerpg_terrain::height::HeightSampler::new(
        PausedHeightTiles(sampling.clone()),
    ));
    (game, id, rx, sampling.write_owned().await)
}

#[tokio::test]
async fn direction_preserves_food_healing_during_simulation() {
    let (game, id, mut rx, sampling) = paused_direction_walker("direction_healing").await;
    game.start_food_regeneration(&id, 20).await;
    let tick = tokio::task::unconstrained(advance_direction(&game, id, 0.1));
    tokio::pin!(tick);
    assert!(futures_util::poll!(tick.as_mut()).is_pending());
    game.tick_food_regeneration().await;
    assert_eq!(game.players.read().await[&id].health, 7);
    drop(sampling);
    tick.await;
    let player = game.players.read().await[&id].clone();
    assert_eq!(player.health, 7);
    assert!(player.position.x > 100.0);
    assert!((player.rotation - std::f32::consts::FRAC_PI_2).abs() < 0.01);
    let ServerMessage::PlayerMovePath { position, .. } = next_path(&mut rx).await else {
        unreachable!()
    };
    assert_eq!(position, player.position);
}

#[tokio::test]
async fn direction_preserves_damage_and_stops_if_killed_during_simulation() {
    for health in [3, 0] {
        let (game, id, mut rx, sampling) =
            paused_direction_walker(&format!("direction_damage_{health}")).await;
        let before = game.players.read().await[&id].clone();
        let tick = tokio::task::unconstrained(advance_direction(&game, id, 0.1));
        tokio::pin!(tick);
        assert!(futures_util::poll!(tick.as_mut()).is_pending());
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&id).unwrap();
            player.health = health;
            player.last_combat_at = 123;
            player.torch_on = true;
        }
        drop(sampling);
        tick.await;
        let player = game.players.read().await[&id].clone();
        assert_eq!(player.health, health);
        assert_eq!(player.last_combat_at, 123);
        assert!(player.torch_on);
        if health == 0 {
            assert_eq!(player.position, before.position);
            assert_eq!(player.rotation, before.rotation);
            assert_eq!(player.floor_level, before.floor_level);
            assert!(game.goal_moves.lock().await[&id].direction.is_none());
            assert!(matches!(
                onlinerpg_shared::deserialize_server_msg(&rx.try_recv().unwrap()).unwrap(),
                ServerMessage::PlayerMoveProgress {
                    request_id: 1,
                    status: MoveStatus::Stopped,
                    position,
                    ..
                } if position == before.position
            ));
        } else {
            assert!(player.position.x > before.position.x);
            assert!(game.goal_moves.lock().await[&id].direction.is_some());
        }
    }
}

#[tokio::test]
async fn direction_does_not_restore_a_player_removed_during_simulation() {
    let (game, id, _, sampling) = paused_direction_walker("direction_removed").await;
    let tick = tokio::task::unconstrained(advance_direction(&game, id, 0.1));
    tokio::pin!(tick);
    assert!(futures_util::poll!(tick.as_mut()).is_pending());
    game.players.write().await.remove(&id);
    drop(sampling);
    tick.await;
    assert!(!game.players.read().await.contains_key(&id));
    assert!(game.goal_moves.lock().await[&id].direction.is_none());
}

#[tokio::test]
async fn direction_is_server_driven_accelerates_and_stops_without_a_client_position() {
    let (game, id, mut rx) = walker("direction_acceleration", false).await;
    game.request_move_direction(id, 1, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    next_path(&mut rx).await;
    assert_eq!(game.players.read().await[&id].position.x, 1.5);
    advance_direction(&game, id, 0.1).await;
    let first = game.players.read().await[&id].position.x;
    assert!(first > 1.5 && first < 1.7, "{first}");
    game.request_move_direction(id, 2, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    advance_direction(&game, id, 0.1).await;
    let second = game.players.read().await[&id].position.x;
    assert!(second - first > first - 1.5);
    game.stop_move_goal(id, 3).await;
    let stopped = game.players.read().await[&id].position;
    game.tick_player_movement(60.0).await;
    assert_eq!(game.players.read().await[&id].position, stopped);
    game.request_move_direction(id, 2, 0.0, 1, 0, false).await;
    assert!(game.goal_moves.lock().await[&id].direction.is_none());
}

#[tokio::test]
async fn direction_lease_and_collision_stop_without_automatic_restart() {
    let (game, id, _) = walker("direction_blocked", true).await;
    game.request_move_direction(id, 1, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    advance_direction(&game, id, 2.0).await;
    assert!(game.players.read().await[&id].position.x < 3.71);
    assert!(game.goal_moves.lock().await[&id].direction.is_none());
    game.passability_write()
        .insert("corridor".into(), corridor(false));
    game.request_move_direction(id, 1, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    assert!(game.goal_moves.lock().await[&id].direction.is_none());
    game.request_move_direction(id, 2, std::f32::consts::FRAC_PI_2, 1, 0, false)
        .await;
    {
        let mut states = game.goal_moves.lock().await;
        let direction = states.get_mut(&id).unwrap().direction.as_mut().unwrap();
        direction.advanced_at = Instant::now() - Duration::from_secs(2);
        direction.expires_at = direction.advanced_at + Duration::from_millis(500);
    }
    let before = game.players.read().await[&id].position.x;
    game.advance_goal_players(&[id]).await;
    assert!(game.players.read().await[&id].position.x - before <= 1.51);
    assert!(game.goal_moves.lock().await[&id].direction.is_none());
}

#[tokio::test]
async fn mounted_direction_turns_and_reverses_with_authoritative_facing() {
    use onlinerpg_shared::mount::MountKind;
    let game = make_test_game_state("mounted_direction");
    let mut player = make_player("mounted_direction", 100.0, 0.0);
    player.position.y = 5.0;
    player.mount = Some(MountKind::Horse);
    let id = player.id;
    game.add_player(player).await;
    game.request_move_direction(id, 1, 0.0, -1, 0, true).await;
    advance_direction(&game, id, 0.4).await;
    let reversed = game.players.read().await[&id].clone();
    assert!(
        reversed.position.z < 0.0 && reversed.position.z >= -0.61,
        "{:?}",
        reversed.position
    );
    assert!(reversed.rotation.abs() < 0.01);
    game.request_move_direction(id, 2, 0.0, 0, 1, false).await;
    let before = game.players.read().await[&id].position;
    advance_direction(&game, id, 0.2).await;
    let turned = game.players.read().await[&id].clone();
    assert_eq!(turned.position, before);
    assert!(turned.rotation < -0.4 && turned.rotation > -0.6);
}

#[tokio::test]
async fn a_door_cannot_close_on_a_body_and_settles_crossing_before_closing() {
    let game = make_test_game_state("goal_door_ordering");
    let dungeon_id = "skeleton_crypt";
    game.ensure_dungeon_runtime(dungeon_id).await;
    let door = dungeon::interior_doors(&game.dungeons.read().await[dungeon_id].layouts[0])
        .into_iter()
        .find(|door| !door.locked)
        .unwrap();
    let [ax, az, bx, bz] = game
        .dungeon_door_segment(dungeon_id, 1, door.door_id)
        .await
        .unwrap();
    let center = Position {
        x: (ax + bx) * 0.5,
        y: dungeon::floor_world_y(game.dungeon_defs.get(dungeon_id).unwrap().y, 1),
        z: (az + bz) * 0.5,
    };
    let mut player = make_player("door_body", center.x, center.z);
    player.position = center;
    player.floor_level = -1;
    let id = player.id;
    game.add_player(player).await;
    assert_eq!(
        game.toggle_dungeon_door(&id, dungeon_id, 1, door.door_id)
            .await,
        Some(true)
    );
    assert_eq!(
        game.toggle_dungeon_door(&id, dungeon_id, 1, door.door_id)
            .await,
        Some(true)
    );
    let (dx, dz) = if ax == bx { (1.0, 0.0) } else { (0.0, 1.0) };
    let before = Position {
        x: center.x - dx,
        z: center.z - dz,
        ..center
    };
    game.teleport_player(&id, before, 0.0, -1).await;
    let mut mover = make_player("door_crossing", before.x, before.z);
    mover.position = before;
    mover.floor_level = -1;
    let mover_id = mover.id;
    game.add_player(mover).await;
    let mut rx = game.register_connection_channel(&mover_id).await;
    game.request_move_goal(mover_id, 1, center.x + dx, center.z + dz, false)
        .await;
    next_path(&mut rx).await;
    game.goal_moves
        .lock()
        .await
        .get_mut(&mover_id)
        .unwrap()
        .plan
        .as_mut()
        .unwrap()
        .advanced_at -= Duration::from_secs(1);
    assert_eq!(
        game.toggle_dungeon_door(&id, dungeon_id, 1, door.door_id)
            .await,
        Some(false)
    );
    let after = game.players.read().await[&mover_id].position;
    assert!(
        (after.x - center.x) * dx + (after.z - center.z) * dz > 0.31,
        "{after:?}"
    );
    assert!(game.goal_moves.lock().await[&mover_id].plan.is_none());
}

fn two_storey_house() -> RuntimePassability {
    let grid = |floor_level, y_base, cells| RuntimeFloorGrid {
        floor_level,
        origin_x: 0,
        origin_z: 0,
        width: 3,
        depth: 4,
        y_base,
        wall_height: 3.0,
        cells,
    };
    let mut ground = vec![0u8; 12];
    ground[2 + 2 * 3] = 4;
    ground[2 + 3 * 3] = 1;
    RuntimePassability {
        house_origin_x: 10.0,
        house_origin_z: 10.0,
        min_x: 10.0,
        max_x: 13.0,
        min_z: 10.0,
        max_z: 14.0,
        floors: vec![grid(0, 0.0, ground), grid(1, 3.1, vec![0u8; 12])],
        stairwells: vec![onlinerpg_shared::pathfinding::StairwellInfo {
            local_min_x: 0,
            local_min_z: 0,
            local_max_x: 1,
            local_max_z: 4,
            lower_floor: 0,
            upper_floor: 1,
            along_z: true,
            reversed: false,
        }],
        yields_to_trapped_mover: false,
        allows_projectiles: false,
        is_ground: true,
    }
}

#[tokio::test]
async fn house_stairs_change_floor_only_as_the_approved_route_advances() {
    let game = make_test_game_state("goal_house_stairs");
    game.passability_write()
        .insert("stairs".into(), two_storey_house());
    let player = make_player("climber", 10.5, 10.5);
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal(id, 1, 10.5, 13.5, false).await;
    next_path(&mut rx).await;
    assert_eq!(game.players.read().await[&id].floor_level, 0);
    advance(&game, id, 2.0).await;
    let top = game.players.read().await[&id].clone();
    assert_eq!(top.floor_level, 1, "{:?}", top.position);
    assert!((top.position.y - 3.1).abs() < 0.01);
    while rx.try_recv().is_ok() {}
    game.request_move_goal(id, 2, 10.5, 10.5, false).await;
    next_path(&mut rx).await;
    advance(&game, id, 2.0).await;
    assert_eq!(game.players.read().await[&id].floor_level, 0);
}

#[tokio::test]
async fn terrain_edits_reheight_the_current_pose_and_remaining_route() {
    let game = make_test_game_state("goal_terrain_edit");
    let mut player = make_player("landscaper", 100.0, 10.0);
    player.position.y = 5.0;
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal(id, 1, 110.0, 10.0, false).await;
    next_path(&mut rx).await;
    let raw = onlinerpg_terrain::height::encode_height(7.0)
        .to_le_bytes()
        .repeat(onlinerpg_terrain::defaults::VERTS_PER_SIDE.pow(2));
    game.height_sampler.update_tile(2, 0, &raw).await.unwrap();
    advance(&game, id, 0.2).await;
    let ServerMessage::PlayerMovePath {
        position,
        waypoints,
        ..
    } = next_path(&mut rx).await
    else {
        unreachable!()
    };
    assert!((position.y - 7.0).abs() < 0.01, "{position:?}");
    assert!(waypoints.iter().all(|p| (p.position.y - 7.0).abs() < 0.01));
    let current = game.players.read().await[&id].position;
    assert!(
        (current.y - 7.0).abs() < 0.01 && current.x > 100.0,
        "{current:?}"
    );
}

#[tokio::test]
async fn mounted_goal_preserves_turn_time_and_honors_stop() {
    let game = make_test_game_state("goal_mounted_turn");
    let mut player = make_player("rider", 100.5, 10.5);
    player.position.y = 5.0;
    player.mount = Some(onlinerpg_shared::mount::MountKind::Horse);
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal(id, 1, 100.5, 1.5, false).await;
    let ServerMessage::PlayerMovePath { waypoints, .. } = next_path(&mut rx).await else {
        unreachable!()
    };
    assert!(waypoints
        .iter()
        .all(|point| point.rotation.is_some() && point.travel_seconds.is_some()));
    advance(&game, id, 0.1).await;
    let current = game.players.read().await[&id].clone();
    assert!(current.rotation.abs() < 0.5, "{}", current.rotation);
    assert!(
        current.position.dist_xz_sq(&Position {
            x: 100.5,
            y: 5.0,
            z: 10.5
        }) < 0.9
    );
    game.stop_move_goal(id, 2).await;
    let stopped = game.players.read().await[&id].position;
    advance(&game, id, 3.0).await;
    assert_eq!(game.players.read().await[&id].position, stopped);
}

#[tokio::test]
async fn a_house_click_can_stop_inside_the_entrance_without_truncating_interaction_goals() {
    use onlinerpg_shared::housing::{HouseData, RoomData};
    let game = make_test_game_state("goal_house_entry");
    let room: RoomData = serde_json::from_value(serde_json::json!({
        "localX":0,"localZ":0,"sizeX":8,"sizeZ":6,"floorLevel":0,
        "floorTexture":0,"roofTexture":0,"wallHeight":3,
        "wallNorth":[],"wallSouth":[],"wallEast":[],"wallWest":[]
    }))
    .unwrap();
    let house = HouseData {
        id: "r0_0_1".into(),
        owner_id: String::new(),
        source_scroll_id: None,
        origin: Position {
            x: 10.0,
            y: 5.0,
            z: 10.0,
        },
        rooms: vec![room],
        passability: vec![onlinerpg_shared::housing::PassabilityGrid {
            floor_level: 0,
            origin_x: 0,
            origin_z: 0,
            width: 8,
            depth: 6,
            cells: vec![0; 48],
        }],
    };
    game.housing_io.write_house(&house).await.unwrap();
    game.passability_add_house(&house).await;
    let mut player = make_player("visitor", 8.5, 12.5);
    player.position.y = 5.0;
    let id = player.id;
    game.add_player(player).await;
    let mut rx = game.register_connection_channel(&id).await;
    game.request_move_goal_with_entrance(id, 1, 16.5, 12.5, false, true)
        .await;
    next_path(&mut rx).await;
    advance(&game, id, 5.0).await;
    let entered = game.players.read().await[&id].position;
    assert!((10.5..11.0).contains(&entered.x), "{entered:?}");
    while rx.try_recv().is_ok() {}
    game.request_move_goal(id, 2, 16.5, 12.5, false).await;
    next_path(&mut rx).await;
    advance(&game, id, 5.0).await;
    assert!((game.players.read().await[&id].position.x - 16.5).abs() < 0.01);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "5000-player movement benchmark; run explicitly in release mode"]
async fn movement_load_5000() {
    let game = make_test_game_state("goal_load_5000");
    let mut ids = Vec::new();
    let mut channels = Vec::new();
    for index in 0..5000 {
        let mut player = make_player(
            &format!("load_{index}"),
            1000.5 + (index % 100) as f32 * 32.0,
            1000.5 + (index / 100) as f32 * 32.0,
        );
        player.position.y = 5.0;
        ids.push(player.id);
        game.player_spatial_cells
            .write()
            .await
            .insert(player.id, &player.position);
        channels.push(game.register_connection_channel(&player.id).await);
        game.players.write().await.insert(player.id, player);
    }
    let started = std::time::Instant::now();
    let mut tasks = tokio::task::JoinSet::new();
    for &id in &ids {
        let game = game.clone();
        tasks.spawn(async move {
            let p = game.players.read().await[&id].position;
            game.request_move_goal(id, 1, p.x + 12.0, p.z, false).await;
            search_finished(&game, id).await;
        });
        if tasks.len() >= 64 {
            tasks.join_next().await.unwrap().unwrap();
        }
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    let approval_ms = started.elapsed().as_secs_f64() * 1000.0;
    let approved = game
        .goal_moves
        .lock()
        .await
        .values()
        .filter(|state| state.plan.is_some())
        .count();
    assert_eq!(approved, 5000);
    for channel in &mut channels {
        while channel.try_recv().is_ok() {}
    }
    let mut elapsed = Vec::new();
    let mut packets = 0usize;
    let mut bytes = 0usize;
    for _ in 0..10 {
        let now = Instant::now();
        for state in game.goal_moves.lock().await.values_mut() {
            if let Some(plan) = &mut state.plan {
                plan.advanced_at = now - Duration::from_millis(200);
            }
        }
        let tick = std::time::Instant::now();
        game.tick_goal_movement().await;
        elapsed.push(tick.elapsed().as_secs_f64() * 1000.0);
        for channel in &mut channels {
            while let Ok(packet) = channel.try_recv() {
                packets += 1;
                bytes += packet.len();
            }
        }
    }
    elapsed.sort_by(f64::total_cmp);
    eprintln!("movement_load_5000 approved={approved} approval_ms={approval_ms:.2} tick_p50_ms={:.2} tick_max_ms={:.2} packets={packets} bytes={bytes}",elapsed[5],elapsed[9]);
}
