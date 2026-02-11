use crate::game_state::GameState;
use crate::types::{ClientMessage, Player, PlayerId, ServerMessage};
use futures_util::{SinkExt, StreamExt};
use serde_json;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::broadcast;
use tokio_tungstenite::{accept_async, tungstenite::Message};
use tracing::{error, info, warn};

pub async fn handle_connection(stream: TcpStream, game_state: Arc<GameState>) {
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

    loop {
        tokio::select! {
            // Handle incoming messages from client
            msg = ws_receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        info!("Received message: {}", text);
                        match handle_client_message(&text, &game_state, &mut player_id).await
                        {
                            Ok(responses) => {
                                // Send all direct responses to this client
                                for response in responses {
                                    if let Ok(json) = serde_json::to_string(&response) {
                                        if let Err(e) = ws_sender.send(Message::Text(json)).await {
                                            error!(
                                                "Failed to send direct response to client: {}",
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "Error handling client message: {} - message was: {}",
                                    e, text
                                );
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

                        if let Ok(json) = serde_json::to_string(&server_msg) {
                            if let Err(e) = ws_sender.send(Message::Text(json)).await {
                                error!("Failed to send message to client: {}", e);
                                break;
                            }
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
        }
    }

    // Clean up on disconnect
    if let Some(id) = player_id {
        game_state.remove_player(&id).await;
    }

    info!("Connection handler finished");
}

async fn handle_client_message(
    message: &str,
    game_state: &Arc<GameState>,
    player_id: &mut Option<PlayerId>,
) -> Result<Vec<ServerMessage>, Box<dyn std::error::Error + Send + Sync>> {
    let client_msg: ClientMessage = serde_json::from_str(message)?;

    match client_msg {
        ClientMessage::Join { player_name } => {
            if player_id.is_some() {
                warn!("Player already joined, ignoring join request");
                return Ok(vec![]);
            }

            let player = Player::new(player_name);
            let id = player.id.clone();

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
