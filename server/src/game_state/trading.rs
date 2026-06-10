use crate::merchant_defs::{merchant_defs, MerchantDefinition};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::inventory::ItemInstance;
use tracing::info;

/// Maximum distance between player and merchant for any shop interaction.
const MAX_TRADE_DISTANCE: f32 = 6.0;

impl super::GameState {
    async fn send_trade_error(&self, player_id: &PlayerId, message: &str) {
        self.send_direct_message(
            player_id,
            ServerMessage::TradeError {
                message: message.to_string(),
            },
        )
        .await;
    }

    async fn send_gold_update(&self, player_id: &PlayerId) {
        let gold = self.get_player_gold(player_id).await;
        self.send_direct_message(player_id, ServerMessage::GoldUpdate { gold })
            .await;
    }

    /// Validate that `merchant_player_id` is a merchant NPC within trading
    /// range of the player. Returns the merchant definition on success.
    async fn validate_merchant(
        &self,
        player_id: &PlayerId,
        merchant_player_id: &str,
    ) -> Result<MerchantDefinition, &'static str> {
        let players = self.players.read().await;
        let player = players.get(player_id).ok_or("Player not found")?;
        let merchant = players
            .get(merchant_player_id)
            .ok_or("Merchant not found")?;

        if !merchant.is_npc {
            return Err("That character is not a merchant");
        }
        let def = merchant_defs()
            .get_by_npc_name(&merchant.name)
            .ok_or("This NPC does not trade")?;

        let dx = player.position.x - merchant.position.x;
        let dz = player.position.z - merchant.position.z;
        if dx * dx + dz * dz > MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE {
            return Err("Too far away from the merchant");
        }

        Ok(def.clone())
    }

    pub async fn open_shop(&self, player_id: &PlayerId, merchant_player_id: &str) {
        match self.validate_merchant(player_id, merchant_player_id).await {
            Ok(def) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::ShopState {
                        merchant_player_id: merchant_player_id.to_string(),
                        merchant_name: def.npc_name.clone(),
                        catalog: def.catalog.clone(),
                        sell_rate_percent: def.sell_rate_percent,
                    },
                )
                .await;
                self.send_gold_update(player_id).await;
            }
            Err(reason) => self.send_trade_error(player_id, reason).await,
        }
    }

    /// Buy one unit of `item_def_id` from the merchant at base price.
    /// Merchant stock is unlimited; the item is created from its definition.
    pub async fn buy_item(
        &self,
        player_id: &PlayerId,
        merchant_player_id: &str,
        item_def_id: &str,
    ) {
        let def = match self.validate_merchant(player_id, merchant_player_id).await {
            Ok(def) => def,
            Err(reason) => return self.send_trade_error(player_id, reason).await,
        };

        if !def.sells(item_def_id) {
            return self
                .send_trade_error(player_id, "The merchant does not sell that item")
                .await;
        }

        let Some(price) = self
            .item_defs
            .get(item_def_id)
            .and_then(|item| item.base_price)
        else {
            return self
                .send_trade_error(player_id, "That item has no price")
                .await;
        };

        let item_weight = self.item_defs.weight(item_def_id);
        let max_weight = self.max_carry_weight(player_id).await;
        let instance_id = self.next_instance_id().await;

        // Gold and inventory mutate under both write locks so a concurrent
        // request cannot double-spend between the check and the deduction.
        let snapshot = {
            let mut gold_map = self.player_gold.write().await;
            let Some(gold) = gold_map.get_mut(player_id) else {
                return;
            };
            if *gold < price {
                drop(gold_map);
                return self.send_trade_error(player_id, "Not enough gold").await;
            }

            let mut inventories = self.inventories.write().await;
            let Some(inv) = inventories.get_mut(player_id) else {
                return;
            };
            if self.calc_total_weight(inv) + item_weight > max_weight {
                drop(inventories);
                drop(gold_map);
                return self.send_trade_error(player_id, "Too heavy to carry").await;
            }

            *gold -= price;
            inv.bag.push(ItemInstance {
                instance_id,
                item_def_id: item_def_id.to_string(),
                quantity: 1,
            });
            inv.clone()
        };

        info!(
            "{} bought {} from {} for {}",
            player_id, item_def_id, def.npc_name, price
        );
        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_direct_message(
            player_id,
            ServerMessage::InventoryUpdated {
                inventory: snapshot,
            },
        )
        .await;
        self.send_gold_update(player_id).await;
    }

    /// Sell one unit of a bag item to the merchant. The item is consumed
    /// (merchant stock is abstract) and the player is paid
    /// `base_price * sell_rate_percent / 100`, at least 1.
    pub async fn sell_item(
        &self,
        player_id: &PlayerId,
        merchant_player_id: &str,
        instance_id: u64,
    ) {
        let def = match self.validate_merchant(player_id, merchant_player_id).await {
            Ok(def) => def,
            Err(reason) => return self.send_trade_error(player_id, reason).await,
        };

        let (snapshot, item_def_id, payout) = {
            let mut gold_map = self.player_gold.write().await;
            let Some(gold) = gold_map.get_mut(player_id) else {
                return;
            };

            let mut inventories = self.inventories.write().await;
            let Some(inv) = inventories.get_mut(player_id) else {
                return;
            };

            let Some(idx) = inv.bag.iter().position(|i| i.instance_id == instance_id) else {
                drop(inventories);
                drop(gold_map);
                return self
                    .send_trade_error(player_id, "Item not found in bag")
                    .await;
            };

            let item_def_id = inv.bag[idx].item_def_id.clone();
            let Some(base_price) = self
                .item_defs
                .get(&item_def_id)
                .and_then(|item| item.base_price)
            else {
                drop(inventories);
                drop(gold_map);
                return self
                    .send_trade_error(player_id, "The merchant will not buy that")
                    .await;
            };

            let payout = (base_price * i64::from(def.sell_rate_percent) / 100).max(1);

            if inv.bag[idx].quantity > 1 {
                inv.bag[idx].quantity -= 1;
            } else {
                inv.bag.remove(idx);
            }
            *gold += payout;

            (inv.clone(), item_def_id, payout)
        };

        info!(
            "{} sold {} to {} for {}",
            player_id, item_def_id, def.npc_name, payout
        );
        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_direct_message(
            player_id,
            ServerMessage::InventoryUpdated {
                inventory: snapshot,
            },
        )
        .await;
        self.send_gold_update(player_id).await;
    }
}
