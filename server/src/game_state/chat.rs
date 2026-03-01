use crate::types::{PlayerId, ServerMessage};
use tracing::{info, warn};

impl super::GameState {
    pub async fn send_chat_message(&self, player_id: &PlayerId, message: String) {
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
