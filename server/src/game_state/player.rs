use crate::auth::CharacterSaveData;
use crate::types::{CharacterAttributes, Player, PlayerId, Position, ServerMessage};
use crate::world_config::world_config;
use std::collections::{HashMap, HashSet};
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

    pub async fn send_direct_message_to_players(
        &self,
        player_ids: &[PlayerId],
        msg: ServerMessage,
    ) {
        self.send_direct_message_to_players_except(player_ids, msg, None)
            .await;
    }

    pub async fn send_direct_message_to_players_except(
        &self,
        player_ids: &[PlayerId],
        msg: ServerMessage,
        skip_player_id: Option<&PlayerId>,
    ) {
        let channels = self.direct_channels.read().await;
        for player_id in player_ids {
            if skip_player_id.is_some_and(|skip_id| skip_id == player_id) {
                continue;
            }
            if let Some(tx) = channels.get(player_id) {
                let _ = tx.send(msg.clone());
            }
        }
    }

    pub async fn send_direct_message_to_players_within_position(
        &self,
        position: &Position,
        radius: f32,
        msg: ServerMessage,
        skip_player_id: Option<&PlayerId>,
    ) {
        let player_ids = self.player_ids_within_position(position, radius).await;
        self.send_direct_message_to_players_except(&player_ids, msg, skip_player_id)
            .await;
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
        let player_position = player.position.clone();

        {
            let mut players = self.players.write().await;
            players.insert(player_id.clone(), player.clone());
        }
        self.insert_player_spatial_cell(&player_id, &player_position)
            .await;

        info!(
            "Player {} ({}) joined the game [#{}]",
            player_name, player_id, player_number
        );

        let nearby_player_ids = self
            .player_ids_within(&player_id, super::AGENT_EVENT_DELIVERY_RADIUS)
            .await;
        let nearby_player_set: HashSet<_> = nearby_player_ids.iter().cloned().collect();
        self.send_direct_message_to_players_except(
            &nearby_player_ids,
            ServerMessage::PlayerJoined {
                player: player.clone(),
            },
            Some(&player_id),
        )
        .await;

        // Return visible game_state to be sent directly to the new player only
        let current_players = self.players.read().await;
        let other_players: HashMap<String, Player> = current_players
            .iter()
            .filter(|(id, _)| nearby_player_set.contains(*id) && *id != &player_id)
            .map(|(id, player)| (id.clone(), player.clone()))
            .collect();

        let monsters: HashMap<String, crate::types::Monster> = self
            .monsters
            .read()
            .await
            .iter()
            .filter(|(_, monster)| {
                monster.position.dist_xz_sq(&player_position)
                    <= super::AGENT_EVENT_DELIVERY_RADIUS * super::AGENT_EVENT_DELIVERY_RADIUS
            })
            .map(|(id, monster)| (id.clone(), monster.clone()))
            .collect();
        let ground_items: Vec<_> = self
            .ground_items
            .read()
            .await
            .values()
            .filter(|sgi| {
                sgi.item.position.dist_xz_sq(&player_position)
                    <= super::AGENT_EVENT_DELIVERY_RADIUS * super::AGENT_EVENT_DELIVERY_RADIUS
            })
            .map(|sgi| sgi.item.clone())
            .collect();

        if !other_players.is_empty() || !monsters.is_empty() || !ground_items.is_empty() {
            return Some(ServerMessage::GameState {
                players: other_players,
                monsters,
                ground_items,
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

        let nearby_player_ids = self
            .player_ids_within(player_id, super::AGENT_EVENT_DELIVERY_RADIUS)
            .await;
        let removed_player = {
            let mut players = self.players.write().await;
            players.remove(player_id)
        };

        if let Some(player) = removed_player {
            self.remove_player_spatial_cell(player_id, &player.position)
                .await;
            info!(
                "Player {} ({}) left the game{}",
                player.name,
                player_id,
                removed_player_number
                    .map(|n| format!(" [#{}]", n))
                    .unwrap_or_default()
            );
            self.send_direct_message_to_players_except(
                &nearby_player_ids,
                ServerMessage::PlayerLeft {
                    player_id: player_id.clone(),
                },
                Some(player_id),
            )
            .await;
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
        let (old_position, moved_player) = {
            let mut players = self.players.write().await;

            let Some(player) = players.get_mut(player_id) else {
                warn!("Attempted to move non-existent player: {}", player_id);
                return;
            };

            let old_position = player.position.clone();
            player.position = new_position.clone();
            player.rotation = new_rotation;
            player.floor_level = floor_level;
            (old_position, player.clone())
        };

        self.move_player_spatial_cell(player_id, &old_position, &new_position)
            .await;
        self.mark_dirty(player_id).await;
        self.fanout_player_position_update(
            player_id,
            &old_position,
            &moved_player,
            ServerMessage::PlayerMoved {
                player_id: player_id.clone(),
                position: new_position,
                rotation: new_rotation,
                floor_level,
            },
        )
        .await;
    }

    pub async fn teleport_player(
        &self,
        player_id: &PlayerId,
        new_position: Position,
        new_rotation: f32,
    ) {
        let moved = {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                let old_position = player.position.clone();
                player.position = new_position.clone();
                player.rotation = new_rotation;
                Some((old_position, player.clone()))
            } else {
                None
            }
        };

        if let Some((old_position, moved_player)) = moved {
            self.move_player_spatial_cell(player_id, &old_position, &new_position)
                .await;
            self.mark_dirty(player_id).await;
            self.fanout_player_position_update(
                player_id,
                &old_position,
                &moved_player,
                ServerMessage::PlayerTeleported {
                    player_id: player_id.clone(),
                    position: new_position,
                    rotation: new_rotation,
                },
            )
            .await;
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
                let old_position = player.position.clone();
                let spawn = &world_config().spawn_position;
                player.position = Position {
                    x: spawn.x,
                    y: spawn.y,
                    z: spawn.z,
                };
                player.rotation = spawn.rotation;
                Some((old_position, player.clone()))
            } else {
                None
            }
        };

        if let Some((old_position, player)) = respawned_player {
            info!("Player {} ({}) respawned", player.name, player.id);
            self.move_player_spatial_cell(player_id, &old_position, &player.position)
                .await;
            self.mark_dirty(player_id).await;
            self.fanout_player_position_update(
                player_id,
                &old_position,
                &player,
                ServerMessage::PlayerRespawned {
                    player: player.clone(),
                },
            )
            .await;
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
        let position = {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                player.torch_on = enabled;
                Some(player.position)
            } else {
                None
            }
        };

        if let Some(position) = position {
            self.send_direct_message_to_players_within_position(
                &position,
                super::AGENT_EVENT_DELIVERY_RADIUS,
                ServerMessage::PlayerTorchToggled {
                    player_id: player_id.clone(),
                    enabled,
                },
                None,
            )
            .await;
        }
    }

    pub async fn set_player_interaction(
        &self,
        player_id: &PlayerId,
        object_type: Option<String>,
        object_id: Option<u32>,
    ) {
        let rejected_or_position = {
            let mut players = self.players.write().await;

            // Reject if the specific object is already occupied
            if object_id.is_some_and(|fid| {
                players
                    .values()
                    .any(|p| p.id != *player_id && p.object_id == Some(fid))
            }) {
                Err(())
            } else if let Some(player) = players.get_mut(player_id) {
                player.object_type = object_type.clone();
                player.object_id = object_id;
                Ok(Some(player.position))
            } else {
                Ok(None)
            }
        };

        if rejected_or_position.is_err() {
            self.send_direct_message(
                player_id,
                ServerMessage::InteractionRejected {
                    reason: "occupied".to_string(),
                },
            )
            .await;
        } else if let Ok(Some(position)) = rejected_or_position {
            self.send_direct_message_to_players_within_position(
                &position,
                super::AGENT_EVENT_DELIVERY_RADIUS,
                ServerMessage::PlayerInteractionChanged {
                    player_id: player_id.clone(),
                    object_type,
                },
                None,
            )
            .await;
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

    async fn insert_player_spatial_cell(&self, player_id: &PlayerId, position: &Position) {
        let mut cells = self.player_spatial_cells.write().await;
        cells
            .entry(super::SpatialCell::from_position(position))
            .or_default()
            .insert(player_id.clone());
    }

    async fn remove_player_spatial_cell(&self, player_id: &PlayerId, position: &Position) {
        let cell = super::SpatialCell::from_position(position);
        let mut cells = self.player_spatial_cells.write().await;
        let should_remove = if let Some(player_ids) = cells.get_mut(&cell) {
            player_ids.remove(player_id);
            player_ids.is_empty()
        } else {
            false
        };

        if should_remove {
            cells.remove(&cell);
        }
    }

    async fn move_player_spatial_cell(
        &self,
        player_id: &PlayerId,
        old_position: &Position,
        new_position: &Position,
    ) {
        let old_cell = super::SpatialCell::from_position(old_position);
        let new_cell = super::SpatialCell::from_position(new_position);
        if old_cell == new_cell {
            return;
        }

        let mut cells = self.player_spatial_cells.write().await;
        let should_remove_old = if let Some(player_ids) = cells.get_mut(&old_cell) {
            player_ids.remove(player_id);
            player_ids.is_empty()
        } else {
            false
        };

        if should_remove_old {
            cells.remove(&old_cell);
        }

        cells.entry(new_cell).or_default().insert(player_id.clone());
    }

    async fn fanout_player_position_update(
        &self,
        player_id: &PlayerId,
        old_position: &Position,
        player: &Player,
        update_msg: ServerMessage,
    ) {
        let old_visible: HashSet<PlayerId> = self
            .player_ids_within_position(old_position, super::AGENT_EVENT_DELIVERY_RADIUS)
            .await
            .into_iter()
            .filter(|id| id != player_id)
            .collect();
        let new_visible: HashSet<PlayerId> = self
            .player_ids_within_position(&player.position, super::AGENT_EVENT_DELIVERY_RADIUS)
            .await
            .into_iter()
            .filter(|id| id != player_id)
            .collect();

        let left: Vec<_> = old_visible.difference(&new_visible).cloned().collect();
        let entered: Vec<_> = new_visible.difference(&old_visible).cloned().collect();
        let stayed: Vec<_> = new_visible.intersection(&old_visible).cloned().collect();

        for other_id in &left {
            self.send_direct_message(
                player_id,
                ServerMessage::PlayerDisappeared {
                    player_id: other_id.clone(),
                },
            )
            .await;
            self.send_direct_message(
                other_id,
                ServerMessage::PlayerDisappeared {
                    player_id: player_id.clone(),
                },
            )
            .await;
        }

        let entered_players = {
            let players = self.players.read().await;
            entered
                .iter()
                .filter_map(|id| players.get(id).cloned())
                .collect::<Vec<_>>()
        };

        for other in entered_players {
            self.send_direct_message(
                player_id,
                ServerMessage::PlayerAppeared {
                    player: other.clone(),
                },
            )
            .await;
            self.send_direct_message(
                &other.id,
                ServerMessage::PlayerAppeared {
                    player: player.clone(),
                },
            )
            .await;
        }

        let (monsters_left, monsters_entered) = {
            let monsters = self.monsters.read().await;
            let radius_sq = super::AGENT_EVENT_DELIVERY_RADIUS * super::AGENT_EVENT_DELIVERY_RADIUS;
            let old_monsters: HashSet<_> = monsters
                .iter()
                .filter(|(_, monster)| old_position.dist_xz_sq(&monster.position) <= radius_sq)
                .map(|(id, _)| id.clone())
                .collect();
            let new_monsters: HashSet<_> = monsters
                .iter()
                .filter(|(_, monster)| player.position.dist_xz_sq(&monster.position) <= radius_sq)
                .map(|(id, _)| id.clone())
                .collect();

            let left = old_monsters
                .difference(&new_monsters)
                .cloned()
                .collect::<Vec<_>>();
            let entered = new_monsters
                .difference(&old_monsters)
                .filter_map(|id| monsters.get(id).cloned())
                .collect::<Vec<_>>();
            (left, entered)
        };

        for monster_id in monsters_left {
            self.send_direct_message(player_id, ServerMessage::MonsterRemoved { monster_id })
                .await;
        }
        for monster in monsters_entered {
            self.send_direct_message(player_id, ServerMessage::MonsterSpawned { monster })
                .await;
        }

        let (items_left, items_entered) = {
            let ground_items = self.ground_items.read().await;
            let radius_sq = super::AGENT_EVENT_DELIVERY_RADIUS * super::AGENT_EVENT_DELIVERY_RADIUS;
            let old_items: HashSet<_> = ground_items
                .iter()
                .filter(|(_, sgi)| old_position.dist_xz_sq(&sgi.item.position) <= radius_sq)
                .map(|(id, _)| *id)
                .collect();
            let new_items: HashSet<_> = ground_items
                .iter()
                .filter(|(_, sgi)| player.position.dist_xz_sq(&sgi.item.position) <= radius_sq)
                .map(|(id, _)| *id)
                .collect();

            let left = old_items
                .difference(&new_items)
                .copied()
                .collect::<Vec<_>>();
            let entered = new_items
                .difference(&old_items)
                .filter_map(|id| ground_items.get(id).map(|sgi| sgi.item.clone()))
                .collect::<Vec<_>>();
            (left, entered)
        };

        for instance_id in items_left {
            self.send_direct_message(player_id, ServerMessage::GroundItemRemoved { instance_id })
                .await;
        }
        for item in items_entered {
            self.send_direct_message(player_id, ServerMessage::GroundItemAppeared { item })
                .await;
        }

        self.send_direct_message(player_id, update_msg.clone())
            .await;
        self.send_direct_message_to_players(&stayed, update_msg)
            .await;
    }

    pub async fn player_ids_within_position(
        &self,
        position: &Position,
        radius: f32,
    ) -> Vec<PlayerId> {
        let center_cell = super::SpatialCell::from_position(position);
        let cell_radius = (radius / super::PLAYER_SPATIAL_CELL_SIZE).ceil() as i32;
        let radius_sq = radius * radius;
        let players = self.players.read().await;
        let cells = self.player_spatial_cells.read().await;
        let mut player_ids = Vec::new();

        for cell_x in (center_cell.x - cell_radius)..=(center_cell.x + cell_radius) {
            for cell_z in (center_cell.z - cell_radius)..=(center_cell.z + cell_radius) {
                let cell = super::SpatialCell {
                    x: cell_x,
                    z: cell_z,
                };

                let Some(cell_player_ids) = cells.get(&cell) else {
                    continue;
                };

                for player_id in cell_player_ids {
                    let Some(player) = players.get(player_id) else {
                        continue;
                    };

                    if position.dist_xz_sq(&player.position) <= radius_sq {
                        player_ids.push(player_id.clone());
                    }
                }
            }
        }

        player_ids
    }

    pub async fn player_ids_within(&self, player_id: &PlayerId, radius: f32) -> Vec<PlayerId> {
        let position = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                return Vec::new();
            };
            player.position
        };

        self.player_ids_within_position(&position, radius).await
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
