use super::*;
use onlinerpg_shared::dungeon::{dungeon_cache_key, dungeon_passability, floor_height_at};

const CRYPT: &str = "skeleton_crypt";

struct CryptTerrain;

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for CryptTerrain {
    async fn read_heightmap(&self, _tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        Ok(uniform_heightmap(0.95))
    }
}

async fn crypt_player(tag: &str, position: Position, floor: i8) -> (GameState, PlayerId) {
    let game = make_game_state_with(tag, CryptTerrain, SeaOnlyWater);
    game.ensure_dungeon_runtime(CRYPT).await;
    {
        let dungeons = game.dungeons.read().await;
        let entrance = game.dungeon_defs.get(CRYPT).unwrap().position();
        game.passability_write().insert(
            dungeon_cache_key(CRYPT),
            dungeon_passability(&entrance, &dungeons[CRYPT].layouts),
        );
    }
    let mut player = make_player(tag, position.x, position.z);
    player.position = position;
    player.floor_level = floor;
    let id = player.id;
    game.add_player(player).await;
    (game, id)
}

fn stair_position(y: f32) -> Position {
    Position {
        x: -1037.2192,
        y,
        z: 4262.4873,
    }
}

fn dungeon_move(x: f32, z: f32, floor: i8, append: bool) -> MoveCommand {
    MoveCommand {
        floor_level: floor,
        ..move_cmd(Position { x, y: 0.0, z }, append)
    }
}

async fn assert_on_stairs(game: &GameState, id: &PlayerId) {
    let position = game.players.read().await[id].position;
    let dungeons = game.dungeons.read().await;
    let entrance = game.dungeon_defs.get(CRYPT).unwrap().position();
    let y = floor_height_at(
        &entrance,
        &dungeons[CRYPT].layouts,
        2,
        position.x,
        position.z,
    )
    .unwrap();
    assert!((position.y - y).abs() < 0.001, "{position:?}, ground={y}");
}

#[tokio::test]
async fn dungeon_slide_uses_ground_at_actual_position() {
    for (tag, y, target_x, target_z) in [
        ("ramp_slide", -4.5294924, -1037.0, 4248.4),
        ("ramp_slide_east", -4.5294924, -1035.3, 4247.2),
        ("saved_slide", -7.0497413, -1037.0, 4248.4),
    ] {
        let (game, id) = crypt_player(tag, stair_position(y), -1).await;
        game.update_player_position(&id, dungeon_move(target_x, target_z, -2, false), false)
            .await;
        for _ in 0..5 {
            game.tick_player_movement(0.2).await;
            assert_on_stairs(&game, &id).await;
            assert_eq!(game.players.read().await[&id].position.z, 4262.4873);
        }
    }
}

#[tokio::test]
async fn stalled_dungeon_slide_resyncs_then_allows_stair_descent() {
    for (tag, y, target_x, target_z) in [
        ("ramp_stall", -4.5294924, -1037.0, 4248.4),
        ("ramp_stall_east", -4.5294924, -1035.3, 4247.2),
        ("saved_stall", -7.0497413, -1037.0, 4248.4),
    ] {
        let (game, id) = crypt_player(tag, stair_position(y), -1).await;
        let mut rx = game.register_direct_channel(&id).await;
        let command = dungeon_move(target_x, target_z, -2, false);
        game.update_player_position(&id, command, false).await;
        game.update_player_position(&id, dungeon_move(target_x, target_z, -2, true), false)
            .await;
        let version = game.player_movement_versions.read().await[&id];

        for _ in 0..10 {
            game.tick_player_movement(0.2).await;
        }
        let messages = drain(&mut rx);
        let resyncs: Vec<_> = messages
            .iter()
            .filter_map(|message| match message {
                ServerMessage::MovementResync {
                    resync_id,
                    position,
                    floor_level,
                    ..
                } => Some((*resync_id, *position, *floor_level)),
                _ => None,
            })
            .collect();
        assert_eq!(resyncs.len(), 1, "{tag}: stalled movement must resync");
        let (resync_id, corrected, floor) = resyncs[0];
        assert_eq!(floor, -1);
        assert_eq!(game.players.read().await[&id].position, corrected);
        assert_on_stairs(&game, &id).await;
        assert!(!game.movement_intents.read().await.contains_key(&id));
        assert_eq!(game.player_movement_versions.read().await[&id], version + 1);
        assert!(!game.stale_layout_grinds.read().await.contains_key(&id));

        game.update_player_position(&id, command, false).await;
        game.update_player_floor(&id, -2).await;
        game.tick_player_movement(0.2).await;
        assert_eq!(game.players.read().await[&id].position, corrected);
        assert_eq!(game.players.read().await[&id].floor_level, -1);
        assert!(!game.movement_intents.read().await.contains_key(&id));

        game.acknowledge_movement_resync(&id, resync_id);
        game.update_player_position(&id, dungeon_move(-1041.5, 4262.5, -2, false), false)
            .await;
        for _ in 0..20 {
            game.tick_player_movement(0.2).await;
        }
        let player = game.players.read().await[&id].clone();
        assert_eq!(player.floor_level, -2);
        assert_eq!((player.position.x, player.position.z), (-1041.5, 4262.5));
        assert!((player.position.y + 7.05).abs() < 0.001);
        assert!(!game.movement_intents.read().await.contains_key(&id));
        assert!(!drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::MovementResync { .. } | ServerMessage::PositionCorrected { .. }
        )));
    }
}

#[tokio::test]
async fn ordinary_dungeon_stair_travel_does_not_resync() {
    for (tag, x, y, floor, target_x, target_floor) in [
        ("stair_descent", -1034.5, -3.3833334, -1, -1041.5, -2),
        ("stair_ascent", -1041.5, -7.05, -2, -1034.5, -1),
        ("saved_descent", -1037.2192, -7.0497413, -1, -1041.5, -2),
    ] {
        let (game, id) = crypt_player(tag, Position { x, y, z: 4262.5 }, floor).await;
        let mut rx = game.register_direct_channel(&id).await;
        game.update_player_position(
            &id,
            dungeon_move(target_x, 4262.5, target_floor, false),
            false,
        )
        .await;
        for _ in 0..30 {
            game.tick_player_movement(0.2).await;
        }
        let player = game.players.read().await[&id].clone();
        assert_eq!(player.floor_level, target_floor, "{tag}");
        assert_eq!((player.position.x, player.position.z), (target_x, 4262.5));
        assert_on_stairs(&game, &id).await;
        assert!(!game.movement_intents.read().await.contains_key(&id));
        assert!(!drain(&mut rx).iter().any(|message| matches!(
            message,
            ServerMessage::MovementResync { .. } | ServerMessage::PositionCorrected { .. }
        )));
    }
}

#[tokio::test]
async fn replacing_a_dungeon_route_keeps_the_stair_connection() {
    let (game, id) = crypt_player("replace_stair_route", stair_position(-4.5294924), -1).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.update_player_position(&id, dungeon_move(-1041.5, 4262.5, -2, false), false)
        .await;
    game.update_player_position(&id, dungeon_move(-1041.5, 4261.5, -2, true), false)
        .await;
    game.update_player_position(&id, dungeon_move(-1036.5, 4258.5, -2, false), false)
        .await;
    {
        let queues = game.movement_intents.read().await;
        let queue = &queues[&id];
        assert!(queue.len() > 1, "replacement needs the stair landing");
        assert_eq!((queue[0].target.x, queue[0].target.z), (-1041.5, 4262.5));
        assert_eq!(queue.back().unwrap().target.x, -1036.5);
    }
    game.update_player_position(&id, dungeon_move(-1037.0, 4248.4, -2, true), false)
        .await;
    for _ in 0..100 {
        if !game.movement_intents.read().await.contains_key(&id) {
            break;
        }
        game.tick_player_movement(0.2).await;
        assert_eq!(game.movement_audit.last_tick(id).unwrap().outcome, "clear");
    }
    let player = game.players.read().await[&id].clone();
    assert_eq!(
        (player.position.x, player.position.z, player.floor_level),
        (-1037.0, 4248.4, -2)
    );
    assert!(!game.movement_intents.read().await.contains_key(&id));
    assert!(!drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::MovementResync { .. } | ServerMessage::PositionCorrected { .. }
    )));
}

#[tokio::test]
async fn recorded_entry_replacement_routes_through_the_entrance() {
    let start = Position {
        x: -1064.2316,
        y: 0.93712014,
        z: 4246.855,
    };
    let (game, id) = crypt_player("replace_entry_route", start, 0).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.update_player_position(&id, dungeon_move(-1064.5, 4247.5, 0, false), false)
        .await;
    game.update_player_position(&id, dungeon_move(-1064.5, 4249.5, 0, true), false)
        .await;
    let command = MoveCommand {
        position: Position {
            x: -1059.3263,
            y: -1.4991374,
            z: 4248.0586,
        },
        rotation: 1.4766377,
        floor_level: 0,
        append: false,
        sprinting: true,
    };
    game.update_player_position(&id, command, false).await;
    {
        let queues = game.movement_intents.read().await;
        let queue = &queues[&id];
        assert_eq!((queue[0].target.x, queue[0].target.z), (-1064.5, 4248.5));
    }
    game.update_player_floor(&id, -1).await;
    assert_eq!(game.players.read().await[&id].position, start);
    assert_eq!(game.movement_intents.read().await[&id][0].floor_level, 0);
    assert_eq!(
        game.movement_intents.read().await[&id]
            .back()
            .unwrap()
            .floor_level,
        -1
    );
    for _ in 0..30 {
        if !game.movement_intents.read().await.contains_key(&id) {
            break;
        }
        game.tick_player_movement(0.2).await;
        assert_eq!(game.movement_audit.last_tick(id).unwrap().outcome, "clear");
    }
    assert_eq!(
        player_xz(&game, &id).await,
        (command.position.x, command.position.z)
    );
    assert_eq!(game.players.read().await[&id].floor_level, -1);
    assert!(!game.movement_intents.read().await.contains_key(&id));
    assert!(!drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::MovementResync { .. } | ServerMessage::PositionCorrected { .. }
    )));
}

#[tokio::test]
async fn reversing_on_stairs_discards_the_old_descent() {
    let (game, id) = crypt_player("reverse_stair_route", stair_position(-4.5294924), -1).await;
    game.update_player_position(&id, dungeon_move(-1041.5, 4262.5, -2, false), false)
        .await;
    game.update_player_position(&id, dungeon_move(-1036.5, 4258.5, -2, true), false)
        .await;
    game.update_player_position(&id, dungeon_move(-1034.5, 4262.5, -1, false), false)
        .await;
    assert_eq!(game.movement_intents.read().await[&id].len(), 1);
    game.tick_player_movement(0.2).await;
    assert!(game.players.read().await[&id].position.x > -1037.2192);
    for _ in 0..20 {
        game.tick_player_movement(0.2).await;
    }
    assert_eq!(player_xz(&game, &id).await, (-1034.5, 4262.5));
    assert_eq!(game.players.read().await[&id].floor_level, -1);
}
