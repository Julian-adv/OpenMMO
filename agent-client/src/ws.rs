//! WebSocket helpers shared between main (MCP mode) and orchestrator.

use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{
    deserialize_server_msg, serialize_client_msg, ClientMessage, ServerMessage,
};
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::msg_name;

pub type WsTx = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;

pub type WsRx = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

/// Connect to WebSocket with retry loop. `label` is used for log context.
pub async fn connect_ws(
    url: &str,
    label: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    loop {
        info!("[{label}] Connecting to {url}");
        match tokio_tungstenite::connect_async(url).await {
            Ok((stream, _)) => {
                info!("[{label}] Connected");
                return stream;
            }
            Err(e) => {
                warn!("[{label}] Connection failed: {e} -- retrying in 3s...");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

/// Server refused the connection (bad protocol version, bad token, unusable
/// account). Retrying cannot fix any of those, so the session loop gives up
/// instead of reconnecting forever.
#[derive(Debug)]
pub struct AuthRejected(pub String);

impl std::fmt::Display for AuthRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AuthRejected {}

/// Announce the protocol version and which client this is. Must be the first
/// message on the connection; the server refuses everything else until it
/// arrives (see `doc/REMOTE_AGENT_CLIENT.md`).
pub async fn send_client_info(tx: &mut WsTx) -> anyhow::Result<()> {
    send(
        tx,
        &ClientMessage::ClientInfo {
            protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
            client_kind: "cli".to_string(),
            client_version: env!("CARGO_PKG_VERSION").to_string(),
        },
    )
    .await
}

/// Wait for AuthSuccess, returning the character list.
pub async fn wait_for_auth(
    ws_rx: &mut WsRx,
    label: &str,
) -> anyhow::Result<Vec<onlinerpg_shared::Character>> {
    loop {
        match recv(ws_rx).await? {
            ServerMessage::AuthSuccess { characters, .. } => {
                info!(
                    "[{label}] Authenticated. {} character(s):",
                    characters.len()
                );
                for c in &characters {
                    info!(
                        "  [{label}] [{}] {} (Lv.{} {:?})",
                        c.id, c.name, c.level, c.class
                    );
                }
                return Ok(characters);
            }
            ServerMessage::AuthError { message } => {
                return Err(AuthRejected(format!("Auth failed: {message}")).into());
            }
            other => {
                warn!(
                    "[{label}] Unexpected message during auth: {:?}",
                    msg_name(&other)
                );
            }
        }
    }
}

/// Wait for a specific server message, skipping irrelevant ones.
pub async fn wait_for_msg(
    ws_rx: &mut WsRx,
    label: &str,
    expected: &str,
    matches: impl Fn(&ServerMessage) -> bool,
) -> anyhow::Result<ServerMessage> {
    loop {
        let msg = recv(ws_rx).await?;
        if matches(&msg) {
            return Ok(msg);
        }
        warn!(
            "[{label}] Waiting for {expected}, got {:?} — skipping",
            crate::msg_name(&msg)
        );
    }
}

pub async fn send(tx: &mut WsTx, msg: &ClientMessage) -> anyhow::Result<()> {
    let bytes = serialize_client_msg(msg)?;
    tx.send(Message::Binary(bytes.into())).await?;
    Ok(())
}

pub async fn recv(rx: &mut WsRx) -> anyhow::Result<ServerMessage> {
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
