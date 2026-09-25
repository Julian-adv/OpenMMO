use super::*;

#[tokio::test]
async fn pending_movement_cannot_republish_a_despawned_monster() {
    let game = make_flat_world_game_state("despawn_during_monster_move");
    game.add_player(make_player("watcher", 0.0, 0.0)).await;
    let mut rx = game.register_direct_channel(&pid("watcher")).await;
    let monster = spawn(&game, pos(2.0), 0, MonsterLifecycle::Ambient)
        .await
        .unwrap();
    drain(&mut rx);
    let brains = game.monster_brains.lock().await;
    let moving_game = game.clone();
    let moving_id = monster.id.clone();
    let movement = tokio::spawn(async move {
        moving_game
            .apply_ai_move(
                &moving_id,
                0,
                pos(3.0),
                0.0,
                MonsterState::Walk,
                pos(4.0),
                None,
            )
            .await;
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while game.monsters.read().await[&monster.id].position.x != 3.0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    let removing_game = game.clone();
    let removing_id = monster.id.clone();
    let removal = tokio::spawn(async move {
        removing_game.despawn_monsters(vec![removing_id]).await;
    });
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        while game.monsters.read().await.get(&monster.id).is_some() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    drop(brains);
    movement.await.unwrap();
    removal.await.unwrap();
    assert!(!game
        .interest_lock()
        .has_subject(&format!("monster:{}", monster.id)));
    assert!(
        matches!(drain(&mut rx).last(), Some(ServerMessage::MonsterRemoved { monster_id }) if monster_id == &monster.id)
    );
}

async fn spawn(
    game: &GameState,
    position: Position,
    floor: i8,
    lifecycle: MonsterLifecycle,
) -> Option<crate::types::Monster> {
    game.spawn_monster(
        "goblin".into(),
        position,
        0.0,
        floor,
        lifecycle,
        None,
        false,
    )
    .await
}

#[tokio::test]
async fn nearby_cap_is_shared_and_dead_monsters_free_space() {
    let game = make_test_game_state("nearby_monster_cap");
    game.add_player(make_player("first", 0.0, 0.0)).await;
    game.add_player(make_player("second", 0.0, 0.0)).await;
    let cap = world_config().max_nearby_monsters as usize;
    let mut ids = Vec::new();
    for _ in 0..cap {
        ids.push(
            spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
                .await
                .unwrap()
                .id,
        );
    }
    assert!(spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
        .await
        .is_none());
    assert!(spawn(&game, pos(100.0), 0, MonsterLifecycle::Ambient)
        .await
        .is_some());
    assert!(spawn(&game, pos(0.0), 1, MonsterLifecycle::Ambient)
        .await
        .is_some());
    assert!(spawn(&game, pos(0.0), 0, MonsterLifecycle::DungeonSlot)
        .await
        .is_some());
    game.monsters.write().await.mark_dead(&ids[0]);
    game.monsters.write().await.mark_dead(&ids[1]);
    assert!(spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
        .await
        .is_some());
    assert!(spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
        .await
        .is_none());
}

#[tokio::test]
async fn concurrent_spawns_cannot_exceed_the_nearby_cap() {
    let game = make_test_game_state("concurrent_monster_cap");
    let cap = world_config().max_nearby_monsters;
    for _ in 1..cap {
        spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
            .await
            .unwrap();
    }
    let (a, b) = tokio::join!(
        spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient),
        spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient),
    );
    assert_eq!(usize::from(a.is_some()) + usize::from(b.is_some()), 1);
}

#[tokio::test]
async fn nearby_cap_wraps_at_the_world_seam() {
    let game = make_test_game_state("seam_monster_cap");
    let edge = onlinerpg_shared::WORLD_WIDTH_X / 2.0;
    for _ in 0..world_config().max_nearby_monsters {
        spawn(&game, pos(edge - 1.0), 0, MonsterLifecycle::Ambient)
            .await
            .unwrap();
    }
    assert!(spawn(&game, pos(-edge + 1.0), 0, MonsterLifecycle::Ambient)
        .await
        .is_none());
}

#[tokio::test]
async fn departure_keeps_monsters_until_the_last_viewer_leaves() {
    let game = make_test_game_state("monster_departure");
    game.add_player(make_player("first", 0.0, 0.0)).await;
    game.add_player(make_player("second", 0.0, 0.0)).await;
    let monster = spawn(&game, pos(0.0), 0, MonsterLifecycle::Ambient)
        .await
        .unwrap();
    game.teleport_player(&pid("first"), pos(1000.0), 0.0, 0)
        .await;
    assert!(game.monsters.read().await.get(&monster.id).is_some());
    game.remove_player(&pid("second")).await;
    assert!(game.monsters.read().await.get(&monster.id).is_none());
}

#[tokio::test]
async fn despawn_notifies_every_subscriber_once_and_drops_the_brain() {
    let game = make_flat_world_game_state("monster_removal_delivery");
    game.add_player(make_player("first", 0.0, 0.0)).await;
    game.add_player(make_player("second", 1.0, 0.0)).await;
    let mut first = game.register_direct_channel(&pid("first")).await;
    let mut second = game.register_direct_channel(&pid("second")).await;
    let monster = spawn(&game, pos(2.0), 0, MonsterLifecycle::Ambient)
        .await
        .unwrap();
    game.tick_monster_ai_by(0.0).await;
    assert_eq!(game.brain_count().await, 1);
    drain(&mut first);
    drain(&mut second);
    game.despawn_monsters(vec![monster.id.clone()]).await;
    assert_eq!(game.brain_count().await, 0);
    for messages in [drain(&mut first), drain(&mut second)] {
        assert_eq!(messages.iter().filter(|message| matches!(message, ServerMessage::MonsterRemoved { monster_id } if monster_id == &monster.id)).count(), 1);
    }
}

#[tokio::test]
async fn unattended_cleanup_respects_floor_and_dungeon_lifecycle() {
    let game = make_test_game_state("unattended_monsters");
    game.add_player(make_player("surface", 0.0, 0.0)).await;
    let ambient = spawn(&game, pos(0.0), -1, MonsterLifecycle::Ambient)
        .await
        .unwrap();
    let slot = spawn(&game, pos(0.0), -1, MonsterLifecycle::DungeonSlot)
        .await
        .unwrap();
    game.tick_monster_despawns().await;
    let monsters = game.monsters.read().await;
    assert!(monsters.get(&ambient.id).is_none());
    assert!(monsters.get(&slot.id).is_some());
}

#[tokio::test]
async fn server_movement_despawns_a_monster_that_leaves_every_player() {
    let game = make_flat_world_game_state("wandering_monster_despawn");
    game.add_player(make_player("watcher", 0.0, 0.0)).await;
    let monster = spawn(&game, pos(30.0), 0, MonsterLifecycle::Ambient)
        .await
        .unwrap();
    game.apply_ai_move(
        &monster.id,
        0,
        pos(40.0),
        0.0,
        MonsterState::Walk,
        pos(40.0),
        None,
    )
    .await;
    assert!(game.monsters.read().await.get(&monster.id).is_none());
}
