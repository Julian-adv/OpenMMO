use crate::auth::AuthService;
use crate::game::character_attributes::roll_character_attributes;
use crate::game::character_hp::{level_one_max_hp, DEFAULT_CHARACTER_RACE};
use crate::game_state::GameState;
use crate::types::{
    new_player, Character, CharacterAttributes, CharacterClass, ClientMessage, PlayerId, Position,
    ServerMessage,
};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{deserialize_client_msg, serialize_server_msg};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

const FALLBACK_DEFAULT_MAX_HP: u32 = 13;

/// How many seconds without a heartbeat before we consider the client dead.
const HEARTBEAT_TIMEOUT_SECS: u64 = 30;

struct ConnectionState {
    account_name: Option<String>,
    player_id: Option<PlayerId>,
    direct_rx: Option<mpsc::UnboundedReceiver<ServerMessage>>,
    pending_character_attributes: Option<CharacterAttributes>,
    last_heartbeat: std::time::Instant,
    is_npc: bool,
}

impl ConnectionState {
    fn new() -> Self {
        Self {
            account_name: None,
            player_id: None,
            direct_rx: None,
            pending_character_attributes: None,
            last_heartbeat: std::time::Instant::now(),
            is_npc: false,
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
) {
    let ws_stream = match accept_async(stream).await {
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

    loop {
        tokio::select! {
            // Heartbeat timeout check (only for in-game players)
            _ = heartbeat_check.tick() => {
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
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        match handle_client_message(
                            &bytes,
                            &game_state,
                            &auth_service,
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
                    Ok(msg) => {
                        // Skip messages intended to be filtered for this player
                        if let Some(ref skip_id) = msg.skip_player_id {
                            if let Some(ref current_player) = state.player_id {
                                if skip_id == current_player {
                                    continue;
                                }
                            }
                        }

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

    // Save full character state to DB before cleanup
    if let Some(ref id) = state.player_id {
        if let Some(save_data) = game_state.get_player_save_data(id).await {
            game_state.remove_dirty(id).await;
            let auth = auth_service.clone();
            if let Err(e) =
                tokio::task::spawn_blocking(move || auth.save_characters_batch(&[save_data]))
                    .await
                    .unwrap_or_else(|e| {
                        error!("spawn_blocking panicked: {}", e);
                        Ok(())
                    })
            {
                error!("Failed to save character state on disconnect: {}", e);
            }
        }

        game_state.unregister_direct_channel(id).await;
        game_state.unregister_player_character(id).await;
        game_state.remove_player(id).await;
    }

    info!("Connection handler finished");
}

async fn handle_client_message(
    message: &[u8],
    game_state: &Arc<GameState>,
    auth_service: &Arc<AuthService>,
    state: &mut ConnectionState,
) -> Result<Vec<ServerMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let client_msg: ClientMessage = deserialize_client_msg(message)?;

    match client_msg {
        ClientMessage::Authenticate {
            account_name: requested_account_name,
            password_hash,
            create_account,
            is_npc,
        } => {
            if state.account_name.is_some() {
                warn!("Client is already authenticated");
                return Ok(vec![ServerMessage::AuthError {
                    message: "Already authenticated".to_string(),
                }]);
            }

            if let Err(auth_err) =
                auth_service.authenticate(&requested_account_name, &password_hash, create_account)
            {
                warn!(
                    "Auth failed for account '{}', create_account={}: {}",
                    requested_account_name, create_account, auth_err
                );
                return Ok(vec![ServerMessage::AuthError {
                    message: auth_err.client_message().to_string(),
                }]);
            }

            let character_records = match auth_service.list_characters(&requested_account_name) {
                Ok(characters) => characters,
                Err(err) => {
                    warn!(
                        "Failed to load character list for account '{}': {}",
                        requested_account_name, err
                    );
                    return Ok(vec![ServerMessage::AuthError {
                        message: err.client_message().to_string(),
                    }]);
                }
            };

            let characters = character_records
                .into_iter()
                .map(character_record_to_shared)
                .collect::<Vec<Character>>();

            state.account_name = Some(requested_account_name.clone());
            state.is_npc = is_npc;
            state.pending_character_attributes = None;

            info!(
                "Account '{}' authenticated successfully with {} character(s)",
                requested_account_name,
                characters.len()
            );
            return Ok(vec![ServerMessage::AuthSuccess {
                account_name: requested_account_name,
                characters,
            }]);
        }

        ClientMessage::CreateCharacter {
            character_name,
            character_class,
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

        ClientMessage::RollCharacterStats => {
            if let Err(responses) = state.require_not_in_game("RollCharacterStats") {
                return Ok(responses);
            }
            if let Err(responses) = state.require_auth("RollCharacterStats") {
                return Ok(responses);
            }

            let attributes = roll_character_attributes();
            // Class is not yet chosen at roll time; use knight as preview (warrior and knight share the same hit die)
            let max_hp = default_character_max_hp(&attributes, &CharacterClass::Knight);
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

            // Enforced unique character names allow name-based session replacement.
            game_state
                .kick_player_by_name(&selected_character.name)
                .await;

            let max_hp = selected_character.max_hp;
            let character_xp = selected_character.xp;

            let mut player = new_player(
                selected_character.name.clone(),
                selected_character.level,
                max_hp,
                selected_character.class.clone(),
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
            let id = player.id.clone();

            state.direct_rx = Some(game_state.register_direct_channel(&id).await);
            game_state
                .register_player_character(
                    &id,
                    character_id,
                    character_xp,
                    selected_character.attributes.clone(),
                )
                .await;

            let mut responses = vec![ServerMessage::JoinSuccess {
                player: player.clone(),
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

            if let Some(game_state_msg) = game_state.add_player(player).await {
                responses.push(game_state_msg);
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
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .update_player_position(id, position, rotation, floor_level)
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
                // Validate position is within allowed spawn area
                if !game_state.validate_spawn_position(&monster_type, &position) {
                    warn!(
                        "Spawn request rejected: position ({:.1}, {:.1}) outside allowed area for {}",
                        position.x, position.z, monster_type
                    );
                } else if let Some(monster) = game_state
                    .spawn_monster(monster_type, position, rotation, Some(id.clone()))
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
            if state.player_id.is_some() {
                game_state
                    .update_monster_position(
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

        ClientMessage::DebugTeleport { position } => {
            if let Some(id) = &state.player_id {
                let rotation = game_state
                    .get_player_position(id)
                    .await
                    .map(|(_, rot)| rot)
                    .unwrap_or(0.0);
                game_state.teleport_player(id, position, rotation).await;
            } else {
                warn!("Received debug teleport from client that is not in game");
            }
        }

        ClientMessage::TorchToggle { enabled } => {
            if let Some(id) = &state.player_id {
                game_state.toggle_player_torch(id, enabled).await;
            } else {
                warn!("Received torch toggle from client that is not in game");
            }
        }

        ClientMessage::InteractFurniture {
            furniture_type,
            furniture_id,
        } => {
            if let Some(id) = &state.player_id {
                game_state
                    .set_player_interaction(id, Some(furniture_type), Some(furniture_id))
                    .await;
            } else {
                warn!("Received interact furniture from client that is not in game");
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

        ClientMessage::PlaceHouse { house } => {
            let player_id = state.player_id.clone();
            if let Some(pid) = player_id {
                game_state.broadcast(ServerMessage::HouseSpawned { house }, Some(pid));
            }
        }

        ClientMessage::ModifyRoom { .. } => {
            // TODO: room modification broadcast
        }

        ClientMessage::RemoveHouse { house_id } => {
            let player_id = state.player_id.clone();
            if let Some(pid) = player_id {
                game_state.broadcast(ServerMessage::HouseRemoved { house_id }, Some(pid));
            }
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
                    // Broadcast to ALL players (including sender) so everyone
                    // converges on the server-authoritative door state.
                    game_state.broadcast(
                        ServerMessage::DoorToggled {
                            house_id,
                            room_index,
                            wall_dir,
                            segment_index,
                            is_open,
                        },
                        None,
                    );
                }
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
