//! `/follow <name>`: an admin shadows a player. The admin's client walks after
//! the target; this tick moves the admin beside them when walking can't keep
//! up (teleports, dungeon entrances, respawns, mounts).

use super::GameState;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::shortest_world_delta_x;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

pub(crate) const ADMIN_FOLLOW_TICK: Duration = Duration::from_secs(1);
/// Past this the client's walk can't close the gap in reasonable time.
const RELOCATE_DISTANCE_M: f32 = 24.0;
/// Ticks of a floor mismatch before relocating, so stairs are walked.
const FLOOR_MISMATCH_TICKS: u8 = 3;

#[derive(Clone, Copy)]
pub(super) struct AdminFollow {
    target: PlayerId,
    floor_mismatch_ticks: u8,
}

pub(super) type AdminFollows = std::sync::Mutex<HashMap<PlayerId, AdminFollow>>;

enum Step {
    Stay,
    Relocate,
    TargetGone,
}

impl GameState {
    fn admin_follows(&self) -> std::sync::MutexGuard<'_, HashMap<PlayerId, AdminFollow>> {
        self.admin_follows.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(super) async fn follow_command(
        &self,
        admin_id: &PlayerId,
        name: &str,
    ) -> Result<String, String> {
        if name.is_empty() {
            return self.unfollow_command(admin_id).await;
        }
        let target_id = self
            .admin_target(admin_id, name, "Follow", "/follow <name>")
            .await?;
        let canonical = self.player_name_of(&target_id).await;
        self.admin_follows().insert(
            *admin_id,
            AdminFollow {
                target: target_id,
                floor_mismatch_ticks: 0,
            },
        );
        self.relocate_follower(admin_id, &target_id).await;
        self.send_direct_message(
            admin_id,
            ServerMessage::AdminFollowChanged {
                target: Some(target_id),
                name: canonical.clone(),
            },
        )
        .await;
        info!(admin = ?admin_id, target = %canonical, "admin follow");
        Ok(format!("Follow: following {canonical}. /unfollow to stop."))
    }

    pub(super) async fn unfollow_command(&self, admin_id: &PlayerId) -> Result<String, String> {
        let removed = self.admin_follows().remove(admin_id);
        let Some(follow) = removed else {
            // The client may still hold a follow a server restart forgot.
            self.send_follow_ended(admin_id, "").await;
            return Err("Follow: you are not following anyone.".to_string());
        };
        let name = self.player_name_of(&follow.target).await;
        self.send_follow_ended(admin_id, &name).await;
        info!(admin = ?admin_id, target = %name, "admin unfollow");
        Ok(format!("Follow: stopped following {name}."))
    }

    async fn send_follow_ended(&self, admin_id: &PlayerId, name: &str) {
        self.send_direct_message(
            admin_id,
            ServerMessage::AdminFollowChanged {
                target: None,
                name: name.to_string(),
            },
        )
        .await;
    }

    async fn relocate_follower(&self, admin_id: &PlayerId, target_id: &PlayerId) {
        let Some((center, rotation, floor, _)) = self.player_pose(target_id).await else {
            return;
        };
        let arrival = self.arrival_beside(admin_id, &center);
        self.teleport_player(admin_id, arrival, rotation, floor)
            .await;
    }

    /// Drops follows that involve a leaving player: their own, and any aimed
    /// at them, so a later login can't inherit the id.
    pub(super) async fn end_admin_follows_of(&self, player_id: &PlayerId) {
        let orphaned: Vec<PlayerId> = {
            let mut follows = self.admin_follows();
            if follows.is_empty() {
                return;
            }
            follows.remove(player_id);
            let orphaned = follows
                .iter()
                .filter(|(_, f)| f.target == *player_id)
                .map(|(admin, _)| *admin)
                .collect();
            follows.retain(|_, f| f.target != *player_id);
            orphaned
        };
        if orphaned.is_empty() {
            return;
        }
        let name = self.player_name_of(player_id).await;
        for admin_id in orphaned {
            self.send_follow_ended(&admin_id, &name).await;
            self.send_system_message(&admin_id, format!("Follow: {name} left the world."))
                .await;
        }
    }

    pub async fn tick_admin_follows(&self) {
        let pairs: Vec<(PlayerId, AdminFollow)> = {
            let follows = self.admin_follows();
            if follows.is_empty() {
                return;
            }
            follows.iter().map(|(a, f)| (*a, *f)).collect()
        };
        let steps: Vec<(PlayerId, AdminFollow, Step)> = {
            let players = self.players.read().await;
            pairs
                .into_iter()
                .filter_map(|(admin_id, mut follow)| {
                    let admin = players.get(&admin_id)?;
                    let Some(target) = players.get(&follow.target) else {
                        return Some((admin_id, follow, Step::TargetGone));
                    };
                    if admin.health == 0 {
                        return Some((admin_id, follow, Step::Stay));
                    }
                    let gap = shortest_world_delta_x(admin.position.x, target.position.x)
                        .hypot(target.position.z - admin.position.z);
                    follow.floor_mismatch_ticks = if admin.floor_level == target.floor_level {
                        0
                    } else {
                        follow.floor_mismatch_ticks.saturating_add(1)
                    };
                    let step = if gap > RELOCATE_DISTANCE_M
                        || follow.floor_mismatch_ticks >= FLOOR_MISMATCH_TICKS
                    {
                        follow.floor_mismatch_ticks = 0;
                        Step::Relocate
                    } else {
                        Step::Stay
                    };
                    Some((admin_id, follow, step))
                })
                .collect()
        };
        {
            let mut follows = self.admin_follows();
            for (admin_id, follow, step) in &steps {
                let Some(current) = follows.get_mut(admin_id) else {
                    continue;
                };
                if current.target != follow.target {
                    continue;
                }
                if matches!(step, Step::TargetGone) {
                    follows.remove(admin_id);
                } else {
                    *current = *follow;
                }
            }
        }
        for (admin_id, follow, step) in steps {
            match step {
                Step::Stay => {}
                Step::Relocate => self.relocate_follower(&admin_id, &follow.target).await,
                Step::TargetGone => {
                    let name = self.player_name_of(&follow.target).await;
                    self.send_follow_ended(&admin_id, &name).await;
                    self.send_system_message(&admin_id, "Follow: your target left the world.")
                        .await;
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn admin_follow_target(&self, admin_id: &PlayerId) -> Option<PlayerId> {
        self.admin_follows().get(admin_id).map(|f| f.target)
    }
}
