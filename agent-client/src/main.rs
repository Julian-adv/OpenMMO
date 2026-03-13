mod claude;
mod codex;
mod driver;
mod mcp;
mod openrouter;
mod state;

use std::sync::Arc;

use std::time::Duration;

use claude::ClaudeConfig;
use codex::CodexConfig;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{
    deserialize_server_msg, serialize_client_msg, ClientMessage, ServerMessage,
};
use onlinerpg_terrain::height::HeightSampler;
use onlinerpg_terrain::io::TerrainIO;
use openrouter::OpenRouterConfig;
use serde::Deserialize;
use state::SharedState;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

/// Which LLM backend to use for the agent driver.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
enum LlmType {
    /// No LLM driver (MCP or direct mode)
    #[default]
    None,
    /// Claude CLI (stdio subprocess)
    Claude,
    /// OpenRouter API (HTTP)
    Openrouter,
    /// Codex CLI (stdio subprocess)
    Codex,
}

#[derive(Deserialize)]
struct Config {
    /// Server WebSocket URL
    server: String,
    /// Account name
    account: String,
    /// Password
    password: String,
    /// Create a new account instead of logging in
    #[serde(default)]
    create_account: bool,
    /// Character ID to enter game with (if omitted, waits for MCP connection)
    character_id: Option<i64>,
    /// MCP HTTP server port (default: 8808)
    #[serde(default = "default_mcp_port")]
    mcp_port: u16,
    /// LLM backend type: "none", "claude", "openrouter", "codex"
    #[serde(default)]
    llm: LlmType,
    /// Minimum interval between LLM prompts in seconds
    #[serde(default = "default_min_interval_secs")]
    min_interval_secs: u64,
    /// Debounce window for batching urgent events in seconds
    #[serde(default = "default_debounce_secs")]
    debounce_secs: u64,
    /// Idle interval (secs): LLM call frequency when no chat/combat activity
    #[serde(default = "default_idle_interval_secs")]
    idle_interval_secs: u64,
    /// Activity window (secs): how long after last chat/combat to stay in active mode
    #[serde(default = "default_activity_window_secs")]
    activity_window_secs: u64,
    /// Claude CLI integration config
    #[serde(default)]
    claude: ClaudeConfig,
    /// OpenRouter API integration config
    #[serde(default)]
    openrouter: OpenRouterConfig,
    /// Codex CLI integration config
    #[serde(default)]
    codex: CodexConfig,
    /// Path to terrain data directory (for heightmap sampling)
    #[serde(default = "default_terrain_dir")]
    terrain_dir: String,
}

fn default_terrain_dir() -> String {
    "../data/terrain".to_string()
}

fn default_mcp_port() -> u16 {
    8808
}

fn default_min_interval_secs() -> u64 {
    5
}

fn default_debounce_secs() -> u64 {
    2
}

fn default_idle_interval_secs() -> u64 {
    3600
}

fn default_activity_window_secs() -> u64 {
    30
}

const CONFIG_PATH: &str = "data/config.toml";

/// FNV-1a 32-bit hash (matches the JS client implementation)
fn fnv1a_hash(input: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in input.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:08x}")
}

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config_text = std::fs::read_to_string(CONFIG_PATH)
        .map_err(|e| anyhow::anyhow!("Failed to read {CONFIG_PATH}: {e}"))?;
    let config: Config = toml::from_str(&config_text)
        .map_err(|e| anyhow::anyhow!("Failed to parse {CONFIG_PATH}: {e}"))?;

    let llm_enabled = config.llm != LlmType::None;

    // MCP mode doesn't reconnect — it runs the HTTP server
    if config.character_id.is_none() && !llm_enabled {
        return run_mcp_mode(&config).await;
    }

    // Reconnect loop for game sessions
    loop {
        match run_session(&config).await {
            Ok(()) => {
                info!(
                    "Session ended cleanly. Reconnecting in {}s...",
                    RECONNECT_DELAY.as_secs()
                );
            }
            Err(e) => {
                warn!(
                    "Session failed: {e}. Reconnecting in {}s...",
                    RECONNECT_DELAY.as_secs()
                );
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// MCP mode: single session with HTTP server (no reconnect).
async fn run_mcp_mode(config: &Config) -> anyhow::Result<()> {
    let password_hash = fnv1a_hash(&config.password);

    let ws_stream = connect_ws(&config.server).await;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    send(
        &mut ws_tx,
        &ClientMessage::Authenticate {
            account_name: config.account.clone(),
            password_hash,
            create_account: config.create_account,
        },
    )
    .await?;

    let characters = wait_for_auth(&mut ws_rx).await?;

    let (cmd_tx, _cmd_rx) = mpsc::channel::<ClientMessage>(32);
    let height_sampler = create_height_sampler(&config.terrain_dir);
    let state = Arc::new(Mutex::new(SharedState::new(
        characters,
        cmd_tx,
        height_sampler,
    )));

    info!(
        "No character_id configured — starting MCP HTTP server on port {}...",
        config.mcp_port
    );
    mcp::run_mcp_server(state, config.mcp_port).await
}

/// Run a single game session: connect, authenticate, enter game, run until disconnected.
async fn run_session(config: &Config) -> anyhow::Result<()> {
    let password_hash = fnv1a_hash(&config.password);

    // Connect with retry
    let ws_stream = connect_ws(&config.server).await;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // Authenticate
    send(
        &mut ws_tx,
        &ClientMessage::Authenticate {
            account_name: config.account.clone(),
            password_hash,
            create_account: config.create_account,
        },
    )
    .await?;

    let characters = wait_for_auth(&mut ws_rx).await?;

    // Determine which character to enter game with
    let llm_enabled = config.llm != LlmType::None;
    let enter_char_id = if let Some(char_id) = config.character_id {
        Some(char_id)
    } else if llm_enabled {
        characters.first().map(|c| c.id)
    } else {
        None
    };

    if let Some(char_id) = enter_char_id {
        send(
            &mut ws_tx,
            &ClientMessage::EnterGame {
                character_id: char_id,
            },
        )
        .await?;
        info!("Entering game with character {char_id}...");
    }

    // Set up shared state and command channel
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientMessage>(32);
    let height_sampler = create_height_sampler(&config.terrain_dir);
    let state = Arc::new(Mutex::new(SharedState::new(
        characters,
        cmd_tx,
        height_sampler,
    )));

    // Background task: forward commands from channel to WebSocket
    let tx_task = tokio::spawn(async move {
        while let Some(msg) = cmd_rx.recv().await {
            if let Err(e) = send(&mut ws_tx, &msg).await {
                error!("Failed to send command: {e}");
                break;
            }
        }
    });

    // Background task: read WebSocket messages into shared state
    let state_for_rx = Arc::clone(&state);
    let rx_task = tokio::spawn(async move {
        loop {
            match recv(&mut ws_rx).await {
                Ok(msg) => {
                    if matches!(msg, ServerMessage::GameTimeSync { .. }) {
                        let mut s = state_for_rx.lock().await;
                        let _ = s.send_command(ClientMessage::Heartbeat).await;
                        s.push_event(msg);
                        continue;
                    }

                    let needs_height_sync = matches!(
                        msg,
                        ServerMessage::JoinSuccess { .. } | ServerMessage::PlayerRespawned { .. }
                    );

                    let mut s = state_for_rx.lock().await;
                    s.push_event(msg);

                    if needs_height_sync {
                        if let Err(e) = s.sync_height().await {
                            warn!("Failed to sync height after spawn: {e}");
                        }
                    }
                }
                Err(e) => {
                    error!("Connection lost: {e}");
                    break;
                }
            }
        }
    });

    // Start LLM driver based on configured backend
    let llm_task = match config.llm {
        LlmType::Claude => {
            info!(
                "Claude CLI integration enabled (model={})",
                config.claude.model
            );
            let state_for_llm = Arc::clone(&state);
            let min_interval = Duration::from_secs(config.min_interval_secs);
            let debounce = Duration::from_secs(config.debounce_secs);
            let idle_interval = Duration::from_secs(config.idle_interval_secs);
            let activity_window = Duration::from_secs(config.activity_window_secs);
            match claude::ClaudeInvoker::new(&config.claude) {
                Ok(invoker) => Some(tokio::spawn(async move {
                    driver::llm_driver(
                        state_for_llm,
                        Arc::new(invoker),
                        min_interval,
                        debounce,
                        idle_interval,
                        activity_window,
                    )
                    .await;
                })),
                Err(e) => {
                    error!("Failed to create Claude invoker: {e}");
                    None
                }
            }
        }
        LlmType::Openrouter => {
            info!(
                "OpenRouter API integration enabled (model={})",
                config.openrouter.model
            );
            let state_for_llm = Arc::clone(&state);
            let min_interval = Duration::from_secs(config.min_interval_secs);
            let debounce = Duration::from_secs(config.debounce_secs);
            let idle_interval = Duration::from_secs(config.idle_interval_secs);
            let activity_window = Duration::from_secs(config.activity_window_secs);
            match openrouter::OpenRouterInvoker::new(&config.openrouter) {
                Ok(invoker) => Some(tokio::spawn(async move {
                    driver::llm_driver(
                        state_for_llm,
                        Arc::new(invoker),
                        min_interval,
                        debounce,
                        idle_interval,
                        activity_window,
                    )
                    .await;
                })),
                Err(e) => {
                    error!("Failed to create OpenRouter invoker: {e}");
                    None
                }
            }
        }
        LlmType::Codex => {
            info!(
                "Codex CLI integration enabled (model={})",
                config.codex.model
            );
            let state_for_llm = Arc::clone(&state);
            let min_interval = Duration::from_secs(config.min_interval_secs);
            let debounce = Duration::from_secs(config.debounce_secs);
            let idle_interval = Duration::from_secs(config.idle_interval_secs);
            let activity_window = Duration::from_secs(config.activity_window_secs);
            match codex::CodexInvoker::new(&config.codex) {
                Ok(invoker) => Some(tokio::spawn(async move {
                    driver::llm_driver(
                        state_for_llm,
                        Arc::new(invoker),
                        min_interval,
                        debounce,
                        idle_interval,
                        activity_window,
                    )
                    .await;
                })),
                Err(e) => {
                    error!("Failed to create Codex invoker: {e}");
                    None
                }
            }
        }
        LlmType::None => None,
    };

    if llm_enabled {
        info!("Running in LLM-driven mode");
    } else {
        info!("Running in direct mode (character_id set in config)");
    }

    // Wait until the WebSocket reader dies (connection lost)
    let _ = rx_task.await;

    // Clean up
    tx_task.abort();
    if let Some(t) = llm_task {
        t.abort();
    }

    Ok(())
}

fn create_height_sampler(terrain_dir: &str) -> HeightSampler {
    HeightSampler::new(TerrainIO::new(std::path::PathBuf::from(terrain_dir)))
}

/// Connect to WebSocket with retry loop.
async fn connect_ws(
    url: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    loop {
        info!("Connecting to {url}");
        match tokio_tungstenite::connect_async(url).await {
            Ok((stream, _)) => {
                info!("Connected");
                return stream;
            }
            Err(e) => {
                warn!("Connection failed: {e} — retrying in 3s...");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

/// Wait for AuthSuccess, returning the character list.
async fn wait_for_auth(ws_rx: &mut WsRx) -> anyhow::Result<Vec<onlinerpg_shared::Character>> {
    loop {
        match recv(ws_rx).await? {
            ServerMessage::AuthSuccess { characters, .. } => {
                info!("Authenticated. {} character(s):", characters.len());
                for c in &characters {
                    info!("  [{}] {} (Lv.{} {:?})", c.id, c.name, c.level, c.class);
                }
                return Ok(characters);
            }
            ServerMessage::AuthError { message } => {
                anyhow::bail!("Auth failed: {message}");
            }
            other => {
                warn!("Unexpected message during auth: {:?}", msg_name(&other));
            }
        }
    }
}

type WsTx = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

type WsRx = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

async fn send(tx: &mut WsTx, msg: &ClientMessage) -> anyhow::Result<()> {
    let bytes = serialize_client_msg(msg)?;
    tx.send(Message::Binary(bytes.into())).await?;
    Ok(())
}

async fn recv(rx: &mut WsRx) -> anyhow::Result<ServerMessage> {
    loop {
        match rx.next().await {
            Some(Ok(Message::Binary(bytes))) => {
                return Ok(deserialize_server_msg(&bytes)?);
            }
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
            Some(Ok(Message::Close(_))) => anyhow::bail!("Server closed connection"),
            Some(Ok(other)) => {
                warn!("Unexpected WS frame: {other:?}");
                continue;
            }
            Some(Err(e)) => anyhow::bail!("WebSocket error: {e}"),
            None => anyhow::bail!("WebSocket stream ended"),
        }
    }
}

fn msg_name(msg: &ServerMessage) -> &'static str {
    match msg {
        ServerMessage::AuthSuccess { .. } => "AuthSuccess",
        ServerMessage::AuthError { .. } => "AuthError",
        ServerMessage::JoinSuccess { .. } => "JoinSuccess",
        ServerMessage::CharacterCreated { .. } => "CharacterCreated",
        ServerMessage::CharacterStatsRolled { .. } => "CharacterStatsRolled",
        ServerMessage::CharacterDeleted { .. } => "CharacterDeleted",
        ServerMessage::CharacterError { .. } => "CharacterError",
        ServerMessage::PlayerJoined { .. } => "PlayerJoined",
        ServerMessage::PlayerLeft { .. } => "PlayerLeft",
        ServerMessage::PlayerMoved { .. } => "PlayerMoved",
        ServerMessage::PlayerTeleported { .. } => "PlayerTeleported",
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::GameState { .. } => "GameState",
        ServerMessage::GameTimeSync { .. } => "GameTimeSync",
        ServerMessage::MonsterSpawned { .. } => "MonsterSpawned",
        ServerMessage::MonsterMoved { .. } => "MonsterMoved",
        ServerMessage::MonsterRemoved { .. } => "MonsterRemoved",
        ServerMessage::MonsterDead { .. } => "MonsterDead",
        ServerMessage::PlayerAttacked { .. } => "PlayerAttacked",
        ServerMessage::MonsterAttackedPlayer { .. } => "MonsterAttackedPlayer",
        ServerMessage::PlayerDead { .. } => "PlayerDead",
        ServerMessage::PlayerRespawned { .. } => "PlayerRespawned",
        ServerMessage::PlayerHealthUpdate { .. } => "PlayerHealthUpdate",
        ServerMessage::XpGained { .. } => "XpGained",
        ServerMessage::Kicked { .. } => "Kicked",
        ServerMessage::PlayerTorchToggled { .. } => "PlayerTorchToggled",
    }
}
