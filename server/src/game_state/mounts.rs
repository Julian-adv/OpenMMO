use super::GameState;
use crate::types::{Player, PlayerId, ServerMessage};

impl GameState {
    pub(super) async fn can_ride_here(&self, player: &Player) -> bool {
        if player.health == 0
            || player.floor_level != 0
            || player.object_type.is_some()
            || Self::in_combat(player)
        {
            return false;
        }
        let inside = self.passability_read().values().any(|entry| {
            entry.is_ground
                && player.position.x >= entry.min_x
                && player.position.x <= entry.max_x
                && player.position.z >= entry.min_z
                && player.position.z <= entry.max_z
        });
        !inside
            && self
                .water_depth_at(player.position.x, player.position.z)
                .await
                .is_some_and(|depth| depth <= 0.6)
    }

    pub(super) async fn toggle_horse_mount(&self, player_id: &PlayerId) {
        let Some(player) = self.players.read().await.get(player_id).cloned() else {
            return;
        };
        if player.mounted {
            self.set_horse_mounted(player_id, false).await;
            return;
        }
        if !self.can_ride_here(&player).await {
            self.send_system_message(
                player_id,
                "Mount on outdoor ground while alive and out of combat.",
            )
            .await;
            return;
        }
        if !self.holds_item(player_id, "horse_reins").await {
            return;
        }
        self.set_horse_mounted(player_id, true).await;
    }

    pub(super) async fn set_horse_mounted(&self, player_id: &PlayerId, mounted: bool) {
        let (position, floor) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            if player.mounted == mounted {
                return;
            }
            player.mounted = mounted;
            (player.position, player.floor_level)
        };
        self.send_direct_message_to_players_within_position(
            &position,
            floor,
            super::EVENT_DELIVERY_RADIUS,
            ServerMessage::PlayerMountChanged {
                player_id: *player_id,
                mounted,
            },
            None,
        )
        .await;
    }

    pub(super) async fn validate_horse_mounts(&self) {
        let riders: Vec<_> = self
            .players
            .read()
            .await
            .values()
            .filter(|player| player.mounted)
            .cloned()
            .collect();
        for player in riders {
            if !self.can_ride_here(&player).await
                || !self.holds_item(&player.id, "horse_reins").await
            {
                self.set_horse_mounted(&player.id, false).await;
            }
        }
    }
}
