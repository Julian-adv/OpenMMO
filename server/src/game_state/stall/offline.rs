use super::*;
use crate::auth::{CharacterSaveData, ItemRow};
use onlinerpg_shared::inventory::PlayerInventory;
use onlinerpg_shared::Player;

pub(super) struct OfflineStallOwner {
    pub character: CharacterSaveData,
    pub inventory: PlayerInventory,
    pub carry_capacity: f32,
    pub blocked_names: HashSet<String>,
    pub dirty: bool,
}

impl GameState {
    pub(crate) async fn lock_player_persistence(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.persistence_lock.lock().await
    }

    pub(in crate::game_state) async fn detach_stall_owner(
        &self,
        player_id: &PlayerId,
        character: &CharacterSaveData,
    ) -> Option<(i64, Vec<ItemRow>)> {
        let eligible = self
            .players
            .read()
            .await
            .get(player_id)
            .is_some_and(|player| !player.is_official_npc);
        if !eligible || self.stall_of(player_id).await.is_none() {
            return None;
        }
        let carry_capacity = self.max_carry_weight(player_id).await;
        let blocked_names = self
            .blocked_names
            .read()
            .await
            .get(player_id)
            .cloned()
            .unwrap_or_default();
        let mut stalls = self.stalls.write().await;
        let entry = stalls.get_mut(player_id)?;
        let inventory = self.inventories.write().await.remove(player_id)?;
        let rows = serialize_inventory(&inventory);
        let owner = OfflineStallOwner {
            character: character.clone(),
            inventory,
            dirty: true,
            carry_capacity,
            blocked_names,
        };
        entry.offline_owner = Some(owner);
        drop(stalls);
        self.dirty_inventories.write().await.remove(player_id);
        Some((character.character_id, rows))
    }

    pub(crate) async fn reconnect_stall_owner(&self, player: &Player, character_id: i64) -> bool {
        let restored = {
            let mut stalls = self.stalls.write().await;
            let previous = stalls.iter().find_map(|(id, entry)| {
                entry
                    .offline_owner
                    .as_ref()
                    .filter(|owner| owner.character.character_id == character_id)
                    .map(|_| *id)
            });
            let Some(previous) = previous else {
                return false;
            };
            let mut entry = stalls.remove(&previous).expect("offline stall exists");
            let owner = entry.offline_owner.take().expect("offline owner exists");
            entry.stall.owner = player.id;
            entry.stall.owner_name = player.name.clone();
            let stall = entry.stall.clone();
            stalls.insert(player.id, entry);
            (stall, owner)
        };
        let (stall, owner) = restored;
        self.player_gold
            .write()
            .await
            .insert(player.id, owner.character.gold);
        self.refresh_hunger_gear_drain(&player.id, &owner.inventory)
            .await;
        self.inventories
            .write()
            .await
            .insert(player.id, owner.inventory);
        if owner.dirty {
            self.mark_inventory_dirty(&player.id).await;
        }
        if stall.strayed_from(&player.position, player.floor_level) {
            self.remove_player_stall(&player.id).await;
        } else {
            self.broadcast_stall(&stall, |stall| ServerMessage::StallPlaced { stall })
                .await;
            self.push_stall_state(&player.id, None).await;
        }
        true
    }

    pub(in crate::game_state) async fn remove_character_stall(&self, character_id: i64) {
        let owner = self.stalls.read().await.iter().find_map(|(id, entry)| {
            entry
                .offline_owner
                .as_ref()
                .filter(|owner| owner.character.character_id == character_id)
                .map(|_| *id)
        });
        if let Some(owner) = owner {
            self.remove_player_stall(&owner).await;
        }
    }

    pub(in crate::game_state) async fn flush_offline_stall_owners(&self, auth: &AuthService) {
        let (characters, inventories): (Vec<_>, Vec<_>) = self
            .stalls
            .read()
            .await
            .values()
            .filter_map(|entry| entry.offline_owner.as_ref())
            .filter(|owner| owner.dirty)
            .map(|owner| {
                (
                    owner.character.clone(),
                    (
                        owner.character.character_id,
                        serialize_inventory(&owner.inventory),
                    ),
                )
            })
            .unzip();
        if characters.is_empty() {
            return;
        }
        let auth = auth.clone();
        if let Err(error) =
            auth_db(move || auth.save_batch(&characters, &inventories, &[], &[], None)).await
        {
            error!("Offline stall owner save failed: {error}");
            return;
        }
        for entry in self.stalls.write().await.values_mut() {
            if let Some(owner) = entry.offline_owner.as_mut() {
                owner.dirty = false;
            }
        }
    }

    pub(in crate::game_state) async fn mark_stall_owner_saved(&self, player_id: &PlayerId) {
        if let Some(owner) = self
            .stalls
            .write()
            .await
            .get_mut(player_id)
            .and_then(|entry| entry.offline_owner.as_mut())
        {
            owner.dirty = false;
        }
    }

    pub(in crate::game_state) async fn stall_owner_is_offline(&self, owner: &PlayerId) -> bool {
        self.stalls
            .read()
            .await
            .get(owner)
            .is_some_and(|entry| entry.offline_owner.is_some())
    }

    pub(super) async fn stall_owner_has_blocked(&self, owner: &PlayerId, name: &str) -> bool {
        if self.has_blocked(owner, name).await {
            return true;
        }
        self.stalls
            .read()
            .await
            .get(owner)
            .and_then(|entry| entry.offline_owner.as_ref())
            .is_some_and(|owner| owner.blocked_names.contains(name))
    }

    pub(super) async fn stall_trader_save_data(
        &self,
        player_id: &PlayerId,
    ) -> Option<CharacterSaveData> {
        if let Some(owner) = self
            .stalls
            .read()
            .await
            .get(player_id)
            .and_then(|entry| entry.offline_owner.as_ref())
        {
            return Some(owner.character.clone());
        }
        self.get_player_save_data(player_id).await
    }

    pub(super) async fn stall_trader_name(&self, player_id: &PlayerId) -> Option<String> {
        if let Some(player) = self.players.read().await.get(player_id) {
            return Some(player.name.clone());
        }
        self.stall_of(player_id).await.map(|stall| stall.owner_name)
    }
}
