use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

const PRUNE_AT: usize = 4096;

/// When each key may next act.
pub(super) struct Cooldowns<K>(HashMap<K, Instant>);

impl<K> Default for Cooldowns<K> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Hash + Eq> Cooldowns<K> {
    #[cfg(test)]
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    /// Starts the cooldown and returns true when `key` may act now.
    pub(super) fn try_start(&mut self, key: K, cooldown: Duration, now: Instant) -> bool {
        if self.0.get(&key).is_some_and(|until| now < *until) {
            return false;
        }
        if self.0.len() >= PRUNE_AT {
            self.0.retain(|_, until| now < *until);
        }
        self.0.insert(key, now + cooldown);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_blocks_each_key_until_it_elapses() {
        let mut cooldowns = Cooldowns::default();
        let start = Instant::now();
        let wait = Duration::from_secs(60);
        assert!(cooldowns.try_start((1, "post"), wait, start));
        assert!(!cooldowns.try_start((1, "post"), wait, start + Duration::from_secs(1)));
        assert!(cooldowns.try_start((1, "comment"), wait, start));
        assert!(cooldowns.try_start((2, "post"), wait, start));
        assert!(cooldowns.try_start((1, "post"), wait, start + wait));
    }
}
