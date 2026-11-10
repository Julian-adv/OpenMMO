//! Families (doc/FAMILY_SYSTEM.md). Membership, ranks and estate links live
//! in the DB (`auth_family.rs`); this side keeps which members are online,
//! pending invites, and pushes rosters and name changes.

use std::collections::HashMap;
use std::time::Instant;

use onlinerpg_shared::family::{
    FamilyEstate, FamilyInfo, FamilyMember, FAMILY_CREATE_COST, FAMILY_INVITE_TTL,
    FAMILY_RENAME_COST, FAMILY_RENAME_INTERVAL_SECS,
};
use tracing::{info, warn};

use super::auth_db;
use super::consent::{answer_consent, PendingConsent};
use super::inventory::serialize_inventory;
use crate::auth::{unix_now, AuthError, AuthService, CharacterSaveData, FamilyRoster, ItemRow};
use crate::types::{PlayerId, ServerMessage};

const FAMILY_PENDING_INVITE_CAP: usize = 5;
const SERVER_ERROR: &str = "server error, try again.";

/// Never held while taking another `GameState` lock.
#[derive(Default)]
pub(crate) struct Families {
    online: HashMap<i64, Vec<PlayerId>>,
    of_player: HashMap<PlayerId, i64>,
    invites: HashMap<(PlayerId, PlayerId), PendingConsent>,
}

impl Families {
    fn enter(&mut self, player_id: PlayerId, family_id: i64) {
        self.leave(&player_id);
        self.of_player.insert(player_id, family_id);
        self.online.entry(family_id).or_default().push(player_id);
    }

    fn leave(&mut self, player_id: &PlayerId) -> Option<i64> {
        let family_id = self.of_player.remove(player_id)?;
        if let Some(list) = self.online.get_mut(&family_id) {
            list.retain(|id| id != player_id);
            if list.is_empty() {
                self.online.remove(&family_id);
            }
        }
        Some(family_id)
    }

    fn online(&self, family_id: i64) -> Vec<PlayerId> {
        self.online.get(&family_id).cloned().unwrap_or_default()
    }

    fn sweep_and_count(&mut self, inviter: &PlayerId) -> usize {
        let now = Instant::now();
        let mut pending = 0;
        self.invites.retain(|(from, _), invite| {
            let keep = invite.expires_at > now;
            if keep && from == inviter {
                pending += 1;
            }
            keep
        });
        pending
    }
}

fn family_info(roster: &FamilyRoster, viewer: i64, online: &[i64], now: i64) -> Option<FamilyInfo> {
    let me = roster.members.iter().find(|m| m.character_id == viewer)?;
    Some(FamilyInfo {
        name: roster.name.clone(),
        ranks: roster.ranks.clone(),
        members: roster
            .members
            .iter()
            .map(|m| FamilyMember {
                character_id: m.character_id,
                name: m.name.clone(),
                level: m.level,
                class: m.class.clone(),
                rank: m.rank,
                online: online.contains(&m.character_id),
            })
            .collect(),
        estates: roster
            .estates
            .iter()
            .map(|e| FamilyEstate {
                estate_id: e.estate_id,
                owner_name: e.owner_name.clone(),
                plots: e.plots,
                missed: e.missed,
                mine: e.account_name == me.account_name,
            })
            .collect(),
        my_rank: me.rank,
        can_attach_estate: roster.unattached_accounts.contains(&me.account_name),
        can_claim_head: roster.successor(now, online) == Some(viewer),
        rename_at: roster
            .renamed_at
            .map_or(0, |at| at + FAMILY_RENAME_INTERVAL_SECS),
    })
}

impl super::GameState {
    /// Entry hook: index the member and bring every online member's roster up
    /// to date, the newcomer's included.
    pub async fn player_entered_family(&self, player_id: &PlayerId, auth: &AuthService) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth_copy = auth.clone();
        match auth_db(move || auth_copy.family_id_of(character_id)).await {
            Ok(Some(family_id)) => {
                self.families.write().await.enter(*player_id, family_id);
                self.refresh_family(family_id, auth).await;
            }
            Ok(None) => {
                self.send_direct_message(player_id, ServerMessage::FamilyState { family: None })
                    .await;
            }
            Err(err) => warn!("Failed to load family of character {character_id}: {err}"),
        }
    }

    pub(super) async fn player_left_family(&self, player_id: &PlayerId, auth: &AuthService) {
        if let Some(family_id) = self.forget_player_family(player_id).await {
            self.refresh_family(family_id, auth).await;
        }
    }

    pub(super) async fn forget_player_family(&self, player_id: &PlayerId) -> Option<i64> {
        let mut families = self.families.write().await;
        families
            .invites
            .retain(|(from, to), _| from != player_id && to != player_id);
        families.leave(player_id)
    }

    /// Online members with their character ids.
    async fn online_family_members(&self, family_id: i64) -> Vec<(PlayerId, i64)> {
        let online = self.families.read().await.online(family_id);
        let characters = self.player_characters.read().await;
        online
            .into_iter()
            .filter_map(|id| characters.get(&id).map(|entry| (id, entry.0)))
            .collect()
    }

    /// Send every online member the current roster.
    pub(super) async fn refresh_family(&self, family_id: i64, auth: &AuthService) {
        let members = self.online_family_members(family_id).await;
        if members.is_empty() {
            return;
        }
        let auth = auth.clone();
        let roster = match auth_db(move || auth.family_roster(family_id)).await {
            Ok(Some(roster)) => roster,
            Ok(None) => return,
            Err(err) => {
                warn!("Failed to load family {family_id}: {err}");
                return;
            }
        };
        let online: Vec<i64> = members.iter().map(|(_, id)| *id).collect();
        let now = unix_now();
        for (player_id, character_id) in &members {
            let family = family_info(&roster, *character_id, &online, now);
            self.send_direct_message(player_id, ServerMessage::FamilyState { family })
                .await;
        }
    }

    /// Change the family name shown after a player's name and tell everyone
    /// who sees them.
    async fn show_family_name(&self, player_id: &PlayerId, family: Option<String>) {
        let at = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            if player.family == family {
                return;
            }
            player.family = family.clone();
            (player.position, player.floor_level)
        };
        self.publish_nearby(
            &at.0,
            at.1,
            ServerMessage::PlayerFamilyChanged {
                player_id: *player_id,
                family,
            },
            None,
        )
        .await;
    }

    async fn family_note(&self, player_id: &PlayerId, text: impl std::fmt::Display) {
        self.send_system_message(player_id, format!("Family: {text}"))
            .await;
    }

    /// Run a DB write that charges `cost`. The gold leaves the live balance
    /// first and comes back if the write refuses or fails.
    async fn family_paid_write<F>(
        &self,
        player_id: &PlayerId,
        cost: i64,
        write: F,
    ) -> Result<i64, String>
    where
        F: FnOnce(CharacterSaveData, Vec<ItemRow>) -> Result<Result<i64, &'static str>, AuthError>
            + Send
            + 'static,
    {
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "pay family fees").await {
            return Err("finish your player trade first.".to_string());
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or(SERVER_ERROR)?;
        {
            let mut gold = self.player_gold.write().await;
            let balance = gold.get_mut(player_id).ok_or(SERVER_ERROR)?;
            if *balance < cost {
                return Err(format!("you need {cost} gold."));
            }
            *balance -= cost;
            character.gold = *balance;
        }
        let rows = {
            let inventories = self.inventories.read().await;
            inventories.get(player_id).map(serialize_inventory)
        };
        let outcome = match rows {
            Some(rows) => auth_db(move || write(character, rows)).await,
            None => Err(AuthError::Database("inventory not found".to_string())),
        };
        match outcome {
            Ok(Ok(family_id)) => {
                self.mark_dirty(player_id).await;
                self.send_gold_update(player_id).await;
                Ok(family_id)
            }
            Ok(Err(reason)) => {
                self.adjust_gold(player_id, cost).await;
                Err(reason.to_string())
            }
            Err(err) => {
                warn!("Family write failed: {err}");
                self.adjust_gold(player_id, cost).await;
                Err(SERVER_ERROR.to_string())
            }
        }
    }

    pub async fn create_family(
        &self,
        player_id: &PlayerId,
        merchant_id: &PlayerId,
        name: &str,
        auth: &AuthService,
    ) {
        match self.validate_trader(player_id, merchant_id).await {
            Ok(trader) if trader.is_land_registrar() => {}
            _ => {
                self.family_note(player_id, "visit the Land Registrar to found a family.")
                    .await;
                return;
            }
        }
        let name = name.trim().to_string();
        let auth_copy = auth.clone();
        let saved = name.clone();
        let outcome = self
            .family_paid_write(player_id, FAMILY_CREATE_COST, move |character, rows| {
                auth_copy.create_family(&character, &rows, &saved)
            })
            .await;
        let family_id = match outcome {
            Ok(family_id) => family_id,
            Err(reason) => {
                self.family_note(player_id, reason).await;
                return;
            }
        };
        info!(
            "Player {} founded family '{name}'",
            self.player_name_of(player_id).await
        );
        self.families.write().await.enter(*player_id, family_id);
        self.show_family_name(player_id, Some(name.clone())).await;
        self.family_note(player_id, format!("you founded the {name} family."))
            .await;
        self.refresh_family(family_id, auth).await;
    }

    pub async fn rename_family(&self, player_id: &PlayerId, name: &str, auth: &AuthService) {
        let name = name.trim().to_string();
        let auth_copy = auth.clone();
        let saved = name.clone();
        let outcome = self
            .family_paid_write(player_id, FAMILY_RENAME_COST, move |character, rows| {
                auth_copy.rename_family(&character, &rows, &saved)
            })
            .await;
        let family_id = match outcome {
            Ok(family_id) => family_id,
            Err(reason) => {
                self.family_note(player_id, reason).await;
                return;
            }
        };
        info!(
            "Player {} renamed family {family_id} to '{name}'",
            self.player_name_of(player_id).await
        );
        for (member, _) in self.online_family_members(family_id).await {
            self.show_family_name(&member, Some(name.clone())).await;
            self.family_note(&member, format!("the family is now called {name}."))
                .await;
        }
        self.refresh_family(family_id, auth).await;
    }

    pub async fn invite_to_family(
        &self,
        inviter_id: &PlayerId,
        target_name: &str,
        auth: &AuthService,
    ) {
        let target_name = target_name.trim();
        if target_name.is_empty()
            || target_name.chars().count() > super::party::MAX_TARGET_NAME_CHARS
        {
            self.family_note(inviter_id, "name someone to invite.")
                .await;
            return;
        }
        let target_id = self.player_id_by_name(target_name).await;
        let (inviter_name, target) = {
            let players = self.players.read().await;
            let inviter = players.get(inviter_id).map(|p| p.name.clone());
            let target = target_id.and_then(|id| {
                players
                    .get(&id)
                    .map(|p| (id, p.name.clone(), p.is_official_npc))
            });
            (inviter, target)
        };
        let Some(inviter_name) = inviter_name else {
            return;
        };
        let Some((target_id, target_name, target_is_npc)) = target else {
            self.family_note(
                inviter_id,
                format!("no one called {target_name} is online."),
            )
            .await;
            return;
        };
        if target_id == *inviter_id {
            self.family_note(inviter_id, "that's you.").await;
            return;
        }
        if target_is_npc {
            self.family_note(inviter_id, format!("{target_name} is an NPC."))
                .await;
            return;
        }
        let (Some(inviter_character), Some(target_character)) = (
            self.character_id_of(inviter_id).await,
            self.character_id_of(&target_id).await,
        ) else {
            self.family_note(inviter_id, SERVER_ERROR).await;
            return;
        };
        let auth = auth.clone();
        let family_name =
            match auth_db(move || auth.check_family_invite(inviter_character, target_character))
                .await
            {
                Ok(Ok(name)) => name,
                Ok(Err(reason)) => {
                    self.family_note(inviter_id, format!("{target_name}: {reason}"))
                        .await;
                    return;
                }
                Err(err) => {
                    warn!("Failed to check family invite: {err}");
                    self.family_note(inviter_id, SERVER_ERROR).await;
                    return;
                }
            };
        // A block changes only the delivery, never the reply, so the inviter
        // cannot detect it (the party-invite rule).
        let suppressed = self.has_blocked(&target_id, &inviter_name).await;
        // Ok(true) is a fresh invite to deliver; Ok(false) one still pending.
        let outcome = {
            let mut families = self.families.write().await;
            let pending = families.sweep_and_count(inviter_id);
            match families
                .invites
                .get(&(*inviter_id, target_id))
                .map(|invite| invite.answered)
            {
                Some(true) => Err(format!(
                    "{target_name} declined; you can ask again in a couple of minutes."
                )),
                Some(false) => Ok(false),
                None if pending >= FAMILY_PENDING_INVITE_CAP => {
                    Err("you have too many pending invites.".to_string())
                }
                None => {
                    families.invites.insert(
                        (*inviter_id, target_id),
                        PendingConsent::new(FAMILY_INVITE_TTL),
                    );
                    Ok(true)
                }
            }
        };
        match outcome {
            Err(reason) => self.family_note(inviter_id, reason).await,
            Ok(fresh) => {
                if fresh && !suppressed {
                    self.send_direct_message(
                        &target_id,
                        ServerMessage::FamilyInviteReceived {
                            inviter_id: *inviter_id,
                            inviter_name,
                            family_name,
                        },
                    )
                    .await;
                }
                self.family_note(inviter_id, format!("invited {target_name}."))
                    .await;
            }
        }
    }

    pub async fn respond_to_family_invite(
        &self,
        target_id: &PlayerId,
        inviter_id: &PlayerId,
        accept: bool,
        auth: &AuthService,
    ) {
        let usable = {
            let mut families = self.families.write().await;
            answer_consent(&mut families.invites, (*inviter_id, *target_id), accept)
        };
        if !usable {
            self.family_note(target_id, "that invite has expired.")
                .await;
            return;
        }
        let target_name = self.player_name_of(target_id).await;
        if !accept {
            self.family_note(inviter_id, format!("{target_name} declined your invite."))
                .await;
            return;
        }
        let (Some(inviter_character), Some(target_character)) = (
            self.character_id_of(inviter_id).await,
            self.character_id_of(target_id).await,
        ) else {
            self.family_note(target_id, "that invite is no longer valid.")
                .await;
            return;
        };
        let auth_copy = auth.clone();
        let joined = auth_db(move || {
            let family_id = match auth_copy.join_family(inviter_character, target_character)? {
                Ok(family_id) => family_id,
                Err(reason) => return Ok(Err(reason)),
            };
            Ok(Ok((family_id, auth_copy.family_name_of(target_character)?)))
        })
        .await;
        let (family_id, family_name) = match joined {
            Ok(Ok((family_id, Some(name)))) => (family_id, name),
            Ok(Ok((_, None))) => return,
            Ok(Err(reason)) => {
                let reason = reason
                    .strip_prefix("They ")
                    .map_or_else(|| reason.to_string(), |rest| format!("you {rest}"));
                self.family_note(target_id, reason).await;
                return;
            }
            Err(err) => {
                warn!("Failed to join family: {err}");
                self.family_note(target_id, SERVER_ERROR).await;
                return;
            }
        };
        info!("Player {target_name} joined family '{family_name}'");
        self.families.write().await.enter(*target_id, family_id);
        self.show_family_name(target_id, Some(family_name.clone()))
            .await;
        for (member, _) in self.online_family_members(family_id).await {
            self.family_note(
                &member,
                format!("{target_name} joined the {family_name} family."),
            )
            .await;
        }
        self.refresh_family(family_id, auth).await;
    }

    /// Leave, or kick `target_character` when given.
    pub async fn remove_from_family(
        &self,
        player_id: &PlayerId,
        target_character: Option<i64>,
        auth: &AuthService,
    ) {
        let Some(actor) = self.character_id_of(player_id).await else {
            return;
        };
        let target = target_character.unwrap_or(actor);
        let auth_copy = auth.clone();
        let departure = match auth_db(move || {
            let names = auth_copy.character_names(&[target])?;
            Ok((auth_copy.remove_family_member(actor, target)?, names))
        })
        .await
        {
            Ok((Ok(departure), names)) => (departure, names),
            Ok((Err(reason), _)) => {
                self.family_note(player_id, reason).await;
                return;
            }
            Err(err) => {
                warn!("Failed to remove family member: {err}");
                self.family_note(player_id, SERVER_ERROR).await;
                return;
            }
        };
        let (departure, names) = departure;
        let target_name = names
            .into_iter()
            .next()
            .map_or_else(|| target.to_string(), |(_, name)| name);
        let kicked = target != actor;
        info!(
            "Character {target_name} left family '{}' ({})",
            departure.family_name,
            if kicked { "kicked" } else { "left" }
        );
        if let Some(target_player) = self.player_id_of_character(target).await {
            self.forget_player_family(&target_player).await;
            self.show_family_name(&target_player, None).await;
            self.send_direct_message(&target_player, ServerMessage::FamilyState { family: None })
                .await;
            let note = match (kicked, departure.dissolved) {
                (true, _) => format!(
                    "you were removed from the {} family.",
                    departure.family_name
                ),
                (false, true) => format!("the {} family is dissolved.", departure.family_name),
                (false, false) => format!("you left the {} family.", departure.family_name),
            };
            self.family_note(&target_player, note).await;
        }
        if departure.dissolved {
            return;
        }
        let verb = if kicked { "was removed from" } else { "left" };
        for (member, _) in self.online_family_members(departure.family_id).await {
            self.family_note(&member, format!("{target_name} {verb} the family."))
                .await;
        }
        self.refresh_family(departure.family_id, auth).await;
    }

    /// Run a rule-checked family change and refresh the roster on success.
    async fn family_change<F>(&self, player_id: &PlayerId, auth: &AuthService, change: F) -> bool
    where
        F: FnOnce(AuthService, i64) -> Result<Result<i64, &'static str>, AuthError>
            + Send
            + 'static,
    {
        let Some(actor) = self.character_id_of(player_id).await else {
            return false;
        };
        let auth_copy = auth.clone();
        match auth_db(move || change(auth_copy, actor)).await {
            Ok(Ok(family_id)) => {
                self.refresh_family(family_id, auth).await;
                true
            }
            Ok(Err(reason)) => {
                self.family_note(player_id, reason).await;
                false
            }
            Err(err) => {
                warn!("Family change failed: {err}");
                self.family_note(player_id, SERVER_ERROR).await;
                false
            }
        }
    }

    pub async fn set_family_member_rank(
        &self,
        player_id: &PlayerId,
        target: i64,
        rank: u8,
        auth: &AuthService,
    ) {
        self.family_change(player_id, auth, move |auth, actor| {
            auth.set_family_member_rank(actor, target, rank)
        })
        .await;
    }

    pub async fn transfer_family_head(
        &self,
        player_id: &PlayerId,
        target: i64,
        auth: &AuthService,
    ) {
        let changed = self
            .family_change(player_id, auth, move |auth, actor| {
                auth.transfer_family_head(actor, target)
            })
            .await;
        if changed {
            info!(
                "Player {} handed their family to character {target}",
                self.player_name_of(player_id).await
            );
        }
    }

    pub async fn claim_family_head(&self, player_id: &PlayerId, auth: &AuthService) {
        let family_id = self.families.read().await.of_player.get(player_id).copied();
        let Some(family_id) = family_id else {
            self.family_note(player_id, "you do not belong to a family.")
                .await;
            return;
        };
        let online: Vec<i64> = self
            .online_family_members(family_id)
            .await
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let changed = self
            .family_change(player_id, auth, move |auth, actor| {
                auth.claim_family_head(actor, &online)
            })
            .await;
        if changed {
            info!(
                "Player {} succeeded the absent head of family {family_id}",
                self.player_name_of(player_id).await
            );
        }
    }

    pub async fn set_family_rank(
        &self,
        player_id: &PlayerId,
        rank: u8,
        label: String,
        permissions: u32,
        auth: &AuthService,
    ) {
        self.family_change(player_id, auth, move |auth, actor| {
            auth.set_family_rank(actor, rank, label.trim(), permissions)
        })
        .await;
    }

    pub async fn attach_estate_to_family(
        &self,
        player_id: &PlayerId,
        attach: bool,
        auth: &AuthService,
    ) {
        self.tick_land_taxes(auth).await;
        self.family_change(player_id, auth, move |auth, actor| {
            auth.attach_family_estate(actor, attach)
        })
        .await;
    }

    pub async fn deposit_family_estate(
        &self,
        player_id: &PlayerId,
        estate_id: i64,
        amount: i64,
        auth: &AuthService,
    ) {
        self.tick_land_taxes(auth).await;
        let outcome = self
            .try_deposit_family_estate(player_id, estate_id, amount, auth)
            .await;
        match outcome {
            Ok(family_id) => {
                self.family_note(
                    player_id,
                    format!("paid {amount} gold into the estate's tax account."),
                )
                .await;
                self.refresh_family(family_id, auth).await;
            }
            Err(reason) => self.family_note(player_id, reason).await,
        }
    }

    async fn try_deposit_family_estate(
        &self,
        player_id: &PlayerId,
        estate_id: i64,
        amount: i64,
        auth: &AuthService,
    ) -> Result<i64, &'static str> {
        if amount <= 0 {
            return Err("enter a positive amount.");
        }
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "pay estate taxes").await {
            return Err("finish your player trade first.");
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or(SERVER_ERROR)?;
        {
            let mut gold = self.player_gold.write().await;
            let balance = gold.get_mut(player_id).ok_or(SERVER_ERROR)?;
            if *balance < amount {
                return Err("you do not have enough gold.");
            }
            character.gold = *balance;
            *balance -= amount;
        }
        let rows = {
            let inventories = self.inventories.read().await;
            inventories.get(player_id).map(serialize_inventory)
        };
        let auth = auth.clone();
        let outcome = match rows {
            Some(rows) => {
                auth_db(move || auth.deposit_family_estate(character, &rows, estate_id, amount))
                    .await
            }
            None => Err(AuthError::Database("inventory not found".to_string())),
        };
        match outcome {
            Ok(Ok((_, family_id))) => {
                self.mark_dirty(player_id).await;
                self.send_gold_update(player_id).await;
                Ok(family_id)
            }
            Ok(Err(reason)) => {
                self.adjust_gold(player_id, amount).await;
                Err(reason)
            }
            Err(err) => {
                warn!("Failed to pay family estate taxes: {err}");
                self.adjust_gold(player_id, amount).await;
                Err(SERVER_ERROR)
            }
        }
    }

    /// After characters were deleted: repair the families they left and
    /// refresh the members still online.
    pub(super) async fn settle_families(&self, auth: &AuthService) {
        let auth_copy = auth.clone();
        match auth_db(move || auth_copy.settle_families()).await {
            Ok(changed) => {
                for family_id in changed {
                    self.refresh_family(family_id, auth).await;
                }
            }
            Err(err) => warn!("Failed to settle families: {err}"),
        }
    }
}
