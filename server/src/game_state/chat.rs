use crate::types::{PlayerId, ServerMessage};

const CHAT_DELIVERY_RADIUS: f32 = super::AGENT_EVENT_DELIVERY_RADIUS;
use tracing::{info, warn};

impl super::GameState {
    pub async fn send_chat_message(&self, player_id: &PlayerId, message: String) {
        // Handle /give command
        if let Some(item_id) = message.strip_prefix("/give ") {
            let item_id = item_id.trim();
            if self.give_item(player_id, item_id).await {
                self.send_direct_message(
                    player_id,
                    ServerMessage::ChatMessage {
                        player_id: player_id.clone(),
                        message: format!("Gave item: {}", item_id),
                    },
                )
                .await;
            } else {
                self.send_direct_message(
                    player_id,
                    ServerMessage::InventoryError {
                        message: format!("Unknown item: {}", item_id),
                    },
                )
                .await;
            }
            return;
        }

        let player_name = {
            let players = self.players.read().await;
            players.get(player_id).map(|player| player.name.clone())
        };

        if let Some(player_name) = player_name {
            info!("Chat message from {}: {}", player_name, message);
            let recipients = self
                .player_ids_within(player_id, CHAT_DELIVERY_RADIUS)
                .await;
            self.send_direct_message_to_players(
                &recipients,
                ServerMessage::ChatMessage {
                    player_id: player_id.clone(),
                    message,
                },
            )
            .await;
        } else {
            warn!("Chat message from non-existent player: {}", player_id);
        }
    }
}
