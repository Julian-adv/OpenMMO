//! Server-side dungeon runtime. Layouts are regenerated deterministically
//! from the entrance id (same shared-crate generator the client runs via
//! wasm), so the runtime holds only live state: cached layouts now, spawn
//! slots and chest cooldowns in later stages. Everything is in-memory —
//! after a restart, reconnecting players rehydrate from the generator
//! plus their persisted position/floor_level.

use std::collections::{HashMap, HashSet};

use onlinerpg_shared::dungeon::{
    cell_center, dungeon_seed, floor_world_y, generate_dungeon, monster_level_for_depth,
    FloorLayout,
};
use onlinerpg_shared::{Position, ServerMessage};
use tracing::{info, warn};

use crate::types::PlayerId;

use super::GameState;

/// Vertical slack when checking that a reported floor change matches the
/// dungeon floor's Y (covers mid-stair positions near the switch point).
const FLOOR_Y_TOLERANCE: f32 = 2.5;

const MONSTER_RESPAWN_MS: u64 = 5 * 60 * 1000;
const BOSS_RESPAWN_MS: u64 = 30 * 60 * 1000;
/// Retry delay when a spawn attempt failed (e.g. global monster cap).
const SPAWN_RETRY_MS: u64 = 10 * 1000;
/// Per-player chest cooldown (shared dungeon — the chest refills for each
/// player separately). In-memory; resets on server restart, acceptable.
const CHEST_COOLDOWN_MS: u64 = 60 * 60 * 1000;
const CHEST_INTERACT_RANGE: f32 = 2.5;
const CHEST_ITEM_MIN_PRICE: i64 = 2000;

pub(super) struct DungeonRuntime {
    pub layouts: Vec<FloorLayout>,
    /// Live per-floor state, keyed by depth. Created when a player first
    /// enters the floor.
    pub floors: HashMap<u8, FloorRuntime>,
    /// Per-player chest open timestamps (ms).
    pub chest_opened_at: HashMap<PlayerId, u64>,
}

pub(super) struct FloorRuntime {
    /// One slot per layout SpawnSpec, same order.
    pub slots: Vec<SpawnSlot>,
    pub players: HashSet<PlayerId>,
}

pub(super) struct SpawnSlot {
    pub alive_monster_id: Option<String>,
    pub respawn_at_ms: u64,
    pub is_boss: bool,
}

/// Reverse index entry: which dungeon slot a live monster belongs to.
pub(super) struct DungeonMonsterRef {
    pub entrance_id: String,
    pub depth: u8,
    pub slot: usize,
}

impl GameState {
    /// Lazily generate and cache the layouts for a dungeon.
    pub(super) async fn ensure_dungeon_runtime(&self, entrance_id: &str) {
        {
            let dungeons = self.dungeons.read().await;
            if dungeons.contains_key(entrance_id) {
                return;
            }
        }
        let layouts = generate_dungeon(dungeon_seed(entrance_id));
        info!(
            "Dungeon '{}' runtime created ({} floors)",
            entrance_id,
            layouts.len()
        );
        let mut dungeons = self.dungeons.write().await;
        dungeons
            .entry(entrance_id.to_string())
            .or_insert(DungeonRuntime {
                layouts,
                floors: HashMap::new(),
                chest_opened_at: HashMap::new(),
            });
    }

    /// Open the final-floor treasure chest: requires standing next to it
    /// on the last floor with the boss dead and the per-player cooldown
    /// expired. Loot (2–3 equipment rolls + depth-scaled gold) goes
    /// straight to the opener; the open is broadcast nearby.
    pub async fn open_dungeon_chest(&self, player_id: &PlayerId, entrance_id: &str) {
        let Some(entrance) = self.dungeon_defs.get(entrance_id).cloned() else {
            return;
        };
        self.ensure_dungeon_runtime(entrance_id).await;

        let (player_pos, player_floor) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) if p.health > 0 => (p.position.clone(), p.floor_level),
                _ => return,
            }
        };

        let now = Self::now_ms();
        // Validate under the dungeon lock; claim the cooldown on success.
        let chest_check = {
            let mut dungeons = self.dungeons.write().await;
            let Some(rt) = dungeons.get_mut(entrance_id) else {
                return;
            };
            let total = rt.layouts.len() as u8;
            let last = match rt.layouts.last() {
                Some(l) => l,
                None => return,
            };
            let Some(chest) = last.chest else { return };

            if player_floor != -(total as i8) {
                Some("You must be on the deepest floor")
            } else {
                let chest_pos = cell_center(&entrance.position(), total, chest);
                let dx = player_pos.x - chest_pos.x;
                let dz = player_pos.z - chest_pos.z;
                if dx * dx + dz * dz > CHEST_INTERACT_RANGE * CHEST_INTERACT_RANGE {
                    Some("Too far from the chest")
                } else if rt.floors.get(&total).is_some_and(|fr| {
                    fr.slots.iter().any(|s| {
                        s.is_boss && s.alive_monster_id.as_ref().is_some_and(|id| !id.is_empty())
                    })
                }) {
                    Some("The guardian still lives")
                } else if rt
                    .chest_opened_at
                    .get(player_id)
                    .is_some_and(|&t| now.saturating_sub(t) < CHEST_COOLDOWN_MS)
                {
                    Some("The chest is empty (come back later)")
                } else {
                    rt.chest_opened_at.insert(player_id.clone(), now);
                    None
                }
            }
        };
        if let Some(reason) = chest_check {
            self.send_direct_message(
                player_id,
                ServerMessage::InteractionRejected {
                    reason: reason.to_string(),
                },
            )
            .await;
            return;
        }

        // Roll loot: 2–3 distinct equipment items priced for endgame.
        let depth = {
            let dungeons = self.dungeons.read().await;
            dungeons
                .get(entrance_id)
                .map(|rt| rt.layouts.len() as i64)
                .unwrap_or(5)
        };
        let (item_def_ids, gold) = {
            use rand::seq::SliceRandom;
            use rand::Rng;
            let mut rng = rand::thread_rng();
            let mut pool = self
                .item_defs
                .equipment_ids_with_min_price(CHEST_ITEM_MIN_PRICE);
            pool.shuffle(&mut rng);
            let count = rng.gen_range(2..=3).min(pool.len());
            let items: Vec<String> = pool.into_iter().take(count).collect();
            let gold = rng.gen_range(depth * 500..=depth * 1500);
            (items, gold)
        };

        for item_def_id in &item_def_ids {
            // give_item falls back to an inventory error when over carry
            // weight; the player keeps what fits.
            self.give_item(player_id, item_def_id).await;
        }
        let new_gold = {
            let mut gold_map = self.player_gold.write().await;
            let wallet = gold_map.entry(player_id.clone()).or_insert(0);
            *wallet += gold;
            *wallet
        };
        self.mark_dirty(player_id).await;
        self.send_direct_message(player_id, ServerMessage::GoldUpdate { gold: new_gold })
            .await;

        info!(
            "Player {} opened dungeon chest '{}': {:?} + {} gold",
            player_id, entrance_id, item_def_ids, gold
        );
        self.send_direct_message_to_players_within_position(
            &player_pos,
            player_floor,
            super::AGENT_EVENT_DELIVERY_RADIUS,
            ServerMessage::DungeonChestOpened {
                entrance_id: entrance_id.to_string(),
                player_id: player_id.clone(),
                item_def_ids,
                gold,
            },
            None,
        )
        .await;
    }

    /// Track floor occupancy and monster lifecycles across dungeon floor
    /// changes (stairs, death respawn, disconnect, login rehydrate).
    /// `old_pos`/`new_pos` locate the dungeon for each side — on respawn
    /// the new position is the world spawn, far from the footprint.
    pub(crate) async fn handle_player_floor_change(
        &self,
        player_id: &PlayerId,
        old_floor: i8,
        new_floor: i8,
        old_pos: &Position,
        new_pos: &Position,
    ) {
        if old_floor >= 0 && new_floor >= 0 {
            return;
        }
        if old_floor < 0 {
            if let Some(entrance) = self.dungeon_defs.entrance_at(old_pos.x, old_pos.z).cloned() {
                self.leave_dungeon_floor(player_id, &entrance.id, (-old_floor) as u8)
                    .await;
            }
        }
        if new_floor < 0 {
            if let Some(entrance) = self.dungeon_defs.entrance_at(new_pos.x, new_pos.z).cloned() {
                self.enter_dungeon_floor(player_id, &entrance.id, (-new_floor) as u8)
                    .await;
            }
        }
    }

    async fn enter_dungeon_floor(&self, player_id: &PlayerId, entrance_id: &str, depth: u8) {
        self.ensure_dungeon_runtime(entrance_id).await;
        {
            let mut dungeons = self.dungeons.write().await;
            let Some(rt) = dungeons.get_mut(entrance_id) else {
                return;
            };
            let Some(layout) = rt.layouts.get((depth - 1) as usize) else {
                return;
            };
            let slots: Vec<SpawnSlot> = layout
                .spawns
                .iter()
                .map(|s| SpawnSlot {
                    alive_monster_id: None,
                    respawn_at_ms: 0,
                    is_boss: s.is_boss,
                })
                .collect();
            let fr = rt.floors.entry(depth).or_insert_with(|| FloorRuntime {
                slots,
                players: HashSet::new(),
            });
            fr.players.insert(player_id.clone());
        }
        self.populate_dungeon_floor(entrance_id, depth, player_id)
            .await;
    }

    /// Spawn monsters into every free, respawn-ready slot of a floor and
    /// assign their AI to `owner`. Claims slots under the lock, spawns
    /// outside it, then records the ids.
    pub(crate) async fn populate_dungeon_floor(
        &self,
        entrance_id: &str,
        depth: u8,
        owner: &PlayerId,
    ) {
        let Some(entrance) = self.dungeon_defs.get(entrance_id).cloned() else {
            return;
        };
        let now = Self::now_ms();

        let to_spawn: Vec<(usize, i32, i32, String)> = {
            let mut dungeons = self.dungeons.write().await;
            let Some(rt) = dungeons.get_mut(entrance_id) else {
                return;
            };
            let Some(layout) = rt.layouts.get((depth - 1) as usize) else {
                return;
            };
            let specs = layout.spawns.clone();
            let Some(fr) = rt.floors.get_mut(&depth) else {
                return;
            };
            let mut claimed = Vec::new();
            for (i, slot) in fr.slots.iter_mut().enumerate() {
                if slot.alive_monster_id.is_none() && now >= slot.respawn_at_ms {
                    // Claim under the lock so concurrent callers can't
                    // double-spawn the slot.
                    slot.alive_monster_id = Some(String::new());
                    let spec = &specs[i];
                    claimed.push((i, spec.x, spec.z, spec.monster_type.clone()));
                }
            }
            claimed
        };

        for (slot_idx, cx, cz, monster_type) in to_spawn {
            let def_level = self
                .monster_defs
                .get(&monster_type)
                .map(|d| d.level)
                .unwrap_or(1);
            let level = monster_level_for_depth(def_level, depth);
            let pos = cell_center(&entrance.position(), depth, (cx, cz));
            let spawned = self
                .spawn_monster(
                    monster_type,
                    pos,
                    0.0,
                    Some(owner.clone()),
                    -(depth as i8),
                    Some(level),
                )
                .await;

            let mut dungeons = self.dungeons.write().await;
            let slot = dungeons
                .get_mut(entrance_id)
                .and_then(|rt| rt.floors.get_mut(&depth))
                .and_then(|fr| fr.slots.get_mut(slot_idx));
            match (slot, spawned) {
                (Some(slot), Some(monster)) => {
                    slot.alive_monster_id = Some(monster.id.clone());
                    drop(dungeons);
                    let mut index = self.dungeon_monsters.write().await;
                    index.insert(
                        monster.id.clone(),
                        DungeonMonsterRef {
                            entrance_id: entrance_id.to_string(),
                            depth,
                            slot: slot_idx,
                        },
                    );
                    drop(index);
                    self.send_direct_message(owner, ServerMessage::MonsterAssigned { monster })
                        .await;
                }
                (Some(slot), None) => {
                    slot.alive_monster_id = None;
                    slot.respawn_at_ms = now + SPAWN_RETRY_MS;
                }
                _ => {}
            }
        }
    }

    async fn leave_dungeon_floor(&self, player_id: &PlayerId, entrance_id: &str, depth: u8) {
        // Occupancy + alive-monster snapshot under one lock.
        let (remaining_owner, alive_ids, floor_emptied) = {
            let mut dungeons = self.dungeons.write().await;
            let Some(fr) = dungeons
                .get_mut(entrance_id)
                .and_then(|rt| rt.floors.get_mut(&depth))
            else {
                return;
            };
            fr.players.remove(player_id);
            let remaining = fr.players.iter().next().cloned();
            let alive: Vec<String> = fr
                .slots
                .iter()
                .filter_map(|s| s.alive_monster_id.clone())
                .filter(|id| !id.is_empty())
                .collect();
            if remaining.is_none() {
                for slot in fr.slots.iter_mut() {
                    slot.alive_monster_id = None;
                    // Empty floors repopulate instantly on next entry.
                    slot.respawn_at_ms = 0;
                }
            }
            (remaining, alive, false)
        };
        let _ = floor_emptied;

        match remaining_owner {
            Some(new_owner) => {
                // Hand the leaver's monsters to a player still on the floor.
                let reassigned: Vec<crate::types::Monster> = {
                    let mut monsters = self.monsters.write().await;
                    let mut out = Vec::new();
                    for id in &alive_ids {
                        if let Some(m) = monsters.get_mut(id) {
                            if m.owner_id.as_deref() == Some(player_id.as_str()) {
                                m.owner_id = Some(new_owner.clone());
                                out.push(m.clone());
                            }
                        }
                    }
                    out
                };
                for monster in reassigned {
                    info!("Dungeon monster {} reassigned to {}", monster.id, new_owner);
                    self.send_direct_message(
                        &new_owner,
                        ServerMessage::MonsterAssigned { monster },
                    )
                    .await;
                }
            }
            None => {
                // Floor emptied: despawn everything (only monsters respawn
                // in a shared dungeon — and this bounds live monster count).
                let removed: Vec<crate::types::Monster> = {
                    let mut monsters = self.monsters.write().await;
                    alive_ids
                        .iter()
                        .filter_map(|id| monsters.remove(id))
                        .collect()
                };
                {
                    let mut index = self.dungeon_monsters.write().await;
                    for id in &alive_ids {
                        index.remove(id);
                    }
                }
                for monster in removed {
                    self.send_direct_message_to_players_within_position(
                        &monster.position,
                        monster.floor_level,
                        super::AGENT_EVENT_DELIVERY_RADIUS,
                        ServerMessage::MonsterRemoved {
                            monster_id: monster.id,
                        },
                        None,
                    )
                    .await;
                }
            }
        }
    }

    /// Periodic tick: refill expired spawn slots on occupied floors so
    /// monsters respawn while players camp a floor.
    pub async fn tick_dungeons(&self) {
        let occupied: Vec<(String, u8, PlayerId)> = {
            let dungeons = self.dungeons.read().await;
            dungeons
                .iter()
                .flat_map(|(id, rt)| {
                    rt.floors.iter().filter_map(|(depth, fr)| {
                        fr.players
                            .iter()
                            .next()
                            .map(|p| (id.clone(), *depth, p.clone()))
                    })
                })
                .collect()
        };
        for (entrance_id, depth, owner) in occupied {
            self.populate_dungeon_floor(&entrance_id, depth, &owner)
                .await;
        }
    }

    /// Mark a dungeon monster's slot for respawn after it dies. Called
    /// from the combat death path; no-op for non-dungeon monsters.
    pub(super) async fn on_dungeon_monster_dead(&self, monster_id: &str) {
        let entry = {
            let mut index = self.dungeon_monsters.write().await;
            index.remove(monster_id)
        };
        let Some(entry) = entry else { return };
        let now = Self::now_ms();
        let mut dungeons = self.dungeons.write().await;
        if let Some(slot) = dungeons
            .get_mut(&entry.entrance_id)
            .and_then(|rt| rt.floors.get_mut(&entry.depth))
            .and_then(|fr| fr.slots.get_mut(entry.slot))
        {
            slot.alive_monster_id = None;
            slot.respawn_at_ms = now
                + if slot.is_boss {
                    BOSS_RESPAWN_MS
                } else {
                    MONSTER_RESPAWN_MS
                };
        }
    }

    /// Validate a client-reported floor change into/inside/out of a
    /// dungeon. Returns the floor to actually store: the requested floor
    /// when plausible, otherwise the player's current floor.
    ///
    /// Movement here mirrors the codebase's trust model (terrain
    /// collision is client-side): we only require that the position lies
    /// inside a known dungeon footprint and that the reported Y matches
    /// the floor's world Y, which is what walking the stair shafts
    /// produces naturally.
    pub(super) async fn validated_dungeon_floor(
        &self,
        player_id: &PlayerId,
        current_floor: i8,
        requested_floor: i8,
        position: &Position,
    ) -> i8 {
        if requested_floor >= 0 {
            return requested_floor;
        }

        let Some(entrance) = self
            .dungeon_defs
            .entrance_at(position.x, position.z)
            .cloned()
        else {
            warn!(
                "Player {} reported dungeon floor {} outside any dungeon footprint",
                player_id, requested_floor
            );
            return current_floor.max(0);
        };

        self.ensure_dungeon_runtime(&entrance.id).await;
        let depth = (-requested_floor) as usize;
        let total = {
            let dungeons = self.dungeons.read().await;
            dungeons
                .get(&entrance.id)
                .map(|d| d.layouts.len())
                .unwrap_or(0)
        };
        if depth == 0 || depth > total {
            warn!(
                "Player {} reported invalid dungeon depth {} (dungeon '{}' has {} floors)",
                player_id, depth, entrance.id, total
            );
            return current_floor;
        }

        let expected_y = floor_world_y(entrance.y, depth as u8);
        if (position.y - expected_y).abs() > FLOOR_Y_TOLERANCE {
            warn!(
                "Player {} floor {} Y mismatch: reported {:.1}, expected {:.1}",
                player_id, requested_floor, position.y, expected_y
            );
            return current_floor;
        }

        requested_floor
    }

    /// Infer the dungeon floor for an arbitrary position (used by debug
    /// teleports): if it lies in a dungeon footprint and its Y matches a
    /// floor's world Y, return that floor; otherwise 0 (surface).
    pub(crate) async fn dungeon_floor_for_position(&self, position: &Position) -> i8 {
        let Some(entrance) = self
            .dungeon_defs
            .entrance_at(position.x, position.z)
            .cloned()
        else {
            return 0;
        };
        self.ensure_dungeon_runtime(&entrance.id).await;
        let total = {
            let dungeons = self.dungeons.read().await;
            dungeons
                .get(&entrance.id)
                .map(|d| d.layouts.len())
                .unwrap_or(0)
        };
        for depth in 1..=total {
            let y = floor_world_y(entrance.y, depth as u8);
            if (position.y - y).abs() <= FLOOR_Y_TOLERANCE {
                return -(depth as i8);
            }
        }
        0
    }

    /// Called on login when the persisted floor_level is negative: verify
    /// the saved position still maps to a known dungeon and prime its
    /// runtime. Returns false when the dungeon no longer exists (caller
    /// should fall back to the world spawn).
    pub(crate) async fn rehydrate_dungeon_player(
        &self,
        player_id: &PlayerId,
        position: &Position,
        floor_level: i8,
    ) -> bool {
        let Some(entrance) = self
            .dungeon_defs
            .entrance_at(position.x, position.z)
            .cloned()
        else {
            warn!(
                "Player {} saved at dungeon floor {} but no entrance covers ({:.1}, {:.1})",
                player_id, floor_level, position.x, position.z
            );
            return false;
        };
        self.ensure_dungeon_runtime(&entrance.id).await;
        let depth = (-floor_level) as usize;
        let valid = {
            let dungeons = self.dungeons.read().await;
            dungeons
                .get(&entrance.id)
                .is_some_and(|d| depth >= 1 && depth <= d.layouts.len())
        };
        if valid {
            info!(
                "Player {} rehydrated in dungeon '{}' at depth {}",
                player_id, entrance.id, depth
            );
        }
        valid
    }
}
