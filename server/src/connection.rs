use crate::auth::AuthService;
use crate::conn_limit::ConnectLimiter;
use crate::game_state::{
    encode_server_msg, parse_admin_command, parse_notice_command, EstateFurnitureMove, GameState,
};
use crate::item_defs::AuthenticatedUseAction;
use crate::types::{ClientMessage, PlayerId, ServerMessage};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::ability::AbilityId;
use onlinerpg_shared::deserialize_client_msg;
use onlinerpg_shared::furniture_shop::{FurnitureTip, SHOP};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, OnceLock};
use std::time::Instant;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame, WebSocketConfig},
        Message,
    },
};
use tracing::{debug, error, info, warn};

mod account;
mod auth_context;
mod entry;
mod handshake;
mod protocol;
mod state;
mod throttle;

pub use auth_context::{token_matches, AuthContext};
use entry::{end_entry_grace, ends_entry_grace};
use protocol::handle_handshake;
use state::ConnectionState;
use throttle::RATE_LIMIT_LOG;
pub use throttle::{flush_refusal_logs, REFUSAL_LOG_WINDOW};

/// How many seconds without a heartbeat before we consider the client dead.
const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

/// Grace period before an unauthenticated connection is dropped. Measured
/// from connect time (not heartbeats — those are accepted pre-auth) so idle
/// sockets can't hold server resources without ever authenticating.
const UNAUTH_TIMEOUT_SECS: u64 = 60;

/// IP notices repeat while in game so a long session still sees them.
const IP_NOTICE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// Caps tungstenite's 64 MiB default; legit client messages are a few KB.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;

/// Per-connection read buffer; the 128 KiB default is oversized for game packets.
const WS_READ_BUFFER_BYTES: usize = 16 * 1024;

/// Tighter caps until auth succeeds; legit pre-auth traffic is just auth attempts.
const UNAUTH_MAX_MESSAGE_BYTES: usize = 8 * 1024;
const UNAUTH_MAX_MESSAGES: u32 = 30;

fn close_frame(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code: CloseCode::Library(code),
        reason: reason.into(),
    }))
}

/// Services shared by all connections.
pub struct ServerContext {
    pub geoip: crate::geoip::GeoIp,
    pub game_state: Arc<GameState>,
    pub auth_service: Arc<AuthService>,
    pub auth_ctx: Arc<AuthContext>,
    pub connect_limiter: ConnectLimiter,
    /// See doc/IP_NOTICES.md.
    pub ip_notices: Option<std::path::PathBuf>,
}

pub async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    ctx: Arc<ServerContext>,
    shutdown_started: watch::Receiver<()>,
    mut shutdown: watch::Receiver<()>,
) {
    let ServerContext {
        geoip,
        game_state,
        auth_service,
        auth_ctx,
        connect_limiter,
        ip_notices,
    } = &*ctx;
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_WS_MESSAGE_BYTES))
        .read_buffer_size(WS_READ_BUFFER_BYTES);

    let forwarded_ip: OnceLock<IpAddr> = OnceLock::new();
    let stream = handshake::HandshakeStream::new(stream, peer, &forwarded_ip);

    let handshake = tokio::select! {
        biased;
        _ = shutdown.changed() => {
            info!("Closing pending connection for server shutdown");
            return;
        }
        result = accept_async_with_config(stream, Some(ws_config)) => result,
    };
    let client_ip = forwarded_ip.get().copied().unwrap_or(peer.ip());
    let ws_stream = match handshake {
        Ok(ws) => ws,
        Err(e) => {
            error!(
                ip = %client_ip, peer_ip = %peer.ip(), error = %e,
                "WebSocket handshake failed"
            );
            return;
        }
    };

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Charged here rather than at accept: behind nginx the peer address is
    // useless, and this is the first point where the real one is known.
    // Connections that skipped nginx were already charged in the accept loop.
    if peer.ip().is_loopback() && !connect_limiter.allow(client_ip) {
        if let Some(tail) = RATE_LIMIT_LOG.claim(client_ip) {
            warn!("Rate-limiting client {client_ip}: too many connections{tail}");
        }
        let _ = ws_sender
            .send(close_frame(
                onlinerpg_shared::CLOSE_CODE_RATE_LIMITED,
                "too many connections",
            ))
            .await;
        return;
    }

    debug!(ip = %client_ip, "New WebSocket connection established");

    let mut game_receiver = game_state.subscribe();
    let mut state = ConnectionState::new(client_ip);
    state.country = geoip.country(client_ip);

    let mut heartbeat_check = tokio::time::interval(std::time::Duration::from_secs(10));
    let mut unauth_message_count: u32 = 0;

    // Built once and polled by reference: rebuilding it per iteration would
    // re-register a waker on tokio's shared notify shards for every message.
    let shutdown = async move {
        let _ = shutdown.changed().await;
    };
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            biased;

            _ = &mut shutdown => {
                info!("Closing connection for server shutdown");
                break;
            }

            // A replacement login wins over buffered traffic from the stale socket.
            account_msg = async {
                match state.account_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(notice) = account_msg {
                    if let Some(bytes) = encode_server_msg(&notice.message) {
                        let _ = ws_sender.send(Message::Binary(bytes)).await;
                    }
                    if notice.keep_open {
                        continue;
                    }
                    if let Some(code) = notice.close_code {
                        let _ = ws_sender.send(close_frame(code, "client desync")).await;
                    }
                    info!(ip = %state.client_ip, account = state.account_name.as_deref(),
                        character = state.character_name.as_deref(), player_id = state.player_id.map(PlayerId::get),
                        "Session ended by the server");
                    break;
                }
            }

            // Periodic timeout checks: unauth grace period, in-game heartbeat
            _ = heartbeat_check.tick() => {
                if state.account_name.is_none()
                    && state.connected_at.elapsed().as_secs() > UNAUTH_TIMEOUT_SECS
                {
                    warn!("Dropping connection: unauthenticated after {}s ip={}", UNAUTH_TIMEOUT_SECS, state.client_ip);
                    let _ = ws_sender.send(close_frame(
                        onlinerpg_shared::CLOSE_CODE_IDLE_TIMEOUT,
                        "login did not complete in time",
                    )).await;
                    break;
                }
                if state.account_name.is_none() {
                    // World broadcasts are withheld until auth, so this ping is
                    // the only thing telling a client mid-login that the socket
                    // is still alive. Costs nothing at rest: a connection is
                    // unauthenticated for at most UNAUTH_TIMEOUT_SECS.
                    let _ = ws_sender.send(Message::Ping(Bytes::new())).await;
                }
                if state.player_id.is_some()
                    && state.last_heartbeat.elapsed().as_secs() > HEARTBEAT_TIMEOUT_SECS
                {
                    warn!(ip = %state.client_ip, account = state.account_name.as_deref(),
                        character = state.character_name.as_deref(), player_id = state.player_id.map(PlayerId::get),
                        "Heartbeat timeout");
                    let _ = ws_sender.send(close_frame(
                        onlinerpg_shared::CLOSE_CODE_IDLE_TIMEOUT,
                        "no heartbeat",
                    )).await;
                    break;
                }
                send_due_ip_notices(&mut ws_sender, ip_notices.as_deref(), &mut state).await;
                continue;
            }

            // Handle incoming messages from client
            msg = ws_receiver.next() => {
                // Metered before the type match so Text/Ping can't bypass it
                if state.account_name.is_none() {
                    if let Some(Ok(m)) = &msg {
                        unauth_message_count += 1;
                        if m.len() > UNAUTH_MAX_MESSAGE_BYTES
                            || unauth_message_count > UNAUTH_MAX_MESSAGES
                        {
                            warn!(
                                "Dropping unauthenticated connection: pre-auth limits exceeded ({} bytes, message #{}) ip={}",
                                m.len(),
                                unauth_message_count,
                                state.client_ip
                            );
                            break;
                        }
                    }
                }
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        match handle_client_message(
                            &bytes,
                            game_state,
                            auth_service,
                            auth_ctx,
                            &mut state,
                        )
                        .await
                        {
                            Ok(responses) => {
                                // Send all direct responses to this client
                                for response in responses {
                                    let Some(bytes) = encode_server_msg(&response) else {
                                        continue;
                                    };
                                    if let Err(e) = ws_sender.send(Message::Binary(bytes)).await {
                                        error!(ip = %state.client_ip, account = state.account_name.as_deref(),
                                            error = %e, "Failed to send direct response to client");
                                    }
                                }
                                send_due_ip_notices(&mut ws_sender, ip_notices.as_deref(), &mut state).await;
                                if state.must_close {
                                    debug!("Closing connection: failed protocol handshake");
                                    // Short reason: 123-byte cap. The full
                                    // hint went out as the AuthError above.
                                    let _ = ws_sender
                                        .send(close_frame(
                                            onlinerpg_shared::CLOSE_CODE_PROTOCOL_MISMATCH,
                                            "protocol mismatch",
                                        ))
                                        .await;
                                    break;
                                }
                            }
                            Err(e) => {
                                error!(ip = %state.client_ip, account = state.account_name.as_deref(),
                                    character = state.character_name.as_deref(),
                                    player_id = state.player_id.map(PlayerId::get), error = %e,
                                    "Closing connection: undecodable client message");
                                let _ = ws_sender
                                    .send(close_frame(
                                        onlinerpg_shared::CLOSE_CODE_CLIENT_DESYNC,
                                        "undecodable message",
                                    ))
                                    .await;
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!(ip = %state.client_ip, account = state.account_name.as_deref(),
                            player_id = state.player_id.map(PlayerId::get), "Client requested close");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(ip = %state.client_ip, account = state.account_name.as_deref(),
                            player_id = state.player_id.map(PlayerId::get), error = %e, "WebSocket error");
                        break;
                    }
                    None => {
                        info!(ip = %state.client_ip, account = state.account_name.as_deref(),
                            player_id = state.player_id.map(PlayerId::get), "WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            // Handle game state broadcasts
            broadcast_msg = game_receiver.recv() => {
                match broadcast_msg {
                    // Drop world state for unauthenticated sockets (info leak)
                    Ok(_) if state.account_name.is_none() => {}
                    Ok(msg) => {
                        if let Err(e) = ws_sender.send(Message::Binary(msg.bytes.clone())).await {
                            error!(ip = %state.client_ip, account = state.account_name.as_deref(),
                                player_id = state.player_id.map(PlayerId::get), error = %e,
                                "Failed to send message to client");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Game state broadcast channel closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(ip = %state.client_ip, account = state.account_name.as_deref(),
                            player_id = state.player_id.map(PlayerId::get), skipped,
                            "Client lagged behind");
                    }
                }
            }

            // Handle direct messages to this player
            direct_msg = async {
                match state.direct_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(bytes) = direct_msg {
                    let _ = ws_sender.send(Message::Binary(bytes)).await;
                }
            }
        }
    }

    // Once the drain starts, `persist_shutdown_snapshot` owns persistence for
    // every connected player and needs these maps left populated to see them.
    if !shutdown_started.has_changed().unwrap_or(true) {
        if let (Some(account_name), Some(session_id)) =
            (state.account_name.as_deref(), state.account_session_id)
        {
            game_state
                .end_account_session(account_name, session_id, auth_service)
                .await;
        }
    }

    if let Some(token) = state.cape_upload_token.take() {
        game_state.cape_textures().close_session(&token).await;
    }

    info!(
        ip = %state.client_ip, account = state.account_name.as_deref(),
        character = state.character_name.as_deref(), player_id = state.player_id.map(PlayerId::get),
        "Connection handler finished"
    );
}

/// Debug/cheat messages every new Debug* variant must be added to; anything
/// listed here is dropped before dispatch unless the connection is admin.
fn requires_admin(msg: &ClientMessage) -> bool {
    match msg {
        ClientMessage::DebugTeleport { .. }
        | ClientMessage::DebugDropItem { .. }
        | ClientMessage::DebugSetTime { .. }
        | ClientMessage::DebugResetDungeonProps { .. } => true,
        ClientMessage::ChatMessage { message } => {
            parse_notice_command(message).is_some() || parse_admin_command(message).is_some()
        }
        _ => false,
    }
}

/// Sends this IP's operator notices once due and schedules the next round.
async fn send_due_ip_notices<S>(
    sender: &mut S,
    path: Option<&std::path::Path>,
    state: &mut ConnectionState,
) where
    S: futures_util::Sink<Message> + Unpin,
{
    let now = std::time::Instant::now();
    let Some(path) = path.filter(|_| state.next_ip_notice.is_some_and(|due| due <= now)) else {
        return;
    };
    state.next_ip_notice = Some(now + IP_NOTICE_INTERVAL);
    let notices = crate::ip_notices::notices_for(path, state.client_ip).await;
    if notices.is_empty() {
        return;
    }
    info!(ip = %state.client_ip, account = state.account_name.as_deref(),
        character = state.character_name.as_deref(), count = notices.len(), "Sent IP notices");
    for message in notices {
        if let Some(bytes) = encode_server_msg(&ServerMessage::OperatorNotice { message }) {
            let _ = sender.send(Message::Binary(bytes)).await;
        }
    }
}

async fn handle_client_message(
    message: &[u8],
    game_state: &Arc<GameState>,
    auth_service: &Arc<AuthService>,
    auth_ctx: &Arc<AuthContext>,
    state: &mut ConnectionState,
) -> Result<Vec<ServerMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let client_msg: ClientMessage = deserialize_client_msg(message)?;

    if let Some(responses) = handle_handshake(&client_msg, state) {
        return Ok(responses);
    }

    if matches!(
        client_msg,
        ClientMessage::Authenticate { .. } | ClientMessage::AuthenticateNpc { .. }
    ) && state.account_name.is_some()
    {
        warn!(ip = %state.client_ip, account = state.account_name.as_deref(),
            "Client is already authenticated");
        return Ok(vec![ServerMessage::AuthError {
            message: "Already authenticated".to_string(),
        }]);
    }

    if requires_admin(&client_msg) && !state.is_admin {
        warn!(
            ip = %state.client_ip, account = state.account_name.as_deref(),
            character = state.character_name.as_deref(), player_id = state.player_id.map(PlayerId::get),
            "Admin-only message rejected"
        );
        return Ok(match &state.player_id {
            Some(_) => vec![ServerMessage::SystemMessage {
                localization: None,
                message: "Admin only".to_string(),
            }],
            None => vec![],
        });
    }

    if let Some(id) = state.player_id {
        if state.awaiting_world_ready && ends_entry_grace(&client_msg) {
            end_entry_grace(game_state, state, id, &client_msg).await;
        }
        game_state.advance_goal_players(&[id]).await;
        if client_msg.blocked_while_petrified(state.is_admin) && game_state.is_petrified(&id).await
        {
            return Ok(vec![]);
        }
    }

    match client_msg {
        ClientMessage::Authenticate { google_id_token } => {
            return Ok(account::authenticate(
                game_state,
                auth_service,
                auth_ctx,
                state,
                google_id_token,
            )
            .await);
        }

        ClientMessage::AuthenticateNpc {
            account_name,
            npc_token,
        } => {
            return Ok(account::authenticate_npc(
                game_state,
                auth_service,
                auth_ctx,
                state,
                account_name,
                npc_token,
            )
            .await);
        }

        ClientMessage::CreateCharacter {
            character_name,
            character_class,
            gender,
            appearance,
        } => {
            return Ok(account::create_character(
                game_state,
                auth_service,
                state,
                character_name,
                character_class,
                gender,
                appearance,
            )
            .await);
        }

        ClientMessage::DeleteCharacter { character_id }
        | ClientMessage::CancelCharacterDeletion { character_id } => {
            let cancel = matches!(client_msg, ClientMessage::CancelCharacterDeletion { .. });
            return Ok(account::request_character_deletion(
                game_state,
                auth_service,
                state,
                character_id,
                cancel,
            )
            .await);
        }

        ClientMessage::RenameCharacter {
            character_id,
            new_name,
        } => {
            return Ok(account::rename_character(
                game_state,
                auth_service,
                state,
                character_id,
                new_name,
            )
            .await);
        }

        ClientMessage::RollCharacterStats {
            character_class,
            gender,
        } => {
            return Ok(account::roll_character_stats(
                state,
                character_class,
                gender,
            ));
        }

        ClientMessage::EnterGame { character_id } => {
            return Ok(entry::enter_game(game_state, auth_service, state, character_id).await);
        }

        ClientMessage::PlayerMoveGoal {
            request_id,
            x,
            z,
            sprinting,
            stop_at_entrance,
            client_position,
        } => {
            if let Some(id) = state.player_id {
                game_state
                    .request_move_goal_with_entrance(
                        id,
                        request_id,
                        [x, z],
                        sprinting,
                        stop_at_entrance,
                        client_position,
                    )
                    .await;
            }
        }
        ClientMessage::PlayerMoveStop { request_id } => {
            if let Some(id) = state.player_id {
                game_state.stop_move_goal(id, request_id).await;
            }
        }
        ClientMessage::PlayerMoveDirection {
            request_id,
            rotation,
            forward,
            turn,
            sprinting,
        } => {
            if let Some(id) = state.player_id {
                game_state
                    .request_move_direction(id, request_id, rotation, forward, turn, sprinting)
                    .await;
            }
        }
        ClientMessage::PlayerFace { rotation } => {
            if let Some(id) = state.player_id {
                game_state.face_player(id, rotation).await;
            }
        }

        ClientMessage::ResyncWorld => {
            if let Some(id) = &state.player_id {
                game_state.reset_world_view(id).await;
            }
        }
        ClientMessage::WorldReady => {
            if let Some(id) = &state.require_in_game("world ready") {
                if std::mem::take(&mut state.awaiting_world_ready) {
                    game_state.mark_world_ready(id).await;
                }
            }
        }

        ClientMessage::ChatMessage { message } => {
            if let Some(id) = &state.require_in_game("chat message") {
                game_state
                    .send_chat_message(id, message, auth_service)
                    .await;
            }
        }

        ClientMessage::UseAbility {
            ability,
            monster_id,
            target_player_id,
        } => {
            if let Some(id) = state.require_in_game("attack") {
                let game = Arc::clone(game_state);
                let auth = Arc::clone(auth_service);
                let cast = async move {
                    game.use_targeted_ability(
                        &id,
                        ability,
                        monster_id.as_deref(),
                        target_player_id,
                        Some(&auth),
                    )
                    .await;
                };
                if ability == AbilityId::DaggerDoubleSlash {
                    tokio::spawn(cast);
                } else {
                    cast.await;
                }
            }
        }
        ClientMessage::PlayerAttack { monster_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .player_attack(id, monster_id, Some(auth_service))
                    .await;
            }
        }

        ClientMessage::StartPlayerAttack {
            monster_id,
            dagger_skill,
            request_id,
        } => {
            if let Some(id) = state.require_in_game("attack") {
                game_state
                    .start_player_attack(
                        id,
                        monster_id,
                        dagger_skill,
                        request_id,
                        Some(Arc::clone(auth_service)),
                    )
                    .await;
            }
        }
        ClientMessage::StopPlayerAttack { request_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .stop_player_attack_request(id, Some(request_id))
                    .await;
            }
        }
        ClientMessage::SetPlayerAttackSkill {
            request_id,
            dagger_skill,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_player_attack_skill(id, request_id, dagger_skill)
                    .await;
            }
        }

        ClientMessage::FishingCast { position } => {
            if let Some(id) = &state.require_in_game("fishing cast") {
                game_state.start_fishing(id, position).await;
            }
        }

        ClientMessage::FishingRespond { action } => {
            if let Some(id) = &state.require_in_game("fishing response") {
                game_state.respond_fishing(id, action).await;
            }
        }

        ClientMessage::FishingStop => {
            if let Some(id) = &state.require_in_game("fishing stop") {
                game_state.stop_fishing(id).await;
            }
        }

        ClientMessage::RequestRespawn => {
            if let Some(id) = &state.require_in_game("respawn request") {
                game_state.respawn_player(id).await;
            }
        }

        ClientMessage::OpenDungeonChest { entrance_id } => {
            if let Some(id) = &state.require_in_game("chest open") {
                game_state
                    .open_dungeon_chest(id, &entrance_id, auth_service)
                    .await;
            }
        }

        ClientMessage::BreakDungeonProp {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.require_in_game("prop break") {
                game_state
                    .break_dungeon_prop(id, &entrance_id, depth, prop_id)
                    .await;
            }
        }

        ClientMessage::MineDungeonProp {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.require_in_game("mining") {
                game_state
                    .start_mining(id, &entrance_id, depth, prop_id)
                    .await;
            }
        }

        ClientMessage::PushDungeonCart {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.require_in_game("cart push") {
                game_state
                    .push_dungeon_cart(id, &entrance_id, depth, prop_id, Some(auth_service))
                    .await;
            }
        }

        ClientMessage::OpenDungeonProp {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.require_in_game("prop open") {
                game_state
                    .open_dungeon_prop(id, &entrance_id, depth, prop_id)
                    .await;
            }
        }

        ClientMessage::DebugTeleport { position } => {
            if let Some(id) = &state.player_id {
                game_state.debug_teleport_player(id, position).await;
            }
        }

        ClientMessage::DebugDropItem { item_def_id } => {
            if let Some(id) = &state.require_in_game("debug drop") {
                game_state.debug_drop_item(id, &item_def_id).await;
            }
        }

        ClientMessage::DebugSetTime { hour, minute } => {
            if state.require_in_game("debug set time").is_some() {
                let datetime = game_state.debug_set_time(hour, minute);
                info!("Debug time jump to {}", datetime);
            }
        }

        ClientMessage::DebugResetDungeonProps { entrance_id } => {
            if state.require_in_game("debug dungeon prop reset").is_some() {
                game_state.debug_reset_dungeon_props(&entrance_id).await;
            }
        }

        ClientMessage::InteractObject {
            object_type,
            object_id,
        } => {
            if let Some(id) = &state.require_in_game("interact object") {
                game_state
                    .set_player_interaction(id, Some(object_type), Some(object_id))
                    .await;
            }
        }

        ClientMessage::OpenNoticeBoard { board } => {
            if let Some(id) = &state.require_in_game("open notice board") {
                game_state.open_notice_board(id, board, auth_service).await;
            }
        }

        ClientMessage::ReadNoticePost { board, post_id } => {
            if let Some(id) = &state.require_in_game("read notice post") {
                game_state
                    .read_notice_post(id, board, post_id, auth_service)
                    .await;
            }
        }

        ClientMessage::WriteNoticePost {
            board,
            title,
            body,
            image,
            position,
        } => {
            if let Some(id) = &state.require_in_game("write notice post") {
                game_state
                    .write_notice_post(
                        id,
                        board,
                        &title,
                        &body,
                        image.as_deref(),
                        position,
                        auth_service,
                    )
                    .await;
            }
        }

        ClientMessage::PeekNoticeBoard { board } => {
            if let Some(id) = &state.require_in_game("peek notice board") {
                game_state.peek_notice_board(id, board, auth_service).await;
            }
        }

        ClientMessage::OpenMailbox { mailbox } => {
            if let Some(id) = &state.require_in_game("open mailbox") {
                game_state.open_mailbox(id, mailbox, auth_service).await;
            }
        }

        ClientMessage::ReadMail { mailbox, letter_id } => {
            if let Some(id) = &state.require_in_game("read mail") {
                game_state
                    .read_mail(id, mailbox, letter_id, auth_service)
                    .await;
            }
        }

        ClientMessage::SendMail {
            mailbox,
            recipient,
            body,
        } => {
            if let Some(id) = &state.require_in_game("send mail") {
                game_state
                    .send_mail(id, mailbox, &recipient, &body, auth_service)
                    .await;
            }
        }

        ClientMessage::DeleteMail { mailbox, letter_id } => {
            if let Some(id) = &state.require_in_game("delete mail") {
                game_state
                    .delete_mail(id, mailbox, letter_id, auth_service)
                    .await;
            }
        }

        ClientMessage::TakeDownNotices { object_id } => {
            if let Some(id) = &state.require_in_game("take down notices") {
                game_state
                    .take_down_notices(id, object_id, auth_service)
                    .await;
            }
        }

        ClientMessage::WriteNoticeComment {
            board,
            post_id,
            body,
        } => {
            if let Some(id) = &state.require_in_game("write notice comment") {
                game_state
                    .write_notice_comment(id, board, post_id, &body, auth_service)
                    .await;
            }
        }

        ClientMessage::RemoveNoticePost { board, post_id } => {
            if let Some(id) = &state.require_in_game("remove notice post") {
                game_state
                    .remove_notice_post(id, board, post_id, auth_service)
                    .await;
            }
        }

        ClientMessage::InspectAltar { object_id } => {
            if let Some(id) = &state.require_in_game("inspect altar") {
                game_state.inspect_altar(id, object_id).await;
            }
        }

        ClientMessage::OfferAtAltar { object_id } => {
            if let Some(id) = &state.require_in_game("offer at altar") {
                game_state.offer_at_altar(id, object_id).await;
            }
        }

        ClientMessage::StartInstrument => {
            if let Some(id) = state.require_in_game("instrument start") {
                game_state.start_live_instrument(&id).await;
            }
        }

        ClientMessage::InstrumentNotes { events } => {
            if let Some(id) = state.require_in_game("instrument notes") {
                if state.instrument_batch_limiter.allow() {
                    game_state.play_live_instrument_notes(&id, events).await;
                }
            }
        }

        ClientMessage::StopInteraction => {
            if let Some(id) = &state.require_in_game("stop interaction") {
                game_state.set_player_interaction(id, None, None).await;
            }
        }

        ClientMessage::Heartbeat => {
            state.last_heartbeat = std::time::Instant::now();
        }

        ClientMessage::EnvReport(r) => {
            if state.env_reported {
                return Ok(vec![]);
            }
            state.env_reported = true;
            let account = state.account_name.as_deref().unwrap_or("<unauth>");
            let character = state.character_name.as_deref().unwrap_or("<none>");
            // The client clamps these too, but a hostile client could skip that.
            fn clamp(s: &str, max: usize) -> &str {
                s.char_indices().nth(max).map_or(s, |(i, _)| &s[..i])
            }
            info!(
                target: "env_report",
                "account='{account}' character='{character}' backend={} gpu={}/{}/{} desc='{}' quality={}({}) aa={} \
                 pr={:.2} dpr={:.2} viewport={}x{} screen={}x{} ua='{}'",
                clamp(&r.backend, 16),
                clamp(&r.gpu_vendor, 64),
                clamp(&r.gpu_architecture, 64),
                clamp(&r.gpu_device, 64),
                clamp(&r.gpu_description, 128),
                clamp(&r.quality, 16),
                clamp(&r.render_budget, 16),
                r.antialias,
                r.pixel_ratio,
                r.device_pixel_ratio,
                r.viewport_w,
                r.viewport_h,
                r.screen_w,
                r.screen_h,
                clamp(&r.user_agent, 256),
            );
        }

        ClientMessage::PlaceHouse {
            instance_id,
            origin,
            quarter_turns,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .place_house(id, instance_id, origin, quarter_turns, auth_service)
                    .await;
            }
        }

        ClientMessage::RemoveHouse { house_id } => {
            if let Some(id) = &state.player_id {
                game_state.demolish_house(id, house_id, auth_service).await;
            }
        }

        ClientMessage::ToggleDoor { door } => {
            if let Some(id) = &state.require_in_game("toggle door") {
                game_state.toggle_door(id, &door).await;
            }
        }

        ClientMessage::SelectAmmo { item_def_id } => {
            if let Some(id) = &state.player_id {
                game_state.select_ammo(id, item_def_id).await;
            }
        }
        ClientMessage::EquipItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.equip_item(id, instance_id).await;
            }
        }

        ClientMessage::UnequipItem { slot } => {
            if let Some(id) = &state.player_id {
                game_state.unequip_item(id, slot).await;
            }
        }

        ClientMessage::SetItemLocked {
            instance_id,
            locked,
        } => {
            if let Some(id) = &state.player_id {
                game_state.set_item_locked(id, instance_id, locked).await;
            }
        }

        ClientMessage::DropItems { items } => {
            if let Some(id) = &state.player_id {
                game_state.drop_items(id, items).await;
            }
        }

        ClientMessage::PickupStarted => {
            if let Some(id) = &state.player_id {
                game_state.broadcast_pickup_animation(id).await;
            }
        }

        ClientMessage::PickupItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.pickup_item(id, instance_id).await;
            }
        }

        ClientMessage::PickupNearbyItems => {
            if let Some(id) = &state.player_id {
                game_state.pickup_nearby_items(id).await;
            }
        }

        ClientMessage::UseItem { instance_id } => {
            if let Some(id) = &state.player_id {
                if game_state.teleports_on_use(id, instance_id).await {
                    let (id, auth_service) = (*id, Arc::clone(auth_service));
                    game_state.depart_then(id, move |game| async move {
                        if AuthenticatedUseAction::returns_to_estate(
                            game.authenticated_use_action(&id, instance_id).await,
                        ) {
                            game.use_estate_return_scroll(&id, instance_id, &auth_service)
                                .await;
                        } else {
                            game.use_item(&id, instance_id).await;
                        }
                    });
                    return Ok(vec![]);
                }
                match game_state.authenticated_use_action(id, instance_id).await {
                    Some(
                        AuthenticatedUseAction::EstateReturn | AuthenticatedUseAction::FamilyReturn,
                    ) => {
                        game_state
                            .use_estate_return_scroll(id, instance_id, auth_service)
                            .await;
                    }
                    Some(AuthenticatedUseAction::EstateStorage) => {
                        game_state
                            .try_start_estate_chest_mode(id, instance_id, auth_service)
                            .await;
                    }
                    Some(AuthenticatedUseAction::EstateFence) => {
                        game_state
                            .try_start_fence_mode(id, instance_id, auth_service)
                            .await;
                    }
                    Some(AuthenticatedUseAction::LandClaim) => {
                        game_state
                            .try_preview_land_claim(id, instance_id, auth_service)
                            .await;
                    }
                    None => {
                        if !game_state
                            .try_start_house_placement(id, instance_id, auth_service)
                            .await
                            && !game_state
                                .try_use_landscaping_item(
                                    id,
                                    instance_id,
                                    auth_service,
                                    state.is_admin,
                                )
                                .await
                        {
                            game_state.use_item(id, instance_id).await;
                        }
                    }
                }
            }
        }

        ClientMessage::DyeCape { instance_id, color } => {
            if let Some(id) = &state.player_id {
                game_state.dye_cape(id, instance_id, &color).await;
            }
        }

        ClientMessage::UseLandDocument {
            instance_id,
            tile_x,
            tile_z,
            quadrant,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .claim_land(id, instance_id, (tile_x, tile_z, quadrant), auth_service)
                    .await;
            }
        }
        ClientMessage::EditFence { edge, place } => {
            if let Some(id) = &state.player_id {
                game_state
                    .edit_fence(id, edge, place, auth_service, state.is_admin)
                    .await;
            }
        }
        ClientMessage::PatrolSweep { patrol_id } => {
            if let Some(id) = &state.player_id {
                game_state.patrol_sweep(id, patrol_id, auth_service).await;
            }
        }
        ClientMessage::PatrolScout => {
            if let Some(id) = &state.player_id {
                game_state.patrol_scout(id, auth_service).await;
            }
        }
        ClientMessage::PatrolStrike => {
            if let Some(id) = &state.player_id {
                game_state.patrol_strike(id).await;
            }
        }
        ClientMessage::PatrolBreakOut { recall } => {
            if let Some(id) = &state.player_id {
                game_state.patrol_break_out(id, recall, auth_service).await;
            }
        }
        ClientMessage::ImpoundQuery { npc_player_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .impound_action(id, &npc_player_id, false, auth_service)
                    .await;
            }
        }
        ClientMessage::OfferImpound { target_player_id } => {
            if let Some(id) = &state.player_id {
                game_state.offer_impound(id, &target_player_id).await;
            }
        }
        ClientMessage::ImpoundReclaim { npc_player_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .impound_action(id, &npc_player_id, true, auth_service)
                    .await;
            }
        }
        ClientMessage::StartLandscapingMode { tool } => {
            if let Some(id) = &state.player_id {
                game_state
                    .start_landscaping_mode(id, auth_service, tool, state.is_admin)
                    .await;
            }
        }
        ClientMessage::SelectFurnitureDisplay { display_id } => {
            let tip = SHOP
                .products
                .iter()
                .find(|product| product.display_ids.contains(&display_id))
                .and_then(|product| FurnitureTip::for_item(&product.item_def_id));
            if let (Some(id), Some(tip)) = (state.player_id, tip) {
                if state.furniture_tip_due(tip)
                    && game_state.notify_furniture_selection(&id, display_id).await
                {
                    state.last_furniture_tip.insert(tip, Instant::now());
                }
            }
        }
        ClientMessage::CheckoutFurniture {
            items,
            expected_gold,
            expected_total,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .checkout_furniture(id, items, expected_gold, expected_total, auth_service)
                    .await;
            }
        }
        ClientMessage::SetEstateFurnitureText { furniture_id, text } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_estate_furniture_text(id, furniture_id, text, auth_service)
                    .await;
            }
        }
        ClientMessage::PlaceEstateChest {
            instance_id,
            position,
            rotation_deg,
            floor_level,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .place_estate_chest(
                        id,
                        instance_id,
                        position,
                        rotation_deg,
                        floor_level,
                        auth_service,
                    )
                    .await;
            }
        }
        ClientMessage::EditLandscape { stroke } => {
            if let Some(id) = &state.player_id {
                game_state
                    .edit_landscape(id, stroke, auth_service, state.is_admin)
                    .await;
            }
        }
        ClientMessage::OpenEstateChest { chest_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .open_estate_chest(id, chest_id, auth_service)
                    .await;
            }
        }
        ClientMessage::StartEstateFurnitureMove { furniture_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .start_estate_furniture_move(id, furniture_id, auth_service)
                    .await;
            }
        }
        ClientMessage::MoveEstateFurniture {
            furniture_id,
            expected_revision,
            position,
            rotation_deg,
            floor_level,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .move_estate_furniture(
                        id,
                        EstateFurnitureMove {
                            furniture_id,
                            expected_revision,
                            position,
                            rotation_deg,
                            floor_level,
                        },
                        auth_service,
                    )
                    .await;
            }
        }
        ClientMessage::TransferEstateItems {
            chest_id,
            deposits,
            withdrawals,
            expected_revision,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .transfer_estate_items(
                        id,
                        chest_id,
                        deposits,
                        withdrawals,
                        expected_revision,
                        auth_service,
                    )
                    .await;
            }
        }
        ClientMessage::RecoverEstateChest { chest_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .recover_estate_chest(id, chest_id, auth_service)
                    .await;
            }
        }

        ClientMessage::LandAccount { merchant_player_id } => {
            if let Some(id) = &state.player_id {
                game_state
                    .land_account_action(id, &merchant_player_id, None, auth_service)
                    .await;
            }
        }
        ClientMessage::LandDeposit {
            merchant_player_id,
            amount,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .land_account_action(
                        id,
                        &merchant_player_id,
                        Some((amount, true)),
                        auth_service,
                    )
                    .await;
            }
        }
        ClientMessage::LandWithdraw {
            merchant_player_id,
            amount,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .land_account_action(
                        id,
                        &merchant_player_id,
                        Some((amount, false)),
                        auth_service,
                    )
                    .await;
            }
        }

        ClientMessage::ApplyCapeTexture {
            instance_id,
            texture,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .apply_cape_texture(id, instance_id, &texture)
                    .await;
            }
        }

        ClientMessage::ReportCapeTexture { player_id } => {
            if let Some(id) = &state.player_id {
                game_state.report_cape_texture(id, &player_id).await;
            }
        }

        ClientMessage::TipHat {
            hat_id,
            amount,
            song,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .tip_hat_tip(id, hat_id, amount, song.as_deref())
                    .await;
            }
        }

        ClientMessage::ServeMeal {
            chair_object_id,
            item_def_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .serve_meal(id, chair_object_id, &item_def_id)
                    .await;
            }
        }

        ClientMessage::EatMeal { meal_id } => {
            if let Some(id) = &state.player_id {
                game_state.eat_meal(id, meal_id).await;
            }
        }

        ClientMessage::ClearMeal { meal_id } => {
            if let Some(id) = &state.player_id {
                game_state.clear_meal(id, meal_id).await;
            }
        }

        ClientMessage::CreateFamily {
            merchant_player_id,
            name,
        } => {
            if let Some(id) = &state.require_in_game("found family") {
                game_state
                    .create_family(id, &merchant_player_id, &name, auth_service)
                    .await;
            }
        }

        ClientMessage::InviteToFamily { name } => {
            if let Some(id) = &state.require_in_game("invite to family") {
                game_state.invite_to_family(id, &name, auth_service).await;
            }
        }

        ClientMessage::RespondFamilyInvite { inviter_id, accept } => {
            if let Some(id) = &state.require_in_game("answer family invite") {
                game_state
                    .respond_to_family_invite(id, &inviter_id, accept, auth_service)
                    .await;
            }
        }

        ClientMessage::LeaveFamily => {
            if let Some(id) = &state.require_in_game("leave family") {
                game_state.remove_from_family(id, None, auth_service).await;
            }
        }

        ClientMessage::DissolveFamily { merchant_player_id } => {
            if let Some(id) = &state.require_in_game("dissolve family") {
                game_state
                    .dissolve_family(id, &merchant_player_id, auth_service)
                    .await;
            }
        }

        ClientMessage::KickFamilyMember { character_id } => {
            if let Some(id) = &state.require_in_game("kick family member") {
                game_state
                    .remove_from_family(id, Some(character_id), auth_service)
                    .await;
            }
        }

        ClientMessage::SetFamilyMemberRank { character_id, rank } => {
            if let Some(id) = &state.require_in_game("set family member rank") {
                game_state
                    .set_family_member_rank(id, character_id, rank, auth_service)
                    .await;
            }
        }

        ClientMessage::TransferFamilyHead { character_id } => {
            if let Some(id) = &state.require_in_game("transfer family head") {
                game_state
                    .transfer_family_head(id, character_id, auth_service)
                    .await;
            }
        }

        ClientMessage::ClaimFamilyHead => {
            if let Some(id) = &state.require_in_game("claim family head") {
                game_state.claim_family_head(id, auth_service).await;
            }
        }

        ClientMessage::SetFamilyRank {
            rank,
            label,
            permissions,
        } => {
            if let Some(id) = &state.require_in_game("set family rank") {
                game_state
                    .set_family_rank(id, rank, label, permissions, auth_service)
                    .await;
            }
        }

        ClientMessage::RenameFamily {
            merchant_player_id,
            name,
        } => {
            if let Some(id) = &state.require_in_game("rename family") {
                game_state
                    .rename_family(id, &merchant_player_id, &name, auth_service)
                    .await;
            }
        }

        ClientMessage::AttachEstateToFamily { attach } => {
            if let Some(id) = &state.require_in_game("attach estate to family") {
                game_state
                    .attach_estate_to_family(id, attach, auth_service)
                    .await;
            }
        }

        ClientMessage::DepositFamilyEstate { estate_id, amount } => {
            if let Some(id) = &state.require_in_game("pay family estate taxes") {
                game_state
                    .deposit_family_estate(id, estate_id, amount, auth_service)
                    .await;
            }
        }

        ClientMessage::SetActiveTitle { title } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_active_title(id, title, Some(auth_service))
                    .await;
            }
        }

        ClientMessage::OpenShop { merchant_player_id } => {
            if let Some(id) = &state.player_id {
                game_state.open_shop(id, &merchant_player_id, true).await;
            }
        }

        ClientMessage::CloseShop { merchant_player_id } => {
            if let Some(id) = &state.player_id {
                game_state.close_shop(id, &merchant_player_id).await;
            }
        }

        ClientMessage::PartyInvite { target_name } => {
            if let Some(id) = &state.player_id {
                game_state.invite_to_party(id, &target_name).await;
            }
        }

        ClientMessage::PartyRespond { inviter_id, accept } => {
            if let Some(id) = &state.player_id {
                game_state
                    .respond_to_party_invite(id, &inviter_id, accept)
                    .await;
            }
        }

        ClientMessage::PlayerTradeRequest { target_name } => {
            if let Some(id) = &state.player_id {
                game_state.request_player_trade(id, &target_name).await;
            }
        }

        ClientMessage::OpenStall { stall_id } => {
            if let Some(id) = &state.player_id {
                game_state.open_stall(id, stall_id).await;
            }
        }

        ClientMessage::CloseStall => {
            if let Some(id) = &state.player_id {
                game_state.close_stall(id).await;
            }
        }

        ClientMessage::SetStallSign { sign } => {
            if let Some(id) = &state.player_id {
                game_state.set_stall_sign(id, sign).await;
            }
        }

        ClientMessage::ListStallItem {
            instance_id,
            quantity,
            unit_price,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .list_stall_item(id, instance_id, quantity, unit_price)
                    .await;
            }
        }

        ClientMessage::UnlistStallItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.unlist_stall_item(id, instance_id).await;
            }
        }

        ClientMessage::BuyFromStall { stall_id, lines } => {
            if let Some(id) = &state.player_id {
                game_state
                    .buy_from_stall(id, stall_id, lines, auth_service)
                    .await;
            }
        }

        ClientMessage::SetStallBuyOrder {
            item_def_id,
            quantity,
            enchant,
            unit_price,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_stall_buy_order(id, item_def_id, quantity, enchant, unit_price)
                    .await;
            }
        }

        ClientMessage::RemoveStallBuyOrder { order_id } => {
            if let Some(id) = &state.player_id {
                game_state.remove_stall_buy_order(id, order_id).await;
            }
        }

        ClientMessage::SellToStall {
            stall_id,
            order_id,
            instance_id,
            quantity,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .sell_to_stall(id, stall_id, order_id, instance_id, quantity, auth_service)
                    .await;
            }
        }

        ClientMessage::PlayerTradeRespond {
            requester_id,
            accept,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .respond_player_trade(id, &requester_id, accept)
                    .await;
            }
        }

        ClientMessage::PlayerTradeSetOffer { items, copper } => {
            if let Some(id) = &state.player_id {
                game_state.set_player_trade_offer(id, items, copper).await;
            }
        }

        ClientMessage::PlayerTradeLock { revision } => {
            if let Some(id) = &state.player_id {
                game_state.lock_player_trade(id, revision).await;
            }
        }

        ClientMessage::PlayerTradeUnlock => {
            if let Some(id) = &state.player_id {
                game_state.unlock_player_trade(id).await;
            }
        }

        ClientMessage::PlayerTradeConfirm { revision } => {
            if let Some(id) = &state.player_id {
                game_state
                    .confirm_player_trade(id, revision, auth_service)
                    .await;
            }
        }

        ClientMessage::PlayerTradeCancel => {
            if let Some(id) = &state.player_id {
                game_state.cancel_player_trade(id).await;
            }
        }

        ClientMessage::PartySummonRespond { caster_id, accept } => {
            if let Some(id) = state.player_id {
                if accept {
                    game_state.depart_then(id, move |game| async move {
                        game.respond_to_party_summon(&id, &caster_id, true).await;
                    });
                } else {
                    game_state
                        .respond_to_party_summon(&id, &caster_id, false)
                        .await;
                }
            }
        }

        ClientMessage::PartyLeave => {
            if let Some(id) = &state.player_id {
                game_state.leave_party(id).await;
            }
        }

        ClientMessage::PartyKick { target_id } => {
            if let Some(id) = &state.player_id {
                game_state.kick_from_party(id, &target_id).await;
            }
        }

        ClientMessage::PartyPromote { target_id } => {
            if let Some(id) = &state.player_id {
                game_state.promote_party_leader(id, &target_id).await;
            }
        }

        ClientMessage::PartySetLoot { shared } => {
            if let Some(id) = &state.player_id {
                game_state.set_party_loot(id, Some(shared)).await;
            }
        }

        ClientMessage::SetFamilyEmblem { texture, color } => {
            if let Some(id) = &state.require_in_game("set family emblem") {
                game_state
                    .set_family_emblem(id, texture, color, auth_service)
                    .await;
            }
        }

        ClientMessage::SetFamilySeat { estate_id } => {
            if let Some(id) = &state.require_in_game("choose family seat") {
                game_state
                    .set_family_seat(id, estate_id, auth_service)
                    .await;
            }
        }

        ClientMessage::SetFamilyCapeEmblem { show } => {
            if let Some(id) = &state.require_in_game("choose family cape") {
                game_state
                    .set_family_cape_emblem(id, show, auth_service)
                    .await;
            }
        }

        ClientMessage::ProposeFamilyMerge { head_name, terms } => {
            if let Some(id) = &state.require_in_game("propose family merge") {
                game_state
                    .propose_family_merge(id, &head_name, terms, auth_service)
                    .await;
            }
        }

        ClientMessage::InspectFamilyEmblem { player_name } => {
            if let Some(id) = &state.require_in_game("inspect family emblem") {
                game_state.show_family_emblem(id, player_name).await;
            }
        }

        ClientMessage::OfferMergerPledge { target_player_id } => {
            if let Some(id) = &state.require_in_game("offer merger pledge") {
                game_state
                    .offer_merger_pledge(id, &target_player_id, auth_service)
                    .await;
            }
        }

        ClientMessage::RespondFamilyMerge {
            proposer_id,
            accept,
        } => {
            if let Some(id) = &state.require_in_game("answer family merge") {
                game_state
                    .respond_to_family_merge(id, &proposer_id, accept, auth_service)
                    .await;
            }
        }

        ClientMessage::ConfirmFamilyMerge { accept } => {
            if let Some(id) = &state.require_in_game("confirm family merge") {
                game_state.confirm_family_merge(id, accept).await;
            }
        }

        ClientMessage::RequestFamilyStorageLog => {
            if let Some(id) = &state.player_id {
                game_state.send_family_storage_log(id, auth_service).await;
            }
        }

        ClientMessage::FamilyChat { message } => {
            if let Some(id) = &state.player_id {
                game_state.send_family_chat(id, message).await;
            }
        }

        ClientMessage::PartyChat { message } => {
            if let Some(id) = &state.player_id {
                game_state.send_party_chat(id, message).await;
            }
        }

        ClientMessage::FriendRespond {
            requester_id,
            accept,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .respond_to_friend_request(id, &requester_id, accept, auth_service)
                    .await;
            }
        }

        ClientMessage::FriendRemove { name } => {
            if let Some(id) = &state.player_id {
                game_state
                    .remove_friend_by_name(id, &name, auth_service)
                    .await;
            }
        }

        ClientMessage::RequestFriendsOnline => {
            if let Some(id) = state.player_id {
                if state.friends_online_poll_due() {
                    game_state.send_friends_online(&id).await;
                }
            }
        }

        ClientMessage::RequestPartyPositions => {
            if let Some(id) = state.player_id {
                if state.party_positions_poll_due() {
                    game_state.send_party_positions(&id).await;
                }
            }
        }

        ClientMessage::BuyItem {
            merchant_player_id,
            item_def_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .buy_item(id, &merchant_player_id, &item_def_id)
                    .await;
            }
        }

        ClientMessage::SellItem {
            merchant_player_id,
            instance_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .sell_item(id, &merchant_player_id, instance_id)
                    .await;
            }
        }

        ClientMessage::BuybackItem {
            merchant_player_id,
            entry_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .buyback_item(id, &merchant_player_id, entry_id)
                    .await;
            }
        }

        ClientMessage::BuyItems {
            merchant_player_id,
            items,
        } => {
            if let Some(id) = &state.player_id {
                game_state.buy_items(id, &merchant_player_id, items).await;
            }
        }

        ClientMessage::SellItems {
            merchant_player_id,
            items,
        } => {
            if let Some(id) = &state.player_id {
                game_state.sell_items(id, &merchant_player_id, items).await;
            }
        }

        ClientMessage::BuybackItems {
            merchant_player_id,
            entry_ids,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .buyback_items(id, &merchant_player_id, entry_ids)
                    .await;
            }
        }

        ClientMessage::OfferDeal {
            target_player_id,
            item_def_id,
            kind,
            modifier_pct,
            reason,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .offer_deal(
                        id,
                        &target_player_id,
                        &item_def_id,
                        kind,
                        modifier_pct,
                        &reason,
                    )
                    .await;
            }
        }

        ClientMessage::OpenTrade { target_player_id } => {
            if let Some(id) = &state.player_id {
                game_state.open_trade(id, &target_player_id).await;
            }
        }

        ClientMessage::DeclineTrade { merchant_player_id } => {
            if let Some(id) = &state.player_id {
                game_state.decline_trade(id, &merchant_player_id).await;
            }
        }

        // Consumed by `handle_handshake` above; a repeat never reaches here.
        ClientMessage::ClientInfo { .. } => {}
    }

    Ok(vec![])
}

#[cfg(test)]
mod tests;
