mod access_log;
mod store;

use crate::{auth::unix_now, game_state::GameState};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc, RwLock},
    time::{Duration, Instant},
};
use tokio::sync::watch;
use tracing::warn;

#[derive(Clone)]
pub(crate) struct Config {
    pub path: PathBuf,
    pub interface: Option<String>,
    pub access_log: Option<PathBuf>,
    pub interval_seconds: u64,
}

#[derive(Clone, Default, Serialize)]
struct Health {
    network_error: bool,
    log_error: bool,
}

struct SampleRequest {
    accounts: u32,
    captured_at: Instant,
}

#[derive(Clone)]
pub(crate) struct TrafficMetrics {
    config: Config,
    health: Arc<RwLock<Health>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct NetworkSample {
    timestamp: i64,
    interface: String,
    seconds: f64,
    rx_bytes: u64,
    tx_bytes: u64,
    accounts: u32,
}

#[derive(Serialize)]
struct NetworkHistorySample {
    timestamp: i64,
    seconds: f64,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[derive(Serialize)]
struct NetworkHistory {
    from: i64,
    until: i64,
    collection_started_at: Option<i64>,
    collection_interval_seconds: u64,
    sample_interval_seconds: u64,
    available: bool,
    latest: Option<NetworkSample>,
    samples: Vec<NetworkHistorySample>,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct AccessStatus {
    started_at: i64,
    updated_at: i64,
    skipped_lines: u64,
    gaps: u64,
    pending_bytes: u64,
}

#[derive(Serialize)]
struct AssetEntry {
    category: String,
    path: String,
    bytes: u64,
    requests: u64,
    revalidations: u64,
}

#[derive(Serialize)]
struct AssetTraffic {
    from: i64,
    until: i64,
    configured: bool,
    available: bool,
    status: Option<AccessStatus>,
    total_bytes: u64,
    categories: Vec<AssetEntry>,
    files: Vec<AssetEntry>,
}

impl TrafficMetrics {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            health: Arc::new(RwLock::new(Health::default())),
        }
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/api/metrics/network", get(network))
            .route("/api/metrics/asset-traffic", get(assets))
            .with_state(self.clone())
    }

    pub async fn run(self, game: Arc<GameState>, mut shutdown: watch::Receiver<()>) {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = self.clone();
        let handle = match std::thread::Builder::new()
            .name("traffic-metrics".into())
            .spawn(move || worker.collect(receiver))
        {
            Ok(handle) => handle,
            Err(error) => {
                warn!("Traffic worker failed to start: {error}");
                *self.health.write().unwrap_or_else(|e| e.into_inner()) = Health {
                    network_error: true,
                    log_error: true,
                };
                return;
            }
        };
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.interval_seconds));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = interval.tick() => {
                    let accounts = game.concurrent_account_counts().await.total();
                    let request = SampleRequest { accounts, captured_at: Instant::now() };
                    if let Err(mpsc::TrySendError::Disconnected(_)) = sender.try_send(request) { break; }
                }
            }
        }
        drop(sender);
        if !matches!(
            tokio::task::spawn_blocking(move || handle.join()).await,
            Ok(Ok(()))
        ) {
            warn!("Traffic worker stopped unexpectedly");
            *self.health.write().unwrap_or_else(|e| e.into_inner()) = Health {
                network_error: true,
                log_error: true,
            };
        }
    }

    fn collect(&self, receiver: mpsc::Receiver<SampleRequest>) {
        let mut connection = None;
        let mut baseline: Option<Counters> = None;
        let mut last_log = None;
        for request in receiver {
            let now = unix_now();
            if connection.is_none() {
                match store::open_writer(&self.config.path) {
                    Ok(conn) => connection = Some(conn),
                    Err(error) => {
                        warn!("Traffic database unavailable: {error}");
                        *self.health.write().unwrap_or_else(|e| e.into_inner()) = Health {
                            network_error: true,
                            log_error: true,
                        };
                        continue;
                    }
                }
            }
            let Some(conn) = connection.as_ref() else {
                continue;
            };
            let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                if request.captured_at.elapsed() > Duration::from_secs(5) {
                    return Err("Skipped delayed account snapshot".into());
                }
                let current = read_counters(self.config.interface.as_deref())?;
                if let Some(sample) = baseline
                    .as_ref()
                    .and_then(|previous| current.sample(previous, request.accounts, now))
                {
                    store::save_network(conn, &sample)?;
                }
                baseline = Some(current);
                Ok(())
            })();
            if let Err(error) = &result {
                baseline = None;
                warn!("Network metrics sample failed: {error}");
            }
            self.health
                .write()
                .unwrap_or_else(|e| e.into_inner())
                .network_error = result.is_err();
            let log_bucket = now.div_euclid(600);
            if last_log != Some(log_bucket) {
                last_log = Some(log_bucket);
                if let Some(path) = &self.config.access_log {
                    let result = access_log::collect(conn, path, now);
                    if let Err(error) = &result {
                        warn!("Access log metrics failed: {error}");
                    }
                    self.health
                        .write()
                        .unwrap_or_else(|e| e.into_inner())
                        .log_error = result.is_err();
                }
                if let Err(error) = store::prune(conn, now) {
                    warn!("Traffic retention failed: {error}");
                }
            }
        }
    }
}

#[derive(Clone)]
struct Counters {
    interface: String,
    index: String,
    rx: u64,
    tx: u64,
    at: Instant,
}

impl Counters {
    fn sample(&self, previous: &Self, accounts: u32, timestamp: i64) -> Option<NetworkSample> {
        if self.interface != previous.interface || self.index != previous.index {
            return None;
        }
        let seconds = self.at.duration_since(previous.at).as_secs_f64();
        if seconds <= 0.0 {
            return None;
        }
        Some(NetworkSample {
            timestamp,
            interface: self.interface.clone(),
            seconds,
            rx_bytes: self.rx.checked_sub(previous.rx)?,
            tx_bytes: self.tx.checked_sub(previous.tx)?,
            accounts,
        })
    }
}

fn default_interface(routes: &str) -> Option<String> {
    routes
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            if fields.len() < 8
                || fields[1] != "00000000"
                || fields[7] != "00000000"
                || fields[0] == "lo"
            {
                return None;
            }
            let flags = u32::from_str_radix(fields[3], 16).ok()?;
            (flags & 1 != 0).then_some((fields[6].parse::<u32>().ok()?, fields[0]))
        })
        .min_by_key(|(metric, _)| *metric)
        .map(|(_, name)| name.to_owned())
}

fn read_counters(configured: Option<&str>) -> std::io::Result<Counters> {
    let interface =
        match configured {
            Some(name) => name.to_owned(),
            None => default_interface(&std::fs::read_to_string("/proc/net/route")?).ok_or_else(
                || std::io::Error::other("Set NETWORK_INTERFACE when no IPv4 default route exists"),
            )?,
        };
    if interface.is_empty()
        || interface == "lo"
        || interface.contains('/')
        || interface.contains("..")
    {
        return Err(std::io::Error::other("Invalid external network interface"));
    }
    let root = PathBuf::from("/sys/class/net").join(&interface);
    let read = |name| -> std::io::Result<u64> {
        std::fs::read_to_string(root.join("statistics").join(name))?
            .trim()
            .parse()
            .map_err(std::io::Error::other)
    };
    Ok(Counters {
        index: std::fs::read_to_string(root.join("ifindex"))?,
        rx: read("rx_bytes")?,
        tx: read("tx_bytes")?,
        interface,
        at: Instant::now(),
    })
}

#[derive(Deserialize)]
struct QueryHours {
    hours: Option<u32>,
}

fn hours(query: QueryHours) -> Result<u32, (StatusCode, &'static str)> {
    match query.hours.unwrap_or(24) {
        hours @ (1 | 24 | 168 | 720) => Ok(hours),
        _ => Err((StatusCode::BAD_REQUEST, "hours must be 1, 24, 168, or 720")),
    }
}

async fn network(State(state): State<TrafficMetrics>, Query(query): Query<QueryHours>) -> Response {
    let hours = match hours(query) {
        Ok(hours) => hours,
        Err(error) => return error.into_response(),
    };
    let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<NetworkHistory> {
        let until = unix_now();
        let mut data = NetworkHistory {
            from: until - i64::from(hours) * 3600,
            until,
            collection_started_at: None,
            collection_interval_seconds: state.config.interval_seconds,
            sample_interval_seconds: state.config.interval_seconds.max(match hours {
                168 => 600,
                720 => 1800,
                _ => 60,
            }),
            available: !state
                .health
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .network_error,
            latest: None,
            samples: vec![],
            rx_bytes: 0,
            tx_bytes: 0,
        };
        if state.config.path.try_exists().unwrap_or(true) {
            store::network_history(&store::open_reader(&state.config.path)?, &mut data)?;
        }
        Ok(data)
    })
    .await;
    respond(result)
}

async fn assets(State(state): State<TrafficMetrics>, Query(query): Query<QueryHours>) -> Response {
    let hours = match hours(query) {
        Ok(hours) => hours,
        Err(error) => return error.into_response(),
    };
    let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<AssetTraffic> {
        let now = unix_now();
        let until = now - now.rem_euclid(600);
        let mut data = AssetTraffic {
            from: until - i64::from(hours) * 3600,
            until,
            configured: state.config.access_log.is_some(),
            available: !state
                .health
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .log_error,
            status: None,
            total_bytes: 0,
            categories: vec![],
            files: vec![],
        };
        if state.config.path.try_exists().unwrap_or(true) {
            let conn = store::open_reader(&state.config.path)?;
            if let Some(path) = state.config.access_log {
                data.status = store::access_status(&conn, &path.to_string_lossy())?;
            }
            store::asset_traffic(&conn, &mut data)?;
        }
        Ok(data)
    })
    .await;
    respond(result)
}

fn respond<T: Serialize>(result: Result<rusqlite::Result<T>, tokio::task::JoinError>) -> Response {
    match result {
        Ok(Ok(data)) => {
            return ([(header::CACHE_CONTROL, "no-store")], Json(data)).into_response();
        }
        Ok(Err(error)) => warn!("Traffic query failed: {error}"),
        Err(error) => warn!("Traffic query task failed: {error}"),
    }
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CACHE_CONTROL, "no-store")],
        "Traffic metrics unavailable",
    )
        .into_response()
}

#[cfg(test)]
mod tests;
