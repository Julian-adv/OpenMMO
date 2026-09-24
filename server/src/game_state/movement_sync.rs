use super::movement_audit::{self, Pose, Tick};
use super::GameState;
use crate::types::{PlayerId, Position};
use onlinerpg_shared::shortest_world_delta_x;
use serde::Serialize;
use std::collections::VecDeque;

pub(super) const RESYNC_QUEUE_LENGTH: usize = 24;
const SAMPLE_INTERVAL_MS: u64 = 150;
const HISTORY_MS: u64 = 30_000;
const SAMPLE_LIMIT: usize = 201;
const DIVERGENCE_CONFIRM_MS: u64 = 600;
const DETAIL_COOLDOWN_MS: u64 = 30_000;
const RESYNC_COOLDOWN_MS: u64 = 2_000;

#[derive(Clone, Serialize)]
pub(super) struct MovementSample {
    pub at_ms: u64,
    pub client_position: Position,
    pub client_rotation: f32,
    pub client_floor: i8,
    pub server_pose: Pose,
    pub queue_len: usize,
    pub oldest_request_age_ms: Option<u64>,
    pub last_tick: Option<Tick>,
    pub gap: f32,
}

#[derive(Serialize)]
pub(super) struct PendingResync {
    id: u64,
    at_ms: u64,
    discarded_messages: u64,
}

#[derive(Default)]
pub(super) struct SyncHistory {
    samples: VecDeque<MovementSample>,
    candidate_since: Option<u64>,
    diverged: bool,
    last_detail_ms: Option<u64>,
    last_resync_ms: Option<u64>,
    pending: Option<PendingResync>,
}

impl SyncHistory {
    pub(super) fn blocks_movement(&mut self) -> bool {
        let Some(pending) = &mut self.pending else {
            return false;
        };
        pending.discarded_messages += 1;
        true
    }

    fn sample(&mut self, sample: MovementSample) -> Option<&'static str> {
        let now = sample.at_ms;
        if self.pending.is_some()
            || self
                .samples
                .back()
                .is_some_and(|s| now.saturating_sub(s.at_ms) < SAMPLE_INTERVAL_MS)
        {
            return None;
        }
        if self
            .samples
            .back()
            .is_some_and(|s| now.saturating_sub(s.at_ms) > 1_000)
        {
            self.candidate_since = None;
        }
        while self
            .samples
            .front()
            .is_some_and(|s| now.saturating_sub(s.at_ms) > HISTORY_MS)
            || self.samples.len() >= SAMPLE_LIMIT
        {
            self.samples.pop_front();
        }
        let speed = sample
            .last_tick
            .as_ref()
            .filter(|tick| now.saturating_sub(tick.at_ms) < 1_000)
            .map_or(0.0, |tick| tick.speed);
        let threshold = (speed * 0.5).max(3.0);
        let gap = sample.gap;
        self.samples.push_back(sample);
        if gap < threshold * 0.5 {
            self.candidate_since = None;
            return std::mem::replace(&mut self.diverged, false).then_some("recovered");
        }
        if gap < threshold {
            self.candidate_since = None;
            return None;
        }
        let since = *self.candidate_since.get_or_insert(now);
        if !self.diverged
            && now.saturating_sub(since) >= DIVERGENCE_CONFIRM_MS
            && self
                .last_detail_ms
                .is_none_or(|at| now.saturating_sub(at) >= DETAIL_COOLDOWN_MS)
        {
            self.diverged = true;
            self.last_detail_ms = Some(now);
            return Some("divergence_started");
        }
        None
    }

    fn begin_resync(&mut self, now: u64) -> Option<u64> {
        if self.pending.is_some()
            || self
                .last_resync_ms
                .is_some_and(|at| now.saturating_sub(at) < RESYNC_COOLDOWN_MS)
        {
            return None;
        }
        let id = movement_audit::next_request_id();
        self.last_resync_ms = Some(now);
        self.pending = Some(PendingResync {
            id,
            at_ms: now,
            discarded_messages: 0,
        });
        Some(id)
    }

    fn acknowledge(&mut self, id: u64) -> Option<PendingResync> {
        if self.pending.as_ref().is_none_or(|p| p.id != id) {
            return None;
        }
        self.candidate_since = None;
        self.diverged = false;
        self.pending.take()
    }
}

impl GameState {
    pub(crate) fn movement_resync_pending(&self, player_id: &PlayerId) -> bool {
        self.movement_audit
            .with_sync(*player_id, SyncHistory::blocks_movement)
            .unwrap_or(false)
    }

    pub(crate) async fn record_movement_sample(
        &self,
        player_id: &PlayerId,
        position: Position,
        rotation: f32,
        floor_level: i8,
    ) {
        if self.owns_goal_movement(player_id).await {
            return;
        }
        if !position.is_finite() || !rotation.is_finite() {
            return;
        }
        let now = Self::now_ms();
        let queues = self.movement_intents.read().await;
        let players = self.players.read().await;
        let Some(player) = players.get(player_id) else {
            return;
        };
        let queue = queues.get(player_id);
        let sample = MovementSample {
            at_ms: now,
            client_position: position,
            client_rotation: rotation,
            client_floor: floor_level,
            server_pose: Pose::from(player),
            queue_len: queue.map_or(0, VecDeque::len),
            oldest_request_age_ms: queue
                .and_then(|q| q.front())
                .and_then(|i| i.request)
                .map(|r| now.saturating_sub(r.received_ms)),
            last_tick: self.movement_audit.last_tick(*player_id),
            gap: shortest_world_delta_x(player.position.x, position.x)
                .hypot(position.z - player.position.z),
        };
        drop(players);
        drop(queues);
        let event = self
            .movement_audit
            .with_sync(*player_id, |history| history.sample(sample))
            .flatten();
        if let Some(event) = event {
            self.log_movement_sync(
                *player_id,
                event,
                serde_json::json!({}),
                event != "recovered",
            );
        }
    }

    pub(crate) fn acknowledge_movement_resync(&self, player_id: &PlayerId, resync_id: u64) {
        if let Some(pending) = self
            .movement_audit
            .with_sync(*player_id, |h| h.acknowledge(resync_id))
            .flatten()
        {
            self.log_movement_sync(
                *player_id,
                "resync_acknowledged",
                serde_json::json!({
                    "resync_id": resync_id,
                    "elapsed_ms": Self::now_ms().saturating_sub(pending.at_ms),
                    "discarded_messages": pending.discarded_messages,
                }),
                false,
            );
        }
    }

    pub(super) fn begin_movement_resync(&self, player_id: PlayerId) -> Option<u64> {
        self.movement_audit
            .with_sync(player_id, |h| h.begin_resync(Self::now_ms()))
            .flatten()
    }

    pub(super) fn log_movement_sync(
        &self,
        player_id: PlayerId,
        event: &str,
        detail: serde_json::Value,
        full: bool,
    ) {
        let trace_id = movement_audit::next_request_id();
        let Some((samples, since_ms)) = self.movement_audit.with_sync(player_id, |h| {
            (
                if full {
                    h.samples.iter().cloned().collect::<Vec<_>>()
                } else {
                    h.samples.back().cloned().into_iter().collect()
                },
                h.candidate_since,
            )
        }) else {
            return;
        };
        let header = serde_json::json!({
            "schema": 2, "trace_id": trace_id, "player_id": player_id, "event": event,
            "at_ms": Self::now_ms(), "divergence_since_ms": since_ms, "detail": detail,
            "sample_count": samples.len(),
        });
        tracing::info!(target: "movement_audit", detail = %header, "Movement sync trace");
        movement_audit::emit_sync_parts(trace_id, player_id, "samples", &samples);
        if full {
            if let Some(history) = self.movement_audit.sync_trace(player_id) {
                history.log_sync_parts(trace_id, player_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(at_ms: u64, gap: f32) -> MovementSample {
        MovementSample {
            at_ms,
            client_position: Position {
                x: gap,
                y: 0.0,
                z: 0.0,
            },
            client_rotation: 0.0,
            client_floor: 0,
            server_pose: Pose {
                position: Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                rotation: 0.0,
                floor: 0,
                mounted: true,
            },
            queue_len: 0,
            oldest_request_age_ms: None,
            last_tick: None,
            gap,
        }
    }

    #[test]
    fn gradual_divergence_keeps_the_healthy_history_and_recovers() {
        let mut history = SyncHistory::default();
        for at_ms in (0..30_000).step_by(200) {
            assert_eq!(history.sample(sample(at_ms, 0.5)), None);
        }
        for (at_ms, gap) in [
            (30_000, 1.0),
            (30_200, 2.0),
            (30_400, 3.0),
            (30_600, 4.0),
            (30_800, 5.0),
        ] {
            assert_eq!(history.sample(sample(at_ms, gap)), None);
        }
        assert_eq!(
            history.sample(sample(31_000, 6.0)),
            Some("divergence_started")
        );
        assert_eq!(history.candidate_since, Some(30_400));
        assert_eq!(history.samples.front().unwrap().at_ms, 1_000);
        assert_eq!(history.samples.front().unwrap().gap, 0.5);
        assert_eq!(history.sample(sample(31_200, 7.0)), None);
        assert_eq!(history.sample(sample(31_400, 0.5)), Some("recovered"));
        assert_eq!(history.sample(sample(31_600, 0.5)), None);
    }

    #[test]
    fn transient_latency_and_sparse_reports_do_not_confirm_divergence() {
        let mut history = SyncHistory::default();
        for (at_ms, gap) in [(0, 0.5), (200, 8.0), (400, 0.5), (600, 8.0), (10_000, 8.0)] {
            assert_eq!(history.sample(sample(at_ms, gap)), None);
        }
        assert_eq!(history.candidate_since, Some(10_000));
        for at_ms in [10_200, 10_400] {
            assert_eq!(history.sample(sample(at_ms, 8.0)), None);
        }
        assert_eq!(
            history.sample(sample(10_600, 8.0)),
            Some("divergence_started")
        );
    }

    #[test]
    fn repeated_onset_is_logged_after_the_cooldown() {
        let mut history = SyncHistory::default();
        for at_ms in (0..=600).step_by(200) {
            history.sample(sample(at_ms, 4.0));
        }
        assert_eq!(history.sample(sample(800, 0.0)), Some("recovered"));
        for at_ms in (1_000..30_600).step_by(200) {
            assert_eq!(history.sample(sample(at_ms, 4.0)), None);
        }
        assert_eq!(
            history.sample(sample(30_600, 4.0)),
            Some("divergence_started")
        );
        assert!(history.samples.len() <= SAMPLE_LIMIT);
    }

    #[test]
    fn resync_requires_the_matching_ack_and_is_rate_limited() {
        let mut history = SyncHistory::default();
        let id = history.begin_resync(1_000).unwrap();
        assert!(history.blocks_movement());
        assert!(history.acknowledge(id + 1).is_none());
        assert!(history.begin_resync(4_000).is_none());
        assert!(history.blocks_movement());
        let ack = history.acknowledge(id).unwrap();
        assert_eq!(ack.discarded_messages, 2);
        assert!(!history.blocks_movement());
        assert!(history.acknowledge(id).is_none());
        assert!(history.begin_resync(2_999).is_none());
        assert!(history.begin_resync(3_000).is_some());
    }
}
