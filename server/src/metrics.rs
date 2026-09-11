use crate::auth::{unix_now, AuthService};
use crate::game_state::{auth_db, GameState};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::warn;

pub const SAMPLE_INTERVAL_SECONDS: i64 = 60;
pub const DAY_SECONDS: i64 = 86400;
pub const UNIQUE_PERIOD_DAYS: [u32; 5] = [1, 7, 30, 180, 365];

pub fn kst_day_start(timestamp: i64) -> i64 {
    timestamp - (timestamp + 9 * 3600).rem_euclid(DAY_SECONDS)
}

#[derive(Clone)]
pub struct AccountActivity {
    pub id: String,
    pub account_name: String,
    pub started_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UniqueSample {
    pub timestamp: i64,
    pub accounts: u32,
}

#[derive(Serialize, Deserialize)]
pub struct UniqueHistory {
    pub from: i64,
    pub until: i64,
    pub window_seconds: i64,
    pub sample_interval_seconds: i64,
    pub collection_started_at: i64,
    pub last_aggregated_at: Option<i64>,
    pub samples: Vec<UniqueSample>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConcurrentCounts {
    pub web_accounts: u32,
    pub agent_accounts: u32,
    pub other_accounts: u32,
}

impl ConcurrentCounts {
    pub fn total(&self) -> u32 {
        self.web_accounts + self.agent_accounts + self.other_accounts
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConcurrentSample {
    pub timestamp: i64,
    pub accounts: u32,
    #[serde(flatten)]
    pub counts: ConcurrentCounts,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ConcurrentHistorySample {
    pub timestamp: i64,
    pub accounts: f64,
    pub web_accounts: f64,
    pub agent_accounts: f64,
    pub other_accounts: f64,
    pub peak_accounts: u32,
    pub peak_timestamp: i64,
    pub sample_count: u32,
}

#[derive(Serialize, Deserialize)]
struct ConcurrentHistory {
    from: i64,
    until: i64,
    sample_interval_seconds: i64,
    current: ConcurrentSample,
    samples: Vec<ConcurrentHistorySample>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldSample {
    pub timestamp: i64,
    pub total_gold: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct GoldHistorySample {
    pub timestamp: i64,
    pub total_gold: f64,
    pub peak_gold: i64,
    pub sample_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct GoldHistory {
    pub from: i64,
    pub until: i64,
    pub sample_interval_seconds: i64,
    pub latest: Option<GoldSample>,
    pub samples: Vec<GoldHistorySample>,
}

#[derive(Clone)]
struct MetricsState {
    game: Arc<GameState>,
    auth: Arc<AuthService>,
}

#[derive(Deserialize)]
struct HistoryQuery {
    hours: Option<u32>,
}

pub fn metrics_router(game: Arc<GameState>, auth: Arc<AuthService>) -> Router {
    Router::new()
        .route("/api/metrics/concurrent", get(concurrent_history))
        .route("/api/metrics/unique", get(unique_history))
        .route("/api/metrics/gold", get(gold_history))
        .with_state(MetricsState { game, auth })
}

pub async fn record_concurrent_sample(game: &GameState, auth: Arc<AuthService>) {
    let counts = game.concurrent_account_counts().await;
    let activities = game.account_activity_snapshot().await;
    let now = unix_now();
    if let Err(error) = auth_db(move || {
        if let Err(error) = auth.record_account_activities(&activities) {
            warn!("Account activity snapshot failed: {error}");
        } else if let Err(error) = auth.aggregate_daily_unique_accounts(now) {
            warn!("Daily unique account aggregation failed: {error}");
        }
        auth.record_concurrent_accounts(now, counts)
    })
    .await
    {
        warn!("Concurrent account snapshot failed: {error}");
    }
}

fn history_interval(hours: u32) -> Option<i64> {
    match hours {
        1 | 6 | 24 => Some(SAMPLE_INTERVAL_SECONDS),
        168 => Some(600),
        720 => Some(3600),
        4320 => Some(21600),
        8760 => Some(86400),
        _ => None,
    }
}

fn invalid_hours() -> Response {
    (
        StatusCode::BAD_REQUEST,
        "hours must be 1, 6, 24, 168, 720, 4320, or 8760",
    )
        .into_response()
}

fn metrics_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        [(header::CACHE_CONTROL, "no-store")],
        "Metrics are temporarily unavailable",
    )
        .into_response()
}

async fn unique_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    if !hours.is_multiple_of(24) || !UNIQUE_PERIOD_DAYS.contains(&(hours / 24)) {
        return (
            StatusCode::BAD_REQUEST,
            "hours must be 24, 168, 720, 4320, or 8760",
        )
            .into_response();
    }
    match auth_db(move || state.auth.unique_account_history(unix_now(), hours / 24)).await {
        Ok(history) => ([(header::CACHE_CONTROL, "no-store")], Json(history)).into_response(),
        Err(error) => {
            warn!("Unique account history failed: {error}");
            metrics_unavailable()
        }
    }
}

async fn concurrent_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    let Some(interval) = history_interval(hours) else {
        return invalid_hours();
    };

    let counts = state.game.concurrent_account_counts().await;
    let until = unix_now();
    let from = until - i64::from(hours) * 3600;
    let samples =
        match auth_db(move || state.auth.concurrent_account_samples(from, until, interval)).await {
            Ok(samples) => samples,
            Err(error) => {
                warn!("Concurrent account history failed: {error}");
                return metrics_unavailable();
            }
        };
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(ConcurrentHistory {
            from,
            until,
            sample_interval_seconds: interval,
            current: ConcurrentSample {
                timestamp: until,
                accounts: counts.total(),
                counts,
            },
            samples,
        }),
    )
        .into_response()
}

async fn gold_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    let interval = match hours {
        1 | 24 | 168 | 720 => 3600,
        4320 => 21600,
        8760 => DAY_SECONDS,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "hours must be 1, 24, 168, 720, 4320, or 8760",
            )
                .into_response();
        }
    };
    match auth_db(move || state.auth.gold_history(unix_now(), hours, interval)).await {
        Ok(history) => ([(header::CACHE_CONTROL, "no-store")], Json(history)).into_response(),
        Err(error) => {
            warn!("Gold history failed: {error}");
            metrics_unavailable()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::tests::make_test_game_state;

    #[tokio::test]
    async fn gold_endpoint_returns_saved_totals_for_all_six_periods() {
        let path = crate::test_util::unique_temp_dir("gold_endpoint").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("gold_endpoint"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/gold");
        let empty: GoldHistory = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(empty.until - empty.from, 86400);
        assert_eq!(empty.latest, None);
        assert!(empty.samples.is_empty());
        let now = unix_now();
        auth.record_gold_snapshot(now - 364 * DAY_SECONDS, 30)
            .unwrap();
        auth.record_gold_snapshot(now, 30).unwrap();
        for (hours, interval) in [
            (1, 3600),
            (24, 3600),
            (168, 3600),
            (720, 3600),
            (4320, 21600),
            (8760, 86400),
        ] {
            let response = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let body: GoldHistory = response.json().await.unwrap();
            assert_eq!(body.until - body.from, hours * 3600);
            assert_eq!(body.sample_interval_seconds, interval);
            assert_eq!(body.latest.unwrap().total_gold, 0);
            assert_eq!(
                body.samples
                    .iter()
                    .map(|sample| sample.sample_count)
                    .sum::<u32>(),
                if hours == 8760 { 2 } else { 1 }
            );
        }
        for hours in [
            "0",
            "6",
            "25",
            "8761",
            "-1",
            "invalid",
            "99999999999999999999",
        ] {
            assert_eq!(
                client
                    .get(format!("{url}?hours={hours}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        rusqlite::Connection::open(path)
            .unwrap()
            .execute("DROP TABLE gold_snapshots", [])
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn account_endpoints_limit_ranges_and_preserve_distinct_counts() {
        let path = crate::test_util::unique_temp_dir("metrics_endpoint").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("metrics_endpoint"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(Arc::clone(&game), Arc::clone(&auth));
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/concurrent");
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: ConcurrentHistory =
            serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
        assert_eq!(body.until - body.from, 86400);
        assert_eq!(body.current.accounts, 0);
        assert_eq!(body.current.counts, ConcurrentCounts::default());
        assert!(body.samples.is_empty());

        auth.record_concurrent_accounts(
            unix_now() - 7200,
            ConcurrentCounts {
                web_accounts: 5,
                agent_accounts: 4,
                other_accounts: 0,
            },
        )
        .unwrap();
        auth.record_concurrent_accounts(
            unix_now() - 364 * 86400,
            ConcurrentCounts {
                other_accounts: 12,
                ..Default::default()
            },
        )
        .unwrap();
        record_concurrent_sample(&game, Arc::clone(&auth)).await;

        for (hours, interval) in [
            (1, 60),
            (6, 60),
            (24, 60),
            (168, 600),
            (720, 3600),
            (4320, 21600),
            (8760, 86400),
        ] {
            let response = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: ConcurrentHistory =
                serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
            assert_eq!(body.until - body.from, hours * 3600);
            assert_eq!(body.sample_interval_seconds, interval);
            assert_eq!(body.current.accounts, 0);
            assert_eq!(body.current.counts.total(), body.current.accounts);
            assert!(body.samples.iter().all(|sample| (sample.accounts
                - sample.web_accounts
                - sample.agent_accounts
                - sample.other_accounts)
                .abs()
                < 1e-9));
            assert_eq!(
                body.samples
                    .iter()
                    .map(|sample| sample.sample_count)
                    .sum::<u32>(),
                match hours {
                    1 => 1,
                    8760 => 3,
                    _ => 2,
                }
            );
            assert_eq!(
                body.samples.iter().map(|sample| sample.peak_accounts).max(),
                Some(match hours {
                    1 => 0,
                    8760 => 12,
                    _ => 9,
                })
            );
            assert!(body.samples.len() <= (hours * 3600 / interval + 1) as usize);
            if hours <= 24 {
                assert_eq!(body.samples.last().unwrap().accounts, 0.0);
            }
        }

        for hours in ["0", "25", "8761", "99999999999999999999", "-1", "invalid"] {
            assert_eq!(
                client
                    .get(format!("{url}?hours={hours}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        let now = kst_day_start(unix_now());
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute(
                "UPDATE unique_account_collection SET started_at = ?1",
                [now - 2 * 365 * 86400],
            )
            .unwrap();
        auth.record_account_activities(&[
            AccountActivity {
                id: "old".into(),
                account_name: "alice".into(),
                started_at: now - 364 * 86400,
                last_seen_at: now - 364 * 86400,
            },
            AccountActivity {
                id: "recent".into(),
                account_name: "alice".into(),
                started_at: now - 7200,
                last_seen_at: now - 7100,
            },
            AccountActivity {
                id: "short".into(),
                account_name: "bob".into(),
                started_at: now - 30,
                last_seen_at: now - 30,
            },
        ])
        .unwrap();
        let pending = auth.unique_account_history(now, 1).unwrap();
        assert!(pending.samples.is_empty());
        assert_eq!(pending.last_aggregated_at, None);
        assert!(auth.aggregate_daily_unique_accounts(now).unwrap());
        let unique_url = format!("http://{addr}/api/metrics/unique");
        let default: UniqueHistory = client
            .get(&unique_url)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(default.window_seconds, 86400);
        for days in UNIQUE_PERIOD_DAYS {
            let hours = i64::from(days) * 24;
            let response = client
                .get(format!("{unique_url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let json: serde_json::Value = response.json().await.unwrap();
            assert!(!json.to_string().contains("alice"));
            let body: UniqueHistory = serde_json::from_value(json).unwrap();
            assert_eq!(body.window_seconds, hours * 3600);
            assert_eq!(body.until - body.from, hours * 3600);
            assert_eq!(body.sample_interval_seconds, DAY_SECONDS);
            assert_eq!(body.samples.last().unwrap().accounts, 2);
            assert_eq!(body.samples.len(), 1);
            assert_eq!(body.last_aggregated_at, Some(now));
            assert_eq!(body.samples.last().unwrap().timestamp, body.until);
            assert!(body.samples.len() <= days as usize + 1);
        }
        for hours in [
            "0",
            "1",
            "6",
            "25",
            "8761",
            "99999999999999999999",
            "-1",
            "invalid",
        ] {
            assert_eq!(
                client
                    .get(format!("{unique_url}?hours={hours}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute("DROP TABLE account_activity_sessions", [])
            .unwrap();
        let response = client.get(&unique_url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cached: UniqueHistory = response.json().await.unwrap();
        assert_eq!(cached.samples.last().unwrap().accounts, 2);
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute("DROP TABLE unique_account_daily_samples", [])
            .unwrap();
        let response = client.get(&unique_url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }
}
