use super::*;

/// Ground height equals the tile X index, so crossing a tile seam is a 1m step.
struct SteppedHeightTiles;

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for SteppedHeightTiles {
    async fn read_heightmap(&self, tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        Ok(uniform_heightmap(tx as f32))
    }
}

/// `base` with one component replaced by each non-finite value, labelled.
fn non_finite_positions(base: Position) -> Vec<(String, Position)> {
    let mut out = Vec::new();
    for (value, value_name) in NON_FINITE {
        for (axis, axis_name) in [(0, "x"), (1, "y"), (2, "z")] {
            let mut position = base;
            match axis {
                0 => position.x = value,
                1 => position.y = value,
                _ => position.z = value,
            }
            out.push((format!("{axis_name} {value_name}"), position));
        }
    }
    out
}

#[tokio::test]
async fn monster_events_do_not_cross_floors() {
    let game_state = make_test_game_state("monster_floor_segregation");

    // A surface guard and a dungeon delver share the exact XZ footprint: the
    // guard stands directly above the dungeon floor the delver is on.
    let mut guard = make_player("guard", 0.0, 0.0);
    guard.floor_level = 0;
    let mut delver = make_player("delver", 0.0, 0.0);
    delver.floor_level = -1;
    game_state.add_player(guard).await;
    game_state.add_player(delver).await;

    // Channels registered after join so the AOI snapshots don't pollute them.
    let mut guard_rx = game_state.register_direct_channel(&pid("guard")).await;
    let mut delver_rx = game_state.register_direct_channel(&pid("delver")).await;

    let monster_pos = Position {
        x: 0.0,
        y: -40.0,
        z: 0.0,
    };
    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("dungeon_monster", monster_pos, -1);
        monster.owner_id = Some(pid("keeper"));
        monsters.insert("dungeon_monster".to_string(), monster);
    }

    game_state
        .update_monster_position(
            &pid("keeper"),
            "dungeon_monster".to_string(),
            monster_pos,
            0.0,
            MonsterState::Walk,
            monster_pos,
        )
        .await;

    // Same-floor delver sees the movement; the surface guard above never does.
    match delver_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { monster_id, .. }) => {
            assert_eq!(monster_id, "dungeon_monster");
        }
        other => panic!(
            "Expected MonsterMoved for same-floor delver, got {:?}",
            other
        ),
    }
    match guard_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!(
            "Surface guard must not receive dungeon monster events, got {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn monster_move_requires_ownership() {
    let game_state = make_test_game_state("monster_move_ownership");
    let owner_id = pid("owner");
    let hijacker_id = pid("hijacker");

    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    game_state
        .add_player(make_player("hijacker", 0.0, 0.0))
        .await;
    let mut hijacker_rx = game_state.register_direct_channel(&hijacker_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("victim_monster", pos(1.0), 0);
        monster.owner_id = Some(owner_id);
        monsters.insert("victim_monster".to_string(), monster);
    }

    game_state
        .update_monster_position(
            &hijacker_id,
            "victim_monster".to_string(),
            pos(50.0),
            0.0,
            MonsterState::Walk,
            pos(50.0),
        )
        .await;

    assert_eq!(
        game_state.monsters.read().await["victim_monster"]
            .position
            .x,
        1.0,
        "a non-owner move must not change the monster position"
    );
    // The non-owner is told to drop its brain and, still being in sight,
    // gets the monster back as a bystander view.
    match hijacker_rx.try_recv() {
        Ok(ServerMessage::MonsterRemoved { monster_id }) => {
            assert_eq!(monster_id, "victim_monster")
        }
        other => panic!("a non-owner move must be answered by a release, got {other:?}"),
    }
    match hijacker_rx.try_recv() {
        Ok(ServerMessage::MonsterSpawned { monster }) => {
            assert_eq!(monster.owner_id, Some(owner_id))
        }
        other => panic!("Expected a bystander respawn for the watcher, got {other:?}"),
    }

    game_state
        .update_monster_position(
            &owner_id,
            "victim_monster".to_string(),
            pos(2.0),
            0.0,
            MonsterState::Walk,
            pos(2.0),
        )
        .await;

    assert_eq!(
        game_state.monsters.read().await["victim_monster"]
            .position
            .x,
        2.0,
        "the owner's move must apply"
    );
}

#[tokio::test]
async fn non_finite_monster_move_is_rejected() {
    let game_state = make_test_game_state("monster_move_non_finite");
    let owner_id = pid("owner");
    let observer_id = pid("observer");
    let authoritative_position = Position {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };

    game_state.add_player(make_player("owner", 1.0, 3.0)).await;
    game_state
        .add_player(make_player("observer", 1.0, 3.0))
        .await;
    let mut owner_rx = game_state.register_direct_channel(&owner_id).await;
    let mut observer_rx = game_state.register_direct_channel(&observer_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("owned_monster", authoritative_position, 0);
        monster.owner_id = Some(owner_id);
        monster.rotation = 0.25;
        monster.move_budget = 7.0;
        monster.last_move_at = 42;
        monsters.insert("owned_monster".to_string(), monster);
    }

    let mut cases = Vec::new();
    for (label, position) in non_finite_positions(authoritative_position) {
        cases.push((
            format!("position {label}"),
            position,
            0.5,
            authoritative_position,
        ));
        cases.push((
            format!("target {label}"),
            authoritative_position,
            0.5,
            position,
        ));
    }
    for (rotation, value_name) in NON_FINITE {
        cases.push((
            format!("rotation {value_name}"),
            authoritative_position,
            rotation,
            authoritative_position,
        ));
    }

    for (case, position, rotation, target_position) in cases {
        let before = game_state.monsters.read().await["owned_monster"].clone();

        game_state
            .update_monster_position(
                &owner_id,
                "owned_monster".to_string(),
                position,
                rotation,
                MonsterState::Run,
                target_position,
            )
            .await;

        let after = game_state.monsters.read().await["owned_monster"].clone();
        assert_eq!(after.position, before.position, "{case}");
        assert_eq!(after.rotation, before.rotation, "{case}");
        assert_eq!(after.state, before.state, "{case}");
        assert_eq!(after.move_budget, before.move_budget, "{case}");
        assert_eq!(after.last_move_at, before.last_move_at, "{case}");

        match observer_rx.try_recv() {
            Err(MpscTryRecvError::Empty) => {}
            other => panic!("{case} must not fan out, got {other:?}"),
        }
        match owner_rx.try_recv() {
            Ok(ServerMessage::MonsterMoved {
                monster_id,
                position,
                rotation,
                state,
                target_position,
                ..
            }) => {
                assert_eq!(monster_id, "owned_monster", "{case}");
                assert_eq!(position, before.position, "{case}");
                assert_eq!(rotation, before.rotation, "{case}");
                assert_eq!(state, before.state, "{case}");
                assert_eq!(target_position, before.position, "{case}");
            }
            other => panic!("{case} must correct the owner, got {other:?}"),
        }
        assert!(matches!(owner_rx.try_recv(), Err(MpscTryRecvError::Empty)));
    }
}

#[tokio::test]
async fn monster_move_cannot_report_dead_state() {
    let game_state = make_test_game_state("monster_move_dead_state");
    let owner_id = pid("owner");
    let observer_id = pid("observer");
    let authoritative_position = pos(1.0);

    game_state.add_player(make_player("owner", 1.0, 0.0)).await;
    game_state
        .add_player(make_player("observer", 1.0, 0.0))
        .await;
    let mut owner_rx = game_state.register_direct_channel(&owner_id).await;
    let mut observer_rx = game_state.register_direct_channel(&observer_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("owned_monster", authoritative_position, 0);
        monster.owner_id = Some(owner_id);
        monster.move_budget = 7.0;
        monster.last_move_at = 42;
        monsters.insert(monster.id.clone(), monster);
    }

    game_state
        .update_monster_position(
            &owner_id,
            "owned_monster".to_string(),
            authoritative_position,
            0.5,
            MonsterState::Dead,
            authoritative_position,
        )
        .await;

    let monster = game_state.monsters.read().await["owned_monster"].clone();
    assert_eq!(monster.position, authoritative_position);
    assert_eq!(monster.rotation, 0.0);
    assert_eq!(monster.state, MonsterState::Idle);
    assert_eq!(monster.move_budget, 7.0);
    assert_eq!(monster.last_move_at, 42);

    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved {
            monster_id,
            position,
            rotation,
            state,
            target_position,
            ..
        }) => {
            assert_eq!(monster_id, "owned_monster");
            assert_eq!(position, authoritative_position);
            assert_eq!(rotation, 0.0);
            assert_eq!(state, MonsterState::Idle);
            assert_eq!(target_position, authoritative_position);
        }
        other => panic!("a client-reported death must correct the owner, got {other:?}"),
    }
    assert!(matches!(owner_rx.try_recv(), Err(MpscTryRecvError::Empty)));
    assert!(matches!(
        observer_rx.try_recv(),
        Err(MpscTryRecvError::Empty)
    ));
}

#[tokio::test]
async fn ambient_spawn_stores_authoritative_world_position() {
    let game_state = make_test_game_state("canonical_spawn_position");
    let player_id = pid("spawner");
    game_state
        .add_player(make_player("spawner", 1.0, 0.0))
        .await;
    // What the placement hands `spawn_monster`: a seam-wrapped X and the
    // ground the server samples, never the reported pose.
    let position = Position {
        x: onlinerpg_shared::wrap_world_x(onlinerpg_shared::WORLD_WIDTH_X * 2.0 + 1.0),
        y: 5.0,
        z: 0.0,
    };

    let monster = game_state
        .spawn_monster(
            "goblin".to_string(),
            position,
            0.0,
            Some(player_id),
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .expect("spawn should fit the test cap");
    assert_eq!(monster.position.x, 1.0);
    assert_eq!(monster.position.y, 5.0);
    assert_eq!(
        game_state.monsters.read().await[&monster.id].position.x,
        1.0
    );
}

/// Spawn canonicalization only holds if moves keep it. The budget and the
/// terrain sweep both measure periodic distance, so an owner could otherwise
/// report a whole-world-width offset as a short move and park the monster
/// outside every spatial-hash lookup — invisible to watchers while its
/// periodic attack reach still landed.
#[tokio::test]
async fn client_monster_move_stores_canonical_world_x() {
    let game_state = make_test_game_state("monster_move_canonical_x");
    let owner_id = pid("owner");
    let start = pos(1.0);

    game_state.add_player(make_player("owner", 1.0, 0.0)).await;
    game_state
        .add_player(make_player("observer", 1.0, 0.0))
        .await;
    let mut observer_rx = game_state.register_direct_channel(&pid("observer")).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("wrapping_monster", start, 0);
        monster.owner_id = Some(owner_id);
        monster.move_budget = 10.0;
        monster.last_move_at = GameState::now_ms();
        monsters.insert(monster.id.clone(), monster);
    }

    let wrapped = pos(1.5 + onlinerpg_shared::WORLD_WIDTH_X * 2.0);
    game_state
        .update_monster_position(
            &owner_id,
            "wrapping_monster".to_string(),
            wrapped,
            0.0,
            MonsterState::Run,
            wrapped,
        )
        .await;

    assert_eq!(
        game_state.monsters.read().await["wrapping_monster"]
            .position
            .x,
        1.5
    );
    // The watcher still sees it move rather than leave: a non-canonical X
    // would fall outside every queried spatial cell and read as a departure.
    match observer_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved {
            position,
            target_position,
            ..
        }) => {
            assert_eq!(position.x, 1.5);
            assert_eq!(target_position.x, 1.5);
        }
        other => panic!("a periodic-equivalent move must fan out, got {other:?}"),
    }
}

#[tokio::test]
async fn client_monster_move_charges_vertical_displacement() {
    let game_state = make_game_state_with(
        "monster_move_vertical_budget",
        SteppedHeightTiles,
        SeaOnlyWater,
    );
    let owner_id = pid("owner");
    let observer_id = pid("observer");
    let start = Position {
        x: 31.9,
        y: 0.0,
        z: 1.0,
    };

    game_state
        .add_player(make_player("owner", start.x, start.z))
        .await;
    game_state
        .add_player(make_player("observer", start.x, start.z))
        .await;
    let mut owner_rx = game_state.register_direct_channel(&owner_id).await;
    let mut observer_rx = game_state.register_direct_channel(&observer_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("vertical_monster", start, 0);
        monster.owner_id = Some(owner_id);
        monster.move_budget = 1.0;
        monster.last_move_at = GameState::now_ms();
        monsters.insert(monster.id.clone(), monster);
    }

    let forged = Position { y: -40.0, ..start };
    game_state
        .update_monster_position(
            &owner_id,
            "vertical_monster".to_string(),
            forged,
            0.0,
            MonsterState::Run,
            forged,
        )
        .await;

    let after = game_state.monsters.read().await["vertical_monster"].clone();
    assert_eq!(after.position, start);
    assert!(after.move_budget >= 1.0, "a rejected move keeps its refill");
    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => assert_eq!(position, start),
        other => panic!("a vertical teleport must correct its owner, got {other:?}"),
    }
    assert!(matches!(
        observer_rx.try_recv(),
        Err(MpscTryRecvError::Empty)
    ));

    {
        let mut monsters = game_state.monsters.write().await;
        let monster = monsters.get_mut("vertical_monster").unwrap();
        monster.move_budget = 12.0;
        monster.last_move_at = GameState::now_ms();
    }
    let legal = Position {
        x: 32.1,
        y: 1.0,
        z: 1.0,
    };
    let expected = legal.wrapped_x();
    game_state
        .update_monster_position(
            &owner_id,
            "vertical_monster".to_string(),
            legal,
            0.0,
            MonsterState::Walk,
            legal,
        )
        .await;

    let after = game_state.monsters.read().await["vertical_monster"].clone();
    assert_eq!(after.position, expected);
    assert!(
        after.move_budget < 11.5,
        "a vertical-dominant move must spend its vertical displacement, got {}",
        after.move_budget
    );
    match observer_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => assert_eq!(position, expected),
        other => panic!("a bounded vertical move must fan out, got {other:?}"),
    }
}

/// A client-owned monster is still untrusted movement. Staying within the
/// speed bucket must not let its owner walk it through the same solid cell that
/// blocks server-simulated player movement — while an equally long move that
/// clears the furniture still goes through.
#[tokio::test]
async fn client_owned_monster_cannot_cross_solid_furniture() {
    let game_state = make_test_game_state("monster_furniture_collision");
    let owner_id = pid("monster_owner");
    let observer_id = pid("monster_observer");
    let start = Position {
        x: 0.5,
        y: 0.0,
        z: 4.5,
    };
    let destination = Position {
        x: 0.5,
        y: 0.0,
        z: 6.5,
    };

    game_state
        .add_player(make_player("monster_owner", start.x, start.z))
        .await;
    game_state
        .add_player(make_player("monster_observer", start.x, start.z))
        .await;
    let mut owner_rx = game_state.register_direct_channel(&owner_id).await;
    let mut observer_rx = game_state.register_direct_channel(&observer_id).await;
    game_state.sync_region_furniture(0, 0, &[table_placement(0.5, 5.5)]);

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("wall_walking_monster", start, 0);
        monster.owner_id = Some(owner_id);
        monster.move_budget = 10.0;
        monster.last_move_at = GameState::now_ms();
        monsters.insert(monster.id.clone(), monster);
    }

    game_state
        .update_monster_position(
            &owner_id,
            "wall_walking_monster".to_string(),
            destination,
            0.0,
            MonsterState::Run,
            destination,
        )
        .await;

    let after = game_state.monsters.read().await["wall_walking_monster"].clone();
    assert_eq!(
        after.position, start,
        "solid furniture must block a client-owned monster move"
    );
    // Banked, not spent: a block keeps the refill like the speed-reject path.
    assert!(
        after.move_budget >= 10.0,
        "a blocked move must not consume budget, got {}",
        after.move_budget
    );
    assert!(matches!(
        observer_rx.try_recv(),
        Err(MpscTryRecvError::Empty)
    ));
    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => assert_eq!(position, start),
        other => panic!("a blocked monster move must correct its owner, got {other:?}"),
    }

    let forged_height = Position {
        x: start.x + 0.25,
        y: 2.0,
        ..start
    };
    game_state
        .update_monster_position(
            &owner_id,
            "wall_walking_monster".to_string(),
            forged_height,
            0.0,
            MonsterState::Run,
            forged_height,
        )
        .await;
    assert_eq!(
        game_state.monsters.read().await["wall_walking_monster"].position,
        start,
        "a client-selected height must not clear the furniture"
    );
    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => assert_eq!(position, start),
        other => panic!("a forged height must correct its owner, got {other:?}"),
    }
    assert!(matches!(
        observer_rx.try_recv(),
        Err(MpscTryRecvError::Empty)
    ));

    // The sweep's other side. An owner reports whole path legs at once, so a
    // sweep that over-refuses would freeze legitimate monsters at every corner:
    // the same 2m hop away from the table must apply and reach watchers.
    let clear = Position {
        x: 0.5,
        y: 0.0,
        z: 2.5,
    };
    game_state
        .update_monster_position(
            &owner_id,
            "wall_walking_monster".to_string(),
            clear,
            0.0,
            MonsterState::Run,
            clear,
        )
        .await;

    assert_eq!(
        game_state.monsters.read().await["wall_walking_monster"].position,
        clear,
        "a clear move must apply"
    );
    match observer_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => assert_eq!(position, clear),
        other => panic!("a clear move must fan out to watchers, got {other:?}"),
    }
    // The owner applied it optimistically and gets no correction.
    assert!(matches!(owner_rx.try_recv(), Err(MpscTryRecvError::Empty)));
}

/// Even the owner can't teleport a monster onto a distant victim: a move is
/// capped to what the monster could run since its last accepted move, so an
/// owned monster stays a melee threat only where it could actually walk.
#[tokio::test]
async fn monster_move_is_speed_capped() {
    let game_state = make_test_game_state("monster_move_speed_cap");
    let owner_id = pid("owner");

    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    // A bystander next to the monster observes fanout: the owner is skipped in
    // the position broadcast, so it can't tell an applied move from a refused
    // one on its own.
    game_state
        .add_player(make_player("observer", 0.0, 0.0))
        .await;
    let mut observer_rx = game_state.register_direct_channel(&pid("observer")).await;
    let mut owner_rx = game_state.register_direct_channel(&owner_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("owned_monster", pos(0.0), 0);
        monster.owner_id = Some(owner_id);
        // A large elapsed budget (last_move_at = 0) still can't clear the
        // absolute per-move cap.
        monsters.insert("owned_monster".to_string(), monster);
    }

    // A 50m jump exceeds the absolute step cap and is refused outright.
    game_state
        .update_monster_position(
            &owner_id,
            "owned_monster".to_string(),
            pos(50.0),
            0.0,
            MonsterState::Run,
            pos(50.0),
        )
        .await;
    assert_eq!(
        game_state.monsters.read().await["owned_monster"].position.x,
        0.0,
        "a teleport past the step cap must not move the monster"
    );
    match observer_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!("a rejected move must not fan out, got {other:?}"),
    }
    // The mover applies its moves optimistically, so a reject must echo the
    // authoritative position back to it (and only to it).
    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => {
            assert_eq!(position.x, 0.0, "correction must carry the real position");
        }
        other => panic!("a rejected move must send a correction to the mover, got {other:?}"),
    }

    // A short hop within the cap still applies normally.
    game_state
        .update_monster_position(
            &owner_id,
            "owned_monster".to_string(),
            pos(5.0),
            0.0,
            MonsterState::Run,
            pos(5.0),
        )
        .await;
    assert_eq!(
        game_state.monsters.read().await["owned_monster"].position.x,
        5.0,
        "a move within the cap must apply"
    );
    match observer_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { monster_id, .. }) => {
            assert_eq!(monster_id, "owned_monster");
        }
        other => panic!("an accepted move must fan out to bystanders, got {other:?}"),
    }
    match owner_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!("an accepted move must not echo to the mover, got {other:?}"),
    }

    // Drain the bucket and reset its clock: an under-cap jump (8m < 15m) is now
    // refused by the refill rate, not the cap, since no time has passed to
    // refill it.
    {
        let mut monsters = game_state.monsters.write().await;
        let monster = monsters.get_mut("owned_monster").unwrap();
        monster.move_budget = 0.0;
        monster.last_move_at = GameState::now_ms();
    }
    game_state
        .update_monster_position(
            &owner_id,
            "owned_monster".to_string(),
            pos(13.0),
            0.0,
            MonsterState::Run,
            pos(13.0),
        )
        .await;
    assert_eq!(
        game_state.monsters.read().await["owned_monster"].position.x,
        5.0,
        "an 8m jump with an empty budget must be refused by the refill rate"
    );
    match owner_rx.try_recv() {
        Ok(ServerMessage::MonsterMoved { position, .. }) => {
            assert_eq!(position.x, 5.0, "correction must carry the real position");
        }
        other => panic!("a rate-refused move must send a correction to the mover, got {other:?}"),
    }
}

/// Owning a monster must not let a player damage arbitrary targets at range.
/// Anyone can spawn a monster beside themselves and become its owner, so the
/// ownership check alone would leave `target_player_id` as a world-wide damage
/// primitive against any id the attacker can name.
#[tokio::test]
async fn monster_attack_requires_proximity_to_target() {
    let game_state = make_test_game_state("monster_attack_range");
    let owner_id = pid("owner");
    let victim_id = pid("victim");

    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    // Far out of any monster's reach, but well within the attacker's ability to
    // name: an id is all the exploit needed.
    game_state
        .add_player(make_player("victim", 500.0, 0.0))
        .await;

    // Each half uses its own monster: a rejected attack still consumes the
    // cooldown, so reusing one would block the in-range case for 1.5s.
    {
        let mut monsters = game_state.monsters.write().await;
        for (id, x) in [("far_monster", 0.0), ("near_monster", 499.0)] {
            let mut monster = make_monster(id, pos(x), 0);
            monster.owner_id = Some(owner_id);
            monsters.insert(id.to_string(), monster);
        }
    }

    game_state
        .broadcast_monster_attack(&owner_id, "far_monster", &victim_id)
        .await;

    // `last_combat_at` is stamped for any in-range attack, hit or miss, so it
    // records that the swing was processed without depending on a damage roll.
    assert_eq!(
        game_state.players.read().await[&victim_id].last_combat_at,
        0,
        "a monster 500m from its target must not reach it"
    );
    assert_eq!(
        game_state.players.read().await[&victim_id].health,
        10,
        "an out-of-range monster attack must not deal damage"
    );

    game_state
        .broadcast_monster_attack(&owner_id, "near_monster", &victim_id)
        .await;

    assert_ne!(
        game_state.players.read().await[&victim_id].last_combat_at,
        0,
        "a monster standing next to its target must still land its attack"
    );
}

/// A monster and its target must share a floor, so a surface monster cannot
/// strike a player on the dungeon floor directly beneath it.
#[tokio::test]
async fn cross_floor_monster_attack_is_rejected() {
    let game_state = make_test_game_state("cross_floor_monster_attack");
    let owner_id = pid("owner");
    let delver_id = pid("delver");

    game_state.add_player(make_player("owner", 0.0, 0.0)).await;
    let mut delver = make_player("delver", 0.0, 0.0);
    delver.floor_level = -1;
    delver.position.y = -40.0;
    game_state.add_player(delver).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("surface_monster", pos(0.0), 0);
        monster.owner_id = Some(owner_id);
        monsters.insert("surface_monster".to_string(), monster);
    }

    game_state
        .broadcast_monster_attack(&owner_id, "surface_monster", &delver_id)
        .await;

    assert_eq!(
        game_state.players.read().await[&delver_id].last_combat_at,
        0,
        "a surface monster must not reach a player one floor below it"
    );
}

#[tokio::test]
async fn cross_floor_player_attack_is_rejected() {
    let game_state = make_test_game_state("cross_floor_attack");

    let mut guard = make_player("guard", 0.0, 0.0);
    guard.floor_level = 0;
    game_state.add_player(guard).await;
    let mut guard_rx = game_state.register_direct_channel(&pid("guard")).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "dungeon_monster".to_string(),
            make_monster(
                "dungeon_monster",
                Position {
                    x: 0.0,
                    y: -40.0,
                    z: 0.0,
                },
                -1,
            ),
        );
    }

    game_state
        .broadcast_player_attack(&pid("guard"), "dungeon_monster".to_string())
        .await;

    // The attack is dropped server-side: the monster keeps full HP and the
    // attacker gets a coarse rejection that must not name the floor.
    let health = game_state
        .monsters
        .read()
        .await
        .get("dungeon_monster")
        .map(|m| m.health)
        .unwrap();
    assert_eq!(health, 10, "cross-floor attack must not damage the monster");
    expect_attack_rejected(
        &mut guard_rx,
        "dungeon_monster",
        AttackRejectReason::InvalidTarget,
    );
}

#[tokio::test]
async fn out_of_range_player_attack_only_provokes_monster() {
    let game_state = make_test_game_state("out_of_range_attack");
    let player_id = pid("attacker");
    let controller_id = pid("monster_controller");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    let mut controller_rx = game_state.register_direct_channel(&controller_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("distant_monster", pos(3.01), 0);
        monster.owner_id = Some(controller_id);
        monsters.insert("distant_monster".to_string(), monster);
    }

    game_state
        .broadcast_player_attack(&player_id, "distant_monster".to_string())
        .await;

    let monsters = game_state.monsters.read().await;
    assert_eq!(
        monsters["distant_monster"].health, 10,
        "an out-of-range attack must not damage the monster"
    );
    drop(monsters);
    assert_eq!(
        game_state.players.read().await[&player_id].last_combat_at,
        0,
        "a rejected attack must not enter combat"
    );
    match controller_rx.try_recv() {
        Ok(ServerMessage::MonsterProvoked {
            player_id: actual_player_id,
            monster_id,
        }) => {
            assert_eq!(actual_player_id, player_id);
            assert_eq!(monster_id, "distant_monster");
        }
        other => panic!("Expected only an aggro event outside melee range, got {other:?}"),
    }
    expect_attack_rejected(
        &mut attacker_rx,
        "distant_monster",
        AttackRejectReason::OutOfRange,
    );
}

#[tokio::test]
async fn player_attack_beyond_provoke_range_is_fully_rejected() {
    let game_state = make_test_game_state("beyond_provoke_range_attack");
    let player_id = pid("attacker");
    let controller_id = pid("monster_controller");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    let mut controller_rx = game_state.register_direct_channel(&controller_id).await;

    {
        let mut monster = make_monster(
            "remote_monster",
            pos(super::combat::PLAYER_ATTACK_PROVOKE_RANGE_METERS + 0.01),
            0,
        );
        monster.owner_id = Some(controller_id);
        game_state
            .monsters
            .write()
            .await
            .insert("remote_monster".to_string(), monster);
    }

    game_state
        .broadcast_player_attack(&player_id, "remote_monster".to_string())
        .await;

    assert_eq!(
        game_state.monsters.read().await["remote_monster"].health,
        10
    );
    assert_eq!(
        game_state.players.read().await[&player_id].last_combat_at,
        0
    );
    match controller_rx.try_recv() {
        Err(MpscTryRecvError::Empty) => {}
        other => panic!("Expected no provoke event beyond 10m, got {other:?}"),
    }
    expect_attack_rejected(
        &mut attacker_rx,
        "remote_monster",
        AttackRejectReason::OutOfRange,
    );
}

#[tokio::test]
async fn player_attack_at_melee_range_is_allowed() {
    let game_state = make_test_game_state("melee_range_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "nearby_monster".to_string(),
            make_monster("nearby_monster", pos(2.0), 0),
        );
    }

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    match attacker_rx.try_recv() {
        Ok(ServerMessage::PlayerAttacked {
            player_id: actual_player_id,
            monster_id,
            ..
        }) => {
            assert_eq!(actual_player_id, player_id);
            assert_eq!(monster_id, "nearby_monster");
        }
        other => panic!("Expected an attack echo at melee range, got {other:?}"),
    }
    assert_ne!(
        game_state.players.read().await[&player_id].last_combat_at,
        0,
        "an allowed attack must enter combat"
    );
}

#[tokio::test]
async fn player_attack_interval_is_server_enforced() {
    let game_state = make_test_game_state("player_attack_interval");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    game_state.monsters.write().await.insert(
        "nearby_monster".to_string(),
        make_monster("nearby_monster", pos(1.0), 0),
    );

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    let attack_count = drain(&mut attacker_rx)
        .into_iter()
        .filter(|message| matches!(message, ServerMessage::PlayerAttacked { .. }))
        .count();
    assert_eq!(
        attack_count, 1,
        "back-to-back requests must produce one authoritative attack roll"
    );

    game_state.last_player_attacks.write().await.insert(
        player_id,
        GameState::now_ms().saturating_sub(*super::combat::PLAYER_ATTACK_INTERVAL_MS),
    );
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    assert!(drain(&mut attacker_rx)
        .into_iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
}

#[tokio::test]
async fn rejected_player_attack_does_not_consume_interval() {
    let game_state = make_test_game_state("rejected_player_attack_interval");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;
    game_state.monsters.write().await.insert(
        "nearby_monster".to_string(),
        make_monster("nearby_monster", pos(1.0), 0),
    );

    game_state
        .broadcast_player_attack(&player_id, "missing_monster".to_string())
        .await;
    expect_attack_rejected(
        &mut attacker_rx,
        "missing_monster",
        AttackRejectReason::InvalidTarget,
    );
    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    assert!(drain(&mut attacker_rx)
        .into_iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
}

/// A player at 0 HP (awaiting respawn) must not be able to keep attacking.
#[tokio::test]
async fn dead_player_cannot_attack() {
    let game_state = make_test_game_state("dead_player_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    game_state
        .players
        .write()
        .await
        .get_mut(&player_id)
        .unwrap()
        .health = 0;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert(
            "nearby_monster".to_string(),
            make_monster("nearby_monster", pos(2.0), 0),
        );
    }

    game_state
        .broadcast_player_attack(&player_id, "nearby_monster".to_string())
        .await;

    expect_attack_rejected(
        &mut attacker_rx,
        "nearby_monster",
        AttackRejectReason::AttackerDead,
    );
    assert_eq!(
        game_state.monsters.read().await["nearby_monster"].health,
        10,
        "a dead player's attack must deal no damage"
    );
}

/// A stale id — dead or never known — earns the same coarse rejection, so
/// probing ids reveals nothing about hidden monster state.
#[tokio::test]
async fn stale_monster_attack_is_rejected_as_invalid_target() {
    let game_state = make_test_game_state("stale_monster_attack");
    let player_id = pid("attacker");

    game_state
        .add_player(make_player("attacker", 0.0, 0.0))
        .await;
    let mut attacker_rx = game_state.register_direct_channel(&player_id).await;

    {
        let mut monsters = game_state.monsters.write().await;
        let mut monster = make_monster("dead_monster", pos(1.0), 0);
        monster.state = MonsterState::Dead;
        monsters.insert("dead_monster".to_string(), monster);
    }

    for target in ["dead_monster", "unknown_monster"] {
        game_state
            .broadcast_player_attack(&player_id, target.to_string())
            .await;
        expect_attack_rejected(&mut attacker_rx, target, AttackRejectReason::InvalidTarget);
    }
}

// --- Ranged attacks (doc/COMBAT.md 원거리 전투) ---

/// An archer at the origin: `weapon` in hand, a full quiver, and the given
/// attribute spread, with a direct channel to read the attack broadcast off.
async fn setup_archer(
    game_state: &GameState,
    weapon: &str,
    attrs: CharacterAttributes,
) -> DirectRx {
    setup_archer_with_ammo(game_state, weapon, attrs, &[("iron_arrow", 20)]).await
}

/// `setup_archer` with the quiver spelled out — `(item_def_id, quantity)`
/// stacks, or none at all.
async fn setup_archer_with_ammo(
    game_state: &GameState,
    weapon: &str,
    attrs: CharacterAttributes,
    quiver: &[(&str, u32)],
) -> DirectRx {
    let player_id = pid("archer");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.equipped.insert(
        EquipSlot::MainHand,
        ItemInstance {
            locked: false,
            instance_id: 1,
            item_def_id: weapon.to_string(),
            quantity: 1,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
        },
    );
    for (index, (item_def_id, quantity)) in quiver.iter().enumerate() {
        inv.bag
            .push(bag_item(index as u64 + 10, item_def_id, *quantity));
    }
    game_state.inventories.write().await.insert(player_id, inv);
    game_state
        .player_characters
        .write()
        .await
        .insert(player_id, (1, 0, attrs));
    game_state.register_direct_channel(&player_id).await
}

/// Attributes with one ability raised to 30 (+10) and the rest at 10 (+0), so
/// which modifier the roll used is readable straight off the damage.
fn attrs_with(str_score: u8, dex: u8) -> CharacterAttributes {
    CharacterAttributes {
        r#str: str_score,
        dex,
        con: 10,
        int: 10,
        wis: 10,
        cha: 10,
        guard: 0,
    }
}

fn at(x: f32) -> Position {
    Position { x, y: 0.0, z: 0.5 }
}

/// This monster's attack result, skipping whatever else the swing sent first
/// — spending a round pushes an inventory update ahead of the broadcast.
fn expect_attacked(rx: &mut DirectRx, expected_id: &str) -> (bool, u32) {
    let mut seen = Vec::new();
    while let Ok(message) = rx.try_recv() {
        if let ServerMessage::PlayerAttacked {
            monster_id,
            hit,
            damage,
            ..
        } = &message
        {
            assert_eq!(monster_id, expected_id);
            return (*hit, *damage);
        }
        seen.push(message);
    }
    panic!("Expected a PlayerAttacked broadcast, got {seen:?}")
}

#[tokio::test]
async fn a_bow_cannot_spend_trade_reserved_ammo() {
    let game_state = make_test_game_state("bow_reserved_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    let archer = pid("archer");
    let buyer = pid("buyer");
    game_state.add_player(make_player("buyer", 1.0, 0.5)).await;
    game_state.request_player_trade(&archer, "buyer").await;
    game_state.respond_player_trade(&buyer, &archer, true).await;
    game_state
        .set_player_trade_offer(
            &archer,
            vec![onlinerpg_shared::messages::PlayerTradeSlot {
                instance_id: 10,
                quantity: 20,
            }],
            0,
        )
        .await;
    assert_eq!(game_state.trade_reserved_quantity(&archer, 10).await, 20);
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&archer, "far".to_string())
        .await;

    let messages = drain(&mut rx);
    assert!(messages.iter().any(|message| matches!(
        message,
        ServerMessage::PlayerAttackRejected { monster_id, reason }
            if monster_id == "far" && *reason == AttackRejectReason::OutOfAmmo
    )));
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::PlayerAttacked { .. })));
    assert_eq!(
        game_state.inventories.read().await[&archer].bag[0].quantity,
        20
    );
    assert!(!game_state
        .last_player_attacks
        .read()
        .await
        .contains_key(&archer));

    game_state.set_player_trade_offer(&archer, vec![], 0).await;
    game_state
        .broadcast_player_attack(&archer, "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    assert_eq!(
        game_state.inventories.read().await[&archer].bag[0].quantity,
        19
    );
}

#[tokio::test]
async fn a_bow_reaches_its_declared_range() {
    let game_state = make_test_game_state("bow_range_reaches");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(9.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "far");
    assert!(hit, "a +10 attack bonus always clears guard 10");
}

/// The server trails the walking player by the network lag, so a shot loosed
/// the moment the chase stops at 10m reads as slightly farther here. Melee has
/// always had that allowance; folding it into a `max` against the weapon range
/// swallowed it, and every shot taken at the edge was refused.
#[tokio::test]
async fn a_bow_shot_at_its_range_survives_the_lag_the_server_trails_by() {
    let game_state = make_test_game_state("bow_range_lag_allowance");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("edge".to_string(), make_monster("edge", at(10.6), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "edge".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "edge");
    assert!(hit);
}

#[tokio::test]
async fn a_bow_shot_past_its_range_is_rejected() {
    let game_state = make_test_game_state("bow_range_rejects");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("beyond".to_string(), make_monster("beyond", at(11.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "beyond".to_string())
        .await;

    expect_attack_rejected(&mut rx, "beyond", AttackRejectReason::OutOfRange);
}

/// A melee weapon keeps the 2m reach (plus lag tolerance) it always had, so
/// an empty `range` column changes nothing.
#[tokio::test]
async fn a_melee_weapon_keeps_its_hardcoded_reach() {
    let game_state = make_test_game_state("melee_reach_unchanged");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(5.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attack_rejected(&mut rx, "far", AttackRejectReason::OutOfRange);
}

#[tokio::test]
async fn a_bow_shoots_over_a_fence_at_close_and_long_range() {
    for distance in [1.5, 8.5] {
        let game_state = make_test_game_state(&format!("bow_over_fence_{distance}"));
        let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
        add_combat_fence(&game_state);
        game_state.monsters.write().await.insert(
            "behind".to_string(),
            make_monster("behind", at(distance), 0),
        );

        game_state
            .broadcast_player_attack(&pid("archer"), "behind".to_string())
            .await;

        let (hit, damage) = expect_attacked(&mut rx, "behind");
        assert!(hit);
        assert!(damage > 0);
        assert_eq!(
            game_state.inventories.read().await[&pid("archer")].bag[0].quantity,
            19
        );
    }
}

#[tokio::test]
async fn a_melee_attack_through_a_fence_is_rejected() {
    let game_state = make_test_game_state("melee_blocked_by_fence");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    add_combat_fence(&game_state);
    game_state
        .monsters
        .write()
        .await
        .insert("behind".to_string(), make_monster("behind", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "behind".to_string())
        .await;

    expect_attack_rejected(&mut rx, "behind", AttackRejectReason::OutOfRange);
}

fn add_combat_fence(game_state: &GameState) {
    use onlinerpg_shared::fence::{sync_passability, Fence, FenceAxis, FenceEdge};

    sync_passability(
        &mut game_state.passability_write(),
        "fences",
        &[Fence {
            edge: FenceEdge {
                x: 1,
                z: 0,
                axis: FenceAxis::Z,
            },
            y: 0.0,
            owner_id: 1,
        }],
    );
}

#[tokio::test]
async fn a_bow_shot_through_a_wall_is_rejected() {
    let game_state = make_test_game_state("bow_walled_off");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    add_combat_fence(&game_state);
    game_state.sync_region_furniture(0, 0, &[table_placement(4.5, 0.5)]);
    game_state
        .monsters
        .write()
        .await
        .insert("behind".to_string(), make_monster("behind", at(8.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "behind".to_string())
        .await;

    expect_attack_rejected(&mut rx, "behind", AttackRejectReason::OutOfRange);
}

/// DEX 30 (+10) with STR 10 (+0): a 1d6 bow can only reach 11 damage through
/// the DEX modifier.
#[tokio::test]
async fn a_ranged_hit_rolls_on_the_weapons_ability() {
    let game_state = make_test_game_state("bow_rolls_dex");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    let (hit, damage) = expect_attacked(&mut rx, "far");
    assert!(hit);
    assert!(damage >= 11, "1d1 + 1d6 arrow + DEX(+10), got {damage}");
}

/// The mirror image: STR 30 (+10) with DEX 10 (+0) on a melee weapon.
#[tokio::test]
async fn a_melee_hit_still_rolls_on_str() {
    let game_state = make_test_game_state("melee_rolls_str");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("near".to_string(), make_monster("near", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "near".to_string())
        .await;

    let (hit, damage) = expect_attacked(&mut rx, "near");
    assert!(hit);
    assert!(damage >= 11, "1d4 + STR(+10), got {damage}");
}

/// A shot that lands from outside melee reach must wake the monster onto the
/// shooter: its owner otherwise only applies the hit at its own impact frame.
#[tokio::test]
async fn a_landed_shot_from_range_provokes_the_target() {
    let game_state = make_test_game_state("bow_hit_provokes");
    let _rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    let controller_id = pid("monster_controller");
    let mut controller_rx = game_state.register_direct_channel(&controller_id).await;
    {
        let mut monster = make_monster("far", at(8.0), 0);
        monster.owner_id = Some(controller_id);
        monster.health = 100;
        game_state
            .monsters
            .write()
            .await
            .insert("far".to_string(), monster);
    }

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    let provoked = std::iter::from_fn(|| controller_rx.try_recv().ok()).any(|msg| {
        matches!(
            msg,
            ServerMessage::MonsterProvoked { player_id, monster_id }
                if player_id == pid("archer") && monster_id == "far"
        )
    });
    assert!(provoked, "a landed shot from 8m must aggro the target");
}

// --- Ammunition (doc/COMBAT.md 원거리 전투) ---

#[tokio::test]
async fn a_shot_spends_a_round() {
    let game_state = make_test_game_state("bow_spends_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let quiver = inventories[&pid("archer")]
        .bag
        .iter()
        .find(|item| item.item_def_id == "iron_arrow")
        .expect("the quiver survives one shot");
    assert_eq!(quiver.quantity, 19);
}

/// An empty quiver reads as its own refusal, not as an unreachable target —
/// the client has to be able to tell the player why nothing happened.
#[tokio::test]
async fn an_empty_quiver_refuses_the_shot() {
    let game_state = make_test_game_state("bow_out_of_ammo");
    let mut rx = setup_archer_with_ammo(&game_state, "bow", attrs_with(10, 30), &[]).await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attack_rejected(&mut rx, "far", AttackRejectReason::OutOfAmmo);
    assert_eq!(
        game_state.monsters.read().await["far"].health,
        10,
        "a shot with nothing to fire must not damage the target"
    );
}

/// A melee weapon declares no `ammoKind`, so an empty bag never stops a swing.
#[tokio::test]
async fn a_melee_swing_needs_no_ammunition() {
    let game_state = make_test_game_state("melee_needs_no_ammo");
    let mut rx = setup_archer_with_ammo(&game_state, "dagger", attrs_with(30, 10), &[]).await;
    game_state
        .monsters
        .write()
        .await
        .insert("near".to_string(), make_monster("near", at(1.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "near".to_string())
        .await;

    let (hit, _) = expect_attacked(&mut rx, "near");
    assert!(hit);
}

/// The strongest round of the right kind goes first, so buying better arrows
/// is the whole of using them.
#[tokio::test]
async fn the_strongest_round_is_the_one_spent() {
    let game_state = make_test_game_state("bow_picks_best_ammo");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 5), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("archer")].bag;
    let count = |id: &str| {
        bag.iter()
            .find(|item| item.item_def_id == id)
            .map_or(0, |item| item.quantity)
    };
    assert_eq!(count("steel_arrow"), 4, "the steel arrow is spent first");
    assert_eq!(count("iron_arrow"), 5, "the iron stack is left alone");
}

/// A round refused before the roll is a round still in the quiver — the gates
/// run first, and the cooldown claim after them.
#[tokio::test]
async fn a_refused_shot_keeps_its_round() {
    let game_state = make_test_game_state("bow_refused_keeps_ammo");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    game_state
        .monsters
        .write()
        .await
        .insert("beyond".to_string(), make_monster("beyond", at(11.5), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "beyond".to_string())
        .await;

    expect_attack_rejected(&mut rx, "beyond", AttackRejectReason::OutOfRange);
    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")]
            .bag
            .iter()
            .find(|item| item.item_def_id == "iron_arrow")
            .map(|item| item.quantity),
        Some(20)
    );
}

/// The archer's own choice outranks the strongest — dropping to the cheaper
/// arrow is the whole point of being able to choose.
#[tokio::test]
async fn a_chosen_round_outranks_the_strongest() {
    let game_state = make_test_game_state("bow_honours_choice");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 5), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    game_state
        .monsters
        .write()
        .await
        .insert("far".to_string(), make_monster("far", at(8.0), 0));

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;

    expect_attacked(&mut rx, "far");
    let inventories = game_state.inventories.read().await;
    let bag = &inventories[&pid("archer")].bag;
    let count = |id: &str| {
        bag.iter()
            .find(|item| item.item_def_id == id)
            .map_or(0, |item| item.quantity)
    };
    assert_eq!(count("iron_arrow"), 4, "the chosen pile is the one spent");
    assert_eq!(count("steel_arrow"), 5);
}

/// Running the chosen pile dry drops to the next round rather than reading as
/// an empty quiver, which would be a lie with arrows still in the bag.
#[tokio::test]
async fn an_exhausted_choice_falls_to_the_next_round() {
    let game_state = make_test_game_state("bow_falls_back");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 30),
        &[("iron_arrow", 1), ("steel_arrow", 5)],
    )
    .await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    {
        let mut monsters = game_state.monsters.write().await;
        monsters.insert("far".to_string(), make_monster("far", at(8.0), 0));
        monsters.get_mut("far").unwrap().health = 500;
    }

    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;
    expect_attacked(&mut rx, "far");
    // The cooldown is per player, so the second shot has to wait it out.
    tokio::time::sleep(std::time::Duration::from_millis(
        *super::combat::PLAYER_ATTACK_INTERVAL_MS + 20,
    ))
    .await;
    game_state
        .broadcast_player_attack(&pid("archer"), "far".to_string())
        .await;
    expect_attacked(&mut rx, "far");

    let inventories = game_state.inventories.read().await;
    let inv = &inventories[&pid("archer")];
    assert!(
        !inv.bag.iter().any(|item| item.item_def_id == "iron_arrow"),
        "the iron stack is gone"
    );
    assert_eq!(
        inv.bag
            .iter()
            .find(|item| item.item_def_id == "steel_arrow")
            .map(|item| item.quantity),
        Some(4),
        "the second shot came from the steel stack"
    );
    assert_eq!(inv.active_ammo.as_deref(), Some("steel_arrow"));
}

/// Equipping a bow chooses for an archer who has never chosen, so the first
/// shot does not need a trip through the panel.
#[tokio::test]
async fn equipping_a_bow_loads_the_strongest_round() {
    let game_state = make_test_game_state("bow_equip_loads_ammo");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "bow", 1));
    inv.bag.push(bag_item(2, "iron_arrow", 5));
    inv.bag.push(bag_item(3, "steel_arrow", 5));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("archer"), inv);

    game_state.equip_item(&pid("archer"), 1).await;

    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")].active_ammo.as_deref(),
        Some("steel_arrow")
    );
}

/// A deliberate choice survives an unequip — otherwise "use the cheaper
/// arrow" would be undone by any gear change.
#[tokio::test]
async fn re_equipping_a_bow_keeps_the_chosen_round() {
    let game_state = make_test_game_state("bow_keeps_choice");
    game_state.add_player(make_player("archer", 0.0, 0.5)).await;
    let mut inv: onlinerpg_shared::inventory::PlayerInventory = Default::default();
    inv.bag.push(bag_item(1, "bow", 1));
    inv.bag.push(bag_item(2, "iron_arrow", 5));
    inv.bag.push(bag_item(3, "steel_arrow", 5));
    game_state
        .inventories
        .write()
        .await
        .insert(pid("archer"), inv);

    game_state.equip_item(&pid("archer"), 1).await;
    game_state
        .select_ammo(&pid("archer"), Some("iron_arrow".to_string()))
        .await;
    game_state
        .unequip_item(&pid("archer"), EquipSlot::MainHand)
        .await;
    game_state.equip_item(&pid("archer"), 1).await;

    let inventories = game_state.inventories.read().await;
    assert_eq!(
        inventories[&pid("archer")].active_ammo.as_deref(),
        Some("iron_arrow")
    );
}

// --- Archery skill (doc/COMBAT.md 궁술) ---

use onlinerpg_shared::skills::SkillId;

/// The whole "no animation work" premise of the archery speed bonus: at the
/// level cap the interval must still outlast the bow_shoot clip. Fail this and
/// the shot animation is cut mid-draw and `player_ranged_impact` has to move
/// with it.
#[test]
fn the_capped_archery_cadence_still_outlasts_the_bow_clip() {
    let capped = (*super::combat::PLAYER_ATTACK_INTERVAL_MS as f32
        / onlinerpg_shared::skills::archery_attack_mult(onlinerpg_shared::skills::SKILL_LEVEL_CAP))
    .ceil() as u64;
    assert!(
        capped >= *super::combat::PLAYER_RANGED_CLIP_MS,
        "capped archery interval {capped}ms is under the {}ms bow_shoot clip",
        *super::combat::PLAYER_RANGED_CLIP_MS
    );
}

/// Register `player` with one skill already trained to `level`.
async fn train(game_state: &GameState, player: &PlayerId, skill: SkillId, level: u32) {
    let mut skills = onlinerpg_shared::skills::Skills::default();
    skills.map.insert(
        skill,
        onlinerpg_shared::skills::SkillProgress {
            level,
            xp: onlinerpg_shared::skills::skill_xp_for_level(level),
        },
    );
    game_state.register_player_skills(player, skills).await;
}

/// A tanky target that survives a long string of shots.
async fn training_dummy(game_state: &GameState, id: &str, x: f32) {
    let mut monster = make_monster(id, at(x), 0);
    monster.health = 100_000;
    monster.max_health = 100_000;
    game_state
        .monsters
        .write()
        .await
        .insert(id.to_string(), monster);
}

/// A landed shot banks XP without touching the skills map, and the flush folds
/// the whole window in as one grant with one message.
#[tokio::test]
async fn landed_shots_bank_archery_xp_and_the_flush_pays_it_as_one_grant() {
    let game_state = make_test_game_state("archery_xp_flush");
    let player = pid("archer");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    train(&game_state, &player, SkillId::Archery, 0).await;
    training_dummy(&game_state, "dummy", 8.0).await;

    for _ in 0..3 {
        game_state.last_player_attacks.write().await.remove(&player);
        game_state
            .broadcast_player_attack(&player, "dummy".to_string())
            .await;
    }
    // Still nothing in the map, and nothing on the wire, until the flush.
    assert_eq!(
        game_state.skill_level(&player, SkillId::Archery).await,
        0,
        "banked XP must not reach the skills map before the flush"
    );
    assert!(!drain(&mut rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::SkillXpGained { .. })));

    game_state.tick_archery_xp().await;
    let gains: Vec<_> = drain(&mut rx)
        .into_iter()
        .filter_map(|msg| match msg {
            ServerMessage::SkillXpGained {
                skill, xp_amount, ..
            } => Some((skill, xp_amount)),
            _ => None,
        })
        .collect();
    assert_eq!(
        gains,
        vec![(SkillId::Archery, 3 * super::combat::ARCHERY_XP_PER_HIT)],
        "three shots must arrive as one summed grant"
    );
    // Drained: a second flush with nothing banked says nothing.
    game_state.tick_archery_xp().await;
    assert!(!drain(&mut rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::SkillXpGained { .. })));
}

/// A blade trains nothing, however many times it lands.
#[tokio::test]
async fn melee_hits_never_train_archery() {
    let game_state = make_test_game_state("archery_xp_melee");
    let player = pid("archer");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    train(&game_state, &player, SkillId::Archery, 0).await;
    training_dummy(&game_state, "dummy", 1.5).await;

    for _ in 0..3 {
        game_state.last_player_attacks.write().await.remove(&player);
        game_state
            .broadcast_player_attack(&player, "dummy".to_string())
            .await;
    }
    game_state.tick_archery_xp().await;
    assert!(!drain(&mut rx)
        .iter()
        .any(|msg| matches!(msg, ServerMessage::SkillXpGained { .. })));
}

/// Only hits train. With a +0 attack bonus against guard 10 the d20 splits the
/// shots either way, and the banked total must track the hits exactly — a
/// whiff teaches nothing.
#[tokio::test]
async fn a_missed_shot_trains_nothing() {
    let game_state = make_test_game_state("archery_xp_miss");
    let player = pid("archer");
    let mut rx = setup_archer_with_ammo(
        &game_state,
        "bow",
        attrs_with(10, 10),
        &[("iron_arrow", 200)],
    )
    .await;
    train(&game_state, &player, SkillId::Archery, 0).await;
    training_dummy(&game_state, "dummy", 8.0).await;

    const SHOTS: usize = 40;
    let mut hits = 0;
    for _ in 0..SHOTS {
        game_state.last_player_attacks.write().await.remove(&player);
        game_state
            .broadcast_player_attack(&player, "dummy".to_string())
            .await;
        let (hit, _) = expect_attacked(&mut rx, "dummy");
        hits += u64::from(hit);
    }
    // A 50/50 roll 40 times over: seeing only one outcome is a 2^-40 event and
    // means the roll, not the test, has changed.
    assert!(hits > 0 && hits < SHOTS as u64, "got {hits}/{SHOTS} hits");
    assert_eq!(
        game_state
            .pending_archery_xp
            .read()
            .await
            .get(&player)
            .copied(),
        Some(hits * super::combat::ARCHERY_XP_PER_HIT)
    );
}

/// The damage bonus rides the ranged roll only. A +10 modifier guarantees the
/// hit, so the damage bands are exact: the trained floor lands where the
/// untrained ceiling does, and a blade's band never moves at all.
#[tokio::test]
async fn the_archery_damage_bonus_applies_to_shots_and_not_to_blades() {
    let cap = onlinerpg_shared::skills::SKILL_LEVEL_CAP;
    let bonus = onlinerpg_shared::skills::archery_damage_bonus(cap) as u32;

    let game_state = make_test_game_state("archery_damage_bow");
    let player = pid("archer");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    train(&game_state, &player, SkillId::Archery, cap).await;
    training_dummy(&game_state, "dummy", 8.0).await;
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    let (hit, damage) = expect_attacked(&mut rx, "dummy");
    assert!(hit, "DEX(+10) against guard 10 cannot miss");
    // 1d1 bow + 1d6 arrow + DEX(+10) is 11..=17 untrained; the bonus shifts
    // the whole band up, so the worst trained shot matches the best raw one.
    assert!(
        (12 + bonus..=17 + bonus).contains(&damage),
        "1d1 + 1d6 + DEX(+10) + {bonus}, got {damage}"
    );

    // The same skill with a blade in hand: 1d4 + STR(+10), bonus nowhere.
    let game_state = make_test_game_state("archery_damage_dagger");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    train(&game_state, &player, SkillId::Archery, cap).await;
    training_dummy(&game_state, "dummy", 1.5).await;
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    let (hit, damage) = expect_attacked(&mut rx, "dummy");
    assert!(hit);
    assert!(
        (11..=14).contains(&damage),
        "1d4 + STR(+10) with no archery bonus, got {damage}"
    );
}

/// The shortened window is authoritative, and it is the bow's alone.
#[tokio::test]
async fn a_trained_archer_shoots_inside_the_base_interval_but_not_below_the_floor() {
    let cap = onlinerpg_shared::skills::SKILL_LEVEL_CAP;
    let capped_interval = (*super::combat::PLAYER_ATTACK_INTERVAL_MS as f32
        / onlinerpg_shared::skills::archery_attack_mult(cap))
    .ceil() as u64;

    let game_state = make_test_game_state("archery_cadence");
    let player = pid("archer");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    train(&game_state, &player, SkillId::Archery, cap).await;
    training_dummy(&game_state, "dummy", 8.0).await;

    let fired = |rx: &mut DirectRx| {
        drain(rx)
            .iter()
            .any(|msg| matches!(msg, ServerMessage::PlayerAttacked { .. }))
    };

    // Inside the base interval, but past the archer's own: accepted.
    game_state
        .last_player_attacks
        .write()
        .await
        .insert(player, GameState::now_ms().saturating_sub(capped_interval));
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    assert!(
        fired(&mut rx),
        "a capped archer fires at {capped_interval}ms"
    );

    // A hair under it: refused.
    game_state.last_player_attacks.write().await.insert(
        player,
        GameState::now_ms().saturating_sub(capped_interval - 50),
    );
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    assert!(!fired(&mut rx), "nothing beats the archer's own interval");

    // The same skill, a blade in hand: back to the base cadence.
    let game_state = make_test_game_state("archery_cadence_melee");
    let mut rx = setup_archer(&game_state, "dagger", attrs_with(30, 10)).await;
    train(&game_state, &player, SkillId::Archery, cap).await;
    training_dummy(&game_state, "dummy", 1.5).await;
    game_state
        .last_player_attacks
        .write()
        .await
        .insert(player, GameState::now_ms().saturating_sub(capped_interval));
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    assert!(!fired(&mut rx), "archery must never quicken a blade");
}

/// Hunger and archery are one multiplier: a starving archer is slower than a
/// fed one at the same skill, and faster than a starving beginner.
#[tokio::test]
async fn archery_and_hunger_compose_into_one_interval() {
    let cap = onlinerpg_shared::skills::SKILL_LEVEL_CAP;
    let combined = (*super::combat::PLAYER_ATTACK_INTERVAL_MS as f32
        / (onlinerpg_shared::hunger::WEAK_ATTACK_MULT
            * onlinerpg_shared::skills::archery_attack_mult(cap)))
    .ceil() as u64;

    let game_state = make_test_game_state("archery_hunger");
    let player = pid("archer");
    let mut rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    // setup_archer seeds the character without hunger; re-register it starved.
    game_state
        .register_player_character(&player, 1, 0, attrs_with(10, 30), 0, Some(50))
        .await;
    train(&game_state, &player, SkillId::Archery, cap).await;
    training_dummy(&game_state, "dummy", 8.0).await;

    let fired = |rx: &mut DirectRx| {
        drain(rx)
            .iter()
            .any(|msg| matches!(msg, ServerMessage::PlayerAttacked { .. }))
    };

    game_state
        .last_player_attacks
        .write()
        .await
        .insert(player, GameState::now_ms().saturating_sub(combined - 50));
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    assert!(
        !fired(&mut rx),
        "hunger must still lengthen a trained window"
    );

    game_state
        .last_player_attacks
        .write()
        .await
        .insert(player, GameState::now_ms().saturating_sub(combined));
    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;
    assert!(fired(&mut rx), "the two multipliers compose, not override");
}

/// Logging out mid-window keeps the banked XP: the save rows carry it.
#[tokio::test]
async fn logging_out_flushes_banked_archery_xp_into_the_saved_rows() {
    let game_state = make_test_game_state("archery_logout_flush");
    let player = pid("archer");
    let _rx = setup_archer(&game_state, "bow", attrs_with(10, 30)).await;
    train(&game_state, &player, SkillId::Archery, 0).await;
    training_dummy(&game_state, "dummy", 8.0).await;

    game_state
        .broadcast_player_attack(&player, "dummy".to_string())
        .await;

    let (character_id, rows) = game_state
        .take_player_skills(&player)
        .await
        .expect("archer has skills");
    assert_eq!(character_id, 1);
    let archery = rows
        .iter()
        .find(|row| row.skill_id == "archery")
        .expect("the banked window must survive the logout");
    assert_eq!(archery.xp, super::combat::ARCHERY_XP_PER_HIT);
}
