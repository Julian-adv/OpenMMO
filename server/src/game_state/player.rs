use crate::auth::CharacterSaveData;
use crate::types::{CharacterAttributes, Player, PlayerId, Position, ServerMessage};
use crate::world_config::world_config;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};

fn build_save_data(player: &Player, character_id: i64, xp: u64) -> CharacterSaveData {
    CharacterSaveData {
        character_id,
        x: player.position.x,
        y: player.position.y,
        z: player.position.z,
        rotation: player.rotation,
        xp,
        level: player.level,
        max_hp: player.max_health,
        health: player.health,
        floor_level: player.floor_level,
    }
}

impl super::GameState {
    pub async fn get_or_assign_player_number(&self, player_id: &str) -> u32 {
        let mut id_state = self.id_state.write().await;
        if let Some(player_number) = id_state.player_numbers.get(player_id).copied() {
            player_number
        } else {
            id_state.next_player_number = id_state.next_player_number.saturating_add(1);
            let player_number = id_state.next_player_number;
            id_state
                .player_numbers
                .insert(player_id.to_string(), player_number);
            player_number
        }
    }

    pub async fn register_direct_channel(
        &self,
        player_id: &PlayerId,
    ) -> mpsc::UnboundedReceiver<ServerMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut channels = self.direct_channels.write().await;
        channels.insert(player_id.clone(), tx);
        rx
    }

    pub async fn unregister_direct_channel(&self, player_id: &PlayerId) {
        let mut channels = self.direct_channels.write().await;
        channels.remove(player_id);
    }

    pub async fn send_direct_message(&self, player_id: &PlayerId, msg: ServerMessage) {
        let channels = self.direct_channels.read().await;
        if let Some(tx) = channels.get(player_id) {
            let _ = tx.send(msg);
        }
    }

    pub async fn register_player_character(
        &self,
        player_id: &PlayerId,
        character_id: i64,
        xp: u64,
        attributes: CharacterAttributes,
    ) {
        let mut map = self.player_characters.write().await;
        map.insert(player_id.clone(), (character_id, xp, attributes));
    }

    pub async fn unregister_player_character(&self, player_id: &PlayerId) {
        let mut map = self.player_characters.write().await;
        map.remove(player_id);
    }

    pub async fn kick_player_by_name(&self, name: &str) -> Option<PlayerId> {
        let old_player_id = {
            let players = self.players.read().await;
            players
                .iter()
                .find(|(_, p)| p.name == name)
                .map(|(id, _)| id.clone())
        };

        if let Some(ref player_id) = old_player_id {
            info!("Kicking existing player '{}' ({})", name, player_id);

            self.send_direct_message(
                player_id,
                ServerMessage::Kicked {
                    player_id: player_id.clone(),
                    reason: "Another session logged in with the same account".to_string(),
                },
            )
            .await;

            self.remove_player(player_id).await;
        }

        old_player_id
    }

    pub async fn add_player(&self, player: Player) -> Option<ServerMessage> {
        let player_id = player.id.clone();
        let player_name = player.name.clone();
        let player_number = self.get_or_assign_player_number(&player_id).await;

        {
            let mut players = self.players.write().await;
            players.insert(player_id.clone(), player.clone());
        }

        info!(
            "Player {} ({}) joined the game [#{}]",
            player_name, player_id, player_number
        );

        self.broadcast(ServerMessage::PlayerJoined { player }, None);

        // Return game_state to be sent directly to the new player only
        let current_players = self.players.read().await;
        let other_players: HashMap<String, Player> = current_players
            .iter()
            .filter(|(id, _)| *id != &player_id)
            .map(|(id, player)| (id.clone(), player.clone()))
            .collect();

        let monsters = self.monsters.read().await.clone();

        if !other_players.is_empty() || !monsters.is_empty() {
            return Some(ServerMessage::GameState {
                players: other_players,
                monsters,
            });
        }

        None
    }

    pub async fn remove_player(&self, player_id: &PlayerId) {
        self.remove_monsters_by_owner(player_id).await;

        let removed_player_number = {
            let mut id_state = self.id_state.write().await;
            let removed = id_state.player_numbers.remove(player_id);
            if let Some(player_number) = removed {
                id_state.owner_spawn_counts.remove(&player_number);
            }
            removed
        };

        let mut players = self.players.write().await;

        if let Some(player) = players.remove(player_id) {
            info!(
                "Player {} ({}) left the game{}",
                player.name,
                player_id,
                removed_player_number
                    .map(|n| format!(" [#{}]", n))
                    .unwrap_or_default()
            );
            self.broadcast(
                ServerMessage::PlayerLeft {
                    player_id: player_id.clone(),
                },
                None,
            );
        } else {
            warn!("Attempted to remove non-existent player: {}", player_id);
        }
    }

    pub async fn update_player_position(
        &self,
        player_id: &PlayerId,
        new_position: Position,
        new_rotation: f32,
        floor_level: i8,
    ) {
        let mut players = self.players.write().await;

        if let Some(player) = players.get_mut(player_id) {
            player.position = new_position.clone();
            player.rotation = new_rotation;
            player.floor_level = floor_level;
            drop(players);
            self.mark_dirty(player_id).await;
            self.broadcast(
                ServerMessage::PlayerMoved {
                    player_id: player_id.clone(),
                    position: new_position,
                    rotation: new_rotation,
                },
                None,
            );
        } else {
            warn!("Attempted to move non-existent player: {}", player_id);
        }
    }

    pub async fn teleport_player(
        &self,
        player_id: &PlayerId,
        new_position: Position,
        new_rotation: f32,
    ) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.position = new_position.clone();
            player.rotation = new_rotation;
            drop(players);
            self.mark_dirty(player_id).await;
            self.broadcast(
                ServerMessage::PlayerTeleported {
                    player_id: player_id.clone(),
                    position: new_position,
                    rotation: new_rotation,
                },
                None,
            );
        }
    }

    pub async fn respawn_player(&self, player_id: &PlayerId) {
        let respawned_player = {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                if player.health > 0 {
                    info!(
                        "Ignored respawn request for alive player {} ({}) HP: {}/{}",
                        player.name, player.id, player.health, player.max_health
                    );
                    return;
                }
                player.health = player.max_health;
                let spawn = &world_config().spawn_position;
                player.position = Position {
                    x: spawn.x,
                    y: spawn.y,
                    z: spawn.z,
                };
                player.rotation = spawn.rotation;
                Some(player.clone())
            } else {
                None
            }
        };

        if let Some(player) = respawned_player {
            info!("Player {} ({}) respawned", player.name, player.id);
            self.mark_dirty(player_id).await;
            self.broadcast(ServerMessage::PlayerRespawned { player }, None);
        } else {
            warn!("Attempted to respawn non-existent player: {}", player_id);
        }
    }

    pub async fn get_player_position(&self, player_id: &PlayerId) -> Option<(Position, f32)> {
        let players = self.players.read().await;
        players
            .get(player_id)
            .map(|p| (p.position.clone(), p.rotation))
    }

    pub async fn toggle_player_torch(&self, player_id: &PlayerId, enabled: bool) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.torch_on = enabled;
            self.broadcast(
                ServerMessage::PlayerTorchToggled {
                    player_id: player_id.clone(),
                    enabled,
                },
                None,
            );
        }
    }

    pub async fn mark_dirty(&self, player_id: &PlayerId) {
        let mut dirty = self.dirty_players.write().await;
        dirty.insert(player_id.clone());
    }

    pub async fn remove_dirty(&self, player_id: &PlayerId) {
        let mut dirty = self.dirty_players.write().await;
        dirty.remove(player_id);
    }

    pub async fn collect_dirty_character_states(&self) -> Vec<CharacterSaveData> {
        let dirty_ids: Vec<PlayerId> = {
            let mut dirty = self.dirty_players.write().await;
            dirty.drain().collect()
        };

        if dirty_ids.is_empty() {
            return Vec::new();
        }

        let players = self.players.read().await;
        let player_chars = self.player_characters.read().await;

        let mut result = Vec::with_capacity(dirty_ids.len());
        for pid in &dirty_ids {
            if let (Some(player), Some((char_id, xp, _))) =
                (players.get(pid), player_chars.get(pid))
            {
                result.push(build_save_data(player, *char_id, *xp));
            }
        }

        result
    }

    pub async fn get_player_save_data(&self, player_id: &PlayerId) -> Option<CharacterSaveData> {
        let players = self.players.read().await;
        let player_chars = self.player_characters.read().await;

        let player = players.get(player_id)?;
        let (char_id, xp, _) = player_chars.get(player_id)?;

        Some(build_save_data(player, *char_id, *xp))
    }

    #[allow(dead_code)]
    pub async fn get_player_count(&self) -> usize {
        self.players.read().await.len()
    }

    #[allow(dead_code)]
    pub async fn get_all_players(&self) -> HashMap<PlayerId, Player> {
        self.players.read().await.clone()
    }
}
