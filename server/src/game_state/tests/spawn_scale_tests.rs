//! Scale measurements for the ambient spawn path, sized against the 5,000
//! concurrent-user target. Run with --nocapture to read the timings.
use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const USERS: usize = 5_000;

async fn preload_monsters(
    game_state: &GameState,
    count: usize,
    players: &[PlayerId],
    place: impl Fn(usize, &PlayerId) -> Position,
) {
    let mut monsters = game_state.monsters.write().await;
    for i in 0..count {
        let player = players[i % players.len()];
        let mut monster = make_monster(&format!("pre{i}"), place(i, &player), 0);

        monster.monster_type = "goblin".to_string();
        monsters.insert(monster.id.clone(), monster);
    }
}

fn grid_position(index: usize, _player: &PlayerId) -> Position {
    Position {
        x: (index % 2000) as f32 * 3.0,
        y: 0.0,
        z: (index / 2000) as f32 * 3.0,
    }
}

async fn player_positions(
    game_state: &GameState,
    player_ids: &[PlayerId],
) -> std::collections::HashMap<PlayerId, Position> {
    let players = game_state.players.read().await;
    player_ids
        .iter()
        .map(|id| (*id, players[id].position))
        .collect()
}

async fn add_bots(game_state: &GameState, count: usize) -> Vec<PlayerId> {
    let mut ids = Vec::with_capacity(count);
    for i in 0..count {
        let name = format!("scale_bot{i}");
        let id = pid(&name);
        let player = make_player(&name, (i % 100) as f32 * 60.0, (i / 100) as f32 * 60.0);
        game_state.add_player(player).await;
        ids.push(id);
    }
    ids
}

#[tokio::test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn spawn_path_cost_at_scale() {
    // Populations spanning the target scale; the measured spawns below run
    // the full insert path at each.
    for &population in &[1_000usize, 10_000, 50_000, 134_000] {
        let game_state = make_test_game_state(&format!("scale_{population}"));
        let players = add_bots(&game_state, USERS).await;
        preload_monsters(&game_state, population, &players, grid_position).await;

        let start = Instant::now();
        const SPAWNS: usize = 50;
        for i in 0..SPAWNS {
            game_state
                .spawn_monster(
                    "goblin".to_string(),
                    Position {
                        x: 5.0 + i as f32,
                        y: 0.0,
                        z: 5.0,
                    },
                    0.0,
                    -1,
                    // Slot lifecycle skips the nearby cap, so all 50
                    // spawns succeed and we time the real work.
                    MonsterLifecycle::DungeonSlot,
                    None,
                    false,
                )
                .await
                .expect("dungeon-slot spawns skip the nearby cap");
        }
        let per_spawn = start.elapsed() / SPAWNS as u32;

        // The move-coupled spawn roll, once per moving player per tick: the
        // cost that replaced the old 10s issuance tick.
        let (from, to) = (
            Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            Position {
                x: 0.6,
                y: 0.0,
                z: 0.0,
            },
        );
        let steps: Vec<crate::game_state::ambient_spawn::MoveStep> = players
            .iter()
            .map(|id| crate::game_state::ambient_spawn::MoveStep {
                player_id: *id,
                from,
                to,
                floor_level: 0,
                is_official_npc: false,
                mount: None,
            })
            .collect();
        game_state.enable_ambient_spawns();
        let start = Instant::now();
        game_state.spawn_along_movement(&steps).await;
        let spawn_rolls = start.elapsed() / steps.len() as u32;

        // Compare initial cleanup with a steady population.
        let start = Instant::now();
        game_state.tick_monster_despawns().await;
        let cold_tick = start.elapsed();
        let start = Instant::now();
        game_state.tick_monster_despawns().await;
        let warm_tick = start.elapsed();

        // The per-move fanout is the steady-state cost: every alive monster
        // reports movement roughly once a second.
        let movers: Vec<(String, Position)> = {
            let monsters = game_state.monsters.read().await;
            monsters
                .values()
                .take(200)
                .map(|m| (m.id.clone(), m.position))
                .collect()
        };
        let start = Instant::now();
        for (id, position) in &movers {
            let target = Position {
                x: position.x + 0.5,
                ..*position
            };
            game_state
                .apply_ai_move(id, 0, target, 0.0, MonsterState::Idle, target, None)
                .await;
        }
        let per_move = start.elapsed() / movers.len() as u32;

        println!(
            "{population:>7} monsters / {USERS} users: spawn_monster {per_spawn:>10.2?}  \
             spawn roll {spawn_rolls:>10.2?}  despawn cold {cold_tick:>10.2?} \
             warm {warm_tick:>10.2?}  \
             move {per_move:>10.2?}"
        );
    }
}

#[tokio::test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn player_move_fanout_cost_at_scale() {
    const POPULATION: usize = 134_000;
    /// Players packed into one spot, monsters and all.
    const CROWD: usize = 200;
    let crowd_spot = Position {
        x: 9_000.0,
        y: 0.0,
        z: 9_000.0,
    };

    let game_state = make_test_game_state("player_move_fanout");
    let players = add_bots(&game_state, USERS).await;
    for player in players.iter().take(CROWD) {
        game_state.teleport_player(player, crowd_spot, 0.0, 0).await;
    }
    // Read after the teleports, so a crowd member's monsters land in the crowd
    // with it.
    let positions = player_positions(&game_state, &players).await;
    preload_monsters(&game_state, POPULATION, &players, |_, player| {
        positions[player]
    })
    .await;

    let sparse = pid("sparse_mover");
    game_state
        .add_player(make_player("sparse_mover", 12_000.0, 12_000.0))
        .await;

    for (label, mover) in [
        ("crowd", players[0]),
        ("near monsters", players[CROWD]),
        ("no monsters", sparse),
    ] {
        let origin = game_state.players.read().await[&mover].position;
        const MOVES: usize = 200;
        let start = Instant::now();
        for i in 0..MOVES {
            let position = Position {
                x: origin.x + if i % 2 == 0 { 0.25 } else { -0.25 },
                ..origin
            };
            game_state
                .update_player_position(&mover, move_cmd(position, false), false)
                .await;
        }
        let per_move = start.elapsed() / MOVES as u32;
        println!(
            "{POPULATION} monsters / {USERS} users: player move ({label:>12}) {per_move:>10.2?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn despawn_tick_writer_stall() {
    const POPULATION: usize = 134_000;
    let game_state = make_test_game_state("despawn_stall");
    let players = add_bots(&game_state, USERS).await;
    let positions = player_positions(&game_state, &players).await;
    preload_monsters(&game_state, POPULATION, &players, |_, player| {
        positions[player]
    })
    .await;

    let stop = Arc::new(AtomicBool::new(false));
    let writer_state = game_state.clone();
    let writer_stop = Arc::clone(&stop);
    let writer = tokio::spawn(async move {
        let mut worst = Duration::ZERO;
        while !writer_stop.load(Ordering::Relaxed) {
            let start = Instant::now();
            let mut monsters = writer_state.monsters.write().await;
            worst = worst.max(start.elapsed());
            monsters.mark_dead("no_such_monster");
            drop(monsters);
            tokio::task::yield_now().await;
        }
        worst
    });

    let start = Instant::now();
    game_state.tick_monster_despawns().await;
    let tick = start.elapsed();
    stop.store(true, Ordering::Relaxed);
    let worst = writer.await.expect("writer task finishes");

    assert_eq!(
        game_state.monsters.read().await.len(),
        POPULATION,
        "every monster is attended by its player, so none should be despawned"
    );
    println!(
        "{POPULATION} monsters / {USERS} users: despawn tick {tick:>10.2?}  \
         worst writer wait {worst:>10.2?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn disconnect_cleanup_cost_at_scale() {
    const POPULATION: usize = 134_000;
    let game_state = make_test_game_state("disconnect_cleanup");
    let players = add_bots(&game_state, USERS).await;
    // A nearby player keeps monsters alive after the first departure.
    let leaver = players[0];
    let neighbour = players[1];
    let spot = game_state.players.read().await[&leaver].position;
    game_state.teleport_player(&neighbour, spot, 0.0, 0).await;
    let positions = player_positions(&game_state, &players).await;
    preload_monsters(&game_state, POPULATION, &players, |_, player| {
        positions[player]
    })
    .await;

    let start = Instant::now();
    game_state.remove_player(&leaver).await;
    let retained_call = start.elapsed();
    assert!(game_state.monsters.read().await.alive_near(&spot, 0) > 0);
    let alone = positions[&players[2]];
    let start = Instant::now();
    game_state.remove_player(&players[2]).await;
    let despawn_call = start.elapsed();
    assert_eq!(game_state.monsters.read().await.alive_near(&alone, 0), 0);
    println!("{POPULATION} monsters / {USERS} users: disconnect retained {retained_call:?}, despawn {despawn_call:?}");
}

#[tokio::test]
async fn registry_indexes_track_the_map_through_every_mutation() {
    let game_state = make_test_game_state("registry_indexes");
    let players = add_bots(&game_state, 3).await;

    let mut spawned = Vec::new();
    for i in 0..players.len() {
        for t in ["goblin", "orc"] {
            let monster = game_state
                .spawn_monster(
                    t.to_string(),
                    Position {
                        x: i as f32 * 10.0,
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
                .expect("spawn succeeds under the caps");
            spawned.push(monster.id);
        }
    }

    let audit = |label: &'static str| {
        let game_state = game_state.clone();
        async move {
            let monsters = game_state.monsters.read().await;
            assert!(
                monsters.cell_index_matches_map(),
                "{label}: the cell index drifted from the map"
            );
        }
    };

    audit("after spawns").await;

    // A move that changes cells, then one that does not — the second covers the
    // same-cell skip.
    let moved_to = Position {
        x: 900.0,
        y: 0.0,
        z: 900.0,
    };
    game_state
        .monsters
        .write()
        .await
        .set_position(&spawned[3], moved_to);
    audit("after a move across cells").await;
    game_state.monsters.write().await.set_position(
        &spawned[3],
        Position {
            x: 901.0,
            ..moved_to
        },
    );
    audit("after a move within one cell").await;

    // Inserting over a live key: the old player has to be unfiled before the new
    // one is filed, or a same-player replace drops the id it just re-filed.
    let mut replacement = game_state
        .monsters
        .read()
        .await
        .get(&spawned[3])
        .cloned()
        .expect("the monster is still in the map");
    replacement.position = pos(500.0);
    game_state
        .monsters
        .write()
        .await
        .insert(spawned[3].clone(), replacement.clone());
    audit("after a replacing insert under a new player").await;
    game_state
        .monsters
        .write()
        .await
        .insert(spawned[3].clone(), replacement);
    audit("after a replacing insert under the same player").await;

    game_state.monsters.write().await.mark_dead(&spawned[0]);
    audit("after mark_dead").await;
    // Idempotent: a second kill leaves the indexes alone.
    game_state.monsters.write().await.mark_dead(&spawned[0]);
    audit("after repeat mark_dead").await;

    game_state.monsters.write().await.remove(&spawned[0]);
    audit("after removing the corpse").await;

    game_state.monsters.write().await.remove(&spawned[1]);
    audit("after removing a live monster").await;

    game_state.remove_player(&players[2]).await;
    audit("after disconnect").await;
}
