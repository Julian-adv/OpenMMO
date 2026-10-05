use super::*;
use crate::state::tests::{house, p, room, test_player, test_state};
use onlinerpg_shared::housing::{WallConfig, WallDirection, WallVariant};
use onlinerpg_shared::interest::{InterestChange, WorldEvent};
use onlinerpg_shared::messages::MoveStatus;
use onlinerpg_shared::ServerMessage;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

fn walker() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    walker_on_floor(0)
}

fn walker_on_floor(floor: i8) -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    let (mut s, rx) = test_state();
    let mut me = test_player(0.5, 0.5);
    me.floor_level = floor;
    me.position.y = 1.3 + floor as f32 * 3.1;
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    s.self_floor_level = floor;
    (Arc::new(Mutex::new(s)), rx)
}

fn progress(request_id: u32, x: f32, status: MoveStatus) -> ServerMessage {
    progress_on_floor(request_id, x, 0, status)
}

fn progress_on_floor(request_id: u32, x: f32, floor: i8, status: MoveStatus) -> ServerMessage {
    ServerMessage::PlayerMoveProgress {
        request_id,
        server_time_ms: 1,
        position: p(x, 1.3 + floor as f32 * 3.1, 0.5),
        rotation: 0.0,
        floor_level: floor,
        next_waypoint: 1,
        speed: if status == MoveStatus::Moving {
            3.0
        } else {
            0.0
        },
        status,
    }
}

async fn add_closed_door(state: &Arc<Mutex<SharedState>>) {
    let s = state.lock().await;
    let mut room = room(1, s.self_floor_level as u8, Default::default());
    room.size_x = 1;
    room.size_z = 1;
    room.wall_east = vec![WallConfig {
        variant: WallVariant::WithDoor,
        texture: 0,
        is_open: false,
    }];
    s.world_cache.write().unwrap().apply_house_event(
        s.self_player_id.unwrap(),
        "test",
        &WorldEvent {
            subject: "house:door-house".into(),
            revision: 1,
            change: InterestChange::Enter,
            messages: vec![ServerMessage::HouseUpdated {
                house: house("door-house", p(0.0, 0.0, 0.0), vec![room]),
            }],
        },
    );
}

async fn next_door_toggle(rx: &mut mpsc::Receiver<ClientMessage>) {
    let command = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("door toggle")
        .expect("open command channel");
    assert!(matches!(
        command,
        ClientMessage::ToggleDoor {
            house_id,
            room_index: 0,
            wall_dir: WallDirection::East,
            segment_index: 0,
        } if house_id == "door-house"
    ));
}

fn spawn_move(
    state: &Arc<Mutex<SharedState>>,
    entry: ScheduleEntry,
) -> tokio::task::JoinHandle<()> {
    let state = Arc::clone(state);
    tokio::spawn(async move { execute_schedule_move(&state, &entry).await })
}

async fn next_goal(rx: &mut mpsc::Receiver<ClientMessage>, expected_x: f32) -> u32 {
    let command = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("movement request")
        .expect("open command channel");
    let ClientMessage::PlayerMoveGoal {
        request_id, x, z, ..
    } = command
    else {
        panic!("expected a movement goal, got {command:?}");
    };
    assert_eq!((x, z), (expected_x, 0.5));
    request_id
}

#[tokio::test(start_paused = true)]
async fn schedule_keeps_the_server_destination_without_relocation_or_retry() {
    for status in [
        MoveStatus::Arrived,
        MoveStatus::Partial,
        MoveStatus::Blocked,
    ] {
        let (state, mut rx) = walker();
        add_closed_door(&state).await;
        let task = spawn_move(
            &state,
            ScheduleEntry {
                pos: [1.5, 99.0, 0.5],
                rotation: 90.0,
                ..Default::default()
            },
        );
        let id = next_goal(&mut rx, 1.5).await;
        state
            .lock()
            .await
            .push_event(progress(id, 1.45, MoveStatus::Moving));
        tokio::time::advance(Duration::from_millis(250)).await;
        tokio::task::yield_now().await;
        assert!(
            !task.is_finished(),
            "wait for server completion even near the goal"
        );
        assert!(rx.try_recv().is_err());
        state.lock().await.push_event(progress(id, 1.1, status));
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::PlayerFace { rotation })
            if rotation == 90_f32.to_radians())
        );
        assert!(
            rx.try_recv().is_err(),
            "no stop, relocation, or repeated goal"
        );
        let s = state.lock().await;
        assert_eq!(s.self_player.as_ref().unwrap().position, p(1.1, 1.3, 0.5));
        assert_eq!(s.relocations, 0);
    }
}

#[tokio::test(start_paused = true)]
async fn consecutive_schedule_moves_ignore_the_previous_completion() {
    let (state, mut rx) = walker();
    let task_state = Arc::clone(&state);
    let task = tokio::spawn(async move {
        for x in [4.5, 8.5] {
            execute_schedule_move(
                &task_state,
                &ScheduleEntry {
                    pos: [x, 0.0, 0.5],
                    ..Default::default()
                },
            )
            .await;
        }
    });
    let first = next_goal(&mut rx, 4.5).await;
    state
        .lock()
        .await
        .push_event(progress(first, 4.5, MoveStatus::Arrived));
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::PlayerFace { .. })
    ));
    let second = next_goal(&mut rx, 8.5).await;
    assert_ne!(first, second);
    state
        .lock()
        .await
        .push_event(progress(first, 4.5, MoveStatus::Arrived));
    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert!(!task.is_finished());
    assert!(rx.try_recv().is_err());
    state
        .lock()
        .await
        .push_event(progress(second, 8.1, MoveStatus::Partial));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
    assert!(rx.try_recv().is_err());
    assert_eq!(
        state.lock().await.self_player.as_ref().unwrap().position,
        p(8.1, 1.3, 0.5)
    );
}

#[tokio::test(start_paused = true)]
async fn furniture_schedule_requests_the_authored_goal_and_uses_the_server_stop() {
    let (state, mut rx) = walker();
    state
        .lock()
        .await
        .world_cache
        .write()
        .unwrap()
        .sync_furniture(
            0,
            0,
            vec![FurniturePlacement {
                id: 23,
                type_id: "bed".into(),
                x: 10.5,
                y: 0.0,
                z: 0.5,
                rotation_deg: 0.0,
                floor_level: 0,
            }],
        );
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [10.5, 0.0, 0.5],
            action: Some("bed".into()),
            object_id: Some(23),
            ..Default::default()
        },
    );
    let id = next_goal(&mut rx, 10.5).await;
    state
        .lock()
        .await
        .push_event(progress(id, 9.5, MoveStatus::Partial));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
    assert!(
        matches!(rx.try_recv(), Ok(ClientMessage::InteractObject { object_type, object_id: 23 })
        if object_type == "bed")
    );
    assert!(rx.try_recv().is_err());
    assert_eq!(
        state.lock().await.self_player.as_ref().unwrap().position,
        p(9.5, 1.3, 0.5)
    );
}

#[tokio::test(start_paused = true)]
async fn distant_or_refused_schedule_moves_do_not_interact_or_relocate() {
    for status in [
        MoveStatus::Arrived,
        MoveStatus::Partial,
        MoveStatus::Blocked,
        MoveStatus::Rejected,
        MoveStatus::Busy,
        MoveStatus::NodeLimit,
        MoveStatus::MapChanged,
        MoveStatus::Stopped,
    ] {
        let (state, mut rx) = walker();
        let task = spawn_move(
            &state,
            ScheduleEntry {
                pos: [10.5, 0.0, 0.5],
                action: Some("bed".into()),
                object_id: Some(23),
                ..Default::default()
            },
        );
        let id = next_goal(&mut rx, 10.5).await;
        state.lock().await.push_event(progress(id, 0.5, status));
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            rx.try_recv().is_err(),
            "{status:?} must not start another action"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn schedule_completion_requires_the_target_floor_even_when_nearby() {
    for status in [
        MoveStatus::Arrived,
        MoveStatus::Partial,
        MoveStatus::Blocked,
    ] {
        let (state, mut rx) = walker();
        let task = spawn_move(
            &state,
            ScheduleEntry {
                pos: [1.5, 4.2, 0.5],
                floor_level: 1,
                action: Some("bed".into()),
                object_id: Some(23),
                ..Default::default()
            },
        );
        let id = next_goal(&mut rx, 1.5).await;
        state.lock().await.push_event(progress(id, 1.1, status));
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(
            rx.try_recv().is_err(),
            "{status:?} on the wrong floor must not start the action"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_partial_intermediate_segment_does_not_complete_the_schedule() {
    let (state, mut rx) = walker();
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [70.5, 0.0, 0.5],
            ..Default::default()
        },
    );
    let id = next_goal(&mut rx, 48.5).await;
    state
        .lock()
        .await
        .push_event(progress(id, 48.1, MoveStatus::Partial));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(rx.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn long_schedule_moves_finish_each_server_segment_before_requesting_the_next() {
    let (state, mut rx) = walker();
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [70.5, 0.0, 0.5],
            ..Default::default()
        },
    );
    let first = next_goal(&mut rx, 48.5).await;
    state
        .lock()
        .await
        .push_event(progress(first, 20.5, MoveStatus::Moving));
    tokio::time::advance(Duration::from_millis(250)).await;
    tokio::task::yield_now().await;
    assert!(
        rx.try_recv().is_err(),
        "an in-flight fixed segment must not be replaced"
    );
    state
        .lock()
        .await
        .push_event(progress(first, 48.5, MoveStatus::Arrived));
    let second = next_goal(&mut rx, 70.5).await;
    state
        .lock()
        .await
        .push_event(progress(second, 70.0, MoveStatus::Arrived));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
    assert!(rx.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn blocked_schedule_opens_a_door_and_resumes_the_target() {
    for (start_floor, target_x, target_floor) in
        [(0, 10.5, 0), (0, 10.5, 1), (1, 10.5, 0), (0, 70.5, 0)]
    {
        for status in [
            MoveStatus::Partial,
            MoveStatus::Blocked,
            MoveStatus::Arrived,
        ] {
            if target_x == 70.5 && status == MoveStatus::Arrived {
                continue;
            }
            let (state, mut rx) = walker_on_floor(start_floor);
            add_closed_door(&state).await;
            let task = spawn_move(
                &state,
                ScheduleEntry {
                    pos: [target_x, 0.0, 0.5],
                    floor_level: target_floor,
                    action: Some("bed".into()),
                    object_id: Some(23),
                    ..Default::default()
                },
            );
            let id = next_goal(&mut rx, target_x.min(48.5)).await;
            state
                .lock()
                .await
                .push_event(progress_on_floor(id, 0.5, start_floor, status));
            let approach = next_goal(&mut rx, 1.5).await;
            state.lock().await.push_event(progress_on_floor(
                approach,
                1.5,
                start_floor,
                MoveStatus::Arrived,
            ));
            next_door_toggle(&mut rx).await;
            state.lock().await.push_event(ServerMessage::DoorToggled {
                house_id: "door-house".into(),
                room_index: 0,
                wall_dir: WallDirection::East,
                segment_index: 0,
                is_open: true,
            });
            let mut id = next_goal(&mut rx, target_x.min(49.5)).await;
            if target_x > 49.5 {
                state
                    .lock()
                    .await
                    .push_event(progress(id, 49.5, MoveStatus::Arrived));
                id = next_goal(&mut rx, target_x).await;
            }
            state.lock().await.push_event(progress_on_floor(
                id,
                target_x - 0.4,
                target_floor as i8,
                MoveStatus::Partial,
            ));
            timeout(Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(
                rx.try_recv(),
                Ok(ClientMessage::PlayerFace { .. })
            ));
            assert!(matches!(
                rx.try_recv(),
                Ok(ClientMessage::InteractObject { object_id: 23, .. })
            ));
            assert!(rx.try_recv().is_err());
            assert_eq!(state.lock().await.relocations, 0);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn schedule_door_retries_are_bounded_when_the_door_stays_closed() {
    let (state, mut rx) = walker();
    add_closed_door(&state).await;
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [10.5, 0.0, 0.5],
            action: Some("bed".into()),
            object_id: Some(23),
            ..Default::default()
        },
    );
    for _ in 0..walk::MAX_DOORS_PER_WALK {
        let id = next_goal(&mut rx, 10.5).await;
        state
            .lock()
            .await
            .push_event(progress(id, 0.5, MoveStatus::Blocked));
        let approach = next_goal(&mut rx, 1.5).await;
        state
            .lock()
            .await
            .push_event(progress(approach, 1.5, MoveStatus::Arrived));
        next_door_toggle(&mut rx).await;
    }
    let id = next_goal(&mut rx, 10.5).await;
    state
        .lock()
        .await
        .push_event(progress(id, 1.5, MoveStatus::Blocked));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(rx.try_recv().is_err());
    assert_eq!(state.lock().await.relocations, 0);
}
