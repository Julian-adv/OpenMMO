use crate::types::{PlayerId, ServerMessage};
use crate::world_config::world_config;
use tracing::{info, warn};

impl super::GameState {
    pub async fn send_chat_message(&self, player_id: &PlayerId, message: String) {
        if message.trim() == "/escape" {
            self.escape_to_spawn(player_id).await;
            return;
        }

        if message.trim() == "/who" {
            let (humans, npcs) =
                {
                    let players = self.players.read().await;
                    players.values().fold((0u32, 0u32), |(h, n), p| {
                        if p.is_npc {
                            (h, n + 1)
                        } else {
                            (h + 1, n)
                        }
                    })
                };
            self.send_direct_message(
                player_id,
                ServerMessage::ChatMessage {
                    player_id: *player_id,
                    message: format!("Online: {} ({} human, {} NPC)", humans + npcs, humans, npcs),
                },
            )
            .await;
            return;
        }

        // Handle /give command
        if let Some(item_id) = message.strip_prefix("/give ") {
            let item_id = item_id.trim();
            if self.give_item(player_id, item_id).await {
                self.send_direct_message(
                    player_id,
                    ServerMessage::ChatMessage {
                        player_id: *player_id,
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
            // Chat content stays out of logs on purpose (privacy, F-012).
            info!(from = %player_name, len = message.len(), "chat message");
            let recipients = self
                .player_ids_within(player_id, super::EVENT_DELIVERY_RADIUS)
                .await;
            self.send_direct_message_to_players(
                &recipients,
                ServerMessage::ChatMessage {
                    player_id: *player_id,
                    message,
                },
            )
            .await;
        } else {
            warn!("Chat message from non-existent player: {}", player_id);
        }
    }

    /// Last resort for a player wedged somewhere movement can't undo: return
    /// them to the world spawn on the surface.
    ///
    /// Open to everyone by design — the players who need it are precisely the
    /// ones who cannot reach an admin. The combat lockout is what keeps it from
    /// doubling as a free disengage.
    async fn escape_to_spawn(&self, player_id: &PlayerId) {
        let reply = |message: &str| ServerMessage::ChatMessage {
            player_id: *player_id,
            message: message.to_string(),
        };

        let in_combat = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                warn!("/escape from non-existent player: {}", player_id);
                return;
            };
            Self::now_ms().saturating_sub(player.last_combat_at) < super::OUT_OF_COMBAT_MS
        };
        if in_combat {
            self.send_direct_message(player_id, reply("Escape: not while in combat."))
                .await;
            return;
        }

        // Queued waypoints target the place being escaped from; snapping to one
        // after the teleport would drag the player straight back.
        self.movement_intents.write().await.remove(player_id);

        let spawn = &world_config().spawn_position;
        self.teleport_player(player_id, spawn.position(), spawn.rotation, 0)
            .await;
        info!("Player {} escaped to spawn", player_id);
        self.send_direct_message(player_id, reply("Escape: returned to the starting point."))
            .await;
    }
}
