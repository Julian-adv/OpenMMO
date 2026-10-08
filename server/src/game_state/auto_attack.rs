use super::GameState;
use crate::auth::AuthService;
use crate::types::{PlayerId, ServerMessage};
use std::collections::{hash_map::Entry, BTreeSet, HashMap};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio::task::JoinSet;
use tokio::time::{Duration, Instant};

pub(super) struct AutoAttack {
    monster_id: String,
    dagger_skill: bool,
    request_id: u32,
    /// Any other action bumps the player's version, which ends the attack.
    action_version: u64,
}

pub(super) type AttackSession = Arc<Mutex<Option<AutoAttack>>>;

#[derive(Default)]
pub(super) struct AttackSchedule {
    deadlines: BTreeSet<(Instant, u64)>,
    players: HashMap<PlayerId, (Instant, AttackSession)>,
}

impl AttackSchedule {
    /// True when `deadline` became the earliest, so the waiter must re-arm.
    fn insert(&mut self, id: PlayerId, deadline: Instant, session: AttackSession) -> bool {
        if let Some((previous, _)) = self.players.insert(id, (deadline, session)) {
            self.deadlines.remove(&(previous, id.get()));
        }
        self.deadlines.insert((deadline, id.get()));
        self.deadlines.first() == Some(&(deadline, id.get()))
    }

    fn remove(&mut self, id: &PlayerId, session: &AttackSession) {
        if let Entry::Occupied(entry) = self.players.entry(*id) {
            if Arc::ptr_eq(&entry.get().1, session) {
                let (deadline, _) = entry.remove();
                self.deadlines.remove(&(deadline, id.get()));
            }
        }
    }

    fn take_due(&mut self, now: Instant) -> Option<(PlayerId, AttackSession)> {
        let &(deadline, id) = self.deadlines.first()?;
        if deadline > now {
            return None;
        }
        self.deadlines.pop_first();
        let id = PlayerId::from(id);
        let (_, session) = self.players.remove(&id)?;
        Some((id, session))
    }
}

impl GameState {
    pub(crate) async fn start_player_attack(
        &self,
        player_id: PlayerId,
        monster_id: String,
        dagger_skill: bool,
        request_id: u32,
        auth: Option<Arc<AuthService>>,
    ) {
        // A stop may remove the session between the map insert and the lock;
        // retry so the attack never lands in an orphaned session.
        let mut attack = loop {
            let session = self
                .auto_attacks
                .write()
                .await
                .entry(player_id)
                .or_default()
                .clone();
            let guard = session.clone().lock_owned().await;
            if self.session_is_current(&player_id, &session).await {
                break guard;
            }
        };
        let action_version = self.action_version(&player_id).await;
        if let Some(stale) = attack.take_if(|a| a.action_version != action_version) {
            self.send_attack_stopped(&player_id, stale).await;
        }
        if let Some(current) = attack.as_mut().filter(|a| a.monster_id == monster_id) {
            if current.request_id != request_id {
                current.request_id = request_id;
                current.dagger_skill = dagger_skill;
            }
            return;
        }
        *attack = Some(AutoAttack {
            monster_id,
            dagger_skill,
            request_id,
            action_version,
        });
        self.advance_player_attack(player_id, attack, &auth).await;
    }

    pub(crate) async fn stop_player_attack(&self, player_id: &PlayerId) {
        self.stop_player_attack_request(player_id, None).await;
    }

    pub(crate) async fn stop_player_attack_request(
        &self,
        player_id: &PlayerId,
        request_id: Option<u32>,
    ) {
        let session = self.auto_attacks.read().await.get(player_id).cloned();
        if let Some(session) = session {
            let mut attack = session.lock_owned().await;
            if attack
                .as_ref()
                .is_some_and(|a| request_id.is_none_or(|id| a.request_id == id))
            {
                self.end_player_attack(player_id, &mut attack).await;
            }
        }
    }

    pub(crate) async fn set_player_attack_skill(
        &self,
        player_id: &PlayerId,
        request_id: u32,
        dagger_skill: bool,
    ) {
        let session = self.auto_attacks.read().await.get(player_id).cloned();
        let Some(session) = session else {
            return;
        };
        let mut guard = session.lock().await;
        if let Some(attack) = guard.as_mut().filter(|a| a.request_id == request_id) {
            attack.dagger_skill = dagger_skill;
        }
    }

    pub(crate) async fn wait_for_player_attack(&self) {
        loop {
            let deadline = self.attack_schedule.lock().await.deadlines.first().copied();
            if let Some((deadline, _)) = deadline {
                tokio::select! {
                    _ = self.attack_wakeup.notified() => {},
                    _ = tokio::time::sleep_until(deadline) => return,
                }
            } else {
                self.attack_wakeup.notified().await;
            }
        }
    }

    /// Swings run in parallel; the per-session lock keeps each player's in order.
    pub(crate) async fn process_due_player_attacks(&self, auth: Option<Arc<AuthService>>) {
        let now = Instant::now();
        let mut swings = JoinSet::new();
        loop {
            let next = self.attack_schedule.lock().await.take_due(now);
            let Some((id, session)) = next else { break };
            let game = self.clone();
            let auth = auth.clone();
            swings.spawn(async move {
                let attack = session.clone().lock_owned().await;
                if game.session_is_current(&id, &session).await {
                    game.advance_player_attack(id, attack, &auth).await;
                }
            });
        }
        while let Some(swing) = swings.join_next().await {
            if swing.is_err() {
                tracing::error!("player attack swing panicked");
            }
        }
    }

    pub(super) async fn wake_player_attack(&self, player_id: &PlayerId) {
        if !self.auto_attacks.read().await.contains_key(player_id) {
            return;
        }
        let mut schedule = self.attack_schedule.lock().await;
        if let Some((_, session)) = schedule.players.get(player_id).cloned() {
            if schedule.insert(*player_id, Instant::now(), session) {
                self.attack_wakeup.notify_one();
            }
        }
    }

    async fn session_is_current(&self, player_id: &PlayerId, session: &AttackSession) -> bool {
        self.auto_attacks
            .read()
            .await
            .get(player_id)
            .is_some_and(|current| Arc::ptr_eq(current, session))
    }

    async fn schedule_player_attack(
        &self,
        player_id: PlayerId,
        guard: &OwnedMutexGuard<Option<AutoAttack>>,
    ) {
        let Some(attack) = guard.as_ref() else { return };
        let delay = if self.action_version(&player_id).await != attack.action_version {
            0
        } else {
            self.player_attack_remaining_ms(&player_id).await
        };
        let sessions = self.auto_attacks.read().await;
        let session = OwnedMutexGuard::mutex(guard);
        if sessions
            .get(&player_id)
            .is_some_and(|current| Arc::ptr_eq(current, session))
            && self.attack_schedule.lock().await.insert(
                player_id,
                Instant::now() + Duration::from_millis(delay),
                session.clone(),
            )
        {
            self.attack_wakeup.notify_one();
        }
    }

    async fn advance_player_attack(
        &self,
        player_id: PlayerId,
        mut guard: OwnedMutexGuard<Option<AutoAttack>>,
        auth: &Option<Arc<AuthService>>,
    ) {
        let Some(attack) = guard.as_mut() else {
            return;
        };
        if self.action_version(&player_id).await != attack.action_version {
            self.end_player_attack(&player_id, &mut guard).await;
            return;
        }
        if !self.player_attack_ready(&player_id).await {
            self.schedule_player_attack(player_id, &guard).await;
            return;
        }
        // The swing stands a seated player up, which bumps the version itself.
        let posed = self
            .players
            .read()
            .await
            .get(&player_id)
            .is_some_and(|p| p.object_type.is_some());
        let monster_id = attack.monster_id.clone();
        let double_slash = if std::mem::take(&mut attack.dagger_skill) {
            self.prepare_dagger_double_slash(&player_id, monster_id.clone())
                .await
        } else {
            None
        };
        let swung = double_slash.is_some()
            || self
                .player_attack(&player_id, monster_id, auth.as_deref())
                .await;
        if !swung {
            self.end_player_attack(&player_id, &mut guard).await;
        } else if posed {
            attack.action_version = self.action_version(&player_id).await;
        }
        self.schedule_player_attack(player_id, &guard).await;
        if let Some(prepared) = double_slash {
            drop(guard);
            let game = self.clone();
            let auth = auth.clone();
            tokio::spawn(async move {
                game.resolve_dagger_double_slash(&player_id, prepared, auth.as_deref())
                    .await;
            });
        }
    }

    async fn end_player_attack(
        &self,
        player_id: &PlayerId,
        guard: &mut OwnedMutexGuard<Option<AutoAttack>>,
    ) {
        let Some(stopped) = guard.take() else {
            return;
        };
        {
            let session = OwnedMutexGuard::mutex(guard);
            let mut sessions = self.auto_attacks.write().await;
            if sessions
                .get(player_id)
                .is_some_and(|current| Arc::ptr_eq(current, session))
            {
                sessions.remove(player_id);
            }
        }
        self.attack_schedule
            .lock()
            .await
            .remove(player_id, OwnedMutexGuard::mutex(guard));
        self.send_attack_stopped(player_id, stopped).await;
    }

    async fn send_attack_stopped(&self, player_id: &PlayerId, stopped: AutoAttack) {
        self.send_direct_message(
            player_id,
            ServerMessage::PlayerAttackStopped {
                monster_id: stopped.monster_id,
                request_id: stopped.request_id,
            },
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn rescheduling_and_cancelling_does_not_accumulate_reservations() {
        let mut schedule = AttackSchedule::default();
        let id = PlayerId::from(1);
        let session = AttackSession::default();
        let now = Instant::now();
        for offset in 0..10000 {
            schedule.insert(id, now + Duration::from_millis(offset), session.clone());
        }
        assert_eq!(schedule.deadlines.len(), 1);
        assert_eq!(schedule.players.len(), 1);
        schedule.remove(&id, &session);
        assert!(schedule.take_due(now + Duration::from_secs(20)).is_none());
        assert!(schedule.deadlines.is_empty());
        assert!(schedule.players.is_empty());
    }
}
