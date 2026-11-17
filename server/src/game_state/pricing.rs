//! Pricing system (doc/PRICING.md): hourly gold snapshots and the price
//! meeting on Serin's dark evening that moves the merchant price index.

use std::sync::Arc;

use tracing::{info, warn};

use onlinerpg_shared::celestial::is_after_sunset;
use onlinerpg_shared::moon::{days_until_serin_dark, is_serin_dark_day};
use onlinerpg_shared::pricing::{PricingNotice, Trend};

use super::time::{GAME_SECONDS_PER_DAY, REAL_DAY_DURATION_SECONDS};
use crate::auth::{unix_now, AuthService, PricingMeeting, PricingState};
use crate::metrics::DAY_SECONDS;
use crate::types::{PlayerId, ServerMessage};
use crate::world_config::{world_config, PricingConfig};

/// The baseline averages this many real days and sits at their midpoint.
const BASELINE_DAYS: i64 = 30;
const BASELINE_OFFSET_SECONDS: i64 = BASELINE_DAYS * DAY_SECONDS / 2;

/// Fractional step for `M`'s gap above the target path (doc/PRICING.md 조정 공식).
fn index_step(gap: f64, cfg: &PricingConfig) -> f64 {
    (cfg.gain * gap).clamp(-cfg.max_step_per_meeting, cfg.max_step_per_meeting)
}

fn grown(stored: f64, days: f64, growth: f64) -> Option<f64> {
    let target = stored * (1.0 + growth).powf(days);
    (target > 0.0).then_some(target)
}

/// The stored target grown to `game_day`; a `g` change applies only forward.
fn target_on(state: &PricingState, game_day: i64, cfg: &PricingConfig) -> Option<f64> {
    let (stored, day) = state.target_m.zip(state.target_day)?;
    grown(stored, (game_day - day) as f64, cfg.target_daily_growth)
}

/// Unix time at which the game clock reaches `game_day`.
fn game_day_ts(game_day: i64, day_now: f64) -> i64 {
    unix_now() - ((day_now - game_day as f64) * REAL_DAY_DURATION_SECONDS) as i64
}

/// The start of a path stored before `target_since`: the baseline midpoint
/// before the first meeting held on it, else `target_day`. `meetings` are
/// `(ts, game_day, reference)` oldest first.
fn inferred_since(state: &PricingState, day_now: f64, meetings: &[(i64, i64, f64)]) -> Option<i64> {
    let cfg = &world_config().pricing;
    let on_path = |&&(_, day, reference): &&(i64, i64, f64)| {
        target_on(state, day, cfg).is_some_and(|target| (reference / target - 1.0).abs() < 1e-6)
    };
    Some(match meetings.iter().find(on_path) {
        Some((ts, ..)) => ts - BASELINE_OFFSET_SECONDS,
        None => game_day_ts(state.target_day?, day_now),
    })
}

/// The target path over real time, for the dashboard: the targets the
/// meetings compared against, continued from the stored one.
pub struct TargetPath {
    since: i64,
    growth: f64,
    /// `(ts, target)`, oldest first, never empty.
    anchors: Vec<(i64, f64)>,
}

impl TargetPath {
    pub fn new(state: &PricingState, day_now: f64, meetings: &[(i64, i64, f64)]) -> Option<Self> {
        let (target, target_day) = state.target_m.zip(state.target_day)?;
        let since = state.target_since?;
        let mut anchors: Vec<_> = meetings
            .iter()
            .filter(|(ts, _, reference)| *ts >= since + BASELINE_OFFSET_SECONDS && *reference > 0.0)
            .map(|&(ts, _, reference)| (ts, reference))
            .collect();
        let stored_ts = game_day_ts(target_day, day_now);
        let recorded = meetings
            .last()
            .is_some_and(|(_, day, _)| *day == target_day);
        if !recorded && anchors.last().is_none_or(|(ts, _)| *ts < stored_ts) {
            anchors.push((stored_ts, target));
        }
        Some(Self {
            since,
            growth: world_config().pricing.target_daily_growth,
            anchors,
        })
        .filter(|path| !path.anchors.is_empty())
    }

    /// None before the path starts. Between two meetings it runs from one
    /// target to the next; outside them it grows at the configured rate.
    pub fn at(&self, ts: i64) -> Option<f64> {
        if ts < self.since {
            return None;
        }
        let next = self.anchors.partition_point(|(at, _)| *at <= ts);
        let before = next.checked_sub(1).map(|index| self.anchors[index]);
        match (before, self.anchors.get(next)) {
            (Some((t0, v0)), Some(&(t1, v1))) => {
                Some(v0 * (v1 / v0).powf((ts - t0) as f64 / (t1 - t0) as f64))
            }
            (Some((at, target)), None) | (None, Some(&(at, target))) => grown(
                target,
                (ts - at) as f64 / REAL_DAY_DURATION_SECONDS,
                self.growth,
            ),
            (None, None) => None,
        }
    }
}

pub(crate) fn next_index_percent(current: u32, gap: f64, cfg: &PricingConfig) -> u32 {
    let step = index_step(gap, cfg);
    let index = (f64::from(current) / 100.0 * (1.0 + step)).clamp(cfg.index_min, cfg.index_max);
    (index * 100.0).round() as u32
}

impl super::GameState {
    fn game_day_now(&self) -> f64 {
        self.current_total_game_seconds() as f64 / GAME_SECONDS_PER_DAY as f64
    }

    pub async fn load_pricing(&self, auth: &Arc<AuthService>) {
        let auth = Arc::clone(auth);
        let day_now = self.game_day_now();
        let loaded = super::auth_db(move || {
            let mut state = auth.load_pricing_state()?;
            if state.target_since.is_none() {
                let meetings = auth.pricing_meeting_references()?;
                if let Some(since) = inferred_since(&state, day_now, &meetings) {
                    state.target_since = Some(since);
                    auth.save_pricing_state(&state, None)?;
                    info!("pricing: target path starts at {since}");
                }
            }
            Ok(state)
        })
        .await;
        match loaded {
            Ok(state) => {
                info!("pricing: index {}%", state.index_percent);
                *self.pricing.write().await = state;
            }
            Err(err) => warn!("pricing: failed to load state, using defaults: {err}"),
        }
    }

    pub async fn price_index_percent(&self) -> u32 {
        self.pricing.read().await.index_percent
    }

    /// The stored state and the fractional game day, for `TargetPath::new`.
    pub async fn pricing_snapshot(&self) -> (PricingState, f64) {
        let day_now = self.game_day_now();
        (self.pricing.read().await.clone(), day_now)
    }

    #[cfg(test)]
    pub(crate) async fn set_price_index_percent(&self, percent: u32) {
        self.pricing.write().await.index_percent = percent;
    }

    /// The hourly metrics job records the saved gold supply.
    pub async fn tick_gold_snapshot(&self, auth: &Arc<AuthService>) {
        let auth = Arc::clone(auth);
        let active_days = world_config().pricing.active_days;
        match super::auth_db(move || auth.record_gold_snapshot(unix_now(), active_days)).await {
            Ok(Some(ts)) => info!("gold snapshot: recorded hour {ts}"),
            Ok(None) => {}
            Err(err) => warn!("gold snapshot: {err}"),
        }
    }

    /// The market picture for NPC roleplay; the trend projects the next
    /// meeting's step from the latest hourly snapshot.
    pub async fn pricing_notice(&self, auth: &Arc<AuthService>) -> ServerMessage {
        let state = self.pricing.read().await.clone();
        let game_day = self.current_game_day();
        let reader = Arc::clone(auth);
        let (last_change_pct, reading) =
            super::auth_db(move || reader.pricing_notice_inputs(unix_now()))
                .await
                .unwrap_or_else(|err| {
                    warn!("pricing: notice fell back to defaults: {err}");
                    (0, None)
                });

        let cfg = &world_config().pricing;
        let step = reading
            .zip(target_on(&state, game_day, cfg))
            .map_or(0.0, |(now, target)| index_step(now / target - 1.0, cfg));
        let trend = if step > 0.01 {
            Trend::Rising
        } else if step < -0.01 {
            Trend::Falling
        } else {
            Trend::Steady
        };
        let mut meeting_in_days = days_until_serin_dark(game_day);
        if meeting_in_days == 0 && state.last_meeting_day == Some(game_day) {
            meeting_in_days = days_until_serin_dark(game_day + 1) + 1;
        }
        ServerMessage::PricingNotice(PricingNotice {
            index_percent: state.index_percent,
            last_change_pct,
            trend,
            meeting_in_days,
        })
    }

    async fn send_pricing_notice_to_npcs(&self, auth: &Arc<AuthService>) {
        let npcs: Vec<PlayerId> = {
            let players = self.players.read().await;
            players
                .values()
                .filter(|p| p.is_official_npc)
                .map(|p| p.id)
                .collect()
        };
        if npcs.is_empty() {
            return;
        }
        let notice = self.pricing_notice(auth).await;
        self.send_direct_message_to_players(&npcs, notice).await;
    }

    /// Holds the merchants' price meeting once per Serin dark evening. The
    /// first one fixes the baseline; each one advances the stored target.
    pub async fn tick_pricing_meeting(&self, auth: &Arc<AuthService>) {
        let datetime = self.current_game_datetime();
        let game_day = self.current_game_day();
        if !is_serin_dark_day(game_day) || !is_after_sunset(&datetime) {
            return;
        }
        let mut state = self.pricing.write().await;
        if state.last_meeting_day.is_some_and(|day| day >= game_day) {
            return;
        }

        let cfg = &world_config().pricing;
        let now = unix_now();
        let reader = Arc::clone(auth);
        let (base, m_now) = match super::auth_db(move || {
            reader
                .average_gold_per_account(now - BASELINE_DAYS * DAY_SECONDS, now)
                .and_then(|base| {
                    Ok((
                        base,
                        reader.average_gold_per_account(now - DAY_SECONDS, now)?,
                    ))
                })
        })
        .await
        {
            Ok(readings) => readings,
            Err(err) => return warn!("pricing: meeting skipped, DB read failed: {err}"),
        };
        if let Some(base) = base.filter(|_| state.target_m.is_none()) {
            let offset = BASELINE_OFFSET_SECONDS;
            state.target_m = Some(base);
            state.target_day = Some(game_day - (offset as f64 / REAL_DAY_DURATION_SECONDS) as i64);
            state.target_since = Some(now - offset);
            info!(
                "pricing: baseline gold/active account {base:.1} at day {:?}",
                state.target_day
            );
        }

        let target = target_on(&state, game_day, cfg);
        let meeting = m_now.zip(target).map(|(now, target)| {
            let gap = now / target - 1.0;
            PricingMeeting {
                game_day,
                reference: target,
                m_now: now,
                gap,
                index_before: state.index_percent,
                index_after: next_index_percent(state.index_percent, gap, cfg),
            }
        });
        if let Some(m) = &meeting {
            state.index_percent = m.index_after;
            info!(
                "pricing: meeting on day {game_day}: gold/active account {:.1} vs target {:.1} ({:+.1}%), index {}% -> {}%",
                m.m_now, m.reference, m.gap * 100.0, m.index_before, m.index_after
            );
        } else {
            info!("pricing: meeting on day {game_day} had no reading");
        }
        if target.is_some() {
            state.target_m = target;
            state.target_day = Some(game_day);
        }
        state.last_meeting_day = Some(game_day);
        let snapshot = state.clone();
        drop(state);

        let writer = Arc::clone(auth);
        if let Err(err) =
            super::auth_db(move || writer.save_pricing_state(&snapshot, meeting.as_ref())).await
        {
            warn!("pricing: failed to persist meeting: {err}");
        }
        self.send_pricing_notice_to_npcs(auth).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_path_runs_through_the_meeting_targets() {
        let day = REAL_DAY_DURATION_SECONDS as i64;
        let now = unix_now();
        let since = now - 40 * day - BASELINE_OFFSET_SECONDS;
        let state = PricingState {
            target_m: Some(130.0),
            target_day: Some(90),
            target_since: Some(since),
            ..Default::default()
        };
        let meetings = [
            (now - 60 * day - BASELINE_OFFSET_SECONDS, 40, 500.0),
            (now - 40 * day, 60, 100.0),
            (now - 20 * day, 80, 121.0),
        ];
        let path = TargetPath::new(&state, 100.0, &meetings).unwrap();
        let close = |ts, expected: f64| (path.at(ts).unwrap() - expected).abs() < 1e-6;
        assert_eq!(path.at(since - 1), None);
        assert!(close(now - 40 * day, 100.0));
        assert!(close(now - 30 * day, 110.0));
        assert!(close(now - 20 * day, 121.0));
        assert!(close(now - 15 * day, (121.0f64 * 130.0).sqrt()));
        let growth = world_config().pricing.target_daily_growth;
        assert!(close(now, 130.0 * (1.0 + growth).powi(10)));
        assert!(close(now - 41 * day, 100.0 / (1.0 + growth)));

        let recorded = PricingState {
            target_m: Some(121.0),
            target_day: Some(80),
            ..state.clone()
        };
        let path = TargetPath::new(&recorded, 100.0, &meetings).unwrap();
        assert_eq!(path.anchors.len(), 2);
        assert_eq!(
            TargetPath::new(&PricingState::default(), 100.0, &meetings).map(|_| ()),
            None
        );
    }

    #[test]
    fn a_path_without_its_start_infers_it_from_the_first_meeting_on_it() {
        let cfg = &world_config().pricing;
        let state = PricingState {
            target_m: Some(100.0),
            target_day: Some(40),
            ..Default::default()
        };
        let on_path = target_on(&state, 20, cfg).unwrap();
        let meetings = [
            (500_000, 0, 90.0),
            (700_000, 20, on_path),
            (900_000, 40, 100.0),
        ];
        assert_eq!(
            inferred_since(&state, 50.0, &meetings),
            Some(700_000 - BASELINE_OFFSET_SECONDS)
        );
        let fallback = inferred_since(&state, 50.0, &[]).unwrap();
        let expected = unix_now() - 10 * REAL_DAY_DURATION_SECONDS as i64;
        assert!((fallback - expected).abs() <= 1);
        assert_eq!(
            inferred_since(&PricingState::default(), 50.0, &meetings),
            None
        );
    }

    #[tokio::test]
    async fn loading_a_path_stored_without_its_start_infers_and_saves_it() {
        let (auth, _) = crate::test_util::temp_auth("pricing_target_since_backfill");
        let auth = Arc::new(auth);
        let game = crate::game_state::tests::make_test_game_state("pricing_target_since_backfill");
        let stored = PricingState {
            target_m: Some(100.0),
            target_day: Some(game.current_game_day()),
            ..Default::default()
        };
        auth.save_pricing_state(&stored, None).unwrap();
        game.load_pricing(&auth).await;
        let since = game.pricing.read().await.target_since.unwrap();
        assert!((unix_now() - since).abs() <= REAL_DAY_DURATION_SECONDS as i64);
        assert_eq!(auth.load_pricing_state().unwrap().target_since, Some(since));
    }

    #[test]
    fn index_follows_the_gap_above_the_target_path() {
        let cfg = PricingConfig::default();
        assert_eq!(next_index_percent(100, 0.0, &cfg), 100);
        // 0.5 * 12% = +6%.
        assert_eq!(next_index_percent(100, 0.12, &cfg), 106);
        assert_eq!(next_index_percent(100, -0.08, &cfg), 96);
        // Capped per meeting even on a gold flood.
        assert_eq!(next_index_percent(100, 2.0, &cfg), 110);
        // Floor and ceiling.
        assert_eq!(next_index_percent(92, -0.5, &cfg), 90);
        assert_eq!(next_index_percent(198, 1.0, &cfg), 200);
    }

    #[test]
    fn target_grows_from_the_stored_day_at_the_current_rate() {
        let mut cfg = PricingConfig::default();
        let mut state = PricingState::default();
        assert_eq!(target_on(&state, 10, &cfg), None);
        state.target_m = Some(100.0);
        state.target_day = Some(10);
        assert_eq!(target_on(&state, 10, &cfg), Some(100.0));
        let whole = target_on(&state, 50, &cfg).unwrap();
        state.target_m = target_on(&state, 30, &cfg);
        state.target_day = Some(30);
        assert!((target_on(&state, 50, &cfg).unwrap() - whole).abs() < 1e-9);
        cfg.target_daily_growth = 0.0;
        assert_eq!(target_on(&state, 50, &cfg), state.target_m);
    }
}
