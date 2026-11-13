use super::*;
use crate::state::tests::{house, p, room, test_player, test_state};
use onlinerpg_shared::door::DoorId;
use onlinerpg_shared::housing::{WallConfig, WallDirection, WallVariant};
use onlinerpg_shared::interest::{InterestChange, WorldEvent};
use onlinerpg_shared::messages::MoveStatus;
use onlinerpg_shared::ServerMessage;
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};

async fn combat_walker() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    let (state, rx) = walker();
    {
        let mut s = state.lock().await;
        let me = s.self_player.as_mut().unwrap();
        me.name = "Ysolde".into();
        me.is_official_npc = true;
        s.nearby_monsters
            .insert("m21".into(), crate::state::tests::monster("m21"));
    }
    (state, rx)
}

async fn start_combat(state: &Arc<Mutex<SharedState>>) {
    let mut s = state.lock().await;
    let player = s.self_player.as_ref().unwrap();
    let event = ServerMessage::MonsterAttackedPlayer {
        monster_id: "m21".into(),
        player_id: player.id,
        hit: false,
        roll: 1,
        damage: 0,
        current_health: player.health,
        ranged: false,
    };
    s.push_event(event);
}

async fn end_combat(state: &Arc<Mutex<SharedState>>) {
    state
        .lock()
        .await
        .push_event(ServerMessage::MonsterRemoved {
            monster_id: "m21".into(),
        });
}

#[tokio::test(start_paused = true)]
async fn combat_stops_a_patrol_and_resumes_its_destination_when_safe() {
    let (state, mut rx) = combat_walker().await;
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [8.5, 1.3, 0.5],
            ..Default::default()
        },
    );
    next_goal(&mut rx, 8.5).await;
    start_combat(&state).await;
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::PlayerMoveStop { .. })
    ));
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert!(rx.try_recv().is_err());
    end_combat(&state).await;
    let request = next_goal(&mut rx, 8.5).await;
    state
        .lock()
        .await
        .push_event(progress(request, 8.5, MoveStatus::Arrived));
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
async fn combat_interrupts_excavation_and_holds_the_next_scheduled_move() {
    let (state, mut rx) = combat_walker().await;
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [8.5, 1.3, 0.5],
            waypoints: vec![[0.5, 1.3, 0.5]],
            waypoint_activities: vec![onlinerpg_shared::schedule::WaypointActivity {
                waypoint: 0,
                emote: "excavate".into(),
                duration_secs: 18,
                rotation: 90.0,
                radius: 0.0,
                probability: 1.0,
            }],
            ..Default::default()
        },
    );
    assert!(matches!(
        rx.recv().await,
        Some(ClientMessage::PlayerFace { .. })
    ));
    assert!(
        matches!(rx.recv().await, Some(ClientMessage::ChatMessage { message }) if message == "/emote excavate")
    );
    start_combat(&state).await;
    assert!(matches!(
        timeout(Duration::from_secs(1), rx.recv()).await.unwrap(),
        Some(ClientMessage::StopInteraction)
    ));
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert!(rx.try_recv().is_err());
    end_combat(&state).await;
    let request = next_goal(&mut rx, 8.5).await;
    state
        .lock()
        .await
        .push_event(progress(request, 8.5, MoveStatus::Arrived));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn excavation_waits_for_arrival_holds_the_pose_and_stops_before_walking() {
    for status in [MoveStatus::Arrived, MoveStatus::Blocked] {
        let (state, mut rx) = walker();
        let state2 = Arc::clone(&state);
        let task = tokio::spawn(async move {
            let entry = ScheduleEntry {
                waypoints: vec![[5.5, 1.3, 0.5]],
                ..Default::default()
            };
            let activity = onlinerpg_shared::schedule::WaypointActivity {
                waypoint: 0,
                emote: "excavate".into(),
                duration_secs: 10,
                rotation: 90.0,
                radius: 0.0,
                probability: 1.0,
            };
            perform_waypoint_activity(&state2, &entry, &activity).await;
        });
        let request = next_goal(&mut rx, 5.5).await;
        state.lock().await.push_event(progress(
            request,
            if status == MoveStatus::Arrived {
                5.5
            } else {
                0.5
            },
            status,
        ));
        if status == MoveStatus::Arrived {
            assert!(matches!(
                rx.recv().await,
                Some(ClientMessage::PlayerFace { .. })
            ));
            assert!(
                matches!(rx.recv().await, Some(ClientMessage::ChatMessage { message }) if message == "/emote excavate")
            );
            tokio::time::advance(Duration::from_secs(9)).await;
            assert!(rx.try_recv().is_err());
            tokio::time::advance(Duration::from_secs(1)).await;
            assert!(matches!(
                rx.recv().await,
                Some(ClientMessage::StopInteraction)
            ));
        }
        timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap();
        assert!(rx.try_recv().is_err());
    }
}

#[test]
fn excavation_spots_vary_within_the_camp_radius_and_can_be_skipped() {
    use rand::SeedableRng;
    let mut file: serde_json::Value =
        serde_json::from_str(include_str!("../../data/npcs/ysolde/schedule.json")).unwrap();
    let entry: ScheduleEntry = serde_json::from_value(file["schedule"][0].take()).unwrap();
    let mut rng = rand::rngs::StdRng::seed_from_u64(1024);
    for activity in &entry.waypoint_activities {
        let anchor = entry.waypoints[activity.waypoint];
        let mut chosen = Vec::new();
        let mut skipped = 0;
        for _ in 0..100 {
            match activity_destination(&entry, activity, &mut rng) {
                Some((x, z)) => {
                    assert!(
                        (x - anchor[0]).powi(2) + (z - anchor[2]).powi(2)
                            <= (activity.radius + 0.001).powi(2)
                    );
                    chosen.push((x, z));
                }
                None => skipped += 1,
            }
        }
        assert!(skipped > 0 && chosen.len() > 1);
        assert!(chosen.windows(2).any(|pair| pair[0] != pair[1]));
    }
}

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
        ClientMessage::ToggleDoor { door } if door == "house:door-house:0:east:0"
    ));
}

fn spawn_move(
    state: &Arc<Mutex<SharedState>>,
    entry: ScheduleEntry,
) -> tokio::task::JoinHandle<()> {
    let state = Arc::clone(state);
    tokio::spawn(async move { execute_schedule_move(&state, &entry).await })
}

async fn goal(rx: &mut mpsc::Receiver<ClientMessage>) -> (u32, f32, f32) {
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
    (request_id, x, z)
}

async fn next_goal(rx: &mut mpsc::Receiver<ClientMessage>, expected_x: f32) -> u32 {
    let (request_id, x, z) = goal(rx).await;
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
                open_doors: 0,
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
            state.lock().await.push_event(
                DoorId::house("door-house", 0, WallDirection::East, 0).state(Some(true)),
            );
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

fn progress_at(request_id: u32, x: f32, z: f32, status: MoveStatus) -> ServerMessage {
    ServerMessage::PlayerMoveProgress {
        request_id,
        server_time_ms: 1,
        position: p(x, 1.3, z),
        rotation: 0.0,
        floor_level: 0,
        next_waypoint: 1,
        speed: 0.0,
        status,
    }
}

/// The door stands on the far side of the building, so its inner side is the
/// nearer one in a straight line.
async fn opens_a_door_from_behind_its_building(
    state: Arc<Mutex<SharedState>>,
    mut rx: mpsc::Receiver<ClientMessage>,
    start: (f32, f32),
    stalled: (f32, f32),
    target: (f32, f32),
    outside: (f32, f32),
    door: DoorId,
) {
    {
        let mut s = state.lock().await;
        let me = s.self_player.as_mut().unwrap();
        me.position = p(start.0, 1.3, start.1);
    }
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [target.0, 1.3, target.1],
            ..Default::default()
        },
    );
    let (id, x, z) = goal(&mut rx).await;
    assert_eq!((x, z), target);
    state
        .lock()
        .await
        .push_event(progress_at(id, stalled.0, stalled.1, MoveStatus::Partial));
    let (id, x, z) = goal(&mut rx).await;
    assert!(
        (x - outside.0).abs() < 0.01 && (z - outside.1).abs() < 0.01,
        "approached ({x}, {z})"
    );
    state
        .lock()
        .await
        .push_event(progress_at(id, x, z, MoveStatus::Arrived));
    let command = timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("door toggle")
        .expect("open command channel");
    assert!(matches!(
        command,
        ClientMessage::ToggleDoor { door: toggled } if toggled == door.to_string()
    ));
    task.abort();
}

async fn walker_by_the_chapel() -> (Arc<Mutex<SharedState>>, mpsc::Receiver<ClientMessage>) {
    use onlinerpg_shared::furniture::FurniturePlacement;
    let (state, rx) = walker();
    state
        .lock()
        .await
        .world_cache
        .write()
        .unwrap()
        .sync_furniture(
            -2,
            4,
            vec![FurniturePlacement {
                id: 121,
                type_id: "union_hall".into(),
                x: -1444.781,
                y: 1.0343,
                z: 4798.467,
                rotation_deg: 270.0,
                floor_level: 0,
                open_doors: 0,
            }],
        );
    (state, rx)
}

#[tokio::test(start_paused = true)]
async fn a_door_side_the_server_refuses_gives_way_to_the_other_side() {
    let (state, mut rx) = walker_by_the_chapel().await;
    let start = (-1425.5, 4798.5);
    state.lock().await.self_player.as_mut().unwrap().position = p(start.0, 1.3, start.1);
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [-1434.33, 1.3, 4798.47],
            ..Default::default()
        },
    );
    let (id, ..) = goal(&mut rx).await;
    state
        .lock()
        .await
        .push_event(progress_at(id, start.0, start.1, MoveStatus::NodeLimit));
    let (id, _, z) = goal(&mut rx).await;
    assert!((z - 4803.537).abs() < 0.01, "approached {z}");
    state
        .lock()
        .await
        .push_event(progress_at(id, start.0, start.1, MoveStatus::Partial));
    let (id, x, z) = goal(&mut rx).await;
    assert!((z - 4801.937).abs() < 0.01, "approached {z}");
    state
        .lock()
        .await
        .push_event(progress_at(id, x, z, MoveStatus::Arrived));
    assert!(matches!(
        timeout(Duration::from_secs(1), rx.recv()).await,
        Ok(Some(ClientMessage::ToggleDoor { .. }))
    ));
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn a_shut_door_is_opened_when_it_saves_most_of_an_open_way_round() {
    let (state, mut rx) = walker_by_the_chapel().await;
    {
        let mut s = state.lock().await;
        s.self_player.as_mut().unwrap().position = p(-1425.5, 1.3, 4801.5);
        s.world_cache
            .write()
            .unwrap()
            .set_door(&DoorId::object(-2, 4, 121, 0).to_string(), Some(true));
    }
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [-1434.33, 1.3, 4798.47],
            ..Default::default()
        },
    );
    let (id, x, z) = goal(&mut rx).await;
    assert!(
        (x + 1431.581).abs() < 0.01 && (z - 4803.537).abs() < 0.01,
        "approached ({x}, {z})"
    );
    state
        .lock()
        .await
        .push_event(progress_at(id, x, z, MoveStatus::Arrived));
    let command = timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(matches!(
        command,
        Ok(Some(ClientMessage::ToggleDoor { door })) if door == "object:-2:4:121:1"
    ));
    state
        .lock()
        .await
        .push_event(DoorId::object(-2, 4, 121, 1).state(Some(true)));
    let (_, x, z) = goal(&mut rx).await;
    assert_eq!((x, z), (-1434.33, 4798.47));
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn an_open_way_round_is_kept_when_no_shut_door_saves_much() {
    let (state, mut rx) = walker_by_the_chapel().await;
    {
        let mut s = state.lock().await;
        s.self_player.as_mut().unwrap().position = p(-1434.33, 1.3, 4798.47);
        s.world_cache
            .write()
            .unwrap()
            .set_door(&DoorId::object(-2, 4, 121, 0).to_string(), Some(true));
    }
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [-1470.5, 1.3, 4798.5],
            ..Default::default()
        },
    );
    let (_, x, z) = goal(&mut rx).await;
    assert_eq!((x, z), (-1470.5, 4798.5));
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn the_priest_door_is_opened_from_outside_when_approached_from_the_rectory() {
    let (state, rx) = walker_by_the_chapel().await;
    opens_a_door_from_behind_its_building(
        state,
        rx,
        (-1425.5, 4798.5),
        (-1425.5, 4798.5),
        (-1434.33, 4798.47),
        (-1431.581, 4803.537),
        DoorId::object(-2, 4, 121, 1),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_house_door_is_opened_from_outside_when_approached_from_behind() {
    let (state, rx) = walker();
    {
        let s = state.lock().await;
        let wall = |variant, len| {
            vec![
                WallConfig {
                    variant,
                    texture: 0,
                    is_open: false,
                };
                len
            ]
        };
        let mut room = room(0, 0, Default::default());
        room.size_x = 6;
        room.wall_north = wall(WallVariant::Solid, 6);
        room.wall_south = wall(WallVariant::Solid, 6);
        room.wall_east = wall(WallVariant::Solid, 4);
        room.wall_west = wall(WallVariant::Solid, 4);
        room.wall_west[1].variant = WallVariant::WithDoor;
        let mut rectory = house("rectory", p(-1424.0, 1.3, 4800.0), vec![room]);
        rectory.passability = vec![onlinerpg_shared::housing::PassabilityGrid {
            floor_level: 0,
            origin_x: 0,
            origin_z: 0,
            width: 6,
            depth: 4,
            cells: vec![
                9, 1, 1, 1, 1, 3, 8, 0, 0, 0, 0, 2, 8, 0, 0, 0, 0, 2, 12, 4, 4, 4, 4, 6,
            ],
        }];
        s.world_cache.write().unwrap().apply_house_event(
            s.self_player_id.unwrap(),
            "test",
            &WorldEvent {
                subject: "house:rectory".into(),
                revision: 1,
                change: InterestChange::Enter,
                messages: vec![ServerMessage::HouseUpdated { house: rectory }],
            },
        );
    }
    opens_a_door_from_behind_its_building(
        state,
        rx,
        (-1416.5, 4802.5),
        (-1418.5, 4804.5),
        (-1418.9, 4802.6),
        (-1424.5, 4801.5),
        DoorId::house("rectory", 0, WallDirection::West, 1),
    )
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_leg_that_stops_beside_a_far_target_completes_the_schedule() {
    let (state, mut rx) = walker();
    let task = spawn_move(
        &state,
        ScheduleEntry {
            pos: [50.0, 0.0, 0.5],
            action: Some("bed".into()),
            object_id: Some(23),
            ..Default::default()
        },
    );
    let (id, x, _) = goal(&mut rx).await;
    assert!(x < 49.0, "first leg {x}");
    state
        .lock()
        .await
        .push_event(progress(id, 48.4, MoveStatus::Partial));
    timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::PlayerFace { .. })
    ));
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

#[tokio::test(start_paused = true)]
async fn a_long_walk_goes_round_a_known_fence_and_replans_after_a_dead_end() {
    use onlinerpg_shared::fence::{Fence, FenceAxis, FenceEdge};
    let (state, mut rx) = walker();
    let cache = {
        let s = state.lock().await;
        let wall: Vec<_> = (-40..40)
            .map(|z| Fence {
                edge: FenceEdge {
                    x: 20,
                    z,
                    axis: FenceAxis::Z,
                },
                y: 0.0,
                owner_id: 0,
            })
            .collect();
        let mut world = s.world_cache.write().unwrap();
        world.update_fences(s.self_player_id.unwrap(), &wall, &[]);
        world.passability_cache().clone()
    };
    let task = {
        let state = Arc::clone(&state);
        tokio::spawn(async move { execute_move(&state, 200.5, 0.5, 0, None).await })
    };
    let blocked = |x: f32, z: f32| {
        onlinerpg_shared::pathfinding::is_movement_blocked(&cache, 0.5, 0.5, x, z, 0, None)
    };
    let (first, x, z) = goal(&mut rx).await;
    assert!(!blocked(x, z), "first leg ({x}, {z}) runs into the fence");
    state
        .lock()
        .await
        .push_event(progress(first, 0.5, MoveStatus::NodeLimit));
    let (_, x2, z2) = goal(&mut rx).await;
    assert!(
        !blocked(x2, z2),
        "replanned leg ({x2}, {z2}) runs into the fence"
    );
    assert!(
        (x2 - x).hypot(z2 - z) > 1.5,
        "the dead end was offered again"
    );
    task.abort();
}

#[tokio::test(start_paused = true)]
async fn a_straight_leg_into_an_unseen_obstacle_is_replanned_not_abandoned() {
    let (state, mut rx) = walker();
    let task = {
        let state = Arc::clone(&state);
        tokio::spawn(async move { execute_move(&state, 200.5, 0.5, 0, None).await })
    };
    let first = next_goal(&mut rx, 48.5).await;
    state
        .lock()
        .await
        .push_event(progress(first, 0.5, MoveStatus::Partial));
    let (_, x, z) = goal(&mut rx).await;
    assert!(
        (x - 48.5).hypot(z - 0.5) > 1.5,
        "the failed leg ({x}, {z}) was sent again"
    );
    assert!(!task.is_finished());
    task.abort();
}

fn guard_schedule(json: &str) -> Vec<ScheduleEntry> {
    #[derive(serde::Deserialize)]
    struct File {
        schedule: Vec<ScheduleEntry>,
    }
    let mut schedule = serde_json::from_str::<File>(json).unwrap().schedule;
    assert!(onlinerpg_shared::schedule::parse_conditions(&mut schedule).is_empty());
    schedule
}

#[test]
fn the_guard_watches_hand_over_without_sharing_a_bunk_or_a_seat() {
    let guards = [
        include_str!("../../data/npcs/karl/schedule.json"),
        include_str!("../../data/npcs/garrick/schedule.json"),
        include_str!("../../data/npcs/brenna/schedule.json"),
        include_str!("../../data/npcs/osric/schedule.json"),
        include_str!("../../data/npcs/halda/schedule.json"),
    ]
    .map(guard_schedule);
    let at = |guard: usize, minute: u32| {
        let (i, _) = resolve_active_schedule(
            &guards[guard],
            None,
            Some(minute / 60),
            Some(minute % 60),
            None,
        );
        &guards[guard][i.expect("every minute has an entry")]
    };
    let label = |guard: usize, minute: u32| at(guard, minute).display_label().to_string();
    let mut reports = 0;
    for minute in 0..24 * 60 {
        let mut used = HashSet::new();
        for guard in 0..guards.len() {
            if let Some(id) = at(guard, minute).object_id {
                assert!(used.insert(id), "object {id} shared at minute {minute}");
            }
        }
        for guard in 1..5 {
            if label(guard, minute).starts_with("reporting") {
                reports += 1;
                assert!(
                    label(0, minute).starts_with("at your post"),
                    "minute {minute}"
                );
            }
        }
        for (night, day) in [(3, 1), (4, 2)] {
            if label(night, minute).starts_with("getting up") {
                assert!(
                    label(day, minute - 5).starts_with("waking"),
                    "minute {minute}"
                );
            }
        }
    }
    assert_eq!(reports, 4 * 20);
    for guard in 0..5 {
        let asleep = (0..24 * 60).filter(|m| at(guard, *m).is_sleeping()).count();
        assert!(asleep >= 9 * 60, "guard {guard} sleeps {asleep} minutes");
    }
}
