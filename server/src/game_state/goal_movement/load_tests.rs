use super::super::tests::{make_player, make_test_game_state};
use super::lock_measurement::LockSample;
use super::*;
use onlinerpg_shared::mount::MountKind;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use tokio::sync::mpsc::UnboundedReceiver;

const MOVERS: usize = 5000;
const PROBES: usize = 30;
const ROUNDS: usize = 20;

#[derive(Clone, Copy)]
enum Mix {
    Click,
    Keyboard,
    MountedKeyboard,
    Mixed,
}

impl Mix {
    fn player_mode(self, index: usize) -> (bool, bool) {
        match self {
            Self::Click => (false, false),
            Self::Keyboard => (true, false),
            Self::MountedKeyboard => (true, true),
            Self::Mixed => (index % 4 >= 2, index % 2 == 1),
        }
    }
}

#[derive(Clone)]
struct MeasuredHeightTiles {
    reads: Arc<AtomicUsize>,
    delay_next: Arc<AtomicBool>,
    delayed_reads: Arc<std::sync::Mutex<Vec<Duration>>>,
    raw: Arc<Vec<u8>>,
}

impl MeasuredHeightTiles {
    fn new() -> Self {
        Self {
            reads: Arc::new(AtomicUsize::new(0)),
            delay_next: Arc::new(AtomicBool::new(false)),
            delayed_reads: Arc::default(),
            raw: Arc::new(
                onlinerpg_terrain::height::encode_height(5.0)
                    .to_le_bytes()
                    .repeat(onlinerpg_terrain::defaults::VERTS_PER_SIDE.pow(2)),
            ),
        }
    }
}

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for MeasuredHeightTiles {
    async fn read_heightmap(&self, _tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        if self.delay_next.swap(false, Ordering::Relaxed) {
            let started = std::time::Instant::now();
            tokio::time::sleep(Duration::from_millis(20)).await;
            self.delayed_reads.lock().unwrap().push(started.elapsed());
        }
        Ok((*self.raw).clone())
    }
}

fn distribution(samples: impl IntoIterator<Item = Duration>) -> Value {
    let mut values: Vec<_> = samples
        .into_iter()
        .map(|duration| duration.as_secs_f64() * 1000.0)
        .collect();
    values.sort_by(f64::total_cmp);
    if values.is_empty() {
        return json!({ "count": 0 });
    }
    let percentile = |p: usize| values[(values.len() * p).div_ceil(100) - 1];
    json!({
        "count": values.len(),
        "p50_ms": percentile(50),
        "p95_ms": percentile(95),
        "p99_ms": percentile(99),
        "max_ms": values.last().unwrap(),
    })
}

fn lock_distribution(samples: &[LockSample]) -> Value {
    json!({
        "wait": distribution(samples.iter().map(|sample| sample.wait)),
        "hold": distribution(samples.iter().map(|sample| sample.hold)),
    })
}

fn drain(channels: &mut [UnboundedReceiver<bytes::Bytes>]) -> (usize, usize) {
    let mut packets = 0;
    let mut bytes = 0;
    for channel in channels {
        while let Ok(packet) = channel.try_recv() {
            packets += 1;
            bytes += packet.len();
        }
    }
    (packets, bytes)
}

async fn measure_case(name: &str, mix: Mix, cold: bool, delayed: bool) -> Value {
    let mut game = make_test_game_state(&format!("movement_contention_{name}"));
    let tiles = MeasuredHeightTiles::new();
    game.height_sampler = Arc::new(onlinerpg_terrain::height::HeightSampler::new(tiles.clone()));
    let mut ids = Vec::new();
    let mut channels = Vec::new();
    for index in 0..MOVERS + PROBES {
        let mut player = make_player(
            &format!("contention_{index}"),
            1000.5 + (index % 100) as f32 * 32.0,
            1000.5 + (index / 100) as f32 * 32.0,
        );
        player.position.y = 5.0;
        player.rotation = std::f32::consts::FRAC_PI_2;
        if index < MOVERS && mix.player_mode(index).1 {
            player.mount = Some(MountKind::Horse);
        }
        ids.push(player.id);
        game.player_spatial_cells
            .write()
            .await
            .insert(player.id, &player.position);
        channels.push(game.register_connection_channel(&player.id).await);
        game.goal_moves.register(player.id);
        game.players.write().await.insert(player.id, player);
    }
    let mut setup = tokio::task::JoinSet::new();
    for (index, &id) in ids.iter().enumerate() {
        let game = game.clone();
        setup.spawn(async move {
            if index >= MOVERS || mix.player_mode(index).0 {
                game.request_move_direction(id, 1, std::f32::consts::FRAC_PI_2, 1, 0, false)
                    .await;
            } else {
                let position = game.players.read().await[&id].position;
                game.request_move_goal(id, 1, position.x + 24.0, position.z, false)
                    .await;
                super::tests::search_finished(&game, id).await;
            }
        });
        if setup.len() >= 64 {
            setup.join_next().await.unwrap().unwrap();
        }
    }
    while let Some(result) = setup.join_next().await {
        result.unwrap();
    }
    {
        assert_eq!(game.goal_moves.ids().len(), MOVERS + PROBES);
        for &id in &ids {
            let mut slot = game.goal_moves.lock(id).await;
            let state = slot.as_mut().unwrap();
            assert!(state.plan.is_some() || state.direction.is_some());
            if let Some(plan) = &mut state.plan {
                plan.advanced_at = Instant::now() - Duration::from_millis(200);
            }
            if let Some(direction) = &mut state.direction {
                direction.advanced_at = Instant::now() - Duration::from_millis(200);
                direction.expires_at = Instant::now() + Duration::from_secs(60);
            }
        }
    }
    game.advance_goal_players(&ids[..MOVERS]).await;
    let baseline_players = game.players.read().await.clone();
    let mut baseline_goals = std::collections::HashMap::new();
    for &id in &ids {
        baseline_goals.insert(id, game.goal_moves.lock(id).await.clone());
    }
    let mut ticks = Vec::new();
    let mut requests = [Vec::new(), Vec::new(), Vec::new()];
    let mut scheduling = Vec::new();
    let mut gate_samples = Vec::new();
    let mut goal_samples = Vec::new();
    let mut packets = 0;
    let mut bytes = 0;
    let mut height_reads = 0;
    for _ in 0..ROUNDS {
        *game.players.write().await = baseline_players.clone();
        {
            let mut cells = game.player_spatial_cells.write().await;
            *cells = Default::default();
            for player in baseline_players.values() {
                cells.insert(player.id, &player.position);
            }
        }
        {
            let now = Instant::now();
            for (&id, baseline) in &baseline_goals {
                let mut slot = game.goal_moves.lock(id).await;
                *slot = baseline.clone();
                let state = slot.as_mut().unwrap();
                if let Some(plan) = &mut state.plan {
                    plan.advanced_at = now - Duration::from_millis(200);
                }
                if let Some(direction) = &mut state.direction {
                    direction.advanced_at = now - Duration::from_millis(200);
                    direction.expires_at = now + Duration::from_secs(60);
                }
            }
        }
        if cold {
            game.height_sampler =
                Arc::new(onlinerpg_terrain::height::HeightSampler::new(tiles.clone()));
        }
        drain(&mut channels);
        tiles.reads.store(0, Ordering::Relaxed);
        tiles.delay_next.store(delayed, Ordering::Relaxed);
        game.movement_regions.measurement.start_measurement();
        game.goal_moves.measurement.start_measurement();
        let started = Instant::now();
        let mut probes = tokio::task::JoinSet::new();
        for (index, &id) in ids[MOVERS..].iter().enumerate() {
            let game = game.clone();
            probes.spawn(async move {
                let deadline = started + Duration::from_millis(1);
                tokio::time::sleep_until(deadline).await;
                let launched = Instant::now();
                let kind = index % 3;
                match kind {
                    0 => {
                        game.request_move_direction(
                            id,
                            1,
                            std::f32::consts::FRAC_PI_2,
                            1,
                            0,
                            false,
                        )
                        .await;
                    }
                    1 => {
                        game.request_move_direction(id, 2, 0.0, 1, 0, false).await;
                    }
                    _ => game.stop_move_goal(id, 2).await,
                }
                (kind, deadline.elapsed(), launched.duration_since(deadline))
            });
        }
        game.advance_goal_players(&ids[..MOVERS]).await;
        ticks.push(started.elapsed());
        while let Some(result) = probes.join_next().await {
            let (kind, latency, scheduling_delay) = result.unwrap();
            requests[kind].push(latency);
            scheduling.push(scheduling_delay);
        }
        gate_samples.extend(game.movement_regions.measurement.finish_measurement());
        goal_samples.extend(game.goal_moves.measurement.finish_measurement());
        height_reads += tiles.reads.load(Ordering::Relaxed);
        for (index, channel) in channels[MOVERS..].iter_mut().enumerate() {
            let kind = index % 3;
            let mut matched = kind == 0;
            while let Ok(packet) = channel.try_recv() {
                packets += 1;
                bytes += packet.len();
                let message = onlinerpg_shared::deserialize_server_msg(&packet).unwrap();
                matched |= match message {
                    ServerMessage::PlayerMovePath { request_id: 2, .. } => kind == 1,
                    ServerMessage::PlayerMoveProgress {
                        request_id: 2,
                        status: MoveStatus::Stopped,
                        ..
                    } => kind == 2,
                    _ => false,
                };
                assert_ne!(kind, 0, "renewal should not emit a path");
            }
            assert!(matched, "probe did not receive its movement response");
        }
        let (round_packets, round_bytes) = drain(&mut channels[..MOVERS]);
        assert_eq!(round_packets, MOVERS);
        packets += round_packets;
        bytes += round_bytes;
    }
    if !cold {
        assert_eq!(height_reads, 0, "warm scenario must stay cached");
    }
    if delayed {
        assert_eq!(tiles.delayed_reads.lock().unwrap().len(), ROUNDS);
    }
    json!({
        "case": name,
        "movers": MOVERS,
        "rounds": ROUNDS,
        "probes_per_round": PROBES,
        "tick": distribution(ticks.iter().copied()),
        "ticks_over_200_ms": ticks.iter().filter(|tick| **tick > Duration::from_millis(200)).count(),
        "renewal": distribution(requests[0].iter().copied()),
        "new_direction": distribution(requests[1].iter().copied()),
        "stop": distribution(requests[2].iter().copied()),
        "probe_scheduling_delay": distribution(scheduling),
        "movement_regions": lock_distribution(&gate_samples),
        "player_state": lock_distribution(&goal_samples),
        "height_reads": height_reads,
        "injected_read_delay": distribution(tiles.delayed_reads.lock().unwrap().iter().copied()),
        "packets": packets,
        "bytes": bytes,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "5000-player lock contention benchmark; run explicitly in release mode"]
async fn movement_lock_contention_5000() {
    if cfg!(debug_assertions) {
        panic!("run this benchmark with --release");
    }
    for (name, mix, cold, delayed) in [
        ("click_warm", Mix::Click, false, false),
        ("keyboard_warm", Mix::Keyboard, false, false),
        ("mounted_keyboard_warm", Mix::MountedKeyboard, false, false),
        ("mixed_warm", Mix::Mixed, false, false),
        ("mixed_cold", Mix::Mixed, true, false),
        ("mixed_cold_delayed", Mix::Mixed, true, true),
    ] {
        let result = tokio::time::timeout(
            Duration::from_secs(180),
            measure_case(name, mix, cold, delayed),
        )
        .await
        .expect("benchmark case completed within three minutes");
        eprintln!("movement_lock_contention {result}");
    }
}
