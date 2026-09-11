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

pub const SAMPLE_INTERVAL_SECONDS: i64 = 3600;
pub const DAY_SECONDS: i64 = 86400;
pub const UNIQUE_PERIOD_DAYS: [u32; 5] = [1, 7, 30, 180, 365];

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum GoldSource {
    ItemSale { item_def_id: String },
    DungeonChest,
    CoinPile,
    CoinPouch,
    NpcSalary,
}

impl GoldSource {
    pub fn storage_key(&self) -> (&str, &str) {
        match self {
            Self::ItemSale { item_def_id } => ("item_sale", item_def_id),
            Self::DungeonChest => ("dungeon_chest", ""),
            Self::CoinPile => ("coin_pile", ""),
            Self::CoinPouch => ("coin_pouch", ""),
            Self::NpcSalary => ("npc_salary", ""),
        }
    }
}

#[derive(Clone)]
pub struct GoldSourceRecord {
    pub timestamp: i64,
    pub source: GoldSource,
    pub quantity: u64,
    pub gold: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemGoldSource {
    #[serde(flatten)]
    pub source: GoldSource,
    pub name: String,
    pub quantity: u64,
    pub gold: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ItemGoldSources {
    pub from: i64,
    pub until: i64,
    pub collection_started_at: i64,
    pub rewards_started_at: i64,
    pub total_gold: i64,
    pub entries: Vec<ItemGoldSource>,
}

#[derive(Serialize, Deserialize)]
pub struct CharacterLeaderboard<Entry, Series> {
    pub timestamp: i64,
    pub from: i64,
    pub sample_interval_seconds: i64,
    pub entries: Vec<Entry>,
    pub series: Vec<Series>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LevelLeaderboardEntry {
    pub name: String,
    pub level: u32,
    pub account_first_rank: usize,
}

pub type LevelLeaderboard = CharacterLeaderboard<LevelLeaderboardEntry, LevelSeries>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LevelSample {
    pub timestamp: i64,
    pub level: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LevelSeries {
    pub name: String,
    pub started_at: i64,
    pub samples: Vec<LevelSample>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldLeaderboardEntry {
    pub name: String,
    pub gold: i64,
    pub account_first_rank: usize,
}

pub type GoldLeaderboard = CharacterLeaderboard<GoldLeaderboardEntry, CharacterGoldSeries>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterGoldSample {
    pub timestamp: i64,
    pub gold: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CharacterGoldSeries {
    pub name: String,
    pub started_at: i64,
    pub samples: Vec<CharacterGoldSample>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeaponEnchantLeaderboardEntry {
    pub name: String,
    pub weapon_enchant: u32,
    pub account_first_rank: usize,
}

pub type WeaponEnchantLeaderboard =
    CharacterLeaderboard<WeaponEnchantLeaderboardEntry, WeaponEnchantSeries>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeaponEnchantSample {
    pub timestamp: i64,
    pub weapon_enchant: u32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WeaponEnchantSeries {
    pub name: String,
    pub started_at: i64,
    pub samples: Vec<WeaponEnchantSample>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArmorEnchantLeaderboardEntry {
    pub name: String,
    pub armor_enchant: u64,
    pub account_first_rank: usize,
}

pub type ArmorEnchantLeaderboard =
    CharacterLeaderboard<ArmorEnchantLeaderboardEntry, ArmorEnchantSeries>;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArmorEnchantSample {
    pub timestamp: i64,
    pub armor_enchant: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArmorEnchantSeries {
    pub name: String,
    pub started_at: i64,
    pub samples: Vec<ArmorEnchantSample>,
}

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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PerAccountGoldSample {
    pub timestamp: i64,
    pub total_gold: i64,
    pub accounts: u32,
    pub gold_per_account: f64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct PerAccountGoldHistorySample {
    pub timestamp: i64,
    pub gold_per_account: f64,
    pub peak_gold_per_account: f64,
    pub sample_count: u32,
}

#[derive(Serialize, Deserialize)]
pub struct PerAccountGoldHistory {
    pub from: i64,
    pub until: i64,
    pub sample_interval_seconds: i64,
    pub window_seconds: i64,
    pub collection_started_at: i64,
    pub latest: Option<PerAccountGoldSample>,
    pub samples: Vec<PerAccountGoldHistorySample>,
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

#[derive(Deserialize)]
struct PerAccountGoldQuery {
    hours: Option<u32>,
    active_hours: Option<u32>,
}

pub fn metrics_router(game: Arc<GameState>, auth: Arc<AuthService>) -> Router {
    Router::new()
        .route("/api/metrics/concurrent", get(concurrent_history))
        .route("/api/metrics/unique", get(unique_history))
        .route("/api/metrics/gold", get(gold_history))
        .route("/api/metrics/item-gold-sources", get(item_gold_sources))
        .route("/api/metrics/level-leaderboard", get(level_leaderboard))
        .route("/api/metrics/gold-leaderboard", get(gold_leaderboard))
        .route(
            "/api/metrics/weapon-enchant-leaderboard",
            get(weapon_enchant_leaderboard),
        )
        .route(
            "/api/metrics/armor-enchant-leaderboard",
            get(armor_enchant_leaderboard),
        )
        .route(
            "/api/metrics/gold-per-account",
            get(per_account_gold_history),
        )
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

pub async fn record_hourly_metrics(game: &GameState, auth: Arc<AuthService>) {
    game.flush_dirty_saves(&auth).await;
    game.flush_gold_sources(&auth, unix_now(), false).await;
    game.tick_gold_snapshot(&auth).await;
    let history_auth = Arc::clone(&auth);
    if let Err(error) =
        auth_db(move || history_auth.record_hourly_character_metrics(unix_now())).await
    {
        warn!("Character metrics snapshot failed: {error}");
    }
    record_concurrent_sample(game, auth).await;
}

fn history_interval(hours: u32) -> Option<i64> {
    match hours {
        1 | 6 | 24 | 168 | 720 => Some(SAMPLE_INTERVAL_SECONDS),
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

fn metrics_response<T: Serialize>(
    result: Result<T, crate::auth::AuthError>,
    label: &str,
) -> Response {
    match result {
        Ok(data) => ([(header::CACHE_CONTROL, "no-store")], Json(data)).into_response(),
        Err(error) => {
            warn!("{label} failed: {error}");
            metrics_unavailable()
        }
    }
}

fn leaderboard_interval(hours: u32) -> Result<i64, (StatusCode, &'static str)> {
    match hours {
        168 | 720 => Ok(3600),
        4320 => Ok(21600),
        8760 => Ok(86400),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "hours must be 168, 720, 4320, or 8760",
        )),
    }
}

async fn level_leaderboard(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    leaderboard_response(
        state.auth,
        query,
        "Level leaderboard",
        AuthService::level_leaderboard,
    )
    .await
}

async fn gold_leaderboard(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    leaderboard_response(
        state.auth,
        query,
        "Gold leaderboard",
        AuthService::gold_leaderboard,
    )
    .await
}

async fn weapon_enchant_leaderboard(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    leaderboard_response(
        state.auth,
        query,
        "Weapon enchant leaderboard",
        AuthService::weapon_enchant_leaderboard,
    )
    .await
}

async fn armor_enchant_leaderboard(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    leaderboard_response(
        state.auth,
        query,
        "Armor enchant leaderboard",
        AuthService::armor_enchant_leaderboard,
    )
    .await
}

async fn leaderboard_response<T: Serialize + Send + 'static>(
    auth: Arc<AuthService>,
    query: HistoryQuery,
    label: &str,
    load: fn(&AuthService, u32, i64) -> Result<T, crate::auth::AuthError>,
) -> Response {
    let hours = query.hours.unwrap_or(168);
    let interval = match leaderboard_interval(hours) {
        Ok(interval) => interval,
        Err(error) => return error.into_response(),
    };
    metrics_response(auth_db(move || load(&auth, hours, interval)).await, label)
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
    metrics_response(
        auth_db(move || state.auth.unique_account_history(unix_now(), hours / 24)).await,
        "Unique account history",
    )
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

fn gold_history_interval(hours: u32) -> Result<i64, (StatusCode, &'static str)> {
    match hours {
        1 | 24 | 168 | 720 => Ok(3600),
        4320 => Ok(21600),
        8760 => Ok(DAY_SECONDS),
        _ => Err((
            StatusCode::BAD_REQUEST,
            "hours must be 1, 24, 168, 720, 4320, or 8760",
        )),
    }
}

async fn gold_history(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    let interval = match gold_history_interval(hours) {
        Ok(interval) => interval,
        Err(response) => return response.into_response(),
    };
    metrics_response(
        auth_db(move || state.auth.gold_history(unix_now(), hours, interval)).await,
        "Gold history",
    )
}

async fn item_gold_sources(
    State(state): State<MetricsState>,
    Query(query): Query<HistoryQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    if let Err(error) = gold_history_interval(hours) {
        return error.into_response();
    }
    metrics_response(
        auth_db(move || state.auth.item_gold_sources(unix_now(), hours)).await,
        "Item gold sources",
    )
}

async fn per_account_gold_history(
    State(state): State<MetricsState>,
    Query(query): Query<PerAccountGoldQuery>,
) -> Response {
    let hours = query.hours.unwrap_or(24);
    let interval = match gold_history_interval(hours) {
        Ok(interval) => interval,
        Err(response) => return response.into_response(),
    };
    let active_hours = query.active_hours.unwrap_or(24);
    if !active_hours.is_multiple_of(24) || !UNIQUE_PERIOD_DAYS.contains(&(active_hours / 24)) {
        return (
            StatusCode::BAD_REQUEST,
            "active_hours must be 24, 168, 720, 4320, or 8760",
        )
            .into_response();
    }
    metrics_response(
        auth_db(move || {
            state
                .auth
                .per_account_gold_history(unix_now(), hours, interval, active_hours / 24)
        })
        .await,
        "Per-account gold history",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_state::tests::make_test_game_state;

    #[tokio::test]
    async fn item_gold_sources_api_validates_periods_and_reports_database_failures() {
        let path = crate::test_util::unique_temp_dir("item_gold_sources_api").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(
            Arc::new(make_test_game_state("item_gold_sources_api")),
            Arc::clone(&auth),
        );
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/item-gold-sources");
        let empty: ItemGoldSources = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(empty.until - empty.from, 24 * 3600);
        assert!(empty.entries.is_empty());
        auth.record_gold_sources(&[GoldSourceRecord {
            timestamp: unix_now() - 3600,
            source: GoldSource::ItemSale {
                item_def_id: "iron_sword".into(),
            },
            quantity: 2,
            gold: 9000,
        }])
        .unwrap();
        auth.record_gold_sources(&[GoldSourceRecord {
            timestamp: unix_now() - 3600,
            source: GoldSource::DungeonChest,
            quantity: 1,
            gold: 10000,
        }])
        .unwrap();
        for hours in [1, 24, 168, 720, 4320, 8760] {
            let response = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let sources: ItemGoldSources = response.json().await.unwrap();
            assert_eq!(sources.until - sources.from, hours * 3600);
            assert_eq!(sources.total_gold, 19000);
            assert_eq!(sources.entries[0].source, GoldSource::DungeonChest);
            assert_eq!(sources.entries[0].quantity, 1);
            assert_eq!(sources.entries[1].quantity, 2);
        }
        for hours in ["0", "6", "-1", "25", "invalid"] {
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
            .execute("DROP TABLE item_sale_samples", [])
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn weapon_enchant_leaderboard_ranks_inventory_maxima_and_returns_history() {
        let path = crate::test_util::unique_temp_dir("weapon_enchant_leaderboard").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("weapon_enchant_leaderboard"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/weapon-enchant-leaderboard");
        let empty: WeaponEnchantLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(empty.entries.is_empty());
        assert!(empty.series.is_empty());
        assert_eq!(empty.timestamp - empty.from, 168 * 3600);
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, 86400)] {
            let history: WeaponEnchantLeaderboard = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(history.timestamp - history.from, hours * 3600);
            assert_eq!(history.sample_interval_seconds, interval);
        }
        for hours in ["0", "24", "169", "-1", "invalid"] {
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
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');",
        )
        .unwrap();
        let sword = |enchant| crate::auth::ItemRow {
            item_def_id: "iron_sword".into(),
            quantity: 1,
            enchant,
            equip_slot: None,
            cape_color: None,
            cape_texture: None,
            locked: false,
        };
        for id in 1..=14 {
            let account = match id {
                13 => "npc_test",
                14 => "npcxplayer",
                _ => "player",
            };
            conn.execute(
                "INSERT INTO characters (id, account_name, character_name) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, account, format!("Hero{id}")],
            )
            .unwrap();
            auth.save_batch(
                &[],
                &[(i64::from(id), vec![sword(if id == 10 { 11 } else { id })])],
                &[],
                &[],
                None,
            )
            .unwrap();
        }
        auth.record_hourly_character_metrics(unix_now()).unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: serde_json::Value = response.json().await.unwrap();
        for entry in body["entries"].as_array().unwrap() {
            assert_eq!(entry.as_object().unwrap().len(), 3);
        }
        let leaderboard: WeaponEnchantLeaderboard = serde_json::from_value(body).unwrap();
        assert_eq!(
            leaderboard
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Hero14", "Hero12", "Hero10", "Hero11", "Hero9", "Hero8", "Hero7", "Hero6",
                "Hero5", "Hero4"
            ]
        );
        assert_eq!(leaderboard.entries[0].weapon_enchant, 14);
        assert_eq!(leaderboard.entries[0].account_first_rank, 1);
        assert!(leaderboard.entries[1..]
            .iter()
            .all(|entry| entry.account_first_rank == 2));
        assert_eq!(leaderboard.series.len(), 10);
        for (entry, series) in leaderboard.entries.iter().zip(&leaderboard.series) {
            assert_eq!(entry.name, series.name);
            assert_eq!(
                entry.weapon_enchant,
                series.samples.last().unwrap().weapon_enchant
            );
        }
        let previous = unix_now() - 86400;
        conn.execute(
            "UPDATE character_weapon_enchant_history SET timestamp = ?1 WHERE character_id = 1",
            [previous],
        )
        .unwrap();
        auth.save_batch(&[], &[(1, vec![sword(15)])], &[], &[], None)
            .unwrap();
        let updated: WeaponEnchantLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(updated.entries[0].name, "Hero1");
        assert_eq!(updated.entries[0].weapon_enchant, 15);
        assert_eq!(updated.entries[2].account_first_rank, 1);
        assert_eq!(
            updated.series[0].samples[0],
            WeaponEnchantSample {
                timestamp: previous,
                weapon_enchant: 1
            }
        );
        assert_eq!(updated.series[0].samples.last().unwrap().weapon_enchant, 1);
        assert!(!auth.record_hourly_character_metrics(unix_now()).unwrap());
        conn.execute("DROP TABLE character_weapon_enchant_history", [])
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn armor_enchant_leaderboard_ranks_slot_totals_and_returns_history() {
        let path = crate::test_util::unique_temp_dir("armor_enchant_leaderboard").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("armor_enchant_leaderboard"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/armor-enchant-leaderboard");
        let empty: ArmorEnchantLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(empty.entries.is_empty());
        assert!(empty.series.is_empty());
        assert_eq!(empty.timestamp - empty.from, 168 * 3600);
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player'), ('npc_test'), ('npcxplayer');",
        )
        .unwrap();
        let armor = |item_def_id: &str, enchant| crate::auth::ItemRow {
            item_def_id: item_def_id.into(),
            quantity: 1,
            enchant,
            equip_slot: None,
            cape_color: None,
            cape_texture: None,
            locked: false,
        };
        for id in 1..=14 {
            let account = match id {
                13 => "npc_test",
                14 => "npcxplayer",
                _ => "player",
            };
            conn.execute(
                "INSERT INTO characters (id, account_name, character_name) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, account, format!("Hero{id}")],
            )
            .unwrap();
            auth.save_batch(
                &[],
                &[(
                    i64::from(id),
                    vec![
                        armor("iron_helmet", if id == 10 { 11 } else { id }),
                        armor("leather_helmet", 1),
                        armor("wooden_shield", 3),
                    ],
                )],
                &[],
                &[],
                None,
            )
            .unwrap();
        }
        auth.record_hourly_character_metrics(unix_now()).unwrap();
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, 86400)] {
            let response = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let body: serde_json::Value = response.json().await.unwrap();
            for entry in body["entries"].as_array().unwrap() {
                assert_eq!(entry.as_object().unwrap().len(), 3);
            }
            let result: ArmorEnchantLeaderboard = serde_json::from_value(body).unwrap();
            assert_eq!(result.timestamp - result.from, hours * 3600);
            assert_eq!(result.sample_interval_seconds, interval);
            assert_eq!(
                result
                    .entries
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>(),
                [
                    "Hero14", "Hero12", "Hero10", "Hero11", "Hero9", "Hero8", "Hero7", "Hero6",
                    "Hero5", "Hero4"
                ]
            );
            assert_eq!(result.entries[0].armor_enchant, 17);
            assert_eq!(result.entries[0].account_first_rank, 1);
            assert!(result.entries[1..]
                .iter()
                .all(|entry| entry.account_first_rank == 2));
            assert_eq!(result.series.len(), 10);
            for (entry, series) in result.entries.iter().zip(&result.series) {
                assert_eq!(entry.name, series.name);
                assert_eq!(
                    entry.armor_enchant,
                    series.samples.last().unwrap().armor_enchant
                );
            }
        }
        for hours in ["0", "24", "169", "-1", "invalid"] {
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
        conn.execute("DROP TABLE character_armor_enchant_history", [])
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn gold_leaderboard_ranks_saved_characters_and_returns_their_history() {
        let path = crate::test_util::unique_temp_dir("gold_leaderboard").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("gold_leaderboard"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/gold-leaderboard");
        let empty: GoldLeaderboard = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(empty.entries.is_empty());
        assert!(empty.series.is_empty());
        assert_eq!(empty.timestamp - empty.from, 168 * 3600);
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, 86400)] {
            let history: GoldLeaderboard = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(history.timestamp - history.from, hours * 3600);
            assert_eq!(history.sample_interval_seconds, interval);
        }
        for hours in ["0", "24", "169", "-1", "invalid"] {
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

        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('player'), ('npc_rich'), ('npcxplayer');
             INSERT INTO characters (account_name, character_name, gold)
             VALUES ('npc_rich', 'OfficialNpc', 999999999999);",
        )
        .unwrap();
        let npc_only: GoldLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(npc_only.entries.is_empty());
        for i in 1..=12 {
            conn.execute(
                "INSERT INTO characters (account_name, character_name, gold, xp)
                 VALUES ('player', ?1, ?2, ?3)",
                rusqlite::params![
                    format!("Hero{i}"),
                    if i == 10 { 1100 } else { i * 100 },
                    i * 1000
                ],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO characters (account_name, character_name, gold)
             VALUES ('npcxplayer', 'LooksLikeNpc', 5000000000);",
        )
        .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: serde_json::Value = response.json().await.unwrap();
        for entry in body["entries"].as_array().unwrap() {
            assert_eq!(entry.as_object().unwrap().len(), 3);
        }
        let leaderboard: GoldLeaderboard = serde_json::from_value(body).unwrap();
        assert_eq!(
            leaderboard
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [
                "LooksLikeNpc",
                "Hero12",
                "Hero10",
                "Hero11",
                "Hero9",
                "Hero8",
                "Hero7",
                "Hero6",
                "Hero5",
                "Hero4"
            ]
        );
        assert_eq!(leaderboard.entries[0].gold, 5000000000);
        assert_eq!(leaderboard.entries[0].account_first_rank, 1);
        assert!(leaderboard.entries[1..]
            .iter()
            .all(|entry| entry.account_first_rank == 2));
        assert_eq!(leaderboard.series.len(), 10);
        for (entry, series) in leaderboard.entries.iter().zip(&leaderboard.series) {
            assert_eq!(entry.name, series.name);
            assert_eq!(entry.gold, series.samples.last().unwrap().gold);
        }

        let previous = unix_now() - 86400;
        conn.execute(
            "UPDATE character_gold_history SET timestamp = ?1
             WHERE character_id = (SELECT id FROM characters WHERE character_name = 'Hero1')",
            [previous],
        )
        .unwrap();
        conn.execute(
            "UPDATE characters SET gold = 6000000000 WHERE character_name = 'Hero1'",
            [],
        )
        .unwrap();
        auth.record_hourly_character_metrics(unix_now()).unwrap();
        let updated: GoldLeaderboard = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(updated.entries[0].name, "Hero1");
        assert_eq!(updated.entries[0].gold, 6000000000);
        assert_eq!(updated.entries[2].account_first_rank, 1);
        assert_eq!(
            updated.series[0].samples[0],
            CharacterGoldSample {
                timestamp: previous,
                gold: 100
            }
        );
        assert_eq!(updated.series[0].samples.last().unwrap().gold, 6000000000);

        conn.execute_batch(
            "DELETE FROM characters WHERE character_name != 'Hero1';
             UPDATE characters SET gold = 0 WHERE character_name = 'Hero1';",
        )
        .unwrap();
        let single: GoldLeaderboard = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(
            single.entries,
            [GoldLeaderboardEntry {
                name: "Hero1".into(),
                gold: 0,
                account_first_rank: 1
            }]
        );
        assert_eq!(single.series[0].samples.last().unwrap().gold, 6000000000);
        assert!(!auth.record_hourly_character_metrics(unix_now()).unwrap());
        conn.execute("DROP TABLE characters", []).unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn level_leaderboard_ranks_saved_characters_and_excludes_npc_accounts() {
        let path = crate::test_util::unique_temp_dir("level_leaderboard").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("level_leaderboard"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/level-leaderboard");
        let empty: LevelLeaderboard = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(empty.entries.is_empty());
        assert!(empty.series.is_empty());
        assert_eq!(empty.timestamp - empty.from, 168 * 3600);
        for (hours, interval) in [(168, 3600), (720, 3600), (4320, 21600), (8760, 86400)] {
            let history: LevelLeaderboard = client
                .get(format!("{url}?hours={hours}"))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(history.timestamp - history.from, hours * 3600);
            assert_eq!(history.sample_interval_seconds, interval);
        }
        for hours in ["0", "24", "169", "-1", "invalid"] {
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

        let conn = rusqlite::Connection::open(path).unwrap();
        let npc = auth.login_npc("npc_leaderboard").unwrap();
        conn.execute(
            "INSERT INTO characters (account_name, character_name, level, xp)
             VALUES (?1, 'OfficialNpc', 99, 999999)",
            [&npc],
        )
        .unwrap();
        let npc_only: LevelLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(npc_only.entries.is_empty());
        for i in 1..=12 {
            let account = auth
                .login_google(&format!("ranking-account-{}", (i - 1) / 3))
                .unwrap();
            conn.execute(
                "INSERT INTO characters (account_name, character_name, level, xp)
                 VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![
                    account,
                    format!("Hero{i}"),
                    if i == 12 { 11 } else { 10 },
                    match i {
                        12 => 0,
                        10 => 1100,
                        _ => i * 100,
                    }
                ],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO accounts (player_name) VALUES ('npcxplayer')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO characters (account_name, character_name, level, xp)
             VALUES ('npcxplayer', 'LooksLikeNpc', 10, 5000)",
            [],
        )
        .unwrap();

        let before = unix_now();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body: serde_json::Value = response.json().await.unwrap();
        for entry in body["entries"].as_array().unwrap() {
            assert_eq!(entry.as_object().unwrap().len(), 3);
        }
        let leaderboard: LevelLeaderboard = serde_json::from_value(body).unwrap();
        assert_eq!(leaderboard.series.len(), 10);
        for (entry, series) in leaderboard.entries.iter().zip(&leaderboard.series) {
            assert_eq!(entry.name, series.name);
            assert_eq!(entry.level, series.samples.last().unwrap().level);
        }
        assert!((before..=unix_now()).contains(&leaderboard.timestamp));
        assert_eq!(
            leaderboard
                .entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            [
                "Hero12",
                "LooksLikeNpc",
                "Hero10",
                "Hero11",
                "Hero9",
                "Hero8",
                "Hero7",
                "Hero6",
                "Hero5",
                "Hero4"
            ]
        );
        assert_eq!(leaderboard.entries[0].level, 11);
        assert!(leaderboard.entries[1..]
            .iter()
            .all(|entry| entry.level == 10));
        assert_eq!(
            leaderboard
                .entries
                .iter()
                .map(|entry| entry.account_first_rank)
                .collect::<Vec<_>>(),
            [1, 2, 1, 1, 5, 5, 5, 8, 8, 8]
        );

        conn.execute(
            "UPDATE characters SET level = 20 WHERE character_name = 'Hero1'",
            [],
        )
        .unwrap();
        let updated: LevelLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(updated.entries[0].name, "Hero1");
        assert_eq!(updated.entries[0].level, 20);
        conn.execute("DELETE FROM characters WHERE character_name = 'Hero1'", [])
            .unwrap();
        let deleted: LevelLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(deleted.entries, leaderboard.entries);
        conn.execute(
            "UPDATE characters SET level = 20 WHERE character_name = 'Hero11'",
            [],
        )
        .unwrap();
        let reordered: LevelLeaderboard =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(reordered.entries[0].name, "Hero11");
        assert_eq!(reordered.entries[1].name, "Hero12");
        assert_eq!(reordered.entries[3].name, "Hero10");
        for index in [0, 1, 3] {
            assert_eq!(reordered.entries[index].account_first_rank, 1);
        }
        conn.execute(
            "DELETE FROM characters WHERE character_name != 'Hero12'",
            [],
        )
        .unwrap();
        let single: LevelLeaderboard = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(
            single.entries,
            [LevelLeaderboardEntry {
                name: "Hero12".into(),
                level: 11,
                account_first_rank: 1
            }]
        );

        conn.execute("DROP TABLE characters", []).unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

    #[tokio::test]
    async fn per_account_gold_endpoint_supports_independent_ranges_and_active_windows() {
        let path = crate::test_util::unique_temp_dir("per_account_gold_endpoint").join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("per_account_gold_endpoint"));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let router = metrics_router(game, Arc::clone(&auth));
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let url = format!("http://{addr}/api/metrics/gold-per-account");
        let empty: PerAccountGoldHistory =
            client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(empty.window_seconds, DAY_SECONDS);
        assert_eq!(empty.latest, None);
        assert!(empty.samples.is_empty());
        let now = unix_now();
        auth.record_gold_snapshot(now, 30).unwrap();
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute("UPDATE gold_snapshots SET total_gold = 100", [])
            .unwrap();
        conn.execute("UPDATE unique_account_collection SET started_at = 0", [])
            .unwrap();
        conn.execute(
            "INSERT INTO unique_account_daily_samples VALUES (?1, 2, 4, 5, 10, 20)",
            [kst_day_start(now)],
        )
        .unwrap();
        for hours in [1, 24, 168, 720, 4320, 8760] {
            for (active_hours, accounts) in [(24, 2), (168, 4), (720, 5), (4320, 10), (8760, 20)] {
                let response = client
                    .get(format!("{url}?hours={hours}&active_hours={active_hours}"))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::OK);
                assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
                let body: PerAccountGoldHistory = response.json().await.unwrap();
                assert_eq!(body.until - body.from, hours * 3600);
                assert_eq!(body.window_seconds, active_hours * 3600);
                assert_eq!(body.latest.unwrap().accounts, accounts);
                assert_eq!(
                    body.samples[0].gold_per_account,
                    100.0 / f64::from(accounts)
                );
            }
        }
        for query in [
            "hours=6",
            "hours=0",
            "hours=invalid",
            "active_hours=0",
            "active_hours=1",
            "active_hours=25",
            "active_hours=-24",
            "active_hours=invalid",
            "active_hours=8761",
        ] {
            assert_eq!(
                client
                    .get(format!("{url}?{query}"))
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
        conn.execute("DROP TABLE unique_account_daily_samples", [])
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        task.abort();
    }

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
            (1, 3600),
            (6, 3600),
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
