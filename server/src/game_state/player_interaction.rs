use super::{dance::Stand, estate_storage, passability::regions_around, GameState};
use crate::types::{Player, PlayerId, Position, ServerMessage};
use onlinerpg_shared::estate_storage::{
    estate_storage_def, is_estate_storage_item, INTERACTION_RANGE,
};
use onlinerpg_shared::furniture::{self, FurniturePlacement};
use onlinerpg_shared::occupancy::BODY_RADIUS_M as BODY_RADIUS;
use onlinerpg_shared::pathfinding::{self, PassabilityCache};
use onlinerpg_shared::{shortest_world_delta_x, wrap_world_x, WORLD_WIDTH_X};
use std::{collections::HashMap, sync::OnceLock};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogInteraction {
    id: String,
    interaction: Option<String>,
    interact_offset: Option<Position>,
}

#[derive(Clone, Copy)]
pub(super) struct InteractionPose {
    pub sleep: bool,
    pub offset: Position,
}

pub(super) fn interaction_pose(kind: &str) -> Option<InteractionPose> {
    static POSES: OnceLock<HashMap<String, InteractionPose>> = OnceLock::new();
    let model = estate_storage_def(kind).map_or(kind, |definition| definition.model_id.as_str());
    POSES
        .get_or_init(|| {
            let entries: Vec<CatalogInteraction> =
                serde_json::from_str(include_str!("../../../data/object_catalog.json"))
                    .expect("object_catalog.json is malformed");
            entries
                .into_iter()
                .filter_map(|entry| {
                    let sleep = match entry.interaction.as_deref()? {
                        "sit" => false,
                        "sleep" => true,
                        _ => return None,
                    };
                    let offset = entry.interact_offset.unwrap_or(Position {
                        x: 0.0,
                        y: 0.0,
                        z: 0.0,
                    });
                    Some((entry.id, InteractionPose { sleep, offset }))
                })
                .collect()
        })
        .get(model)
        .copied()
}

pub(super) type FurnitureIndex = std::collections::HashMap<(i32, i32), Vec<FurniturePlacement>>;

pub(super) fn placement_position(placement: &FurniturePlacement) -> Position {
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

pub(super) fn clear_segment(
    cache: &PassabilityCache,
    from: Position,
    to: Position,
    floor: u8,
) -> bool {
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
    let (lo_x, hi_x) = (
        from.x.min(from.x + dx) - BODY_RADIUS,
        from.x.max(from.x + dx) + BODY_RADIUS,
    );
    let (lo_z, hi_z) = (
        from.z.min(to.z) - BODY_RADIUS,
        from.z.max(to.z) + BODY_RADIUS,
    );
    let near: Vec<_> = cache
        .values()
        .filter(|entry| {
            !entry.yields_to_trapped_mover
                && entry.max_z >= lo_z
                && entry.min_z <= hi_z
                && [0.0, WORLD_WIDTH_X, -WORLD_WIDTH_X]
                    .iter()
                    .any(|shift| entry.max_x >= lo_x - shift && entry.min_x <= hi_x - shift)
        })
        .collect();
    if near.is_empty() {
        return true;
    }
    let steps = (dx.hypot(dz) / 0.1).ceil().max(1.0) as usize;
    (1..=steps).all(|step| {
        let t = step as f32 / steps as f32;
        !pathfinding::is_circle_blocked_by_passability(
            near.iter().copied(),
            wrap_world_x(from.x + dx * t),
            from.z + dz * t,
            BODY_RADIUS,
            floor,
            Some(from.y + (to.y - from.y) * t),
        )
    })
}

impl GameState {
    /// The placement `object_id` within the player's reach that `accept`
    /// takes, with its region.
    pub(super) async fn reachable_placement(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        accept: impl Fn(&FurniturePlacement) -> bool,
    ) -> Option<((i32, i32), FurniturePlacement)> {
        let players = self.players.read().await;
        let player = players.get(player_id)?;
        let index = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner());
        regions_around(player.position.x, player.position.z).find_map(|region| {
            index.get(&region)?.iter().find_map(|p| {
                (p.id == object_id
                    && accept(p)
                    && super::bed_rest::within_reach(
                        player,
                        placement_position(p),
                        p.floor_level as i8,
                    ))
                .then(|| (region, p.clone()))
            })
        })
    }

    pub(super) async fn interaction_placement(
        &self,
        player: &Player,
        kind: &str,
        id: u32,
    ) -> Option<(FurniturePlacement, PassabilityCache, Position)> {
        let offset = interaction_pose(kind)?.offset;
        let (placement, key, others) = if estate_storage_def(kind).is_some() {
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
        furniture::sync_passability(&mut cache, &key, &others);
        Some((placement, cache, offset))
    }

    /// Whether a player at `from` can walk to and stand at `to` on `floor`:
    /// a storey under it, no wall between, and room to stand.
    pub(super) fn can_step_to(
        &self,
        cache: &PassabilityCache,
        floor: u8,
        from: Position,
        to: Position,
    ) -> bool {
        (floor == 0 || pathfinding::storey_ground_y(cache, floor, to.x, to.z, to.y).is_some())
            && clear_segment(cache, from, to, floor)
            && !pathfinding::is_circle_blocked_on_floor(
                &self.passability_read(),
                to.x,
                to.z,
                BODY_RADIUS,
                floor,
                Some(to.y),
            )
    }

    pub(super) async fn interaction_exit(&self, player: &Player) -> Position {
        let cache = match (player.object_type.as_deref(), player.object_id) {
            (Some(kind), Some(id)) => self
                .interaction_placement(player, kind, id)
                .await
                .map(|(_, cache, _)| cache),
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
                    || !self.can_step_to(&cache, floor, player.position, position)
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
    ) -> bool {
        self.apply_player_interaction(player_id, object_type, object_id, None)
            .await
    }

    /// [`Self::set_player_interaction`], first moving the player to a
    /// `JoinDance` spot when `stand` is given. Returns whether it was applied.
    pub(super) async fn apply_player_interaction(
        &self,
        player_id: &PlayerId,
        object_type: Option<String>,
        object_id: Option<u32>,
        stand: Option<&Stand>,
    ) -> bool {
        if object_type.is_some() && stand.is_none() {
            self.bump_action_version(player_id).await;
        }
        let destinations = stand.map_or(&[][..], |stand| std::slice::from_ref(&stand.spot));
        let Some(mut movement) = self
            .lock_player_movement(
                *player_id,
                destinations,
                INTERACTION_RANGE + 0.5,
                true,
                None,
            )
            .await
        else {
            return false;
        };
        let previous = movement.player;
        if let Some(stand) = stand {
            if !self.stand_holds(stand, &previous).await {
                return false;
            }
            self.bump_action_version(player_id).await;
        }
        let furniture_change = object_id.is_some() || previous.object_id.is_some();
        let relocates = furniture_change || stand.is_some();
        let mut position = previous.position;
        let mut rotation = previous.rotation;
        let mut rejection = None;
        if let (Some(kind), Some(id)) = (object_type.as_deref(), object_id) {
            match self.interaction_placement(&previous, kind, id).await {
                Some((placement, cache, offset)) => {
                    position = placement_position(&placement);
                    let yaw = placement.rotation_deg.to_radians();
                    position.x =
                        wrap_world_x(position.x + offset.x * yaw.cos() + offset.z * yaw.sin());
                    position.z += -offset.x * yaw.sin() + offset.z * yaw.cos();
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
        } else if previous.object_id.is_some() && stand.is_none() {
            position = self.interaction_exit(&previous).await;
        }
        if let Some(stand) = stand {
            position = stand.spot;
            rotation = stand.rotation;
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
            self.resend_dance_beat(player_id).await;
            return false;
        }
        if object_type.is_some() || previous.object_id.is_some() {
            self.cancel_goal_movement_locked(player_id, &mut movement.state)
                .await;
        }
        // Before the new pose, so no join pairs it with the old beat.
        let keeps_beat = object_type
            .as_deref()
            .is_some_and(super::dance::is_looping_emote)
            && (stand.is_some() || object_type == previous.object_type);
        if !keeps_beat {
            self.end_dance_beat(player_id).await;
        }
        let player = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return false;
            };
            if relocates {
                player.position = position;
                player.rotation = rotation;
            }
            player.object_type = object_type.clone();
            player.object_id = object_id;
            self.update_bed_rest(player).await;
            player.clone()
        };
        drop(movement.regions);
        let performance_over =
            object_type.as_deref() != Some(onlinerpg_shared::messages::MUSIC_EMOTE);
        if performance_over {
            self.music_performances.write().await.remove(player_id);
            self.remove_live_instrument(player_id).await;
        }
        let message = interaction_message(&player);
        if relocates {
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
        drop(movement.state);
        if performance_over
            && previous.object_type.as_deref() == Some(onlinerpg_shared::messages::MUSIC_EMOTE)
        {
            self.interrupt_battle_hymn(player_id).await;
        }
        true
    }
}
