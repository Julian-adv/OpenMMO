//! Stalls support unattended trading after logout.

mod buy_orders;
mod offline;

use onlinerpg_shared::character::CharacterClass;
use onlinerpg_shared::messages::StallBuyLine;
use onlinerpg_shared::stall::{
    stall_tax, Stall, StallBuyOrder, StallListing, STALL_MAX_LISTINGS, STALL_MAX_SIGN_CHARS,
};
use onlinerpg_shared::{PlayerId, ServerMessage};
use std::collections::HashSet;
use tracing::{error, info};

use super::combat::reachable_dist_sq;
use super::inventory::{serialize_inventory, stack_into_bag, BagInsert};
use super::player_trade::{format_copper, take_from_bag};
use super::trading::MAX_TRADE_DISTANCE;
use super::{auth_db, GameState};
use crate::auth::{AuthService, TradeLedgerEntry};

/// Shown instead of a refusal whenever the owner has the customer blocked, so
/// the block stays invisible (doc/TRADE.md).
const BUSY: &str = "The stallholder is busy with someone else.";

/// A stall and the goods on it. Listings live here rather than on the wire
/// `Stall` so they never ride the AOI broadcast.
pub(super) struct StallEntry {
    pub stall: Stall,
    /// Deployed bag instance, locked against transfers. Zero for NPC stalls.
    pub placed_with: u64,
    pub listings: Vec<StallListing>,
    pub buy_orders: Vec<StallBuyOrder>,
    offline_owner: Option<offline::OfflineStallOwner>,
    /// Who has the panel open. All of them get the whole state on any change.
    pub viewers: HashSet<PlayerId>,
}

impl StallEntry {
    fn listing(&self, instance_id: u64) -> Option<&StallListing> {
        self.listings
            .iter()
            .find(|listing| listing.instance_id == instance_id)
    }
}

impl GameState {
    /// `/lay_stall` — NPC merchants only. Players use the item instead, so
    /// there is one entry per audience rather than two per behaviour.
    pub(super) async fn lay_stall(&self, player_id: &PlayerId) {
        let is_merchant = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) => p.class == CharacterClass::Merchant,
                None => return,
            }
        };
        if !is_merchant {
            self.send_system_message(player_id, "Only a merchant can lay out a stall.")
                .await;
            return;
        }
        if self.stall_of(player_id).await.is_some() {
            self.send_system_message(player_id, "Your stall is already laid out.")
                .await;
            return;
        }
        self.place_stall(player_id, 0).await;
    }

    /// The `peddler_stall` item: lay the table out, or fold it back up. The
    /// item itself is never spent (`tip_hat` does the same).
    pub(super) async fn toggle_stall(&self, player_id: &PlayerId, instance_id: u64) {
        if self.remove_player_stall(player_id).await {
            self.send_system_message(player_id, "You pack up your stall.")
                .await;
            return;
        }
        if self
            .reject_if_defeated(player_id, "You can't lay a stall out while defeated")
            .await
        {
            return;
        }
        self.place_stall(player_id, instance_id).await;
    }

    async fn place_stall(&self, player_id: &PlayerId, placed_with: u64) {
        let Some((placement, floor_level)) = self
            .outdoor_placement(
                player_id,
                super::inventory::PLACEMENT_DISTANCE_M,
                "You can only lay out a stall outdoors",
                "You can't lay out a stall in water",
            )
            .await
        else {
            return;
        };
        let Some((owner_name, rotation)) = ({
            let players = self.players.read().await;
            players.get(player_id).map(|p| (p.name.clone(), p.rotation))
        }) else {
            return;
        };
        let stall = Stall {
            id: self.next_instance_id().await,
            owner: *player_id,
            position: placement,
            rotation,
            floor_level,
            owner_name,
            sign: String::new(),
        };
        self.stalls.write().await.insert(
            *player_id,
            StallEntry {
                stall: stall.clone(),
                placed_with,
                listings: Vec::new(),
                buy_orders: Vec::new(),
                offline_owner: None,
                viewers: HashSet::new(),
            },
        );
        self.broadcast_stall(&stall, |stall| ServerMessage::StallPlaced { stall })
            .await;
        self.send_system_message(player_id, "You lay out your stall.")
            .await;
    }

    /// `/pack_stall` — fold the table back up.
    pub(super) async fn pack_stall(&self, player_id: &PlayerId) {
        if self.remove_player_stall(player_id).await {
            self.send_system_message(player_id, "You pack up your stall.")
                .await;
        } else {
            self.send_system_message(player_id, "You have no stall laid out.")
                .await;
        }
    }

    /// The owner wandered off, changed floor, or died and respawned elsewhere.
    pub(super) async fn pack_up_strayed_stall(&self, player_id: &PlayerId) {
        if self.remove_player_stall(player_id).await {
            self.send_system_message(player_id, "You leave your stall behind and pack it up.")
                .await;
        }
    }

    /// Remove the stall and notify the area. Unsold goods remain in the bag.
    pub(super) async fn remove_player_stall(&self, player_id: &PlayerId) -> bool {
        let Some(entry) = self.stalls.write().await.remove(player_id) else {
            return false;
        };
        // Viewers stand at the table, so the area broadcast already reaches them.
        self.publish_nearby(
            &entry.stall.position,
            entry.stall.floor_level,
            ServerMessage::StallRemoved {
                stall_id: entry.stall.id,
            },
            None,
        )
        .await;
        true
    }

    /// The stall standing in front of `player_id`, if they have one out.
    pub(super) async fn stall_of(&self, player_id: &PlayerId) -> Option<Stall> {
        self.stalls
            .read()
            .await
            .get(player_id)
            .map(|entry| entry.stall.clone())
    }

    /// Listed units reserved against other inventory actions.
    pub(super) async fn stall_reserved_quantity(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
    ) -> u32 {
        self.stalls
            .read()
            .await
            .get(player_id)
            .and_then(|entry| entry.listing(instance_id))
            .map(|listing| listing.quantity)
            .unwrap_or(0)
    }

    /// Refuse to part with the item currently holding a table up. Not folded
    /// into the reservation guard because `use` is exactly how a stall is
    /// packed away — only the paths that would orphan it ask.
    pub(super) async fn reject_if_holding_up_stall(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        action: &str,
    ) -> bool {
        let deployed = self
            .stalls
            .read()
            .await
            .get(player_id)
            .is_some_and(|entry| entry.placed_with == instance_id);
        if deployed {
            self.send_system_message(
                player_id,
                &format!("Pack your stall up before you {action} it."),
            )
            .await;
        }
        deployed
    }

    /// Whether goods are priced up on this player's stall. Def-keyed bulk
    /// draws cannot be reconciled against instance-level reservations, so
    /// they are refused outright while any listing stands.
    pub(super) async fn stall_has_listings(&self, player_id: &PlayerId) -> bool {
        self.stalls
            .read()
            .await
            .get(player_id)
            .is_some_and(|entry| !entry.listings.is_empty())
    }

    /// Instances this player cannot part with while their stall is out: every
    /// listing, plus the item holding the table up.
    pub(super) async fn stall_locked_instances(&self, player_id: &PlayerId) -> HashSet<u64> {
        let stalls = self.stalls.read().await;
        let Some(entry) = stalls.get(player_id) else {
            return HashSet::new();
        };
        entry
            .listings
            .iter()
            .map(|listing| listing.instance_id)
            .chain(std::iter::once(entry.placed_with))
            .collect()
    }

    /// Step up to a stall: an NPC's opens their shop, a player's opens the
    /// consignment panel. Refusals go to chat — there is no panel to show
    /// them in yet.
    pub async fn open_stall(&self, player_id: &PlayerId, stall_id: u64) {
        let Some(owner) = self.owner_of_stall(stall_id).await else {
            self.send_system_message(player_id, "That stall is gone.")
                .await;
            return;
        };
        let (customer, owner_is_npc) = {
            let players = self.players.read().await;
            (
                players
                    .get(player_id)
                    .map(|p| (p.name.clone(), p.is_official_npc)),
                players.get(&owner).map(|p| p.is_official_npc),
            )
        };
        let Some((customer_name, customer_is_npc)) = customer else {
            return;
        };
        match owner_is_npc {
            None if !self.stall_owner_is_offline(&owner).await => {
                self.send_system_message(player_id, "That stall is gone.")
                    .await;
                return;
            }
            // An NPC's stall is their shop front: same intent for the customer,
            // a different validation pipeline behind it.
            Some(true) => {
                self.open_shop(player_id, &owner, true).await;
                return;
            }
            Some(false) | None => {}
        }
        if owner != *player_id {
            if customer_is_npc {
                self.send_system_message(player_id, "Stalls are for player travelers.")
                    .await;
                return;
            }
            if !self.customer_at_stall(player_id, &owner).await {
                self.send_system_message(player_id, "Step up to the stall first.")
                    .await;
                return;
            }
            if self.stall_owner_has_blocked(&owner, &customer_name).await {
                self.send_system_message(player_id, BUSY).await;
                return;
            }
        }
        {
            let mut stalls = self.stalls.write().await;
            if stalls
                .get(&owner)
                .is_none_or(|entry| entry.stall.id != stall_id)
            {
                return;
            }
            for (id, entry) in stalls.iter_mut() {
                if *id == owner {
                    entry.viewers.insert(*player_id);
                } else {
                    entry.viewers.remove(player_id);
                }
            }
        }
        self.push_stall_state(&owner, Some(player_id)).await;
    }

    /// Stop pushing listing changes at a player who closed the panel.
    pub async fn close_stall(&self, player_id: &PlayerId) {
        let mut stalls = self.stalls.write().await;
        for entry in stalls.values_mut() {
            entry.viewers.remove(player_id);
        }
    }

    pub async fn set_stall_sign(&self, player_id: &PlayerId, sign: String) {
        let sign = sign.trim().to_string();
        if sign.chars().count() > STALL_MAX_SIGN_CHARS {
            self.send_system_message(
                player_id,
                format!("A sign fits {STALL_MAX_SIGN_CHARS} characters."),
            )
            .await;
            return;
        }
        let name = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) => p.name.clone(),
                None => return,
            }
        };
        // A mute takes the board away with the voice; the sign is a broadcast.
        if !sign.is_empty()
            && self
                .refuse_if_muted(player_id, &name, "put up a sign")
                .await
        {
            return;
        }
        let stall = {
            let mut stalls = self.stalls.write().await;
            let Some(entry) = stalls.get_mut(player_id) else {
                drop(stalls);
                self.send_system_message(player_id, "You have no stall laid out.")
                    .await;
                return;
            };
            if entry.stall.sign == sign {
                return;
            }
            entry.stall.sign = sign;
            entry.stall.clone()
        };
        self.broadcast_stall(&stall, |stall| ServerMessage::StallSignChanged {
            stall_id: stall.id,
            sign: stall.sign,
        })
        .await;
        self.push_stall_state(player_id, None).await;
    }

    /// Put goods on the table, or re-price what is already there.
    pub async fn list_stall_item(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        quantity: u32,
        unit_price: i64,
    ) {
        if quantity == 0 || unit_price < 0 {
            return;
        }
        if self
            .reject_if_trade_reserved(player_id, instance_id, "put on a stall")
            .await
        {
            return;
        }
        let refusal = {
            let stalls = self.stalls.read().await;
            match stalls.get(player_id) {
                None => Some("You have no stall laid out.".to_string()),
                Some(entry) if entry.placed_with == instance_id => {
                    Some("You're doing business on that stall.".to_string())
                }
                Some(entry)
                    if entry.listing(instance_id).is_none()
                        && entry.listings.len() + entry.buy_orders.len() >= STALL_MAX_LISTINGS =>
                {
                    Some(format!(
                        "A stall holds {STALL_MAX_LISTINGS} kinds of goods."
                    ))
                }
                Some(_) => None,
            }
        };
        if let Some(reason) = refusal {
            self.send_system_message(player_id, reason).await;
            return;
        }
        let resolved = {
            let inventories = self.inventories.read().await;
            let item = inventories
                .get(player_id)
                .and_then(|inv| inv.bag.iter().find(|i| i.instance_id == instance_id));
            match item {
                None => Err("You no longer have that item.".to_string()),
                Some(item) if self.item_defs.untradeable(&item.item_def_id) => {
                    let name = self
                        .item_defs
                        .get(&item.item_def_id)
                        .map(|def| def.name.as_str())
                        .unwrap_or("That item");
                    Err(format!("{name} cannot be sold on."))
                }
                Some(item) if item.locked => Err(super::inventory::LOCKED_ITEM_MESSAGE.to_string()),
                Some(item) if item.quantity < quantity => {
                    Err("You don't have that many.".to_string())
                }
                Some(item) => Ok(StallListing {
                    instance_id,
                    item_def_id: item.item_def_id.clone(),
                    quantity,
                    enchant: item.enchant,
                    unit_price,
                }),
            }
        };
        let listing = match resolved {
            Ok(listing) => listing,
            Err(reason) => {
                self.send_system_message(player_id, reason).await;
                return;
            }
        };
        {
            let mut stalls = self.stalls.write().await;
            let Some(entry) = stalls.get_mut(player_id) else {
                return;
            };
            match entry
                .listings
                .iter_mut()
                .find(|l| l.instance_id == instance_id)
            {
                Some(existing) => *existing = listing,
                None => entry.listings.push(listing),
            }
        }
        self.push_stall_state(player_id, None).await;
    }

    pub async fn unlist_stall_item(&self, player_id: &PlayerId, instance_id: u64) {
        {
            let mut stalls = self.stalls.write().await;
            let Some(entry) = stalls.get_mut(player_id) else {
                return;
            };
            entry.listings.retain(|l| l.instance_id != instance_id);
        }
        self.push_stall_state(player_id, None).await;
    }

    /// Reserve every cart line before settling the purchase atomically.
    pub async fn buy_from_stall(
        &self,
        player_id: &PlayerId,
        stall_id: u64,
        lines: Vec<StallBuyLine>,
        auth: &AuthService,
    ) {
        let _persistence = self.persistence_lock.lock().await;
        let lines: Vec<StallBuyLine> = lines.into_iter().filter(|l| l.quantity > 0).collect();
        if lines.is_empty() || lines.len() > STALL_MAX_LISTINGS {
            return;
        }
        let mut seen = HashSet::new();
        if lines.iter().any(|line| !seen.insert(line.instance_id)) {
            self.send_system_message(player_id, "Choose each listing only once.")
                .await;
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
                Some(p) if p.is_official_npc => {
                    drop(players);
                    self.send_system_message(player_id, "Stalls are for player travelers.")
                        .await;
                    return;
                }
                Some(p) => p.name.clone(),
                None => return,
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

        let taken = {
            let mut stalls = self.stalls.write().await;
            (|| {
                let entry = stalls
                    .get_mut(&owner)
                    .filter(|entry| entry.stall.id == stall_id)
                    .ok_or("That stall is gone.")?;
                let mut sold = Vec::with_capacity(lines.len());
                let mut total = 0_i64;
                for line in &lines {
                    let listing = entry
                        .listing(line.instance_id)
                        .filter(|listing| listing.quantity >= line.quantity)
                        .ok_or("That's already sold.")?;
                    total = listing
                        .unit_price
                        .checked_mul(i64::from(line.quantity))
                        .and_then(|price| total.checked_add(price))
                        .ok_or("That purchase costs too much.")?;
                    sold.push((listing.clone(), line.quantity));
                }
                for listing in &mut entry.listings {
                    if let Some(line) = lines
                        .iter()
                        .find(|line| line.instance_id == listing.instance_id)
                    {
                        listing.quantity -= line.quantity;
                    }
                }
                entry.listings.retain(|listing| listing.quantity > 0);
                Ok::<_, &str>((sold, total))
            })()
        };
        let (sold, total) = match taken {
            Ok(taken) => taken,
            Err(reason) => {
                self.send_system_message(player_id, reason).await;
                self.push_stall_state(&owner, None).await;
                return;
            }
        };

        if let Err(reason) = self
            .settle_stall_sale(player_id, &owner, &sold, total, auth)
            .await
        {
            // The units never left the bag; hand them back to the table.
            let mut stalls = self.stalls.write().await;
            if let Some(entry) = stalls
                .get_mut(&owner)
                .filter(|entry| entry.stall.id == stall_id)
            {
                for (listing, qty) in &sold {
                    match entry
                        .listings
                        .iter_mut()
                        .find(|l| l.instance_id == listing.instance_id)
                    {
                        Some(back) => back.quantity += qty,
                        None => entry.listings.push(StallListing {
                            quantity: *qty,
                            ..listing.clone()
                        }),
                    }
                }
            }
            drop(stalls);
            self.send_system_message(player_id, reason).await;
        }
        self.push_stall_state(&owner, None).await;
    }

    /// Called with the persistence lock held by the purchase or sale.
    async fn settle_stall_sale(
        &self,
        buyer: &PlayerId,
        seller: &PlayerId,
        sold: &[(StallListing, u32)],
        total: i64,
        auth: &AuthService,
    ) -> Result<(), &'static str> {
        let tax = stall_tax(total);
        let units: u64 = sold
            .iter()
            .map(|(listing, qty)| {
                if self.item_defs.stackable(&listing.item_def_id) {
                    1
                } else {
                    u64::from(*qty)
                }
            })
            .sum();
        let mut next_id = self.reserve_instance_ids(units).await;
        let offline_capacity = self
            .stalls
            .read()
            .await
            .get(buyer)
            .and_then(|entry| entry.offline_owner.as_ref())
            .map(|owner| owner.carry_capacity);
        let (capacity, armor_mult) = match offline_capacity {
            Some(capacity) => (capacity, 1.0),
            None => (
                self.max_carry_weight(buyer).await,
                self.armor_weight_mult(buyer).await,
            ),
        };
        let incoming_weight: f32 = sold
            .iter()
            .map(|(listing, qty)| {
                self.item_defs.weight_with(&listing.item_def_id, armor_mult) * *qty as f32
            })
            .sum();
        let mut buyer_save = self
            .stall_trader_save_data(buyer)
            .await
            .ok_or("The sale could not be completed.")?;
        let mut seller_save = self
            .stall_trader_save_data(seller)
            .await
            .ok_or("The sale could not be completed.")?;

        let mut stalls = self.stalls.write().await;
        let mut gold = self.player_gold.write().await;
        let mut inventories = self.inventories.write().await;
        let snapshot = |id: &PlayerId| {
            if let Some(owner) = stalls
                .get(id)
                .and_then(|entry| entry.offline_owner.as_ref())
            {
                Some((owner.character.gold, owner.inventory.clone()))
            } else {
                Some((*gold.get(id)?, inventories.get(id)?.clone()))
            }
        };
        let (buyer_gold_before, mut buyer_inv) =
            snapshot(buyer).ok_or("The sale could not be completed.")?;
        let (seller_gold_before, mut seller_inv) =
            snapshot(seller).ok_or("The sale could not be completed.")?;
        if buyer_gold_before < total {
            return Err("The buyer can't afford that.");
        }
        buyer_save.gold = buyer_gold_before - total;
        seller_save.gold = seller_gold_before
            .checked_add(total - tax)
            .ok_or("The seller can't hold that much gold.")?;
        if self.calc_total_weight(&buyer_inv, armor_mult) + incoming_weight > capacity {
            return Err("The buyer can't carry that much.");
        }
        for (listing, qty) in sold {
            let item = seller_inv
                .bag
                .iter()
                .find(|item| {
                    item.instance_id == listing.instance_id
                        && !item.locked
                        && item.item_def_id == listing.item_def_id
                        && item.enchant == listing.enchant
                        && item.quantity >= *qty
                })
                .cloned()
                .ok_or("The seller no longer has that.")?;
            if !take_from_bag(&mut seller_inv, listing.instance_id, *qty) {
                return Err("The seller no longer has that.");
            }
            next_id += stack_into_bag(
                &mut buyer_inv.bag,
                BagInsert {
                    locked: false,
                    stackable: self.item_defs.stackable(&listing.item_def_id),
                    item_def_id: &listing.item_def_id,
                    enchant: listing.enchant,
                    cape_color: item.cape_color,
                    cape_texture: item.cape_texture,
                    first_instance_id: next_id,
                    quantity: *qty,
                },
            )
            .ids_used;
        }

        let ledger = TradeLedgerEntry {
            a_character_id: buyer_save.character_id,
            b_character_id: seller_save.character_id,
            a_gold_before: buyer_gold_before,
            a_gold_after: buyer_save.gold,
            b_gold_before: seller_gold_before,
            b_gold_after: seller_save.gold,
            a_items: "[]".to_string(),
            b_items: serde_json::Value::Array(
                sold.iter()
                    .map(|(listing, qty)| {
                        serde_json::json!({
                            "def": listing.item_def_id,
                            "qty": qty,
                            "ench": listing.enchant,
                        })
                    })
                    .collect(),
            )
            .to_string(),
        };
        let rows = vec![
            (buyer_save.character_id, serialize_inventory(&buyer_inv)),
            (seller_save.character_id, serialize_inventory(&seller_inv)),
        ];
        let characters = vec![buyer_save.clone(), seller_save.clone()];
        let auth = auth.clone();
        if let Err(error) = auth_db(move || auth.commit_trade(&characters, &rows, &ledger)).await {
            error!("Stall sale commit failed: {error}");
            return Err("The sale could not be completed.");
        }
        for (id, character, inventory) in [
            (buyer, buyer_save, buyer_inv),
            (seller, seller_save, seller_inv),
        ] {
            if let Some(owner) = stalls
                .get_mut(id)
                .and_then(|entry| entry.offline_owner.as_mut())
            {
                owner.character = character;
                owner.inventory = inventory;
                owner.dirty = false;
            } else {
                gold.insert(*id, character.gold);
                inventories.insert(*id, inventory);
            }
        }
        drop(inventories);
        drop(gold);
        drop(stalls);
        self.record_gold_sink(crate::metrics::GoldSink::StallTax, 1, tax)
            .await;
        self.send_gold_update(buyer).await;
        self.send_gold_update(seller).await;
        self.push_inventory_update(buyer).await;
        self.push_inventory_update(seller).await;
        self.report_stall_sale(buyer, seller, sold, total, tax)
            .await;
        Ok(())
    }

    /// The seller's receipt includes the tax.
    async fn report_stall_sale(
        &self,
        buyer: &PlayerId,
        seller: &PlayerId,
        sold: &[(StallListing, u32)],
        total: i64,
        tax: i64,
    ) {
        let describe = |listing: &StallListing, qty: u32| -> String {
            let name = self.item_name(&listing.item_def_id);
            let name = if listing.enchant != 0 {
                format!("+{} {name}", listing.enchant)
            } else {
                name
            };
            if qty > 1 {
                format!("{name} x{qty}")
            } else {
                name
            }
        };
        let goods = sold
            .iter()
            .map(|(listing, qty)| describe(listing, *qty))
            .collect::<Vec<_>>()
            .join(", ");
        let (Some(buyer_name), Some(seller_name)) = (
            self.stall_trader_name(buyer).await,
            self.stall_trader_name(seller).await,
        ) else {
            return;
        };
        self.send_system_message(
            buyer,
            format!(
                "You buy {goods} from {seller_name} for {}.",
                format_copper(total)
            ),
        )
        .await;
        self.send_system_message(
            seller,
            format!(
                "{buyer_name} buys {goods} for {}. You keep {} after {} tax.",
                format_copper(total),
                format_copper(total - tax),
                format_copper(tax)
            ),
        )
        .await;
        // Def ids and raw copper, so an economy sweep of the journal totals
        // stall flow the same way it totals the merchant lines.
        let logged = sold
            .iter()
            .map(|(listing, qty)| format!("{qty}x{}", listing.item_def_id))
            .collect::<Vec<_>>()
            .join(",");
        info!(
            "Stall sale: {seller_name} sold {logged} to {buyer_name} for {total} copper, {tax} taxed"
        );
    }

    async fn owner_of_stall(&self, stall_id: u64) -> Option<PlayerId> {
        self.stalls
            .read()
            .await
            .values()
            .find(|entry| entry.stall.id == stall_id)
            .map(|entry| entry.stall.owner)
    }

    /// Check the customer's distance from the table.
    async fn customer_at_stall(&self, customer: &PlayerId, owner: &PlayerId) -> bool {
        let table = self
            .stalls
            .read()
            .await
            .get(owner)
            .map(|entry| (entry.stall.position, entry.stall.floor_level));
        let Some((position, floor_level)) = table else {
            return false;
        };
        let players = self.players.read().await;
        players.get(customer).is_some_and(|p| {
            reachable_dist_sq(p.position, p.floor_level, position, floor_level)
                .is_some_and(|dist_sq| dist_sq <= MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE)
        })
    }

    /// Send a listing snapshot to one viewer or the whole audience.
    async fn push_stall_state(&self, owner: &PlayerId, only: Option<&PlayerId>) {
        let Some((stall, listings, buy_orders, viewers)) = ({
            let stalls = self.stalls.read().await;
            stalls.get(owner).map(|entry| {
                (
                    entry.stall.clone(),
                    entry.listings.clone(),
                    entry.buy_orders.clone(),
                    match only {
                        Some(one) => vec![*one],
                        None => entry.viewers.iter().copied().collect(),
                    },
                )
            })
        }) else {
            return;
        };
        for viewer in viewers {
            let sign = self.visible_sign(&stall, &viewer).await;
            self.send_direct_message(
                &viewer,
                ServerMessage::StallState {
                    stall_id: stall.id,
                    owner_name: stall.owner_name.clone(),
                    sign,
                    listings: listings.clone(),
                    owned: viewer == *owner,
                    buy_orders: buy_orders.clone(),
                },
            )
            .await;
        }
    }

    /// The sign as `recipient` may see it: a muted owner's board is blank, and
    /// a blocked owner's board is hidden the way their chat is.
    pub(super) async fn visible_sign(&self, stall: &Stall, recipient: &PlayerId) -> String {
        if stall.sign.is_empty()
            || self.is_muted(&stall.owner_name).await
            || self.has_blocked(recipient, &stall.owner_name).await
        {
            return String::new();
        }
        stall.sign.clone()
    }

    /// Announce a stall to its area with the sign each recipient may see. Two
    /// sends at most: the board is either shown or blank.
    async fn broadcast_stall(&self, stall: &Stall, make: impl Fn(Stall) -> ServerMessage) {
        self.sync_interest_blocks().await;
        let mut public = stall.clone();
        if self.is_muted(&stall.owner_name).await {
            public.sign.clear();
        }
        self.publish_subject_change(make(public));
    }
}
