use crate::auth::AuthService;
use crate::game::character_attributes::roll_character_attributes;
use crate::game::character_hp::{level_one_max_hp, DEFAULT_CHARACTER_RACE};
use crate::game_state::GameState;
use crate::google_auth::GoogleAuthVerifier;
use crate::types::{
    new_player, Character, CharacterAttributes, CharacterClass, ClientMessage, PlayerId, Position,
    ServerMessage,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::inventory::EquipSlot;
use onlinerpg_shared::{deserialize_client_msg, serialize_server_msg};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{
    accept_async_with_config,
    tungstenite::{protocol::WebSocketConfig, Message},
};
use tracing::{error, info, warn};

const FALLBACK_DEFAULT_MAX_HP: u32 = 13;

/// Credential checkers shared by every connection and the REST API.
pub struct AuthContext {
    /// None when the server was started without a Google client id; browser
    /// logins are rejected until it is configured.
    pub google: Option<GoogleAuthVerifier>,
    pub npc_token: String,
    /// Google account emails allowed to call REST write endpoints.
    pub admin_emails: Vec<String>,
}

impl AuthContext {
    /// Verified-email allowlist check shared by REST writes and in-game
    /// debug/cheat commands.
    pub fn is_admin(&self, claims: &crate::google_auth::GoogleClaims) -> bool {
        claims.email_verified == Some(true)
            && claims.email.as_deref().is_some_and(|email| {
                self.admin_emails
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(email))
            })
    }
}

/// Constant-time equality so the NPC token can't be probed byte by byte.
pub fn token_matches(provided: &str, expected: &str) -> bool {
    provided.len() == expected.len()
        && provided
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
}

/// How many seconds without a heartbeat before we consider the client dead.
const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

/// Grace period before an unauthenticated connection is dropped. Measured
/// from connect time (not heartbeats — those are accepted pre-auth) so idle
/// sockets can't hold server resources without ever authenticating.
const UNAUTH_TIMEOUT_SECS: u64 = 60;

/// Caps tungstenite's 64 MiB default; legit client messages are a few KB.
const MAX_WS_MESSAGE_BYTES: usize = 64 * 1024;

/// Per-connection read buffer; the 128 KiB default is oversized for game packets.
const WS_READ_BUFFER_BYTES: usize = 16 * 1024;

/// Tighter caps until auth succeeds; legit pre-auth traffic is just auth attempts.
const UNAUTH_MAX_MESSAGE_BYTES: usize = 8 * 1024;
const UNAUTH_MAX_MESSAGES: u32 = 30;

struct ConnectionState {
    account_name: Option<String>,
    player_id: Option<PlayerId>,
    direct_rx: Option<mpsc::UnboundedReceiver<ServerMessage>>,
    pending_character_attributes: Option<CharacterAttributes>,
    connected_at: std::time::Instant,
    last_heartbeat: std::time::Instant,
    is_npc: bool,
    /// Account email is on the admin allowlist.
    admin_eligible: bool,
    /// admin_eligible && the entered character's admin_role > 0.
    is_admin: bool,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            account_name: None,
            player_id: None,
            direct_rx: None,
            pending_character_attributes: None,
            connected_at: std::time::Instant::now(),
            last_heartbeat: std::time::Instant::now(),
            is_npc: false,
            admin_eligible: false,
            is_admin: false,
        }
    }

    fn require_auth(&self, action: &str) -> Result<String, Vec<ServerMessage>> {
        match &self.account_name {
            Some(name) => Ok(name.clone()),
            None => {
                warn!("{} requested by unauthenticated client", action);
                Err(vec![ServerMessage::CharacterError {
                    message: "Authenticate first".to_string(),
                }])
            }
        }
    }

    fn require_not_in_game(&self, action: &str) -> Result<(), Vec<ServerMessage>> {
        if self.player_id.is_some() {
            warn!("{} ignored because client is already in game", action);
            Err(vec![ServerMessage::CharacterError {
                message: format!("Cannot {} while in game", action),
            }])
        } else {
            Ok(())
        }
    }
}

pub async fn handle_connection(
    stream: TcpStream,
    game_state: Arc<GameState>,
    auth_service: Arc<AuthService>,
    auth_ctx: Arc<AuthContext>,
) {
    let ws_config = WebSocketConfig::default()
        .max_message_size(Some(MAX_WS_MESSAGE_BYTES))
        .max_frame_size(Some(MAX_WS_MESSAGE_BYTES))
        .read_buffer_size(WS_READ_BUFFER_BYTES);
    let ws_stream = match accept_async_with_config(stream, Some(ws_config)).await {
        Ok(ws) => ws,
        Err(e) => {
            error!("WebSocket handshake failed: {}", e);
            return;
        }
    };

    info!("New WebSocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let mut game_receiver = game_state.subscribe();
    let mut state = ConnectionState::new();

    let mut heartbeat_check = tokio::time::interval(std::time::Duration::from_secs(10));
    let mut unauth_message_count: u32 = 0;

    loop {
        tokio::select! {
            // Periodic timeout checks: unauth grace period, in-game heartbeat
            _ = heartbeat_check.tick() => {
                if state.account_name.is_none()
                    && state.connected_at.elapsed().as_secs() > UNAUTH_TIMEOUT_SECS
                {
                    warn!("Dropping connection: unauthenticated after {}s", UNAUTH_TIMEOUT_SECS);
                    break;
                }
                if state.player_id.is_some()
                    && state.last_heartbeat.elapsed().as_secs() > HEARTBEAT_TIMEOUT_SECS
                {
                    warn!("Heartbeat timeout for player {:?}", state.player_id);
                    break;
                }
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
                                "Dropping unauthenticated connection: pre-auth limits exceeded ({} bytes, message #{})",
                                m.len(),
                                unauth_message_count
                            );
                            break;
                        }
                    }
                }
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        match handle_client_message(
                            &bytes,
                            &game_state,
                            &auth_service,
                            &auth_ctx,
                            &mut state,
                        )
                        .await
                        {
                            Ok(responses) => {
                                // Send all direct responses to this client
                                for response in responses {
                                    match serialize_server_msg(&response) {
                                        Ok(bytes) => {
                                            if let Err(e) = ws_sender.send(Message::Binary(Bytes::from(bytes))).await {
                                                error!(
                                                    "Failed to send direct response to client: {}",
                                                    e
                                                );
                                            }
                                        }
                                        Err(e) => error!("Serialization failed: {}", e),
                                    }
                                }
                            }
                            Err(e) => {
                                error!("Error handling client message: {}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("Client requested close");
                        break;
                    }
                    Some(Err(e)) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        info!("WebSocket stream ended");
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
                            error!("Failed to send message to client: {}", e);
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Game state broadcast channel closed");
                        break;
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("Client lagged behind, skipped {} messages", skipped);
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
                if let Some(msg) = direct_msg {
                    let is_kicked = matches!(msg, ServerMessage::Kicked { .. });
                    match serialize_server_msg(&msg) {
                        Ok(bytes) => {
                            let _ = ws_sender.send(Message::Binary(Bytes::from(bytes))).await;
                        }
                        Err(e) => error!("Serialization failed: {}", e),
                    }
                    if is_kicked {
                        info!("Player {:?} kicked", state.player_id);
                        break;
                    }
                }
            }
        }
    }

    // Save full character state and inventory to DB before cleanup. A kick
    // already flushed and detached the replaced session, so this is a no-op
    // for that player id.
    if let Some(ref id) = state.player_id {
        game_state
            .persist_and_detach_player(id, &auth_service)
            .await;

        game_state.unregister_direct_channel(id).await;
        game_state.unregister_player_character(id).await;
        game_state.remove_player(id).await;
    }

    info!("Connection handler finished");
}

/// Shared tail of both auth paths: load characters, mark the connection
/// authenticated, and build the AuthSuccess reply.
fn finish_auth(
    auth_service: &AuthService,
    state: &mut ConnectionState,
    account_name: String,
    is_npc: bool,
) -> Vec<ServerMessage> {
    let character_records = match auth_service.list_characters(&account_name) {
        Ok(characters) => characters,
        Err(err) => {
            warn!(
                "Failed to load character list for account '{}': {}",
                account_name, err
            );
            return vec![ServerMessage::AuthError {
                message: err.client_message().to_string(),
            }];
        }
    };

    let characters = character_records
        .into_iter()
        .map(character_record_to_shared)
        .collect::<Vec<Character>>();

    state.account_name = Some(account_name.clone());
    state.is_npc = is_npc;
    state.pending_character_attributes = None;

    info!(
        "Account '{}' authenticated successfully with {} character(s)",
        account_name,
        characters.len()
    );
    vec![ServerMessage::AuthSuccess {
        account_name,
        characters,
    }]
}

/// Debug/cheat messages every new Debug* variant must be added to; anything
/// listed here is dropped before dispatch unless the connection is admin.
fn requires_admin(msg: &ClientMessage) -> bool {
    match msg {
        ClientMessage::DebugTeleport { .. }
        | ClientMessage::DebugDropItem { .. }
        | ClientMessage::DebugSetTime { .. }
        | ClientMessage::DebugResetDungeonProps { .. } => true,
        ClientMessage::ChatMessage { message } => message.starts_with("/give "),
        _ => false,
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

    if matches!(
        client_msg,
        ClientMessage::Authenticate { .. } | ClientMessage::AuthenticateNpc { .. }
    ) && state.account_name.is_some()
    {
        warn!("Client is already authenticated");
        return Ok(vec![ServerMessage::AuthError {
            message: "Already authenticated".to_string(),
        }]);
    }

    if requires_admin(&client_msg) && !state.is_admin {
        warn!(
            "Admin-only message rejected for account {:?}",
            state.account_name
        );
        return Ok(match &state.player_id {
            Some(id) => vec![ServerMessage::ChatMessage {
                player_id: id.clone(),
                message: "Admin only".to_string(),
            }],
            None => vec![],
        });
    }

    match client_msg {
        ClientMessage::Authenticate { google_id_token } => {
            let Some(verifier) = &auth_ctx.google else {
                warn!("Google login attempted but no --google-client-id is configured");
                return Ok(vec![ServerMessage::AuthError {
                    message: "Google sign-in is not configured on this server".to_string(),
                }]);
            };

            let claims = match verifier.verify(&google_id_token).await {
                Ok(claims) => claims,
                Err(err) => {
                    warn!("Google token verification failed: {}", err);
                    return Ok(vec![ServerMessage::AuthError {
                        message: "Google sign-in verification failed".to_string(),
                    }]);
                }
            };

            let account_name = match auth_service.login_google(&claims.sub) {
                Ok(name) => name,
                Err(err) => {
                    warn!("Google login failed for sub '{}': {}", claims.sub, err);
                    return Ok(vec![ServerMessage::AuthError {
                        message: err.client_message().to_string(),
                    }]);
                }
            };
            info!("Google sub '{}' -> account '{}'", claims.sub, account_name);

            state.admin_eligible = auth_ctx.is_admin(&claims);
            return Ok(finish_auth(auth_service, state, account_name, false));
        }

        ClientMessage::AuthenticateNpc {
            account_name,
            npc_token,
        } => {
            if !token_matches(&npc_token, &auth_ctx.npc_token) {
                warn!("NPC auth rejected for {:?}: bad token", account_name);
                return Ok(vec![ServerMessage::AuthError {
                    message: "Invalid NPC token".to_string(),
                }]);
            }

            let account_name = match auth_service.login_npc(&account_name) {
                Ok(name) => name,
                Err(err) => {
                    warn!("NPC login failed for {:?}: {}", account_name, err);
                    return Ok(vec![ServerMessage::AuthError {
                        message: err.client_message().to_string(),
                    }]);
                }
            };

            return Ok(finish_auth(auth_service, state, account_name, true));
        }

        ClientMessage::CreateCharacter {
            character_name,
            character_class,
            gender,
        } => {
            if let Err(responses) = state.require_not_in_game("CreateCharacter") {
                return Ok(responses);
            }
            let authed_account_name = match state.require_auth("CreateCharacter") {
                Ok(name) => name,
                Err(responses) => return Ok(responses),
            };

            let Some(rolled_attributes) = state.pending_character_attributes.clone() else {
                warn!(
                    "Character creation requested without rolled stats for account '{}'",
                    authed_account_name
                );
                return Ok(vec![ServerMessage::CharacterError {
                    message: "Roll attributes first".to_string(),
                }]);
            };

            let max_hp = default_character_max_hp(&rolled_attributes, &character_class);
            match auth_service.create_character(
                &authed_account_name,
                &character_name,
                &rolled_attributes,
                max_hp,
                character_class.clone(),
                gender,
            ) {
                Ok(character) => {
                    state.pending_character_attributes = None;
                    info!(
                        "Character '{}' created for account '{}'",
                        character.name, authed_account_name
                    );
                    return Ok(vec![ServerMessage::CharacterCreated {
                        character: character_record_to_shared(character),
                    }]);
                }
                Err(err) => {
                    warn!(
                        "Character create failed for account '{}': {}",
                        authed_account_name, err
                    );
                    return Ok(vec![ServerMessage::CharacterError {
                        message: err.client_message().to_string(),
                    }]);
                }
            }
        }

        ClientMessage::DeleteCharacter { character_id } => {
            if let Err(responses) = state.require_not_in_game("DeleteCharacter") {
                return Ok(responses);
            }
            let authed_account_name = match state.require_auth("DeleteCharacter") {
                Ok(name) => name,
                Err(responses) => return Ok(responses),
            };

            match auth_service.delete_character(&authed_account_name, character_id) {
                Ok(()) => {
                    info!(
                        "Character id={} deleted for account '{}'",
                        character_id, authed_account_name
                    );
                    return Ok(vec![ServerMessage::CharacterDeleted { character_id }]);
                }
                Err(err) => {
                    warn!(
                        "Character delete failed for account '{}': {}",
                        authed_account_name, err
                    );
                    return Ok(vec![ServerMessage::CharacterError {
                        message: err.client_message().to_string(),
                    }]);
                }
            }
        }

        ClientMessage::RollCharacterStats {
            character_class,
            gender,
        } => {
            if let Err(responses) = state.require_not_in_game("RollCharacterStats") {
                return Ok(responses);
            }
            if let Err(responses) = state.require_auth("RollCharacterStats") {
                return Ok(responses);
            }

            let attributes = roll_character_attributes(&character_class, gender);
            let max_hp = default_character_max_hp(&attributes, &character_class);
            state.pending_character_attributes = Some(attributes.clone());
            return Ok(vec![ServerMessage::CharacterStatsRolled {
                attributes,
                max_hp,
            }]);
        }

        ClientMessage::EnterGame { character_id } => {
            if state.player_id.is_some() {
                warn!("Client already entered game, ignoring EnterGame request");
                return Ok(vec![]);
            }

            let authed_account_name = match state.require_auth("EnterGame") {
                Ok(name) => name,
                Err(responses) => return Ok(responses),
            };

            let selected_character =
                match auth_service.get_character_for_account(&authed_account_name, character_id) {
                    Ok(character) => character,
                    Err(err) => {
                        warn!(
                            "EnterGame failed for account '{}': {}",
                            authed_account_name, err
                        );
                        return Ok(vec![ServerMessage::CharacterError {
                            message: err.client_message().to_string(),
                        }]);
                    }
                };

            state.is_admin = state.admin_eligible && selected_character.admin_role > 0;
            if state.is_admin {
                info!(
                    "Account '{}' entering as admin character '{}' (role {})",
                    authed_account_name, selected_character.name, selected_character.admin_role
                );
            }

            // Enforced unique character names allow name-based session replacement.
            game_state
                .kick_player_by_name(&selected_character.name, auth_service)
                .await;

            let max_hp = selected_character.max_hp;
            let character_xp = selected_character.xp;

            let mut player = new_player(
                selected_character.name.clone(),
                selected_character.level,
                max_hp,
                selected_character.class.clone(),
                selected_character.gender,
                Position {
                    x: selected_character.last_x,
                    y: selected_character.last_y,
                    z: selected_character.last_z,
                },
                selected_character.last_rotation,
                state.is_npc,
            );

            // Restore saved health (if available) and floor_level from DB
            if let Some(saved_health) = selected_character.health {
                player.health = saved_health.min(max_hp);
            }
            player.floor_level = selected_character.floor_level;
            // A negative floor means the player logged out inside a
            // dungeon: re-prime that dungeon's runtime, or fall back to
            // the world spawn if the entrance no longer exists.
            if player.floor_level < 0 {
                let ok = game_state
                    .rehydrate_dungeon_player(&player.id, &player.position, player.floor_level)
                    .await;
                if !ok {
                    let spawn = &crate::world_config::world_config().spawn_position;
                    player.position = spawn.position();
                    player.rotation = spawn.rotation;
                    player.floor_level = 0;
                }
            }
            let id = player.id.clone();

            state.direct_rx = Some(game_state.register_direct_channel(&id).await);
            game_state
                .register_player_character(
                    &id,
                    character_id,
                    character_xp,
                    selected_character.attributes.clone(),
                    selected_character.gold,
                )
                .await;

            // Load inventory from DB
            game_state
                .load_player_inventory(&id, character_id, auth_service)
                .await;

            // The equipped off-hand is the authoritative carried-torch state.
            // Resolve it before add_player builds the late-join GameState snapshot.
            let inventory = game_state.get_player_inventory(&id).await;
            player.torch_on = inventory
                .as_ref()
                .is_some_and(|inv| inv.has_equipped_item(EquipSlot::OffHand, "torch"));

            let mut responses = vec![ServerMessage::JoinSuccess {
                player: player.clone(),
                is_admin: state.is_admin,
            }];
            let datetime = game_state.current_game_datetime();
            responses.push(ServerMessage::GameTimeSync {
                is_night: GameState::is_night(&datetime),
                datetime,
            });

            // Send no-spawn zones so client can validate spawn positions
            responses.push(ServerMessage::NoSpawnZones {
                zones: game_state.no_spawn_zones().to_vec(),
            });

            // Send inventory state
            if let Some(inv) = inventory {
                responses.push(ServerMessage::InventoryState { inventory: inv });
            }

            responses.push(ServerMessage::GuardUpdated {
                guard: game_state.effective_guard(&id).await,
            });

            responses.push(ServerMessage::GoldUpdate {
                gold: selected_character.gold,
            });

            let rejoin_floor = player.floor_level;
            let rejoin_pos = player.position;
            if let Some(game_state_msg) = game_state.add_player(player).await {
                responses.push(game_state_msg);
            }
            if rejoin_floor < 0 {
                // Rejoining inside a dungeon: enter its floor (occupancy
                // + lazy monster spawn with this player as AI owner).
                game_state
                    .handle_player_floor_change(&id, 0, rejoin_floor, &rejoin_pos, &rejoin_pos)
                    .await;
            }

            state.player_id = Some(id);

            info!(
                "Account '{}' entered game as character '{}' with player ID {:?}",
                authed_account_name, selected_character.name, state.player_id
            );
            return Ok(responses);
        }

        ClientMessage::PlayerMove {
            position,
            rotation,
            floor_level,
            append,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .update_player_position(
                        id,
                        crate::game_state::MoveCommand {
                            position,
                            rotation,
                            floor_level,
                            append,
                        },
                        state.is_admin,
                        state.is_npc,
                    )
                    .await;
            } else {
                warn!("Received move from client that is not in game");
            }
        }

        ClientMessage::ChatMessage { message } => {
            if let Some(id) = &state.player_id {
                game_state.send_chat_message(id, message).await;
            } else {
                warn!("Received chat message from client that is not in game");
            }
        }

        ClientMessage::RequestSpawnMonster {
            monster_type,
            position,
            rotation,
        } => {
            if let Some(id) = &state.player_id {
                // Validate the client-picked position (no-spawn zones + range)
                if !game_state
                    .validate_spawn_position(id, &monster_type, &position)
                    .await
                {
                    warn!(
                        "Spawn request rejected: position ({:.1}, {:.1}) invalid for {}",
                        position.x, position.z, monster_type
                    );
                } else if let Some(monster) = game_state
                    .spawn_monster(
                        monster_type,
                        position,
                        rotation,
                        Some(id.clone()),
                        0,
                        None,
                        false,
                    )
                    .await
                {
                    game_state
                        .send_direct_message(id, ServerMessage::MonsterAssigned { monster })
                        .await;
                }
            } else {
                warn!("Received spawn request from client that is not in game");
            }
        }

        ClientMessage::MonsterMove {
            monster_id,
            position,
            rotation,
            state: monster_state,
            target_position,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .update_monster_position(
                        id,
                        monster_id,
                        position,
                        rotation,
                        monster_state,
                        target_position,
                    )
                    .await;
            } else {
                warn!("Received monster move from client that is not in game");
            }
        }

        ClientMessage::PlayerAttack { monster_id } => {
            if let Some(id) = &state.player_id {
                game_state.broadcast_player_attack(id, monster_id).await;
            } else {
                warn!("Received attack from client that is not in game");
            }
        }

        ClientMessage::MonsterAttack {
            monster_id,
            target_player_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .broadcast_monster_attack(id, &monster_id, &target_player_id)
                    .await;
            } else {
                warn!("Received monster attack from client that is not in game");
            }
        }

        ClientMessage::RequestRespawn => {
            if let Some(id) = &state.player_id {
                game_state.respawn_player(id).await;
            } else {
                warn!("Received respawn request from client that is not in game");
            }
        }

        ClientMessage::OpenDungeonChest { entrance_id } => {
            if let Some(id) = &state.player_id {
                game_state.open_dungeon_chest(id, &entrance_id).await;
            } else {
                warn!("Received chest open from client that is not in game");
            }
        }

        ClientMessage::BreakDungeonProp {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .break_dungeon_prop(id, &entrance_id, depth, prop_id)
                    .await;
            } else {
                warn!("Received prop break from client that is not in game");
            }
        }

        ClientMessage::OpenDungeonProp {
            entrance_id,
            depth,
            prop_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .open_dungeon_prop(id, &entrance_id, depth, prop_id)
                    .await;
            } else {
                warn!("Received prop open from client that is not in game");
            }
        }

        ClientMessage::ToggleDungeonDoor {
            entrance_id,
            depth,
            door_id,
        } => {
            if let Some(id) = &state.player_id {
                if let Some(is_open) = game_state
                    .toggle_dungeon_door(&entrance_id, depth, door_id)
                    .await
                {
                    if let Some((position, _, floor_level)) =
                        game_state.get_player_position(id).await
                    {
                        game_state
                            .send_direct_message_to_players_within_position(
                                &position,
                                floor_level,
                                crate::game_state::AGENT_EVENT_DELIVERY_RADIUS,
                                ServerMessage::DungeonDoorToggled {
                                    entrance_id,
                                    depth,
                                    door_id,
                                    is_open,
                                },
                                None,
                            )
                            .await;
                    }
                }
            }
        }

        ClientMessage::RequestDungeonDoors { entrance_id } => {
            if let Some(id) = &state.player_id {
                let doors = game_state.dungeon_open_doors(&entrance_id).await;
                game_state
                    .send_direct_message(
                        id,
                        ServerMessage::DungeonDoorsState { entrance_id, doors },
                    )
                    .await;
            }
        }

        ClientMessage::DebugTeleport { position } => {
            if let Some(id) = &state.player_id {
                let rotation = game_state
                    .get_player_position(id)
                    .await
                    .map(|(_, rot, _)| rot)
                    .unwrap_or(0.0);
                // Debug teleports can land inside a dungeon; infer the
                // floor from the target Y instead of trusting the old one.
                let floor_level = game_state.dungeon_floor_for_position(&position).await;
                game_state
                    .teleport_player(id, position, rotation, floor_level)
                    .await;
            } else {
                warn!("Received debug teleport from client that is not in game");
            }
        }

        ClientMessage::DebugDropItem { item_def_id } => {
            if let Some(id) = &state.player_id {
                game_state.debug_drop_item(id, &item_def_id).await;
            } else {
                warn!("Received debug drop from client that is not in game");
            }
        }

        ClientMessage::DebugSetTime { hour, minute } => {
            if state.player_id.is_some() {
                let datetime = game_state.debug_set_time(hour, minute);
                info!(
                    "Debug time jump to {:04}-{:02}-{:02} {:02}:{:02}",
                    datetime.year, datetime.month, datetime.day, datetime.hour, datetime.minute
                );
            } else {
                warn!("Received debug set time from client that is not in game");
            }
        }

        ClientMessage::DebugResetDungeonProps { entrance_id } => {
            if state.player_id.is_some() {
                game_state.debug_reset_dungeon_props(&entrance_id).await;
            } else {
                warn!("Received debug dungeon prop reset from client that is not in game");
            }
        }

        ClientMessage::TorchToggle { enabled } => {
            if let Some(id) = &state.player_id {
                game_state.set_player_torch(id, enabled).await;
            } else {
                warn!("Received torch toggle from client that is not in game");
            }
        }

        ClientMessage::InteractObject {
            object_type,
            object_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_player_interaction(id, Some(object_type), Some(object_id))
                    .await;
            } else {
                warn!("Received interact object from client that is not in game");
            }
        }

        ClientMessage::StopInteraction => {
            if let Some(id) = &state.player_id {
                game_state.set_player_interaction(id, None, None).await;
            } else {
                warn!("Received stop interaction from client that is not in game");
            }
        }

        ClientMessage::Heartbeat => {
            state.last_heartbeat = std::time::Instant::now();
        }

        ClientMessage::PlaceHouse { .. } => {
            warn!("Ignoring client-side PlaceHouse broadcast request; use the housing REST API");
        }

        ClientMessage::ModifyRoom { .. } => {
            // TODO: room modification broadcast
        }

        ClientMessage::RemoveHouse { .. } => {
            warn!("Ignoring client-side RemoveHouse broadcast request; use the housing REST API");
        }

        ClientMessage::ToggleDoor {
            house_id,
            room_index,
            wall_dir,
            segment_index,
        } => {
            // Toggle door is_open and broadcast to all players
            if let Some(ref pid) = state.player_id {
                let toggled = game_state
                    .toggle_door(pid, &house_id, room_index, wall_dir, segment_index)
                    .await;
                if let Some(is_open) = toggled {
                    if let Some((position, _, floor_level)) =
                        game_state.get_player_position(pid).await
                    {
                        game_state
                            .send_direct_message_to_players_within_position(
                                &position,
                                floor_level,
                                crate::game_state::AGENT_EVENT_DELIVERY_RADIUS,
                                ServerMessage::DoorToggled {
                                    house_id,
                                    room_index,
                                    wall_dir,
                                    segment_index,
                                    is_open,
                                },
                                None,
                            )
                            .await;
                    }
                }
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

        ClientMessage::DropItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.drop_item(id, instance_id).await;
            }
        }

        ClientMessage::PickupItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.pickup_item(id, instance_id).await;
            }
        }

        ClientMessage::UseItem { instance_id } => {
            if let Some(id) = &state.player_id {
                game_state.use_item(id, instance_id).await;
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
    }

    Ok(vec![])
}

fn character_record_to_shared(record: crate::auth::CharacterRecord) -> Character {
    Character {
        id: record.id,
        name: record.name,
        created_at: record.created_at,
        level: record.level,
        xp: record.xp,
        max_hp: record.max_hp,
        attributes: record.attributes,
        class: record.class,
        gender: record.gender,
    }
}

fn default_character_max_hp(
    attributes: &CharacterAttributes,
    character_class: &CharacterClass,
) -> u32 {
    match level_one_max_hp(DEFAULT_CHARACTER_RACE, character_class, attributes.con) {
        Ok(value) => value,
        Err(err) => {
            warn!(
                "Failed to resolve level 1 max HP for race='{}', class='{}', con='{}': {}",
                DEFAULT_CHARACTER_RACE,
                character_class.as_str(),
                attributes.con,
                err
            );
            FALLBACK_DEFAULT_MAX_HP
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_requires_exact_token() {
        assert!(token_matches("secret-token", "secret-token"));
        assert!(!token_matches("secret-token", "secret-tokeN"));
        assert!(!token_matches("secret", "secret-token"));
        assert!(!token_matches("", "secret-token"));
    }

    #[test]
    fn requires_admin_classifies_cheat_messages() {
        assert!(requires_admin(&ClientMessage::DebugSetTime {
            hour: 0,
            minute: 0
        }));
        assert!(requires_admin(&ClientMessage::DebugDropItem {
            item_def_id: "x".into()
        }));
        assert!(requires_admin(&ClientMessage::ChatMessage {
            message: "/give sword".into()
        }));
        assert!(!requires_admin(&ClientMessage::ChatMessage {
            message: "hello".into()
        }));
        assert!(!requires_admin(&ClientMessage::Heartbeat));
    }
}
