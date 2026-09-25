use super::*;
use crate::state::tests::{test_player, test_state};
use onlinerpg_shared::{PlayerId, ServerMessage};
use std::sync::atomic::{AtomicUsize, Ordering};

struct FailAfter {
    calls: Arc<AtomicUsize>,
    successful_calls: usize,
}

#[async_trait]
impl LlmBackend for FailAfter {
    async fn send_message(&self, _: &str) -> anyhow::Result<String> {
        if self.calls.fetch_add(1, Ordering::SeqCst) < self.successful_calls {
            Ok("{}".into())
        } else {
            anyhow::bail!("Selected model is at capacity")
        }
    }
}

#[tokio::test]
async fn initial_failure_delays_even_a_forced_guest_prompt() {
    check_failed_driver(0).await;
}

#[tokio::test]
async fn later_failure_delays_even_a_forced_guest_prompt() {
    check_failed_driver(1).await;
}

async fn check_failed_driver(successful_calls: usize) {
    let (mut s, mut rx) = test_state();
    let me = test_player(0.0, 0.0);
    let position = me.position;
    s.in_game = true;
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    let state = Arc::new(Mutex::new(s));
    let drain = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    let calls = Arc::new(AtomicUsize::new(0));
    let config = DriverConfig {
        label: "backoff_test".into(),
        memory_file: None,
        favor_file: None,
        min_interval: Duration::ZERO,
        urgent_min_interval: Duration::ZERO,
        debounce: Duration::ZERO,
        idle_interval: Duration::ZERO,
        activity_window: Duration::ZERO,
        always_active: true,
        schedule: Vec::new(),
        sickroom: Vec::new(),
        serve_tables: true,
        maid_names: HashSet::new(),
        tables: vec![VisitSpot {
            object_id: 46,
            pos: [position.x, position.y, position.z],
            rotation: 0.0,
            floor_level: 0,
        }],
        claims: Arc::default(),
        api_base_url: "http://127.0.0.1:9".into(),
    };
    let driver = tokio::spawn(llm_driver(
        Arc::clone(&state),
        Arc::new(FailAfter {
            calls: Arc::clone(&calls),
            successful_calls,
        }),
        LlmScheduler::new(1, Duration::from_secs(5)),
        config,
    ));
    let failed = tokio::time::timeout(Duration::from_secs(5), async {
        while calls.load(Ordering::SeqCst) <= successful_calls {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    if failed.is_ok() {
        let mut s = state.lock().await;
        let mut guest = test_player(2.0, 0.0);
        guest.id = PlayerId::from(8);
        guest.name = "Guest".into();
        s.nearby_players.insert(guest.id, guest);
        s.push_event(ServerMessage::PlayerInteractionChanged {
            position: onlinerpg_shared::Position {
                x: 2.0,
                y: 0.0,
                z: 0.0,
            },
            rotation: 0.0,
            floor_level: 0,
            player_id: PlayerId::from(8),
            object_type: Some(crate::state::SIT_OBJECT_TYPE.into()),
            object_id: Some(46),
        });
        s.push_event(ServerMessage::ChatMessage {
            player_id: PlayerId::from(8),
            message: "Me, can I buy some bread?".into(),
        });
    }
    tokio::time::sleep(Duration::from_secs(3)).await;
    driver.abort();
    drain.abort();
    failed.expect("the driver should reach the failing call");
    assert_eq!(calls.load(Ordering::SeqCst), successful_calls + 1);
    let mut s = state.lock().await;
    assert!(s
        .drain_agent_events()
        .iter()
        .any(|event| event.contains("[Guest] Guest")));
    assert!(!s.pending_chat().is_empty());
}
