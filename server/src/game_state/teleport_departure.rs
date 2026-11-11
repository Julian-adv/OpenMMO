//! Every requested teleport (scrolls, accepted summons) waits out the same
//! departure on the server, so no client can skip it.

use super::GameState;
use crate::item_defs::{AuthenticatedUseAction, UseEffect};
use crate::types::PlayerId;
use onlinerpg_shared::TeleportPhase;
use std::collections::HashSet;
use std::future::Future;
use std::sync::MutexGuard;
use std::time::Duration;

/// Matches the web client's `TELEPORT_DEPARTURE_MS` animation.
pub(crate) const TELEPORT_DEPARTURE: Duration = Duration::from_millis(520);
const DEPARTURE_DRIFT: f32 = 0.1;

impl GameState {
    fn teleport_departures(&self) -> MutexGuard<'_, HashSet<PlayerId>> {
        self.teleport_departures
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Whether using this bag item teleports its reader.
    pub async fn teleports_on_use(&self, player_id: &PlayerId, instance_id: u64) -> bool {
        let inventories = self.inventories.read().await;
        inventories
            .get(player_id)
            .and_then(|inv| inv.bag.iter().find(|item| item.instance_id == instance_id))
            .and_then(|item| self.item_defs.get(&item.item_def_id))
            .is_some_and(|def| {
                matches!(
                    def.use_effect(),
                    Some(UseEffect::TeleportTown | UseEffect::TeleportRandom)
                ) || AuthenticatedUseAction::returns_to_estate(def.authenticated_use_action)
            })
    }

    /// Run `teleport` after the departure unless the player moved off the
    /// spot or left; a request during another departure is ignored.
    pub fn depart_then<F, Fut>(&self, player_id: PlayerId, teleport: F)
    where
        F: FnOnce(GameState) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let game = self.clone();
        tokio::spawn(async move {
            let Some(stayed) = game.wait_out_departure(&player_id).await else {
                return;
            };
            if stayed {
                teleport(game.clone()).await;
            }
            game.end_departure(&player_id).await;
        });
    }

    async fn wait_out_departure(&self, player_id: &PlayerId) -> Option<bool> {
        let (origin, _, floor_level) = self.get_player_position(player_id).await?;
        if !self.teleport_departures().insert(*player_id) {
            return None;
        }
        self.publish_teleport_effect(player_id, origin, floor_level, TeleportPhase::Departing)
            .await;
        tokio::time::sleep(TELEPORT_DEPARTURE).await;
        let stayed =
            self.get_player_position(player_id)
                .await
                .is_some_and(|(position, _, floor)| {
                    floor == floor_level && position.dist_xz_sq(&origin) <= DEPARTURE_DRIFT.powi(2)
                });
        Some(stayed)
    }

    /// True once after the departure, so the teleport skips showing it.
    pub(super) fn take_shown_departure(&self, player_id: &PlayerId) -> bool {
        self.teleport_departures().remove(player_id)
    }

    /// A departure that never became a teleport is cancelled for everyone
    /// who saw it start.
    async fn end_departure(&self, player_id: &PlayerId) {
        if !self.teleport_departures().remove(player_id) {
            return;
        }
        if let Some((position, _, floor_level)) = self.get_player_position(player_id).await {
            self.publish_teleport_effect(
                player_id,
                position,
                floor_level,
                TeleportPhase::Cancelled,
            )
            .await;
        }
    }
}
