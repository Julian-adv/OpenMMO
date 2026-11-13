use super::{auth_db, metrics_response, unix_now, MetricsState};
use axum::{extract::State, response::Response};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyEventCounts {
    pub founded: u64,
    /// Families absorbed into another by a merger.
    pub merged: u64,
    pub dissolved: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyDayCounts {
    /// KST midnight.
    pub day_start: i64,
    #[serde(flatten)]
    pub counts: FamilyEventCounts,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopFamily {
    pub name: String,
    pub created_at: i64,
    pub members: u32,
    pub estates: u32,
    /// `None` until a family left headless by a deletion is settled.
    pub head: Option<String>,
    pub elders: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FamilyMetrics {
    pub until: i64,
    pub collection_started_at: i64,
    pub active: u64,
    pub members: u64,
    pub total: FamilyEventCounts,
    /// Oldest first, ending today.
    pub daily: Vec<FamilyDayCounts>,
    pub top: Vec<TopFamily>,
}

const DAYS: i64 = 30;
const TOP_FAMILIES: usize = 10;

pub(super) async fn families(State(state): State<MetricsState>) -> Response {
    let until = unix_now();
    metrics_response(
        auth_db(move || state.auth.family_metrics(until, DAYS, TOP_FAMILIES)).await,
        "Families",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::AuthService, game_state::tests::make_test_game_state};
    use axum::http::StatusCode;
    use std::sync::Arc;

    #[tokio::test]
    async fn families_report_an_empty_server_as_thirty_zero_days() {
        let dir = crate::test_util::unique_temp_dir("families_api");
        let auth = Arc::new(AuthService::new(dir.join("game.db")).unwrap());
        let game = Arc::new(make_test_game_state("families_api"));
        let router = crate::metrics::metrics_routes(game, auth);
        let url = format!(
            "{}/api/metrics/families",
            crate::test_util::serve(router).await
        );

        let response = reqwest::get(&url).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let data: FamilyMetrics = response.json().await.unwrap();
        assert_eq!((data.active, data.members), (0, 0));
        assert_eq!(data.total, FamilyEventCounts::default());
        assert_eq!(data.daily.len(), 30);
        assert!(data.top.is_empty());
        assert!(data.collection_started_at <= data.until);
    }
}
