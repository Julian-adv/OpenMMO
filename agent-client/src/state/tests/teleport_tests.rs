use super::*;
use onlinerpg_shared::messages::{MoveStatus, TeleportPhase};

#[tokio::test(start_paused = true)]
async fn teleport_blocks_foreground_and_background_movement_until_completion() {
    let (mut s, mut rx) = teleport_state("scroll_of_return");
    s.send_command(ClientMessage::UseItem { instance_id: 100 })
        .await
        .unwrap();
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::PlayerMoveStop { .. })
    ));
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::UseItem { instance_id: 100 })
    ));
    assert!(s.teleport_pending());
    assert!(s.request_move(10.5, 0.5, false, None).await.is_err());
    assert!(s
        .send_background_command(ClientMessage::PlayerMoveDirection {
            request_id: 10,
            rotation: 0.0,
            forward: 1,
            turn: 0,
            sprinting: false,
        })
        .await
        .is_err());
    assert!(rx.try_recv().is_err());
    s.send_background_command(ClientMessage::Heartbeat)
        .await
        .unwrap();
    assert!(matches!(rx.recv().await, Some(ClientMessage::Heartbeat)));

    s.push_event(ServerMessage::PlayerTeleported {
        player_id: 1.into(),
        position: p(500.5, 0.0, 500.5),
        rotation: 1.0,
        floor_level: 0,
    });
    assert!(!s.teleport_pending());
    s.request_move(501.5, 500.5, false, None).await.unwrap();
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::PlayerMoveGoal {
            x: 501.5,
            z: 500.5,
            ..
        })
    ));
}

#[tokio::test(start_paused = true)]
async fn only_our_cancellation_unblocks_movement_and_missing_replies_expire() {
    let (mut s, _rx) = teleport_state("scroll_of_return");
    s.send_command(ClientMessage::UseItem { instance_id: 100 })
        .await
        .unwrap();
    for player_id in [2, 1] {
        s.push_event(ServerMessage::PlayerTeleportEffect {
            player_id: player_id.into(),
            position: p(0.5, 0.0, 0.5),
            floor_level: 0,
            phase: TeleportPhase::Cancelled,
        });
        assert_eq!(s.teleport_pending(), player_id == 2);
    }
    s.request_move(1.5, 0.5, false, None).await.unwrap();
    s.send_command(ClientMessage::UseItem { instance_id: 100 })
        .await
        .unwrap();
    assert!(s.teleport_pending());
    tokio::time::advance(TELEPORT_ACK_TIMEOUT).await;
    assert!(!s.teleport_pending());
    s.request_move(2.5, 0.5, false, None).await.unwrap();
}

#[tokio::test]
async fn old_movement_progress_cannot_overwrite_a_teleport() {
    let (mut s, mut rx) = teleport_state("scroll_of_return");
    s.request_move(10.5, 0.5, false, None).await.unwrap();
    let Some(ClientMessage::PlayerMoveGoal { request_id, .. }) = rx.recv().await else {
        panic!("expected move goal");
    };
    s.push_event(ServerMessage::PlayerTeleported {
        player_id: 1.into(),
        position: p(500.5, 0.0, 500.5),
        rotation: 1.0,
        floor_level: 0,
    });
    s.push_event(ServerMessage::PlayerMoveProgress {
        request_id,
        server_time_ms: 1,
        position: p(1.5, 0.0, 0.5),
        rotation: 0.0,
        floor_level: -1,
        next_waypoint: 0,
        speed: 4.0,
        status: MoveStatus::Moving,
    });
    let me = s.self_player.as_ref().unwrap();
    assert_eq!(me.position, p(500.5, 0.0, 500.5));
    assert_eq!(me.rotation, 1.0);
    assert_eq!(s.self_floor_level, 0);
    assert!(s.move_status.is_none());
}

#[test]
fn a_worn_ring_of_teleport_control_says_plain_scrolls_can_aim() {
    let (mut s, _rx) = teleport_state(onlinerpg_shared::bookmark::SCROLL);
    let hint = "lets a plain Scroll of Teleportation go to a bookmark";
    assert!(!s.format_world_state().contains(hint));
    s.self_equipped.insert(
        onlinerpg_shared::inventory::EquipSlot::Ring,
        bag_item(101, "ring_of_teleport_control", 1),
    );
    assert!(s.format_world_state().contains(hint));
}
