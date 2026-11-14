use super::*;
use crate::game_state::{
    tests::{make_flat_world_game_state, make_player},
    GameState,
};
use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};

async fn enclosed_target(name: &str, count: usize) -> (GameState, PlayerId, Vec<String>) {
    let game = make_flat_world_game_state(name);
    let player = make_player("prey", 4.25, 1.45);
    let id = player.id;
    game.add_player(player).await;
    let mut cells = vec![0; 10 * 7];
    for x in 0..10 {
        cells[x] |= 1;
        cells[x + 60] |= 4;
    }
    for z in 0..7 {
        cells[z * 10] |= 8;
        cells[z * 10 + 9] |= 2;
    }
    game.passability_write().insert(
        "enclosure".into(),
        RuntimePassability {
            house_origin_x: 0.0,
            house_origin_z: 0.0,
            min_x: 0.0,
            max_x: 10.0,
            min_z: 0.0,
            max_z: 7.0,
            floors: vec![RuntimeFloorGrid {
                floor_level: 0,
                origin_x: 0,
                origin_z: 0,
                width: 10,
                depth: 7,
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
    let mut ids = vec![];
    for i in 0..count {
        let position = Position {
            x: 7.5 + i as f32,
            y: 0.0,
            z: -0.5,
        };
        let monster = game
            .spawn_monster(
                "hobgoblin".into(),
                position,
                0.0,
                0,
                crate::types::MonsterLifecycle::Ambient,
                None,
                false,
            )
            .await
            .unwrap();
        let mut brain = game.new_brain(&monster);
        brain.handle_hit_with_behavior_tree(&id, false, 0);
        game.monster_brains
            .lock()
            .await
            .entries
            .insert(monster.id.clone(), Entry::new(brain, Instant::now()));
        ids.push(monster.id);
    }
    ids.sort_unstable();
    (game, id, ids)
}

#[tokio::test]
async fn unchanged_holds_reuse_failures_and_geometry_changes_resume_immediately() {
    let (game, target, ids) = enclosed_target("ai_failure_reuse", 1).await;
    game.teleport_player(
        &target,
        Position {
            x: 4.25,
            y: 0.0,
            z: 4.45,
        },
        0.0,
        0,
    )
    .await;
    for _ in 0..30 {
        game.tick_monster_ai_by(200.0).await;
    }
    let (queries, hits, position) = {
        let brains = game.monster_brains.lock().await;
        assert_eq!(brains.entries[&ids[0]].brain.state(), AiState::Hold);
        (
            brains.stats.pathfinds,
            brains.stats.cache_hits,
            brains.entries[&ids[0]].brain.position,
        )
    };
    for _ in 0..25 {
        game.tick_monster_ai_by(200.0).await;
    }
    {
        let brains = game.monster_brains.lock().await;
        assert_eq!(brains.stats.pathfinds, queries);
        assert!(brains.stats.cache_hits > hits);
        assert_eq!(
            brains.entries[&ids[0]].brain.position.dist_xz_sq(&position),
            0.0
        );
    }
    game.passability_write().remove("enclosure");
    game.tick_monster_ai_by(200.0).await;
    let brains = game.monster_brains.lock().await;
    assert!(brains.stats.pathfinds > queries);
    assert!(brains.entries[&ids[0]].brain.position.dist_xz_sq(&position) > 0.0);
    assert_eq!(brains.entries[&ids[0]].brain.state(), AiState::Chase);
}

#[tokio::test]
async fn a_moving_target_resumes_a_cached_failed_chase() {
    let (game, target, ids) = enclosed_target("ai_failure_target_moved", 1).await;
    for _ in 0..30 {
        game.tick_monster_ai_by(200.0).await;
    }
    let position = game.monster_brains.lock().await.entries[&ids[0]]
        .brain
        .position;
    game.teleport_player(
        &target,
        Position {
            x: 7.5,
            y: 0.0,
            z: -4.0,
        },
        0.0,
        0,
    )
    .await;
    game.tick_monster_ai_by(200.0).await;
    let brains = game.monster_brains.lock().await;
    assert!(brains.entries[&ids[0]].brain.position.dist_xz_sq(&position) > 0.0);
    assert_eq!(brains.entries[&ids[0]].brain.state(), AiState::Chase);
}

#[tokio::test]
async fn holds_keep_their_failures_through_edits_elsewhere() {
    let (game, _, ids) = enclosed_target("ai_failures_survive_far_edits", 4).await;
    for _ in 0..30 {
        game.tick_monster_ai_by(200.0).await;
    }
    let queries = game.monster_brains.lock().await.stats.pathfinds;
    let dungeon_floor = onlinerpg_shared::dungeon::DUNGEON_FLOOR_INDEX_BASE;
    for (key, entry) in [
        ("far", path_budget::furniture_piece(300, 1, 0)),
        ("crypt", path_budget::furniture_piece(4, 1, dungeon_floor)),
    ] {
        game.passability_write().insert(key.into(), entry);
        for _ in 0..5 {
            game.tick_monster_ai_by(200.0).await;
        }
    }
    let brains = game.monster_brains.lock().await;
    assert_eq!(brains.stats.pathfinds, queries);
    for id in &ids {
        assert_eq!(brains.entries[id].brain.state(), AiState::Hold);
    }
}
