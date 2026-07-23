//! Local spectator panel: a small HTTP server that exposes each NPC's live
//! game state (map, entities, chat/combat feed, LLM turns) to a browser page.
//! Read-only and bound to 127.0.0.1 — it observes the agent, never drives it.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use axum::extract::{Query, State};
use axum::response::{Html, IntoResponse, Json};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::state::SharedState;
use onlinerpg_terrain::height::HeightSampler;

const FEED_CAP: usize = 300;
const FEED_SNAPSHOT: usize = 200;
const PROMPT_TEXT_CAP: usize = 6000;

#[derive(Clone, serde::Serialize)]
pub struct FeedItem {
    /// Unix ms.
    pub t: u64,
    /// Kind: chat | combat | trade | system | agent | llm-prompt | llm-response | llm-error.
    pub k: &'static str,
    pub m: String,
    /// Duration ms (LLM responses only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub d: Option<u64>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Per-NPC watch handle. Outlives sessions, so the feed survives reconnects.
pub struct NpcWatch {
    feed: StdMutex<VecDeque<FeedItem>>,
    state: StdMutex<Option<Arc<Mutex<SharedState>>>>,
    connected: AtomicBool,
}

impl NpcWatch {
    fn new() -> Self {
        Self {
            feed: StdMutex::new(VecDeque::new()),
            state: StdMutex::new(None),
            connected: AtomicBool::new(false),
        }
    }

    pub fn push(&self, kind: &'static str, text: String) {
        self.push_timed(kind, text, None)
    }

    pub fn push_timed(&self, kind: &'static str, text: String, duration_ms: Option<u64>) {
        let mut feed = self.feed.lock().unwrap();
        feed.push_back(FeedItem {
            t: now_ms(),
            k: kind,
            m: text,
            d: duration_ms,
        });
        while feed.len() > FEED_CAP {
            feed.pop_front();
        }
    }

    pub fn set_state(&self, state: Arc<Mutex<SharedState>>) {
        *self.state.lock().unwrap() = Some(state);
        self.connected.store(true, Ordering::Relaxed);
    }

    pub fn set_disconnected(&self) {
        self.connected.store(false, Ordering::Relaxed);
    }

    fn current_state(&self) -> Option<Arc<Mutex<SharedState>>> {
        self.state.lock().unwrap().clone()
    }

    fn feed_snapshot(&self) -> Vec<FeedItem> {
        let feed = self.feed.lock().unwrap();
        feed.iter()
            .skip(feed.len().saturating_sub(FEED_SNAPSHOT))
            .cloned()
            .collect()
    }
}

/// All NPC watch handles, keyed by orchestrator label. Fixed at startup.
pub struct WatchHub {
    npcs: Vec<(String, Arc<NpcWatch>)>,
}

impl WatchHub {
    pub fn new(labels: Vec<String>) -> Self {
        Self {
            npcs: labels
                .into_iter()
                .map(|l| (l, Arc::new(NpcWatch::new())))
                .collect(),
        }
    }

    pub fn handle(&self, label: &str) -> Option<Arc<NpcWatch>> {
        self.npcs
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, w)| Arc::clone(w))
    }

    /// Record an LLM turn start; returns the handle for the response push.
    pub fn llm_prompt(&self, label: &str, prompt: &str) -> Option<Arc<NpcWatch>> {
        let w = self.handle(label)?;
        let mut text = prompt.chars().take(PROMPT_TEXT_CAP).collect::<String>();
        if prompt.len() > text.len() {
            text.push_str("\n… (truncated)");
        }
        w.push("llm-prompt", text);
        Some(w)
    }
}

struct AppState {
    hub: Arc<WatchHub>,
    height: Arc<HeightSampler>,
}

#[derive(Deserialize)]
struct NpcQuery {
    npc: Option<String>,
}

#[derive(Deserialize)]
struct HeightQuery {
    cx: f32,
    cz: f32,
    #[serde(default = "default_half")]
    half: f32,
    #[serde(default = "default_res")]
    res: usize,
}

fn default_half() -> f32 {
    100.0
}

fn default_res() -> usize {
    96
}

pub async fn serve(hub: Arc<WatchHub>, height: Arc<HeightSampler>, port: u16) {
    let app_state = Arc::new(AppState { hub, height });
    let app = Router::new()
        .route("/", get(page))
        .route("/api/npcs", get(npcs))
        .route("/api/state", get(state_snapshot))
        .route("/api/height", get(height_grid))
        .with_state(app_state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!("Watch panel failed to bind {addr}: {e}");
            return;
        }
    };
    info!("Watch panel: http://{addr}/");
    if let Err(e) = axum::serve(listener, app).await {
        warn!("Watch panel server stopped: {e}");
    }
}

async fn page() -> Html<&'static str> {
    Html(include_str!("watch.html"))
}

async fn npcs(State(app): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let labels: Vec<&str> = app.hub.npcs.iter().map(|(l, _)| l.as_str()).collect();
    Json(json!({ "npcs": labels }))
}

async fn state_snapshot(
    State(app): State<Arc<AppState>>,
    Query(q): Query<NpcQuery>,
) -> impl IntoResponse {
    let entry = match &q.npc {
        Some(label) => app.hub.npcs.iter().find(|(l, _)| l == label),
        None => app.hub.npcs.first(),
    };
    let Some((label, watch)) = entry else {
        return Json(json!({ "error": "unknown npc" }));
    };

    let feed = watch.feed_snapshot();
    let connected = watch.connected.load(Ordering::Relaxed);

    let Some(state_arc) = watch.current_state() else {
        return Json(json!({
            "npc": label, "connected": false, "feed": feed,
        }));
    };

    let s = state_arc.lock().await;
    let houses: Vec<serde_json::Value> = {
        let wc = s.world_cache.read().unwrap();
        wc.houses()
            .values()
            .map(|h| {
                let rooms: Vec<serde_json::Value> = h
                    .rooms
                    .iter()
                    .map(|r| {
                        json!({
                            "x": h.origin.x + r.local_x as f32,
                            "z": h.origin.z + r.local_z as f32,
                            "w": r.size_x, "d": r.size_z, "floor": r.floor_level,
                        })
                    })
                    .collect();
                json!({ "id": h.id, "rooms": rooms })
            })
            .collect()
    };

    Json(json!({
        "npc": label,
        "connected": connected && s.in_game,
        "self": s.self_player,
        "gold": s.self_gold,
        "floor": s.self_floor_level,
        "bag": s.self_bag,
        "time": { "hour": s.game_hour, "minute": s.game_minute, "night": s.is_night },
        "players": s.nearby_players.values().collect::<Vec<_>>(),
        "monsters": s.nearby_monsters.values().collect::<Vec<_>>(),
        "houses": houses,
        "feed": feed,
    }))
}

async fn height_grid(
    State(app): State<Arc<AppState>>,
    Query(q): Query<HeightQuery>,
) -> Json<serde_json::Value> {
    let res = q.res.clamp(8, 160);
    let half = q.half.clamp(10.0, 600.0);
    let step = (half * 2.0) / res as f32;
    let x0 = q.cx - half;
    let z0 = q.cz - half;

    let mut heights = Vec::with_capacity(res * res);
    for iz in 0..res {
        for ix in 0..res {
            let x = x0 + (ix as f32 + 0.5) * step;
            let z = z0 + (iz as f32 + 0.5) * step;
            let h = app.height.sample_height(x, z).await.unwrap_or(0.0);
            heights.push((h * 100.0).round() / 100.0);
        }
    }

    Json(json!({
        "x0": x0, "z0": z0, "step": step, "res": res, "heights": heights,
    }))
}

/// Feed category for a server message; `None` keeps it off the panel
/// (movement/time spam is visible on the map already).
pub fn feed_kind(msg: &onlinerpg_shared::ServerMessage) -> Option<&'static str> {
    use onlinerpg_shared::ServerMessage as M;
    Some(match msg {
        M::ChatMessage { .. } => "chat",
        M::PlayerAttacked { .. }
        | M::MonsterAttackedPlayer { .. }
        | M::MonsterDead { .. }
        | M::PlayerDead { .. }
        | M::PlayerRespawned { .. }
        | M::XpGained { .. } => "combat",
        M::DealResult { .. } | M::TradeNotice { .. } | M::TradeError { .. } => "trade",
        M::JoinSuccess { .. }
        | M::Kicked { .. }
        | M::ServerNotice { .. }
        | M::PlayerJoined { .. }
        | M::PlayerLeft { .. }
        | M::PlayerAppeared { .. }
        | M::PlayerDisappeared { .. }
        | M::CharacterCreated { .. }
        | M::CharacterError { .. } => "system",
        _ => return None,
    })
}

/// Message text for feed kinds `format_event` does not cover.
pub fn feed_fallback(msg: &onlinerpg_shared::ServerMessage) -> Option<String> {
    use onlinerpg_shared::ServerMessage as M;
    match msg {
        M::ServerNotice { message } => Some(format!(
            "[Notice] {}",
            message.as_deref().unwrap_or("(cleared)")
        )),
        _ => None,
    }
}
