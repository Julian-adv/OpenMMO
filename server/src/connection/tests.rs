use super::account::finish_auth;
#[cfg(debug_assertions)]
use super::auth_context::{dev_login_secret, parse_dev_login};
use super::*;
use crate::types::{CharacterClass, ClientKind};
use std::net::Ipv4Addr;

mod account;
mod dispatch;
mod entry;
mod protocol;
mod watching;

fn test_auth_ctx() -> Arc<AuthContext> {
    Arc::new(AuthContext {
        google: None,
        npc_token: String::new(),
        admin_emails: vec![],
    })
}

fn client_info(protocol_version: u32, kind: &str) -> ClientMessage {
    ClientMessage::ClientInfo {
        protocol_version,
        client_kind: kind.to_string(),
        client_version: onlinerpg_shared::stamp_layout_version("test"),
    }
}

fn is_auth_error(responses: &Option<Vec<ServerMessage>>) -> bool {
    matches!(
        responses.as_deref(),
        Some([ServerMessage::AuthError { .. }])
    )
}
