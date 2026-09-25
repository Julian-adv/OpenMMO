use super::*;
use crate::game_state::dungeon::BOSS_RESPAWN_NEVER;
use onlinerpg_shared::dungeon::floor_world_y;

const DUNGEON: &str = "ogre_stronghold";

async fn enter_floor(game_state: &GameState, name: &str, entrance_id: &str, depth: u8) -> PlayerId {
    let entrance = game_state.dungeon_defs.get(entrance_id).expect("dungeon");
    let mut player = make_player(name, entrance.x, entrance.z);
    player.position.y = floor_world_y(entrance.y, depth);
    player.floor_level = -(depth as i8);
    let id = player.id;
    let at = player.position;
    game_state.add_player(player).await;
    game_state
        .handle_player_floor_change(&id, 0, -(depth as i8), &at, &at)
        .await;
    id
}

async fn kill_monster(
    game_state: &GameState,
    depth: u8,
    is_boss: bool,
    killer: &PlayerId,
) -> (usize, u64) {
    let (id, slot) = {
        let index = game_state.dungeon_monsters.read().await;
        index
            .iter()
            .find_map(|(id, entry)| {
                (entry.entrance_id == DUNGEON && entry.depth == depth && entry.is_boss == is_boss)
                    .then(|| (id.clone(), entry.slot))
            })
            .expect("spawned monster")
    };
    game_state.monsters.write().await.mark_dead(&id);
    game_state.on_dungeon_monster_dead(&id, killer, None).await;
    let dungeons = game_state.dungeons.read().await;
    let state = &dungeons[DUNGEON].floors[&depth].slots[slot];
    assert!(state.alive_monster_id.is_none());
    (slot, state.respawn_at_ms)
}

async fn assert_respawn_delay(
    game_state: &GameState,
    depth: u8,
    killer: &PlayerId,
    expected_ms: u64,
) -> (usize, u64) {
    let before = GameState::now_ms();
    let (slot, deadline) = kill_monster(game_state, depth, false, killer).await;
    let after = GameState::now_ms();
    assert!(
        (before + expected_ms..=after + expected_ms).contains(&deadline),
        "depth {depth}: expected a {expected_ms}ms delay, got deadline {deadline} after {before}"
    );
    (slot, deadline)
}

#[tokio::test]
async fn respawn_delay_scales_with_living_population_and_caps_at_five() {
    for (count, expected_ms) in [
        300_000, 300_000, 240_000, 180_000, 150_000, 120_000, 120_000,
    ]
    .into_iter()
    .enumerate()
    {
        let game_state = make_test_game_state(&format!("respawn_population_{count}"));
        let killer = enter_floor(&game_state, "killer", DUNGEON, 1).await;
        if count == 0 {
            game_state
                .players
                .write()
                .await
                .get_mut(&killer)
                .expect("killer")
                .health = 0;
        }
        for i in 1..count {
            enter_floor(&game_state, &format!("delver{i}"), DUNGEON, 1).await;
        }
        assert_respawn_delay(&game_state, 1, &killer, expected_ms).await;
    }
}

#[tokio::test]
async fn respawn_population_excludes_other_bands_dungeons_and_dead_players() {
    let game_state = make_test_game_state("respawn_band_boundaries");
    let mut occupants = HashMap::new();
    for depth in [1, 5, 6, 7, 10, 11, 12, 13, 15] {
        let id = enter_floor(&game_state, &format!("depth{depth}"), DUNGEON, depth).await;
        occupants.insert(depth, id);
    }
    let dead = enter_floor(&game_state, "dead", DUNGEON, 1).await;
    game_state
        .players
        .write()
        .await
        .get_mut(&dead)
        .expect("dead occupant")
        .health = 0;
    enter_floor(&game_state, "elsewhere", "old_crypt", 1).await;

    for (depth, expected_ms) in [
        (1, 240_000),
        (5, 240_000),
        (6, 180_000),
        (10, 180_000),
        (11, 150_000),
        (15, 150_000),
    ] {
        assert_respawn_delay(&game_state, depth, &occupants[&depth], expected_ms).await;
    }
}

#[tokio::test]
async fn scheduled_respawn_survives_population_changes_and_reentry() {
    let game_state = make_test_game_state("respawn_fixed_deadline");
    let killer = enter_floor(&game_state, "killer", DUNGEON, 1).await;
    let companion = enter_floor(&game_state, "companion", DUNGEON, 5).await;
    let (slot, deadline) = assert_respawn_delay(&game_state, 1, &killer, 240_000).await;

    let entrance = game_state.dungeon_defs.get(DUNGEON).expect("dungeon");
    let next_band = Position {
        y: floor_world_y(entrance.y, 6),
        ..entrance.position()
    };
    game_state
        .teleport_player(&companion, next_band, 0.0, -6)
        .await;
    game_state.tick_dungeons().await;
    assert_eq!(
        game_state.dungeons.read().await[DUNGEON].floors[&1].slots[slot].respawn_at_ms,
        deadline
    );

    let arrival = enter_floor(&game_state, "arrival", DUNGEON, 1).await;
    let another = enter_floor(&game_state, "another", DUNGEON, 1).await;
    game_state.tick_dungeons().await;
    assert_eq!(
        game_state.dungeons.read().await[DUNGEON].floors[&1].slots[slot].respawn_at_ms,
        deadline
    );

    for player in [killer, arrival, another] {
        game_state.remove_player(&player).await;
    }
    enter_floor(&game_state, "returning", DUNGEON, 1).await;
    game_state.tick_dungeons().await;
    let dungeons = game_state.dungeons.read().await;
    let state = &dungeons[DUNGEON].floors[&1].slots[slot];
    assert_eq!(state.respawn_at_ms, deadline);
    assert!(state.alive_monster_id.is_none());
}

#[tokio::test]
async fn crowded_boss_floor_stays_cleared_after_reentry() {
    let game_state = make_test_game_state("respawn_crowded_boss");
    let killer = enter_floor(&game_state, "killer", DUNGEON, 15).await;
    let mut occupants = vec![killer];
    for depth in 11..=14 {
        occupants.push(enter_floor(&game_state, &format!("depth{depth}"), DUNGEON, depth).await);
    }
    assert_respawn_delay(&game_state, 15, &killer, 120_000).await;
    let (slot, deadline) = kill_monster(&game_state, 15, true, &killer).await;
    assert_eq!(deadline, BOSS_RESPAWN_NEVER);

    for player in occupants {
        game_state.remove_player(&player).await;
    }
    enter_floor(&game_state, "returning", DUNGEON, 15).await;
    game_state.tick_dungeons().await;
    let dungeons = game_state.dungeons.read().await;
    let boss = &dungeons[DUNGEON].floors[&15].slots[slot];
    assert_eq!(boss.respawn_at_ms, BOSS_RESPAWN_NEVER);
    assert!(boss.alive_monster_id.is_none());
}
