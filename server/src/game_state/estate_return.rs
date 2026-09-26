use super::{auth_db, inventory::consume_one, GameState};
use crate::{
    auth::AuthService,
    item_defs::AuthenticatedUseAction,
    types::{PlayerId, Position},
};
use onlinerpg_shared::pathfinding::{get_floor_y_base, is_circle_blocked_on_floor};
use onlinerpg_terrain::{defaults::TILE_DIM, land::PLOT_SIZE};
use std::sync::LazyLock;

static ARRIVAL_OFFSETS: LazyLock<Vec<(i32, i32)>> = LazyLock::new(|| {
    let mut offsets: Vec<_> = (0..PLOT_SIZE)
        .flat_map(|z| (0..PLOT_SIZE).map(move |x| (x, z)))
        .collect();
    offsets
        .sort_by_key(|(x, z)| (2 * x - (PLOT_SIZE - 1)).pow(2) + (2 * z - (PLOT_SIZE - 1)).pow(2));
    offsets
});

impl GameState {
    pub async fn use_estate_return_scroll(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        auth: &AuthService,
    ) {
        if !self.try_estate_return(player_id, instance_id, auth).await {
            self.cancel_teleport_effect(player_id).await;
        }
    }

    async fn try_estate_return(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        auth: &AuthService,
    ) -> bool {
        if self.authenticated_use_action(player_id, instance_id).await
            != Some(AuthenticatedUseAction::EstateReturn)
            || self
                .reject_if_trade_reserved(player_id, instance_id, "use")
                .await
            || self
                .reject_if_defeated(player_id, "You can't read while defeated")
                .await
        {
            return false;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return false;
        };
        let auth = auth.clone();
        let plots = match auth_db(move || auth.homestead_plots(character_id)).await {
            Ok(plots) => plots,
            Err(error) => {
                tracing::warn!(%error, "Failed to load estate return destination");
                self.send_system_message(player_id, "Estate return is temporarily unavailable.")
                    .await;
                return false;
            }
        };
        if plots.is_empty() {
            self.send_system_message(player_id, "You don't own an estate to return to.")
                .await;
            return false;
        }
        let Some(position) = self.estate_return_position(&plots).await else {
            self.send_system_message(
                player_id,
                "No safe outdoor arrival spot was found on your estate.",
            )
            .await;
            return false;
        };
        if self
            .reject_if_trade_reserved(player_id, instance_id, "use")
            .await
            || self
                .reject_if_defeated(player_id, "You can't read while defeated")
                .await
        {
            return false;
        }
        let (def_id, snapshot) = {
            let mut inventories = self.inventories.write().await;
            let Some(inv) = inventories.get_mut(player_id) else {
                return false;
            };
            if !inv.bag.iter().any(|item| {
                item.instance_id == instance_id
                    && item.quantity > 0
                    && self.item_defs.get(&item.item_def_id).is_some_and(|def| {
                        def.authenticated_use_action == Some(AuthenticatedUseAction::EstateReturn)
                    })
            }) {
                return false;
            }
            (consume_one(inv, instance_id), inv.clone())
        };
        if let Some(def_id) = def_id {
            self.log_consumed(player_id, &def_id).await;
        }
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.teleport_player_with_effects(player_id, position, 0.0, 0)
            .await;
        true
    }

    async fn estate_return_position(&self, plots: &[(i32, i32, u8)]) -> Option<Position> {
        'plots: for &(tx, tz, quadrant) in plots {
            let origin_x = tx * TILE_DIM as i32 - PLOT_SIZE + i32::from(quadrant % 2) * PLOT_SIZE;
            let origin_z = tz * TILE_DIM as i32 - PLOT_SIZE + i32::from(quadrant / 2) * PLOT_SIZE;
            for &(dx, dz) in ARRIVAL_OFFSETS.iter() {
                let x = (origin_x + dx) as f32 + 0.5;
                let z = (origin_z + dz) as f32 + 0.5;
                {
                    let cache = self.passability_read();
                    if get_floor_y_base(&cache, x, z, 0).is_some()
                        || is_circle_blocked_on_floor(&cache, x, z, 0.3, 0, None)
                    {
                        continue;
                    }
                }
                let Some((y, depth)) = self.ground_and_depth_at(x, z).await else {
                    continue 'plots;
                };
                if y.is_finite() && depth <= 0.1 {
                    return Some(Position { x, y, z });
                }
            }
        }
        None
    }
}
