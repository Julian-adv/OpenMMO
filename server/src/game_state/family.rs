//! Families (doc/FAMILY_SYSTEM.md). Membership, ranks and estate links live
//! in the DB (`auth_family.rs`); this side keeps which members are online,
//! pending invites, and pushes rosters and name changes.

use std::collections::HashMap;

use onlinerpg_shared::family::{
    FamilyEstate, FamilyInfo, FamilyMember, FamilyMergeProposal, FamilyMergeTerms,
    FAMILY_DISSOLVE_STAMPS, FAMILY_FOUND_STAMPS, FAMILY_INVITE_TTL, FAMILY_MERGE_TTL,
    FAMILY_RENAME_INTERVAL_SECS, FAMILY_RENAME_STAMPS, FAMILY_STAMP_ITEM,
};
use tracing::{info, warn};

use super::auth_db;
use super::consent::{answer_consent, sweep_and_count, PendingConsent};
use super::inventory::hold_by_def;
use crate::auth::{unix_now, AuthError, AuthService, FamilyDeparture, FamilyEmblem, FamilyRoster};
use crate::types::{PlayerId, ServerMessage};

const FAMILY_PENDING_INVITE_CAP: usize = 5;
const SERVER_ERROR: &str = "server error, try again.";

/// Never held while taking another `GameState` lock.
#[derive(Default)]
pub(crate) struct Families {
    online: HashMap<i64, Vec<PlayerId>>,
    of_player: HashMap<PlayerId, i64>,
    invites: HashMap<(PlayerId, PlayerId), PendingConsent>,
    /// Emblems of families with a member online.
    emblems: HashMap<i64, FamilyEmblem>,
    /// Online members who keep their own cape print.
    own_capes: std::collections::HashSet<PlayerId>,
    /// Merge offers by (proposer, target head), with their terms.
    merges: HashMap<(PlayerId, PlayerId), PendingConsent>,
    merge_terms: HashMap<(PlayerId, PlayerId), FamilyMergeTerms>,
}

impl Families {
    fn enter(&mut self, player_id: PlayerId, family_id: i64) {
        self.leave(&player_id);
        self.of_player.insert(player_id, family_id);
        self.online.entry(family_id).or_default().push(player_id);
    }

    fn leave(&mut self, player_id: &PlayerId) -> Option<i64> {
        self.own_capes.remove(player_id);
        let family_id = self.of_player.remove(player_id)?;
        if let Some(list) = self.online.get_mut(&family_id) {
            list.retain(|id| id != player_id);
            if list.is_empty() {
                self.online.remove(&family_id);
                self.emblems.remove(&family_id);
            }
        }
        Some(family_id)
    }

    fn online(&self, family_id: i64) -> Vec<PlayerId> {
        self.online.get(&family_id).cloned().unwrap_or_default()
    }

    fn emblem_of(&self, player_id: &PlayerId) -> Option<FamilyEmblem> {
        if self.own_capes.contains(player_id) {
            return None;
        }
        let family_id = self.of_player.get(player_id)?;
        self.emblems.get(family_id).filter(|e| e.is_set()).cloned()
    }
}

/// The cape colour and texture a player shows: their family's emblem
/// covers whatever cape they wear.
pub(crate) fn shown_cape(
    back: Option<&str>,
    color: Option<String>,
    texture: Option<String>,
    emblem: Option<&FamilyEmblem>,
) -> (Option<String>, Option<String>) {
    match (back, emblem) {
        (Some(_), Some(emblem)) => (emblem.color.clone().or(color), emblem.texture.clone()),
        _ => (color, texture),
    }
}

/// `successor` is who may take over from an absent head, if anyone.
fn family_info(
    roster: &FamilyRoster,
    viewer: i64,
    online: &[i64],
    successor: Option<i64>,
) -> Option<FamilyInfo> {
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
        can_claim_head: successor == Some(viewer),
        rename_at: roster
            .renamed_at
            .map_or(0, |at| at + FAMILY_RENAME_INTERVAL_SECS),
        emblem_texture: roster.emblem.texture.clone(),
        emblem_color: roster.emblem.color.clone(),
        my_cape_emblem: me.cape_emblem,
        seat_estate_id: roster.seat_estate_id,
    })
}

impl super::GameState {
    /// Entry hook, with the family id read at login: index the member and
    /// bring every online member's roster up to date, the newcomer's included.
    pub async fn player_entered_family(
        &self,
        player_id: &PlayerId,
        family_id: Option<i64>,
        auth: &AuthService,
    ) {
        if let Some(family_id) = family_id {
            self.families.write().await.enter(*player_id, family_id);
            self.announce_to_family(family_id, player_id, "is online.")
                .await;
            self.refresh_family(family_id, auth).await;
        }
    }

    pub(super) async fn player_left_family(&self, player_id: &PlayerId, auth: &AuthService) {
        if let Some(family_id) = self.forget_player_family(player_id).await {
            self.announce_to_family(family_id, player_id, "went offline.")
                .await;
            self.refresh_family(family_id, auth).await;
        }
    }

    pub(crate) async fn family_emblem_of(&self, player_id: &PlayerId) -> Option<FamilyEmblem> {
        self.families.read().await.emblem_of(player_id)
    }

    /// Re-send the players' worn capes after their family emblem changed.
    async fn refresh_family_capes(&self, players: &[PlayerId]) {
        for player_id in players {
            if let Some(inventory) = self.get_player_inventory(player_id).await {
                self.set_player_gear(player_id, &inventory).await;
            }
        }
    }

    pub async fn set_family_seat(&self, player_id: &PlayerId, estate_id: i64, auth: &AuthService) {
        self.family_change(player_id, auth, move |auth, actor| {
            auth.set_family_seat(actor, estate_id)
        })
        .await;
    }

    pub async fn set_family_cape_emblem(
        &self,
        player_id: &PlayerId,
        show: bool,
        auth: &AuthService,
    ) {
        let changed = self
            .family_change(player_id, auth, move |auth, actor| {
                auth.set_family_cape_emblem(actor, show)
            })
            .await;
        if changed {
            self.refresh_family_capes(&[*player_id]).await;
        }
    }

    pub async fn set_family_emblem(
        &self,
        player_id: &PlayerId,
        texture: Option<String>,
        color: Option<String>,
        auth: &AuthService,
    ) {
        let color = match color.as_deref().map(super::inventory::normalize_hex_color) {
            Some(None) => {
                self.family_note(player_id, "that is not a colour.").await;
                return;
            }
            other => other.flatten(),
        };
        if let Some(hash) = &texture {
            if !self.cape_textures.is_wearable(hash).await {
                self.family_note(player_id, "that picture is no longer available.")
                    .await;
                return;
            }
        }
        let emblem = FamilyEmblem { texture, color };
        let saved = emblem.clone();
        let changed = self
            .family_change(player_id, auth, move |auth, actor| {
                auth.set_family_emblem(actor, &saved)
            })
            .await;
        if !changed {
            return;
        }
        let Some(family_id) = self.families.read().await.of_player.get(player_id).copied() else {
            return;
        };
        info!(
            "Player {} set the emblem of family {family_id}: {emblem:?}",
            self.player_name_of(player_id).await
        );
        let members = self.families.read().await.online(family_id);
        self.refresh_family_capes(&members).await;
        self.refresh_family_furniture(family_id, auth).await;
    }

    /// Tell the other online members that `player_id` did something.
    async fn announce_to_family(&self, family_id: i64, player_id: &PlayerId, what: &str) {
        let name = self.player_name_of(player_id).await;
        let others: Vec<PlayerId> = self.families.read().await.online(family_id);
        for member in others.iter().filter(|id| *id != player_id) {
            self.family_note(member, format!("{name} {what}")).await;
        }
    }

    pub async fn send_family_storage_log(&self, player_id: &PlayerId, auth: &AuthService) {
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        let auth = auth.clone();
        match auth_db(move || auth.family_storage_log(character_id)).await {
            Ok(Some(entries)) => {
                self.send_direct_message(player_id, ServerMessage::FamilyStorageLog { entries })
                    .await
            }
            Ok(None) => {
                self.family_note(player_id, "you do not belong to a family.")
                    .await
            }
            Err(err) => {
                warn!("Failed to read family storage log: {err}");
                self.family_note(player_id, SERVER_ERROR).await;
            }
        }
    }

    pub async fn send_family_chat(&self, player_id: &PlayerId, message: String) {
        if message.trim().is_empty() {
            return;
        }
        let Some(sender_name) = ({
            let players = self.players.read().await;
            players.get(player_id).map(|p| p.name.clone())
        }) else {
            return;
        };
        if self.refuse_if_muted(player_id, &sender_name, "chat").await {
            return;
        }
        let members = {
            let families = self.families.read().await;
            families
                .of_player
                .get(player_id)
                .map(|id| families.online(*id))
                .unwrap_or_default()
        };
        if members.is_empty() {
            self.family_note(player_id, "you do not belong to a family.")
                .await;
            return;
        }
        let recipients = self
            .unblocked_recipients(player_id, &sender_name, members)
            .await;
        info!(from = %sender_name, len = message.len(), "family chat");
        self.send_direct_message_to_players(
            &recipients,
            ServerMessage::FamilyChatMessage {
                from: sender_name,
                message,
            },
        )
        .await;
    }

    pub(super) async fn forget_player_family(&self, player_id: &PlayerId) -> Option<i64> {
        let mut families = self.families.write().await;
        families
            .invites
            .retain(|(from, to), _| from != player_id && to != player_id);
        families
            .merges
            .retain(|(from, to), _| from != player_id && to != player_id);
        families
            .merge_terms
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
        let mut emblem = roster.emblem.clone();
        if let Some(hash) = &emblem.texture {
            if self.cape_textures.is_blocked(hash).await {
                emblem.texture = None;
            }
        }
        {
            let mut families = self.families.write().await;
            families.emblems.insert(family_id, emblem);
            for (player_id, character_id) in &members {
                let own = roster
                    .members
                    .iter()
                    .any(|m| m.character_id == *character_id && !m.cape_emblem);
                if own {
                    families.own_capes.insert(*player_id);
                } else {
                    families.own_capes.remove(player_id);
                }
            }
        }
        let online: Vec<i64> = members.iter().map(|(_, id)| *id).collect();
        let successor = roster.successor(unix_now(), &online);
        for (player_id, character_id) in &members {
            let family = family_info(&roster, *character_id, &online, successor);
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

    /// File family papers with the land registrar: run a DB write that spends
    /// `stamps` revenue stamps. The stamps leave the bag first and return if
    /// the write refuses or fails. A refusal has already been told to the
    /// player when this returns `None`.
    async fn family_filing<T, F>(
        &self,
        player_id: &PlayerId,
        merchant_id: &PlayerId,
        stamps: u32,
        write: F,
    ) -> Option<T>
    where
        T: Send + 'static,
        F: FnOnce(i64) -> Result<Result<T, &'static str>, AuthError> + Send + 'static,
    {
        let refusal = if !self.at_land_registrar(player_id, merchant_id).await {
            Some("file family papers with the Land Registrar.".to_string())
        } else if self
            .reject_if_trading(player_id, "file family papers")
            .await
        {
            Some("finish your player trade first.".to_string())
        } else {
            None
        };
        if let Some(reason) = refusal {
            self.family_note(player_id, reason).await;
            return None;
        }
        let character_id = self.character_id_of(player_id).await?;
        let hold = self
            .inventories
            .write()
            .await
            .get_mut(player_id)
            .and_then(|inv| hold_by_def(&mut inv.bag, FAMILY_STAMP_ITEM, stamps));
        let Some(hold) = hold else {
            self.family_note(
                player_id,
                format!("you need {stamps} revenue stamp(s); the Land Registrar sells them."),
            )
            .await;
            return None;
        };
        let outcome = match auth_db(move || write(character_id)).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(reason)) => Err(reason),
            Err(err) => {
                warn!("Family filing failed: {err}");
                Err(SERVER_ERROR)
            }
        };
        let saved = outcome.as_ref().map(|_| ()).map_err(|_| "not saved");
        if self
            .finish_bag_write(player_id, hold, &[], saved)
            .await
            .is_ok()
        {
            self.log_consumed(
                player_id,
                std::iter::repeat_n(FAMILY_STAMP_ITEM, stamps as usize),
            )
            .await;
        }
        match outcome {
            Ok(value) => Some(value),
            Err(reason) => {
                self.family_note(player_id, reason).await;
                None
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
        let name = name.trim().to_string();
        let (auth_copy, saved) = (auth.clone(), name.clone());
        let Some(family_id) = self
            .family_filing(player_id, merchant_id, FAMILY_FOUND_STAMPS, move |id| {
                auth_copy.create_family(id, &saved)
            })
            .await
        else {
            return;
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

    pub async fn rename_family(
        &self,
        player_id: &PlayerId,
        merchant_id: &PlayerId,
        name: &str,
        auth: &AuthService,
    ) {
        let name = name.trim().to_string();
        let (auth_copy, saved) = (auth.clone(), name.clone());
        let Some(family_id) = self
            .family_filing(player_id, merchant_id, FAMILY_RENAME_STAMPS, move |id| {
                auth_copy.rename_family(id, &saved)
            })
            .await
        else {
            return;
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
            let inviter = players
                .get(inviter_id)
                .filter(|p| !p.is_official_npc)
                .map(|p| p.name.clone());
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
                Ok(Err(block)) => {
                    self.family_note(inviter_id, format!("{target_name}: {}", block.to_inviter()))
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
            let pending = sweep_and_count(&mut families.invites, inviter_id);
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
        let joined =
            auth_db(move || auth_copy.join_family(inviter_character, target_character)).await;
        let (family_id, family_name) = match joined {
            Ok(Ok(family)) => family,
            Ok(Err(block)) => {
                self.family_note(target_id, block.to_target()).await;
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
        self.refresh_family(family_id, auth).await;
        self.refresh_family_capes(&[*target_id]).await;
        for (member, _) in self.online_family_members(family_id).await {
            self.family_note(
                &member,
                format!("{target_name} joined the {family_name} family."),
            )
            .await;
        }
        self.refresh_family(family_id, auth).await;
    }

    /// The head, as the last member, dissolves the family at the registrar.
    pub async fn dissolve_family(
        &self,
        player_id: &PlayerId,
        merchant_id: &PlayerId,
        auth: &AuthService,
    ) {
        let auth_copy = auth.clone();
        let departure = self
            .family_filing(player_id, merchant_id, FAMILY_DISSOLVE_STAMPS, move |id| {
                Ok(auth_copy
                    .dissolve_family(id)?
                    .map(|departure| (id, departure)))
            })
            .await;
        if let Some((actor, departure)) = departure {
            self.family_departed(actor, false, departure, auth).await;
        }
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
        match auth_db(move || auth_copy.remove_family_member(actor, target)).await {
            Ok(Ok(departure)) => {
                self.family_departed(target, target != actor, departure, auth)
                    .await
            }
            Ok(Err(reason)) => self.family_note(player_id, reason).await,
            Err(err) => {
                warn!("Failed to remove family member: {err}");
                self.family_note(player_id, SERVER_ERROR).await;
            }
        }
    }

    /// Character `target` is out of their family: drop the name they showed
    /// and tell them and whoever remains.
    async fn family_departed(
        &self,
        target: i64,
        kicked: bool,
        departure: FamilyDeparture,
        auth: &AuthService,
    ) {
        let FamilyDeparture {
            family_id,
            family_name,
            target_name,
            dissolved,
        } = departure;
        info!(
            "Character {target_name} left family '{family_name}' ({})",
            if kicked { "kicked" } else { "left" }
        );
        if let Some(target_player) = self.player_id_of_character(target).await {
            self.forget_player_family(&target_player).await;
            self.refresh_family_capes(&[target_player]).await;
            self.show_family_name(&target_player, None).await;
            self.send_direct_message(&target_player, ServerMessage::FamilyState { family: None })
                .await;
            let note = match (kicked, dissolved) {
                (true, _) => format!("you were removed from the {family_name} family."),
                (false, true) => format!("the {family_name} family is dissolved."),
                (false, false) => format!("you left the {family_name} family."),
            };
            self.family_note(&target_player, note).await;
        }
        self.refresh_family_furniture(family_id, auth).await;
        if dissolved {
            return;
        }
        let verb = if kicked { "was removed from" } else { "left" };
        for (member, _) in self.online_family_members(family_id).await {
            self.family_note(&member, format!("{target_name} {verb} the family."))
                .await;
        }
        self.refresh_family(family_id, auth).await;
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
                auth.claim_family_head(actor, family_id, &online)
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
        let changed = self
            .family_change(player_id, auth, move |auth, actor| {
                auth.attach_family_estate(actor, attach)
            })
            .await;
        if changed {
            if let Some(family_id) = self.families.read().await.of_player.get(player_id).copied() {
                self.refresh_family_furniture(family_id, auth).await;
            }
        }
    }

    pub async fn deposit_family_estate(
        &self,
        player_id: &PlayerId,
        estate_id: i64,
        amount: i64,
        auth: &AuthService,
    ) {
        if amount <= 0 {
            self.family_note(player_id, "enter a positive amount.")
                .await;
            return;
        }
        self.tick_land_taxes(auth).await;
        let auth_copy = auth.clone();
        let outcome = self
            .estate_gold_write(player_id, amount, 0, move |character, rows| {
                auth_copy.deposit_family_estate(character, &rows, estate_id, amount)
            })
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

    pub async fn propose_family_merge(
        &self,
        player_id: &PlayerId,
        head_name: &str,
        terms: FamilyMergeTerms,
        auth: &AuthService,
    ) {
        let Some(target_id) = self.player_id_by_name(head_name.trim()).await else {
            self.family_note(player_id, format!("no one called {head_name} is online."))
                .await;
            return;
        };
        let (Some(proposer), Some(target)) = (
            self.character_id_of(player_id).await,
            self.character_id_of(&target_id).await,
        ) else {
            self.family_note(player_id, SERVER_ERROR).await;
            return;
        };
        let auth = auth.clone();
        let (proposer_family, target_family, members) =
            match auth_db(move || auth.check_family_merge(proposer, target)).await {
                Ok(Ok(check)) => check,
                Ok(Err(reason)) => {
                    self.family_note(player_id, reason).await;
                    return;
                }
                Err(err) => {
                    warn!("Failed to check family merge: {err}");
                    self.family_note(player_id, SERVER_ERROR).await;
                    return;
                }
            };
        let proposer_name = self.player_name_of(player_id).await;
        let target_name = self.player_name_of(&target_id).await;
        let refusal = {
            let mut families = self.families.write().await;
            let key = (*player_id, target_id);
            let open = families
                .merges
                .get(&key)
                .filter(|offer| offer.expires_at > std::time::Instant::now())
                .map(|offer| offer.answered);
            match open {
                Some(true) => Some(format!(
                    "{target_name} declined; you can ask again in a couple of minutes."
                )),
                Some(false) => Some(format!("{target_name} is still considering your offer.")),
                None => {
                    families
                        .merges
                        .insert(key, PendingConsent::new(FAMILY_MERGE_TTL));
                    families.merge_terms.insert(key, terms);
                    None
                }
            }
        };
        if let Some(reason) = refusal {
            self.family_note(player_id, reason).await;
            return;
        }
        let suppressed = self.has_blocked(&target_id, &proposer_name).await;
        if !suppressed {
            self.send_direct_message(
                &target_id,
                ServerMessage::FamilyMergeProposal {
                    proposal: FamilyMergeProposal {
                        proposer_id: *player_id,
                        proposer_name,
                        proposer_family,
                        target_family,
                        terms,
                        members,
                    },
                },
            )
            .await;
        }
        self.family_note(player_id, format!("offered {target_name} a merge."))
            .await;
    }

    pub async fn respond_to_family_merge(
        &self,
        target_id: &PlayerId,
        proposer_id: &PlayerId,
        accept: bool,
        auth: &AuthService,
    ) {
        let key = (*proposer_id, *target_id);
        let terms = {
            let mut families = self.families.write().await;
            let usable = answer_consent(&mut families.merges, key, accept);
            let terms = families.merge_terms.get(&key).copied();
            if accept {
                families.merge_terms.remove(&key);
            }
            terms.filter(|_| usable)
        };
        let Some(terms) = terms else {
            self.family_note(target_id, "that merge offer has expired.")
                .await;
            return;
        };
        let target_name = self.player_name_of(target_id).await;
        if !accept {
            self.family_note(proposer_id, format!("{target_name} declined the merge."))
                .await;
            return;
        }
        let (Some(proposer), Some(target)) = (
            self.character_id_of(proposer_id).await,
            self.character_id_of(target_id).await,
        ) else {
            self.family_note(target_id, "that merge offer is no longer valid.")
                .await;
            return;
        };
        let auth_copy = auth.clone();
        let merged = auth_db(move || auth_copy.merge_families(proposer, target, terms)).await;
        let (survivor, absorbed) = match merged {
            Ok(Ok(ids)) => ids,
            Ok(Err(reason)) => {
                self.family_note(target_id, reason).await;
                self.family_note(proposer_id, reason).await;
                return;
            }
            Err(err) => {
                warn!("Family merge failed: {err}");
                self.family_note(target_id, SERVER_ERROR).await;
                return;
            }
        };
        let moved = {
            let mut families = self.families.write().await;
            let moved = families.online(absorbed);
            for member in &moved {
                families.enter(*member, survivor);
            }
            families.emblems.remove(&absorbed);
            moved
        };
        let auth_copy = auth.clone();
        let name = match auth_db(move || auth_copy.family_roster(survivor)).await {
            Ok(Some(roster)) => roster.name,
            _ => String::new(),
        };
        info!("Family {absorbed} merged into family {survivor} '{name}'");
        for member in &moved {
            self.show_family_name(member, Some(name.clone())).await;
        }
        self.refresh_family(survivor, auth).await;
        let members = self.families.read().await.online(survivor);
        for member in &members {
            self.family_note(member, format!("the families are merged as {name}."))
                .await;
        }
        self.refresh_family_capes(&members).await;
        self.refresh_family_furniture(absorbed, auth).await;
        self.refresh_family_furniture(survivor, auth).await;
    }

    /// After characters were deleted: repair the families they left and
    /// refresh the members still online.
    pub(super) async fn settle_families(&self, auth: &AuthService) {
        let auth_copy = auth.clone();
        match auth_db(move || auth_copy.settle_families()).await {
            Ok(changed) => {
                for family_id in changed {
                    self.refresh_family(family_id, auth).await;
                    self.refresh_family_furniture(family_id, auth).await;
                }
            }
            Err(err) => warn!("Failed to settle families: {err}"),
        }
    }
}
