use onlinerpg_shared::mount::MountKind;

use super::GameState;
use crate::types::{Player, PlayerId, ServerMessage};

impl GameState {
    pub(super) async fn can_ride_here(&self, player: &Player, kind: MountKind) -> bool {
        if player.health == 0
            || player.floor_level != 0
            || player.object_type.is_some()
            || (kind.dismounts_in_combat() && Self::in_combat(player))
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
        let (min_depth, max_depth) = kind.water_depth_band();
        !inside
            && self
                .water_depth_at(player.position.x, player.position.z)
                .await
                .is_some_and(|depth| depth >= min_depth && depth <= max_depth)
    }

    pub(super) async fn toggle_mount(&self, player_id: &PlayerId, kind: MountKind) {
        let Some(player) = self.players.read().await.get(player_id).cloned() else {
            return;
        };
        if player.is_mounted() {
            self.set_mount(player_id, None).await;
            return;
        }
        if !self.can_ride_here(&player, kind).await {
            self.send_system_message(player_id, kind.cannot_mount_here_message())
                .await;
            return;
        }
        if !self.holds_item(player_id, kind.item_id()).await {
            return;
        }
        self.set_mount(player_id, Some(kind)).await;
    }

    pub(super) async fn set_mount(&self, player_id: &PlayerId, mount: Option<MountKind>) {
        let Some(mut movement) = self
            .lock_player_movement(*player_id, &[], 0.0, false, None)
            .await
        else {
            return;
        };
        self.cancel_goal_movement_locked(player_id, &mut movement.state)
            .await;
        let (was, at, floor) = (
            movement.player.mount,
            movement.player.position,
            movement.player.floor_level,
        );
        if was == mount {
            return;
        }
        // Boarding changes height immediately, but never lifts a dungeon player.
        let ground_y = if floor == 0 {
            self.surface_ground_y(0, &at, at.y, mount).await
        } else {
            at.y
        };
        let (position, rotation) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            if player.mount != was || player.position != at || player.floor_level != floor {
                return;
            }
            player.mount = mount;
            player.position.y = ground_y;
            (player.position, player.rotation)
        };
        drop(movement.regions);
        if (position.y - at.y).abs() > 1e-3 {
            self.publish_nearby(
                &position,
                floor,
                ServerMessage::PlayerMoved {
                    player_id: *player_id,
                    position,
                    rotation,
                    floor_level: floor,
                    sprinting: false,
                },
                None,
            )
            .await;
        }
        self.publish_nearby(
            &position,
            floor,
            ServerMessage::PlayerMountChanged {
                player_id: *player_id,
                mount,
            },
            None,
        )
        .await;
    }

    pub(super) async fn validate_mounts(&self) {
        let riders: Vec<_> = self
            .players
            .read()
            .await
            .values()
            .filter_map(|player| player.mount.map(|kind| (player.clone(), kind)))
            .collect();
        for (player, kind) in riders {
            if !self.can_ride_here(&player, kind).await
                || !self.holds_item(&player.id, kind.item_id()).await
            {
                self.set_mount(&player.id, None).await;
            }
        }
    }
}
