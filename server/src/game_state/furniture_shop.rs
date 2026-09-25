use super::{
    auth_db,
    inventory::{serialize_inventory, stack_into_bag, BagInsert},
    GameState,
};
use crate::{
    auth::AuthService,
    types::{PlayerId, ServerMessage},
};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::furniture_shop::{
    quote, FurnitureOrderLine, FurnitureProduct, FurnitureTip, SHOP,
};

fn find_display<'a>(
    displays: &'a [FurniturePlacement],
    display_id: u32,
    product: &FurnitureProduct,
) -> Option<&'a FurniturePlacement> {
    displays.iter().find(|display| {
        display.id == display_id
            && display.type_id == product.object_type
            && display.floor_level == 0
            && (SHOP.bounds[0]..=SHOP.bounds[2]).contains(&display.x)
            && (SHOP.bounds[1]..=SHOP.bounds[3]).contains(&display.z)
    })
}

impl GameState {
    pub async fn notify_furniture_selection(&self, player_id: &PlayerId, display_id: u32) -> bool {
        let Some(product) = SHOP.products.iter().find(|product| {
            product.display_ids.contains(&display_id)
                && FurnitureTip::for_item(&product.item_def_id).is_some()
        }) else {
            return false;
        };
        let Ok(raw) = self
            .terrain_io
            .read_object(SHOP.region[0], SHOP.region[1])
            .await
        else {
            return false;
        };
        let Ok(displays) = Self::parse_region_furniture(&raw) else {
            return false;
        };
        let Some(display) = find_display(&displays, display_id, product) else {
            return false;
        };
        let (clerk_id, player_name) = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                return false;
            };
            if player.is_official_npc
                || player.health == 0
                || player.floor_level != 0
                || !(0.0..=4.0).contains(&player.position.y)
                || (player.position.x - display.x).hypot(player.position.z - display.z) > 3.5
            {
                return false;
            }
            let Some(clerk) = players.values().find(|npc| {
                npc.is_official_npc
                    && npc.name == SHOP.clerk_npc_name
                    && npc.health > 0
                    && npc.floor_level == 0
                    && (0.0..=4.0).contains(&npc.position.y)
                    && (SHOP.bounds[0]..=SHOP.bounds[2]).contains(&npc.position.x)
                    && (SHOP.bounds[1]..=SHOP.bounds[3]).contains(&npc.position.z)
                    && !self.is_npc_asleep(&npc.name)
            }) else {
                return false;
            };
            (clerk.id, player.name.clone())
        };
        self.send_direct_message(
            &clerk_id,
            ServerMessage::FurnitureSelectionNotice {
                player_id: *player_id,
                player_name,
                item_def_id: product.item_def_id.clone(),
            },
        )
        .await;
        true
    }

    pub async fn checkout_furniture(
        &self,
        player_id: &PlayerId,
        items: Vec<FurnitureOrderLine>,
        expected_gold: i64,
        expected_total: i64,
        auth: &AuthService,
    ) {
        let error = self
            .try_checkout_furniture(player_id, &items, expected_gold, expected_total, auth)
            .await
            .err()
            .map(str::to_string);
        self.send_direct_message(player_id, ServerMessage::FurniturePurchaseResult { error })
            .await;
    }

    async fn try_checkout_furniture(
        &self,
        player_id: &PlayerId,
        items: &[FurnitureOrderLine],
        expected_gold: i64,
        expected_total: i64,
        auth: &AuthService,
    ) -> Result<(), &'static str> {
        let total = quote(items)?;
        if total != expected_total {
            return Err("Prices changed. Review your basket before paying.");
        }
        let order = items
            .iter()
            .map(|line| {
                SHOP.products
                    .iter()
                    .find(|product| product.display_ids.contains(&line.display_id))
                    .map(|product| (line, product))
                    .ok_or("That display is not for sale.")
            })
            .collect::<Result<Vec<_>, _>>()?;
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "buy furniture").await {
            return Err("Finish your player trade first.");
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or("Character not found.")?;
        let clerk_id = {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("Character not found.")?;
            if player.health == 0
                || player.floor_level != 0
                || player.position.x < SHOP.bounds[0] - 4.0
                || player.position.x > SHOP.bounds[2] + 2.0
                || player.position.z < SHOP.bounds[1] - 2.0
                || player.position.z > SHOP.bounds[3] + 2.0
                || !(0.0..=4.0).contains(&player.position.y)
            {
                return Err("Visit Grida at ORKEA to pay for your furniture.");
            }
            players
                .values()
                .find(|npc| npc.is_official_npc && npc.name == SHOP.clerk_npc_name)
                .map(|npc| npc.id)
                .ok_or("Grida is not available to check out your furniture.")?
        };
        self.validate_trader(player_id, &clerk_id).await?;
        let raw = self
            .terrain_io
            .read_object(SHOP.region[0], SHOP.region[1])
            .await
            .map_err(|_| "The showroom is temporarily unavailable.")?;
        let displays = Self::parse_region_furniture(&raw)
            .map_err(|_| "The showroom is temporarily unavailable.")?;
        for (line, product) in &order {
            if find_display(&displays, line.display_id, product).is_none() {
                return Err(
                    "A selected display is no longer available. Remove it from your basket.",
                );
            }
        }
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let mut next_id = self
            .reserve_instance_ids(items.iter().map(|i| u64::from(i.quantity)).sum())
            .await;
        let mut inventories = self.inventories.write().await;
        let inventory = inventories
            .get_mut(player_id)
            .ok_or("Inventory not found.")?;
        let mut gold = self.player_gold.write().await;
        let balance = gold.get_mut(player_id).ok_or("Gold balance not found.")?;
        if *balance != expected_gold {
            return Err("Your gold balance changed. Review your basket and try again.");
        }
        if *balance < total {
            return Err("Not enough gold.");
        }
        let mut updated = inventory.clone();
        for (line, product) in &order {
            for _ in 0..line.quantity {
                stack_into_bag(
                    &mut updated.bag,
                    BagInsert::one(false, &product.item_def_id, 0, next_id),
                );
                next_id += 1;
            }
        }
        if self.calc_total_weight(&updated, armor_mult) > max_weight {
            return Err("Your bag is too heavy. Remove some furniture or make room first.");
        }
        character.gold = *balance - total;
        let rows = serialize_inventory(&updated);
        let auth = auth.clone();
        auth_db(move || {
            auth.save_batch(
                std::slice::from_ref(&character),
                &[(character.character_id, rows)],
                &[],
                &[],
                None,
            )
        })
        .await
        .map_err(|error| {
            tracing::warn!(%error, "Furniture checkout failed");
            "Payment could not be saved. Your gold and inventory were not changed."
        })?;
        *balance -= total;
        *inventory = updated.clone();
        drop(gold);
        drop(inventories);
        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, updated).await;
        self.send_gold_update(player_id).await;
        for (line, product) in &order {
            self.record_gold_sink(
                crate::metrics::GoldSink::ItemPurchase {
                    item_def_id: product.item_def_id.clone(),
                },
                line.quantity,
                product.price * i64::from(line.quantity),
            )
            .await;
        }
        Ok(())
    }
}
