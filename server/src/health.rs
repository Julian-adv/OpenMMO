mod notify;

use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
    Arc, OnceLock,
};
use tokio::{sync::watch, time::Instant};
use tracing::{error, info};

use crate::game_state::GameState;
pub use notify::Notifier;
use std::time::Duration;

const MAX_SLOTS: usize = 32;
const MIN_TIMEOUT: Duration = Duration::from_secs(30);

/// How long a loop may go without finishing a tick before it counts as stalled.
#[derive(Clone, Copy)]
pub enum Deadline {
    FromPeriod,
    Fixed(Duration),
    None,
}

fn timeout_for(period: Duration) -> Duration {
    (period * 3).max(MIN_TIMEOUT)
}

#[derive(Default)]
struct Slot {
    meta: OnceLock<(&'static str, u64)>,
    // 0 = never; otherwise elapsed ms + 1, so a tick finishing at elapsed 0 still counts.
    completed: AtomicU64,
}

pub struct Health {
    started: Instant,
    started_unix_ms: u64,
    slots: [Slot; MAX_SLOTS],
    len: AtomicUsize,
    stopping: AtomicBool,
}

#[derive(Clone)]
pub struct Checkpoint {
    health: Arc<Health>,
    index: usize,
}

impl Checkpoint {
    pub fn completed(&self) {
        self.health.slots[self.index].completed.store(
            self.health.started.elapsed().as_millis() as u64 + 1,
            Ordering::Relaxed,
        );
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Starting,
    Healthy,
    Stalled,
    Stopping,
}

#[derive(Serialize)]
struct Progress {
    name: &'static str,
    last_completed_unix_ms: Option<u64>,
    age_ms: u64,
    timeout_ms: u64,
    stale: bool,
}

#[derive(Serialize)]
struct Snapshot {
    status: Status,
    checks: Vec<Progress>,
}

impl Health {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Instant::now(),
            started_unix_ms: GameState::now_ms(),
            slots: std::array::from_fn(|_| Slot::default()),
            len: AtomicUsize::new(0),
            stopping: AtomicBool::new(false),
        })
    }

    pub fn register(
        self: &Arc<Self>,
        name: &'static str,
        period: Duration,
        deadline: Deadline,
    ) -> Option<Checkpoint> {
        let timeout = match deadline {
            Deadline::FromPeriod => timeout_for(period),
            Deadline::Fixed(timeout) => timeout,
            Deadline::None => return None,
        };
        let index = self.len.fetch_add(1, Ordering::AcqRel);
        assert!(index < MAX_SLOTS, "health registry full; raise MAX_SLOTS");
        self.slots[index]
            .meta
            .set((name, timeout.as_millis() as u64))
            .expect("slot registered once");
        Some(Checkpoint {
            health: Arc::clone(self),
            index,
        })
    }

    pub fn stop(&self) {
        self.stopping.store(true, Ordering::Relaxed);
    }

    fn snapshot(&self) -> Snapshot {
        let now_ms = self.started.elapsed().as_millis() as u64;
        let len = self.len.load(Ordering::Acquire).min(MAX_SLOTS);
        let checks: Vec<_> = self.slots[..len]
            .iter()
            .filter_map(|slot| {
                let &(name, timeout_ms) = slot.meta.get()?;
                let stored = slot.completed.load(Ordering::Relaxed);
                let last = stored.checked_sub(1);
                let age_ms = now_ms.saturating_sub(last.unwrap_or(0));
                Some(Progress {
                    name,
                    last_completed_unix_ms: last.map(|ms| self.started_unix_ms + ms),
                    age_ms,
                    timeout_ms,
                    stale: age_ms >= timeout_ms,
                })
            })
            .collect();
        let status = if self.stopping.load(Ordering::Relaxed) {
            Status::Stopping
        } else if checks.iter().any(|check| check.stale) {
            Status::Stalled
        } else if checks.is_empty()
            || checks
                .iter()
                .any(|check| check.last_completed_unix_ms.is_none())
        {
            Status::Starting
        } else {
            Status::Healthy
        };
        Snapshot { status, checks }
    }

    pub async fn monitor(self: Arc<Self>, notifier: Arc<Notifier>, shutdown: watch::Receiver<()>) {
        let mut previous = Status::Starting;
        let mut ready = false;
        let health = Arc::clone(&self);
        crate::run_ticks(
            "health_monitor",
            notifier.interval(),
            shutdown,
            &health,
            Deadline::None,
            move || {
                let snapshot = self.snapshot();
                if snapshot.status != previous {
                    match snapshot.status {
                        Status::Stalled => {
                            let stalled: Vec<_> = snapshot
                                .checks
                                .iter()
                                .filter(|check| check.stale)
                                .map(|check| format!("{}={}ms", check.name, check.age_ms))
                                .collect();
                            let message =
                                format!("Server progress stalled: {}", stalled.join(", "));
                            error!("{message}; watchdog keepalives withheld");
                            notifier.send(&format!("STATUS={message}"));
                        }
                        Status::Healthy => info!("Server progress healthy"),
                        Status::Starting | Status::Stopping => {}
                    }
                    previous = snapshot.status;
                }
                if snapshot.status == Status::Healthy {
                    let message = if ready {
                        "WATCHDOG=1\nSTATUS=All periodic loops progressing"
                    } else {
                        "READY=1\nWATCHDOG=1\nSTATUS=All periodic loops progressing"
                    };
                    ready |= notifier.send(message);
                }
                std::future::ready(())
            },
        )
        .await
    }
}

async fn health(State(health): State<Arc<Health>>) -> impl IntoResponse {
    let snapshot = health.snapshot();
    let status = if snapshot.status == Status::Healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        [(header::CACHE_CONTROL, "no-store")],
        Json(snapshot),
    )
}

pub fn router(health: Arc<Health>) -> Router {
    Router::new()
        .route("/api/health", get(self::health))
        .with_state(health)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    pub(super) fn register_three(health: &Arc<Health>) -> Vec<Checkpoint> {
        ["movement", "monster_ai", "batch_save"]
            .into_iter()
            .map(|name| {
                let deadline = if name == "batch_save" {
                    Deadline::Fixed(Duration::from_secs(90))
                } else {
                    Deadline::FromPeriod
                };
                health.register(name, SECOND, deadline).unwrap()
            })
            .collect()
    }

    pub(super) fn complete_all(checkpoints: &[Checkpoint]) {
        for checkpoint in checkpoints {
            checkpoint.completed();
        }
    }

    #[test]
    fn timeout_derives_from_period_with_floor() {
        for (period_ms, timeout_s) in [
            (200, 30),
            (8_000, 30),
            (30_000, 90),
            (300_000, 900),
            (3_600_000, 10_800),
        ] {
            assert_eq!(
                timeout_for(Duration::from_millis(period_ms)),
                Duration::from_secs(timeout_s)
            );
        }
    }

    #[tokio::test(start_paused = true)]
    async fn registry_reports_names_and_timeouts() {
        let health = Health::new();
        assert_eq!(health.snapshot().status, Status::Starting);
        let checkpoints = register_three(&health);
        assert!(health
            .register("unwatched", SECOND, Deadline::None)
            .is_none());
        let snapshot = health.snapshot();
        let names: Vec<_> = snapshot.checks.iter().map(|check| check.name).collect();
        assert_eq!(names, ["movement", "monster_ai", "batch_save"]);
        assert_eq!(snapshot.checks[0].timeout_ms, 30_000);
        assert_eq!(snapshot.checks[2].timeout_ms, 90_000);
        assert_eq!(snapshot.status, Status::Starting);
        checkpoints[0].completed();
        checkpoints[1].completed();
        assert_eq!(health.snapshot().status, Status::Starting);
        checkpoints[2].completed();
        assert_eq!(health.snapshot().status, Status::Healthy);
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_tick_fails_even_while_other_loops_progress() {
        let health = Health::new();
        let checkpoints = register_three(&health);
        complete_all(&checkpoints);
        tokio::time::advance(Duration::from_secs(30)).await;
        checkpoints[0].completed();
        checkpoints[2].completed();
        let snapshot = health.snapshot();
        assert_eq!(snapshot.status, Status::Stalled);
        assert_eq!(
            snapshot.checks.iter().filter(|check| check.stale).count(),
            1
        );
        assert!(snapshot.checks[1].stale);
        checkpoints[1].completed();
        assert_eq!(health.snapshot().status, Status::Healthy);
    }

    #[tokio::test(start_paused = true)]
    async fn hung_game_tick_leaves_health_endpoint_responsive() {
        let health = Health::new();
        let others = register_three(&health);
        complete_all(&others);
        let game_lock = Arc::new(tokio::sync::Mutex::new(()));
        let held = game_lock.clone().lock_owned().await;
        let (_shutdown, rx) = watch::channel(());
        let task = tokio::spawn(crate::run_ticks(
            "test_movement",
            Duration::from_millis(200),
            rx,
            &health,
            Deadline::FromPeriod,
            move || {
                let game_lock = game_lock.clone();
                async move {
                    let _guard = game_lock.lock().await;
                }
            },
        ));
        assert_eq!(health.snapshot().checks[3].name, "test_movement");
        tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(30)).await;
        complete_all(&others);
        assert_eq!(
            super::health(State(health.clone()))
                .await
                .into_response()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(health.snapshot().checks[3].age_ms, 30_000);
        drop(held);
        tokio::task::yield_now().await;
        assert_eq!(health.snapshot().status, Status::Healthy);
        task.abort();
        let _ = task.await;
    }

    #[tokio::test(start_paused = true)]
    async fn fixed_deadline_outlives_default() {
        let health = Health::new();
        let checkpoints = register_three(&health);
        complete_all(&checkpoints);
        tokio::time::advance(Duration::from_secs(89)).await;
        checkpoints[0].completed();
        checkpoints[1].completed();
        assert_eq!(health.snapshot().status, Status::Healthy);
        tokio::time::advance(SECOND).await;
        assert_eq!(health.snapshot().status, Status::Stalled);
        assert!(health.snapshot().checks[2].stale);
    }

    #[tokio::test(start_paused = true)]
    async fn panicking_tick_still_counts_as_progress() {
        let health = Health::new();
        let (_shutdown, rx) = watch::channel(());
        let task = tokio::spawn(crate::run_ticks(
            "panicky",
            SECOND,
            rx,
            &health,
            Deadline::FromPeriod,
            || async { panic!("boom") },
        ));
        tokio::task::yield_now().await;
        assert_eq!(health.snapshot().status, Status::Healthy);
        task.abort();
        let _ = task.await;
    }

    #[test]
    #[should_panic(expected = "health registry full")]
    fn overflow_panics_at_registration() {
        let health = Health::new();
        for _ in 0..=MAX_SLOTS {
            health.register("loop", SECOND, Deadline::FromPeriod);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn endpoint_maps_status_to_http_code() {
        let health = Health::new();
        let checkpoints = register_three(&health);
        let response = super::health(State(health.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        complete_all(&checkpoints);
        let response = super::health(State(health.clone())).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "healthy");
        let check = &json["checks"][0];
        for key in [
            "name",
            "last_completed_unix_ms",
            "age_ms",
            "timeout_ms",
            "stale",
        ] {
            assert!(check.get(key).is_some(), "missing {key}");
        }
        health.stop();
        assert_eq!(
            super::health(State(health)).await.into_response().status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
}
