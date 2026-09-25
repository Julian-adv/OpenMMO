use super::*;
use onlinerpg_shared::estate_storage::estate_storage_def;
use onlinerpg_shared::interest::{InterestChange, WorldEvent};

fn cache_key(id: i64) -> String {
    format!("furniture:estate-storage:{id}")
}

impl WorldCache {
    fn store_estate_chest(&mut self, chest: &EstateChest) {
        let unchanged = self.estate_chests.get(&chest.id).is_some_and(|old| {
            old.item_def_id == chest.item_def_id
                && old.position == chest.position
                && old.rotation_deg == chest.rotation_deg
                && old.floor_level == chest.floor_level
        });
        if unchanged {
            return;
        }
        self.estate_chests.insert(chest.id, chest.clone());
        let runtime = estate_storage_def(&chest.item_def_id).and_then(|definition| {
            furniture::build_furniture_passability_for_placements(&[FurniturePlacement {
                id: chest.id as u32,
                type_id: definition.model_id.clone(),
                x: chest.position.x,
                y: chest.position.y,
                z: chest.position.z,
                rotation_deg: chest.rotation_deg,
                floor_level: u8::try_from(chest.floor_level).ok()?,
            }])
        });
        let key = cache_key(chest.id);
        let changed = match runtime {
            Some(runtime) => {
                self.passability_cache.insert(key, runtime);
                true
            }
            None => self.passability_cache.remove(&key).is_some(),
        };
        if changed {
            self.collision_revision = self.collision_revision.wrapping_add(1);
        }
    }

    fn remove_estate_chest(&mut self, id: i64) {
        self.estate_chests.remove(&id);
        if self.passability_cache.remove(&cache_key(id)).is_some() {
            self.collision_revision = self.collision_revision.wrapping_add(1);
        }
    }

    pub(super) fn clear_estate_chests(&mut self) {
        let ids: Vec<_> = self.estate_chests.keys().copied().collect();
        for id in ids {
            self.remove_estate_chest(id);
        }
        self.estate_chest_views.clear();
        self.estate_chest_revisions.clear();
        self.estate_chest_deleted.clear();
    }

    fn leave_estate_chest(&mut self, viewer: PlayerId, id: i64) {
        if let Some(view) = self.estate_chest_views.get_mut(&viewer) {
            view.remove(&id);
        }
        if !self
            .estate_chest_views
            .values()
            .any(|view| view.contains(&id))
        {
            self.remove_estate_chest(id);
        }
    }

    pub fn remove_estate_chest_view(&mut self, viewer: PlayerId) {
        let ids = self.estate_chest_views.remove(&viewer).unwrap_or_default();
        for id in ids {
            self.leave_estate_chest(viewer, id);
        }
    }

    pub fn update_estate_chests(
        &mut self,
        viewer: PlayerId,
        added: &[EstateChest],
        removed: &[i64],
    ) {
        for id in removed {
            self.leave_estate_chest(viewer, *id);
        }
        for chest in added {
            self.estate_chest_views
                .entry(viewer)
                .or_default()
                .insert(chest.id);
            self.store_estate_chest(chest);
        }
    }

    pub fn apply_estate_chest_event(&mut self, viewer: PlayerId, event: &WorldEvent) {
        let Some(id) = event
            .subject
            .strip_prefix("chest:")
            .and_then(|id| id.parse().ok())
        else {
            return;
        };
        let known = self.estate_chest_revisions.get(&id).copied();
        match event.change {
            InterestChange::Leave => self.leave_estate_chest(viewer, id),
            InterestChange::Delete => {
                if known.is_some_and(|revision| revision > event.revision) {
                    return;
                }
                self.estate_chest_revisions.insert(id, event.revision);
                self.estate_chest_deleted.insert(id);
                for view in self.estate_chest_views.values_mut() {
                    view.remove(&id);
                }
                self.remove_estate_chest(id);
            }
            InterestChange::Enter | InterestChange::Update => {
                if self.estate_chest_deleted.contains(&id)
                    && known.is_some_and(|revision| revision >= event.revision)
                {
                    return;
                }
                self.estate_chest_views
                    .entry(viewer)
                    .or_default()
                    .insert(id);
                if known.is_some_and(|revision| revision > event.revision)
                    || (known == Some(event.revision) && self.estate_chests.contains_key(&id))
                {
                    return;
                }
                for message in &event.messages {
                    if let ServerMessage::EstateChestVisibility { added, .. } = message {
                        if let Some(chest) = added.iter().find(|chest| chest.id == id) {
                            self.estate_chest_revisions.insert(id, event.revision);
                            self.estate_chest_deleted.remove(&id);
                            self.store_estate_chest(chest);
                        }
                    }
                }
            }
        }
    }
}
