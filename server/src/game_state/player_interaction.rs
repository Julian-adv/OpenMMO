use super::{estate_storage, passability::regions_around, GameState};
use crate::types::{Player, PlayerId, Position, ServerMessage};
use onlinerpg_shared::estate_storage::{
    estate_storage_def, is_estate_storage_item, INTERACTION_RANGE,
};
use onlinerpg_shared::furniture::{self, FurniturePlacement};
use onlinerpg_shared::pathfinding::{self, PassabilityCache};
use onlinerpg_shared::{shortest_world_delta_x, wrap_world_x};

pub(super) type FurnitureIndex = std::collections::HashMap<(i32, i32), Vec<FurniturePlacement>>;

fn placement_position(placement: &FurniturePlacement) -> Position {
    Position {
        x: wrap_world_x(placement.x),
        y: placement.y,
        z: placement.z,
    }
}

pub(super) fn interaction_message(player: &Player) -> ServerMessage {
    ServerMessage::PlayerInteractionChanged {
        player_id: player.id,
        object_type: player.object_type.clone(),
        object_id: player.object_id,
        position: player.position,
        rotation: player.rotation,
        floor_level: player.floor_level,
    }
}

fn clear_segment(cache: &PassabilityCache, from: Position, to: Position, floor: u8) -> bool {
    let dx = shortest_world_delta_x(from.x, to.x);
    let dz = to.z - from.z;
    if pathfinding::is_movement_blocked(
        cache,
        from.x,
        from.z,
        from.x + dx,
        to.z,
        floor,
        Some(from.y),
    ) {
        return false;
    }
    let steps = (dx.hypot(dz) / 0.1).ceil().max(1.0) as usize;
    (1..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        !pathfinding::is_circle_blocked_by_passability(
            cache
                .values()
                .filter(|entry| !entry.yields_to_trapped_mover),
            wrap_world_x(from.x + dx * t),
            from.z + dz * t,
            0.31,
            floor,
            Some(from.y + (to.y - from.y) * t),
        )
    })
}

impl GameState {
    pub(super) async fn interaction_placement(
        &self,
        player: &Player,
        kind: &str,
        id: u32,
    ) -> Option<(FurniturePlacement, PassabilityCache)> {
        let (placement, key, others) = if let Some(definition) = estate_storage_def(kind) {
            if !matches!(definition.model_id.as_str(), "chair" | "bed" | "rustic_bed") {
                return None;
            }
            let chests = self.estate_chests.read().await;
            let chest = chests
                .get(i64::from(id))
                .filter(|c| c.item_def_id == kind)?;
            let bucket = estate_storage::EstateChestIndex::bucket(&chest.position);
            (
                estate_storage::placement(chest),
                estate_storage::cache_key(bucket),
                chests
                    .group(bucket)
                    .iter()
                    .filter(|c| c.id != chest.id)
                    .map(estate_storage::placement)
                    .collect::<Vec<_>>(),
            )
        } else {
            if !matches!(kind, "chair" | "bed" | "rustic_bed") {
                return None;
            }
            let index = self
                .interaction_furniture
                .read()
                .unwrap_or_else(|e| e.into_inner());
            let (region, placement) = regions_around(player.position.x, player.position.z)
                .filter_map(|key| index.get(&key).map(|group| (key, group)))
                .flat_map(|(key, group)| group.iter().map(move |p| (key, p)))
                .filter(|(_, p)| {
                    p.id == id && p.type_id == kind && p.floor_level as i8 == player.floor_level
                })
                .min_by(|(_, a), (_, b)| {
                    player
                        .position
                        .dist_xz_sq(&placement_position(a))
                        .total_cmp(&player.position.dist_xz_sq(&placement_position(b)))
                })?;
            let others = index[&region]
                .iter()
                .filter(|p| p.id != id)
                .cloned()
                .collect();
            (
                placement.clone(),
                furniture::region_cache_key(region.0, region.1),
                others,
            )
        };
        let mut cache = (*self.passability.snapshot()).clone();
        cache.remove(&key);
        if let Some(rest) = furniture::build_furniture_passability_for_placements(&others) {
            cache.insert(key, rest);
        }
        Some((placement, cache))
    }

    async fn interaction_exit(&self, player: &Player) -> Position {
        let cache = match (player.object_type.as_deref(), player.object_id) {
            (Some(kind), Some(id)) => self
                .interaction_placement(player, kind, id)
                .await
                .map(|(_, cache)| cache),
            _ => return player.position,
        }
        .unwrap_or_else(|| (*self.passability.snapshot()).clone());
        let floor = onlinerpg_shared::dungeon::passability_floor_for_level(player.floor_level);
        for distance in [1.0, 1.5, 2.0] {
            for angle in [
                std::f32::consts::FRAC_PI_2,
                -std::f32::consts::FRAC_PI_2,
                0.0,
                std::f32::consts::PI,
            ] {
                let rotation = player.rotation + angle;
                let mut position = Position {
                    x: wrap_world_x(player.position.x + rotation.sin() * distance),
                    y: player.position.y,
                    z: player.position.z + rotation.cos() * distance,
                };
                position.y = self
                    .surface_ground_y(floor, &position, player.position.y, None)
                    .await;
                if (position.y - player.position.y).abs() > 0.75
                    || (floor > 0
                        && pathfinding::storey_ground_y(
                            &cache, floor, position.x, position.z, position.y,
                        )
                        .is_none())
                    || !clear_segment(&cache, player.position, position, floor)
                    || pathfinding::is_circle_blocked_on_floor(
                        &self.passability_read(),
                        position.x,
                        position.z,
                        0.31,
                        floor,
                        Some(position.y),
                    )
                {
                    continue;
                }
                return position;
            }
        }
        player.position
    }

    pub async fn set_player_interaction(
        &self,
        player_id: &PlayerId,
        object_type: Option<String>,
        object_id: Option<u32>,
    ) {
        let Some(mut movement) = self
            .lock_player_movement(*player_id, &[], INTERACTION_RANGE + 0.5, true, None)
            .await
        else {
            return;
        };
        let previous = movement.player;
        let furniture_change = object_id.is_some() || previous.object_id.is_some();
        let mut position = previous.position;
        let mut rotation = previous.rotation;
        let mut rejection = None;
        if let (Some(kind), Some(id)) = (object_type.as_deref(), object_id) {
            match self.interaction_placement(&previous, kind, id).await {
                Some((placement, cache)) => {
                    position = placement_position(&placement);
                    let floor = placement.floor_level;
                    position.y = self
                        .surface_ground_y(floor, &position, position.y, None)
                        .await;
                    rotation = placement.rotation_deg.to_radians();
                    if previous.health == 0 || previous.is_mounted() {
                        rejection = Some("You cannot use furniture right now.");
                    } else if previous.floor_level != floor as i8
                        || previous.position.dist_xz_sq(&position)
                            > (INTERACTION_RANGE + 0.5).powi(2)
                        || (previous.position.y - position.y).abs() > 2.0
                    {
                        rejection = Some("Move closer to the furniture.");
                    } else if !clear_segment(&cache, previous.position, position, floor) {
                        rejection = Some("Something blocks the furniture.");
                    }
                }
                None => rejection = Some("That furniture is no longer available."),
            }
            if self.players.read().await.values().any(|p| {
                p.id != *player_id
                    && p.object_id == object_id
                    && p.object_type.as_deref().is_some_and(is_estate_storage_item)
                        == is_estate_storage_item(kind)
                    && p.floor_level == previous.floor_level
                    && p.position.dist_xz_sq(&position) <= (INTERACTION_RANGE + 0.5).powi(2)
            }) {
                rejection = Some("occupied");
            }
        } else if previous.object_id.is_some() {
            position = self.interaction_exit(&previous).await;
        }
        if let Some(reason) = rejection {
            self.send_direct_message(
                player_id,
                ServerMessage::InteractionRejected {
                    reason: reason.into(),
                },
            )
            .await;
            self.send_direct_message(player_id, interaction_message(&previous))
                .await;
            return;
        }
        if object_type.is_some() || previous.object_id.is_some() {
            self.cancel_goal_movement_locked(player_id, &mut movement.state)
                .await;
        }
        let player = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            if furniture_change {
                player.position = position;
                player.rotation = rotation;
            }
            player.object_type = object_type.clone();
            player.object_id = object_id;
            self.update_bed_rest(player).await;
            player.clone()
        };
        drop(movement.regions);
        if object_type.as_deref() != Some(onlinerpg_shared::messages::MUSIC_EMOTE) {
            self.music_performances.write().await.remove(player_id);
            self.remove_live_instrument(player_id).await;
        }
        let message = interaction_message(&player);
        if furniture_change {
            self.finish_position_update(
                player_id,
                previous.position,
                previous.floor_level,
                player,
                message,
            )
            .await;
        } else {
            self.publish_nearby(&player.position, player.floor_level, message, None)
                .await;
        }
    }
}
