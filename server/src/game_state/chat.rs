use crate::types::{PlayerId, ServerMessage};
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

        let players = self.players.read().await;

        if let Some(player) = players.get(player_id) {
            info!("Chat message from {}: {}", player.name, message);
            self.broadcast(
                ServerMessage::ChatMessage {
                    player_id: player_id.clone(),
                    message,
                },
                None,
            );
        } else {
            warn!("Chat message from non-existent player: {}", player_id);
        }
    }
}
