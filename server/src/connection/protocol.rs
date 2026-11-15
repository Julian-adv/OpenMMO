use super::state::ConnectionState;
use super::throttle::{EARLY_MESSAGE_LOG, PROTOCOL_REFUSAL_LOG};
use crate::types::{ClientKind, ClientMessage, ServerMessage};
use tracing::{info, warn};

/// What a refused client should do. Carried in the rejection text because that
/// message is the only thing an out-of-date client can still show, and it has
/// to serve both audiences: a browser holding a cached bundle (reload) and an
/// agent-client binary on someone else's machine (download a new one).
const CLIENT_UPDATE_HINT: &str =
    "reload the page, or update agent-client (https://github.com/Julian-adv/OpenMMO)";

/// Protocol handshake gate, run before every other message.
///
/// `ClientInfo` must be the first message on a connection and must carry this
/// server's exact `PROTOCOL_VERSION`; anything else is refused with an
/// `AuthError` naming the mismatch, and the connection is closed. Deliberately
/// strict: version-straddling code is the kind that only breaks on the clients
/// we cannot redeploy. See `doc/REMOTE_AGENT_CLIENT.md`.
///
/// Returns `Some(responses)` when the message was consumed (handshake or
/// rejection), `None` when the caller should keep handling it.
pub(super) fn handle_handshake(
    client_msg: &ClientMessage,
    state: &mut ConnectionState,
) -> Option<Vec<ServerMessage>> {
    if let ClientMessage::ClientInfo {
        protocol_version,
        client_kind,
        client_version,
    } = client_msg
    {
        if state.client_kind.is_some() {
            warn!(ip = %state.client_ip, account = state.account_name.as_deref(),
                "Duplicate ClientInfo ignored");
            return Some(vec![]);
        }
        if *protocol_version != onlinerpg_shared::PROTOCOL_VERSION {
            if let Some(tail) = PROTOCOL_REFUSAL_LOG.claim(state.client_ip) {
                warn!(
                    "Refusing client: protocol v{} (server speaks v{}) ip={} kind={client_kind} version={client_version}{tail}",
                    protocol_version,
                    onlinerpg_shared::PROTOCOL_VERSION,
                    state.client_ip,
                );
            }
            state.must_close = true;
            return Some(vec![ServerMessage::AuthError {
                message: format!(
                    "Protocol v{} required, you sent v{} — {CLIENT_UPDATE_HINT}",
                    onlinerpg_shared::PROTOCOL_VERSION,
                    protocol_version
                ),
            }]);
        }
        // Same refusal path, for the break the protocol version cannot see:
        // a stale dungeon generator (see `LAYOUT_VERSION`).
        if !onlinerpg_shared::layout_version_matches(client_version) {
            if let Some(tail) = PROTOCOL_REFUSAL_LOG.claim(state.client_ip) {
                warn!(
                    "Refusing client: dungeon layout {} (server built {}) ip={} kind={client_kind} version={client_version}{tail}",
                    onlinerpg_shared::layout_version_of(client_version).unwrap_or("unstamped"),
                    onlinerpg_shared::LAYOUT_VERSION,
                    state.client_ip,
                );
            }
            state.must_close = true;
            return Some(vec![ServerMessage::AuthError {
                message: format!(
                    "This build's dungeon layouts differ from the server's — {CLIENT_UPDATE_HINT}"
                ),
            }]);
        }
        let kind = ClientKind::from_reported(client_kind);
        info!(
            "Client handshake: kind={} version={client_version} ip={}",
            kind.as_str(),
            state.client_ip
        );
        state.client_kind = Some(kind);
        state.reported_client_version = client_version.chars().take(128).collect();
        return Some(vec![]);
    }

    if state.client_kind.is_none() {
        if let Some(tail) = EARLY_MESSAGE_LOG.claim(state.client_ip) {
            let ip = state.client_ip;
            warn!("Refusing client {ip}: message arrived before ClientInfo{tail}");
        }
        state.must_close = true;
        return Some(vec![ServerMessage::AuthError {
            message: format!("Send ClientInfo first — {CLIENT_UPDATE_HINT}"),
        }]);
    }
    None
}
