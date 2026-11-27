//! Waking on an estate's aid cot instead of the inn (doc/RESPAWN_POINT.md).

use super::{auth_db, localization::localized, GameState};
use crate::auth::{AuthService, OwnShelters, ShelterSite};
use crate::types::{PlayerId, Position, ServerMessage};
use onlinerpg_shared::respawn::{NearbyShelter, RespawnPlace, ShelterAccess, AID_COT};
use std::collections::HashMap;

/// Where a defeated player wakes on an estate: lying on `cot`, or standing
/// when every cot is taken.
#[derive(Debug, Clone, Copy)]
pub(super) struct ShelterSpot {
    pub position: Position,
    pub rotation: f32,
    pub floor_level: i8,
    pub cot: Option<u32>,
}

impl GameState {
    /// Public shelters nearer to `position` than the inn, nearest first.
    async fn public_shelters_near(&self, position: &Position) -> Vec<(i64, f32)> {
        let inn = position.dist_xz_sq(&crate::world_config::world_config().respawn.position());
        let mut nearest: HashMap<i64, f32> = HashMap::new();
        for cot in self.estate_chests.read().await.aid_cots() {
            if cot.shelter != Some(ShelterAccess::Public) || cot.floor_level < 0 {
                continue;
            }
            let distance = position.dist_xz_sq(&cot.position);
            if distance < inn {
                let entry = nearest.entry(cot.estate_id).or_insert(distance);
                *entry = entry.min(distance);
            }
        }
        let mut nearest: Vec<(i64, f32)> = nearest
            .into_iter()
            .map(|(id, distance)| (id, distance.sqrt()))
            .collect();
        nearest.sort_by(|a, b| a.1.total_cmp(&b.1));
        nearest
    }

    /// A public shelter's name and cots, unless it closed or fell overdue.
    async fn open_public_shelter(
        &self,
        estate_id: i64,
        distance_m: f32,
        auth: &AuthService,
    ) -> Option<(NearbyShelter, ShelterSite)> {
        let auth = auth.clone();
        match auth_db(move || auth.public_shelter(estate_id)).await {
            Ok(found) => found.map(|(name, site)| {
                (
                    NearbyShelter {
                        estate_id,
                        name,
                        distance_m,
                    },
                    site,
                )
            }),
            Err(error) => {
                tracing::warn!(%error, estate_id, "Failed to load a public shelter");
                None
            }
        }
    }

    async fn own_shelters(&self, character_id: i64, auth: &AuthService) -> OwnShelters {
        let auth = auth.clone();
        auth_db(move || auth.own_shelters(character_id))
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Failed to load respawn shelters");
                OwnShelters::default()
            })
    }

    async fn defeated_pose(&self, player_id: &PlayerId) -> Option<Position> {
        self.players
            .read()
            .await
            .get(player_id)
            .filter(|player| player.health == 0)
            .map(|player| player.position)
    }

    pub async fn send_respawn_options(&self, player_id: &PlayerId, auth: &AuthService) {
        let Some(position) = self.defeated_pose(player_id).await else {
            return;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let own = self.own_shelters(character_id, auth).await;
        let estate = self.has_safe_cot(own.estate.as_ref()).await;
        let family_seat = self.has_safe_cot(own.family_seat.as_ref()).await;
        let skip: Vec<i64> = own
            .estate
            .iter()
            .chain(own.family_seat.iter())
            .map(|site| site.estate_id)
            .collect();
        let mut nearby = None;
        for (estate_id, distance) in self
            .public_shelters_near(&position)
            .await
            .into_iter()
            .filter(|(id, _)| !skip.contains(id))
            .take(3)
        {
            if let Some((shelter, site)) = self.open_public_shelter(estate_id, distance, auth).await
            {
                if self.has_safe_cot(Some(&site)).await {
                    nearby = Some(shelter);
                    break;
                }
            }
        }
        self.send_direct_message(
            player_id,
            ServerMessage::RespawnOptions {
                estate,
                family_seat,
                nearby,
            },
        )
        .await;
    }

    /// Wake at `place`, or at the inn when that shelter has nowhere to lie.
    pub async fn respawn_player_at(
        &self,
        player_id: &PlayerId,
        place: RespawnPlace,
        auth: &AuthService,
    ) {
        if place == RespawnPlace::Town || self.official_npc_def(player_id).await.is_some() {
            self.respawn_player(player_id).await;
            return;
        }
        let Some(position) = self.defeated_pose(player_id).await else {
            return;
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let site = match place {
            RespawnPlace::Town => None,
            RespawnPlace::Estate => self.own_shelters(character_id, auth).await.estate,
            RespawnPlace::FamilySeat => self.own_shelters(character_id, auth).await.family_seat,
            RespawnPlace::Nearby { estate_id } => {
                let near = self
                    .public_shelters_near(&position)
                    .await
                    .into_iter()
                    .find(|(id, _)| *id == estate_id);
                match near {
                    Some((_, distance)) => self
                        .open_public_shelter(estate_id, distance, auth)
                        .await
                        .map(|(_, site)| site),
                    None => None,
                }
            }
        };
        let spot = match &site {
            Some(site) => self.shelter_spot(site).await,
            None => None,
        };
        if spot.is_none() {
            self.send_system_message(
                player_id,
                localized(
                    "server.shelterClosed",
                    "That aid cot is no longer open to you, so you woke at the inn.",
                ),
            )
            .await;
        }
        self.respawn_player_with(player_id, spot).await;
    }

    async fn has_safe_cot(&self, site: Option<&ShelterSite>) -> bool {
        let Some(site) = site else {
            return false;
        };
        for (_, position, _, floor_level) in &site.cots {
            if self
                .estate_floor_y(&site.plots, position, *floor_level)
                .await
                .is_some()
            {
                return true;
            }
        }
        false
    }

    /// A free safe aid cot on the site, else the estate's safe ground, else
    /// the first safe cot.
    pub(super) async fn shelter_spot(&self, site: &ShelterSite) -> Option<ShelterSpot> {
        let mut safe = Vec::new();
        for &(id, position, rotation_deg, floor_level) in &site.cots {
            if self
                .estate_floor_y(&site.plots, &position, floor_level)
                .await
                .is_some()
            {
                safe.push((id, position, rotation_deg, floor_level));
            }
        }
        let first = *safe.first()?;
        let taken: Vec<u32> = self
            .players
            .read()
            .await
            .values()
            .filter(|p| p.object_type.as_deref() == Some(AID_COT))
            .filter_map(|p| p.object_id)
            .collect();
        let spot = |(id, position, rotation_deg, floor_level): (i64, Position, f32, i8),
                    cot: bool| ShelterSpot {
            position,
            rotation: rotation_deg.to_radians(),
            floor_level,
            cot: cot.then_some(id as u32),
        };
        if let Some(&free) = safe.iter().find(|(id, ..)| !taken.contains(&(*id as u32))) {
            return Some(spot(free, true));
        }
        Some(match self.estate_return_position(&site.plots).await {
            Some(position) => ShelterSpot {
                position,
                rotation: first.2.to_radians(),
                floor_level: 0,
                cot: None,
            },
            None => spot(first, false),
        })
    }

    pub async fn set_shelter_access(
        &self,
        player_id: &PlayerId,
        furniture_id: i64,
        access: ShelterAccess,
        auth: &AuthService,
    ) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth.clone();
        let estate_id = match auth_db(move || {
            auth.set_shelter_access(character_id, furniture_id, access)
        })
        .await
        {
            Ok(Ok(estate_id)) => estate_id,
            Ok(Err(reason)) => {
                self.send_system_message(player_id, localized("server.shelterOwnerOnly", reason))
                    .await;
                return;
            }
            Err(error) => {
                tracing::warn!(%error, "Failed to save shelter access");
                self.send_system_message(player_id, "The setting could not be saved.")
                    .await;
                return;
            }
        };
        self.update_estate_shelter(estate_id, access).await;
    }

    pub(super) async fn update_estate_shelter(&self, estate_id: i64, access: ShelterAccess) {
        let added = self
            .estate_chests
            .write()
            .await
            .set_shelter(estate_id, access);
        if !added.is_empty() {
            self.publish_subject_change(ServerMessage::EstateChestVisibility {
                added,
                removed: vec![],
            });
        }
    }
}
