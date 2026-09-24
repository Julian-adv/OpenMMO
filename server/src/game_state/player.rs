use super::localization::PlayerMessage;
use super::KickNotice;
use crate::auth::{AuthError, AuthService, CharacterSaveData, ItemRow};
use crate::types::{CharacterAttributes, Player, PlayerId, Position, ServerMessage};
use crate::world_config::world_config;
use bytes::Bytes;
use onlinerpg_shared::estate_storage::{
    estate_storage_def, is_estate_storage_item, INTERACTION_RANGE,
};
use onlinerpg_shared::housing::MAX_FLOOR_LEVEL;
use onlinerpg_shared::inventory::{EquipSlot, PlayerInventory};
use onlinerpg_shared::wrap_world_x;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Arrival ring radius (m) around a teleport's center (`arrival_beside`);
/// golden-angle spacing keeps simultaneous arrivals apart.
const ARRIVAL_RING_RADIUS: f32 = 1.6;

/// Golden angle in radians, spreading arrival spots around the ring.
const GOLDEN_ANGLE_RAD: f32 = 2.399_963;

/// Keep the shared housing limit representable on the signed wire.
const _: () = assert!(MAX_FLOOR_LEVEL <= i8::MAX as u8);

fn exceeds_positive_floor_limit(floor_level: i8) -> bool {
    floor_level > MAX_FLOOR_LEVEL as i8
}

/// Keep legacy out-of-range rows from re-entering live state.
pub(crate) fn restored_floor_level(saved: i8) -> i8 {
    if exceeds_positive_floor_limit(saved) {
        0
    } else {
        saved
    }
}

/// Run a blocking DB save on the blocking pool and report whether it committed.
async fn flush_save<F>(op: F, what: &str) -> bool
where
    F: FnOnce() -> Result<(), AuthError> + Send + 'static,
{
    match tokio::task::spawn_blocking(op).await {
        Ok(Ok(())) => true,
        Ok(Err(e)) => {
            error!("Failed to save {}: {}", what, e);
            false
        }
        Err(e) => {
            error!("spawn_blocking panicked while saving {}: {}", what, e);
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn build_save_data(
    player: &Player,
    character_id: i64,
    xp: u64,
    gold: i64,
    satiation: u32,
    active_ammo: Option<String>,
    mana: Option<u32>,
    dungeon_epoch: i64,
) -> CharacterSaveData {
    CharacterSaveData {
        character_id,
        x: player.position.x,
        y: player.position.y,
        z: player.position.z,
        rotation: player.rotation,
        xp,
        level: player.level,
        max_hp: player.max_health,
        health: player.health,
        mana,
        floor_level: player.floor_level,
        dungeon_epoch: (player.floor_level < 0).then_some(dungeon_epoch),
        gold,
        satiation,
        active_ammo,
    }
}

impl super::GameState {
    pub async fn get_or_assign_player_number(&self, player_id: &PlayerId) -> u32 {
        let mut id_state = self.id_state.write().await;
        if let Some(player_number) = id_state.player_numbers.get(player_id).copied() {
            player_number
        } else {
            id_state.next_player_number = id_state.next_player_number.saturating_add(1);
            let player_number = id_state.next_player_number;
            id_state.player_numbers.insert(*player_id, player_number);
            player_number
        }
    }

    pub async fn register_connection_channel(
        &self,
        player_id: &PlayerId,
    ) -> mpsc::UnboundedReceiver<Bytes> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut channels = self.direct_channels.write().await;
        channels.insert(*player_id, tx);
        drop(channels);
        self.reset_world_view(player_id).await;
        rx
    }

    pub async fn unregister_connection_channel(&self, player_id: &PlayerId) {
        let mut channels = self.direct_channels.write().await;
        channels.remove(player_id);
        self.interest_lock().remove_view(*player_id);
    }

    pub async fn send_direct_message(&self, player_id: &PlayerId, msg: ServerMessage) {
        let Some(bytes) = super::encode_server_msg(&msg) else {
            return;
        };
        let channels = self.direct_channels.read().await;
        if let Some(tx) = channels.get(player_id) {
            let _ = tx.send(bytes);
        }
    }

    /// Private system chat line for one player (command replies, action
    /// feedback).
    pub async fn send_system_message(
        &self,
        player_id: &PlayerId,
        message: impl Into<PlayerMessage>,
    ) {
        let message = message.into();
        self.send_direct_message(
            player_id,
            ServerMessage::SystemMessage {
                localization: message.localization,
                message: message.message,
            },
        )
        .await;
    }

    pub async fn send_direct_message_to_players(
        &self,
        player_ids: &[PlayerId],
        msg: ServerMessage,
    ) {
        self.send_direct_message_to_players_except(player_ids, msg, None)
            .await;
    }

    /// Serializes once and shares the bytes: every connection would otherwise
    /// re-encode the same message, which adds up on move fanout at scale.
    pub async fn send_direct_message_to_players_except(
        &self,
        player_ids: &[PlayerId],
        msg: ServerMessage,
        skip_player_id: Option<&PlayerId>,
    ) {
        let is_skipped = |id: &PlayerId| skip_player_id.is_some_and(|skip_id| skip_id == id);
        if !player_ids.iter().any(|id| !is_skipped(id)) {
            return;
        }
        let Some(bytes) = super::encode_server_msg(&msg) else {
            return;
        };
        let channels = self.direct_channels.read().await;
        for player_id in player_ids {
            if is_skipped(player_id) {
                continue;
            }
            if let Some(tx) = channels.get(player_id) {
                let _ = tx.send(bytes.clone());
            }
        }
    }

    pub(crate) async fn publish_nearby(
        &self,
        position: &Position,
        floor_level: i8,
        msg: ServerMessage,
        skip_player_id: Option<&PlayerId>,
    ) {
        let mut interest = self.interest_lock();
        if !interest.publish_state(&msg) {
            interest.publish_effect(
                onlinerpg_shared::interest::SubjectArea::point(*position, floor_level),
                &msg,
                skip_player_id,
            );
        }
    }

    pub async fn register_player_character(
        &self,
        player_id: &PlayerId,
        character_id: i64,
        xp: u64,
        attributes: CharacterAttributes,
        gold: i64,
        satiation: Option<u32>,
    ) {
        {
            let mut map = self.player_characters.write().await;
            map.insert(*player_id, (character_id, xp, attributes));
        }
        self.combat_audit.register(*player_id, character_id);
        if let Some(player) = self.players.read().await.get(player_id) {
            self.combat_audit.observe(player);
        }
        {
            let mut gold_map = self.player_gold.write().await;
            gold_map.insert(*player_id, gold);
        }
        // None = official NPC (exempt).
        if let Some(satiation) = satiation {
            self.register_hunger(player_id, satiation).await;
        }
    }

    pub async fn unregister_player_character(&self, player_id: &PlayerId) {
        self.combat_audit.logout(player_id);
        {
            let mut map = self.player_characters.write().await;
            map.remove(player_id);
        }
        {
            let mut gold_map = self.player_gold.write().await;
            gold_map.remove(player_id);
        }
        self.remove_player_blocks(player_id).await;
        self.remove_player_friends(player_id).await;
        self.remove_player_titles(player_id).await;
        self.forget_whisper_partner(player_id).await;
        self.forget_player_skills(player_id).await;
        self.remove_dungeon_discoveries(player_id).await;
        self.forget_hunger(player_id).await;
        self.mana.write().await.remove(player_id);
        self.bed_rest_started.write().await.remove(player_id);
    }

    /// Serializes account replacement and character deletion with game entry.
    pub(crate) async fn lock_character_sessions(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.character_session_lock.lock().await
    }

    #[cfg(test)]
    pub(crate) async fn register_account_session(
        &self,
        account_name: &str,
        kick_tx: mpsc::UnboundedSender<KickNotice>,
        auth: &AuthService,
    ) -> u64 {
        let _sessions = self.character_session_lock.lock().await;
        self.register_account_session_locked(account_name, kick_tx, auth)
            .await
    }

    /// Body of `register_account_session` for callers already holding
    /// `lock_character_sessions` — the mutex is not reentrant, so a login that
    /// checks a ban under the lock has to register through this.
    pub(crate) async fn register_account_session_locked(
        &self,
        account_name: &str,
        kick_tx: mpsc::UnboundedSender<KickNotice>,
        auth: &AuthService,
    ) -> u64 {
        use std::sync::atomic::Ordering;

        let session_id = self.next_account_session.fetch_add(1, Ordering::Relaxed);
        let key = account_name.to_ascii_lowercase();
        let replaced = self.account_sessions.write().await.insert(
            key,
            super::AccountSession {
                id: session_id,
                player_id: None,
                kick_tx,
            },
        );

        if let Some(replaced) = replaced {
            info!("Replacing active session for account '{}'", account_name);
            let _ = replaced.kick_tx.send(KickNotice {
                message: ServerMessage::Kicked {
                    player_id: replaced.player_id.unwrap_or(PlayerId::from(0)),
                    reason: "Another session logged in with the same account".to_string(),
                },
                close_code: None,
            });
            if let Some(player_id) = replaced.player_id {
                self.cleanup_player_session(&player_id, auth).await;
            }
        }

        session_id
    }

    pub(crate) async fn is_current_account_session(
        &self,
        account_name: &str,
        session_id: u64,
    ) -> bool {
        self.account_sessions
            .read()
            .await
            .get(&account_name.to_ascii_lowercase())
            .is_some_and(|session| session.id == session_id)
    }

    pub(crate) async fn attach_player_to_account_session(
        &self,
        account_name: &str,
        session_id: u64,
        player_id: PlayerId,
    ) -> bool {
        let mut sessions = self.account_sessions.write().await;
        let Some(session) = sessions.get_mut(&account_name.to_ascii_lowercase()) else {
            return false;
        };
        if session.id != session_id {
            return false;
        }
        session.player_id = Some(player_id);
        true
    }

    pub(crate) async fn end_account_session(
        &self,
        account_name: &str,
        session_id: u64,
        auth: &AuthService,
    ) {
        let _sessions = self.character_session_lock.lock().await;
        let key = account_name.to_ascii_lowercase();
        let ended = {
            let mut sessions = self.account_sessions.write().await;
            if sessions
                .get(&key)
                .is_some_and(|session| session.id == session_id)
            {
                sessions.remove(&key)
            } else {
                None
            }
        };
        if let Some(player_id) = ended.and_then(|session| session.player_id) {
            self.cleanup_player_session(&player_id, auth).await;
        }
    }

    /// Deletes an inactive character, or returns `false` if it is registered.
    pub(crate) async fn delete_character_if_inactive(
        &self,
        auth: &AuthService,
        account_name: &str,
        character_id: i64,
    ) -> Result<bool, AuthError> {
        let _sessions = self.character_session_lock.lock().await;
        if self
            .player_characters
            .read()
            .await
            .values()
            .any(|(active_id, _, _)| *active_id == character_id)
        {
            return Ok(false);
        }
        auth.delete_character(account_name, character_id)?;
        self.discard_pending_discovery_saves(character_id).await;
        Ok(true)
    }

    pub async fn get_player_gold(&self, player_id: &PlayerId) -> i64 {
        let gold_map = self.player_gold.read().await;
        gold_map.get(player_id).copied().unwrap_or(0)
    }

    /// Online player id for a typed name, ignoring ASCII case — names are
    /// unique ignoring case, so at most one player matches. The caller still
    /// validates the id against `players` under its own lock.
    pub(crate) async fn player_id_by_name(&self, name: &str) -> Option<PlayerId> {
        let names = self.player_ids_by_name.read().await;
        names.get(&name.to_ascii_lowercase()).copied()
    }

    /// Character name for logs and player-facing text. Falls back to the raw
    /// id when the player is gone, so log sites stay useful on the paths that
    /// fire precisely because the lookup missed.
    pub(crate) async fn player_name_of(&self, player_id: &PlayerId) -> String {
        let players = self.players.read().await;
        players
            .get(player_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| player_id.to_string())
    }

    /// Pose snapshot (position, rotation, floor, name) in one lock read.
    pub(crate) async fn player_pose(
        &self,
        player_id: &PlayerId,
    ) -> Option<(Position, f32, i8, String)> {
        let players = self.players.read().await;
        players
            .get(player_id)
            .map(|p| (p.position, p.rotation, p.floor_level, p.name.clone()))
    }

    async fn cleanup_player_session(&self, player_id: &PlayerId, auth: &AuthService) {
        self.end_account_activity(player_id, auth).await;
        self.cancel_concentration_if_active(player_id).await;
        self.persist_and_detach_player(player_id, auth).await;
        self.unregister_connection_channel(player_id).await;
        self.unregister_player_character(player_id).await;
        self.remove_player(player_id).await;
    }

    /// Force-disconnect an online player (admin `/kick`). Ends the account
    /// session the way a replacement login would: the `kick_tx` message makes
    /// the connection loop close the socket, and removing the session first
    /// keeps the disconnect path from cleaning up a second time.
    ///
    /// `close_code` turns the kick into an instruction the client acts on
    /// (reload rather than reconnect); `None` just leaves the reason on screen.
    pub(crate) async fn kick_player(
        &self,
        player_id: &PlayerId,
        reason: &str,
        close_code: Option<u16>,
        auth: &AuthService,
    ) {
        let _sessions = self.character_session_lock.lock().await;
        match self.account_of_player(player_id).await {
            Some(account) => {
                self.evict_account_session_locked(&account, reason, close_code, auth)
                    .await
            }
            // No session row to close, but per-player state still has to go.
            None => self.cleanup_player_session(player_id, auth).await,
        }
    }

    /// Account behind an online player id, read from the session map. `None`
    /// once the session is gone.
    pub(crate) async fn account_of_player(&self, player_id: &PlayerId) -> Option<String> {
        self.account_sessions
            .read()
            .await
            .iter()
            .find_map(|(key, session)| (session.player_id == Some(*player_id)).then(|| key.clone()))
    }

    /// Force-disconnect a whole account, whichever character it is playing — or
    /// none at all, as at character select. The `kick_tx` message makes the
    /// connection loop close the socket, and removing the session first keeps
    /// the disconnect path from cleaning up a second time.
    ///
    /// Assumes `lock_character_sessions` is held, so `/ban` can serialize its
    /// write with the login path.
    pub(crate) async fn evict_account_session_locked(
        &self,
        account_name: &str,
        reason: &str,
        close_code: Option<u16>,
        auth: &AuthService,
    ) {
        let session = self
            .account_sessions
            .write()
            .await
            .remove(&account_name.to_ascii_lowercase());
        let Some(session) = session else {
            return;
        };
        let _ = session.kick_tx.send(KickNotice {
            message: ServerMessage::Kicked {
                player_id: session.player_id.unwrap_or(PlayerId::from(0)),
                reason: reason.to_string(),
            },
            close_code,
        });
        // Only a session that reached the game has per-player state to clear.
        if let Some(player_id) = session.player_id {
            self.cleanup_player_session(&player_id, auth).await;
        }
    }

    /// Synchronously write a player's character row and inventory to the DB,
    /// detaching the in-memory inventory. Shared by the disconnect path and by
    /// session replacement (kick), which relies on the inventory being flushed
    /// before the replacement login loads from the DB (F-015).
    ///
    /// Must run *before* `unregister_player_character`: both the character-state
    /// and inventory snapshots resolve the character id through
    /// `player_characters`, so unregistering first would silently skip both
    /// saves while still detaching the inventory.
    pub async fn persist_and_detach_player(&self, player_id: &PlayerId, auth: &AuthService) {
        let _persistence = self.persistence_lock.lock().await;

        let mut characters = Vec::new();
        if let Some(save_data) = self.get_player_save_data(player_id).await {
            self.remove_dirty(player_id).await;
            characters.push(save_data);
        }
        let inventories = Vec::from_iter(self.take_player_inventory(player_id).await);
        let skills = Vec::from_iter(self.take_player_skills(player_id).await);
        let auth = auth.clone();
        flush_save(
            move || auth.save_batch(&characters, &inventories, &skills, &[], None),
            "player state",
        )
        .await;
    }

    /// Persist every connected player plus the world clock in one transaction.
    /// Used by the shutdown drain, where connections skip their own teardown so
    /// 5,000 logouts don't become 5,000 commits.
    pub async fn persist_shutdown_snapshot(&self, auth: &AuthService) {
        let (characters, inventories) = self.collect_shutdown_snapshot().await;
        let skills = self.collect_all_skill_states().await;
        let discoveries = self.take_pending_discovery_saves().await;
        let character_count = characters.len();
        let inventory_count = inventories.len();
        let datetime = self.current_game_datetime();
        let saved_auth = auth.clone();
        flush_save(
            move || {
                saved_auth.save_batch(
                    &characters,
                    &inventories,
                    &skills,
                    &discoveries,
                    Some(&datetime),
                )?;
                info!(
                    "Saved shutdown snapshot: {} character(s), {} inventory/inventories",
                    character_count, inventory_count
                );
                Ok(())
            },
            "shutdown snapshot",
        )
        .await;
        self.flush_gold_sources(auth, crate::auth::unix_now(), true)
            .await;
        self.flush_gold_sinks(auth, crate::auth::unix_now(), true)
            .await;
        self.flush_weapon_enchant_failures(auth).await;
    }

    async fn collect_shutdown_snapshot(
        &self,
    ) -> (Vec<CharacterSaveData>, Vec<(i64, Vec<ItemRow>)>) {
        let _persistence = self.persistence_lock.lock().await;
        let players = self.players.read().await;
        let player_characters = self.player_characters.read().await;
        let player_gold = self.player_gold.read().await;
        let hunger = self.hunger.read().await;
        let inventories = self.inventories.read().await;

        let mana = self.mana.read().await;
        let dungeon_epoch = self.dungeon_save_epoch().await;
        let mut characters = Vec::with_capacity(player_characters.len());
        let mut inventory_rows = Vec::with_capacity(player_characters.len());

        for (player_id, (character_id, xp, _)) in player_characters.iter() {
            if let Some(player) = players.get(player_id) {
                characters.push(build_save_data(
                    player,
                    *character_id,
                    *xp,
                    player_gold.get(player_id).copied().unwrap_or(0),
                    super::hunger::satiation_for_save(&hunger, player_id),
                    inventories
                        .get(player_id)
                        .and_then(|inv| inv.active_ammo.clone()),
                    mana.get(player_id).map(|data| data.mana),
                    dungeon_epoch,
                ));
            }
            if let Some(inventory) = inventories.get(player_id) {
                inventory_rows.push((
                    *character_id,
                    super::inventory::serialize_inventory(inventory),
                ));
            }
        }

        characters.sort_by_key(|state| state.character_id);
        inventory_rows.sort_by_key(|(character_id, _)| *character_id);
        (characters, inventory_rows)
    }

    /// Write every dirty character state and inventory. Takes the same lock as
    /// `persist_and_detach_player` so a periodic flush cannot interleave with a
    /// logout's save.
    pub async fn flush_dirty_saves(&self, auth: &AuthService) {
        let _persistence = self.persistence_lock.lock().await;
        self.flush_weapon_enchant_failures(auth).await;

        let (dirty_player_ids, dirty_states) = self.collect_dirty_character_states().await;
        let (dirty_inventory_ids, dirty_inventories) = self.collect_dirty_inventory_states().await;
        let (dirty_skill_ids, dirty_skills) = self.collect_dirty_skill_states().await;
        let dirty_discoveries = self.take_pending_discovery_saves().await;
        if dirty_states.is_empty()
            && dirty_inventories.is_empty()
            && dirty_skills.is_empty()
            && dirty_discoveries.is_empty()
        {
            return;
        }

        let character_count = dirty_states.len();
        let inventory_count = dirty_inventories.len();
        let auth = auth.clone();
        let discoveries = dirty_discoveries.clone();
        let saved = flush_save(
            move || {
                auth.save_batch(
                    &dirty_states,
                    &dirty_inventories,
                    &dirty_skills,
                    &discoveries,
                    None,
                )?;
                info!(
                    "Batch-saved {} character state(s), {} inventory/inventories",
                    character_count, inventory_count
                );
                Ok(())
            },
            "dirty state",
        )
        .await;
        if !saved {
            self.restore_dirty_players(dirty_player_ids).await;
            self.restore_dirty_inventories(dirty_inventory_ids).await;
            self.restore_dirty_skills(dirty_skill_ids).await;
            self.restore_pending_discovery_saves(dirty_discoveries)
                .await;
        }
    }

    /// Register the player and queue their current world snapshot.
    pub async fn add_player(&self, mut player: Player) {
        // Normalize persisted legacy positions before they enter the spatial
        // index or are sent to clients.
        player.position.x = onlinerpg_shared::wrap_world_x(player.position.x);
        let player_id = player.id;
        let player_name = player.name.clone();
        let player_number = self.get_or_assign_player_number(&player_id).await;
        let player_position = player.position;

        {
            let mut players = self.players.write().await;
            self.combat_audit.observe(&player);
            players.insert(player_id, player.clone());
        }
        {
            let mut names = self.player_ids_by_name.write().await;
            names.insert(player_name.to_ascii_lowercase(), player_id);
        }
        self.insert_player_spatial_cell(&player_id, &player_position)
            .await;

        info!(
            "Player {} ({}) joined the game [#{}]",
            player_name, player_id, player_number
        );

        self.interest_lock()
            .publish_state(&ServerMessage::PlayerJoined { player });
        self.reset_world_view(&player_id).await;
    }

    pub async fn remove_player(&self, player_id: &PlayerId) {
        self.bed_rest_started.write().await.remove(player_id);
        self.clear_player_movement(player_id, "disconnect").await;
        self.goal_moves.lock().await.remove(player_id);
        self.player_movement_versions
            .write()
            .await
            .remove(player_id);
        self.music_performances.write().await.remove(player_id);
        self.remove_live_instrument(player_id).await;
        self.remove_player_stall(player_id).await;
        self.close_stall(player_id).await;
        self.remove_player_tip_hat(player_id).await;
        self.drop_player_trade(player_id, "They left.").await;
        self.last_player_attacks.write().await.remove(player_id);
        let dungeon_exit = {
            let players = self.players.read().await;
            players
                .get(player_id)
                .filter(|p| p.floor_level < 0)
                .map(|p| (p.floor_level, p.position))
        };
        if let Some((floor, position)) = dungeon_exit {
            self.handle_player_floor_change(player_id, floor, 0, &position, &position)
                .await;
        }

        // Release any trade-window holds: this player may have been shopping
        // with NPCs (free them if it was their last customer) or be a trading
        // NPC itself (forget its entry).
        self.clear_shops_for_player(player_id).await;

        let removed_player_number = {
            let mut id_state = self.id_state.write().await;
            id_state.player_numbers.remove(player_id)
        };

        let removed_player = {
            let mut players = self.players.write().await;
            self.combat_audit.logout(player_id);
            players.remove(player_id)
        };
        if let Some(player) = &removed_player {
            let key = player.name.to_ascii_lowercase();
            let mut names = self.player_ids_by_name.write().await;
            // Guarded so a same-name replacement session keeps its entry.
            if names.get(&key) == Some(player_id) {
                names.remove(&key);
            }
        }

        // After the roster removal on purpose: party mutations hold the
        // players lock, so an in-flight accept lands before this sweep and
        // gets cleaned up instead of leaving a ghost member.
        self.clear_party_for_player(player_id).await;
        self.clear_buffs(player_id).await;

        if let Some(player) = removed_player {
            self.remove_player_spatial_cell(player_id, &player.position)
                .await;
            self.despawn_monsters_near(&player.position).await;
            info!(
                "Player {} ({}) left the game{}",
                player.name,
                player_id,
                removed_player_number
                    .map(|n| format!(" [#{}]", n))
                    .unwrap_or_default()
            );
            self.publish_subject_change(ServerMessage::PlayerLeft {
                player_id: *player_id,
            });
        } else {
            warn!("Attempted to remove non-existent player: {}", player_id);
        }
    }

    pub async fn mark_world_ready(&self, player_id: &PlayerId) {
        let mut players = self.players.write().await;
        if let Some(player) = players.get_mut(player_id) {
            player.ready_at = 0;
        }
    }

    pub(super) async fn clear_player_movement(&self, id: &PlayerId, _reason: &'static str) {
        self.cancel_goal_movement(id).await;
        let mut versions = self.player_movement_versions.write().await;
        let version = versions.entry(*id).or_default();
        *version = version.wrapping_add(1);
    }

    pub async fn tick_player_movement(&self, _dt: f32) {
        self.validate_mounts().await;
        self.tick_goal_movement().await;
    }

    /// Store a position immediately (trusted server-side path) and run the
    /// shared bookkeeping/fanout.
    async fn apply_player_position(
        &self,
        player_id: &PlayerId,
        new_position: Position,
        new_rotation: f32,
        floor_level: i8,
        update_msg: ServerMessage,
    ) {
        self.clear_player_movement(player_id, "teleport").await;
        self.clear_pose_on_move(player_id, "teleport").await;
        let (old_position, old_floor, moved_player) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                warn!("Attempted to move non-existent player: {}", player_id);
                return;
            };
            let old_position = player.position;
            let old_floor = player.floor_level;
            player.position = new_position;
            player.rotation = new_rotation;
            player.floor_level = floor_level;
            (old_position, old_floor, player.clone())
        };
        self.finish_position_update(player_id, old_position, old_floor, moved_player, update_msg)
            .await;
    }

    /// Update spatial state, dungeon occupancy, and nearby clients after relocation.
    pub(super) async fn finish_position_update(
        &self,
        player_id: &PlayerId,
        old_position: Position,
        old_floor: i8,
        moved_player: Player,
        update_msg: ServerMessage,
    ) {
        if old_position != moved_player.position || old_floor != moved_player.floor_level {
            self.cancel_concentration_if_active(player_id).await;
        }
        let new_position = moved_player.position;
        let floor_level = moved_player.floor_level;
        self.move_player_spatial_cell(player_id, &old_position, &new_position)
            .await;
        self.mark_dirty(player_id).await;
        // Rotation-only moves are exempt: markers draw x/z and floor.
        if old_position.x != new_position.x
            || old_position.z != new_position.z
            || old_floor != floor_level
        {
            self.mark_party_position_dirty(player_id).await;
        }
        if old_position != new_position {
            self.check_dungeon_discovery(player_id, &new_position).await;
        }
        let changed_dungeon = old_floor < 0
            && floor_level < 0
            && self
                .dungeon_defs
                .entrance_at(old_position.x, old_position.z)
                .zip(
                    self.dungeon_defs
                        .entrance_at(new_position.x, new_position.z),
                )
                .is_some_and(|(old, new)| old.id != new.id);
        if old_floor != floor_level || changed_dungeon {
            self.handle_player_floor_change(
                player_id,
                old_floor,
                floor_level,
                &old_position,
                &new_position,
            )
            .await;
        }
        self.fanout_player_position_update(
            player_id,
            &old_position,
            old_floor,
            &moved_player,
            update_msg,
        )
        .await;
    }

    /// A walkable spot on a small ring beside `center` for a teleport
    /// arrival, golden-angle-seeded by the mover's id so simultaneous
    /// arrivals don't stack.
    pub(crate) fn arrival_beside(&self, mover: &PlayerId, center: &Position) -> Position {
        let angle = (mover.get() % 360) as f32 * GOLDEN_ANGLE_RAD;
        self.open_spot_beside(center, angle, ARRIVAL_RING_RADIUS)
    }

    /// A walkable spot at `angle` from `center`, X wrapped to the canonical
    /// range (callers store it directly). A blocked spot (dungeon walls run
    /// 1m from a corridor's center) retries at half radius and finally lands
    /// on `center` itself — a walkable cell by construction.
    pub(crate) fn open_spot_beside(&self, center: &Position, angle: f32, radius: f32) -> Position {
        let cache = self.passability_read();
        let cell_floor = super::passability::authoritative_floor(&cache, center);
        for radius in [radius, radius * 0.5] {
            let candidate = Position {
                x: wrap_world_x(center.x + angle.cos() * radius),
                y: center.y,
                z: center.z + angle.sin() * radius,
            };
            if super::passability::wrapped_block_info(
                &cache,
                center.x,
                center.z,
                candidate.x,
                candidate.z,
                cell_floor,
                center.y,
            )
            .is_none()
            {
                return candidate;
            }
        }
        *center
    }

    pub async fn teleport_player(
        &self,
        player_id: &PlayerId,
        mut new_position: Position,
        new_rotation: f32,
        new_floor_level: i8,
    ) {
        // A NaN position would poison the SQLite save batch for everyone.
        if !(new_position.is_finite() && new_rotation.is_finite()) {
            warn!("Rejected non-finite teleport for player {player_id}");
            return;
        }
        new_position.x = wrap_world_x(new_position.x);
        self.stop_bed_rest(player_id).await;
        self.apply_player_position(
            player_id,
            new_position,
            new_rotation,
            new_floor_level,
            ServerMessage::PlayerTeleported {
                player_id: *player_id,
                position: new_position,
                rotation: new_rotation,
                floor_level: new_floor_level,
            },
        )
        .await;
        self.void_summons_aimed_at(player_id).await;
    }

    /// Put a player at the world spawn on the surface.
    pub async fn teleport_to_town(&self, player_id: &PlayerId) {
        let spawn = &world_config().spawn_position;
        self.teleport_player(player_id, spawn.position(), spawn.rotation, 0)
            .await;
    }

    /// Wake the dead in the inn's sick room: lying in the first free bed, or
    /// standing beside them when every bed is taken.
    pub async fn respawn_player(&self, player_id: &PlayerId) {
        self.clear_player_movement(player_id, "respawn").await;
        let respawn = &world_config().respawn;
        let (old_floor, old_position, player) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get(player_id) else {
                warn!("Attempted to respawn non-existent player: {}", player_id);
                return;
            };
            if player.health > 0 {
                info!(
                    "Ignored respawn request for alive player {} ({}) HP: {}/{}",
                    player.name, player.id, player.health, player.max_health
                );
                return;
            }
            let beds = self.respawn_beds();
            let taken: Vec<u32> = players
                .values()
                .filter(|p| p.id != *player_id)
                .filter(|p| !p.object_type.as_deref().is_some_and(is_estate_storage_item))
                .filter_map(|p| p.object_id)
                .collect();
            let free_bed = beds.iter().find(|bed| !taken.contains(&bed.id));
            let player = players.get_mut(player_id).expect("looked up above");
            let old_health = player.health;
            self.delay_mana_regeneration(player_id).await;
            player.health = player.max_health;
            self.combat_audit.health(old_health, player, "respawn");
            let old_floor = player.floor_level;
            let old_position = player.position;
            player.floor_level = respawn.floor_level;
            match free_bed {
                Some(bed) => {
                    player.position = Position {
                        x: bed.x,
                        y: bed.y,
                        z: bed.z,
                    };
                    player.rotation = bed.rotation_deg.to_radians();
                    player.object_type = Some(bed.type_id.clone());
                    player.object_id = Some(bed.id);
                }
                None => {
                    player.position = respawn.position();
                    player.rotation = respawn.rotation_deg.to_radians();
                    player.object_type = None;
                    player.object_id = None;
                }
            }
            self.update_bed_rest(player).await;
            (old_floor, old_position, player.clone())
        };

        info!("Player {} ({}) respawned", player.name, player.id);
        let update_msg = ServerMessage::PlayerRespawned {
            player: player.clone(),
        };
        self.finish_position_update(
            player_id,
            old_position,
            old_floor,
            player.clone(),
            update_msg,
        )
        .await;
        self.reset_hunger_on_respawn(player_id).await;
        self.mark_party_vitals_dirty(player_id).await;
    }

    /// Revive a defeated player where they fell with `hp_percent` of their
    /// max HP. Position, floor and the already-applied death penalty stay, so
    /// this only announces the new HP — no AOI move. Returns false when the
    /// player is alive or unknown.
    pub async fn revive_in_place(&self, player_id: &PlayerId, hp_percent: u32) -> bool {
        let revived = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id).filter(|p| p.health == 0) else {
                return false;
            };
            self.delay_mana_regeneration(player_id).await;
            player.health = (player.max_health * hp_percent / 100).max(1);
            self.combat_audit.health(0, player, "revive");
            player.clone()
        };
        info!("Player {} ({}) revived in place", revived.name, revived.id);
        self.mark_dirty(player_id).await;
        self.mark_party_vitals_dirty(player_id).await;
        let (position, floor_level) = (revived.position, revived.floor_level);
        self.publish_nearby(
            &position,
            floor_level,
            ServerMessage::PlayerRespawned { player: revived },
            None,
        )
        .await;
        true
    }

    pub async fn get_player_position(&self, player_id: &PlayerId) -> Option<(Position, f32, i8)> {
        let players = self.players.read().await;
        players
            .get(player_id)
            .map(|p| (p.position, p.rotation, p.floor_level))
    }

    pub async fn set_player_torch(&self, player_id: &PlayerId, enabled: bool) {
        let position = {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                if player.torch_on == enabled {
                    return;
                }
                player.torch_on = enabled;
                Some((player.position, player.floor_level))
            } else {
                None
            }
        };

        if let Some((position, floor_level)) = position {
            self.publish_nearby(
                &position,
                floor_level,
                ServerMessage::PlayerTorchToggled {
                    player_id: *player_id,
                    enabled,
                },
                Some(player_id),
            )
            .await;
        }
    }

    /// Update the gear nearby clients render and tell them what changed. Both
    /// slots are compared under one write lock: every bag mutation routes
    /// through here and almost none of them touch gear, so taking the global
    /// players lock once per snapshot rather than once per slot matters.
    /// Reads what shows from the inventory itself rather than taking a widening
    /// list of `Option<String>`s. The cape's dye and texture are part of it:
    /// neither changes the def id, so both have to count as a change or the
    /// new look never reaches anyone else.
    pub async fn set_player_gear(&self, player_id: &PlayerId, inventory: &PlayerInventory) {
        let main_hand = inventory.equipped_def_id(EquipSlot::MainHand);
        let back = inventory.equipped_def_id(EquipSlot::Back);
        let back_color = inventory.equipped_cape_color();
        let back_texture = inventory.equipped_cape_texture();
        let changed = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            let mut messages = Vec::new();
            if player.main_hand != main_hand {
                player.main_hand = main_hand.clone();
                messages.push(ServerMessage::PlayerMainHandChanged {
                    player_id: *player_id,
                    item_def_id: main_hand,
                });
            }
            if player.back != back
                || player.back_color != back_color
                || player.back_texture != back_texture
            {
                player.back = back.clone();
                player.back_color = back_color.clone();
                player.back_texture = back_texture.clone();
                messages.push(ServerMessage::PlayerBackChanged {
                    player_id: *player_id,
                    item_def_id: back,
                    cape_color: back_color,
                    cape_texture: back_texture,
                });
            }
            if messages.is_empty() {
                return;
            }
            (player.position, player.floor_level, messages)
        };

        let (position, floor_level, messages) = changed;
        for message in messages {
            self.publish_nearby(&position, floor_level, message, Some(player_id))
                .await;
        }
    }

    /// Clients send StopInteraction before moving; a third-party one that
    /// skips it would leave late joiners seeing the mover frozen in the pose.
    /// Not in the position funnel: respawn writes pose and position together,
    /// and the tick walks the residual leg after InteractObject arrives.
    pub(super) async fn clear_pose_on_move(&self, player_id: &PlayerId, source: &str) {
        let posed = {
            let players = self.players.read().await;
            players
                .get(player_id)
                .and_then(|p| Some((p.object_type.clone()?, p.name.clone(), p.client_kind)))
        };
        let Some((object_type, name, client_kind)) = posed else {
            return;
        };
        info!("Player {name} ({client_kind:?}) left pose {object_type} on {source}");
        self.set_player_interaction(player_id, None, None).await;
    }

    pub async fn set_player_interaction(
        &self,
        player_id: &PlayerId,
        object_type: Option<String>,
        object_id: Option<u32>,
    ) {
        let rejected_or_position = {
            let mut players = self.players.write().await;
            let estate_definition = object_type.as_deref().and_then(estate_storage_def);
            let invalid_estate = if let Some(definition) = estate_definition {
                let chests = self.estate_chests.read().await;
                match (
                    players.get(player_id),
                    object_id.and_then(|id| chests.get(i64::from(id))),
                ) {
                    (Some(player), Some(chest))
                        if chest.item_def_id == definition.id
                            && matches!(
                                definition.model_id.as_str(),
                                "bed" | "rustic_bed" | "chair"
                            ) =>
                    {
                        if player.health == 0 || player.is_mounted() {
                            Some("You cannot use furniture right now.")
                        } else if chest.floor_level != player.floor_level
                            || chest.position.dist_xz_sq(&player.position)
                                > (INTERACTION_RANGE + 0.5).powi(2)
                        {
                            Some("Move closer to the furniture.")
                        } else {
                            None
                        }
                    }
                    _ => Some("That furniture is no longer available."),
                }
            } else {
                None
            };

            if let Some(reason) = invalid_estate {
                Err(reason)
            } else if object_id.is_some_and(|fid| {
                players.values().any(|p| {
                    p.id != *player_id
                        && p.object_id == Some(fid)
                        && p.object_type.as_deref().is_some_and(is_estate_storage_item)
                            == estate_definition.is_some()
                })
            }) {
                Err("occupied")
            } else if let Some(player) = players.get_mut(player_id) {
                player.object_type = object_type.clone();
                player.object_id = object_id;
                self.update_bed_rest(player).await;
                Ok(Some((player.position, player.floor_level)))
            } else {
                Ok(None)
            }
        };

        if let Err(reason) = rejected_or_position {
            self.send_direct_message(
                player_id,
                ServerMessage::InteractionRejected {
                    reason: reason.to_string(),
                },
            )
            .await;
        } else if let Ok(Some((position, floor_level))) = rejected_or_position {
            if object_type.as_deref() != Some(onlinerpg_shared::messages::MUSIC_EMOTE) {
                self.music_performances.write().await.remove(player_id);
                self.remove_live_instrument(player_id).await;
            }
            self.publish_nearby(
                &position,
                floor_level,
                ServerMessage::PlayerInteractionChanged {
                    player_id: *player_id,
                    object_type,
                    object_id,
                },
                None,
            )
            .await;
        }
    }

    pub async fn mark_dirty(&self, player_id: &PlayerId) {
        let mut dirty = self.dirty_players.write().await;
        dirty.insert(*player_id);
    }

    async fn restore_dirty_players(&self, ids: Vec<PlayerId>) {
        if !ids.is_empty() {
            self.dirty_players.write().await.extend(ids);
        }
    }

    pub async fn remove_dirty(&self, player_id: &PlayerId) {
        let mut dirty = self.dirty_players.write().await;
        dirty.remove(player_id);
    }

    pub async fn collect_dirty_character_states(&self) -> (Vec<PlayerId>, Vec<CharacterSaveData>) {
        let dirty_ids: Vec<PlayerId> = {
            let mut dirty = self.dirty_players.write().await;
            dirty.drain().collect()
        };

        if dirty_ids.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let players = self.players.read().await;
        let player_chars = self.player_characters.read().await;
        let gold_map = self.player_gold.read().await;
        let hunger = self.hunger.read().await;

        let inventories = self.inventories.read().await;
        let mana = self.mana.read().await;
        let mut result = Vec::with_capacity(dirty_ids.len());
        let dungeon_epoch = self.dungeon_save_epoch().await;
        for pid in &dirty_ids {
            if let (Some(player), Some((char_id, xp, _))) =
                (players.get(pid), player_chars.get(pid))
            {
                let gold = gold_map.get(pid).copied().unwrap_or(0);
                let satiation = super::hunger::satiation_for_save(&hunger, pid);
                let ammo = inventories.get(pid).and_then(|inv| inv.active_ammo.clone());
                result.push(build_save_data(
                    player,
                    *char_id,
                    *xp,
                    gold,
                    satiation,
                    ammo,
                    mana.get(pid).map(|data| data.mana),
                    dungeon_epoch,
                ));
            }
        }

        (dirty_ids, result)
    }

    pub async fn get_player_save_data(&self, player_id: &PlayerId) -> Option<CharacterSaveData> {
        let players = self.players.read().await;
        let player_chars = self.player_characters.read().await;
        let gold_map = self.player_gold.read().await;
        let hunger = self.hunger.read().await;

        let player = players.get(player_id)?;
        let (char_id, xp, _) = player_chars.get(player_id)?;
        let gold = gold_map.get(player_id).copied().unwrap_or(0);
        let satiation = super::hunger::satiation_for_save(&hunger, player_id);
        let ammo = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inv| inv.active_ammo.clone());

        let mana = self.mana.read().await.get(player_id).map(|data| data.mana);
        let dungeon_epoch = self.dungeon_save_epoch().await;
        Some(build_save_data(
            player,
            *char_id,
            *xp,
            gold,
            satiation,
            ammo,
            mana,
            dungeon_epoch,
        ))
    }

    async fn insert_player_spatial_cell(&self, player_id: &PlayerId, position: &Position) {
        self.player_spatial_cells
            .write()
            .await
            .insert(*player_id, position);
    }

    async fn remove_player_spatial_cell(&self, player_id: &PlayerId, position: &Position) {
        self.player_spatial_cells
            .write()
            .await
            .remove(player_id, position);
    }

    async fn move_player_spatial_cell(
        &self,
        player_id: &PlayerId,
        old_position: &Position,
        new_position: &Position,
    ) {
        // Most moves stay inside one cell. Checking before taking the lock keeps
        // them off the write guard every mover on the server shares.
        if super::SpatialCell::from_position(old_position)
            == super::SpatialCell::from_position(new_position)
        {
            return;
        }
        self.player_spatial_cells
            .write()
            .await
            .moved(player_id, old_position, new_position);
    }

    async fn fanout_player_position_update(
        &self,
        player_id: &PlayerId,
        old_position: &Position,
        old_floor: i8,
        player: &Player,
        update_msg: ServerMessage,
    ) {
        self.interest_lock()
            .publish_player_movement(player, *old_position, old_floor, update_msg);
        self.reconcile_view(player_id).await;
        self.despawn_monsters_near(old_position).await;
        let stall_strayed = self
            .stalls
            .read()
            .await
            .get(player_id)
            .is_some_and(|entry| {
                entry
                    .stall
                    .strayed_from(&player.position, player.floor_level)
            });
        if stall_strayed {
            self.pack_up_strayed_stall(player_id).await;
        }
        let hat_strayed = self
            .tip_hats
            .read()
            .await
            .get(player_id)
            .is_some_and(|hat| hat.strayed_from(&player.position, player.floor_level));
        if hat_strayed {
            self.pack_up_strayed_tip_hat(player_id).await;
        }
    }

    pub async fn player_ids_within_position(
        &self,
        position: &Position,
        floor_level: i8,
        radius: f32,
    ) -> Vec<PlayerId> {
        self.players_within_position(position, floor_level, radius, None)
            .await
            .into_iter()
            .map(|(player_id, _)| player_id)
            .collect()
    }

    /// As `player_ids_within_position`, but keeping the squared distance and
    /// skipping one player — what an ownership handoff needs to pick the
    /// nearest candidate that is not the one leaving.
    pub(super) async fn players_within_position(
        &self,
        position: &Position,
        floor_level: i8,
        radius: f32,
        skip: Option<&PlayerId>,
    ) -> Vec<(PlayerId, f32)> {
        let radius_sq = radius * radius;
        let players = self.players.read().await;
        let cells = self.player_spatial_cells.read().await;
        let mut found: HashMap<PlayerId, f32> = HashMap::new();

        for player_id in cells.keys_near(position, radius) {
            if skip == Some(player_id) {
                continue;
            }
            let Some(player) = players.get(player_id) else {
                continue;
            };

            let dist_sq = position.dist_xz_sq(&player.position);
            if player.floor_level == floor_level && dist_sq <= radius_sq {
                found.insert(*player_id, dist_sq);
            }
        }

        found.into_iter().collect()
    }

    #[cfg(test)]
    pub async fn player_ids_within(&self, player_id: &PlayerId, radius: f32) -> Vec<PlayerId> {
        let (position, floor_level) = {
            let players = self.players.read().await;
            let Some(player) = players.get(player_id) else {
                return Vec::new();
            };
            (player.position, player.floor_level)
        };

        self.player_ids_within_position(&position, floor_level, radius)
            .await
    }

    #[allow(dead_code)]
    pub async fn get_player_count(&self) -> usize {
        self.players.read().await.len()
    }

    #[allow(dead_code)]
    pub async fn get_all_players(&self) -> HashMap<PlayerId, Player> {
        self.players.read().await.clone()
    }
}
