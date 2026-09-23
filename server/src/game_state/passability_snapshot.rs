use onlinerpg_shared::pathfinding::PassabilityCache;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Weak};

#[derive(Default)]
pub(super) struct PassabilityStore(RwLock<Arc<PassabilityCache>>);

pub(super) struct Read<'a>(RwLockReadGuard<'a, Arc<PassabilityCache>>);

pub(super) struct Write<'a>(RwLockWriteGuard<'a, Arc<PassabilityCache>>);

impl PassabilityStore {
    pub(super) fn read(&self) -> Read<'_> {
        Read(self.0.read().unwrap_or_else(|e| e.into_inner()))
    }

    pub(super) fn write(&self) -> Write<'_> {
        Write(self.0.write().unwrap_or_else(|e| e.into_inner()))
    }

    pub(super) fn snapshot(&self) -> Arc<PassabilityCache> {
        Arc::clone(&self.read().0)
    }

    pub(super) fn is_current(&self, snapshot: &Weak<PassabilityCache>) -> bool {
        Weak::ptr_eq(snapshot, &Arc::downgrade(&self.read().0))
    }
}

impl Deref for Read<'_> {
    type Target = PassabilityCache;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Deref for Write<'_> {
    type Target = PassabilityCache;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Write<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::pathfinding::RuntimePassability;

    #[test]
    fn weak_snapshot_tokens_detect_edits_without_retaining_geometry() {
        let store = PassabilityStore::default();
        let snapshot = store.snapshot();
        let token = Arc::downgrade(&snapshot);
        drop(snapshot);
        assert!(store.is_current(&token));
        store.write().clear();
        assert!(!store.is_current(&token));
        assert!(token.upgrade().is_none());
    }

    #[test]
    fn searches_keep_old_geometry_without_blocking_an_edit() {
        let store = PassabilityStore::default();
        let before = store.snapshot();
        assert!(Arc::ptr_eq(&before, &store.snapshot()));
        store.write().insert(
            "test".into(),
            RuntimePassability {
                house_origin_x: 0.0,
                house_origin_z: 0.0,
                min_x: 0.0,
                max_x: 1.0,
                min_z: 0.0,
                max_z: 1.0,
                floors: vec![],
                stairwells: vec![],
                yields_to_trapped_mover: false,
                allows_projectiles: false,
                is_ground: true,
            },
        );
        let after = store.snapshot();
        assert!(before.is_empty());
        assert!(after.contains_key("test"));
        assert!(!Arc::ptr_eq(&before, &after));
        drop(store.write());
        assert!(store.is_current(&Arc::downgrade(&after)));
    }
}
