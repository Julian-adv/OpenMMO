//! NPC outfits: sold once per account, given only after a declined sale.

use std::collections::hash_map::Entry;
use std::collections::HashMap;

use onlinerpg_shared::messages::OutfitGrant;
use tracing::{info, warn};

use crate::auth::{AuthService, OutfitClaim, OutfitHandover};
use crate::metrics::GoldSink;
use crate::npc_defs::{npc_defs, NpcDefinition};
use crate::types::{PlayerId, ServerMessage};

use super::combat::reachable_dist_sq;
use super::deals::DEAL_OFFER_MAX_DISTANCE;
use super::inventory::{stack_into_bag, BagInsert};
use super::localization::{format_copper, localized};
use super::time::{GAME_SECONDS_PER_DAY, GAME_SECONDS_PER_REAL_SECOND};
use super::trading::{trader_def_by_name, TraderDef};
use super::{auth_db, GameState};

/// How long a sale offer waits for an answer.
const OUTFIT_OFFER_TTL_MS: u64 = 60_000;
/// Outfits an NPC sells, and separately gives away, per game day.
const OUTFIT_HANDOVERS_PER_GAME_DAY: u32 = 1;
const SOLD_TODAY: &str = "You have already sold your outfit today; offer it again tomorrow.";
const GIVEN_TODAY: &str = "You have already given your outfit away today; give it again tomorrow.";

/// One online account's standing with every outfit NPC.
pub(crate) struct OutfitLedger {
    account: String,
    character_id: i64,
    by_npc: HashMap<String, OutfitGrant>,
}

impl OutfitLedger {
    fn status(&self, npc_id: &str) -> OutfitGrant {
        self.by_npc
            .get(npc_id)
            .copied()
            .unwrap_or(OutfitGrant::Open)
    }
}

/// Carries the account so an unanswered offer still counts as declined after logout.
pub(crate) struct PendingOutfitOffer {
    expires_at_ms: u64,
    def: &'static NpcDefinition,
    player_name: String,
    account: String,
    character_id: i64,
}

fn outfit_def(npc_name: &str) -> Option<&'static NpcDefinition> {
    npc_defs()
        .get_by_npc_name(npc_name)
        .filter(|def| !def.outfit.is_empty())
}

/// What an offer may do given the account's standing.
fn offer_refusal(status: OutfitGrant, gift: bool) -> Option<&'static str> {
    match (status, gift) {
        (OutfitGrant::Received, _) => Some("already has your outfit; it goes to each person once"),
        (OutfitGrant::Declined, false) => {
            Some("turned down your outfit once; you will not offer it for sale again")
        }
        (OutfitGrant::Open, true) => {
            Some("has not been offered your outfit for sale yet; offer it before giving it")
        }
        _ => None,
    }
}

impl GameState {
    pub async fn set_outfit_ledger(
        &self,
        player_id: &PlayerId,
        account: String,
        character_id: i64,
        by_npc: HashMap<String, OutfitGrant>,
    ) {
        self.outfit_ledgers.write().await.insert(
            *player_id,
            OutfitLedger {
                account,
                character_id,
                by_npc,
            },
        );
    }

    /// A leaving player's offers lapse now; a leaving NPC's are withdrawn.
    pub(super) async fn remove_outfit_state(&self, player_id: &PlayerId) {
        self.outfit_ledgers.write().await.remove(player_id);
        let mut offers = self.outfit_offers.write().await;
        offers.retain(|(npc, _), _| npc != player_id);
        for ((_, target), offer) in offers.iter_mut() {
            if target == player_id {
                offer.expires_at_ms = 0;
            }
        }
    }

    /// Record every lapsed sale offer as turned down.
    async fn settle_lapsed_outfit_offers(&self, auth: &AuthService) {
        let now_ms = Self::now_ms();
        let lapsed: Vec<_> = {
            let mut offers = self.outfit_offers.write().await;
            let keys: Vec<_> = offers
                .iter()
                .filter(|(_, offer)| offer.expires_at_ms <= now_ms)
                .map(|(key, _)| *key)
                .collect();
            keys.into_iter()
                .filter_map(|key| offers.remove(&key).map(|offer| (key, offer)))
                .collect()
        };
        for ((npc_player_id, player_id), offer) in lapsed {
            self.record_outfit_decline(&npc_player_id, &player_id, offer, true, auth)
                .await;
        }
    }

    async fn record_outfit_decline(
        &self,
        npc_player_id: &PlayerId,
        player_id: &PlayerId,
        offer: PendingOutfitOffer,
        lapsed: bool,
        auth: &AuthService,
    ) {
        let PendingOutfitOffer {
            def,
            player_name,
            account,
            character_id,
            ..
        } = offer;
        let auth = auth.clone();
        if let Err(err) =
            auth_db(move || auth.decline_npc_outfit(&account, &def.id, character_id)).await
        {
            warn!("Failed to record a declined outfit for {player_name}: {err}");
            return;
        }
        self.set_outfit_status(player_id, &def.id, OutfitGrant::Declined)
            .await;
        let verb = if lapsed { "let" } else { "turned down" };
        info!(target: "outfit", "outfit declined: npc={} player={player_name} verb={verb}", def.npc_name);
        let note = if lapsed {
            format!("{player_name} let your outfit offer lapse without buying.")
        } else {
            format!("{player_name} turned down your outfit.")
        };
        self.send_outfit_status(
            npc_player_id,
            def,
            &player_name,
            OutfitGrant::Declined,
            Some(note),
        )
        .await;
    }

    async fn outfit_status(&self, player_id: &PlayerId, npc_id: &str) -> OutfitGrant {
        self.outfit_ledgers
            .read()
            .await
            .get(player_id)
            .map_or(OutfitGrant::Received, |ledger| ledger.status(npc_id))
    }

    async fn set_outfit_status(&self, player_id: &PlayerId, npc_id: &str, status: OutfitGrant) {
        if let Some(ledger) = self.outfit_ledgers.write().await.get_mut(player_id) {
            ledger.by_npc.insert(npc_id.to_string(), status);
        }
    }

    async fn close_outfit_handovers(
        &self,
        def: &'static NpcDefinition,
        how: OutfitHandover,
        game_day: i64,
    ) {
        self.outfit_handovers_closed
            .write()
            .await
            .insert((&def.id, how), game_day);
    }

    /// Whether the NPC has used today's share of `how`.
    async fn outfit_used_up(
        &self,
        def: &'static NpcDefinition,
        how: OutfitHandover,
        auth: &AuthService,
    ) -> bool {
        let game_day = self.current_game_day();
        let key = (def.id.as_str(), how);
        if self.outfit_handovers_closed.read().await.get(&key) == Some(&game_day) {
            return true;
        }
        let db = auth.clone();
        let done = match auth_db(move || db.npc_outfit_handovers_on(&def.id, how, game_day)).await {
            Ok(done) => done,
            Err(err) => {
                warn!("Failed to count outfit handovers for {}: {err}", def.id);
                return true;
            }
        };
        let out = done >= OUTFIT_HANDOVERS_PER_GAME_DAY;
        if out {
            self.close_outfit_handovers(def, how, game_day).await;
        }
        out
    }

    async fn send_outfit_status(
        &self,
        npc_player_id: &PlayerId,
        def: &'static NpcDefinition,
        player_name: &str,
        status: OutfitGrant,
        note: Option<String>,
    ) {
        let now = self.current_total_game_seconds();
        let today = Self::game_day(now);
        let (sales_reopen_in_secs, gifts_reopen_in_secs) = {
            let closed = self.outfit_handovers_closed.read().await;
            let reopen_in = |how| {
                let left = (today + 1) * GAME_SECONDS_PER_DAY - now;
                (closed.get(&(def.id.as_str(), how)) == Some(&today))
                    .then(|| (left as f64 / GAME_SECONDS_PER_REAL_SECOND).ceil() as u32)
            };
            (
                reopen_in(OutfitHandover::Bought),
                reopen_in(OutfitHandover::Gifted),
            )
        };
        self.send_direct_message(
            npc_player_id,
            ServerMessage::OutfitStatus {
                player_name: player_name.to_string(),
                status,
                note,
                sales_reopen_in_secs,
                gifts_reopen_in_secs,
            },
        )
        .await;
    }

    /// The outfit NPC's definition, and the target's name when the target is
    /// a nearby human the NPC can hand clothes to.
    async fn outfit_parties(
        &self,
        npc_player_id: &PlayerId,
        target_player_id: &PlayerId,
    ) -> Option<(
        &'static NpcDefinition,
        Result<String, (String, &'static str)>,
    )> {
        let players = self.players.read().await;
        let npc = players.get(npc_player_id).filter(|p| p.is_official_npc)?;
        let def = outfit_def(&npc.name)?;
        let target = match players.get(target_player_id) {
            None => Err(("?".to_string(), "that player is no longer here")),
            Some(t) => {
                let refusal = if t.is_official_npc {
                    Some("your outfit is only for players")
                } else if Some(t.gender) != def.gender {
                    Some("your outfit would not fit them")
                } else {
                    match reachable_dist_sq(
                        t.position,
                        t.floor_level,
                        npc.position,
                        npc.floor_level,
                    ) {
                        Some(d) if d <= DEAL_OFFER_MAX_DISTANCE * DEAL_OFFER_MAX_DISTANCE => None,
                        _ => Some("they are too far away"),
                    }
                };
                match refusal {
                    Some(why) => Err((t.name.clone(), why)),
                    None => Ok(t.name.clone()),
                }
            }
        };
        Some((def, target))
    }

    pub async fn query_outfit(
        &self,
        npc_player_id: &PlayerId,
        target_player_id: &PlayerId,
        auth: &AuthService,
    ) {
        let Some((def, target)) = self.outfit_parties(npc_player_id, target_player_id).await else {
            return;
        };
        self.settle_lapsed_outfit_offers(auth).await;
        let Ok(name) = target else {
            return;
        };
        let status = self.outfit_status(target_player_id, &def.id).await;
        self.send_outfit_status(npc_player_id, def, &name, status, None)
            .await;
    }

    pub async fn offer_outfit(
        &self,
        npc_player_id: &PlayerId,
        target_player_id: &PlayerId,
        gift: bool,
        auth: &AuthService,
    ) {
        let Some((def, target)) = self.outfit_parties(npc_player_id, target_player_id).await else {
            return;
        };
        self.settle_lapsed_outfit_offers(auth).await;
        let npc_name = &def.npc_name;
        let status = self.outfit_status(target_player_id, &def.id).await;
        let (name, refusal) = match target {
            Err((name, why)) => (name, Some(why)),
            Ok(_) if self.is_npc_asleep(npc_name) => return,
            Ok(name) => (name, offer_refusal(status, gift)),
        };
        let tell =
            |note: String| self.send_outfit_status(npc_player_id, def, &name, status, Some(note));
        if let Some(why) = refusal {
            info!(target: "outfit", "outfit offer refused: npc={npc_name} player={name} gift={gift} why={why:?}");
            return tell(format!("{name} {why}.")).await;
        }
        let how = if gift {
            OutfitHandover::Gifted
        } else {
            OutfitHandover::Bought
        };
        if self.outfit_used_up(def, how, auth).await {
            return tell(if gift { GIVEN_TODAY } else { SOLD_TODAY }.to_string()).await;
        }
        if gift {
            return self
                .hand_over_outfit(npc_player_id, def, target_player_id, &name, how, auth)
                .await;
        }

        let Some((account, character_id)) = self.outfit_account(target_player_id).await else {
            return;
        };
        let pending = match self
            .outfit_offers
            .write()
            .await
            .entry((*npc_player_id, *target_player_id))
        {
            Entry::Occupied(_) => true,
            Entry::Vacant(slot) => {
                slot.insert(PendingOutfitOffer {
                    expires_at_ms: Self::now_ms() + OUTFIT_OFFER_TTL_MS,
                    def,
                    player_name: name.clone(),
                    account,
                    character_id,
                });
                false
            }
        };
        if pending {
            return tell(format!(
                "Your offer is still before {name}; wait for their answer."
            ))
            .await;
        }
        info!(target: "outfit", "outfit offered: npc={npc_name} player={name} price={}", def.outfit_price);
        self.send_direct_message(
            target_player_id,
            ServerMessage::OutfitOffered {
                npc_player_id: *npc_player_id,
                npc_name: npc_name.clone(),
                item_def_ids: def.outfit.clone(),
                price: def.outfit_price,
            },
        )
        .await;
        tell(format!(
            "Your offer of the outfit for {} is now before {name}.",
            format_copper(def.outfit_price)
        ))
        .await;
    }

    pub async fn respond_outfit_offer(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        accept: bool,
        auth: &AuthService,
    ) {
        let offer = self
            .outfit_offers
            .write()
            .await
            .remove(&(*npc_player_id, *player_id));
        let lapsed = offer
            .as_ref()
            .is_none_or(|offer| offer.expires_at_ms <= Self::now_ms());
        if accept && lapsed {
            self.send_system_message(
                player_id,
                localized("server.outfitLapsed", "That offer has lapsed."),
            )
            .await;
        }
        let Some(offer) = offer else {
            return;
        };
        if !accept || lapsed {
            return self
                .record_outfit_decline(npc_player_id, player_id, offer, lapsed, auth)
                .await;
        }
        self.hand_over_outfit(
            npc_player_id,
            offer.def,
            player_id,
            &offer.player_name,
            OutfitHandover::Bought,
            auth,
        )
        .await;
    }

    pub(super) async fn outfit_account(&self, player_id: &PlayerId) -> Option<(String, i64)> {
        self.outfit_ledgers
            .read()
            .await
            .get(player_id)
            .map(|ledger| (ledger.account.clone(), ledger.character_id))
    }

    /// Claim the account's once-only record, then put every piece in the bag
    /// (and take the price for a sale) or none, releasing the claim on failure.
    async fn hand_over_outfit(
        &self,
        npc_player_id: &PlayerId,
        def: &'static NpcDefinition,
        player_id: &PlayerId,
        player_name: &str,
        how: OutfitHandover,
        auth: &AuthService,
    ) {
        let Some((account, character_id)) = self.outfit_account(player_id).await else {
            return;
        };
        let npc_name = &def.npc_name;
        let game_day = self.current_game_day();
        let claimed = {
            let (db, account) = (auth.clone(), account.clone());
            auth_db(move || {
                db.claim_npc_outfit(
                    &account,
                    &def.id,
                    character_id,
                    how,
                    game_day,
                    OUTFIT_HANDOVERS_PER_GAME_DAY,
                )
            })
            .await
        };
        let tell = |status, note: String| {
            self.send_outfit_status(npc_player_id, def, player_name, status, Some(note))
        };
        let (prior, last) = match claimed {
            Ok(OutfitClaim::Claimed { prior, last }) => (prior, last),
            Ok(OutfitClaim::SoldOut) => {
                self.close_outfit_handovers(def, how, game_day).await;
                let note = match how {
                    OutfitHandover::Bought => {
                        self.send_system_message(
                            player_id,
                            localized(
                                "server.outfitSoldOut",
                                "That outfit has already been sold today.",
                            ),
                        )
                        .await;
                        format!(
                            "{player_name} could not buy your outfit: you already sold one today."
                        )
                    }
                    OutfitHandover::Gifted => GIVEN_TODAY.to_string(),
                };
                let status = self.outfit_status(player_id, &def.id).await;
                return tell(status, note).await;
            }
            Ok(OutfitClaim::Refused) => {
                let status = self.outfit_status(player_id, &def.id).await;
                return tell(
                    status,
                    format!("{player_name} cannot take your outfit again."),
                )
                .await;
            }
            Err(err) => {
                warn!("Failed to claim {npc_name}'s outfit for {player_name}: {err}");
                return;
            }
        };

        let price = match how {
            OutfitHandover::Bought => def.outfit_price,
            OutfitHandover::Gifted => 0,
        };
        let resident = matches!(trader_def_by_name(npc_name), Some(TraderDef::Resident(_)));
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let weight: f32 = def
            .outfit
            .iter()
            .map(|id| self.item_defs.weight_with(id, armor_mult))
            .sum();
        let first_id = self.reserve_instance_ids(def.outfit.len() as u64).await;

        let handed = {
            let mut gold_map = self.player_gold.write().await;
            let mut inventories = self.inventories.write().await;
            let handed = match (gold_map.get_mut(player_id), inventories.get_mut(player_id)) {
                (Some(gold), Some(_)) if *gold < price => {
                    Err(localized("server.insufficientGold", "Not enough gold"))
                }
                (Some(_), Some(inv))
                    if self.calc_total_weight(inv, armor_mult) + weight > max_weight =>
                {
                    Err(localized("server.tooHeavy", "Too heavy to carry"))
                }
                (Some(gold), Some(inv)) => {
                    for (offset, id) in def.outfit.iter().enumerate() {
                        stack_into_bag(
                            &mut inv.bag,
                            BagInsert::one(
                                self.item_defs.stackable(id),
                                id,
                                0,
                                first_id + offset as u64,
                            ),
                        );
                    }
                    *gold -= price;
                    Ok(inv.clone())
                }
                _ => Err(localized("server.outfitLapsed", "That offer has lapsed.")),
            };
            if handed.is_ok() && resident && price > 0 {
                *gold_map.entry(*npc_player_id).or_insert(0) += price;
            }
            handed
        };

        let snapshot = match handed {
            Ok(snapshot) => snapshot,
            Err(message) => {
                let auth = auth.clone();
                if let Err(err) =
                    auth_db(move || auth.release_npc_outfit(&account, &def.id, prior)).await
                {
                    warn!("Failed to release {npc_name}'s outfit claim for {player_name}: {err}");
                }
                self.outfit_handovers_closed
                    .write()
                    .await
                    .remove(&(def.id.as_str(), how));
                info!(target: "outfit", "outfit handover failed: npc={npc_name} player={player_name} how={how:?} why={:?}", message.message);
                let note = format!(
                    "{player_name} could not take your outfit: {}.",
                    message.message.to_lowercase()
                );
                self.send_system_message(player_id, message).await;
                return tell(prior, note).await;
            }
        };

        self.set_outfit_status(player_id, &def.id, OutfitGrant::Received)
            .await;
        self.mark_dirty(player_id).await;
        self.mark_inventory_dirty(player_id).await;
        self.send_direct_message(
            player_id,
            ServerMessage::InventoryUpdated {
                inventory: snapshot,
            },
        )
        .await;
        let (message, note) = match how {
            OutfitHandover::Bought => {
                self.send_gold_update(player_id).await;
                if resident {
                    self.mark_dirty(npc_player_id).await;
                    self.send_gold_update(npc_player_id).await;
                } else {
                    self.record_gold_sink(
                        GoldSink::NpcOutfit {
                            npc_id: def.id.clone(),
                        },
                        1,
                        price,
                    )
                    .await;
                }
                (
                    localized(
                        "server.outfitBought",
                        format!("You bought the outfit for {}.", format_copper(price)),
                    )
                    .with_param("price", format_copper(price)),
                    format!("{player_name} bought your outfit."),
                )
            }
            OutfitHandover::Gifted => (
                localized("server.outfitGifted", "You received an outfit as a gift."),
                format!("{player_name} received your outfit as a gift."),
            ),
        };
        info!(target: "outfit", "outfit handed over: npc={npc_name} player={player_name} how={how:?} price={price}");
        if last {
            self.close_outfit_handovers(def, how, game_day).await;
        }
        self.send_system_message(player_id, message).await;
        tell(OutfitGrant::Received, note).await;
    }
}
