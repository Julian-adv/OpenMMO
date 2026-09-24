use super::*;

#[tokio::test]
async fn player_aoi_crosses_world_x_seam() {
    let game_state = make_test_game_state("player_aoi_x_wrap");
    let east_id = pid("east_player");
    let west_id = pid("west_player");

    game_state
        .add_player(make_player(
            "east_player",
            onlinerpg_shared::WORLD_MAX_X - 1.0,
            0.0,
        ))
        .await;
    game_state
        .add_player(make_player(
            "west_player",
            onlinerpg_shared::WORLD_MIN_X + 1.0,
            0.0,
        ))
        .await;

    let nearby = game_state
        .player_ids_within(&east_id, onlinerpg_shared::EVENT_DELIVERY_RADIUS)
        .await;
    assert!(nearby.contains(&east_id));
    assert!(nearby.contains(&west_id));
}

#[tokio::test]
async fn movement_into_aoi_sends_existing_monsters_and_ground_items() {
    let game_state = make_test_game_state("movement_world_entity_aoi");
    let player_id = pid("walker");
    let entity_position = Position {
        x: 50.0,
        y: 0.0,
        z: 0.0,
    };

    game_state.add_player(make_player("walker", 0.0, 0.0)).await;
    let mut direct_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "monster_a".to_string(),
            make_monster("monster_a", entity_position, 0),
        );
    }

    {
        let mut ground_items = game_state.ground_items.write().await;
        ground_items.insert(
            42,
            ServerGroundItem {
                item: GroundItem {
                    instance_id: 42,
                    item_def_id: "test_item".to_string(),
                    position: entity_position,
                    floor_level: 0,
                    quantity: 1,
                    enchant: 0,
                    dropped_by: None,
                    cape_color: None,
                    cape_texture: None,
                },
                dropped_at_ms: 0,
            },
        );
    }

    seed_subjects(&game_state).await;
    drain(&mut direct_rx);
    game_state
        .request_test_move(&player_id, move_cmd(entity_position, false), false)
        .await;
    game_state.advance_test_movement(60.0).await;

    let messages = drain(&mut direct_rx);
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::MonsterSpawned { monster } if monster.id == "monster_a")));
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::GroundItemAppeared { item } if item.instance_id == 42)));
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::PlayerMoved { player_id: moved, .. } if *moved == player_id)));
}

/// The AOI diff visits only the monsters the cell index reports near the step,
/// so a monster's index entry has to follow its moves. The arrival point below
/// is picked to query the monster's new cell but not the one it left: with the
/// index left behind, the walker never hears about it.
#[tokio::test]
async fn movement_into_aoi_sends_a_monster_that_walked_there() {
    let game_state = make_test_game_state("moved_monster_aoi");
    let walker_id = pid("walker");
    game_state
        .add_player(make_player("bystander", 60.0, 0.0))
        .await;
    // All three spots hang off the AOI: the monster walks from outside it to a
    // spot the walker only reaches after moving.
    let aoi = onlinerpg_shared::EVENT_DELIVERY_RADIUS;
    let start = Position {
        x: 2.0 * aoi + 1.0,
        y: 0.0,
        z: 0.0,
    };
    let walked_to = Position {
        x: 2.0 * aoi - 6.0,
        y: 0.0,
        z: 0.0,
    };

    game_state.add_player(make_player("walker", 0.0, 0.0)).await;
    {
        let mut monsters = game_state.monsters.write().await;
        let monster = make_monster("wanderer", start, 0);

        monsters.insert(monster.id.clone(), monster);
    }
    game_state
        .apply_ai_move(
            "wanderer",
            0,
            walked_to,
            0.0,
            MonsterState::Run,
            walked_to,
            None,
        )
        .await;
    assert_eq!(
        game_state.monsters.read().await["wanderer"].position.x,
        walked_to.x,
        "the move must be accepted for this to test anything"
    );

    let mut direct_rx = game_state.register_direct_channel(&walker_id).await;
    game_state
        .request_test_move(&walker_id, move_cmd(pos(aoi - 3.0), false), false)
        .await;
    game_state.advance_test_movement(60.0).await;

    let spawned = drain(&mut direct_rx).into_iter().any(
        |msg| matches!(msg, ServerMessage::MonsterSpawned { monster } if monster.id == "wanderer"),
    );
    assert!(
        spawned,
        "a monster that walked into range must be announced"
    );
}

#[test]
fn restored_floor_falls_back_to_surface() {
    let max_floor = onlinerpg_shared::housing::MAX_FLOOR_LEVEL as i8;

    for saved in [max_floor + 1, 120, i8::MAX] {
        assert_eq!(
            super::restored_floor_level(saved),
            0,
            "floor {saved} must fall back to the surface"
        );
    }

    // Negative floors use the existing dungeon rehydration path.
    for saved in [i8::MIN, -1, 0, max_floor] {
        assert_eq!(
            super::restored_floor_level(saved),
            saved,
            "floor {saved} must be restored as-is"
        );
    }
}

#[tokio::test]
async fn teleport_rejects_non_finite_position() {
    let game_state = make_test_game_state("movement_teleport_non_finite");
    let player_id = pid("gm");
    game_state.add_player(make_player("gm", 3.0, 4.0)).await;

    for bad_x in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
        game_state
            .teleport_player(
                &player_id,
                Position {
                    x: bad_x,
                    y: 0.0,
                    z: 0.0,
                },
                0.0,
                0,
            )
            .await;
        assert_eq!(player_xz(&game_state, &player_id).await.0, 3.0);
    }
}

#[test]
fn within_radius_covers_every_point_inside_the_radius() {
    use onlinerpg_shared::{wrap_world_x, WORLD_MIN_X, WORLD_WIDTH_X};

    let radius = super::super::EVENT_DELIVERY_RADIUS;
    let mut seed = 0x5EED_F00Du64;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as f32 / (u32::MAX as f32 / 2.0)
    };

    // Both world edges, the middle, and random interior points: the seam is
    // where a naive implementation breaks.
    let centers: Vec<f32> = [
        WORLD_MIN_X,
        WORLD_MIN_X + 1.0,
        WORLD_MIN_X + radius,
        0.0,
        WORLD_MIN_X + WORLD_WIDTH_X - radius,
        WORLD_MIN_X + WORLD_WIDTH_X - 1.0,
    ]
    .into_iter()
    .chain((0..40).map(|_| WORLD_MIN_X + next() * WORLD_WIDTH_X))
    .collect();

    for center_x in centers {
        let center = Position {
            x: wrap_world_x(center_x),
            y: 0.0,
            z: next() * 400.0 - 200.0,
        };
        let cells: std::collections::HashSet<_> =
            super::super::SpatialCell::within_radius(&center, radius).collect();

        for _ in 0..80 {
            // Points ringing the boundary, inside and just outside.
            let angle = next() * std::f32::consts::TAU;
            let dist = next() * radius * 1.2;
            let probe = Position {
                x: wrap_world_x(center.x + angle.cos() * dist),
                y: 0.0,
                z: center.z + angle.sin() * dist,
            };
            if center.dist_xz_sq(&probe) > radius * radius {
                continue;
            }
            assert!(
                cells.contains(&super::super::SpatialCell::from_position(&probe)),
                "{:?} is {:.1}m from {:?} but its cell was not enumerated",
                probe,
                center.dist_xz_sq(&probe).sqrt(),
                center
            );
        }
    }
}

async fn move_to(game_state: &GameState, id: &PlayerId, x: f32, y: f32, z: f32) -> f32 {
    game_state
        .request_test_move(
            id,
            TestMove {
                position: Position { x, y, z },
                sprinting: false,
            },
            false,
        )
        .await;
    game_state.advance_test_movement(60.0).await;
    game_state.get_all_players().await[id].position.y
}

/// On open terrain the stored Y is the terrain's, not the client's — a
/// forged height neither flies nor sinks the player.
#[tokio::test]
async fn open_terrain_y_comes_from_the_heightmap() {
    let game_state = make_test_game_state("movement_ground_y");
    let id = pid("grounded");
    game_state
        .add_player(make_player("grounded", 100.0, 50.0))
        .await;

    assert_eq!(move_to(&game_state, &id, 102.0, 40.0, 50.0).await, 5.0);
    assert_eq!(move_to(&game_state, &id, 104.0, -40.0, 50.0).await, 5.0);
}

/// A bridge deck lifts the mover to the deck curve from the catalog.
#[tokio::test]
async fn bridge_deck_y_comes_from_the_deck_index() {
    let game_state = make_test_game_state("movement_deck_y");
    let id = pid("crosser");
    game_state
        .add_player(make_player("crosser", 112.0, 50.0))
        .await;
    game_state.sync_region_furniture(0, 0, &[stone_bridge(100.0, 5.0, 50.0)]);

    assert_eq!(move_to(&game_state, &id, 112.0, 0.0, 50.0).await, 5.0);
    let end = move_to(&game_state, &id, 109.5, 0.0, 50.0).await;
    assert!(end > 5.0 && end < 6.0, "{end}");
    let crown = move_to(&game_state, &id, 100.0, 0.0, 50.0).await;
    assert!((crown - 7.4951).abs() < 1e-3, "{crown}");
    assert_eq!(move_to(&game_state, &id, 112.0, 0.0, 50.0).await, 5.0);
}

/// A wader in the river under the span stays on the bed: the deck is only
/// taken by a mover no lower than its abutments.
#[tokio::test]
async fn under_a_bridge_the_mover_keeps_the_river_bed() {
    let game_state = make_test_game_state("movement_under_deck");
    let id = pid("wader");
    game_state
        .add_player(make_player("wader", -100.0, 60.0))
        .await;
    game_state.sync_region_furniture(-1, 0, &[stone_bridge(-100.0, 3.0, 50.0)]);

    assert_eq!(move_to(&game_state, &id, -100.0, 0.0, 55.0).await, -5.0);
    assert_eq!(move_to(&game_state, &id, -100.0, 9.0, 50.0).await, -5.0);
}
