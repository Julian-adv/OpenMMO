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
        .with_state(MetricsState { game, auth })
}

pub async fn record_concurrent_sample(game: &GameState, auth: Arc<AuthService>) {
    let counts = game.concurrent_account_counts().await;
    let now = unix_now();
    if let Err(error) = auth_db(move || auth.record_concurrent_accounts(now, counts)).await {
        warn!("Concurrent account snapshot failed: {error}");
    }
}

async fn concurrent_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    let interval = match hours {
        1 | 6 | 24 => SAMPLE_INTERVAL_SECONDS,
        168 => 600,
        720 => 3600,
        4320 => 21600,
        8760 => 86400,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "hours must be 1, 6, 24, 168, 720, 4320, or 8760",
            )
                .into_response();
        }
    };

    let counts = state.game.concurrent_account_counts().await;
    let until = unix_now();
    let from = until - i64::from(hours) * 3600;
    let samples =
        match auth_db(move || state.auth.concurrent_account_samples(from, until, interval)).await {
            Ok(samples) => samples,
            Err(error) => {
                warn!("Concurrent account history failed: {error}");
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    [(header::CACHE_CONTROL, "no-store")],
                    "Metrics are temporarily unavailable",
                )
                    .into_response();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::tests::{make_test_auth, make_test_game_state};

    #[tokio::test]
    async fn concurrent_endpoint_limits_ranges_and_distinguishes_empty_history() {
        let auth = Arc::new(make_test_auth("metrics_endpoint"));
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
        task.abort();
    }
}
