use super::*;

impl GameState {
    pub async fn set_stall_buy_order(
        &self,
        player_id: &PlayerId,
        item_def_id: String,
        quantity: u32,
        enchant: i32,
        unit_price: i64,
    ) {
        if quantity == 0 || unit_price < 0 || enchant < 0 {
            return;
        }
        let Some(def) = self.item_defs.get(&item_def_id) else {
            self.send_system_message(player_id, "Unknown item.").await;
            return;
        };
        if self.item_defs.untradeable(&item_def_id)
            || (enchant != 0 && !matches!(def.category.as_deref(), Some("weapon" | "armor")))
        {
            self.send_system_message(player_id, "That item cannot be requested.")
                .await;
            return;
        }
        if self
            .players
            .read()
            .await
            .get(player_id)
            .is_none_or(|p| p.is_official_npc)
        {
            return;
        }
        let Some(cost) = unit_price.checked_mul(i64::from(quantity)) else {
            self.send_system_message(player_id, "That order costs too much.")
                .await;
            return;
        };
        let gold = self.get_player_gold(player_id).await;
        let order_id = self.next_instance_id().await;
        let result = {
            let mut stalls = self.stalls.write().await;
            (|| {
                let entry = stalls
                    .get_mut(player_id)
                    .ok_or("You have no stall laid out.")?;
                let existing = entry
                    .buy_orders
                    .iter()
                    .position(|order| order.item_def_id == item_def_id && order.enchant == enchant);
                if existing.is_none()
                    && entry.listings.len() + entry.buy_orders.len() >= STALL_MAX_LISTINGS
                {
                    return Err("Your stall has no room for another listing.");
                }
                let budget = entry
                    .buy_orders
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| Some(*index) != existing)
                    .try_fold(cost, |sum, (_, order)| {
                        order
                            .unit_price
                            .checked_mul(i64::from(order.quantity))
                            .and_then(|amount| sum.checked_add(amount))
                    })
                    .ok_or("That order costs too much.")?;
                if budget > gold {
                    return Err("You don't have enough gold to cover your buy orders.");
                }
                let order = StallBuyOrder {
                    order_id,
                    item_def_id,
                    quantity,
                    enchant,
                    unit_price,
                };
                match existing {
                    Some(index) => entry.buy_orders[index] = order,
                    None => entry.buy_orders.push(order),
                }
                Ok(())
            })()
        };
        if let Err(reason) = result {
            self.send_system_message(player_id, reason).await;
        }
        self.push_stall_state(player_id, None).await;
    }

    pub async fn remove_stall_buy_order(&self, player_id: &PlayerId, order_id: u64) {
        if let Some(entry) = self.stalls.write().await.get_mut(player_id) {
            entry.buy_orders.retain(|order| order.order_id != order_id);
        }
        self.push_stall_state(player_id, None).await;
    }

    pub async fn sell_to_stall(
        &self,
        player_id: &PlayerId,
        stall_id: u64,
        order_id: u64,
        instance_id: u64,
        quantity: u32,
        auth: &AuthService,
    ) {
        let _persistence = self.persistence_lock.lock().await;
        if quantity == 0 {
            return;
        }
        let Some(owner) = self.owner_of_stall(stall_id).await else {
            self.send_system_message(player_id, "That stall is gone.")
                .await;
            return;
        };
        if owner == *player_id {
            self.send_system_message(player_id, "That's your own stall.")
                .await;
            return;
        }
        let customer_name = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(customer)
                    if !customer.is_official_npc
                        && players
                            .get(&owner)
                            .is_none_or(|owner| !owner.is_official_npc) =>
                {
                    customer.name.clone()
                }
                _ => {
                    drop(players);
                    self.send_system_message(player_id, "Stalls are for player travelers.")
                        .await;
                    return;
                }
            }
        };
        if !self.customer_at_stall(player_id, &owner).await {
            self.send_system_message(player_id, "Step up to the stall first.")
                .await;
            return;
        }
        if self.stall_owner_has_blocked(&owner, &customer_name).await {
            self.send_system_message(player_id, BUSY).await;
            return;
        }
        if self
            .reject_if_trade_reserved(player_id, instance_id, "sell")
            .await
            || self
                .reject_if_holding_up_stall(player_id, instance_id, "sell")
                .await
        {
            return;
        }
        let item = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inv| inv.bag.iter().find(|item| item.instance_id == instance_id))
            .cloned();
        let Some(item) = item else {
            self.send_system_message(player_id, "You no longer have that item.")
                .await;
            return;
        };
        if item.locked
            || self.item_defs.untradeable(&item.item_def_id)
            || (item.item_def_id == "tip_hat" && self.tip_hats.read().await.contains_key(player_id))
        {
            self.send_system_message(player_id, "That item cannot be sold.")
                .await;
            return;
        }
        let taken = {
            let mut stalls = self.stalls.write().await;
            (|| {
                let entry = stalls
                    .get_mut(&owner)
                    .filter(|entry| entry.stall.id == stall_id)
                    .ok_or("That stall is gone.")?;
                let order = entry
                    .buy_orders
                    .iter_mut()
                    .find(|order| order.order_id == order_id)
                    .ok_or("That buy order has changed or is gone.")?;
                if order.item_def_id != item.item_def_id || order.enchant != item.enchant {
                    return Err("That item doesn't match the buy order.");
                }
                if quantity > order.quantity || quantity > item.quantity {
                    return Err("There aren't that many left to trade.");
                }
                let total = order
                    .unit_price
                    .checked_mul(i64::from(quantity))
                    .ok_or("That sale costs too much.")?;
                let listing = StallListing {
                    instance_id,
                    item_def_id: item.item_def_id.clone(),
                    quantity,
                    enchant: item.enchant,
                    unit_price: order.unit_price,
                };
                order.quantity -= quantity;
                Ok((listing, total))
            })()
        };
        let (listing, total) = match taken {
            Ok(taken) => taken,
            Err(reason) => {
                self.send_system_message(player_id, reason).await;
                self.push_stall_state(&owner, Some(player_id)).await;
                return;
            }
        };
        let result = self
            .settle_stall_sale(&owner, player_id, &[(listing, quantity)], total, auth)
            .await;
        {
            let mut stalls = self.stalls.write().await;
            if let Some(entry) = stalls
                .get_mut(&owner)
                .filter(|entry| entry.stall.id == stall_id)
            {
                if result.is_err() {
                    if let Some(order) = entry
                        .buy_orders
                        .iter_mut()
                        .find(|order| order.order_id == order_id)
                    {
                        order.quantity += quantity;
                    }
                }
                entry
                    .buy_orders
                    .retain(|order| order.order_id != order_id || order.quantity > 0);
            }
        }
        if let Err(reason) = result {
            self.send_system_message(player_id, reason).await;
        }
        self.push_stall_state(&owner, None).await;
    }
}
