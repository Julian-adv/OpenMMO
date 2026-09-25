use super::{passability::regions_around, GameState};
use crate::types::{Player, PlayerId, Position};
use onlinerpg_shared::estate_storage::{estate_storage_def, INTERACTION_RANGE};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::mana::MANA_REGEN_INTERVAL_MS;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;

pub(super) type BedIndex = HashMap<(i32, i32), Vec<FurniturePlacement>>;

pub(super) fn is_bed(object_type: &str) -> bool {
    matches!(
        object_type,
        "bed" | "rustic_bed" | "furniture_bed" | "furniture_rustic_bed"
    )
}

fn within_reach(player: &Player, position: Position, floor: i8) -> bool {
    player.floor_level == floor
        && player.position.dist_xz_sq(&position) <= (INTERACTION_RANGE + 0.5).powi(2)
        && (player.position.y - position.y).abs() <= 2.0
}

impl GameState {
    pub(super) fn sync_beds(&self, rx: i32, rz: i32, placements: &[FurniturePlacement]) {
        let beds: Vec<_> = placements
            .iter()
            .filter(|p| matches!(p.type_id.as_str(), "bed" | "rustic_bed"))
            .cloned()
            .collect();
        let mut index = self.beds.write().unwrap_or_else(|e| e.into_inner());
        if beds.is_empty() {
            index.remove(&(rx, rz));
        } else {
            index.insert((rx, rz), beds);
        }
    }

    pub(super) async fn valid_bed_pose(&self, player: &Player) -> bool {
        if !player.is_damageable(Self::now_ms()) || player.is_mounted() {
            return false;
        }
        let (Some(kind), Some(id)) = (player.object_type.as_deref(), player.object_id) else {
            return false;
        };
        if !is_bed(kind) {
            return false;
        }
        if estate_storage_def(kind).is_some() {
            return self
                .estate_chests
                .read()
                .await
                .get(i64::from(id))
                .is_some_and(|bed| {
                    bed.item_def_id == kind && within_reach(player, bed.position, bed.floor_level)
                });
        }
        let index = self.beds.read().unwrap_or_else(|e| e.into_inner());
        regions_around(player.position.x, player.position.z)
            .filter_map(|key| index.get(&key))
            .flatten()
            .any(|bed| {
                bed.id == id
                    && bed.type_id == kind
                    && within_reach(
                        player,
                        Position {
                            x: bed.x,
                            y: bed.y,
                            z: bed.z,
                        },
                        bed.floor_level as i8,
                    )
            })
    }

    pub(super) async fn update_bed_rest(&self, player: &Player) {
        let valid = self.valid_bed_pose(player).await;
        let mut rest = self.bed_rest_started.write().await;
        if valid {
            let combat_wait = super::OUT_OF_COMBAT_MS
                .saturating_sub(Self::now_ms().saturating_sub(player.last_combat_at));
            rest.insert(
                player.id,
                Instant::now() + Duration::from_millis(combat_wait),
            );
        } else {
            rest.remove(&player.id);
        }
    }

    pub(super) async fn bed_rest_multiplier(&self, player: &Player) -> u32 {
        if !player.object_type.as_deref().is_some_and(is_bed) {
            return 1;
        }
        if !self.valid_bed_pose(player).await {
            self.bed_rest_started.write().await.remove(&player.id);
            return 1;
        }
        let rested = self
            .bed_rest_started
            .read()
            .await
            .get(&player.id)
            .is_some_and(|start| start.elapsed() >= Duration::from_millis(MANA_REGEN_INTERVAL_MS));
        if rested {
            2
        } else {
            1
        }
    }

    pub(super) async fn end_invalid_bed_rests(&self) {
        let invalid = {
            let players = self.players.read().await;
            let mut invalid = Vec::new();
            for player in players
                .values()
                .filter(|p| p.object_type.as_deref().is_some_and(is_bed))
            {
                if !self.valid_bed_pose(player).await {
                    invalid.push(player.id);
                }
            }
            invalid
        };
        for id in invalid {
            self.stop_bed_rest(&id).await;
        }
    }

    pub(super) async fn stop_bed_rest(&self, player_id: &PlayerId) {
        let in_bed = self
            .players
            .read()
            .await
            .get(player_id)
            .is_some_and(|player| player.object_type.as_deref().is_some_and(is_bed));
        if in_bed {
            self.set_player_interaction(player_id, None, None).await;
        }
    }
}
