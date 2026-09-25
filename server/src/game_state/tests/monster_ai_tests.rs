//! Server-driven monster brains (doc/SERVER_SIDE_MONSTER_AI.md).
use super::*;

async fn spawn_goblin(game_state: &GameState, x: f32) -> String {
    game_state
        .spawn_monster(
            "goblin".to_string(),
            Position { x, y: 0.0, z: 0.0 },
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            true,
        )
        .await
        .expect("goblin spawns")
        .id
}

async fn monster_x(game_state: &GameState, id: &str) -> f32 {
    game_state.monsters.read().await[id].position.x
}

async fn health_of(game_state: &GameState, player_id: &PlayerId) -> u32 {
    game_state.players.read().await[player_id].health
}

#[tokio::test]
async fn server_brain_chases_and_attacks_the_player() {
    let game_state = make_flat_world_game_state("server_ai_chase");
    let player_id = pid("prey");
    game_state.add_player(make_player("prey", 0.0, 0.0)).await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    let goblin = spawn_goblin(&game_state, 12.0).await;

    let mut attacked = false;
    for _ in 0..60 {
        game_state.tick_monster_ai_by(200.0).await;
        if health_of(&game_state, &player_id).await < 10 {
            attacked = true;
            break;
        }
    }
    let x = monster_x(&game_state, &goblin).await;
    assert!(
        x < 12.0,
        "the brain must have walked the goblin toward the player, x={x}"
    );
    assert!(attacked, "12s of ticks must land at least one goblin hit");
    assert_eq!(game_state.brain_count().await, 1);

    let msgs = drain(&mut rx);
    let moved = msgs
        .iter()
        .any(|m| matches!(m, ServerMessage::MonsterMoved { .. }));
    assert!(moved, "server movement reaches the player: {msgs:?}");
}

#[tokio::test]
async fn brain_is_dropped_when_the_monster_dies() {
    let game_state = make_flat_world_game_state("server_ai_death");
    game_state.add_player(make_player("slayer", 0.0, 0.0)).await;
    let goblin = spawn_goblin(&game_state, 2.0).await;
    game_state.tick_monster_ai_by(200.0).await;
    assert_eq!(game_state.brain_count().await, 1);

    game_state.monsters.write().await.mark_dead(&goblin);
    game_state.tick_monster_ai_by(200.0).await;
    assert_eq!(game_state.brain_count().await, 0, "a corpse keeps no brain");
}

#[tokio::test]
async fn a_hit_monster_retaliates() {
    let game_state = make_flat_world_game_state("server_ai_retaliate");
    let player_id = pid("poker");
    game_state.add_player(make_player("poker", 0.0, 0.0)).await;
    let goblin = game_state
        .spawn_monster(
            "goblin".to_string(),
            Position {
                x: 2.5,
                y: 0.0,
                z: 0.0,
            },
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .expect("goblin spawns")
        .id;
    game_state.tick_monster_ai_by(200.0).await;

    game_state
        .broadcast_player_attack(&player_id, goblin.clone())
        .await;
    let mut attacked = false;
    for _ in 0..40 {
        game_state.tick_monster_ai_by(200.0).await;
        if health_of(&game_state, &player_id).await < 10 {
            attacked = true;
            break;
        }
    }
    assert!(attacked, "a poked brave goblin must swing back within 8s");
}

/// The registry learns a running monster's position only at each network
/// sync, so between syncs it trails the brain. A swing must be judged on
/// where the brain has the monster, or a charge is refused at contact.
#[tokio::test]
async fn a_swing_is_judged_on_the_brain_not_the_synced_registry() {
    let game_state = make_flat_world_game_state("server_ai_swing_at_charger");
    let player_id = pid("swinger");
    game_state
        .add_player(make_player("swinger", 0.0, 0.0))
        .await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    let goblin = spawn_goblin(&game_state, 4.2).await;

    // Melee reach is 2m + 1m tolerance: wait for a tick that leaves the brain
    // inside it while the registry still holds a pose outside it.
    let mut staggered = false;
    for _ in 0..10 {
        game_state.tick_monster_ai_by(200.0).await;
        let brain_x = game_state
            .brain_position_now(&goblin)
            .await
            .expect("the goblin has a brain")
            .x;
        let registry_x = monster_x(&game_state, &goblin).await;
        if brain_x <= 2.9 && registry_x > 3.0 {
            staggered = true;
            break;
        }
    }
    assert!(staggered, "the setup never produced a registry lag to test");
    drain(&mut rx);

    game_state
        .broadcast_player_attack(&player_id, goblin.clone())
        .await;

    let msgs = drain(&mut rx);
    assert!(
        msgs.iter()
            .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })),
        "the swing must land on the brain's position: {msgs:?}"
    );
    assert!(
        !msgs
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerAttackRejected { .. })),
        "{msgs:?}"
    );
}

#[tokio::test]
async fn a_monster_blocked_from_returning_can_be_attacked_and_retaliate() {
    use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};

    const EDGE_N: u8 = 1;
    const EDGE_E: u8 = 2;
    const EDGE_S: u8 = 4;
    const EDGE_W: u8 = 8;

    let game_state = make_flat_world_game_state("server_ai_blocked_return");
    let player_id = pid("lurer");
    game_state.add_player(make_player("lurer", 15.0, 0.0)).await;
    let mut rx = game_state.register_direct_channel(&player_id).await;
    let goblin = spawn_goblin(&game_state, 5.0).await;

    for _ in 0..200 {
        let x = game_state
            .brain_position_now(&goblin)
            .await
            .map_or(5.0, |p| p.x);
        game_state
            .teleport_player(
                &player_id,
                Position {
                    x: x + 10.0,
                    y: 0.0,
                    z: 0.0,
                },
                0.0,
                0,
            )
            .await;
        game_state.tick_monster_ai_by(200.0).await;
        let monsters = game_state.monsters.read().await;
        if monsters[&goblin].position.x > 55.0 && monsters[&goblin].state == MonsterState::Walk {
            break;
        }
    }
    let returning = game_state.monsters.read().await[&goblin].clone();
    assert!(
        returning.position.x > 55.0,
        "the monster must leave its leash"
    );
    assert_eq!(returning.state, MonsterState::Walk);

    let mut cells = vec![0; 80 * 4];
    for x in 0..80 {
        cells[x] |= EDGE_N;
        cells[x + 240] |= EDGE_S;
    }
    for z in 0..4 {
        cells[z * 80] |= EDGE_W;
        cells[z * 80 + 79] |= EDGE_E;
        cells[z * 80 + 49] |= EDGE_E;
        cells[z * 80 + 50] |= EDGE_W;
    }
    game_state.passability_write().insert(
        "closed_return_door".into(),
        RuntimePassability {
            house_origin_x: 0.0,
            house_origin_z: -2.0,
            min_x: 0.0,
            max_x: 80.0,
            min_z: -2.0,
            max_z: 2.0,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 80,
                depth: 4,
                y_base: 0.0,
                wall_height: 3.0,
                cells,
            }],
            stairwells: vec![],
            yields_to_trapped_mover: false,
            allows_projectiles: false,
            is_ground: false,
        },
    );
    game_state.tick_monster_ai_by(600.0).await;
    assert_ne!(
        game_state.monsters.read().await[&goblin].state,
        MonsterState::Walk
    );

    let position = game_state.brain_position_now(&goblin).await.unwrap();
    game_state
        .teleport_player(
            &player_id,
            Position {
                x: position.x + 1.0,
                ..position
            },
            0.0,
            0,
        )
        .await;
    drain(&mut rx);
    game_state
        .broadcast_player_attack(&player_id, goblin.clone())
        .await;
    let messages = drain(&mut rx);
    assert!(messages
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
    assert!(messages
        .iter()
        .all(|m| !matches!(m, ServerMessage::PlayerAttackRejected { .. })));

    let mut retaliated = false;
    for _ in 0..10 {
        game_state.tick_monster_ai_by(200.0).await;
        retaliated |= drain(&mut rx).iter().any(|m| {
            matches!(
                m,
                ServerMessage::MonsterAttackedPlayer { monster_id, player_id: target, .. }
                    if monster_id == &goblin && *target == player_id
            )
        });
    }
    assert!(retaliated, "the trapped monster must fight back");
    assert!(monster_x(&game_state, &goblin).await >= 50.0);
}
