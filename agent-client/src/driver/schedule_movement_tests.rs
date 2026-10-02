use super::*;
use crate::state::tests::{p, test_player, test_state};
use onlinerpg_shared::messages::MoveStatus;
use onlinerpg_shared::ServerMessage;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

fn walker() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    let (mut s, rx) = test_state();
    let me = test_player(0.5, 0.5);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    (Arc::new(Mutex::new(s)), rx)
}

fn progress(request_id: u32, x: f32, status: MoveStatus) -> ServerMessage {
    ServerMessage::PlayerMoveProgress {
        request_id,
        server_time_ms: 1,
        position: p(x, 1.3, 0.5),
        rotation: 0.0,
        floor_level: 0,
        next_waypoint: 1,
        speed: if status == MoveStatus::Moving {
            3.0
        } else {
            0.0
        },
        status,
    }
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
