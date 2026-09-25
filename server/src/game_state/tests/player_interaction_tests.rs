use super::*;
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};

fn room(floor_level: u8, y_base: f32, cells: Vec<u8>) -> RuntimePassability {
    RuntimePassability {
        house_origin_x: 98.0,
        house_origin_z: 49.0,
        min_x: 98.0,
        max_x: 104.0,
        min_z: 49.0,
        max_z: 53.0,
        floors: vec![RuntimeFloorGrid {
            floor_level,
            origin_x: 0,
            origin_z: 0,
            width: 6,
            depth: 4,
            y_base,
            wall_height: 3.0,
            cells,
        }],
        stairwells: vec![],
        yields_to_trapped_mover: false,
        allows_projectiles: false,
        is_ground: true,
    }
}

fn chair() -> FurniturePlacement {
    FurniturePlacement {
        id: 7,
        type_id: "chair".into(),
        x: 100.5,
        y: 5.0,
        z: 50.5,
        rotation_deg: 0.0,
        floor_level: 0,
    }
}

async fn setup(name: &str, placements: &[FurniturePlacement]) -> (GameState, PlayerId, DirectRx) {
    let game = make_flat_world_game_state(name);
    game.sync_region_furniture(0, 0, placements);
    let mut player = make_player("sitter", 98.5, 50.5);
    player.position.y = 5.0;
    let id = player.id;
    game.add_player(player).await;
    let rx = game.register_direct_channel(&id).await;
    (game, id, rx)
}

#[tokio::test]
async fn interaction_approval_owns_position_rotation_and_exit_before_next_move() {
    let seat = FurniturePlacement {
        rotation_deg: 90.0,
        ..chair()
    };
    let (game, id, mut rx) = setup("interaction_position", std::slice::from_ref(&seat)).await;
    game.set_player_interaction(&id, Some("chair".into()), Some(seat.id))
        .await;
    let seated = game.players.read().await[&id].clone();
    assert_eq!(
        seated.position,
        Position {
            x: seat.x,
            y: 5.0,
            z: seat.z
        }
    );
    assert_eq!(seated.rotation, std::f32::consts::FRAC_PI_2);
    assert_eq!(seated.object_id, Some(seat.id));
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::PlayerInteractionChanged { position, rotation, floor_level: 0, object_id: Some(7), .. }
        if *position == seated.position && *rotation == seated.rotation
    )));
    game.set_player_interaction(&id, None, None).await;
    let standing = game.players.read().await[&id].clone();
    assert_ne!(standing.position, seated.position);
    assert_eq!(standing.position.y, 5.0);
    assert!(standing.object_id.is_none());
    assert!(!onlinerpg_shared::pathfinding::is_circle_blocked_on_floor(
        &game.passability_read(),
        standing.position.x,
        standing.position.z,
        0.31,
        0,
        Some(5.0)
    ));
    game.request_move_direction(id, 1, 0.0, 1, 0, false).await;
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::PlayerMovePath { position, .. } if *position == standing.position
    )));
}

#[tokio::test]
async fn interaction_rejects_missing_distant_wrong_floor_and_dead_requests() {
    for case in ["missing", "distance", "floor", "dead", "mounted"] {
        let (game, id, mut rx) = setup(case, &[chair()]).await;
        {
            let mut players = game.players.write().await;
            let player = players.get_mut(&id).unwrap();
            match case {
                "distance" => player.position.x -= 20.0,
                "floor" => player.floor_level = 1,
                "dead" => player.health = 0,
                "mounted" => player.mount = Some(onlinerpg_shared::mount::MountKind::Horse),
                _ => {}
            }
        }
        let before = game.players.read().await[&id].position;
        game.set_player_interaction(
            &id,
            Some("chair".into()),
            Some(if case == "missing" { 8 } else { 7 }),
        )
        .await;
        let player = game.players.read().await[&id].clone();
        assert_eq!(player.position, before, "{case}");
        assert!(player.object_type.is_none(), "{case}");
        assert!(
            drain(&mut rx)
                .iter()
                .any(|message| matches!(message, ServerMessage::InteractionRejected { .. })),
            "{case}"
        );
    }
}

#[tokio::test]
async fn interaction_cannot_cross_other_furniture_and_exit_skips_blocked_side() {
    let table = FurniturePlacement {
        id: 8,
        type_id: "table".into(),
        x: 102.0,
        ..chair()
    };
    let (game, id, mut rx) = setup("interaction_obstacle", &[chair(), table.clone()]).await;
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert_eq!(game.players.read().await[&id].object_id, Some(7));
    game.set_player_interaction(&id, None, None).await;
    assert!(game.players.read().await[&id].position.x < chair().x);
    let before = Position {
        x: 103.5,
        y: 5.0,
        z: 50.5,
    };
    game.players.write().await.get_mut(&id).unwrap().position = before;
    drain(&mut rx);
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert_eq!(game.players.read().await[&id].position, before);
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::InteractionRejected { .. })));
}

#[tokio::test]
async fn entering_furniture_cancels_an_approved_movement_and_emotes_do_not_step_out() {
    let (game, id, _) = setup("interaction_movement", &[chair()]).await;
    game.request_move_direction(id, 1, 0.0, 1, 0, false).await;
    assert!(game.has_test_movement(&id).await);
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert!(!game.has_test_movement(&id).await);
    game.set_player_interaction(&id, None, None).await;
    let before = game.players.read().await[&id].position;
    game.set_player_interaction(&id, Some("wave".into()), None)
        .await;
    game.set_player_interaction(&id, None, None).await;
    assert_eq!(game.players.read().await[&id].position, before);
}

#[tokio::test]
async fn furniture_interaction_cannot_cross_a_closed_wall_or_exit_a_sealed_room() {
    let (game, id, mut rx) = setup("interaction_walls", &[chair()]).await;
    let mut cells = vec![0; 24];
    for row in 0..4 {
        cells[row * 6 + 1] = 2;
        cells[row * 6 + 2] = 8;
    }
    game.passability_write()
        .insert("room".into(), room(0, 5.0, cells));
    let before = game.players.read().await[&id].position;
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert_eq!(game.players.read().await[&id].position, before);
    assert!(drain(&mut rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::InteractionRejected { .. })));
    game.passability_write()
        .insert("room".into(), room(0, 5.0, vec![0; 24]));
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    let seated = game.players.read().await[&id].position;
    game.passability_write()
        .insert("room".into(), room(0, 5.0, vec![15; 24]));
    game.set_player_interaction(&id, None, None).await;
    assert_eq!(game.players.read().await[&id].position, seated);
    assert!(game.players.read().await[&id].object_type.is_none());
}

#[tokio::test]
async fn upper_floor_furniture_uses_the_floor_surface_for_entry_and_exit() {
    let seat = FurniturePlacement {
        floor_level: 1,
        y: 8.1,
        ..chair()
    };
    let (game, id, _) = setup("interaction_upper_floor", &[seat]).await;
    game.passability_write()
        .insert("room".into(), room(1, 8.1, vec![0; 24]));
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&id).unwrap();
        player.position.y = 8.1;
        player.floor_level = 1;
    }
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert_eq!(game.players.read().await[&id].object_id, Some(7));
    let seated = game.players.read().await[&id].position;
    game.set_player_interaction(&id, None, None).await;
    let player = game.players.read().await[&id].clone();
    assert_ne!(player.position, seated);
    assert_eq!(player.position.y, 8.1);
    assert_eq!(player.floor_level, 1);
}

#[tokio::test]
async fn leaving_a_chair_to_play_an_instrument_keeps_the_new_performance() {
    let (game, id, _) = setup("interaction_instrument", &[chair()]).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "worn_mandolin", 1)],
            ..Default::default()
        },
    );
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    let seated = game.players.read().await[&id].position;
    game.start_live_instrument(&id).await;
    let player = game.players.read().await[&id].clone();
    assert_ne!(player.position, seated);
    assert_eq!(
        player.object_type.as_deref(),
        Some(onlinerpg_shared::messages::MUSIC_EMOTE)
    );
    assert!(game.live_instrument_players.read().await.contains(&id));
    game.set_player_interaction(&id, Some("chair".into()), Some(7))
        .await;
    assert!(!game.live_instrument_players.read().await.contains(&id));
    assert_eq!(game.players.read().await[&id].object_id, Some(7));
}
