use crate::types::{PlayerId, ServerMessage};
use bytes::Bytes;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

pub(crate) const MAX_WATCHERS_PER_ACCOUNT: usize = 4;
/// A watcher that falls this far behind is dropped; its page reconnects and
/// starts from a fresh snapshot.
const WATCHER_BACKLOG: usize = 4096;
/// A watcher resync resets the player's own view too, so the account's
/// watchers share one allowance.
const RESYNC_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Copies one account's private stream to the browsers watching it. Bound to
/// the account, not a player id, so it outlives re-entry and re-login.
#[derive(Default)]
pub(crate) struct AccountWatch {
    watchers: Mutex<Vec<mpsc::Sender<Bytes>>>,
    /// Lets the owner's hot path skip the lock while nobody watches.
    count: AtomicUsize,
    player: Mutex<Option<PlayerId>>,
    last_resync: Mutex<Option<Instant>>,
}

impl AccountWatch {
    pub(crate) fn tee(&self, bytes: &Bytes) {
        if !self.is_watched() {
            return;
        }
        let mut watchers = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
        watchers.retain(|tx| tx.try_send(bytes.clone()).is_ok());
        self.count.store(watchers.len(), Ordering::Relaxed);
    }

    pub(crate) fn tee_message(&self, msg: &ServerMessage) {
        if !self.is_watched() {
            return;
        }
        if let Some(bytes) = super::encode_server_msg(msg) {
            self.tee(&bytes);
        }
    }

    /// `None` once the account already has the most watchers allowed.
    pub(crate) fn attach(&self) -> Option<mpsc::Receiver<Bytes>> {
        let mut watchers = self.watchers.lock().unwrap_or_else(|e| e.into_inner());
        watchers.retain(|tx| !tx.is_closed());
        if watchers.len() >= MAX_WATCHERS_PER_ACCOUNT {
            return None;
        }
        let (tx, rx) = mpsc::channel(WATCHER_BACKLOG);
        watchers.push(tx);
        self.count.store(watchers.len(), Ordering::Relaxed);
        Some(rx)
    }

    pub(crate) fn is_watched(&self) -> bool {
        self.count.load(Ordering::Relaxed) > 0
    }

    pub(crate) fn player(&self) -> Option<PlayerId> {
        *self.player.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// True at most once per `RESYNC_MIN_INTERVAL` across all watchers.
    pub(crate) fn resync_due(&self) -> bool {
        let mut last = self.last_resync.lock().unwrap_or_else(|e| e.into_inner());
        if last.is_some_and(|at| at.elapsed() < RESYNC_MIN_INTERVAL) {
            return false;
        }
        *last = Some(Instant::now());
        true
    }

    pub(crate) fn entered(&self, id: PlayerId) {
        *self.player.lock().unwrap_or_else(|e| e.into_inner()) = Some(id);
    }

    /// A replacement session may have entered first; only the current
    /// player's exit sends the watchers back to waiting.
    pub(crate) fn left(&self, id: PlayerId) {
        {
            let mut player = self.player.lock().unwrap_or_else(|e| e.into_inner());
            if *player != Some(id) {
                return;
            }
            *player = None;
        }
        self.tee_message(&ServerMessage::WatchWaiting);
    }

    fn close(&self) {
        self.watchers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.count.store(0, Ordering::Relaxed);
    }
}

impl super::GameState {
    pub(crate) async fn watched_player(
        &self,
        watch: &AccountWatch,
    ) -> Option<crate::types::Player> {
        let id = watch.player()?;
        self.players.read().await.get(&id).cloned()
    }

    pub(crate) fn account_watch(&self, account_name: &str) -> Arc<AccountWatch> {
        let mut watches = self
            .account_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        Arc::clone(
            watches
                .entry(account_name.to_ascii_lowercase())
                .or_default(),
        )
    }

    /// Drops the entry once neither an owner nor a watcher holds it.
    pub(crate) fn release_account_watch(&self, account_name: &str, watch: Arc<AccountWatch>) {
        let key = account_name.to_ascii_lowercase();
        let mut watches = self
            .account_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        drop(watch);
        if watches
            .get(&key)
            .is_some_and(|watch| Arc::strong_count(watch) == 1)
        {
            watches.remove(&key);
        }
    }

    pub(crate) fn watch_player(&self, id: PlayerId, watch: &Arc<AccountWatch>) {
        self.player_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id, Arc::clone(watch));
    }

    pub(crate) fn unwatch_player(&self, id: PlayerId) {
        self.player_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }

    pub(crate) fn tee_to_watchers(&self, id: &PlayerId, msg: &ServerMessage) {
        let watch = self
            .player_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned();
        if let Some(watch) = watch {
            watch.tee_message(msg);
        }
    }

    /// Ends every watcher of the account, as a ban or kick must.
    pub(crate) fn close_account_watch(&self, account_name: &str) {
        let watches = self
            .account_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(watch) = watches.get(&account_name.to_ascii_lowercase()) {
            watch.close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tee_reaches_watchers_and_drops_closed_ones() {
        let watch = AccountWatch::default();
        let mut first = watch.attach().unwrap();
        let second = watch.attach().unwrap();
        drop(second);
        watch.tee(&Bytes::from_static(b"x"));
        assert_eq!(first.try_recv().unwrap(), Bytes::from_static(b"x"));
        assert_eq!(watch.count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn attach_caps_watchers() {
        let watch = AccountWatch::default();
        let held: Vec<_> = (0..MAX_WATCHERS_PER_ACCOUNT)
            .map(|_| watch.attach().unwrap())
            .collect();
        assert!(watch.attach().is_none());
        drop(held);
        assert!(watch.attach().is_some());
    }

    #[test]
    fn a_watcher_too_far_behind_is_dropped() {
        let watch = AccountWatch::default();
        let _rx = watch.attach().unwrap();
        for _ in 0..=WATCHER_BACKLOG {
            watch.tee(&Bytes::from_static(b"x"));
        }
        assert!(!watch.is_watched());
    }

    #[test]
    fn a_stale_exit_keeps_the_new_player() {
        let watch = AccountWatch::default();
        let mut rx = watch.attach().unwrap();
        watch.entered(PlayerId::from(1));
        watch.entered(PlayerId::from(2));
        watch.left(PlayerId::from(1));
        assert_eq!(watch.player(), Some(PlayerId::from(2)));
        assert!(rx.try_recv().is_err());
        watch.left(PlayerId::from(2));
        assert_eq!(watch.player(), None);
        assert!(rx.try_recv().is_ok());
    }
}
