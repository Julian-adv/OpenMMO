use crate::merchant_defs::{merchant_defs, MerchantDefinition};
use crate::npc_defs::{npc_defs, NpcDefinition};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::inventory::ItemInstance;
use onlinerpg_shared::messages::{DealKind, StockEntry};
use tracing::info;

use super::deals::{buy_price, deal_half_band_pct, resident_half_band_pct, sell_payout};

/// Maximum distance between player and trader for any shop interaction.
const MAX_TRADE_DISTANCE: f32 = 6.0;

/// How an NPC trades (economy phases 1–3). Merchants sell a data-defined
/// catalog with unlimited stock and an effectively unlimited wallet;
/// residents (non-merchants) buy only their wishlist from a finite,
/// salary-funded wallet and sell from their real inventory.
#[derive(Clone)]
pub(crate) enum TraderDef {
    Merchant(MerchantDefinition),
    Resident(NpcDefinition),
}

impl TraderDef {
    pub(crate) fn npc_name(&self) -> &str {
        match self {
            TraderDef::Merchant(def) => &def.npc_name,
            TraderDef::Resident(def) => &def.npc_name,
        }
    }

    /// Haggle eligibility and pricing for one item: the payout rate and the
    /// CHA-derived band half-width, or why the offer is rejected. Keeps the
    /// trader-kind rules (catalog scope, wishlist disjointness, band width)
    /// in one place.
    pub(crate) fn haggle_params(
        &self,
        kind: DealKind,
        item_def_id: &str,
        cha: u8,
    ) -> Result<(u32, i32), &'static str> {
        match self {
            TraderDef::Merchant(m) => {
                if kind == DealKind::Buy && !m.sells(item_def_id) {
                    return Err("that item is not in your catalog");
                }
                Ok((m.sell_rate_percent, deal_half_band_pct(cha)))
            }
            TraderDef::Resident(r) => {
                // Keep the buy/sell item sets disjoint (see resident_stock).
                match kind {
                    DealKind::Sell if !r.wants(item_def_id) => {
                        Err("that item is not on your wishlist")
                    }
                    DealKind::Buy if r.wants(item_def_id) => {
                        Err("you do not resell your wishlist items")
                    }
                    _ => Ok((r.wishlist_rate_percent, resident_half_band_pct(cha))),
                }
            }
        }
    }
}

/// Look up how an NPC (by character name) trades, if at all.
pub(crate) fn trader_def_by_name(npc_name: &str) -> Option<TraderDef> {
    if let Some(def) = merchant_defs().get_by_npc_name(npc_name) {
        return Some(TraderDef::Merchant(def.clone()));
    }
    npc_defs()
        .get_trader_by_npc_name(npc_name)
        .map(|def| TraderDef::Resident(def.clone()))
}

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

    /// Abort a buy/sell whose single-use deal was already taken: put the
    /// deal back and (optionally) tell the player why. Callers drop any
    /// gold/inventory locks before awaiting this.
    async fn fail_trade(
        &self,
        player_id: &PlayerId,
        npc_name: &str,
        item_def_id: &str,
        kind: DealKind,
        deal: Option<super::deals::DealEntry>,
        message: Option<&'static str>,
    ) {
        self.restore_deal(player_id, npc_name, item_def_id, kind, deal)
            .await;
        if let Some(message) = message {
            self.send_trade_error(player_id, message).await;
        }
    }

    async fn send_gold_update(&self, player_id: &PlayerId) {
        let gold = self.get_player_gold(player_id).await;
        self.send_direct_message(player_id, ServerMessage::GoldUpdate { gold })
            .await;
    }

    async fn player_name_of(&self, player_id: &str) -> String {
        let players = self.players.read().await;
        players
            .get(player_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| player_id.to_string())
    }

    /// Tell the trading NPC's LLM that a player completed a trade with it.
    async fn send_trade_notice(
        &self,
        npc_player_id: &str,
        player_name: String,
        item_def_id: &str,
        kind: DealKind,
        price: i64,
    ) {
        let npc_gold = self.get_player_gold(&npc_player_id.to_string()).await;
        self.send_direct_message(
            &npc_player_id.to_string(),
            ServerMessage::TradeNotice {
                player_name,
                item_def_id: item_def_id.to_string(),
                kind,
                price,
                npc_gold,
            },
        )
        .await;
    }

    /// Validate that `npc_player_id` is a trading NPC within range of the
    /// player. Returns the trader definition on success.
    async fn validate_trader(
        &self,
        player_id: &PlayerId,
        npc_player_id: &str,
    ) -> Result<TraderDef, &'static str> {
        let players = self.players.read().await;
        let player = players.get(player_id).ok_or("Player not found")?;
        let npc = players.get(npc_player_id).ok_or("Trader not found")?;

        if !npc.is_npc {
            return Err("That character is not a trader");
        }
        let def = trader_def_by_name(&npc.name).ok_or("This NPC does not trade")?;

        let dx = player.position.x - npc.position.x;
        let dz = player.position.z - npc.position.z;
        if dx * dx + dz * dz > MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE {
            return Err("Too far away to trade");
        }

        Ok(def)
    }

    pub async fn open_shop(&self, player_id: &PlayerId, npc_player_id: &str) {
        let def = match self.validate_trader(player_id, npc_player_id).await {
            Ok(def) => def,
            Err(reason) => return self.send_trade_error(player_id, reason).await,
        };
        let active_deals = self.active_deals_for(player_id, def.npc_name()).await;
        let state = match def {
            TraderDef::Merchant(def) => ServerMessage::ShopState {
                merchant_player_id: npc_player_id.to_string(),
                merchant_name: def.npc_name.clone(),
                catalog: def.catalog.clone(),
                sell_rate_percent: def.sell_rate_percent,
                active_deals,
                wishlist: Vec::new(),
                stock: Vec::new(),
            },
            TraderDef::Resident(def) => ServerMessage::ShopState {
                merchant_player_id: npc_player_id.to_string(),
                merchant_name: def.npc_name.clone(),
                catalog: Vec::new(),
                sell_rate_percent: def.wishlist_rate_percent,
                active_deals,
                wishlist: def.wishlist.clone(),
                stock: self.resident_stock(npc_player_id, &def).await,
            },
        };
        self.send_direct_message(player_id, state).await;
        self.send_gold_update(player_id).await;
    }

    /// NPC-initiated trade (LLM `open_trade` action): push the NPC's own
    /// shop window onto a nearby player's client.
    pub async fn open_trade(&self, npc_player_id: &PlayerId, target_player_id: &str) {
        let valid = {
            let players = self.players.read().await;
            match (players.get(npc_player_id), players.get(target_player_id)) {
                (Some(npc), _) if !npc.is_npc => Err("only NPCs can push trade windows"),
                (Some(npc), _) if trader_def_by_name(&npc.name).is_none() => {
                    Err("you have nothing to trade with")
                }
                (Some(_), None) => Err("that player is not here"),
                (Some(_), Some(target)) if target.is_npc => {
                    Err("trade windows can only be opened for players")
                }
                (Some(npc), Some(target)) => {
                    let dx = npc.position.x - target.position.x;
                    let dz = npc.position.z - target.position.z;
                    if dx * dx + dz * dz > MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE {
                        Err("the player is too far away to trade — ask them to come closer")
                    } else {
                        Ok(())
                    }
                }
                (None, _) => return,
            }
        };
        if let Err(reason) = valid {
            return self.send_trade_error(npc_player_id, reason).await;
        }
        // open_shop re-validates with the roles in their normal order and
        // sends ShopState + GoldUpdate to the target player.
        self.open_shop(&target_player_id.to_string(), npc_player_id)
            .await;
    }

    /// A resident's purchasable stock: priced bag items that are not on its
    /// wishlist. Wishlist purchases are kept (never resold) so the
    /// buy/sell item sets stay disjoint — no money pump is possible even
    /// though the wishlist rate exceeds the sale price.
    async fn resident_stock(&self, npc_player_id: &str, def: &NpcDefinition) -> Vec<StockEntry> {
        let inventories = self.inventories.read().await;
        let Some(inv) = inventories.get(npc_player_id) else {
            return Vec::new();
        };
        let mut stock: Vec<StockEntry> = Vec::new();
        for item in &inv.bag {
            if def.wants(&item.item_def_id) {
                continue;
            }
            if self
                .item_defs
                .get(&item.item_def_id)
                .and_then(|d| d.base_price)
                .is_none()
            {
                continue;
            }
            match stock
                .iter_mut()
                .find(|entry| entry.item_def_id == item.item_def_id)
            {
                Some(entry) => entry.quantity += item.quantity,
                None => stock.push(StockEntry {
                    item_def_id: item.item_def_id.clone(),
                    quantity: item.quantity,
                }),
            }
        }
        stock
    }

    /// Buy one unit of `item_def_id` from a trading NPC. Merchants create
    /// the item from its definition (unlimited stock); residents transfer
    /// a unit out of their real inventory and pocket the gold.
    pub async fn buy_item(&self, player_id: &PlayerId, npc_player_id: &str, item_def_id: &str) {
        let def = match self.validate_trader(player_id, npc_player_id).await {
            Ok(def) => def,
            Err(reason) => return self.send_trade_error(player_id, reason).await,
        };

        match &def {
            TraderDef::Merchant(m) => {
                if !m.sells(item_def_id) {
                    return self
                        .send_trade_error(player_id, "The merchant does not sell that item")
                        .await;
                }
            }
            TraderDef::Resident(r) => {
                if r.wants(item_def_id) {
                    // Wishlist purchases are kept; see `resident_stock`.
                    return self
                        .send_trade_error(player_id, "They won't part with that")
                        .await;
                }
            }
        }

        let Some(base_price) = self
            .item_defs
            .get(item_def_id)
            .and_then(|item| item.base_price)
        else {
            return self
                .send_trade_error(player_id, "That item has no price")
                .await;
        };

        let npc_name = def.npc_name().to_string();
        let is_resident = matches!(def, TraderDef::Resident(_));

        // Single-use haggled modifier; must be restored if the buy fails.
        let deal = self
            .take_deal(player_id, &npc_name, item_def_id, DealKind::Buy)
            .await;
        let price = buy_price(base_price, deal.as_ref().map_or(0, |d| d.modifier_pct));

        let item_weight = self.item_defs.weight(item_def_id);
        let max_weight = self.max_carry_weight(player_id).await;
        let instance_id = self.next_instance_id().await;

        // Gold and inventory mutate under both write locks so a concurrent
        // request cannot double-spend between the check and the deduction.
        let (snapshot, npc_snapshot) = {
            let mut gold_map = self.player_gold.write().await;
            let Some(gold) = gold_map.get(player_id).copied() else {
                drop(gold_map);
                return self
                    .fail_trade(player_id, &npc_name, item_def_id, DealKind::Buy, deal, None)
                    .await;
            };
            if gold < price {
                drop(gold_map);
                return self
                    .fail_trade(
                        player_id,
                        &npc_name,
                        item_def_id,
                        DealKind::Buy,
                        deal,
                        Some("Not enough gold"),
                    )
                    .await;
            }

            let mut inventories = self.inventories.write().await;
            if inventories.get(player_id).is_none() {
                drop(inventories);
                drop(gold_map);
                return self
                    .fail_trade(player_id, &npc_name, item_def_id, DealKind::Buy, deal, None)
                    .await;
            };
            if self.calc_total_weight(&inventories[player_id]) + item_weight > max_weight {
                drop(inventories);
                drop(gold_map);
                return self
                    .fail_trade(
                        player_id,
                        &npc_name,
                        item_def_id,
                        DealKind::Buy,
                        deal,
                        Some("Too heavy to carry"),
                    )
                    .await;
            }

            // Residents have finite stock: take the unit out of their bag.
            let npc_snapshot = if is_resident {
                let Some(npc_inv) = inventories.get_mut(npc_player_id) else {
                    drop(inventories);
                    drop(gold_map);
                    return self
                        .fail_trade(
                            player_id,
                            &npc_name,
                            item_def_id,
                            DealKind::Buy,
                            deal,
                            Some("They have nothing to sell"),
                        )
                        .await;
                };
                let Some(idx) = npc_inv
                    .bag
                    .iter()
                    .position(|i| i.item_def_id == item_def_id)
                else {
                    drop(inventories);
                    drop(gold_map);
                    return self
                        .fail_trade(
                            player_id,
                            &npc_name,
                            item_def_id,
                            DealKind::Buy,
                            deal,
                            Some("They are out of that item"),
                        )
                        .await;
                };
                if npc_inv.bag[idx].quantity > 1 {
                    npc_inv.bag[idx].quantity -= 1;
                } else {
                    npc_inv.bag.remove(idx);
                }
                Some(npc_inv.clone())
            } else {
                None
            };

            let inv = inventories.get_mut(player_id).expect("checked above");
            inv.bag.push(ItemInstance {
                instance_id,
                item_def_id: item_def_id.to_string(),
                quantity: 1,
            });
            let snapshot = inv.clone();

            *gold_map.get_mut(player_id).expect("checked above") -= price;
            if is_resident {
                *gold_map.entry(npc_player_id.to_string()).or_insert(0) += price;
            }
            (snapshot, npc_snapshot)
        };

        if let Some(entry) = deal {
            info!(
                target: "deal",
                "deal redeemed: npc={npc_name} player={player_id} item={item_def_id} kind=Buy \
                 modifier={} base={base_price} paid={price}",
                entry.modifier_pct
            );
            self.send_deal_cleared(player_id, npc_player_id, item_def_id, DealKind::Buy)
                .await;
        }
        info!("{player_id} bought {item_def_id} from {npc_name} for {price}");
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

        if let Some(npc_snapshot) = npc_snapshot {
            self.mark_dirty(&npc_player_id.to_string()).await;
            self.mark_inventory_dirty(&npc_player_id.to_string()).await;
            self.send_direct_message(
                &npc_player_id.to_string(),
                ServerMessage::InventoryUpdated {
                    inventory: npc_snapshot,
                },
            )
            .await;
            self.send_gold_update(&npc_player_id.to_string()).await;
        }
        let player_name = self.player_name_of(player_id).await;
        self.send_trade_notice(
            npc_player_id,
            player_name,
            item_def_id,
            DealKind::Buy,
            price,
        )
        .await;
    }

    /// Sell one unit of a bag item to a trading NPC. Merchants pay
    /// `base_price * sell_rate_percent / 100` and the item vanishes;
    /// residents only buy wishlist items, pay their premium rate out of a
    /// finite wallet, and keep the item in their real inventory.
    pub async fn sell_item(&self, player_id: &PlayerId, npc_player_id: &str, instance_id: u64) {
        let def = match self.validate_trader(player_id, npc_player_id).await {
            Ok(def) => def,
            Err(reason) => return self.send_trade_error(player_id, reason).await,
        };

        // Resolve the item def up front so any haggled sell bonus can be
        // looked up before taking the gold/inventory locks.
        let item_def_id = {
            let inventories = self.inventories.read().await;
            let Some(item) = inventories
                .get(player_id)
                .and_then(|inv| inv.bag.iter().find(|i| i.instance_id == instance_id))
            else {
                return self
                    .send_trade_error(player_id, "Item not found in bag")
                    .await;
            };
            item.item_def_id.clone()
        };
        let Some(base_price) = self
            .item_defs
            .get(&item_def_id)
            .and_then(|item| item.base_price)
        else {
            return self
                .send_trade_error(player_id, "They will not buy that")
                .await;
        };

        let rate = match &def {
            TraderDef::Merchant(m) => m.sell_rate_percent,
            TraderDef::Resident(r) => {
                if !r.wants(&item_def_id) {
                    return self
                        .send_trade_error(player_id, "They have no use for that")
                        .await;
                }
                r.wishlist_rate_percent
            }
        };
        let npc_name = def.npc_name().to_string();
        let is_resident = matches!(def, TraderDef::Resident(_));

        // Single-use haggled modifier; must be restored if the sell fails.
        let deal = self
            .take_deal(player_id, &npc_name, &item_def_id, DealKind::Sell)
            .await;
        let payout = sell_payout(
            base_price,
            rate,
            deal.as_ref().map_or(0, |d| d.modifier_pct),
        );

        let item_weight = self.item_defs.weight(&item_def_id);
        let npc_max_weight = self.max_carry_weight(&npc_player_id.to_string()).await;
        let npc_instance_id = self.next_instance_id().await;

        let (snapshot, npc_snapshot) = {
            let mut gold_map = self.player_gold.write().await;
            if !gold_map.contains_key(player_id) {
                drop(gold_map);
                return self
                    .fail_trade(
                        player_id,
                        &npc_name,
                        &item_def_id,
                        DealKind::Sell,
                        deal,
                        None,
                    )
                    .await;
            }
            // Residents pay out of a real wallet; merchants out of thin air.
            if is_resident {
                let npc_gold = gold_map.get(npc_player_id).copied().unwrap_or(0);
                if npc_gold < payout {
                    drop(gold_map);
                    return self
                        .fail_trade(
                            player_id,
                            &npc_name,
                            &item_def_id,
                            DealKind::Sell,
                            deal,
                            Some("They cannot afford that right now"),
                        )
                        .await;
                }
            }

            let mut inventories = self.inventories.write().await;
            let Some(idx) = inventories
                .get_mut(player_id)
                .and_then(|inv| inv.bag.iter().position(|i| i.instance_id == instance_id))
            else {
                drop(inventories);
                drop(gold_map);
                return self
                    .fail_trade(
                        player_id,
                        &npc_name,
                        &item_def_id,
                        DealKind::Sell,
                        deal,
                        Some("Item not found in bag"),
                    )
                    .await;
            };

            // The bought unit lands in the resident's real inventory, so it
            // is bound by their carry weight like any player.
            let npc_snapshot = if is_resident {
                let Some(npc_inv) = inventories.get_mut(npc_player_id) else {
                    drop(inventories);
                    drop(gold_map);
                    return self
                        .fail_trade(
                            player_id,
                            &npc_name,
                            &item_def_id,
                            DealKind::Sell,
                            deal,
                            None,
                        )
                        .await;
                };
                if self.calc_total_weight(npc_inv) + item_weight > npc_max_weight {
                    drop(inventories);
                    drop(gold_map);
                    return self
                        .fail_trade(
                            player_id,
                            &npc_name,
                            &item_def_id,
                            DealKind::Sell,
                            deal,
                            Some("They cannot carry any more"),
                        )
                        .await;
                }
                npc_inv.bag.push(ItemInstance {
                    instance_id: npc_instance_id,
                    item_def_id: item_def_id.clone(),
                    quantity: 1,
                });
                Some(npc_inv.clone())
            } else {
                None
            };

            let inv = inventories.get_mut(player_id).expect("checked above");
            if inv.bag[idx].quantity > 1 {
                inv.bag[idx].quantity -= 1;
            } else {
                inv.bag.remove(idx);
            }
            let snapshot = inv.clone();

            *gold_map.get_mut(player_id).expect("checked above") += payout;
            if is_resident {
                *gold_map.get_mut(npc_player_id).expect("checked above") -= payout;
            }
            (snapshot, npc_snapshot)
        };

        if let Some(entry) = deal {
            info!(
                target: "deal",
                "deal redeemed: npc={npc_name} player={player_id} item={item_def_id} kind=Sell \
                 modifier={} base={base_price} paid={payout}",
                entry.modifier_pct
            );
            self.send_deal_cleared(player_id, npc_player_id, &item_def_id, DealKind::Sell)
                .await;
        }
        info!("{player_id} sold {item_def_id} to {npc_name} for {payout}");
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

        if let Some(npc_snapshot) = npc_snapshot {
            self.mark_dirty(&npc_player_id.to_string()).await;
            self.mark_inventory_dirty(&npc_player_id.to_string()).await;
            self.send_direct_message(
                &npc_player_id.to_string(),
                ServerMessage::InventoryUpdated {
                    inventory: npc_snapshot,
                },
            )
            .await;
            self.send_gold_update(&npc_player_id.to_string()).await;
        }
        let player_name = self.player_name_of(player_id).await;
        self.send_trade_notice(
            npc_player_id,
            player_name,
            &item_def_id,
            DealKind::Sell,
            payout,
        )
        .await;
    }
}
