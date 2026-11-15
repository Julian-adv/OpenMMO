use crate::game_state::KickNotice;
use crate::types::{CharacterAttributes, CharacterClass, ClientKind, PlayerId, ServerMessage};
use bytes::Bytes;
use onlinerpg_shared::furniture_shop::FurnitureTip;
use std::net::IpAddr;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::warn;

const INSTRUMENT_BATCHES_PER_SEC: f32 = 4.0;
const INSTRUMENT_BATCH_BURST: f32 = 4.0;

pub(super) struct InstrumentBatchLimiter {
    tokens: f32,
    last: Instant,
}

impl InstrumentBatchLimiter {
    pub(super) fn new() -> Self {
        Self {
            tokens: INSTRUMENT_BATCH_BURST,
            last: Instant::now(),
        }
    }

    pub(super) fn allow(&mut self) -> bool {
        self.allow_at(Instant::now())
    }

    fn allow_at(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f32();
        self.tokens =
            (self.tokens + elapsed * INSTRUMENT_BATCHES_PER_SEC).min(INSTRUMENT_BATCH_BURST);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

pub(super) struct ConnectionState {
    /// Address the client is held accountable for; see `resolve_client_ip`.
    pub(super) client_ip: IpAddr,
    pub(super) country: String,
    /// Client program reported in `ClientInfo`. `None` until the handshake
    /// arrives, which is what gates every other message.
    pub(super) client_kind: Option<ClientKind>,
    pub(super) reported_client_version: String,
    /// Set when the connection must be dropped right after its pending
    /// responses are flushed (protocol mismatch).
    pub(super) must_close: bool,
    pub(super) account_name: Option<String>,
    pub(super) account_session_id: Option<u64>,
    pub(super) account_rx: Option<mpsc::UnboundedReceiver<KickNotice>>,
    pub(super) player_id: Option<PlayerId>,
    /// In game but no `WorldReady` yet; spares the per-message gate a lock.
    pub(super) awaiting_world_ready: bool,
    /// Entered character's name, kept here so disconnect-path logs can name the
    /// player after `GameState` has already dropped the record.
    pub(super) character_name: Option<String>,
    pub(super) direct_rx: Option<mpsc::UnboundedReceiver<Bytes>>,
    pub(super) pending_character_attributes: Option<CharacterAttributes>,
    pub(super) connected_at: std::time::Instant,
    pub(super) last_heartbeat: std::time::Instant,
    pub(super) next_ip_notice: Option<std::time::Instant>,
    pub(super) is_official_npc: bool,
    /// Account email is on the admin allowlist.
    pub(super) admin_eligible: bool,
    /// admin_eligible && the entered character's admin_role > 0.
    pub(super) is_admin: bool,
    /// Last answered positions poll (spam clamp); dies with the connection.
    last_party_positions_poll: Option<Instant>,
    /// Last answered friend-presence poll (spam clamp).
    last_friends_online_poll: Option<Instant>,
    pub(super) last_furniture_tip: std::collections::HashMap<FurnitureTip, Instant>,
    /// An `EnvReport` was already logged; later ones are dropped (spam clamp).
    pub(super) env_reported: bool,
    /// Credential this connection uploads cape textures with. Lives exactly
    /// as long as the connection, which is what saves the store from having
    /// to expire anything.
    pub(super) cape_upload_token: Option<String>,
    pub(super) instrument_batch_limiter: InstrumentBatchLimiter,
}

/// Positions snapshot requests inside this window are dropped. Steady-state
/// data rides the push tick; a client only asks on map open, so this is
/// purely a spam brake.
const PARTY_POSITIONS_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Friend-presence polls inside this window are dropped. The web client polls
/// every 15s with its panel open and every 60s without; this only bounds what
/// a rewritten client can ask for.
const FRIENDS_ONLINE_MIN_INTERVAL: Duration = Duration::from_secs(5);
const FURNITURE_TIP_INTERVAL: Duration = Duration::from_secs(300);

/// True at most once per clamp window; a clamped poll does not refresh the
/// window, so spam cannot starve refreshes.
fn poll_due(last_poll: &mut Option<Instant>, min_interval: Duration) -> bool {
    let now = Instant::now();
    if last_poll.is_some_and(|last| now.duration_since(last) < min_interval) {
        return false;
    }
    *last_poll = Some(now);
    true
}

impl ConnectionState {
    pub(super) fn new(client_ip: IpAddr) -> Self {
        Self {
            client_ip,
            country: crate::geoip::UNKNOWN_COUNTRY.to_owned(),
            client_kind: None,
            reported_client_version: String::new(),
            must_close: false,
            account_name: None,
            account_session_id: None,
            account_rx: None,
            player_id: None,
            awaiting_world_ready: false,
            character_name: None,
            direct_rx: None,
            pending_character_attributes: None,
            connected_at: std::time::Instant::now(),
            last_heartbeat: std::time::Instant::now(),
            next_ip_notice: None,
            is_official_npc: false,
            admin_eligible: false,
            is_admin: false,
            last_party_positions_poll: None,
            last_friends_online_poll: None,
            last_furniture_tip: Default::default(),
            env_reported: false,
            cape_upload_token: None,
            instrument_batch_limiter: InstrumentBatchLimiter::new(),
        }
    }

    pub(super) fn party_positions_poll_due(&mut self) -> bool {
        poll_due(
            &mut self.last_party_positions_poll,
            PARTY_POSITIONS_MIN_INTERVAL,
        )
    }

    pub(super) fn furniture_tip_due(&self, tip: FurnitureTip) -> bool {
        self.last_furniture_tip
            .get(&tip)
            .is_none_or(|last| last.elapsed() >= FURNITURE_TIP_INTERVAL)
    }

    pub(super) fn friends_online_poll_due(&mut self) -> bool {
        poll_due(
            &mut self.last_friends_online_poll,
            FRIENDS_ONLINE_MIN_INTERVAL,
        )
    }

    pub(super) fn require_auth(&self, action: &str) -> Result<String, Vec<ServerMessage>> {
        match &self.account_name {
            Some(name) => Ok(name.clone()),
            None => {
                warn!(ip = %self.client_ip, action, "Request rejected: unauthenticated client");
                Err(vec![ServerMessage::CharacterError {
                    message: "Authenticate first".to_string(),
                }])
            }
        }
    }

    /// Merchant and Guard are reserved for operator-run NPCs.
    pub(super) fn require_selectable_class(
        &self,
        class: &CharacterClass,
    ) -> Result<(), Vec<ServerMessage>> {
        if self.is_official_npc || class.is_player_selectable() {
            return Ok(());
        }
        warn!(
            ip = %self.client_ip, account = self.account_name.as_deref(),
            class = class.as_str(),
            "Character rejected: class is operator-only"
        );
        Err(vec![ServerMessage::CharacterError {
            message: format!("The {} class is not available", class.as_str()),
        }])
    }

    pub(super) fn require_in_game(&self, request: &str) -> Option<PlayerId> {
        if self.player_id.is_none() {
            warn!(
                ip = %self.client_ip, account = self.account_name.as_deref(),
                "Received {request} from client that is not in game"
            );
        }
        self.player_id
    }

    pub(super) fn require_not_in_game(&self, action: &str) -> Result<(), Vec<ServerMessage>> {
        if self.player_id.is_some() {
            warn!(
                ip = %self.client_ip, account = self.account_name.as_deref(),
                character = self.character_name.as_deref(),
                player_id = self.player_id.map(PlayerId::get), action,
                "Request ignored because client is already in game"
            );
            Err(vec![ServerMessage::CharacterError {
                message: format!("Cannot {} while in game", action),
            }])
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
