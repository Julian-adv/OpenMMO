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
async fn invalid_goal_preserves_a_valid_plan_and_legacy_input_takes_over() {
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
    let command = super::super::MoveCommand {
        position: Position {
            x: 2.5,
            y: 0.0,
            z: 1.5,
        },
        rotation: 0.0,
        floor_level: 0,
        append: false,
        sprinting: false,
    };
    game.update_keyboard_movement(&id, command, 1).await;
    assert!(!game.owns_goal_movement(&id).await);
    assert!(game.goal_moves.lock().await[&id].plan.is_none());
    assert!(game.movement_intents.read().await.contains_key(&id));
}

#[tokio::test]
async fn client_floor_and_samples_cannot_override_an_approved_path() {
    let (game, id, mut rx) = walker("goal_no_client_pose", false).await;
    game.request_move_goal(id, 1, 6.5, 1.5, false).await;
    next_path(&mut rx).await;
    game.update_player_floor(&id, 1).await;
    game.record_movement_sample(
        &id,
        Position {
            x: 1000.0,
            y: 99.0,
            z: 1.5,
        },
        0.0,
        1,
    )
    .await;
    assert_eq!(game.players.read().await[&id].floor_level, 0);
    assert!(game.goal_moves.lock().await[&id].plan.is_some());
    assert!(!game.movement_resync_pending(&id));
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
