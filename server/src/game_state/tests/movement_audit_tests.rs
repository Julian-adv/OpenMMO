use super::*;
use serde_json::Value;
use tracing::instrument::WithSubscriber;

#[derive(Clone, Default)]
struct TraceCollector(Arc<std::sync::Mutex<Vec<Value>>>);

impl tracing::Subscriber for TraceCollector {
    fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
        metadata.target() == "movement_audit"
    }
    fn register_callsite(
        &self,
        _: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        tracing::subscriber::Interest::sometimes()
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        struct Detail(Option<Value>);
        impl tracing::field::Visit for Detail {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "detail" {
                    self.0 = Some(serde_json::from_str(&format!("{value:?}")).unwrap());
                }
            }
        }
        let mut detail = Detail(None);
        event.record(&mut detail);
        if let Some(value) = detail.0 {
            self.0.lock().unwrap().push(value);
        }
    }
}

fn trace_capture() -> TraceCollector {
    static CAPTURE: std::sync::OnceLock<(tracing::Dispatch, TraceCollector)> =
        std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let collector = TraceCollector::default();
            (tracing::Dispatch::new(collector.clone()), collector)
        })
        .1
        .clone()
}

async fn orc_player(name: &str) -> (GameState, PlayerId) {
    use onlinerpg_shared::dungeon::*;
    let game = make_test_game_state("movement_audit");
    let entrance = game.dungeon_defs.get("orc_warrens").unwrap().position();
    let layouts = generate_dungeon_for("orc_warrens");
    game.passability_write().insert(
        dungeon_cache_key("orc_warrens"),
        dungeon_passability(&entrance, &layouts),
    );
    let mut player = make_player(name, -1637.8, 4890.8);
    player.position.y = -22.95;
    player.floor_level = -6;
    let id = player.id;
    game.add_player(player).await;
    game.register_player_character(&id, 6698, 0, attrs_with_cha(12), 0, None)
        .await;
    (game, id)
}

fn command(x: f32, z: f32, append: bool) -> MoveCommand {
    MoveCommand {
        floor_level: -6,
        ..move_cmd(Position { x, y: 777., z }, append)
    }
}

#[tokio::test]
async fn movement_audit_links_raw_request_queue_tick_and_correction() {
    let (game, id) = orc_player("trace_links").await;
    let mut rx = game.register_direct_channel(&id).await;
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    async {
        game.update_player_position(&id, command(-1638.8, 4890.8, false), false)
            .await;
        game.tick_player_movement(1.).await;
        game.update_player_position(&id, command(-1641.5, 4890.8, false), false)
            .await;
        game.update_player_position(&id, command(-1642.5, 4890.8, true), false)
            .await;
        for _ in 0..2 {
            game.last_position_correction.write().await.clear();
            game.update_keyboard_movement(&id, command(-1638.8, 4891.3, false), 1)
                .await;
            game.tick_player_movement(0.2).await;
        }
    }
    .with_subscriber(subscriber)
    .await;
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|trace| trace["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    assert_eq!(
        traces.len(),
        1,
        "first collision is detailed; repeats are throttled"
    );
    let trace = &traces[0];
    assert_eq!(trace["character_id"], 6698);
    assert_eq!(trace["block_key"], "dungeon:orc_warrens");
    let requests = trace["history"]["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 4);
    let request = &trace["step"]["intent"]["request"];
    assert_eq!(request, requests.last().unwrap());
    assert_eq!(request["raw"]["position"]["y"], 777.);
    assert_eq!(request["target"]["y"], serde_json::json!(-22.95_f32));
    assert_eq!(
        request["received_pose"]["position"]["x"],
        serde_json::json!(-1638.8_f32)
    );
    assert_eq!(request["queue_before"], 2);
    assert_eq!(request["replaced"], true);
    assert_eq!(request["corrections_issued_before_queue"], 0);
    assert_eq!(requests[2]["raw"]["append"], true);
    assert_eq!(requests[2]["leg_start"], requests[1]["target"]);
    assert_eq!(trace["step"]["queue"].as_array().unwrap().len(), 1);
    assert_eq!(
        trace["step"]["geometry"]["attempted"]["cell"],
        serde_json::json!([17, 13])
    );
    assert_eq!(
        trace["step"]["geometry"]["attempted"]["neighbors"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    let ticks = trace["history"]["ticks"].as_array().unwrap();
    assert_eq!(ticks[0]["outcome"], "clear");
    assert_eq!(ticks[1]["outcome"], "blocked");
    assert_eq!(ticks[1]["last_request_id"], request["id"]);
    assert_eq!(trace["history"]["corrections"], 0);
    let history = serde_json::to_value(
        game.movement_audit
            .snapshot(id, Instant::now() + std::time::Duration::from_secs(31))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(history["corrections"], 2);
    assert_eq!(
        history["requests"].as_array().unwrap().last().unwrap()["corrections_issued_before_queue"],
        1
    );
    assert_eq!(
        history["last_correction"]["position"],
        ticks[1]["to"]["position"]
    );
    let mut corrections = 0;
    while let Ok(message) = rx.try_recv() {
        if let ServerMessage::PositionCorrected {
            position,
            floor_level,
            ..
        } = message
        {
            corrections += 1;
            assert_eq!(position.x, -1638.8);
            assert_eq!(position.z, 4890.8);
            assert_eq!(floor_level, -6);
        }
    }
    assert_eq!(corrections, 2);
}

#[tokio::test]
async fn movement_audit_bounds_history_tracks_overflow_and_cleans_up() {
    let (game, id) = orc_player("trace_bounds").await;
    for _ in 0..40 {
        game.update_player_position(&id, command(-1637.8, 4890.8, true), true)
            .await;
    }
    let now = Instant::now();
    let history = serde_json::to_value(game.movement_audit.snapshot(id, now).unwrap()).unwrap();
    let requests = history["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 16);
    assert_eq!(history["requests_evicted"], 24);
    assert!(requests.last().unwrap()["dropped"].as_u64().is_some());
    assert_eq!(requests.last().unwrap()["queue_before"], 32);
    assert!(game
        .movement_audit
        .snapshot(id, now + std::time::Duration::from_secs(29))
        .is_none());
    assert!(game
        .movement_audit
        .snapshot(id, now + std::time::Duration::from_secs(30))
        .is_some());
    for _ in 0..40 {
        game.update_player_position(&id, command(-1637.8, 4890.8, false), false)
            .await;
        game.tick_player_movement(0.2).await;
    }
    let bounded = serde_json::to_value(
        game.movement_audit
            .snapshot(id, now + std::time::Duration::from_secs(60))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bounded["ticks"].as_array().unwrap().len(), 16);
    assert_eq!(bounded["ticks_evicted"], 24);
    game.remove_player(&id).await;
    game.movement_audit.correction(
        id,
        Position {
            x: 0.,
            y: 0.,
            z: 0.,
        },
        0,
    );
    assert!(game
        .movement_audit
        .snapshot(id, now + std::time::Duration::from_secs(60))
        .is_none());
}

#[tokio::test]
async fn movement_audit_keeps_blocked_intent_after_request_history_eviction() {
    let (game, id) = orc_player("trace_eviction").await;
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    async {
        game.update_player_position(&id, command(-1637.8, 4891.3, false), false)
            .await;
        for _ in 0..23 {
            game.update_player_position(&id, command(-1637.8, 4890.8, true), false)
                .await;
        }
        game.tick_player_movement(0.2).await;
    }
    .with_subscriber(subscriber)
    .await;
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|trace| trace["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    assert_eq!(traces.len(), 1);
    let trace = &traces[0];
    assert!(trace.to_string().len() < 48_000);
    let request = &trace["step"]["intent"]["request"];
    assert_eq!(
        request["raw"]["position"]["z"],
        serde_json::json!(4891.3_f32)
    );
    assert_eq!(trace["history"]["requests_evicted"], 8);
    assert!(trace["history"]["requests"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["id"] != request["id"]));
    assert_eq!(trace["step"]["queue"].as_array().unwrap().len(), 24);
    assert_eq!(trace["step"]["queue"][0]["request_id"], request["id"]);
}

#[tokio::test]
async fn movement_audit_distinguishes_waypoints_consumed_in_one_tick() {
    let (game, id) = orc_player("trace_waypoints").await;
    game.update_player_position(&id, command(-1637.7, 4890.8, false), false)
        .await;
    game.update_player_position(&id, command(-1637.7, 4891.3, true), false)
        .await;
    game.tick_player_movement(0.2).await;
    let history = serde_json::to_value(
        game.movement_audit
            .snapshot(id, Instant::now() + std::time::Duration::from_secs(31))
            .unwrap(),
    )
    .unwrap();
    let tick = &history["ticks"][0];
    assert_eq!(tick["outcome"], "blocked");
    assert_eq!(tick["first_request_id"], history["requests"][0]["id"]);
    assert_eq!(tick["last_request_id"], history["requests"][1]["id"]);
    assert_ne!(tick["from"]["position"], tick["to"]["position"]);
    assert_eq!(tick["queue_remaining"], 1);
}

#[tokio::test]
async fn movement_audit_traces_waypoint_overflow_once_per_interval() {
    let (game, id) = orc_player("trace_overflow").await;
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    let sent = super::super::player::MAX_QUEUED_WAYPOINTS + 8;
    async {
        for _ in 0..sent {
            game.update_player_position(&id, command(-1637.8, 4890.8, true), true)
                .await;
        }
        game.tick_player_movement(0.2).await;
    }
    .with_subscriber(subscriber)
    .await;
    let traces: Vec<Value> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["player_id"] == serde_json::to_value(id).unwrap())
        .cloned()
        .collect();
    assert_eq!(traces.len(), 1);
    let history = &traces[0]["history"];
    assert_eq!(history["overflows"], 1);
    let request = history["requests"].as_array().unwrap().last().unwrap();
    assert_eq!(
        request["queue_before"],
        super::super::player::MAX_QUEUED_WAYPOINTS
    );
    assert!(request["dropped"].as_u64().is_some());
    assert_eq!(request["raw"]["append"], true);
    assert_eq!(request["received_pose"]["mounted"], false);
    let now = Instant::now();
    assert!(game.movement_audit.snapshot(id, now).is_some());
    assert!(game
        .movement_audit
        .overflow(id, now + std::time::Duration::from_secs(29))
        .is_none());
    let later = game
        .movement_audit
        .overflow(id, now + std::time::Duration::from_secs(30))
        .unwrap();
    let later = serde_json::to_value(later).unwrap();
    assert_eq!(
        later["overflows"],
        (sent - super::super::player::MAX_QUEUED_WAYPOINTS + 2) as u64
    );
    let tick = later["ticks"].as_array().unwrap().last().unwrap();
    assert_eq!(tick["hunger_mult"], 1.0);
    assert_eq!(tick["sprint_allowed"], true);
}

#[tokio::test]
async fn queue_resync_logs_authority_history_and_ack_with_discarded_count() {
    let (game, id) = orc_player("resync_audit").await;
    let mut rx = game.register_direct_channel(&id).await;
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    async {
        for _ in 0..30 {
            game.update_player_position(&id, command(-1637.8, 4890.8, true), false)
                .await;
        }
        let message = std::iter::from_fn(|| rx.try_recv().ok())
            .find(|m| matches!(m, ServerMessage::MovementResync { .. }))
            .unwrap();
        let ServerMessage::MovementResync {
            resync_id,
            position,
            floor_level,
            ..
        } = message
        else {
            unreachable!()
        };
        assert_eq!(position, game.players.read().await[&id].position);
        assert_eq!(floor_level, -6);
        game.acknowledge_movement_resync(&id, resync_id);
    }
    .with_subscriber(subscriber)
    .await;
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    let header = traces
        .iter()
        .find(|v| v["event"] == "queue_resync")
        .unwrap();
    assert_eq!(header["detail"]["queue_len"], 24);
    let requests: Vec<_> = traces
        .iter()
        .filter(|v| v["trace_id"] == header["trace_id"] && v["kind"] == "requests")
        .collect();
    assert_eq!(requests.len(), 8);
    for (i, part) in requests.iter().enumerate() {
        assert_eq!(part["part"], i);
        assert_eq!(part["parts"], requests.len());
        assert_eq!(part["entries"].as_array().unwrap().len(), 2);
    }
    let ack = traces
        .iter()
        .find(|v| v["event"] == "resync_acknowledged")
        .unwrap();
    assert_eq!(ack["detail"]["resync_id"], header["detail"]["resync_id"]);
    assert_eq!(ack["detail"]["discarded_messages"], 5);
}

#[tokio::test]
async fn movement_samples_use_actual_pose_not_a_distant_destination_and_wrap_x() {
    let (game, id) = orc_player("sample_position").await;
    let position = game.players.read().await[&id].position;
    game.update_player_position(&id, command(position.x + 10.0, position.z, false), false)
        .await;
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    async {
        game.record_movement_sample(
            &id,
            Position {
                x: position.x + onlinerpg_shared::WORLD_WIDTH_X,
                ..position
            },
            0.0,
            -6,
        )
        .await;
        game.log_movement_sync(id, "test_sample", serde_json::json!({}), true);
    }
    .with_subscriber(subscriber)
    .await;
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|v| v["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    let sample = &traces.iter().find(|v| v["kind"] == "samples").unwrap()["entries"][0];
    assert!(sample["gap"].as_f64().unwrap() < 0.01);
    assert_eq!(sample["queue_len"], 1);
    assert_eq!(
        sample["server_pose"]["position"],
        serde_json::to_value(position).unwrap()
    );
    assert!(!traces.iter().any(|v| v["event"] == "divergence_started"));
}

#[tokio::test]
async fn recovery_audit_throttles_traces_but_keeps_receipt_counts_and_rejections() {
    let (game, id) = orc_player("recovery_trace").await;
    let player = game.players.read().await[&id].clone();
    game.movement_audit
        .correction(id, player.position, player.floor_level);
    let subscriber = trace_capture();
    let buffer = subscriber.0.clone();
    async {
        for _ in 0..40 {
            game.recover_horse(&id, 7, player.position).await;
        }
    }
    .with_subscriber(subscriber.clone())
    .await;
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|trace| trace["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    assert_eq!(traces.len(), 2);
    assert_eq!(traces[0]["event"]["status"], "received");
    assert_eq!(traces[1]["event"]["status"], "rejected");
    assert_eq!(traces[1]["event"]["detail"]["reason"], "not_mounted");
    assert_eq!(traces[0]["event"]["request"], traces[1]["event"]["request"]);
    assert_eq!(traces[0]["event"]["request"]["request_id"], 7);
    assert_eq!(
        traces[0]["event"]["request"]["corrections_issued_before_request"],
        1
    );

    let now = Instant::now();
    let history = serde_json::to_value(game.movement_audit.snapshot(id, now).unwrap()).unwrap();
    assert_eq!(history["recovery_requests"], 40);
    assert_eq!(history["recovery_traces_suppressed"], 39);
    assert_eq!(history["recovery_events_evicted"], 64);
    let events = history["recovery_events"].as_array().unwrap();
    assert_eq!(events.len(), 16);
    assert_eq!(events.last().unwrap()["status"], "rejected");
    assert_ne!(events[0]["request"]["id"], events[2]["request"]["id"]);

    tracing::subscriber::with_default(subscriber, || {
        let later = game.movement_audit.recovery_request(
            &player,
            8,
            player.position,
            now + std::time::Duration::from_secs(30),
        );
        game.movement_audit.recovery_event(
            id,
            later,
            "rejected",
            serde_json::json!({"reason": "not_mounted"}),
        );
    });
    let traces: Vec<_> = buffer
        .lock()
        .unwrap()
        .iter()
        .filter(|trace| trace["player_id"] == serde_json::json!(id))
        .cloned()
        .collect();
    assert_eq!(traces.len(), 4);
    assert_eq!(traces[2]["recovery_requests"], 41);
    assert_eq!(traces[2]["recovery_traces_suppressed"], 39);
}
