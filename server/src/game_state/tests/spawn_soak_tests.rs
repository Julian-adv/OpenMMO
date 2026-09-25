//! Soak harness: does ambient spawning still work after hours of roaming
//! bots? Walks each bot the distance a 10s tick covers and lets the
//! server's move-coupled spawning (`ambient_spawn.rs`) do the rest.
use super::*;

const TICK_SECONDS: u64 = 10;
const TWO_HOURS_TICKS: u64 = 2 * 3600 / TICK_SECONDS;
/// How far a roaming bot travels between two ticks (~3m/s for 10s).
const ROAM_PER_TICK: f32 = 30.0;
/// The bot chases and kills whatever it can see (EVENT_DELIVERY_RADIUS).
const KILL_RADIUS: f32 = onlinerpg_shared::EVENT_DELIVERY_RADIUS;

fn lcg(seed: &mut u64) -> f32 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    ((*seed >> 33) as f32) / (u32::MAX as f32 / 2.0)
}

/// Monsters on the server, corpses included.
async fn monster_count(game_state: &GameState) -> usize {
    game_state.monsters.read().await.len()
}

/// The bot kills what is in reach; corpse cleanup (combat.rs:535) then removes
/// it. Monsters it has walked away from are left behind, exactly as in play.
async fn kill_monsters_in_reach(game_state: &GameState, player_id: &PlayerId) -> usize {
    let center = game_state.get_all_players().await[player_id].position;
    let mut monsters = game_state.monsters.write().await;
    let doomed: Vec<String> = monsters
        .values()
        .filter(|m| m.position.dist_xz_sq(&center) <= KILL_RADIUS * KILL_RADIUS)
        .map(|m| m.id.clone())
        .collect();
    for id in &doomed {
        monsters.remove(id);
    }
    doomed.len()
}

/// One tick of roaming: turn a little, then walk `ROAM_PER_TICK` — through the
/// real move path, which is what grants spawns.
async fn roam(game_state: &GameState, player_id: &PlayerId, seed: &mut u64, heading: &mut f32) {
    *heading += (lcg(seed) - 1.0) * 0.4;
    let from = game_state.get_all_players().await[player_id].position;
    walk_player_to(
        game_state,
        player_id,
        from.x + heading.cos() * ROAM_PER_TICK,
        from.z + heading.sin() * ROAM_PER_TICK,
    )
    .await;
}

#[tokio::test]
async fn ambient_spawns_survive_two_hours_of_roaming_bots() {
    let game_state = make_flat_world_game_state("spawn_soak");
    let player_id = pid("roaming_bot");
    game_state
        .add_player(make_player("roaming_bot", 0.0, 0.0))
        .await;
    game_state.enable_ambient_spawns();

    let mut seed = 0x5EED_1234u64;
    let mut heading = 0.0f32;
    let mut spawns_by_tick = Vec::new();
    let mut kills_total = 0usize;

    for _ in 0..TWO_HOURS_TICKS {
        game_state.tick_monster_despawns().await;
        // Kill before roaming: a real kill costs a chase plus several swings,
        // so a monster spawned this tick is only reachable on a later one.
        kills_total += kill_monsters_in_reach(&game_state, &player_id).await;
        let before = monster_count(&game_state).await;
        roam(&game_state, &player_id, &mut seed, &mut heading).await;
        spawns_by_tick.push(monster_count(&game_state).await.saturating_sub(before));
    }

    let alive = monster_count(&game_state).await;
    let first_hour: usize = spawns_by_tick[..spawns_by_tick.len() / 2].iter().sum();
    let last_30: usize = spawns_by_tick[spawns_by_tick.len() - 30..].iter().sum();
    println!(
        "spawned in hour 1: {first_hour}, in last 5 min: {last_30}, killed: {kills_total}, alive at end: {alive}"
    );

    assert!(
        last_30 > 0,
        "no ambient monster spawned in the last 5 minutes after 2h of roaming \
         (hour 1 spawned {first_hour}, {alive} monsters still alive)"
    );
}

/// H2 at scale: monsters roaming bots left behind must not survive as
/// permanent monsters beyond every player's view.
#[tokio::test]
async fn abandoned_monsters_do_not_accumulate_across_many_bots() {
    let game_state = make_flat_world_game_state("spawn_soak_global");
    let per_player = world_config().max_nearby_monsters as usize;
    let bots = 40;

    let mut bot_state = Vec::new();
    for i in 0..bots {
        let name = format!("bot{i}");
        let id = pid(&name);
        // Spread the bots far apart so nobody shares another's monsters.
        let (x, z) = (i as f32 * 500.0, i as f32 * 500.0);
        game_state.add_player(make_player(&name, x, z)).await;
        bot_state.push((id, 0.0f32));
    }
    game_state.enable_ambient_spawns();

    let mut seed = 0xB07u64;
    for _ in 0..TWO_HOURS_TICKS {
        game_state.tick_monster_despawns().await;
        for (id, heading) in bot_state.iter_mut() {
            roam(&game_state, id, &mut seed, heading).await;
        }
    }

    let peak = monster_count(&game_state).await;
    let peak_unattended = count_unattended(&game_state).await;

    // Bots stand still. One tick clears everything they walked away from —
    // nothing unattended may remain.
    game_state.tick_monster_despawns().await;
    let alive = monster_count(&game_state).await;
    let unattended = count_unattended(&game_state).await;
    println!(
        "{bots} bots x {per_player}/player: peak {peak} alive ({peak_unattended} \
         unattended) -> after drain {alive} alive ({unattended} unattended)"
    );

    assert_eq!(
        unattended, 0,
        "{unattended}/{alive} monsters have no player inside their AOI yet still \
         hold spawn-cap slots"
    );
}

/// Monsters no player is near — invisible to everyone, yet cap-consuming.
async fn count_unattended(game_state: &GameState) -> usize {
    let monsters = game_state.monsters.read().await;
    let mut unattended = 0;
    for monster in monsters.values() {
        if game_state
            .player_ids_within_position(
                &monster.position,
                monster.floor_level,
                EVENT_DELIVERY_RADIUS,
            )
            .await
            .is_empty()
        {
            unattended += 1;
        }
    }
    unattended
}

/// The point of the redesign: a bot that never moves is never given anything,
/// however long it stands there.
#[tokio::test]
async fn stationary_bot_never_spawns() {
    let game_state = make_flat_world_game_state("spawn_soak_still");
    let player_id = pid("still_bot");
    game_state
        .add_player(make_player("still_bot", 0.0, 0.0))
        .await;
    game_state.enable_ambient_spawns();

    for _ in 0..TWO_HOURS_TICKS {
        game_state
            .tick_player_movement(f32::from(TICK_SECONDS as u16))
            .await;
        kill_monsters_in_reach(&game_state, &player_id).await;
    }
    assert_eq!(
        monster_count(&game_state).await,
        0,
        "standing in one spot must not farm"
    );
}
