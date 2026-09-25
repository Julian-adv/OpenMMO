//! Fishing sessions: cast → wait → bite → hook → caught/escaped
//! (design: `doc/FISHING.md`). Server-authoritative: every timer and roll is
//! server-side. Timers are `tokio::time::Instant`s advanced by the 250 ms
//! tick, so paused-time tests drive the whole machine; player deadlines carry
//! `LATENCY_GRACE_MS` of slack.

use onlinerpg_shared::fishing::{
    fish_pull_ps, reel_speed_mps, stamina_drain_ps, stamina_max, FishState, FishingAction,
    FishingOutcome, BITE_WINDOW_MS, CAST_MS, CATCH_SLACK_M, EXHAUSTED_STEER_PER_TICK,
    FIGHT_TIMEOUT_MS, FISH_WANDER_RADIUS_M, FLOTSAM_SHARE_PCT, GIVE_LINE_EXTRA_MPS,
    LATENCY_GRACE_MS, MAX_CAST_DISTANCE_METERS, MIN_FISHABLE_DEPTH_M, MIN_FISH_DISTANCE_M,
    PANIC_BAND_M, REST_MAX_MS, REST_MIN_MS, RUN_MAX_MS, RUN_MAX_PER_RARITY_MS, RUN_MIN_MS,
    RUN_SPEED_BASE_MPS, RUN_SPEED_PER_RARITY_MPS, SHORE_SAMPLE_STEP_M, STAMINA_DRAIN_MIN_TENSION,
    STAMINA_RECOVER_PS, TENSION_GIVE_RELIEF_PS, TENSION_INITIAL, TENSION_MAX, TENSION_REEL_PS,
    TENSION_REST_DECAY_PS, TROPHY_FIGHT_TIMEOUT_MS, TROPHY_HOOK_CHECK_MS, TROPHY_HOOK_SLIP_CHANCE,
    TROPHY_MIN_TENSION, TROPHY_ROLL_CHANCE_PCT, TROPHY_TENSION_RATE, WAIT_MAX_MS, WAIT_MIN_MS,
    WATERLINE_MARGIN_M,
};
use onlinerpg_shared::inventory::EquipSlot;
use onlinerpg_shared::mount::MountKind;
use onlinerpg_shared::skills::SkillId;
use onlinerpg_shared::Position;
use rand::Rng;
use std::time::Duration;
use tokio::time::Instant;
use tracing::warn;

use super::GameState;
use crate::types::{Player, PlayerId, ServerMessage};

const OBSERVATION_RADIUS_M: f32 = 6.0;

fn can_watch_fishing(observer: &Player, angler: &Player) -> bool {
    !observer.is_official_npc
        && observer.health > 0
        && observer.floor_level == angler.floor_level
        && (observer.position.y - angler.position.y).abs() <= 3.0
        && observer.position.dist_xz_sq(&angler.position) <= OBSERVATION_RADIUS_M.powi(2)
}

/// Casts are only valid on the overworld floor — no fishing in dungeons or
/// on house upper floors, whose "water" would be a terrain-height fiction.
pub(crate) const OVERWORLD_FLOOR: i8 = 0;

pub(crate) enum FishingPhase {
    /// Rod is swinging; the bobber lands when this elapses.
    Casting { until: Instant },
    /// Bobber is floating; the fish bites at `bite_at`.
    Waiting { bite_at: Instant },
    /// Bobber dipped at `since`; `Hook` must arrive before
    /// `since + BITE_WINDOW_MS + LATENCY_GRACE_MS`.
    Bite { since: Instant },
    /// Hooked — the fight simulation runs on every tick until it lands,
    /// snaps, or times out. `last_tick` feeds the integration step.
    Fight {
        state: FightState,
        last_tick: Instant,
    },
}

/// Live fight variables, `Instant`-free so `step_fight` stays a pure
/// function of elapsed time (unit-tested below without a clock).
pub(crate) struct FightState {
    /// The angler's held stance (`Hold` until the first input).
    pub stance: FishingAction,
    pub fish_state: FishState,
    /// Time left in the current Running/Resting burst.
    pub state_ms_left: f32,
    pub tension: f32,
    pub trophy: bool,
    pub pressure_established: bool,
    pub loose_ms: f32,
    /// Remaining stamina; the pool size is `stamina_max(rarity)`.
    pub stamina: f32,
    /// Angler-to-fish line length (XZ meters).
    pub distance: f32,
    /// The closest the fish can be reeled: the session's waterline floor
    /// (`FishingSession::reel_floor_m`). Landing happens *at* this floor.
    pub min_distance: f32,
    /// Unit XZ direction from the angler toward the fish.
    pub dir_x: f32,
    pub dir_z: f32,
    pub elapsed_ms: f32,
}

/// Randomness for one behavior flip, drawn lazily (most ticks don't flip)
/// via a caller-supplied closure so `step_fight` stays deterministic under
/// test.
#[derive(Clone, Copy)]
pub(crate) struct FightRolls {
    pub next_run_ms: f32,
    pub next_rest_ms: f32,
    /// Heading jitter (radians) applied on a behavior flip.
    pub wander_rad: f32,
}

/// How one fight tick ended, if it did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum FightOutcome {
    /// Tension hit the max — the line snapped.
    Snapped,
    /// The fish threw the hook from lost pressure or the fight deadline.
    ThrewHook,
    /// Exhausted fish reeled inside catch range.
    Landed,
}

/// Advance the fight by `dt` seconds. Pure: randomness arrives through the
/// caller's `roll` closure (drawn only when the fish actually flips
/// behavior), positions are passed in, and the fish's new XZ position is
/// derived from `player`/`cast` afterward via `fight_fish_pos`.
pub(crate) fn step_fight(
    f: &mut FightState,
    dt: f32,
    rarity: u32,
    roll: &mut impl FnMut() -> FightRolls,
) -> Option<FightOutcome> {
    f.elapsed_ms += dt * 1000.0;
    let timeout_ms = if f.trophy {
        TROPHY_FIGHT_TIMEOUT_MS
    } else {
        FIGHT_TIMEOUT_MS
    };
    if f.elapsed_ms >= timeout_ms as f32 {
        return Some(FightOutcome::ThrewHook);
    }

    // How every burst starts, shared by the flip and the panic below.
    fn enter(f: &mut FightState, state: FishState, rolls: FightRolls) {
        f.fish_state = state;
        f.state_ms_left = match state {
            FishState::Running => rolls.next_run_ms,
            _ => rolls.next_rest_ms,
        };
        rotate_dir(f, rolls.wander_rad);
    }

    // Behavior: exhaustion is terminal; otherwise run/rest bursts, with a
    // panic burst whenever a lively fish is dragged too close to the angler.
    if f.stamina <= 0.0 {
        f.fish_state = FishState::Exhausted;
    } else if f.distance < f.min_distance + PANIC_BAND_M && f.fish_state != FishState::Running {
        enter(f, FishState::Running, roll());
    } else {
        f.state_ms_left -= dt * 1000.0;
        if f.state_ms_left <= 0.0 {
            let next = match f.fish_state {
                FishState::Running => FishState::Resting,
                _ => FishState::Running,
            };
            enter(f, next, roll());
        }
    }

    // Tension: the fish's pull vs the angler's stance.
    let mut tension_ps = match f.fish_state {
        FishState::Running => fish_pull_ps(rarity, f.distance),
        FishState::Resting | FishState::Exhausted => -TENSION_REST_DECAY_PS,
    };
    match f.stance {
        FishingAction::Reel if f.fish_state != FishState::Exhausted => {
            tension_ps += TENSION_REEL_PS;
        }
        FishingAction::GiveLine => tension_ps -= TENSION_GIVE_RELIEF_PS,
        _ => {}
    }
    if f.trophy {
        tension_ps *= TROPHY_TENSION_RATE;
    }
    f.tension = (f.tension + tension_ps * dt).max(0.0);
    if f.tension >= TENSION_MAX {
        return Some(FightOutcome::Snapped);
    }

    // Stamina: drag burns it only while Running under real tension; a rested
    // fish on a slack line gets its wind back.
    let drain_threshold = if f.trophy {
        TROPHY_MIN_TENSION
    } else {
        STAMINA_DRAIN_MIN_TENSION
    };
    if f.fish_state == FishState::Running && f.tension >= drain_threshold {
        f.stamina = (f.stamina - stamina_drain_ps(f.tension) * dt).max(0.0);
    } else if f.fish_state == FishState::Resting && f.tension < STAMINA_DRAIN_MIN_TENSION {
        f.stamina = (f.stamina + STAMINA_RECOVER_PS * dt).min(stamina_max(rarity));
    }

    // Distance: the run takes line, the reel takes it back.
    let mut speed = 0.0;
    if f.fish_state == FishState::Running {
        speed += RUN_SPEED_BASE_MPS + RUN_SPEED_PER_RARITY_MPS * rarity.max(1) as f32;
        if f.stance == FishingAction::GiveLine {
            speed += GIVE_LINE_EXTRA_MPS;
        }
    }
    if f.stance == FishingAction::Reel {
        speed -= reel_speed_mps(f.fish_state);
    }
    f.distance = (f.distance + speed * dt).max(f.min_distance);

    if f.fish_state == FishState::Exhausted
        && f.stance == FishingAction::Reel
        && f.distance <= f.min_distance + CATCH_SLACK_M
    {
        return Some(FightOutcome::Landed);
    }
    None
}

/// Check only uninterrupted loose running time after pressure was first established.
pub(crate) fn step_hook_slip(
    f: &mut FightState,
    dt: f32,
    roll: &mut impl FnMut() -> f32,
) -> Option<FightOutcome> {
    if !f.trophy || f.fish_state != FishState::Running || f.stamina <= 0.0 {
        f.loose_ms = 0.0;
        return None;
    }
    if f.tension >= TROPHY_MIN_TENSION {
        f.pressure_established = true;
        f.loose_ms = 0.0;
        return None;
    }
    if !f.pressure_established {
        return None;
    }
    f.loose_ms += dt * 1000.0;
    while f.loose_ms >= TROPHY_HOOK_CHECK_MS {
        f.loose_ms -= TROPHY_HOOK_CHECK_MS;
        if roll() < TROPHY_HOOK_SLIP_CHANCE {
            return Some(FightOutcome::ThrewHook);
        }
    }
    None
}

fn rotate_dir(f: &mut FightState, rad: f32) {
    let (sin, cos) = rad.sin_cos();
    let (x, z) = (f.dir_x, f.dir_z);
    f.dir_x = x * cos - z * sin;
    f.dir_z = x * sin + z * cos;
}

/// Where the fish (and so the bobber) sits after a step: along the angler's
/// line at `distance`, clamped into the wander disc around the cast point so
/// the float can't beach itself. The clamp feeds back into `distance`/`dir`.
/// `steer_to_cast` (0–1 per call) blends the heading back toward the cast
/// ray — used on the exhausted reel-in, so the fish comes home along the
/// line whose waterline the session actually measured.
pub(crate) fn fight_fish_pos(
    f: &mut FightState,
    player: &Position,
    cast: &Position,
    steer_to_cast: f32,
) -> Position {
    // Player-centered frame with seam-aware X deltas, so a fight straddling
    // the cylindrical world's X seam keeps sane geometry.
    let cast_dx = onlinerpg_shared::shortest_world_delta_x(player.x, cast.x);
    let cast_dz = cast.z - player.z;
    if steer_to_cast > 0.0 {
        let clen = (cast_dx * cast_dx + cast_dz * cast_dz).sqrt();
        if clen > f32::EPSILON {
            let bx = f.dir_x * (1.0 - steer_to_cast) + (cast_dx / clen) * steer_to_cast;
            let bz = f.dir_z * (1.0 - steer_to_cast) + (cast_dz / clen) * steer_to_cast;
            let blen = (bx * bx + bz * bz).sqrt();
            if blen > f32::EPSILON {
                f.dir_x = bx / blen;
                f.dir_z = bz / blen;
            }
        }
    }
    let mut fx = f.dir_x * f.distance;
    let mut fz = f.dir_z * f.distance;
    let (off_x, off_z) = (fx - cast_dx, fz - cast_dz);
    let off_len = (off_x * off_x + off_z * off_z).sqrt();
    if off_len > FISH_WANDER_RADIUS_M {
        let scale = FISH_WANDER_RADIUS_M / off_len;
        fx = cast_dx + off_x * scale;
        fz = cast_dz + off_z * scale;
    }
    let len = (fx * fx + fz * fz).sqrt();
    if len > f32::EPSILON {
        f.distance = len.max(f.min_distance);
        f.dir_x = fx / len;
        f.dir_z = fz / len;
    }
    Position {
        x: onlinerpg_shared::wrap_world_x(player.x + fx),
        y: cast.y,
        z: player.z + fz,
    }
}

/// Roll the pre-drawn randomness for one fight tick.
fn roll_fight(rarity: u32, rng: &mut impl Rng) -> FightRolls {
    FightRolls {
        next_run_ms: rng.gen_range(
            RUN_MIN_MS as f32..=(RUN_MAX_MS + RUN_MAX_PER_RARITY_MS * rarity.max(1)) as f32,
        ),
        next_rest_ms: rng.gen_range(REST_MIN_MS as f32..=REST_MAX_MS as f32),
        wander_rad: rng.gen_range(-0.5..=0.5f32),
    }
}

/// What bit the line. Rolled when the bite fires — not at resolution — so a
/// future "line tension hints at the catch" broadcast stays honest, but only
/// revealed to the player on a successful catch.
pub(crate) struct RolledFish {
    pub item_def_id: String,
    pub rarity: u32,
    pub size_cm: u16,
    pub trophy: bool,
}

pub(crate) struct FishingSession {
    pub observers: Vec<PlayerId>,
    /// Where the float currently sits — the cast point until the hook sets,
    /// then the fighting fish's live position.
    pub bobber: Position,
    /// Where the cast landed; anchors the fight's wander disc and the
    /// water-surface height the bobber keeps.
    pub cast_point: Position,
    /// The closest the fish can be reeled to the angler: the first fishable
    /// water along the cast ray (plus margin), at least the rod's reach.
    /// Measured once at cast time — the only spot tile IO is allowed.
    pub reel_floor_m: f32,
    pub phase: FishingPhase,
    pub rolled_fish: Option<RolledFish>,
    /// Unique per cast. Tick-queued work re-verifies it, so a session that
    /// was cancelled and re-cast between scan and handler is never touched
    /// by the old session's due entries.
    pub session_id: u64,
}

/// Pure catch-table entry, split out so the weighting is unit-testable
/// without a `GameState`.
#[derive(Debug)]
pub(crate) struct CatchCandidate {
    pub item_def_id: String,
    pub rarity: u32,
    pub catch_weight: u32,
}

/// Keep flotsam's fixed share while preserving species catch weights.
pub(crate) fn effective_weights(candidates: &[CatchCandidate]) -> Vec<u64> {
    let mut weights: Vec<u64> = candidates
        .iter()
        .map(|c| u64::from(c.catch_weight))
        .collect();

    let pool = |fish: bool| -> u64 {
        weights
            .iter()
            .zip(candidates)
            .filter(|(_, c)| (c.rarity >= 1) == fish)
            .map(|(w, _)| *w)
            .sum()
    };
    let (fish_total, flotsam_total) = (pool(true), pool(false));
    // A table of only fish or only flotsam has no split to hold; draw it raw.
    if fish_total == 0 || flotsam_total == 0 {
        return weights;
    }

    // Cross-multiply pool totals to preserve exact integer shares.
    for (weight, candidate) in weights.iter_mut().zip(candidates) {
        *weight *= if candidate.rarity >= 1 {
            flotsam_total * (100 - FLOTSAM_SHARE_PCT)
        } else {
            fish_total * FLOTSAM_SHARE_PCT
        };
    }
    weights
}

/// Weighted pick over the catch table. `roll` is a uniform draw in
/// `[0, total_weight)`; separating the draw from the pick keeps this pure.
pub(crate) fn pick_catch(weights: &[u64], mut roll: u64) -> Option<usize> {
    for (index, weight) in weights.iter().enumerate() {
        if roll < *weight {
            return Some(index);
        }
        roll -= weight;
    }
    None
}

pub(crate) fn roll_wait_ms(rng: &mut impl Rng) -> u64 {
    rng.gen_range(u64::from(WAIT_MIN_MS)..=u64::from(WAIT_MAX_MS))
}

impl GameState {
    /// Handle `ClientMessage::FishingCast`: validate everything the design
    /// requires (rod, floor, range, water, liveness) and open the session.
    pub async fn start_fishing(&self, player_id: &PlayerId, target: Position) {
        if self.fishing_sessions.read().await.contains_key(player_id) {
            self.send_fishing_error(player_id, "You are already fishing.")
                .await;
            return;
        }

        let (player_pos, player_rotation, player_floor, alive, mount, official_npc) = {
            let players = self.players.read().await;
            let Some(p) = players.get(player_id) else {
                return;
            };
            (
                p.position,
                p.rotation,
                p.floor_level,
                p.health > 0,
                p.mount,
                p.is_official_npc,
            )
        };
        if !alive {
            self.send_fishing_error(player_id, "You cannot fish while defeated.")
                .await;
            return;
        }
        if player_floor != OVERWORLD_FLOOR {
            self.send_fishing_error(player_id, "You can only fish outdoors.")
                .await;
            return;
        }

        if !self.main_hand_is_rod(player_id).await {
            self.send_fishing_error(player_id, "You need a fishing rod in your main hand.")
                .await;
            return;
        }

        if !official_npc && !self.has_skill(player_id, SkillId::Fishing).await {
            self.send_fishing_error(
                player_id,
                "Watch Tobin fish from casting to catching nearby to learn Fishing first.",
            )
            .await;
            return;
        }

        if player_pos.dist_xz_sq(&target) > MAX_CAST_DISTANCE_METERS * MAX_CAST_DISTANCE_METERS {
            self.send_fishing_error(player_id, "That water is out of casting range.")
                .await;
            return;
        }

        let boating = mount == Some(MountKind::Rowboat);
        if boating
            && !onlinerpg_shared::fishing::is_stern_cast(
                onlinerpg_shared::shortest_world_delta_x(player_pos.x, target.x),
                target.z - player_pos.z,
                player_rotation,
            )
        {
            self.send_fishing_error(player_id, "Cast into the water behind the boat.")
                .await;
            return;
        }

        // Water = baked surface meaningfully above the terrain bed; covers
        // ocean and rivers alike (doc/WATER_SYSTEM.md). First-touch tile IO
        // lives here in the cast handler, never in the tick. Kept inline
        // (not `water_depth_at`) for the surface value and the error split.
        let wx = onlinerpg_shared::wrap_world_x(target.x);
        let water_surface = match tokio::join!(
            self.height_sampler.sample_height(wx, target.z),
            self.water_sampler.sample_surface(wx, target.z),
        ) {
            (Ok(bed), Ok(surface)) if surface - bed > MIN_FISHABLE_DEPTH_M => surface,
            (Ok(_), Ok(_)) => {
                self.send_fishing_error(player_id, "You can only cast into water.")
                    .await;
                return;
            }
            (Err(err), _) | (_, Err(err)) => {
                warn!("start_fishing: water sample failed: {err}");
                self.send_fishing_error(player_id, "You can only cast into water.")
                    .await;
                return;
            }
        };

        // Scan the player→cast ray for where the water actually starts, so
        // the fight can never reel the float up onto the shore. Same tiles
        // the cast validation just touched, and still inside the cast
        // handler — the tick stays IO-free.
        let reel_floor_m = self.measure_reel_floor(&player_pos, wx, target.z).await;

        // The bobber floats on the actual water surface — sea level over the
        // ocean, but the carved channel height over a river.
        let bobber = Position {
            x: wx,
            y: water_surface,
            z: target.z,
        };
        let observers = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(angler) if angler.is_official_npc && angler.name == "Tobin" => players
                    .iter()
                    .filter(|(_, observer)| can_watch_fishing(observer, angler))
                    .map(|(id, _)| *id)
                    .collect(),
                _ => Vec::new(),
            }
        };
        {
            let mut sessions = self.fishing_sessions.write().await;
            sessions.insert(
                *player_id,
                FishingSession {
                    observers,
                    bobber,
                    cast_point: bobber,
                    reel_floor_m,
                    phase: FishingPhase::Casting {
                        until: Instant::now() + Duration::from_millis(u64::from(CAST_MS)),
                    },
                    rolled_fish: None,
                    session_id: self
                        .next_fishing_session
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                },
            );
        }
        self.fishing_active
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let rotation = if boating {
            player_rotation
        } else {
            player_pos.bearing_xz_to(&bobber).unwrap_or(player_rotation)
        };
        self.broadcast_fishing(
            &bobber,
            ServerMessage::FishingCasted {
                player_id: *player_id,
                position: bobber,
                rotation,
            },
        )
        .await;
    }

    /// Handle `ClientMessage::FishingRespond`. Timing is judged here against
    /// the server's own deadlines — a late hook is an escape no matter what
    /// the client believed.
    pub async fn respond_fishing(&self, player_id: &PlayerId, action: FishingAction) {
        let verdict = {
            let mut sessions = self.fishing_sessions.write().await;
            let Some(session) = sessions.get_mut(player_id) else {
                self.send_fishing_error(player_id, "You are not fishing.")
                    .await;
                return;
            };
            match &mut session.phase {
                FishingPhase::Fight { state, .. } => {
                    // A duplicated Hook racing the fight open is swallowed;
                    // anything else is the angler's new stance, applied by
                    // the next tick's integration step.
                    if action != FishingAction::Hook {
                        state.stance = action;
                    }
                    return;
                }
                // A stray Hold outside the fight (client racing the end of
                // one) is harmless; anything else before the hook has
                // consequences below.
                _ if action == FishingAction::Hold => return,
                // Yanking the rod before the bite scares the fish off.
                FishingPhase::Casting { .. } | FishingPhase::Waiting { .. } => Verdict::Escaped,
                FishingPhase::Bite { since } => {
                    let deadline = *since
                        + Duration::from_millis(u64::from(BITE_WINDOW_MS + LATENCY_GRACE_MS));
                    if Instant::now() > deadline {
                        // Too late — the tick will call it escaped; treat the
                        // stale response the same way rather than racing it.
                        Verdict::Escaped
                    } else if action == FishingAction::Hook {
                        Verdict::Hooked
                    } else {
                        // Reeling or giving line before the hook is set:
                        // the fish spits the bait.
                        Verdict::Escaped
                    }
                }
            }
        };

        match verdict {
            Verdict::Hooked => self.begin_fight(player_id).await,
            Verdict::Escaped => {
                self.end_fishing(player_id, FishingOutcome::Escaped).await;
            }
        }
    }

    /// Deliberate reel-in (`ClientMessage::FishingStop`).
    pub async fn stop_fishing(&self, player_id: &PlayerId) {
        self.end_fishing(player_id, FishingOutcome::Aborted).await;
    }

    /// Anything that breaks concentration — movement, combat, disconnect —
    /// lands here. Quiet no-op for the overwhelmingly common case of a
    /// player who isn't fishing.
    pub async fn cancel_fishing_if_active(&self, player_id: &PlayerId) {
        if self.no_fishing_anywhere() {
            return;
        }
        if self.fishing_sessions.read().await.contains_key(player_id) {
            self.end_fishing(player_id, FishingOutcome::Aborted).await;
        }
    }

    /// Lock-free "nobody is fishing" check for hot paths (every move packet).
    fn no_fishing_anywhere(&self) -> bool {
        self.fishing_active
            .load(std::sync::atomic::Ordering::Relaxed)
            == 0
    }

    /// Walk the player→cast ray in `SHORE_SAMPLE_STEP_M` steps and return
    /// the closest fishable-water distance plus `WATERLINE_MARGIN_M`, capped
    /// at the cast distance and floored at the rod's reach. Runs in the cast
    /// handler on the tiles the cast validation just touched — the tick
    /// stays IO-free. Sample failures just keep walking: the cast target
    /// itself already proved fishable, so the fallback is the cast distance.
    /// Ground height and water depth (surface − bed) at a wrapped-x point;
    /// `None` when a sampler fails. The one place both samplers are joined.
    pub(super) async fn ground_and_depth_at(&self, wx: f32, z: f32) -> Option<(f32, f32)> {
        match tokio::join!(
            self.height_sampler.sample_height(wx, z),
            self.water_sampler.sample_surface(wx, z),
        ) {
            (Ok(bed), Ok(surface)) => Some((bed, surface - bed)),
            _ => None,
        }
    }

    /// Water depth at a wrapped-x point.
    pub(super) async fn water_depth_at(&self, wx: f32, z: f32) -> Option<f32> {
        self.ground_and_depth_at(wx, z)
            .await
            .map(|(_, depth)| depth)
    }

    async fn measure_reel_floor(&self, player_pos: &Position, cast_x: f32, cast_z: f32) -> f32 {
        let dx = onlinerpg_shared::shortest_world_delta_x(player_pos.x, cast_x);
        let dz = cast_z - player_pos.z;
        let cast_dist = (dx * dx + dz * dz).sqrt();
        if cast_dist <= SHORE_SAMPLE_STEP_M {
            return MIN_FISH_DISTANCE_M;
        }
        let mut waterline = cast_dist;
        let mut t = SHORE_SAMPLE_STEP_M;
        while t < cast_dist {
            let x = onlinerpg_shared::wrap_world_x(player_pos.x + dx / cast_dist * t);
            let z = player_pos.z + dz / cast_dist * t;
            if let (Ok(bed), Ok(surface)) = tokio::join!(
                self.height_sampler.sample_height(x, z),
                self.water_sampler.sample_surface(x, z),
            ) {
                if surface - bed > MIN_FISHABLE_DEPTH_M {
                    waterline = t;
                    break;
                }
            }
            t += SHORE_SAMPLE_STEP_M;
        }
        (waterline + WATERLINE_MARGIN_M)
            .min(cast_dist)
            .max(MIN_FISH_DISTANCE_M)
    }

    /// Whether the player's main hand currently holds a fishing rod — the
    /// cast precondition, also re-checked when equipment changes mid-session.
    async fn main_hand_is_rod(&self, player_id: &PlayerId) -> bool {
        self.inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inv| inv.equipped.get(&EquipSlot::MainHand))
            .and_then(|item| self.item_defs.get(&item.item_def_id))
            .is_some_and(|def| def.is_fishing_rod())
    }

    /// Called after any equip/unequip: putting the rod away (or swapping a
    /// weapon into the main hand) reels the line in. Gear changes that leave
    /// the rod in hand — armor, an off-hand torch — don't break concentration.
    pub(super) async fn abort_fishing_if_rod_lost(&self, player_id: &PlayerId) {
        if self.no_fishing_anywhere() {
            return;
        }
        if !self.fishing_sessions.read().await.contains_key(player_id) {
            return;
        }
        if !self.main_hand_is_rod(player_id).await {
            self.cancel_fishing_if_active(player_id).await;
        }
    }

    /// Advance fishing sessions on the 250 ms tick.
    pub async fn tick_fishing(&self, auth: Option<&crate::auth::AuthService>) {
        if self.no_fishing_anywhere() {
            return;
        }
        let now = Instant::now();
        // Handlers re-verify the stamped session_id before acting. Fight
        // beats fire every tick for every fight, so they batch separately
        // under a single lock acquisition; the rest are rare transitions.
        enum Due {
            BobberLanded(PlayerId, u64),
            Bite(PlayerId, u64),
            Expired(PlayerId, u64),
            Aborted(PlayerId),
        }
        let mut due = Vec::new();
        let mut fights: Vec<(PlayerId, u64, Position)> = Vec::new();
        {
            let mut sessions = self.fishing_sessions.write().await;
            if sessions.is_empty() {
                return;
            }
            let players = self.players.read().await;
            for (player_id, session) in sessions.iter_mut() {
                let Some(player) = players.get(player_id).filter(|p| p.health > 0) else {
                    due.push(Due::Aborted(*player_id));
                    continue;
                };
                let sid = session.session_id;
                session.observers.retain(|id| {
                    players
                        .get(id)
                        .is_some_and(|observer| can_watch_fishing(observer, player))
                });
                match &session.phase {
                    FishingPhase::Casting { until } if now >= *until => {
                        due.push(Due::BobberLanded(*player_id, sid));
                    }
                    FishingPhase::Waiting { bite_at } if now >= *bite_at => {
                        due.push(Due::Bite(*player_id, sid));
                    }
                    FishingPhase::Bite { since }
                        if now
                            >= *since
                                + Duration::from_millis(u64::from(
                                    BITE_WINDOW_MS + 2 * LATENCY_GRACE_MS,
                                )) =>
                    {
                        // Doubled grace: a response that raced the deadline is
                        // judged in respond_fishing; the tick only reaps
                        // sessions nobody answered for.
                        due.push(Due::Expired(*player_id, sid));
                    }
                    FishingPhase::Fight { .. } => {
                        fights.push((*player_id, sid, player.position));
                    }
                    _ => {}
                }
            }
        }

        for entry in due {
            match entry {
                Due::Aborted(player_id) => {
                    self.end_fishing(&player_id, FishingOutcome::Aborted).await;
                }
                Due::BobberLanded(player_id, sid) => {
                    // rand's thread_rng is !Send: keep it inside an
                    // await-free block.
                    let wait_ms = roll_wait_ms(&mut rand::thread_rng());
                    let mut sessions = self.fishing_sessions.write().await;
                    if let Some(session) =
                        sessions.get_mut(&player_id).filter(|s| s.session_id == sid)
                    {
                        session.phase = FishingPhase::Waiting {
                            bite_at: now + Duration::from_millis(wait_ms),
                        };
                    }
                }
                Due::Bite(player_id, sid) => {
                    let rolled = self.roll_fish();
                    let bobber = {
                        let mut sessions = self.fishing_sessions.write().await;
                        let Some(session) =
                            sessions.get_mut(&player_id).filter(|s| s.session_id == sid)
                        else {
                            continue;
                        };
                        match rolled {
                            Some(fish) => {
                                session.rolled_fish = Some(fish);
                                session.phase = FishingPhase::Bite { since: now };
                                Some(session.bobber)
                            }
                            // Empty catch table (no fish defs): nothing can
                            // ever bite, end the session instead of hanging.
                            None => None,
                        }
                    };
                    match bobber {
                        Some(bobber) => {
                            self.broadcast_fishing(
                                &bobber,
                                ServerMessage::FishingBite { player_id },
                            )
                            .await;
                        }
                        None => {
                            self.end_fishing_if(&player_id, Some(sid), FishingOutcome::Escaped)
                                .await;
                        }
                    }
                }
                Due::Expired(player_id, sid) => {
                    self.end_fishing_if(&player_id, Some(sid), FishingOutcome::Escaped)
                        .await;
                }
            }
        }

        if fights.is_empty() {
            return;
        }
        // One write-lock pass steps every fight (each step is sub-microsecond
        // pure math — no awaits inside); broadcasts and endings follow
        // outside the lock.
        enum After {
            // Boxed: the message dwarfs the other two variants, and this
            // vector holds one entry per fight in flight.
            Beat(Position, Box<ServerMessage>),
            Landed,
            Escaped,
        }
        let mut results: Vec<(PlayerId, u64, After)> = Vec::with_capacity(fights.len());
        {
            let mut sessions = self.fishing_sessions.write().await;
            let mut rng = rand::thread_rng();
            for (player_id, sid, player_pos) in fights {
                let Some(session) = sessions.get_mut(&player_id).filter(|s| s.session_id == sid)
                else {
                    continue;
                };
                let rarity = rarity_of(&session.rolled_fish);
                let cast = session.cast_point;
                let FishingPhase::Fight { state, last_tick } = &mut session.phase else {
                    continue;
                };
                // Real elapsed time, capped so a stalled tick can't
                // teleport the simulation.
                let dt = now
                    .saturating_duration_since(*last_tick)
                    .as_secs_f32()
                    .min(1.0);
                *last_tick = now;
                let outcome = step_fight(state, dt, rarity, &mut || roll_fight(rarity, &mut rng))
                    .or_else(|| {
                        step_hook_slip(state, dt, &mut || {
                            #[cfg(test)]
                            {
                                *self.fishing_hook_roll.lock().unwrap()
                            }
                            #[cfg(not(test))]
                            rng.gen::<f32>()
                        })
                    });
                // The exhausted reel-in steers home along the cast ray,
                // whose waterline the session measured.
                let steer = if state.fish_state == FishState::Exhausted {
                    EXHAUSTED_STEER_PER_TICK
                } else {
                    0.0
                };
                session.bobber = fight_fish_pos(state, &player_pos, &cast, steer);
                let after = match outcome {
                    None => After::Beat(
                        session.bobber,
                        Box::new(ServerMessage::FishingFight {
                            player_id,
                            bobber: session.bobber,
                            fish_state: state.fish_state,
                            stance: state.stance,
                            tension_pct: state.tension.round() as u32,
                            trophy: state.trophy,
                            stamina_pct: (state.stamina / stamina_max(rarity) * 100.0).round()
                                as u32,
                        }),
                    ),
                    Some(FightOutcome::Landed) => After::Landed,
                    Some(FightOutcome::Snapped | FightOutcome::ThrewHook) => After::Escaped,
                };
                results.push((player_id, sid, after));
            }
        }
        for (player_id, sid, after) in results {
            match after {
                After::Beat(bobber, msg) => self.broadcast_fishing(&bobber, *msg).await,
                After::Landed => self.finish_fishing_caught(&player_id, sid, auth).await,
                After::Escaped => {
                    self.end_fishing_if(&player_id, Some(sid), FishingOutcome::Escaped)
                        .await;
                }
            }
        }
    }

    /// Roll species + size + trophy for a bite, from the item-def catch
    /// table (`category == "fish"`, weighted by `catchWeight`).
    fn roll_fish(&self) -> Option<RolledFish> {
        let candidates = self.item_defs.catch_table();
        if candidates.is_empty() {
            return None;
        }
        let weights = effective_weights(candidates);
        let total: u64 = weights.iter().sum();
        // All-zero weights (data-driven) would panic the gen_range below.
        if total == 0 {
            return None;
        }
        let (index, quality) = {
            let mut rng = rand::thread_rng();
            (
                pick_catch(&weights, rng.gen_range(0..total))?,
                rng.gen_range(0..100u32),
            )
        };
        let picked = &candidates[index];
        let def = self.item_defs.get(&picked.item_def_id)?;
        let mut size_cm = def
            .size_dice
            .as_deref()
            .map(crate::game::combat::roll_dice)
            .unwrap_or(10) as u16;
        let trophy_roll = def.is_fish() && quality < TROPHY_ROLL_CHANCE_PCT;
        if trophy_roll {
            size_cm = size_cm.saturating_mul(2);
        }
        let trophy = def.trophy_at(size_cm, trophy_roll);
        Some(RolledFish {
            item_def_id: picked.item_def_id.clone(),
            rarity: picked.rarity,
            size_cm,
            trophy,
        })
    }

    /// Successful hook: the fight begins mid-run — the hooked fish bolts,
    /// the line already carries the hook-set's tension, and the tick
    /// integrates from here.
    async fn begin_fight(&self, player_id: &PlayerId) {
        let Some(player_pos) = self.players.read().await.get(player_id).map(|p| p.position) else {
            return;
        };
        let (announce, trophy) = {
            let mut sessions = self.fishing_sessions.write().await;
            let Some(session) = sessions.get_mut(player_id) else {
                return;
            };
            let rarity = rarity_of(&session.rolled_fish);
            let dx = onlinerpg_shared::shortest_world_delta_x(player_pos.x, session.bobber.x);
            let dz = session.bobber.z - player_pos.z;
            let len = (dx * dx + dz * dz).sqrt();
            let (dir_x, dir_z) = if len > f32::EPSILON {
                (dx / len, dz / len)
            } else {
                (1.0, 0.0)
            };
            let rolls = roll_fight(rarity, &mut rand::thread_rng());
            let trophy = session.rolled_fish.as_ref().is_some_and(|f| f.trophy);
            session.phase = FishingPhase::Fight {
                state: FightState {
                    stance: FishingAction::Hold,
                    fish_state: FishState::Running,
                    state_ms_left: rolls.next_run_ms,
                    tension: TENSION_INITIAL,
                    trophy,
                    pressure_established: false,
                    loose_ms: 0.0,
                    stamina: stamina_max(rarity),
                    distance: len.max(session.reel_floor_m),
                    min_distance: session.reel_floor_m,
                    dir_x,
                    dir_z,
                    elapsed_ms: 0.0,
                },
                last_tick: Instant::now(),
            };
            (session.bobber, trophy)
        };
        self.broadcast_fishing(
            &announce,
            ServerMessage::FishingFight {
                player_id: *player_id,
                bobber: announce,
                fish_state: FishState::Running,
                stance: FishingAction::Hold,
                tension_pct: TENSION_INITIAL.round() as u32,
                stamina_pct: 100,
                trophy,
            },
        )
        .await;
    }

    /// Award the exhausted catch once; `sid` rejects a cancelled or replaced session.
    async fn finish_fishing_caught(
        &self,
        player_id: &PlayerId,
        sid: u64,
        auth: Option<&crate::auth::AuthService>,
    ) {
        let Some(fish) = ({
            let mut sessions = self.fishing_sessions.write().await;
            sessions
                .get_mut(player_id)
                .filter(|session| session.session_id == sid)
                .and_then(|session| session.rolled_fish.take())
        }) else {
            // Bite phase always has a rolled fish; a missing one means the
            // session raced an abort — treat it as gone.
            return;
        };

        // Fish, junk, and sealed coin pouches all go into the bag.
        let caught_id = if fish.trophy {
            format!("trophy_{}", fish.item_def_id)
        } else {
            fish.item_def_id.clone()
        };
        self.award_item(player_id, &caught_id).await;
        self.grant_fishing_catch_title(player_id, &fish.item_def_id, auth)
            .await;
        self.end_fishing(
            player_id,
            FishingOutcome::Caught {
                item_def_id: caught_id,
                size_cm: fish.size_cm,
                trophy: fish.trophy,
            },
        )
        .await;
    }

    async fn end_fishing(&self, player_id: &PlayerId, outcome: FishingOutcome) {
        self.end_fishing_if(player_id, None, outcome).await
    }

    /// `expected_session` guards tick-driven endings: a session cancelled and
    /// re-cast between the tick's scan and this call is left alone.
    async fn end_fishing_if(
        &self,
        player_id: &PlayerId,
        expected_session: Option<u64>,
        outcome: FishingOutcome,
    ) {
        let session = {
            let mut sessions = self.fishing_sessions.write().await;
            match sessions.entry(*player_id) {
                std::collections::hash_map::Entry::Occupied(e)
                    if expected_session.is_none_or(|id| e.get().session_id == id) =>
                {
                    Some(e.remove())
                }
                _ => None,
            }
        };
        let Some(session) = session else {
            return;
        };
        self.fishing_active
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
        if matches!(outcome, FishingOutcome::Caught { .. }) {
            let observers = {
                let players = self.players.read().await;
                session
                    .observers
                    .into_iter()
                    .filter(|id| {
                        players.get(player_id).is_some_and(|angler| {
                            angler.is_official_npc
                                && angler.name == "Tobin"
                                && angler.health > 0
                                && players
                                    .get(id)
                                    .is_some_and(|observer| can_watch_fishing(observer, angler))
                        })
                    })
                    .collect::<Vec<_>>()
            };
            for observer in observers {
                if self.learn_skill(&observer, SkillId::Fishing).await {
                    self.send_system_message(
                        &observer,
                        "You learned Fishing by watching Tobin! Equip a fishing rod and cast into water.",
                    ).await;
                }
            }
        }
        self.broadcast_fishing(
            &session.bobber,
            ServerMessage::FishingEnded {
                player_id: *player_id,
                outcome,
            },
        )
        .await;
    }

    async fn broadcast_fishing(&self, _bobber: &Position, msg: ServerMessage) {
        self.interest_lock().publish_fishing(msg);
    }

    async fn send_fishing_error(&self, player_id: &PlayerId, message: &str) {
        self.send_direct_message(
            player_id,
            ServerMessage::FishingError {
                message: message.to_string(),
            },
        )
        .await;
    }
}

enum Verdict {
    Hooked,
    Escaped,
}

fn rarity_of(rolled: &Option<RolledFish>) -> u32 {
    rolled.as_ref().map_or(1, |f| f.rarity)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::fishing::auto_stance;

    /// Fixed rolls make the pure fight step fully deterministic.
    const ROLLS: FightRolls = FightRolls {
        next_run_ms: 2_000.0,
        next_rest_ms: 1_500.0,
        wander_rad: 0.0,
    };

    fn fresh_fight(rarity: u32) -> FightState {
        FightState {
            stance: FishingAction::Hold,
            fish_state: FishState::Running,
            state_ms_left: 2_000.0,
            tension: TENSION_INITIAL,
            trophy: false,
            pressure_established: false,
            loose_ms: 0.0,
            stamina: stamina_max(rarity),
            distance: 6.0,
            min_distance: MIN_FISH_DISTANCE_M,
            dir_x: 1.0,
            dir_z: 0.0,
            elapsed_ms: 0.0,
        }
    }

    /// Step at the real 250 ms cadence until the fight ends.
    fn run_fight(
        f: &mut FightState,
        rarity: u32,
        stance_for: impl Fn(&FightState) -> FishingAction,
    ) -> (FightOutcome, u32) {
        for tick in 0..1_000 {
            f.stance = stance_for(f);
            if let Some(outcome) = step_fight(f, 0.25, rarity, &mut || ROLLS) {
                return (outcome, tick);
            }
        }
        panic!("fight never ended");
    }

    #[test]
    fn reel_only_play_snaps_the_line() {
        let mut f = fresh_fight(1);
        let (outcome, ticks) = run_fight(&mut f, 1, |_| FishingAction::Reel);
        assert_eq!(outcome, FightOutcome::Snapped);
        assert!(ticks < 60, "cranking against a running fish snaps fast");
    }

    #[test]
    fn slack_line_stalling_never_tires_the_fish_and_times_out() {
        let mut f = fresh_fight(1);
        let (outcome, _) = run_fight(&mut f, 1, |_| FishingAction::GiveLine);
        assert_eq!(outcome, FightOutcome::ThrewHook);
        assert!(
            f.stamina > stamina_max(1) * 0.8,
            "a slack line must not tire the fish (stamina {})",
            f.stamina
        );
    }

    #[test]
    fn ignoring_the_rod_escapes_one_way_or_another() {
        let mut f = fresh_fight(3);
        let (outcome, _) = run_fight(&mut f, 3, |_| FishingAction::Hold);
        assert!(
            matches!(outcome, FightOutcome::Snapped | FightOutcome::ThrewHook),
            "hands-off play must never land a fish, got {outcome:?}"
        );
    }

    #[test]
    fn managed_tension_exhausts_and_lands_even_a_legend() {
        let mut f = fresh_fight(5);
        let (outcome, ticks) = run_fight(&mut f, 5, |f| {
            auto_stance(f.fish_state, f.tension.round() as u32, f.trophy)
        });
        assert_eq!(outcome, FightOutcome::Landed);
        assert!(
            (ticks as f32) * 250.0 < FIGHT_TIMEOUT_MS as f32,
            "the sound policy must land inside the timeout"
        );
        // And the same policy handles a common fish faster.
        let mut c = fresh_fight(1);
        let (outcome, common_ticks) = run_fight(&mut c, 1, |f| {
            auto_stance(f.fish_state, f.tension.round() as u32, f.trophy)
        });
        assert_eq!(outcome, FightOutcome::Landed);
        assert!(common_ticks < ticks, "commons tire before legends");
    }

    /// The agent-client answers the gauge on a human reaction delay
    /// (`STANCE_REACTION_MS`, redrawn per answer) with one answer in flight,
    /// so its stance is always a beat or two stale. Commons and mid fish must
    /// still land always, a legend nearly always. Widening that range puts a
    /// third tick between gauge and hand, and legends stop landing at all.
    #[test]
    fn the_stance_policy_survives_a_human_reaction_delay() {
        const FIGHTS: u32 = 200;
        for rarity in [1u32, 3, 5] {
            for rtt_ms in [0u64, 50, 120] {
                let landed = (0..FIGHTS)
                    .filter(|i| lagged_fight_lands(rarity, rtt_ms, u64::from(*i), false))
                    .count() as u32;
                let floor = if rarity == 5 { FIGHTS * 9 / 10 } else { FIGHTS };
                assert!(
                    landed >= floor,
                    "rarity {rarity} at {rtt_ms}ms rtt landed {landed}/{FIGHTS}, wanted {floor}"
                );
            }
        }
    }

    /// One fight played by `auto_stance` reacting `STANCE_REACTION_MS + rtt`
    /// late, one answer in flight at a time. Seeded per fight so the whole
    /// sweep is deterministic.
    fn lagged_fight_lands(rarity: u32, rtt_ms: u64, seed: u64, trophy: bool) -> bool {
        use onlinerpg_shared::fishing::STANCE_REACTION_MS;
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let mut lag_rng = rand::rngs::StdRng::seed_from_u64(seed + 1_000);
        let mut f = fresh_fight(rarity);
        f.trophy = trophy;
        let mut in_flight: Option<(u32, FishingAction)> = None;
        for tick in 0..1_000u32 {
            match in_flight {
                Some((at, action)) if tick >= at => {
                    f.stance = action;
                    in_flight = None;
                }
                Some(_) => {}
                None => {
                    let want = auto_stance(f.fish_state, f.tension.round() as u32, f.trophy);
                    if want != f.stance {
                        let late = lag_rng.gen_range(STANCE_REACTION_MS) + rtt_ms;
                        in_flight = Some((tick + late.div_ceil(250) as u32, want));
                    }
                }
            }
            if let Some(o) = step_fight(&mut f, 0.25, rarity, &mut || roll_fight(rarity, &mut rng))
            {
                if trophy {
                    assert_ne!(o, FightOutcome::Snapped, "the policy must control tension");
                }
                return o == FightOutcome::Landed;
            }
        }
        panic!("fight never ended");
    }

    #[test]
    fn loose_hook_rolls_once_per_second_and_can_survive_multiple_rolls() {
        let mut f = fresh_fight(3);
        f.trophy = true;
        f.pressure_established = true;
        let mut rolls = [0.9, TROPHY_HOOK_SLIP_CHANCE, 0.0].into_iter();
        let mut checks = 0;
        for tick in 1..=12 {
            let outcome = step_hook_slip(&mut f, 0.25, &mut || {
                checks += 1;
                rolls.next().unwrap()
            });
            assert_eq!(checks, tick / 4);
            assert_eq!(
                outcome,
                if tick == 12 {
                    Some(FightOutcome::ThrewHook)
                } else {
                    None
                }
            );
        }
    }

    #[test]
    fn loose_hook_timer_exempts_buildup_rest_and_exhaustion_and_resets_on_pressure() {
        let mut f = fresh_fight(3);
        let mut never_roll = || panic!("no hook check expected");
        step_hook_slip(&mut f, 5.0, &mut never_roll);
        f.trophy = true;
        step_hook_slip(&mut f, 5.0, &mut never_roll);
        f.tension = TROPHY_MIN_TENSION;
        step_hook_slip(&mut f, 0.25, &mut never_roll);
        assert!(f.pressure_established);
        for state in [FishState::Running, FishState::Resting, FishState::Exhausted] {
            f.fish_state = FishState::Running;
            f.tension = 70.0;
            step_hook_slip(&mut f, 0.75, &mut never_roll);
            f.fish_state = state;
            if state == FishState::Running {
                f.tension = TROPHY_MIN_TENSION;
            }
            step_hook_slip(&mut f, 0.25, &mut never_roll);
            assert_eq!(f.loose_ms, 0.0);
        }
        f.fish_state = FishState::Running;
        f.stamina = 0.0;
        step_hook_slip(&mut f, 5.0, &mut never_roll);
    }

    #[test]
    fn trophies_require_high_tension_to_tire() {
        let mut f = fresh_fight(3);
        f.trophy = true;
        f.tension = 60.0;
        let full = f.stamina;
        step_fight(&mut f, 0.25, 3, &mut || ROLLS);
        assert_eq!(f.stamina, full);
        f.tension = TROPHY_MIN_TENSION;
        step_fight(&mut f, 0.25, 3, &mut || ROLLS);
        assert!(f.stamina < full);
        f.fish_state = FishState::Resting;
        f.state_ms_left = 1_000.0;
        let remaining = f.stamina;
        step_fight(&mut f, 0.25, 3, &mut || ROLLS);
        assert_eq!(f.stamina, remaining);
    }

    #[test]
    fn trophies_time_out_at_forty_seconds_and_ordinary_fish_at_sixty() {
        for (trophy, deadline) in [(true, 40_000.0), (false, 60_000.0)] {
            let mut f = fresh_fight(3);
            f.trophy = trophy;
            f.elapsed_ms = deadline - 500.0;
            f.stance = FishingAction::GiveLine;
            assert_eq!(step_fight(&mut f, 0.25, 3, &mut || ROLLS), None);
            assert_eq!(
                step_fight(&mut f, 0.25, 3, &mut || ROLLS),
                Some(FightOutcome::ThrewHook)
            );
        }
    }

    #[test]
    fn trophy_policy_survives_human_reaction_delay() {
        for rarity in 1..=5 {
            for rtt in [0, 50, 120] {
                let landed = (0..200)
                    .filter(|seed| lagged_fight_lands(rarity, rtt, *seed, true))
                    .count();
                assert!(
                    landed >= 150,
                    "trophy rarity {rarity}, rtt {rtt}: {landed}/200 landed"
                );
            }
        }
    }

    #[test]
    fn a_lively_fish_panics_out_of_landing_range() {
        let mut f = fresh_fight(2);
        f.fish_state = FishState::Resting;
        f.distance = 1.0;
        f.stance = FishingAction::Reel;
        let outcome = step_fight(&mut f, 0.25, 2, &mut || ROLLS);
        assert_eq!(outcome, None, "a fish with stamina left is never landed");
        assert_eq!(f.fish_state, FishState::Running, "it panics into a run");
    }

    #[test]
    fn the_wander_disc_clamps_the_bobber_and_line() {
        let player = Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let cast = Position {
            x: 6.0,
            y: 1.5,
            z: 0.0,
        };
        let mut f = fresh_fight(1);
        f.distance = 40.0;
        let pos = fight_fish_pos(&mut f, &player, &cast, 0.0);
        let off = ((pos.x - cast.x).powi(2) + (pos.z - cast.z).powi(2)).sqrt();
        assert!(
            off <= FISH_WANDER_RADIUS_M + 0.01,
            "fish stays near the cast"
        );
        assert_eq!(pos.y, cast.y, "the bobber keeps the water surface height");
        assert!(
            (f.distance - 12.0).abs() < 0.01,
            "distance re-derived from the clamp"
        );
    }

    #[test]
    fn the_waterline_floor_holds_and_still_lands() {
        // A shore cast measured its waterline 4 m out: the fish can never be
        // reeled past it, and landing happens right at that floor.
        let mut f = fresh_fight(1);
        f.min_distance = 4.0;
        let (outcome, _) = run_fight(&mut f, 1, |f| {
            auto_stance(f.fish_state, f.tension.round() as u32, f.trophy)
        });
        assert_eq!(outcome, FightOutcome::Landed);
        assert!(
            f.distance >= 4.0 - 0.01,
            "the float must stop at the waterline, got {}",
            f.distance
        );
    }

    #[test]
    fn the_exhausted_reel_in_steers_back_to_the_cast_ray() {
        let player = Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        };
        let cast = Position {
            x: 6.0,
            y: 1.5,
            z: 0.0,
        };
        // Fish wandered 90° off the cast ray; repeated steered steps must
        // swing its heading back toward the cast direction (+x).
        let mut f = fresh_fight(1);
        f.dir_x = 0.0;
        f.dir_z = 1.0;
        for _ in 0..30 {
            fight_fish_pos(&mut f, &player, &cast, EXHAUSTED_STEER_PER_TICK);
        }
        assert!(
            f.dir_x > 0.95,
            "heading must converge to the cast ray, got ({}, {})",
            f.dir_x,
            f.dir_z
        );
    }

    fn candidate(id: &str, rarity: u32, catch_weight: u32) -> CatchCandidate {
        CatchCandidate {
            item_def_id: id.into(),
            rarity,
            catch_weight,
        }
    }

    #[test]
    fn species_keep_their_relative_weights() {
        let table = vec![
            candidate("raw_minnow", 1, 130),
            candidate("golden_sturgeon", 5, 5),
        ];
        assert_eq!(effective_weights(&table), vec![130, 5]);
    }

    #[test]
    fn flotsam_holds_a_fixed_share() {
        let table = vec![candidate("raw_minnow", 1, 130), candidate("old_boot", 0, 6)];
        let weights = effective_weights(&table);
        let total: u64 = weights.iter().sum();
        assert_eq!(weights[1] * 100, total * FLOTSAM_SHARE_PCT);
    }

    #[test]
    fn pick_walks_cumulative_weights() {
        let w = vec![50, 1];
        assert_eq!(pick_catch(&w, 0), Some(0));
        assert_eq!(pick_catch(&w, 49), Some(0));
        assert_eq!(pick_catch(&w, 50), Some(1));
        // Out-of-range roll (caller bug) picks nothing rather than panicking.
        assert_eq!(pick_catch(&w, 51), None);
    }

    #[test]
    fn wait_stays_in_the_shared_range() {
        let mut rng = rand::thread_rng();
        for _ in 0..200 {
            let wait = roll_wait_ms(&mut rng);
            assert!((u64::from(WAIT_MIN_MS)..=u64::from(WAIT_MAX_MS)).contains(&wait));
        }
    }
}
