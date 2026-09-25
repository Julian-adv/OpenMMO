use super::*;
use crate::dungeon_defs::DungeonEntranceDef;
use onlinerpg_shared::dungeon::GRID;

const DEPTH: u8 = 1;
const FLOOR: i8 = -(DEPTH as i8);

fn inside(entrance: &DungeonEntranceDef) -> Position {
    Position {
        x: entrance.x,
        y: entrance.y - 4.0,
        z: entrance.z,
    }
}

async fn add_occupant(
    game_state: &GameState,
    name: &str,
    entrance: &DungeonEntranceDef,
) -> PlayerId {
    let at = inside(entrance);
    let mut player = make_player(name, at.x, at.z);
    player.position.y = at.y;
    player.floor_level = FLOOR;
    game_state.add_player(player).await;
    let id = pid(name);
    game_state
        .handle_player_floor_change(&id, 0, FLOOR, &at, &at)
        .await;
    id
}

async fn leave_floor(game_state: &GameState, player_id: &PlayerId, entrance: &DungeonEntranceDef) {
    game_state
        .teleport_player(player_id, entrance.position(), 0.0, 0)
        .await;
}

async fn die_and_respawn(game_state: &GameState, player_id: &PlayerId) {
    game_state
        .players
        .write()
        .await
        .get_mut(player_id)
        .expect("the player is on the floor")
        .health = 0;
    game_state.respawn_player(player_id).await;
}

async fn floor_monster_ids(game_state: &GameState, entrance: &DungeonEntranceDef) -> Vec<String> {
    let ids: Vec<String> = {
        let dungeons = game_state.dungeons.read().await;
        dungeons
            .get(&entrance.id)
            .and_then(|rt| rt.floors.get(&DEPTH))
            .map(|fr| {
                fr.slots
                    .iter()
                    .filter_map(|s| s.alive_monster_id.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    assert!(
        !ids.is_empty(),
        "floor {DEPTH} should hold spawn slots to test with"
    );
    ids
}

fn visible_ids(messages: &[ServerMessage]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|message| match message {
            ServerMessage::MonsterSpawned { monster } => Some(monster.id.clone()),
            _ => None,
        })
        .collect()
}

fn removed_ids(msgs: &[ServerMessage]) -> Vec<String> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::MonsterRemoved { monster_id } => Some(monster_id.clone()),
            _ => None,
        })
        .collect()
}

fn assert_removed_exactly(msgs: &[ServerMessage], ids: &[String], context: &str) {
    let mut removed = removed_ids(msgs);
    removed.sort();
    removed.dedup();
    let mut expected = ids.to_vec();
    expected.sort();
    assert_eq!(removed, expected, "{context}");
}

#[tokio::test]
async fn floor_slots_spawn_dungeon_slot_lifecycle_monsters() {
    let game_state = make_test_game_state("dungeon_slot_lifecycle");
    let entrance = first_dungeon(&game_state);
    add_occupant(&game_state, "delver", &entrance).await;
    let ids = floor_monster_ids(&game_state, &entrance).await;

    let monsters = game_state.monsters.read().await;
    for id in &ids {
        assert_eq!(
            monsters.get(id).map(|m| m.lifecycle),
            Some(MonsterLifecycle::DungeonSlot),
            "slot-spawned monster {id} must carry the DungeonSlot lifecycle"
        );
    }
}

#[tokio::test]
async fn leaving_a_still_occupied_floor_tells_the_leaver_its_monsters_are_gone() {
    let game_state = make_test_game_state("dungeon_leave_reassign");
    let entrance = first_dungeon(&game_state);
    let leaver = add_occupant(&game_state, "leaver", &entrance).await;
    add_occupant(&game_state, "stayer", &entrance).await;
    let ids = floor_monster_ids(&game_state, &entrance).await;

    let mut leaver_rx = DirectRx(
        game_state.register_connection_channel(&leaver).await,
        Default::default(),
    );
    let mut stayer_rx = game_state.register_direct_channel(&pid("stayer")).await;
    let visible = visible_ids(&drain(&mut leaver_rx));
    assert!(!visible.is_empty());
    drain(&mut stayer_rx);

    leave_floor(&game_state, &leaver, &entrance).await;

    for id in &ids {
        assert!(game_state.monsters.read().await.get(id).is_some());
    }
    assert_removed_exactly(
        &drain(&mut leaver_rx),
        &visible,
        "the leaver needs MonsterRemoved for every previously visible monster",
    );
    assert!(removed_ids(&drain(&mut stayer_rx)).is_empty());
}

#[tokio::test]
async fn emptying_a_floor_tells_the_leaver_its_monsters_are_gone() {
    let game_state = make_test_game_state("dungeon_leave_despawn");
    let entrance = first_dungeon(&game_state);
    let leaver = add_occupant(&game_state, "solo", &entrance).await;
    let ids = floor_monster_ids(&game_state, &entrance).await;

    let mut leaver_rx = DirectRx(
        game_state.register_connection_channel(&leaver).await,
        Default::default(),
    );
    let visible = visible_ids(&drain(&mut leaver_rx));
    assert!(!visible.is_empty());

    leave_floor(&game_state, &leaver, &entrance).await;

    for id in &ids {
        assert!(
            game_state.monsters.read().await.get(id).is_none(),
            "monster {id} should be gone from the registry"
        );
    }
    assert_removed_exactly(
        &drain(&mut leaver_rx),
        &visible,
        "the leaver needs MonsterRemoved for every despawned monster",
    );
    assert!(
        game_state.dungeon_monsters.read().await.is_empty(),
        "the slot index must not outlive the monsters it points at"
    );
}

#[tokio::test]
async fn a_floor_mate_out_of_range_keeps_the_monsters_alive() {
    let game_state = make_test_game_state("dungeon_leave_far_side");
    let entrance = first_dungeon(&game_state);
    let leaver = add_occupant(&game_state, "leaver", &entrance).await;
    let far = add_occupant(&game_state, "farside", &entrance).await;
    let ids = floor_monster_ids(&game_state, &entrance).await;

    // Push the remaining occupant past the event radius from every monster,
    // where the AOI rule would see nobody at all.
    let positions: Vec<Position> = {
        let monsters = game_state.monsters.read().await;
        ids.iter()
            .filter_map(|id| monsters.get(id).map(|m| m.position))
            .collect()
    };
    let away = Position {
        x: entrance.x + GRID as f32,
        z: entrance.z + GRID as f32,
        ..inside(&entrance)
    };
    for p in &positions {
        let d = ((away.x - p.x).powi(2) + (away.z - p.z).powi(2)).sqrt();
        assert!(
            d > EVENT_DELIVERY_RADIUS,
            "the far-side player must be outside every monster's AOI, got {d}m"
        );
    }
    game_state.teleport_player(&far, away, 0.0, FLOOR).await;

    leave_floor(&game_state, &leaver, &entrance).await;

    game_state.tick_monster_despawns().await;
    for id in &ids {
        assert!(game_state.monsters.read().await.get(id).is_some());
    }
}

#[tokio::test]
async fn dying_beside_a_party_member_clears_watched_monsters() {
    let game_state = make_test_game_state("dungeon_leave_party_respawn");
    let entrance = first_dungeon(&game_state);
    let leaver = add_occupant(&game_state, "leaver", &entrance).await;
    add_occupant(&game_state, "stayer", &entrance).await;
    let mut leaver_rx = DirectRx(
        game_state.register_connection_channel(&leaver).await,
        Default::default(),
    );
    let visible = visible_ids(&drain(&mut leaver_rx));
    assert!(!visible.is_empty());

    die_and_respawn(&game_state, &leaver).await;

    let removed = removed_ids(&drain(&mut leaver_rx));
    for id in &visible {
        assert!(
            removed.contains(id),
            "respawning off a shared floor must clear {id}; got {removed:?}"
        );
    }
}

#[tokio::test]
async fn dying_alone_on_a_floor_clears_the_monsters_the_exit_despawned() {
    let game_state = make_test_game_state("dungeon_leave_solo_respawn");
    let entrance = first_dungeon(&game_state);
    let leaver = add_occupant(&game_state, "solo", &entrance).await;
    let ids = floor_monster_ids(&game_state, &entrance).await;

    let mut leaver_rx = DirectRx(
        game_state.register_connection_channel(&leaver).await,
        Default::default(),
    );
    let visible = visible_ids(&drain(&mut leaver_rx));
    assert!(!visible.is_empty());

    die_and_respawn(&game_state, &leaver).await;

    assert!(ids
        .iter()
        .all(|id| game_state.monsters.try_read().unwrap().get(id).is_none()));
    let removed = removed_ids(&drain(&mut leaver_rx));
    for id in &visible {
        assert!(
            removed.contains(id),
            "respawning out of a dungeon must clear {id}; got {removed:?}"
        );
    }
}

#[tokio::test]
async fn a_slain_boss_does_not_return_when_the_floor_empties() {
    let game_state = make_test_game_state("boss_slot_holds");
    let entrance = first_dungeon(&game_state);
    let player_id = add_occupant(&game_state, "Delver", &entrance).await;

    // A boss slot on this floor, freshly slain.
    {
        let mut dungeons = game_state.dungeons.write().await;
        let floor = dungeons
            .get_mut(&entrance.id)
            .and_then(|rt| rt.floors.get_mut(&DEPTH))
            .expect("entered floor");
        floor.slots.push(super::dungeon::SpawnSlot {
            alive_monster_id: None,
            respawn_at_ms: super::dungeon::BOSS_RESPAWN_NEVER,
            is_boss: true,
        });
    }

    leave_floor(&game_state, &player_id, &entrance).await;

    let dungeons = game_state.dungeons.read().await;
    let floor = dungeons
        .get(&entrance.id)
        .and_then(|rt| rt.floors.get(&DEPTH))
        .expect("floor runtime outlives its occupants");
    let boss = floor.slots.iter().find(|s| s.is_boss).expect("boss slot");
    assert_eq!(
        boss.respawn_at_ms,
        super::dungeon::BOSS_RESPAWN_NEVER,
        "an empty floor must not free the guardian's slot"
    );
}

#[tokio::test]
async fn emptying_a_floor_keeps_slain_slots_on_their_respawn_timer() {
    let game_state = make_test_game_state("slain_slot_keeps_timer");
    let entrance = first_dungeon(&game_state);
    let player_id = add_occupant(&game_state, "Delver", &entrance).await;

    let far_future = GameState::now_ms() + 10 * 60 * 1000;
    {
        let mut dungeons = game_state.dungeons.write().await;
        let floor = dungeons
            .get_mut(&entrance.id)
            .and_then(|rt| rt.floors.get_mut(&DEPTH))
            .expect("entered floor");
        let slain = floor
            .slots
            .iter_mut()
            .find(|s| s.alive_monster_id.is_some())
            .expect("entry populated at least one slot");
        slain.alive_monster_id = None;
        slain.respawn_at_ms = far_future;
    }

    leave_floor(&game_state, &player_id, &entrance).await;

    let dungeons = game_state.dungeons.read().await;
    let floor = dungeons
        .get(&entrance.id)
        .and_then(|rt| rt.floors.get(&DEPTH))
        .expect("floor runtime outlives its occupants");
    assert!(
        floor.slots.iter().all(|s| s.alive_monster_id.is_none()),
        "a living monster despawns with its floor"
    );
    assert_eq!(
        floor
            .slots
            .iter()
            .filter(|s| s.respawn_at_ms == far_future)
            .count(),
        1,
        "the slain slot must keep its respawn timer across the exit"
    );
}
