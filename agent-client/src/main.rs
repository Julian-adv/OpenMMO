use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{
    deserialize_server_msg, serialize_client_msg, ClientMessage, ServerMessage,
};
use serde::Deserialize;
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
    /// Character ID to enter game with (if omitted, lists characters and exits)
    character_id: Option<i64>,
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
    let (mut tx, mut rx) = ws_stream.split();
    info!("Connected");

    // Authenticate
    let auth_msg = ClientMessage::Authenticate {
        account_name: config.account.clone(),
        password_hash,
        create_account: config.create_account,
    };
    send(&mut tx, &auth_msg).await?;

    // Wait for auth response
    let characters = loop {
        match recv(&mut rx).await? {
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

    // Enter game
    let char_id = match config.character_id {
        Some(id) => id,
        None => {
            if characters.is_empty() {
                info!("No characters. Create one in the game client first.");
            } else {
                info!("Set character_id in {CONFIG_PATH} to enter the game.");
            }
            return Ok(());
        }
    };

    let enter_msg = ClientMessage::EnterGame {
        character_id: char_id,
    };
    send(&mut tx, &enter_msg).await?;

    // Main message loop
    info!("Entering game with character {char_id}...");
    loop {
        match recv(&mut rx).await {
            Ok(msg) => handle_message(&msg),
            Err(e) => {
                error!("Connection lost: {e}");
                break;
            }
        }
    }

    Ok(())
}

fn handle_message(msg: &ServerMessage) {
    match msg {
        ServerMessage::JoinSuccess { player } => {
            info!(
                "Joined as {} at ({:.1}, {:.1}, {:.1})",
                player.name, player.position.x, player.position.y, player.position.z
            );
        }
        ServerMessage::GameState {
            players, monsters, ..
        } => {
            info!(
                "World state: {} player(s), {} monster(s)",
                players.len(),
                monsters.len()
            );
            for p in players.values() {
                info!(
                    "  Player: {} (Lv.{} HP {}/{})",
                    p.name, p.level, p.health, p.max_health
                );
            }
            for m in monsters.values() {
                info!(
                    "  Monster: {} [{}] HP {}/{}",
                    m.monster_type, m.state, m.health, m.max_health
                );
            }
        }
        ServerMessage::GameTimeSync { datetime, is_night } => {
            info!(
                "Time: Year {} Month {} Day {} {:02}:{:02} ({})",
                datetime.year,
                datetime.month,
                datetime.day,
                datetime.hour,
                datetime.minute,
                if *is_night { "night" } else { "day" }
            );
        }
        ServerMessage::ChatMessage {
            player_id, message, ..
        } => {
            info!("[Chat] {player_id}: {message}");
        }
        ServerMessage::PlayerJoined { player } => {
            info!("Player joined: {}", player.name);
        }
        ServerMessage::PlayerLeft { player_id } => {
            info!("Player left: {player_id}");
        }
        ServerMessage::PlayerMoved {
            player_id,
            position,
            ..
        } => {
            tracing::debug!(
                "Player {player_id} moved to ({:.1}, {:.1}, {:.1})",
                position.x,
                position.y,
                position.z
            );
        }
        ServerMessage::MonsterSpawned { monster } => {
            info!("Monster spawned: {} ({})", monster.id, monster.monster_type);
        }
        ServerMessage::MonsterDead { monster_id } => {
            info!("Monster died: {monster_id}");
        }
        ServerMessage::PlayerAttacked {
            player_id,
            monster_id,
            hit,
            roll,
            damage,
        } => {
            info!("Player {player_id} attacks {monster_id}: roll={roll} hit={hit} dmg={damage}");
        }
        ServerMessage::MonsterAttackedPlayer {
            monster_id,
            player_id,
            hit,
            damage,
            current_health,
            ..
        } => {
            info!(
                "Monster {monster_id} attacks {player_id}: hit={hit} dmg={damage} hp={current_health}"
            );
        }
        ServerMessage::PlayerDead { player_id } => {
            warn!("Player died: {player_id}");
        }
        ServerMessage::PlayerRespawned { player } => {
            info!(
                "Player respawned: {} HP {}/{}",
                player.name, player.health, player.max_health
            );
        }
        ServerMessage::XpGained {
            xp_amount,
            total_xp,
            new_level,
            leveled_up,
            ..
        } => {
            info!("XP +{xp_amount} (total: {total_xp}, level: {new_level})");
            if *leveled_up {
                info!("LEVEL UP! Now level {new_level}");
            }
        }
        ServerMessage::Kicked { reason, .. } => {
            warn!("Kicked: {reason}");
        }
        _ => {
            tracing::debug!("Received: {:?}", msg_name(msg));
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
