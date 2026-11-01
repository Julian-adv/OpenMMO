//! WebSocket helpers: connect, authenticate, and frame the game protocol.

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

const RETRY_BASE_DELAY: Duration = Duration::from_secs(3);
const RETRY_MAX_DELAY: Duration = Duration::from_secs(60);

/// Exponential backoff with full jitter, so a fleet of agent-clients riding
/// out a server restart doesn't come back as one synchronized wave. Shared by
/// both retry loops: the inner connect loop here and `run_npc_loop`'s outer
/// session loop, which retries for the same reason one layer up.
pub fn retry_delay(attempt: u32) -> Duration {
    let capped = RETRY_MAX_DELAY.min(RETRY_BASE_DELAY * (1 << attempt.min(5)));
    capped / 2 + capped.mul_f32(rand::random::<f32>() / 2.0)
}

/// Connect to WebSocket with retry loop. `label` is used for log context.
pub async fn connect_ws(
    url: &str,
    label: &str,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let mut attempt = 0u32;
    loop {
        info!("[{label}] Connecting to {url}");
        match tokio_tungstenite::connect_async(url).await {
            Ok((stream, _)) => {
                info!("[{label}] Connected");
                return stream;
            }
            Err(e) => {
                let delay = retry_delay(attempt);
                attempt = attempt.saturating_add(1);
                warn!(
                    "[{label}] Connection failed: {e} -- retrying in {:.1}s...",
                    delay.as_secs_f32()
                );
                tokio::time::sleep(delay).await;
            }
        }
    }
}

/// The server refused something only a config change can fix (token,
/// account, character name). The session loop gives up instead of
/// reconnecting forever.
#[derive(Debug)]
pub struct AuthRejected(pub String);

impl std::fmt::Display for AuthRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AuthRejected {}

/// The server refused this build itself (protocol, dungeon layout, desync):
/// only a new binary fixes it.
#[derive(Debug)]
pub struct BuildRefused(pub String);

impl std::fmt::Display for BuildRefused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for BuildRefused {}

/// How long to wait after an AuthError for the close that says why.
const REFUSAL_CLOSE_GRACE: Duration = Duration::from_secs(3);

fn is_build_refusal(close_code: u16) -> bool {
    matches!(
        close_code,
        onlinerpg_shared::CLOSE_CODE_PROTOCOL_MISMATCH | onlinerpg_shared::CLOSE_CODE_CLIENT_DESYNC
    )
}

/// A build refusal's AuthError is followed by the protocol-mismatch close;
/// an account or token refusal's is not.
async fn closes_as_build_refusal(rx: &mut WsRx) -> bool {
    let wait = async {
        while let Some(Ok(frame)) = rx.next().await {
            if let Message::Close(Some(f)) = frame {
                return is_build_refusal(u16::from(f.code));
            }
        }
        false
    };
    tokio::time::timeout(REFUSAL_CLOSE_GRACE, wait)
        .await
        .unwrap_or(false)
}

/// Announce the protocol version and which client this is. Must be the first
/// message on the connection; the server refuses everything else until it
/// arrives (see `doc/REMOTE_AGENT_CLIENT.md`).
pub async fn send_client_info(tx: &mut WsTx) -> anyhow::Result<()> {
    send(
        tx,
        &ClientMessage::ClientInfo {
            protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
            client_kind: "cli".to_string(),
            client_version: onlinerpg_shared::stamp_layout_version(env!("CARGO_PKG_VERSION")),
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
                let reason = format!("Auth failed: {message}");
                return Err(if closes_as_build_refusal(ws_rx).await {
                    BuildRefused(reason).into()
                } else {
                    AuthRejected(reason).into()
                });
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

/// Every connection hears from the server well inside this window: GameTimeSync
/// broadcasts every 8s once authenticated, a keepalive ping every 10s before
/// then. A socket silent past it is a half-open corpse (server restarted
/// without a RST, NAT dropped the path): reads would otherwise block on it
/// forever. Detection is read-side only — no client pings, so a fleet of idle
/// agents adds zero server traffic — and the session loop's jittered backoff
/// already spreads out the reconnects.
const READ_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// In-game uptime below which a session's end must not reset the retry
/// backoff (enforced in `run_npc_loop`). Kept above `READ_IDLE_TIMEOUT`:
/// a half-open death is only noticed after that long, and the detection
/// lag alone must not count as a healthy session.
pub const HEALTHY_SESSION: Duration = Duration::from_secs(READ_IDLE_TIMEOUT.as_secs() + 30);

pub async fn recv(rx: &mut WsRx) -> anyhow::Result<ServerMessage> {
    loop {
        let frame = tokio::time::timeout(READ_IDLE_TIMEOUT, rx.next())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "No server traffic for {}s — connection presumed dead",
                    READ_IDLE_TIMEOUT.as_secs()
                )
            })?;
        match frame {
            Some(Ok(Message::Binary(bytes))) => {
                return Ok(deserialize_server_msg(&bytes)?);
            }
            Some(Ok(Message::Ping(_))) | Some(Ok(Message::Pong(_))) => continue,
            // A refusal can outrun its own AuthError; then the close code is
            // the only reason left, and this build must not retry it forever.
            // A desync close means the same thing one step later: this build
            // and the server disagree about the world, so the next connection
            // lands in the same place. Both are for a new binary to fix.
            Some(Ok(Message::Close(Some(f)))) if is_build_refusal(u16::from(f.code)) => {
                return Err(BuildRefused(format!(
                    "Server refused this build: {} — update agent-client",
                    f.reason
                ))
                .into())
            }
            // Keep the server's reason: it is the difference between "the
            // server dropped me, here's why" and a bare FIN indistinguishable
            // from a dead path.
            Some(Ok(Message::Close(f))) => match f.map(|f| f.reason).filter(|r| !r.is_empty()) {
                Some(reason) => anyhow::bail!("Server closed connection: {reason}"),
                None => anyhow::bail!("Server closed connection"),
            },
            Some(Ok(other)) => {
                warn!("Unexpected WS frame: {other:?}");
                continue;
            }
            Some(Err(e)) => anyhow::bail!("WebSocket error: {e}"),
            None => anyhow::bail!("WebSocket stream ended"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;

    async fn auth_against(close_code: u16, notice_first: bool) -> anyhow::Error {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let error = ServerMessage::AuthError {
                message: "refused".into(),
            };
            let bytes = onlinerpg_shared::serialize_server_msg(&error).unwrap();
            ws.send(Message::Binary(bytes.into())).await.unwrap();
            if notice_first {
                ws.send(Message::Binary(vec![0xc1].into())).await.unwrap();
            }
            let _ = ws
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::from(close_code),
                    reason: "test".into(),
                })))
                .await;
        });
        let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}"))
            .await
            .unwrap();
        let (_tx, mut rx) = ws.split();
        let err = wait_for_auth(&mut rx, "test").await.unwrap_err();
        server.await.unwrap();
        err
    }

    #[tokio::test]
    async fn protocol_close_after_auth_error_is_a_build_refusal() {
        let err = auth_against(onlinerpg_shared::CLOSE_CODE_PROTOCOL_MISMATCH, true).await;
        assert!(err.is::<BuildRefused>(), "{err}");
    }

    #[tokio::test]
    async fn plain_close_after_auth_error_is_an_account_refusal() {
        let err = auth_against(1000, false).await;
        assert!(err.is::<AuthRejected>(), "{err}");
    }
}
