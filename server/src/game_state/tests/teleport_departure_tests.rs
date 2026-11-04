use super::super::teleport_departure::TELEPORT_DEPARTURE;
use super::*;
use onlinerpg_shared::TeleportPhase;
use std::time::Duration;

const RETURN: &str = "scroll_of_return";

struct Scene {
    game: GameState,
    reader: PlayerId,
    reader_rx: DirectRx,
    watcher_rx: DirectRx,
}

async fn scene(label: &str) -> Scene {
    let game = make_flat_world_game_state(label);
    let reader = make_player("Reader", 100.0, 100.0);
    let watcher = make_player("Watcher", 102.0, 100.0);
    let (reader_id, watcher_id) = (reader.id, watcher.id);
    let reader_rx = game.register_direct_channel(&reader_id).await;
    let watcher_rx = game.register_direct_channel(&watcher_id).await;
    game.add_player(reader).await;
    game.add_player(watcher).await;
    game.inventories.write().await.insert(
        reader_id,
        PlayerInventory {
            bag: vec![bag_item(100, RETURN, 2)],
            ..Default::default()
        },
    );
    Scene {
        game,
        reader: reader_id,
        reader_rx,
        watcher_rx,
    }
}

fn phases(rx: &mut DirectRx, of: PlayerId) -> Vec<TeleportPhase> {
    drain(rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::PlayerTeleportEffect {
                player_id, phase, ..
            } if player_id == of => Some(phase),
            _ => None,
        })
        .collect()
}

fn read_return(scene: &Scene) {
    let id = scene.reader;
    scene
        .game
        .depart_then(id, move |game| async move { game.use_item(&id, 100).await });
}

async fn settle(duration: Duration) {
    tokio::time::sleep(duration).await;
    tokio::task::yield_now().await;
}

#[tokio::test(start_paused = true)]
async fn the_server_shows_the_departure_once_then_teleports() {
    let mut s = scene("teleport_departure_waits").await;
    drain(&mut s.watcher_rx);
    let origin = player_xz(&s.game, &s.reader).await;

    read_return(&s);
    settle(Duration::from_millis(100)).await;

    assert_eq!(player_xz(&s.game, &s.reader).await, origin);
    assert_eq!(bag_count(&s.game, &s.reader, RETURN).await, 2);
    assert_eq!(
        phases(&mut s.watcher_rx, s.reader),
        [TeleportPhase::Departing]
    );

    settle(TELEPORT_DEPARTURE).await;

    assert_ne!(player_xz(&s.game, &s.reader).await, origin);
    assert_eq!(bag_count(&s.game, &s.reader, RETURN).await, 1);
    assert!(!phases(&mut s.watcher_rx, s.reader).contains(&TeleportPhase::Departing));
    assert_eq!(
        phases(&mut s.reader_rx, s.reader),
        [TeleportPhase::Arriving]
    );
}

#[tokio::test(start_paused = true)]
async fn moving_during_the_departure_cancels_it_for_everyone() {
    let mut s = scene("teleport_departure_moved").await;
    drain(&mut s.watcher_rx);

    read_return(&s);
    settle(Duration::from_millis(100)).await;
    s.game
        .players
        .write()
        .await
        .get_mut(&s.reader)
        .unwrap()
        .position
        .x += 1.0;
    let moved = player_xz(&s.game, &s.reader).await;
    settle(TELEPORT_DEPARTURE).await;

    assert_eq!(player_xz(&s.game, &s.reader).await, moved);
    assert_eq!(bag_count(&s.game, &s.reader, RETURN).await, 2);
    assert_eq!(
        phases(&mut s.watcher_rx, s.reader),
        [TeleportPhase::Departing, TeleportPhase::Cancelled]
    );
    assert_eq!(
        phases(&mut s.reader_rx, s.reader),
        [TeleportPhase::Cancelled]
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_sent_before_the_request_settles_a_walk_in_time() {
    let s = scene("teleport_departure_mid_walk").await;
    let target = Position {
        x: 130.0,
        y: 0.0,
        z: 100.0,
    };
    let walk = TestMove {
        position: target,
        sprinting: false,
    };
    s.game.request_test_move(&s.reader, walk, false).await;
    s.game.advance_test_movement(1.0).await;
    let walking = player_xz(&s.game, &s.reader).await;
    assert_ne!(walking, (100.0, 100.0));

    s.game.stop_move_goal(s.reader, 2).await;
    read_return(&s);
    settle(Duration::from_millis(10)).await;
    s.game.advance_test_movement(0.5).await;
    settle(TELEPORT_DEPARTURE).await;

    assert_ne!(player_xz(&s.game, &s.reader).await, walking);
    assert_eq!(bag_count(&s.game, &s.reader, RETURN).await, 1);
}

#[tokio::test(start_paused = true)]
async fn a_second_request_during_a_departure_is_ignored() {
    let s = scene("teleport_departure_once").await;

    read_return(&s);
    read_return(&s);
    settle(TELEPORT_DEPARTURE * 2).await;

    assert_eq!(bag_count(&s.game, &s.reader, RETURN).await, 1);
}
