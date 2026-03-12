mod claude;
mod mcp;
mod state;

use std::sync::Arc;

use claude::ClaudeConfig;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{
    deserialize_server_msg, serialize_client_msg, ClientMessage, ServerMessage,
};
use state::SharedState;
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
    /// Claude CLI integration config
    #[serde(default)]
    claude: ClaudeConfig,
}

fn default_mcp_port() -> u16 {
    8808
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

    // Determine which character to enter game with
    let enter_char_id = if let Some(char_id) = config.character_id {
        Some(char_id)
    } else if config.claude.enabled {
        // Claude mode: auto-select first character
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
                    // Respond to time sync with heartbeat
                    if matches!(msg, ServerMessage::GameTimeSync { .. }) {
                        let mut s = state_for_rx.lock().await;
                        let _ = s.send_command(ClientMessage::Heartbeat).await;
                        s.push_event(msg);
                        continue;
                    }

                    let mut s = state_for_rx.lock().await;
                    s.push_event(msg);
                }
                Err(e) => {
                    error!("Connection lost: {e}");
                    break;
                }
            }
        }
    });

    // Start Claude driver if enabled
    let claude_task = if config.claude.enabled {
        info!("Claude CLI integration enabled (model={})", config.claude.model);
        let state_for_claude = Arc::clone(&state);
        let claude_config = config.claude.clone();
        Some(tokio::spawn(async move {
            claude::claude_driver(state_for_claude, claude_config).await;
        }))
    } else {
        None
    };

    if enter_char_id.is_some() {
        if config.claude.enabled {
            // Claude mode: run until WS reader finishes
            info!("Running in Claude-driven mode");
            let _ = rx_task.await;
        } else {
            // Direct mode: just wait for the WS reader to finish
            info!("Running in direct mode (character_id set in config)");
            let _ = rx_task.await;
        }
    } else {
        // MCP mode: start HTTP MCP server and wait for LLM to drive the session
        info!("No character_id configured — starting MCP HTTP server on port {}...", config.mcp_port);
        mcp::run_mcp_server(state, config.mcp_port).await?;
    }

    tx_task.abort();
    if let Some(ct) = claude_task {
        ct.abort();
    }
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
