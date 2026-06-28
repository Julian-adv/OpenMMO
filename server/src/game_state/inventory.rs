use crate::auth::AuthService;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::inventory::{EquipSlot, GroundItem, ItemInstance, PlayerInventory};
use rand::Rng;
use tracing::{info, warn};

use super::ServerGroundItem;

/// Ground items despawn after 5 minutes.
const GROUND_ITEM_LIFETIME_MS: u64 = 5 * 60 * 1000;

const MAX_PICKUP_DISTANCE: f32 = 2.5;

/// Serialize a PlayerInventory into the flat row format used by AuthService persistence.
fn serialize_inventory(inv: &PlayerInventory) -> Vec<(String, u32, Option<String>)> {
    let mut rows: Vec<(String, u32, Option<String>)> = inv
        .bag
        .iter()
        .map(|item| (item.item_def_id.clone(), item.quantity, None))
        .collect();
    for (slot, item) in &inv.equipped {
        rows.push((item.item_def_id.clone(), 1, Some(slot.as_str().to_string())));
    }
    rows
}

impl super::GameState {
    /// Reserve a range of instance IDs (single lock acquisition).
    async fn reserve_instance_ids(&self, count: u64) -> u64 {
        let mut id = self.next_item_instance_id.write().await;
        let start = *id;
        *id += count;
        start
    }

    pub(super) async fn next_instance_id(&self) -> u64 {
        self.reserve_instance_ids(1).await
    }

    /// D&D 5e carry weight: STR * 15.
    pub(super) async fn max_carry_weight(&self, player_id: &PlayerId) -> f32 {
        let chars = self.player_characters.read().await;
        if let Some((_, _, attrs)) = chars.get(player_id) {
            attrs.r#str as f32 * 15.0
        } else {
            150.0
        }
    }

    pub(super) fn calc_total_weight(&self, inventory: &PlayerInventory) -> f32 {
        let bag_weight: f32 = inventory
            .bag
            .iter()
            .map(|item| self.item_defs.weight(&item.item_def_id) * item.quantity as f32)
            .sum();
        let equip_weight: f32 = inventory
            .equipped
            .values()
            .map(|item| self.item_defs.weight(&item.item_def_id))
            .sum();
        bag_weight + equip_weight
    }

    /// Send an inventory error message to a player.
    async fn send_inventory_error(&self, player_id: &PlayerId, msg: &str) {
        self.send_direct_message(
            player_id,
            ServerMessage::InventoryError {
                message: msg.to_string(),
            },
        )
        .await;
    }

    /// Send the current inventory state directly to a player.
    async fn send_inventory_snapshot(&self, player_id: &PlayerId, inventory: PlayerInventory) {
        self.send_direct_message(player_id, ServerMessage::InventoryUpdated { inventory })
            .await;
    }

    /// Load a player's inventory from the database into memory.
    pub async fn load_player_inventory(
        &self,
        player_id: &PlayerId,
        character_id: i64,
        auth: &AuthService,
    ) {
        let auth = auth.clone();
        let loaded = tokio::task::spawn_blocking(move || auth.load_inventory(character_id))
            .await
            .unwrap_or_else(|e| {
                warn!("spawn_blocking panicked loading inventory: {}", e);
                Err(crate::auth::AuthError::Database(e.to_string()))
            });

        let rows = match loaded {
            Ok(data) => data,
            Err(e) => {
                warn!(
                    "Failed to load inventory for character {}: {}",
                    character_id, e
                );
                return;
            }
        };

        let mut inventory = PlayerInventory::default();

        if !rows.is_empty() {
            let start_id = self.reserve_instance_ids(rows.len() as u64).await;
            let mut next_id = start_id;

            for (item_def_id, quantity, equip_slot) in rows {
                let instance_id = next_id;
                next_id += 1;

                match equip_slot {
                    Some(slot_str) => {
                        if let Ok(slot) = slot_str.parse::<EquipSlot>() {
                            inventory.equipped.insert(
                                slot,
                                ItemInstance {
                                    instance_id,
                                    item_def_id,
                                    quantity: 1,
                                },
                            );
                        } else {
                            warn!(
                                "Unknown equip slot '{}' in DB for character {}",
                                slot_str, character_id
                            );
                        }
                    }
                    None => {
                        inventory.bag.push(ItemInstance {
                            instance_id,
                            item_def_id,
                            quantity,
                        });
                    }
                }
            }
        }

        let mut inventories = self.inventories.write().await;
        inventories.insert(player_id.clone(), inventory);
    }

    pub async fn unload_player_inventory(&self, player_id: &PlayerId) {
        {
            let mut inventories = self.inventories.write().await;
            inventories.remove(player_id);
        }
        let mut dirty = self.dirty_inventories.write().await;
        dirty.remove(player_id);
    }

    pub async fn get_player_inventory(&self, player_id: &PlayerId) -> Option<PlayerInventory> {
        let inventories = self.inventories.read().await;
        inventories.get(player_id).cloned()
    }

    pub(super) async fn mark_inventory_dirty(&self, player_id: &PlayerId) {
        let mut dirty = self.dirty_inventories.write().await;
        dirty.insert(player_id.clone());
    }

    pub async fn give_item(&self, player_id: &PlayerId, item_def_id: &str) -> bool {
        if self.item_defs.get(item_def_id).is_none() {
            warn!("give_item: unknown item_def_id '{}'", item_def_id);
            return false;
        }

        let instance_id = self.next_instance_id().await;
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let inv = match inventories.get_mut(player_id) {
                Some(inv) => inv,
                None => return false,
            };
            inv.bag.push(ItemInstance {
                instance_id,
                item_def_id: item_def_id.to_string(),
                quantity: 1,
            });
            inv.clone()
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        true
    }

    pub async fn equip_item(&self, player_id: &PlayerId, instance_id: u64) {
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let inv = match inventories.get_mut(player_id) {
                Some(inv) => inv,
                None => return,
            };

            let bag_idx = match inv.bag.iter().position(|i| i.instance_id == instance_id) {
                Some(idx) => idx,
                None => {
                    drop(inventories);
                    self.send_inventory_error(player_id, "Item not found in bag")
                        .await;
                    return;
                }
            };

            let item_def_id = inv.bag[bag_idx].item_def_id.clone();
            let equip_slot = match self.item_defs.get(&item_def_id).and_then(|d| d.equip_slot) {
                Some(slot) => slot,
                None => {
                    drop(inventories);
                    self.send_inventory_error(player_id, "This item cannot be equipped")
                        .await;
                    return;
                }
            };

            let target_slot = if inv.equipped.contains_key(&equip_slot) {
                equip_slot
                    .alternate()
                    .filter(|alt| !inv.equipped.contains_key(alt))
                    .unwrap_or(equip_slot)
            } else {
                equip_slot
            };

            let item = inv.bag.remove(bag_idx);
            if let Some(old_item) = inv.equipped.insert(target_slot, item) {
                inv.bag.push(old_item);
            }
            inv.clone()
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
    }

    pub async fn unequip_item(&self, player_id: &PlayerId, slot: EquipSlot) {
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let inv = match inventories.get_mut(player_id) {
                Some(inv) => inv,
                None => return,
            };

            match inv.equipped.remove(&slot) {
                Some(item) => {
                    inv.bag.push(item);
                    inv.clone()
                }
                None => {
                    drop(inventories);
                    self.send_inventory_error(player_id, "No item in that slot")
                        .await;
                    return;
                }
            }
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
    }

    /// Consume a usable item from the bag (e.g. drink a healing potion):
    /// roll its heal dice, restore HP capped at max, decrement the stack and
    /// broadcast the new health to nearby players.
    pub async fn use_item(&self, player_id: &PlayerId, instance_id: u64) {
        // Resolve the item's heal dice before mutating anything.
        let heal_dice = {
            let inventories = self.inventories.read().await;
            let inv = match inventories.get(player_id) {
                Some(inv) => inv,
                None => return,
            };
            let item = match inv.bag.iter().find(|i| i.instance_id == instance_id) {
                Some(item) => item,
                None => {
                    drop(inventories);
                    self.send_inventory_error(player_id, "Item not found in bag")
                        .await;
                    return;
                }
            };
            match self
                .item_defs
                .get(&item.item_def_id)
                .and_then(|d| d.heal_dice().map(str::to_string))
            {
                Some(dice) => dice,
                None => {
                    drop(inventories);
                    self.send_inventory_error(player_id, "This item cannot be used")
                        .await;
                    return;
                }
            }
        };

        // Apply healing to the player, capped at max health. Refuse if the
        // player is defeated or already at full HP (and keep the potion).
        let (health, max_health, position, floor_level) = {
            let mut players = self.players.write().await;
            let player = match players.get_mut(player_id) {
                Some(player) => player,
                None => return,
            };
            if player.health == 0 {
                drop(players);
                self.send_inventory_error(player_id, "You can't drink while defeated")
                    .await;
                return;
            }
            if player.health >= player.max_health {
                drop(players);
                self.send_inventory_error(player_id, "You are already at full health")
                    .await;
                return;
            }
            let amount = crate::game::combat::roll_dice(&heal_dice);
            player.health = (player.health + amount).min(player.max_health);
            (
                player.health,
                player.max_health,
                player.position,
                player.floor_level,
            )
        };

        // Consume one from the stack, removing the instance when it hits zero.
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let inv = match inventories.get_mut(player_id) {
                Some(inv) => inv,
                None => return,
            };
            if let Some(idx) = inv.bag.iter().position(|i| i.instance_id == instance_id) {
                if inv.bag[idx].quantity > 1 {
                    inv.bag[idx].quantity -= 1;
                } else {
                    inv.bag.remove(idx);
                }
            }
            inv.clone()
        };

        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.send_direct_message_to_players_within_position(
            &position,
            floor_level,
            super::AGENT_EVENT_DELIVERY_RADIUS,
            ServerMessage::PlayerHealthUpdate {
                player_id: player_id.clone(),
                health,
                max_health,
            },
            None,
        )
        .await;
    }

    /// Insert a ground item into the world and announce it to nearby players.
    /// `source_monster_id` is set when the item was dropped by a dying monster
    /// so the client can hold the drop until that monster's death plays out.
    pub(super) async fn spawn_ground_item(
        &self,
        ground_item: GroundItem,
        source_monster_id: Option<String>,
    ) {
        let position = ground_item.position;
        let floor_level = ground_item.floor_level;
        {
            let mut ground_items = self.ground_items.write().await;
            ground_items.insert(
                ground_item.instance_id,
                ServerGroundItem {
                    item: ground_item.clone(),
                    dropped_at_ms: Self::now_ms(),
                },
            );
        }
        self.send_direct_message_to_players_within_position(
            &position,
            floor_level,
            super::AGENT_EVENT_DELIVERY_RADIUS,
            ServerMessage::GroundItemSpawned {
                item: ground_item,
                source_monster_id,
            },
            None,
        )
        .await;
    }

    pub async fn drop_item(&self, player_id: &PlayerId, instance_id: u64) {
        let (position, floor_level) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) => (p.position, p.floor_level),
                None => return,
            }
        };

        let (snapshot, item_def_id) = {
            let mut inventories = self.inventories.write().await;
            let inv = match inventories.get_mut(player_id) {
                Some(inv) => inv,
                None => return,
            };

            let def_id =
                if let Some(idx) = inv.bag.iter().position(|i| i.instance_id == instance_id) {
                    inv.bag.remove(idx).item_def_id
                } else if let Some(slot) = inv
                    .equipped
                    .iter()
                    .find(|(_, item)| item.instance_id == instance_id)
                    .map(|(slot, _)| *slot)
                {
                    inv.equipped.remove(&slot).unwrap().item_def_id
                } else {
                    drop(inventories);
                    self.send_inventory_error(player_id, "Item not found").await;
                    return;
                };

            (inv.clone(), def_id)
        };

        let ground_item = GroundItem {
            instance_id,
            item_def_id,
            position,
            floor_level,
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.spawn_ground_item(ground_item, None).await;
    }

    pub async fn debug_drop_item(&self, player_id: &PlayerId, item_def_id: &str) {
        if self.item_defs.get(item_def_id).is_none() {
            self.send_inventory_error(player_id, "Unknown item").await;
            return;
        }

        let (player_position, rotation, floor_level) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) => (p.position, p.rotation, p.floor_level),
                None => return,
            }
        };

        let (landing_angle, landing_distance) = {
            let mut rng = rand::thread_rng();
            (
                rng.gen::<f32>() * std::f32::consts::TAU,
                rng.gen::<f32>().sqrt() * 0.7,
            )
        };
        let position = crate::types::Position {
            x: player_position.x + rotation.sin() + landing_angle.cos() * landing_distance,
            y: player_position.y,
            z: player_position.z + rotation.cos() + landing_angle.sin() * landing_distance,
        };

        let instance_id = self.next_instance_id().await;
        self.spawn_ground_item(
            GroundItem {
                instance_id,
                item_def_id: item_def_id.to_string(),
                position,
                floor_level,
            },
            None,
        )
        .await;
    }

    pub async fn pickup_item(&self, player_id: &PlayerId, instance_id: u64) {
        let (player_pos, player_floor) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) => (p.position, p.floor_level),
                None => return,
            }
        };

        let ground_item = {
            let ground_items = self.ground_items.read().await;
            match ground_items.get(&instance_id) {
                Some(sgi) => sgi.item.clone(),
                None => {
                    self.send_inventory_error(player_id, "Item no longer exists")
                        .await;
                    return;
                }
            }
        };

        let dx = player_pos.x - ground_item.position.x;
        let dz = player_pos.z - ground_item.position.z;
        if dx * dx + dz * dz > MAX_PICKUP_DISTANCE * MAX_PICKUP_DISTANCE {
            self.send_inventory_error(player_id, "Too far away").await;
            return;
        }

        // Exact floor match. Negative floors are dungeon depths now, so
        // the old "-1 matches any floor" wildcard is gone (outdoors and
        // house ground floors are both 0).
        if player_floor != ground_item.floor_level {
            self.send_inventory_error(player_id, "Item is on a different floor")
                .await;
            return;
        }

        // The dungeon coin pile is currency, not a bag item: picking it up
        // credits a few copper to the wallet instead of taking inventory space.
        if ground_item.item_def_id == super::COIN_PILE_ITEM_ID {
            self.pickup_coin_pile(player_id, instance_id, &ground_item, player_floor)
                .await;
            return;
        }

        let item_weight = self.item_defs.weight(&ground_item.item_def_id);
        let max_weight = self.max_carry_weight(player_id).await;

        // Acquire write lock for both weight check and mutation atomically
        let item_position = ground_item.position;
        let snapshot = {
            let mut ground_items = self.ground_items.write().await;
            if ground_items.remove(&instance_id).is_none() {
                self.send_inventory_error(player_id, "Item no longer exists")
                    .await;
                return;
            }

            let mut inventories = self.inventories.write().await;
            if let Some(inv) = inventories.get_mut(player_id) {
                let current_weight = self.calc_total_weight(inv);
                if current_weight + item_weight > max_weight {
                    // Put it back on the ground
                    ground_items.insert(
                        instance_id,
                        ServerGroundItem {
                            item: ground_item,
                            dropped_at_ms: Self::now_ms(),
                        },
                    );
                    drop(inventories);
                    drop(ground_items);
                    self.send_inventory_error(player_id, "Too heavy to carry")
                        .await;
                    return;
                }
                inv.bag.push(ItemInstance {
                    instance_id,
                    item_def_id: ground_item.item_def_id,
                    quantity: 1,
                });
                inv.clone()
            } else {
                return;
            }
        };

        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.send_direct_message_to_players_within_position(
            &item_position,
            player_floor,
            super::AGENT_EVENT_DELIVERY_RADIUS,
            ServerMessage::GroundItemRemoved { instance_id },
            None,
        )
        .await;
    }

    /// Pick up a dungeon coin pile: claim it (first picker wins), credit a
    /// random 1–10 copper to the wallet, then broadcast its removal to nearby
    /// players. Skips the bag/weight path entirely — it's currency, not loot.
    async fn pickup_coin_pile(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        ground_item: &GroundItem,
        player_floor: i8,
    ) {
        // Claim the pile under the ground-items lock so two players racing for
        // the same pile can't both be paid.
        {
            let mut ground_items = self.ground_items.write().await;
            if ground_items.remove(&instance_id).is_none() {
                self.send_inventory_error(player_id, "Item no longer exists")
                    .await;
                return;
            }
        }

        let copper: i64 = rand::thread_rng().gen_range(1..=10);
        let new_gold = {
            let mut gold_map = self.player_gold.write().await;
            let wallet = gold_map.entry(player_id.clone()).or_insert(0);
            *wallet += copper;
            *wallet
        };
        self.mark_dirty(player_id).await;
        self.send_direct_message(player_id, ServerMessage::GoldUpdate { gold: new_gold })
            .await;
        self.send_direct_message(player_id, ServerMessage::GoldGained { amount: copper })
            .await;
        info!(
            "Player {} picked up a coin pile: +{} copper",
            player_id, copper
        );

        self.send_direct_message_to_players_within_position(
            &ground_item.position,
            player_floor,
            super::AGENT_EVENT_DELIVERY_RADIUS,
            ServerMessage::GroundItemRemoved { instance_id },
            None,
        )
        .await;
    }

    pub async fn tick_ground_item_despawn(&self) {
        let now = Self::now_ms();
        let mut to_remove = Vec::new();

        {
            let ground_items = self.ground_items.read().await;
            for (id, sgi) in ground_items.iter() {
                if now.saturating_sub(sgi.dropped_at_ms) > GROUND_ITEM_LIFETIME_MS {
                    to_remove.push(*id);
                }
            }
        }

        if to_remove.is_empty() {
            return;
        }

        let removed_items = {
            let mut ground_items = self.ground_items.write().await;
            to_remove
                .iter()
                .filter_map(|id| {
                    ground_items
                        .remove(id)
                        .map(|sgi| (*id, sgi.item.position, sgi.item.floor_level))
                })
                .collect::<Vec<_>>()
        };

        info!("Despawned {} ground item(s)", removed_items.len());
        for (id, position, floor_level) in removed_items {
            self.send_direct_message_to_players_within_position(
                &position,
                floor_level,
                super::AGENT_EVENT_DELIVERY_RADIUS,
                ServerMessage::GroundItemRemoved { instance_id: id },
                None,
            )
            .await;
        }
    }

    pub async fn collect_dirty_inventory_states(
        &self,
    ) -> Vec<(i64, Vec<(String, u32, Option<String>)>)> {
        let dirty_ids: Vec<PlayerId> = {
            let mut dirty = self.dirty_inventories.write().await;
            dirty.drain().collect()
        };

        if dirty_ids.is_empty() {
            return Vec::new();
        }

        let inventories = self.inventories.read().await;
        let player_chars = self.player_characters.read().await;

        let mut result = Vec::with_capacity(dirty_ids.len());
        for pid in &dirty_ids {
            if let (Some(inv), Some((char_id, _, _))) =
                (inventories.get(pid), player_chars.get(pid))
            {
                result.push((*char_id, serialize_inventory(inv)));
            }
        }

        result
    }

    pub async fn get_inventory_save_data(
        &self,
        player_id: &PlayerId,
    ) -> Option<(i64, Vec<(String, u32, Option<String>)>)> {
        let inventories = self.inventories.read().await;
        let player_chars = self.player_characters.read().await;

        let inv = inventories.get(player_id)?;
        let (char_id, _, _) = player_chars.get(player_id)?;

        Some((*char_id, serialize_inventory(inv)))
    }
}
