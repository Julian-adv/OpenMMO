mod mcp;

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{
    deserialize_server_msg, serialize_client_msg, Character, ClientMessage, ServerMessage,
};
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

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
}

fn default_mcp_port() -> u16 {
    8808
}

const CONFIG_PATH: &str = "data/config.toml";

/// Shared state between MCP server and WebSocket background tasks.
pub struct SharedState {
    pub characters: Vec<Character>,
    pub in_game: bool,
    events: Vec<ServerMessage>,
    cmd_tx: mpsc::Sender<ClientMessage>,
}

impl SharedState {
    fn new(characters: Vec<Character>, cmd_tx: mpsc::Sender<ClientMessage>) -> Self {
        Self {
            characters,
            in_game: false,
            events: Vec::new(),
            cmd_tx,
        }
    }

    pub async fn send_command(&mut self, msg: ClientMessage) -> anyhow::Result<()> {
        self.cmd_tx
            .send(msg)
            .await
            .map_err(|e| anyhow::anyhow!("Command channel closed: {e}"))
    }

    pub fn push_event(&mut self, msg: ServerMessage) {
        if matches!(msg, ServerMessage::JoinSuccess { .. }) {
            self.in_game = true;
        }
        self.events.push(msg);
    }

    pub fn drain_events(&mut self) -> Vec<ServerMessage> {
        std::mem::take(&mut self.events)
    }
}

/// FNV-1a 32-bit hash (matches the JS client implementation)
fn fnv1a_hash(input: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in input.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:08x}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config_text = std::fs::read_to_string(CONFIG_PATH)
        .map_err(|e| anyhow::anyhow!("Failed to read {CONFIG_PATH}: {e}"))?;
    let config: Config = toml::from_str(&config_text)
        .map_err(|e| anyhow::anyhow!("Failed to parse {CONFIG_PATH}: {e}"))?;

    let password_hash = fnv1a_hash(&config.password);

    // Connect with retry (server may be restarting)
    let ws_stream = loop {
        info!("Connecting to {}", config.server);
        match tokio_tungstenite::connect_async(&config.server).await {
            Ok((stream, _)) => break stream,
            Err(e) => {
                warn!("Connection failed: {e} — retrying in 3s...");
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    };
    let (ws_tx, mut ws_rx) = ws_stream.split();
    info!("Connected");

    // Authenticate
    let auth_msg = ClientMessage::Authenticate {
        account_name: config.account.clone(),
        password_hash,
        create_account: config.create_account,
    };
    let mut ws_tx = ws_tx;
    send(&mut ws_tx, &auth_msg).await?;

    // Wait for auth response
    let characters = loop {
        match recv(&mut ws_rx).await? {
            ServerMessage::AuthSuccess { characters, .. } => {
                info!("Authenticated. {} character(s):", characters.len());
                for c in &characters {
                    info!("  [{}] {} (Lv.{} {:?})", c.id, c.name, c.level, c.class);
                }
                break characters;
            }
            ServerMessage::AuthError { message } => {
                error!("Auth failed: {message}");
                return Ok(());
            }
            other => {
                warn!("Unexpected message during auth: {:?}", msg_name(&other));
            }
        }
    };

    // Set up shared state and command channel
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientMessage>(32);
    let state = Arc::new(Mutex::new(SharedState::new(characters.clone(), cmd_tx)));

    // If character_id is set in config, enter game directly
    if let Some(char_id) = config.character_id {
        send(
            &mut ws_tx,
            &ClientMessage::EnterGame {
                character_id: char_id,
            },
        )
        .await?;
        info!("Entering game with character {char_id}...");
    }

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
                    let mut s = state_for_rx.lock().await;
                    if let ServerMessage::CharacterCreated { ref character } = msg {
                        s.characters.push(character.clone());
                    }
                    if let ServerMessage::JoinSuccess { .. } = msg {
                        s.in_game = true;
                    }
                    s.push_event(msg);
                }
                Err(e) => {
                    error!("Connection lost: {e}");
                    break;
                }
            }
        }
    });

    if config.character_id.is_some() {
        // Direct mode: just wait for the WS reader to finish
        info!("Running in direct mode (character_id set in config)");
        let _ = rx_task.await;
    } else {
        // MCP mode: start HTTP MCP server and wait for LLM to drive the session
        info!("No character_id configured — starting MCP HTTP server on port {}...", config.mcp_port);
        mcp::run_mcp_server(state, config.mcp_port).await?;
    }

    tx_task.abort();
    Ok(())
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
