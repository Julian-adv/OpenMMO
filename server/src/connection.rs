use crate::auth::AuthService;
use crate::game_state::GameState;
use crate::types::{new_player, ClientMessage, PlayerId, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::{deserialize_client_msg, serialize_server_msg};
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

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
    let mut player_id: Option<PlayerId> = None;
    let mut direct_rx: Option<mpsc::UnboundedReceiver<ServerMessage>> = None;

    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Binary(bytes))) => {
                        match handle_client_message(
                            &bytes,
                            &game_state,
                            &auth_service,
                            &mut player_id,
                            &mut direct_rx,
                        )
                        .await
                        {
                            Ok(responses) => {
                                // Send all direct responses to this client
                                for response in responses {
                                    match serialize_server_msg(&response) {
                                        Ok(bytes) => {
                                            if let Err(e) = ws_sender.send(Message::Binary(bytes)).await {
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
                    Ok(server_msg) => {
                        // Filter out monster move updates for the owner
                        if let ServerMessage::MonsterMoved { owner_id: Some(ref owner), .. } = server_msg {
                            if let Some(ref current_player) = player_id {
                                if owner == current_player {
                                    continue;
                                }
                            }
                        }

                        match serialize_server_msg(&server_msg) {
                            Ok(bytes) => {
                                if let Err(e) = ws_sender.send(Message::Binary(bytes)).await {
                                    error!("Failed to send message to client: {}", e);
                                    break;
                                }
                            }
                            Err(e) => error!("Serialization failed: {}", e),
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
                match direct_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            } => {
                if let Some(msg) = direct_msg {
                    let is_kicked = matches!(msg, ServerMessage::Kicked { .. });
                    match serialize_server_msg(&msg) {
                        Ok(bytes) => {
                            let _ = ws_sender.send(Message::Binary(bytes)).await;
                        }
                        Err(e) => error!("Serialization failed: {}", e),
                    }
                    if is_kicked {
                        info!("Player {:?} kicked", player_id);
                        player_id = None;
                        break;
                    }
                }
            }
        }
    }

    // Clean up on disconnect
    if let Some(ref id) = player_id {
        game_state.unregister_direct_channel(id).await;
        game_state.remove_player(id).await;
    }

    info!("Connection handler finished");
}

async fn handle_client_message(
    message: &[u8],
    game_state: &Arc<GameState>,
    auth_service: &Arc<AuthService>,
    player_id: &mut Option<PlayerId>,
    direct_rx: &mut Option<mpsc::UnboundedReceiver<ServerMessage>>,
) -> Result<Vec<ServerMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let client_msg: ClientMessage = deserialize_client_msg(message)?;

    match client_msg {
        ClientMessage::Join {
            player_name,
            password_hash,
            create_account,
        } => {
            if player_id.is_some() {
                warn!("Player already joined, ignoring join request");
                return Ok(vec![]);
            }

            if let Err(auth_err) =
                auth_service.authenticate(&player_name, &password_hash, create_account)
            {
                warn!(
                    "Auth failed for player '{}', create_account={}: {}",
                    player_name, create_account, auth_err
                );
                return Ok(vec![ServerMessage::AuthError {
                    message: auth_err.client_message().to_string(),
                }]);
            }

            // Kick any existing player with the same name
            game_state.kick_player_by_name(&player_name).await;

            let player = new_player(player_name);
            let id = player.id.clone();

            // Register direct channel for this player
            *direct_rx = Some(game_state.register_direct_channel(&id).await);

            // Send join_success directly to this client
            let mut responses = vec![ServerMessage::JoinSuccess {
                player: player.clone(),
            }];

            // add_player returns game_state if there are other players
            if let Some(game_state_msg) = game_state.add_player(player).await {
                responses.push(game_state_msg);
            }
            *player_id = Some(id);

            info!("Player joined with ID: {:?}", player_id);
            return Ok(responses);
        }

        ClientMessage::PlayerMove { position, rotation } => {
            if let Some(id) = player_id {
                game_state
                    .update_player_position(id, position, rotation)
                    .await;
            } else {
                warn!("Received move from unauthenticated client");
            }
        }

        ClientMessage::ChatMessage { message } => {
            if let Some(id) = player_id {
                game_state.send_chat_message(id, message).await;
            } else {
                warn!("Received chat message from unauthenticated client");
            }
        }

        ClientMessage::RequestSpawnMonster {
            monster_type,
            position,
            rotation,
        } => {
            if let Some(id) = player_id {
                game_state
                    .spawn_monster(monster_type, position, rotation, Some(id.clone()))
                    .await;
            } else {
                warn!("Received spawn request from unauthenticated client");
            }
        }

        ClientMessage::MonsterMove {
            monster_id,
            position,
            rotation,
            state,
            target_position,
        } => {
            if player_id.is_some() {
                game_state
                    .update_monster_position(monster_id, position, rotation, state, target_position)
                    .await;
            } else {
                warn!("Received monster move from unauthenticated client");
            }
        }

        ClientMessage::PlayerAttack { monster_id } => {
            if let Some(id) = player_id {
                game_state.broadcast_player_attack(id, monster_id).await;
            } else {
                warn!("Received attack from unauthenticated client");
            }
        }

        ClientMessage::MonsterAttack {
            monster_id,
            target_player_id,
        } => {
            if player_id.is_some() {
                game_state
                    .broadcast_monster_attack(&monster_id, &target_player_id)
                    .await;
            } else {
                warn!("Received monster attack from unauthenticated client");
            }
        }

        ClientMessage::RequestRespawn => {
            if let Some(id) = player_id {
                game_state.respawn_player(id).await;
            } else {
                warn!("Received respawn request from unauthenticated client");
            }
        }
    }

    Ok(vec![])
}
