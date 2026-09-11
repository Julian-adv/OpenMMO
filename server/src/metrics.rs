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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConcurrentSample {
    pub timestamp: i64,
    pub accounts: u32,
}

#[derive(Serialize, Deserialize)]
struct ConcurrentHistory {
    from: i64,
    until: i64,
    sample_interval_seconds: i64,
    current: ConcurrentSample,
    samples: Vec<ConcurrentSample>,
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
    let accounts = game.concurrent_account_count().await;
    let now = unix_now();
    if let Err(error) = auth_db(move || auth.record_concurrent_accounts(now, accounts)).await {
        warn!("Concurrent account snapshot failed: {error}");
    }
}

async fn concurrent_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    if !matches!(hours, 1 | 6 | 24) {
        return (StatusCode::BAD_REQUEST, "hours must be 1, 6, or 24").into_response();
    }

    let accounts = state.game.concurrent_account_count().await;
    let until = unix_now();
    let from = until - i64::from(hours) * 3600;
    let samples = match auth_db(move || state.auth.concurrent_account_samples(from, until)).await {
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
            sample_interval_seconds: SAMPLE_INTERVAL_SECONDS,
            current: ConcurrentSample {
                timestamp: until,
                accounts,
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
        assert!(body.samples.is_empty());

        auth.record_concurrent_accounts(unix_now() - 7200, 9)
            .unwrap();
        record_concurrent_sample(&game, Arc::clone(&auth)).await;

        for hours in [1, 6, 24] {
            let response = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body: ConcurrentHistory =
                serde_json::from_slice(&response.bytes().await.unwrap()).unwrap();
            assert_eq!(body.until - body.from, hours * 3600);
            assert_eq!(body.current.accounts, 0);
            assert_eq!(body.samples.len(), if hours == 1 { 1 } else { 2 });
            assert_eq!(body.samples.last().unwrap().accounts, 0);
        }

        for hours in ["0", "25", "99999999999999999999", "-1", "invalid"] {
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
