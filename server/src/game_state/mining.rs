//! Pickaxe mining of Blackvein Mine ore heaps and crystal clusters: a
//! `MINING_CAST_MS` cast that ends like breaking a barrel, with ore instead of
//! coins.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use onlinerpg_shared::dungeon::{PropKind, MINING_CAST_MS};
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::{PlayerId, Position};

use rand::Rng;

use super::dungeon::check_prop_reach;
use super::GameState;

pub(crate) const PICKAXE_ITEM: &str = "miner_pickaxe";
/// Chance a finished cast turns up ore at all.
const MINED_ORE_CHANCE: f64 = 0.20;

pub(crate) struct MiningSession {
    pub(super) until: Instant,
    kind: PropKind,
    entrance_id: String,
    depth: u8,
    prop_id: u32,
    position: Position,
    floor_level: i8,
}

/// What mining `kind` on floor `depth` yields; deeper floors give more.
fn mining_yield(kind: PropKind, depth: u8) -> (&'static str, u32) {
    match kind {
        PropKind::MineCrystalCluster => ("blackvein_crystal_shard", 1 + u32::from(depth) / 10),
        _ => ("blackvein_ore", 2 + u32::from(depth) / 8),
    }
}

impl GameState {
    /// Start mining a prop next to the player, if they carry a pickaxe.
    pub async fn start_mining(
        &self,
        player_id: &PlayerId,
        entrance_id: &str,
        depth: u8,
        prop_id: u32,
    ) {
        if depth == 0 {
            return;
        }
        let Some(entrance) = self.dungeon_defs.get(entrance_id) else {
            return;
        };
        self.ensure_dungeon_runtime(entrance_id).await;
        let (position, floor_level) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) if p.health > 0 => (p.position, p.floor_level),
                _ => return,
            }
        };
        let checked = {
            let dungeons = self.dungeons.read().await;
            let Some(rt) = dungeons.get(entrance_id) else {
                return;
            };
            let Some(prop) = rt
                .layouts
                .get((depth - 1) as usize)
                .and_then(|l| l.props.get(prop_id as usize))
            else {
                return;
            };
            let mined = rt
                .broken_props
                .get(&depth)
                .is_some_and(|ids| ids.contains(&prop_id));
            if !prop.kind.is_minable() || mined {
                return;
            }
            check_prop_reach(
                &entrance.position(),
                depth,
                prop,
                &position,
                floor_level,
                "ore",
            )
            .map(|_| prop.kind)
        };
        let kind = match checked {
            Ok(kind) if self.holds_item(player_id, PICKAXE_ITEM).await => kind,
            rejected => {
                let reason = rejected.err().unwrap_or_else(|| "no_pickaxe".into());
                self.send_direct_message(player_id, ServerMessage::InteractionRejected { reason })
                    .await;
                return;
            }
        };

        self.cancel_concentration_if_active(player_id).await;
        // A queued walk would cancel the cast on the next movement tick.
        self.clear_player_movement(player_id, "mining").await;
        let session = MiningSession {
            until: Instant::now() + Duration::from_millis(MINING_CAST_MS),
            kind,
            entrance_id: entrance_id.to_string(),
            depth,
            prop_id,
            position,
            floor_level,
        };
        if self
            .mining_sessions
            .write()
            .await
            .insert(*player_id, session)
            .is_none()
        {
            self.mining_active.fetch_add(1, Ordering::Relaxed);
        }
        self.publish_nearby(
            &position,
            floor_level,
            ServerMessage::MiningStarted {
                player_id: *player_id,
            },
            None,
        )
        .await;
    }

    /// Resolve due mining casts (250ms sweep, empty-map early-out).
    pub async fn tick_mining(&self) {
        let due: Vec<(PlayerId, MiningSession)> = {
            let mut sessions = self.mining_sessions.write().await;
            if sessions.is_empty() {
                return;
            }
            let now = Instant::now();
            let due_ids: Vec<PlayerId> = sessions
                .iter()
                .filter(|(_, s)| s.until <= now)
                .map(|(pid, _)| *pid)
                .collect();
            let due: Vec<_> = due_ids
                .into_iter()
                .filter_map(|pid| sessions.remove(&pid).map(|s| (pid, s)))
                .collect();
            self.mining_active.fetch_sub(due.len(), Ordering::Relaxed);
            due
        };

        for (player_id, s) in due {
            let mined = self.holds_item(&player_id, PICKAXE_ITEM).await
                && self
                    .interact_with_dungeon_prop(
                        &player_id,
                        &s.entrance_id,
                        s.depth,
                        s.prop_id,
                        "ore",
                        PropKind::is_minable,
                        |rt| &mut rt.broken_props,
                        ServerMessage::DungeonPropBroken {
                            entrance_id: s.entrance_id.clone(),
                            depth: s.depth,
                            prop_id: s.prop_id,
                        },
                    )
                    .await
                    .is_some();
            self.publish_mining_ended(&player_id, &s).await;
            if mined {
                self.rebuild_dungeon_floor_passability(&s.entrance_id, s.depth)
                    .await;
                if rand::thread_rng().gen_bool(MINED_ORE_CHANCE) {
                    let (item, quantity) = mining_yield(s.kind, s.depth);
                    self.award_items(&player_id, item, quantity).await;
                }
            }
        }
    }

    /// Hot path (every move packet): one atomic load while nobody mines.
    pub(super) async fn cancel_mining_if_active(&self, player_id: &PlayerId) {
        if self.mining_active.load(Ordering::Relaxed) == 0
            || !self.mining_sessions.read().await.contains_key(player_id)
        {
            return;
        }
        let session = self.mining_sessions.write().await.remove(player_id);
        if let Some(s) = session {
            self.mining_active.fetch_sub(1, Ordering::Relaxed);
            self.publish_mining_ended(player_id, &s).await;
        }
    }

    async fn publish_mining_ended(&self, player_id: &PlayerId, s: &MiningSession) {
        self.publish_nearby(
            &s.position,
            s.floor_level,
            ServerMessage::MiningEnded {
                player_id: *player_id,
            },
            None,
        )
        .await;
    }
}
