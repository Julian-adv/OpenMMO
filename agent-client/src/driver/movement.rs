//! Schedule transitions and housing-data prefetch for NPC walking.

#[cfg(test)]
#[path = "schedule_movement_tests.rs"]
mod schedule_tests;

use std::collections::HashSet;
use std::sync::Arc;

use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::messages::{
    MoveStatus, PatrolMuster, WATCH_CALL_BACK_WAIT_SECS, WATCH_ROLL_CALL_M,
};
use onlinerpg_shared::ClientMessage;
use onlinerpg_terrain::coords::{tile_to_region, world_to_tile};
use rand::Rng;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::walk;
use crate::state::{SharedState, WatchCall};
use crate::terrain_http::http_client;
use onlinerpg_shared::schedule::{ScheduleCondition, ScheduleEntry};

use onlinerpg_shared::schedule::resolve_active_schedule;

pub(super) enum MoveResult {
    Arrived,
    Blocked(walk::LostReason),
    Died,
    Error,
}

pub(super) async fn resolve_due_schedule(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
) -> (Option<usize>, Option<u32>) {
    let s = state.lock().await;
    let (period, game_hour, game_minute, dark_day) = s.time_context();
    let due = resolve_active_schedule(schedule, period, game_hour, game_minute, dark_day);
    if let Some(entry) = due.0.map(|i| &schedule[i]).filter(|e| e.shelter_from_rain) {
        if s.weather.rain_at(entry.pos) > 0.02 {
            if let Some(i) = schedule
                .iter()
                .position(|e| e.condition == Some(ScheduleCondition::Rain))
            {
                return (Some(i), None);
            }
        }
    }
    due
}

pub(super) async fn check_schedule_transition(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
    current: (Option<usize>, Option<u32>),
    new: (Option<usize>, Option<u32>),
    label: &str,
) -> (Option<usize>, Option<u32>) {
    let routine_paused = {
        let s = state.lock().await;
        s.player_trade_in_progress() || s.schedule_combat_active()
    };
    if routine_paused {
        return current;
    }
    if new != current {
        let meeting = new
            .0
            .map(|i| &schedule[i])
            .filter(|e| e.condition == Some(ScheduleCondition::Meeting));
        stop_current_entry(state, schedule, current.0, label).await;
        if let Some(entry) = meeting {
            state.lock().await.enter_meeting(entry.host);
        }
        if let Some(i) = new.0 {
            let entry = &schedule[i];
            info!(
                "[{label}] Schedule transition: moving to {}",
                entry.display_label()
            );
            // The schedule outranks a follow, and two walkers on one body
            // would only fight.
            if let Some(name) = state.lock().await.cancel_follow() {
                info!("[{label}] Follow of {name} cancelled by a schedule transition");
            }
            execute_schedule_move(state, entry).await;
            if let Some(cue) = &entry.cue {
                state
                    .lock()
                    .await
                    .push_ambient_event(format!("[Duty] {cue}"));
            }
            // Nothing else wakes an NPC at the meeting; the idle poll is an hour away.
            if meeting.is_some() {
                state
                    .lock()
                    .await
                    .push_ambient_event(super::prompt::meeting_arrival_event());
            }
        }
    }
    new
}

pub(super) async fn stop_current_entry(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
    current: Option<usize>,
    label: &str,
) {
    let mut s = state.lock().await;
    if current.is_some_and(|i| schedule[i].is_fishing()) {
        if let Err(e) = s.send_command(ClientMessage::FishingStop).await {
            error!("[{label}] Failed to stop scheduled fishing: {e}");
        }
    }
    if current.is_some_and(|i| schedule[i].action.is_some())
        || s.self_player
            .as_ref()
            .is_some_and(|p| p.object_type.as_deref() == Some(crate::state::MUSIC_EMOTE))
    {
        if let Err(e) = s.send_command(ClientMessage::StopInteraction).await {
            error!("[{label}] Failed to send StopInteraction: {e}");
        }
    }
    s.pack_up_placeables(label).await;
}

async fn send_interact_if_needed(s: &mut SharedState, entry: &ScheduleEntry) {
    if s.schedule_combat_active() {
        return;
    }
    if let Some(position) = entry.fishing_target() {
        if !s.can_start_scheduled_fishing() {
            return;
        }
        if let Err(e) = s
            .send_command(ClientMessage::FishingCast { position })
            .await
        {
            error!("Failed to start scheduled fishing: {e}");
        }
        return;
    }
    if let (Some(ref object_type), Some(object_id)) = (&entry.action, entry.object_id) {
        debug!("Sending InteractObject: {object_type} (id={object_id})");
        let cmd = ClientMessage::InteractObject {
            object_type: object_type.clone(),
            object_id,
        };
        if let Err(e) = s.send_command(cmd).await {
            error!("Failed to send InteractObject: {e}");
        }
    }
}

/// How far (m) from its post an NPC still counts as standing there, and the turn (rad) not worth correcting.
const POST_RANGE_M: f32 = 0.75;
const POST_TURN_SLACK: f32 = 0.05;

/// The turn `entry` still owes: the server has been seen to lose the one sent on arrival, leaving the NPC
/// facing the way it walked in.
pub(super) fn post_facing_owed(s: &SharedState, entry: &ScheduleEntry) -> Option<f32> {
    let me = s.self_player.as_ref()?;
    let wanted = entry.rotation.to_radians();
    let standing_post = entry.action.is_none()
        && entry.patrol.is_none()
        && entry.waypoints.is_empty()
        && !s.player_trade_in_progress()
        && !s.schedule_combat_active()
        && !s.teleport_pending()
        && !matches!(
            s.move_status,
            Some(MoveStatus::Moving | MoveStatus::Searching)
        )
        && me.health > 0
        && !me.petrified
        && me.mount.is_none()
        && crate::geom::PlanarDelta::to_xz(&me.position, entry.pos[0], entry.pos[2]).dist
            <= POST_RANGE_M;
    (standing_post
        && onlinerpg_shared::mount_movement::angle_delta(me.rotation, wanted).abs()
            > POST_TURN_SLACK)
        .then_some(wanted)
}

pub(super) async fn keep_post_facing(
    state: &Arc<Mutex<SharedState>>,
    entry: &ScheduleEntry,
    label: &str,
) {
    let mut s = state.lock().await;
    if let Some(rotation) = post_facing_owed(&s, entry) {
        info!("[{label}] Turning back to the post's facing");
        if let Err(e) = s
            .send_background_command(ClientMessage::PlayerFace { rotation })
            .await
        {
            error!("Failed to face schedule target: {e}");
        }
    }
}

pub(super) async fn maintain_scheduled_fishing(
    state: &Arc<Mutex<SharedState>>,
    entry: &ScheduleEntry,
) {
    let needs_cast = {
        let s = state.lock().await;
        s.can_start_scheduled_fishing()
    };
    if needs_cast {
        execute_schedule_move(state, entry).await;
    }
}

const PARTNER_LAG_M: f32 = 6.0;
const PARTNER_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// Hold while the partner trails us toward `(x, z)`; whoever leads waits.
/// On the way out the wait lasts as long as the partner is in sight, and is
/// false once they have been out of sight for `PARTNER_WAIT`.
async fn wait_for_partner(
    state: &Arc<Mutex<SharedState>>,
    partner: &str,
    [x, z]: [f32; 2],
    outbound: bool,
) -> bool {
    let mut deadline = tokio::time::Instant::now() + PARTNER_WAIT;
    loop {
        match state.lock().await.lag_of_named(partner, x, z) {
            Some(lag) if lag <= PARTNER_LAG_M => return true,
            Some(_) if outbound => deadline = tokio::time::Instant::now() + PARTNER_WAIT,
            Some(_) => {}
            None if outbound => {}
            None => return true,
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Stops on a lap: each waypoint out, back through all but the last, then home.
fn lap_stops(waypoints: usize) -> usize {
    (2 * waypoints).max(1)
}

fn lap_stop(entry: &ScheduleEntry, stop: usize) -> [f32; 3] {
    let n = entry.waypoints.len();
    if stop < n {
        entry.waypoints[stop]
    } else if stop + 1 < lap_stops(n) {
        entry.waypoints[2 * n - 2 - stop]
    } else {
        entry.pos
    }
}

/// The stop that heads home from outbound `stop` without reaching it.
fn turn_back(stop: usize, waypoints: usize) -> usize {
    2 * waypoints - 1 - stop
}

const STRIKE_STANDOFF_M: f32 = 1.3;
const STRIKES: u32 = 3;
const STRIKE_EVERY: std::time::Duration = std::time::Duration::from_millis(1100);
/// Longer than any walk-up and strike, so an aborted lap still lets go of the weapon.
const STRIKE_HOLD: std::time::Duration = std::time::Duration::from_secs(40);
/// How far apart a guard stops to shovel a snowy road.
const SHOVEL_EVERY_M: f32 = 10.0;
/// How long one bout of shovelling takes; the `snow_dig` clip runs this long.
const SHOVEL_BOUT: std::time::Duration = std::time::Duration::from_millis(4900);

async fn send_patrol(state: &Arc<Mutex<SharedState>>, msg: ClientMessage) {
    if let Err(e) = state.lock().await.send_background_command(msg).await {
        error!("Failed to send patrol command: {e}");
    }
}

/// Blocked walks in a row before a guard asks to be brought back to town.
const RECALL_AFTER_BLOCKED: u32 = 3;

/// Ask for the fences penning this guard in to come down, or with `recall`
/// to be brought back to town. True when either happened.
async fn break_out(state: &Arc<Mutex<SharedState>>, recall: bool) -> bool {
    let seen = state.lock().await.patrol_breakouts;
    send_patrol(state, ClientMessage::PatrolBreakOut { recall }).await;
    super::execute::wait_for_answer(state, seen, |s| s.patrol_breakouts).await;
    let s = state.lock().await;
    s.patrol_breakouts != seen && s.patrol_broke_out
}

/// Scout the road here; the answer carries any warnings. A fence due to come
/// down is walked up to and struck, then a sweep clears every such fence around.
async fn clear_road(state: &Arc<Mutex<SharedState>>, patrol_id: i64, floor: u8) {
    let seen = state.lock().await.patrol_scouts;
    send_patrol(state, ClientMessage::PatrolScout).await;
    super::execute::wait_for_answer(state, seen, |s| s.patrol_scouts).await;
    let fence = {
        let mut s = state.lock().await;
        let fence = (s.patrol_scouts != seen)
            .then(|| s.patrol_target.take())
            .flatten();
        if fence.is_some() {
            s.patrol_strike_until = Some(tokio::time::Instant::now() + STRIKE_HOLD);
        }
        fence
    };
    let Some(fence) = fence else {
        return;
    };
    super::combat::walk_to_point(state, fence, floor as i8, STRIKE_STANDOFF_M, Some(false)).await;
    let (face, draw) = {
        let s = state.lock().await;
        (s.face_position_command(fence), s.weapon_command(true))
    };
    for msg in face.into_iter().chain(draw) {
        send_patrol(state, msg).await;
    }
    for _ in 0..STRIKES {
        send_patrol(state, ClientMessage::PatrolStrike).await;
        tokio::time::sleep(STRIKE_EVERY).await;
    }
    send_patrol(state, ClientMessage::PatrolSweep { patrol_id }).await;
    state.lock().await.patrol_strike_until = None;
}

/// Shovel the road here if snow lies on it, standing still for the bout.
/// Returns whether there was snow to clear.
async fn shovel_snow(state: &Arc<Mutex<SharedState>>) -> bool {
    let seen = state.lock().await.patrol_shovels;
    send_patrol(state, ClientMessage::PatrolShovel).await;
    super::execute::wait_for_answer(state, seen, |s| s.patrol_shovels).await;
    let cleared = {
        let s = state.lock().await;
        s.patrol_shovels != seen && s.patrol_shovelled
    };
    if cleared {
        tokio::time::sleep(SHOVEL_BOUT).await;
    }
    cleared
}

/// Shovel here and, while the snow holds, at stops `SHOVEL_EVERY_M` apart on
/// the way to `(x, z)`, ending one stop short of it.
async fn shovel_toward(state: &Arc<Mutex<SharedState>>, x: f32, z: f32, floor: u8) {
    while shovel_snow(state).await {
        let Some(here) = state
            .lock()
            .await
            .self_player
            .as_ref()
            .map(|me| me.position)
        else {
            return;
        };
        let (dx, dz) = (x - here.x, z - here.z);
        let left = dx.hypot(dz);
        if left <= SHOVEL_EVERY_M * 1.5 {
            return;
        }
        let stop = walk::WalkTo::Schedule {
            x: here.x + dx / left * SHOVEL_EVERY_M,
            z: here.z + dz / left * SHOVEL_EVERY_M,
            floor,
        };
        if !matches!(
            execute_walk(state, &stop, Some(false)).await,
            MoveResult::Arrived
        ) {
            return;
        }
    }
}

/// The watch has no time for another lap: report home at the stand-down
/// spot instead of waiting at the road head. The guard is off duty after.
pub(super) async fn stand_down(state: Arc<Mutex<SharedState>>, entry: ScheduleEntry) {
    let Some(spot) = entry.patrol.as_ref().and_then(|patrol| patrol.stand_down) else {
        return;
    };
    let to = walk::WalkTo::Schedule {
        x: spot[0],
        z: spot[2],
        floor: entry.floor_level,
    };
    if !matches!(
        execute_walk(&state, &to, Some(false)).await,
        MoveResult::Arrived
    ) {
        return;
    }
    state.lock().await.push_ambient_event(
        "[Duty] Your watch has walked its last lap and you stand at Captain Karl's post. If the \
         Captain is in sight, report the watch home to him aloud in one short line. With that \
         your duty is done for the day and you are off duty."
            .to_string(),
    );
}

/// How long an off-duty wanderer stands between walks, in seconds.
const ROAM_PAUSE_SECS: std::ops::Range<u64> = 15..50;

/// Stand a while, then stroll to one of the entry's spots picked at random.
pub(super) async fn roam_once(state: Arc<Mutex<SharedState>>, entry: ScheduleEntry) {
    let (pause, spot) = {
        let mut rng = rand::thread_rng();
        let spot = entry.waypoints[rng.gen_range(0..entry.waypoints.len())];
        (rng.gen_range(ROAM_PAUSE_SECS), spot)
    };
    tokio::time::sleep(std::time::Duration::from_secs(pause)).await;
    let to = walk::WalkTo::Schedule {
        x: spot[0],
        z: spot[2],
        floor: entry.floor_level,
    };
    execute_walk(&state, &to, Some(false)).await;
}

/// Ask the server for the roll and have the sergeant remark on recruits who
/// came back. Returns those out of reach whom he is to wait for.
async fn ask_roll(
    state: &Arc<Mutex<SharedState>>,
    muster: PatrolMuster,
    patrol_id: i64,
) -> Vec<String> {
    let seen = state.lock().await.patrol_roll_calls;
    send_patrol(state, ClientMessage::PatrolRollCall { muster, patrol_id }).await;
    super::execute::wait_for_answer(state, seen, |s| s.patrol_roll_calls).await;
    let mut s = state.lock().await;
    if s.patrol_roll_calls == seen {
        return Vec::new();
    }
    let late = std::mem::take(&mut s.patrol_late).join(", ");
    if !late.is_empty() {
        s.push_ambient_event(format!(
            "[Recruit] {late} is back beside your watch only now, after missing roll calls. Say so \
             aloud by name in one short, stern line in the language they speak, and that it comes \
             off their pay, such as \"이제서야 복귀하다니. 일당은 깎일 줄 알아라.\""
        ));
    }
    let rejoined = std::mem::take(&mut s.patrol_rejoined).join(", ");
    if !rejoined.is_empty() {
        s.push_ambient_event(format!(
            "[Recruit] {rejoined} came back when you called, in time. Acknowledge it aloud by name \
             in one short, gruff line in the language they speak, and tell them to keep up."
        ));
    }
    std::mem::take(&mut s.patrol_missing)
}

/// Call the roll at a stop. A recruit out of reach is called back by name
/// and waited for, then the roll is asked again.
async fn call_roll(state: &Arc<Mutex<SharedState>>, patrol_id: i64) {
    let missing = ask_roll(state, PatrolMuster::Stop, patrol_id).await;
    if missing.is_empty() {
        return;
    }
    let names = missing.join(", ");
    state.lock().await.push_ambient_event(format!(
        "[Recruit] {names} has fallen more than {WATCH_ROLL_CALL_M:.0} paces behind your watch. Order them back \
         aloud right now, by name, in one short line in the language they speak, such as \
         \"훈련병 {names}, 어서 복귀하시오.\" You are standing and waiting a few seconds for them."
    ));
    tokio::time::sleep(std::time::Duration::from_secs(WATCH_CALL_BACK_WAIT_SECS)).await;
    ask_roll(state, PatrolMuster::Recheck, patrol_id).await;
}

/// Call the day recruits home once the watch has walked its last lap.
pub(super) async fn call_watch_home(state: &Arc<Mutex<SharedState>>) {
    let patrol_id = {
        let mut s = state.lock().await;
        if s.patrol_call != WatchCall::Out {
            return;
        }
        s.patrol_call = WatchCall::Home;
        s.patrol_lap_id
    };
    let muster = PatrolMuster::Home;
    send_patrol(state, ClientMessage::PatrolRollCall { muster, patrol_id }).await;
}

/// One road-patrol lap: out along the entry's waypoints and back to `pos`.
/// Resumes at `SharedState::patrol_reached`.
pub(super) async fn run_patrol_lap(state: Arc<Mutex<SharedState>>, entry: ScheduleEntry) {
    let Some(patrol) = &entry.patrol else {
        return;
    };
    let start = state.lock().await.patrol_reached;
    let patrol_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64);
    let outbound_stops = entry.waypoints.len();
    let mut stop = start;
    if patrol.sweeps {
        let muster = {
            let mut s = state.lock().await;
            s.patrol_lap_id = patrol_id;
            let out = s.patrol_call == WatchCall::NotOut;
            if out {
                s.patrol_call = WatchCall::Out;
            }
            out.then_some(PatrolMuster::Out)
        };
        if let Some(muster) = muster {
            send_patrol(&state, ClientMessage::PatrolRollCall { muster, patrol_id }).await;
        }
    }
    'lap: while stop < lap_stops(outbound_stops) {
        while state.lock().await.schedule_combat_active() {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
        let wp = lap_stop(&entry, stop);
        let outbound = stop < outbound_stops;
        let mut go_on = match &patrol.partner {
            Some(partner) => {
                wait_for_partner(&state, partner, [wp[0], wp[2]], outbound).await || !outbound
            }
            None => true,
        };
        if state.lock().await.is_dead() {
            break;
        }
        if !go_on {
            info!("Patrol partner fell behind; turning back");
        } else {
            let to = walk::WalkTo::Schedule {
                x: wp[0],
                z: wp[2],
                floor: entry.floor_level,
            };
            go_on = false;
            for attempt in 0..3 {
                if patrol.sweeps {
                    clear_road(&state, patrol_id, entry.floor_level).await;
                }
                if attempt == 0 {
                    shovel_toward(&state, wp[0], wp[2], entry.floor_level).await;
                }
                match execute_walk(&state, &to, Some(false)).await {
                    MoveResult::Arrived => {
                        go_on = true;
                        state.lock().await.patrol_blocked = 0;
                        if patrol.sweeps {
                            call_roll(&state, patrol_id).await;
                        }
                        break;
                    }
                    MoveResult::Died => break 'lap,
                    MoveResult::Blocked(_) | MoveResult::Error if attempt == 0 => {}
                    MoveResult::Blocked(_) | MoveResult::Error if attempt == 1 => {
                        let recall = {
                            let mut s = state.lock().await;
                            s.patrol_blocked += 1;
                            s.patrol_blocked >= RECALL_AFTER_BLOCKED
                        };
                        if !break_out(&state, recall).await {
                            break;
                        }
                        state.lock().await.patrol_blocked = 0;
                    }
                    MoveResult::Blocked(_) | MoveResult::Error => {}
                }
            }
            if !go_on {
                warn!("Patrol waypoint unreachable ({:.1}, {:.1})", wp[0], wp[2]);
            }
        }
        stop = if outbound && !go_on {
            turn_back(stop, outbound_stops)
        } else {
            stop + 1
        };
        state.lock().await.patrol_reached = stop;
    }
    state.lock().await.patrol_reached = 0;
}

/// Inside the dining room without a seat, free to try for one.
pub(super) fn awaits_seat(s: &SharedState, entry: &ScheduleEntry) -> bool {
    entry.seeks_seat()
        && s.own_chair().is_none()
        && s.self_player.as_ref().is_some_and(|p| p.health > 0)
        && s.shares_building(entry.pos[0], entry.pos[2], entry.floor_level)
        && !s.player_trade_in_progress()
        && !s.schedule_combat_active()
        && !matches!(
            s.move_status,
            Some(MoveStatus::Moving | MoveStatus::Searching)
        )
}

const SEAT_HOLD: std::time::Duration = std::time::Duration::from_secs(30);
const SIT_REPLY_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);

/// Sit on the nearest free dining chair, trying the next when one is lost.
pub(super) async fn take_free_seat(
    state: &Arc<Mutex<SharedState>>,
    entry: &ScheduleEntry,
    claims: &super::VisitClaims,
    who: &str,
) -> bool {
    let seats = state
        .lock()
        .await
        .free_seats(entry.pos[0], entry.pos[2], entry.floor_level);
    for chair in seats {
        let key = || super::ClaimKey::Seat(chair.id, chair.x.to_bits(), chair.z.to_bits());
        if state.lock().await.seat_taken(&chair) || !claims.try_claim(key(), who, SEAT_HOLD) {
            continue;
        }
        let seat = ScheduleEntry {
            pos: [chair.x, chair.y, chair.z],
            rotation: chair.rotation_deg,
            floor_level: chair.floor_level,
            action: Some(chair.type_id.clone()),
            object_id: Some(chair.id),
            ..Default::default()
        };
        execute_schedule_move(state, &seat).await;
        tokio::time::sleep(SIT_REPLY_WAIT).await;
        if state.lock().await.own_chair() == Some(chair.id) {
            info!("[{who}] Took free seat {}", chair.id);
            return true;
        }
        claims.release(&key(), who);
    }
    false
}

pub(super) async fn execute_schedule_move(state: &Arc<Mutex<SharedState>>, entry: &ScheduleEntry) {
    // Already inside: `take_free_seat` goes straight to a chair.
    if entry.seeks_seat()
        && state
            .lock()
            .await
            .shares_building(entry.pos[0], entry.pos[2], entry.floor_level)
    {
        return;
    }
    let waypoints: &[[f32; 3]] = if entry.patrol.is_some() || entry.roam {
        &[]
    } else {
        &entry.waypoints
    };
    for (i, wp) in waypoints.iter().enumerate() {
        let (wx, wz) = (wp[0], wp[2]);
        debug!(
            "Patrol waypoint {}/{}: ({:.1}, {:.1})",
            i + 1,
            waypoints.len(),
            wx,
            wz
        );
        let to = walk::WalkTo::Schedule {
            x: wx,
            z: wz,
            floor: entry.floor_level,
        };
        match execute_walk(state, &to, Some(false)).await {
            MoveResult::Arrived => {
                for activity in entry.waypoint_activities.iter().filter(|a| a.waypoint == i) {
                    perform_waypoint_activity(state, entry, activity).await;
                }
            }
            MoveResult::Blocked(_) => {
                warn!("Patrol waypoint {i} blocked — skipping ({wx:.1}, {wz:.1})");
            }
            MoveResult::Died => return,
            MoveResult::Error => {
                error!("Patrol waypoint {i} error");
            }
        }
    }

    let to = walk::WalkTo::Schedule {
        x: entry.pos[0],
        z: entry.pos[2],
        floor: entry.floor_level,
    };
    match execute_walk(state, &to, Some(false)).await {
        MoveResult::Arrived => {}
        MoveResult::Blocked(_) => {
            warn!(
                "Schedule walk to {} blocked at ({:.1}, {:.1})",
                entry.display_label(),
                entry.pos[0],
                entry.pos[2]
            );
            if state.lock().await.protects() {
                let freed = break_out(state, false).await;
                info!("Break-out after a blocked schedule walk: freed={freed}");
            }
            return;
        }
        MoveResult::Died => return,
        MoveResult::Error => {
            error!("Schedule move error");
            return;
        }
    }

    let mut s = state.lock().await;
    if let Err(e) = s
        .send_command(ClientMessage::PlayerFace {
            rotation: entry.rotation.to_radians(),
        })
        .await
    {
        error!("Failed to face schedule target: {e}");
    }
    send_interact_if_needed(&mut s, entry).await;
}

async fn perform_waypoint_activity(
    state: &Arc<Mutex<SharedState>>,
    entry: &ScheduleEntry,
    activity: &onlinerpg_shared::schedule::WaypointActivity,
) {
    let allowed = onlinerpg_shared::messages::LOOPING_EMOTES.contains(&activity.emote.as_str())
        || activity.emote == onlinerpg_shared::messages::EXCAVATE_EMOTE;
    if !allowed || activity.duration_secs == 0 {
        return;
    }
    let Some((x, z)) = activity_destination(entry, activity, &mut rand::thread_rng()) else {
        return;
    };
    let to = walk::WalkTo::Schedule {
        x,
        z,
        floor: entry.floor_level,
    };
    if !matches!(
        execute_walk(state, &to, Some(false)).await,
        MoveResult::Arrived
    ) {
        return;
    }
    {
        let mut s = state.lock().await;
        if s.player_trade_in_progress()
            || s.schedule_combat_active()
            || s.self_player.as_ref().is_none_or(|p| {
                p.health == 0
                    || p.mount.is_some()
                    || (p.position.x - x).powi(2) + (p.position.z - z).powi(2) > 0.25
            })
        {
            return;
        }
        if s.send_background_command(ClientMessage::PlayerFace {
            rotation: activity.rotation.to_radians(),
        })
        .await
        .is_err()
            || s.send_background_command(ClientMessage::ChatMessage {
                message: format!("/emote {}", activity.emote),
            })
            .await
            .is_err()
        {
            return;
        }
    }
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(u64::from(activity.duration_secs.min(120)));
    while tokio::time::Instant::now() < deadline {
        let interrupted = {
            let s = state.lock().await;
            s.schedule_combat_active() || s.self_player.as_ref().is_none_or(|p| p.health == 0)
        };
        if interrupted {
            break;
        }
        tokio::time::sleep_until(
            deadline.min(tokio::time::Instant::now() + std::time::Duration::from_millis(50)),
        )
        .await;
    }
    let mut s = state.lock().await;
    if let Err(error) = s
        .send_background_command(ClientMessage::StopInteraction)
        .await
    {
        warn!("Failed to finish waypoint activity: {error}");
    }
}

fn activity_destination(
    entry: &ScheduleEntry,
    activity: &onlinerpg_shared::schedule::WaypointActivity,
    rng: &mut impl Rng,
) -> Option<(f32, f32)> {
    let waypoint = entry.waypoints.get(activity.waypoint)?;
    if !activity.probability.is_finite() || rng.gen::<f32>() >= activity.probability.clamp(0.0, 1.0)
    {
        return None;
    }
    let radius = if activity.radius.is_finite() {
        activity.radius.clamp(0.0, 8.0)
    } else {
        0.0
    };
    let angle = rng.gen_range(0.0..std::f32::consts::TAU);
    let distance = radius * rng.gen::<f32>().sqrt();
    Some((
        onlinerpg_shared::wrap_world_x(waypoint[0] + angle.cos() * distance),
        waypoint[2] + angle.sin() * distance,
    ))
}

pub(super) async fn execute_move(
    state: &Arc<Mutex<SharedState>>,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
    sprint: Option<bool>,
) -> MoveResult {
    let to = walk::WalkTo::Place {
        x: goal_x,
        z: goal_z,
        floor: goal_floor,
    };
    execute_walk(state, &to, sprint).await
}

async fn execute_walk(
    state: &Arc<Mutex<SharedState>>,
    to: &walk::WalkTo<'_>,
    sprint: Option<bool>,
) -> MoveResult {
    match walk::walk(state, to, false, sprint).await {
        walk::Walked::Arrived => MoveResult::Arrived,
        walk::Walked::Error => MoveResult::Error,
        walk::Walked::Lost(walk::LostReason::PlayerDied) => MoveResult::Died,
        walk::Walked::Lost(reason) => {
            let s = state.lock().await;
            let player = s.self_player.as_ref();
            warn!(
                npc = player.map(|p| p.name.as_str()),
                player_id = s.self_player_id.map(onlinerpg_shared::PlayerId::get),
                target = %to,
                ?reason,
                status = s.move_status.map(tracing::field::debug),
                x = player.map(|p| p.position.x),
                y = player.map(|p| p.position.y),
                z = player.map(|p| p.position.z),
                floor = s.self_floor_level,
                "Move did not complete"
            );
            MoveResult::Blocked(reason)
        }
    }
}

#[derive(serde::Deserialize)]
struct RegionObjects {
    #[serde(default)]
    placements: Vec<FurniturePlacement>,
}

fn insert_region(regions: &mut HashSet<(i32, i32)>, x: f32, z: f32) {
    regions.insert((
        tile_to_region(world_to_tile(x)),
        tile_to_region(world_to_tile(z)),
    ));
}

pub(super) fn coverage_positions(
    schedule: &[ScheduleEntry],
    position: Option<onlinerpg_shared::Position>,
) -> Vec<(f32, f32)> {
    if schedule.is_empty() {
        return position.map(|p| (p.x, p.z)).into_iter().collect();
    }
    schedule
        .iter()
        .flat_map(|e| {
            std::iter::once((e.pos[0], e.pos[2])).chain(e.waypoints.iter().map(|wp| (wp[0], wp[2])))
        })
        .collect()
}

pub(super) async fn fetch_furniture_around(
    world_cache: &Arc<std::sync::RwLock<crate::state::WorldCache>>,
    positions: &[(f32, f32)],
    api_base_url: &str,
    label: &str,
) {
    let mut regions = HashSet::new();
    for (x, z) in positions {
        insert_region(&mut regions, *x, *z);
    }
    let epoch = {
        let world = world_cache.read().unwrap();
        world.unfetched_furniture_regions(&mut regions);
        world.world_epoch().to_owned()
    };
    if regions.is_empty() {
        return;
    }

    let client = http_client();
    let fetches = regions.iter().map(|&(rx, rz)| {
        let client = &client;
        let url = format!("{api_base_url}/api/terrain/objects/{rx}/{rz}");
        async move {
            let resp = match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => resp.json::<RegionObjects>().await.ok(),
                _ => None,
            };
            (rx, rz, resp)
        }
    });
    let results = futures_util::future::join_all(fetches).await;

    let mut world = world_cache.write().unwrap();
    if !world.is_current_epoch(&epoch) {
        return;
    }
    let mut synced_regions = 0usize;
    for (rx, rz, resp) in results {
        let Some(resp) = resp else { continue };
        world.sync_furniture(rx, rz, resp.placements);
        world.mark_furniture_fetched((rx, rz));
        synced_regions += 1;
    }
    if synced_regions > 0 {
        debug!("[{label}] Synced furniture for {synced_regions} region(s)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lap_runs_out_back_and_home() {
        let entry = ScheduleEntry {
            pos: [9.0, 0.0, 9.0],
            waypoints: vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
            ..Default::default()
        };
        let xs: Vec<f32> = (0..lap_stops(3))
            .map(|stop| lap_stop(&entry, stop)[0])
            .collect();
        assert_eq!(xs, [1.0, 2.0, 3.0, 2.0, 1.0, 9.0]);
        assert_eq!(lap_stops(0), 1);
        assert_eq!(
            lap_stop(
                &ScheduleEntry {
                    waypoints: Vec::new(),
                    ..entry.clone()
                },
                0
            )[0],
            9.0
        );
    }

    #[test]
    fn turning_back_heads_for_the_last_stop_reached() {
        let entry = ScheduleEntry {
            pos: [9.0, 0.0, 9.0],
            waypoints: vec![[1.0, 0.0, 0.0], [2.0, 0.0, 0.0], [3.0, 0.0, 0.0]],
            ..Default::default()
        };
        let next = |stop| lap_stop(&entry, turn_back(stop, 3))[0];
        assert_eq!([next(0), next(1), next(2)], [9.0, 1.0, 2.0]);
    }
    use crate::state::tests::{bag_item, test_player, test_state};
    use onlinerpg_shared::fishing::{FishState, FishingAction, FishingOutcome};
    use onlinerpg_shared::inventory::EquipSlot;
    use onlinerpg_shared::{PlayerId, Position, ServerMessage};

    fn npc_schedule(json: &str) -> Vec<ScheduleEntry> {
        #[derive(serde::Deserialize)]
        struct File {
            schedule: Vec<ScheduleEntry>,
        }
        let mut schedule = serde_json::from_str::<File>(json).unwrap().schedule;
        assert!(onlinerpg_shared::schedule::parse_conditions(&mut schedule).is_empty());
        schedule
    }

    #[test]
    fn ysolde_patrol_yields_to_campfire_and_sleep_across_midnight() {
        let schedule = npc_schedule(include_str!("../../data/npcs/ysolde/schedule.json"));
        for (hour, minute, index, recurring) in [
            (0, 0, 2, None),
            (5, 59, 2, None),
            (6, 0, 0, Some(6)),
            (7, 0, 0, Some(7)),
            (17, 59, 0, Some(17)),
            (18, 0, 1, None),
            (20, 59, 1, None),
            (21, 0, 2, None),
            (23, 59, 2, None),
        ] {
            assert_eq!(
                resolve_active_schedule(&schedule, None, Some(hour), Some(minute), None),
                (Some(index), recurring)
            );
        }
        assert!(schedule[1].is_campfire());
        assert!(!schedule[1].is_campfire_meal());
        assert!(schedule[2].is_sleeping());
    }

    fn fishing_state(
        entry: &ScheduleEntry,
    ) -> (
        Arc<Mutex<SharedState>>,
        tokio::sync::mpsc::Receiver<ClientMessage>,
    ) {
        let (mut s, mut rx) = test_state();
        let me = test_player(entry.pos[0], entry.pos[2]);
        s.self_player_id = Some(me.id);
        s.self_player = Some(me);
        s.in_game = true;
        s.self_equipped
            .insert(EquipSlot::MainHand, bag_item(1, "fishing_rod", 1));
        assert!(rx.try_recv().is_err());
        (Arc::new(Mutex::new(s)), rx)
    }

    #[tokio::test]
    async fn a_post_left_facing_the_walk_in_is_turned_back() {
        let entry = ScheduleEntry {
            pos: [-1473.3, 1.1, 4732.8],
            rotation: 0.0,
            ..Default::default()
        };
        let (mut s, mut rx) = test_state();
        let mut me = test_player(-1473.5, 4732.5);
        me.rotation = -std::f32::consts::FRAC_PI_2;
        s.self_player_id = Some(me.id);
        s.self_player = Some(me);
        s.in_game = true;
        let state = Arc::new(Mutex::new(s));

        keep_post_facing(&state, &entry, "test").await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { rotation }) if rotation == 0.0
        ));

        // The server's reply sets the facing, and nothing more is sent.
        {
            let mut s = state.lock().await;
            let me = s.self_player.as_ref().unwrap();
            let moved = ServerMessage::PlayerMoved {
                player_id: me.id,
                position: me.position,
                rotation: 0.0,
                floor_level: 0,
                sprinting: false,
            };
            s.push_event(moved);
        }
        keep_post_facing(&state, &entry, "test").await;
        assert!(rx.try_recv().is_err());

        // Away from the post, seated, or walking, the facing is not the schedule's to set.
        let mut s = state.lock().await;
        s.self_player.as_mut().unwrap().rotation = 1.0;
        let seated = ScheduleEntry {
            action: Some("chair".into()),
            ..entry.clone()
        };
        assert_eq!(post_facing_owed(&s, &seated), None);
        s.move_status = Some(MoveStatus::Moving);
        assert_eq!(post_facing_owed(&s, &entry), None);
        s.move_status = None;
        assert_eq!(post_facing_owed(&s, &entry), Some(0.0));
        s.self_player.as_mut().unwrap().position.x += 2.0;
        assert_eq!(post_facing_owed(&s, &entry), None);
    }

    #[tokio::test(start_paused = true)]
    async fn scheduled_fishing_hooks_reels_rests_and_sleeps_until_eight() {
        let schedule = npc_schedule(include_str!("../../data/npcs/tobin/schedule.json"));
        let entry = &schedule[0];
        let bed = &schedule[1];
        assert!(bed.is_sleeping());
        for (hour, minute, expected) in [
            (0, 0, 0),
            (1, 59, 0),
            (2, 0, 1),
            (7, 59, 1),
            (8, 0, 2),
            (8, 29, 2),
            (8, 30, 0),
            (18, 59, 0),
            (19, 0, 3),
            (19, 29, 3),
            (19, 30, 0),
            (23, 59, 0),
        ] {
            assert_eq!(
                resolve_active_schedule(&schedule, None, Some(hour), Some(minute), None),
                (Some(expected), None)
            );
        }
        let (state, mut rx) = fishing_state(entry);
        let player_id = PlayerId::from(1);

        maintain_scheduled_fishing(&state, entry).await;
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::PlayerFace { rotation })
                if rotation == entry.rotation.to_radians()
            )
        );
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::FishingCast { position })
                if (position.x + 1503.8994).abs() < 0.001 && (position.z - 4728.47).abs() < 0.001
            )
        );

        state.lock().await.push_event(ServerMessage::FishingCasted {
            player_id,
            position: entry.fishing_target().unwrap(),
            rotation: entry.rotation.to_radians(),
        });
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err());
        state
            .lock()
            .await
            .push_event(ServerMessage::FishingBite { player_id });
        assert!(rx.try_recv().is_err());
        assert!(matches!(
            rx.recv().await,
            Some(ClientMessage::FishingRespond {
                action: FishingAction::Hook
            })
        ));
        for (fish_state, tension_pct, expected) in [
            (FishState::Resting, 20, FishingAction::Reel),
            (FishState::Running, 90, FishingAction::GiveLine),
        ] {
            state.lock().await.push_event(ServerMessage::FishingFight {
                player_id,
                bobber: entry.fishing_target().unwrap(),
                fish_state,
                tension_pct,
                stamina_pct: 50,
                trophy: false,
                stance: FishingAction::Hold,
            });
            assert!(matches!(rx.recv().await,
                Some(ClientMessage::FishingRespond { action }) if action == expected
            ));
        }

        state.lock().await.push_event(ServerMessage::FishingEnded {
            player_id,
            outcome: FishingOutcome::Caught {
                item_def_id: "raw_minnow".into(),
                size_cm: 10,
                trophy: false,
            },
        });
        assert!(!state.lock().await.self_fishing);
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err());
        tokio::time::advance(crate::state::FISHING_RECAST_DELAY).await;
        maintain_scheduled_fishing(&state, entry).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::FishingCast { .. })
        ));

        state.lock().await.push_event(ServerMessage::FishingCasted {
            player_id,
            position: entry.fishing_target().unwrap(),
            rotation: entry.rotation.to_radians(),
        });
        state
            .lock()
            .await
            .push_event(ServerMessage::FishingBite { player_id });
        state.lock().await.self_player.as_mut().unwrap().position = Position {
            x: bed.pos[0],
            y: bed.pos[1],
            z: bed.pos[2],
        };
        let active =
            check_schedule_transition(&state, &schedule, (Some(0), None), (Some(1), None), "Tobin")
                .await;
        assert!(!state.lock().await.self_fishing);
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::FishingStop)));
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::StopInteraction)));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::InteractObject { object_type, object_id: 111 })
                if object_type == "rustic_bed"
            )
        );
        assert!(
            rx.try_recv().is_err(),
            "height sync must not wake a sleeping NPC"
        );
        tokio::time::advance(crate::state::FISHING_RECAST_DELAY).await;
        assert!(rx.try_recv().is_err(), "sleeping cancels the pending hook");

        let breakfast = &schedule[2];
        assert!(breakfast.is_campfire_meal());
        state.lock().await.self_player.as_mut().unwrap().position = Position {
            x: breakfast.pos[0],
            y: breakfast.pos[1],
            z: breakfast.pos[2],
        };
        let active =
            check_schedule_transition(&state, &schedule, active, (Some(2), None), "Tobin").await;
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::StopInteraction)));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(rx.try_recv().is_err(), "breakfast must not cast the rod");

        state.lock().await.self_player.as_mut().unwrap().position = Position {
            x: entry.pos[0],
            y: entry.pos[1],
            z: entry.pos[2],
        };
        check_schedule_transition(&state, &schedule, active, (Some(0), None), "Tobin").await;
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::StopInteraction)));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::FishingCast { .. })
        ));
    }

    #[tokio::test(start_paused = true)]
    async fn scheduled_fishing_waits_for_a_usable_rod_and_backs_off_failed_casts() {
        let schedule = npc_schedule(include_str!("../../data/npcs/tobin/schedule.json"));
        let entry = &schedule[0];
        let (state, mut rx) = fishing_state(entry);
        let rod = state
            .lock()
            .await
            .self_equipped
            .remove(&EquipSlot::MainHand)
            .unwrap();
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err());
        {
            let mut s = state.lock().await;
            s.self_equipped.insert(EquipSlot::MainHand, rod);
            s.trade_busy = true;
        }
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err());
        {
            let mut s = state.lock().await;
            s.trade_busy = false;
            s.self_player.as_mut().unwrap().health = 0;
        }
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err());
        state.lock().await.self_player.as_mut().unwrap().health = 10;

        maintain_scheduled_fishing(&state, entry).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::FishingCast { .. })
        ));
        maintain_scheduled_fishing(&state, entry).await;
        assert!(rx.try_recv().is_err(), "wait for the cast acknowledgement");
        tokio::time::advance(crate::state::FISHING_CAST_ACK_TIMEOUT).await;
        maintain_scheduled_fishing(&state, entry).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::FishingCast { .. })
        ));

        state.lock().await.push_event(ServerMessage::FishingError {
            message: "Not water".into(),
        });
        tokio::time::advance(crate::state::FISHING_CAST_ACK_TIMEOUT).await;
        maintain_scheduled_fishing(&state, entry).await;
        assert!(
            rx.try_recv().is_err(),
            "a rejected cast needs a longer pause"
        );
        tokio::time::advance(crate::state::FISHING_ERROR_RETRY_DELAY).await;
        maintain_scheduled_fishing(&state, entry).await;
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::FishingCast { .. })
        ));
    }

    #[tokio::test]
    async fn rain_pauses_only_outdoor_work_and_clear_resumes_the_current_routine() {
        use onlinerpg_shared::schedule::SchedulePeriod;
        use onlinerpg_shared::ServerMessage;

        let signe = npc_schedule(include_str!("../../data/npcs/signe/schedule.json"));
        let wick = npc_schedule(include_str!("../../data/npcs/wick/schedule.json"));
        let (s, _rx) = test_state();
        let state = Arc::new(Mutex::new(s));
        for (schedule, hour, minute, period, dark, wet, dry) in [
            (&signe, 13, 0, SchedulePeriod::Day, false, 5, 2),
            (&signe, 5, 0, SchedulePeriod::Day, false, 0, 0),
            (&signe, 11, 30, SchedulePeriod::Day, false, 1, 1),
            (&signe, 18, 30, SchedulePeriod::Dinner, false, 3, 3),
            (&signe, 20, 0, SchedulePeriod::Night, false, 4, 4),
            (&signe, 2, 0, SchedulePeriod::Night, false, 4, 4),
            (&wick, 22, 0, SchedulePeriod::Night, false, 5, 2),
            (&wick, 13, 0, SchedulePeriod::Day, false, 0, 0),
            (&wick, 18, 30, SchedulePeriod::Dinner, false, 1, 1),
            (&wick, 5, 30, SchedulePeriod::Breakfast, false, 3, 3),
            (&wick, 22, 0, SchedulePeriod::Night, true, 4, 4),
        ] {
            {
                let mut s = state.lock().await;
                s.game_hour = Some(hour);
                s.game_minute = Some(minute);
                s.schedule_period = Some(period);
                s.is_serin_dark_day = Some(dark);
                s.push_event(ServerMessage::WeatherSync {
                    seed: 42,
                    bias: 1.0,
                    sectors_tag: "test".into(),
                    rain_override: Some(1.0),
                    snow_override: false,
                    snow_laid: false,
                });
            }
            assert_eq!(
                resolve_due_schedule(&state, schedule).await,
                (Some(wet), None)
            );
            state.lock().await.push_event(ServerMessage::WeatherSync {
                seed: 42,
                bias: 1.0,
                sectors_tag: "test".into(),
                rain_override: Some(0.0),
                snow_override: false,
                snow_laid: false,
            });
            assert_eq!(
                resolve_due_schedule(&state, schedule).await,
                (Some(dry), None)
            );
        }
    }

    #[tokio::test]
    async fn shelter_stops_music_seats_the_bard_and_blocks_another_performance() {
        use onlinerpg_shared::ServerMessage;

        let schedule = npc_schedule(include_str!("../../data/npcs/signe/schedule.json"));
        let shelter = &schedule[5];
        let (mut s, mut rx) = test_state();
        let me = test_player(shelter.pos[0], shelter.pos[2]);
        s.self_player_id = Some(me.id);
        s.self_player = Some(me);
        s.in_game = true;
        s.push_event(ServerMessage::PlayerInteractionChanged {
            position: s.self_player.as_ref().unwrap().position,
            rotation: 0.0,
            floor_level: 0,
            player_id: s.self_player_id.unwrap(),
            object_type: Some(crate::state::MUSIC_EMOTE.into()),
            object_id: None,
        });
        s.push_event(ServerMessage::PlayerMusicStarted {
            player_id: s.self_player_id.unwrap(),
            track: "Twilight Fields".into(),
            elapsed_secs: 0.0,
        });
        s.begin_recital(&["The rain is coming".into()]).unwrap();
        let piece = |id, type_id: &str, dx: f32| FurniturePlacement {
            id,
            type_id: type_id.into(),
            x: shelter.pos[0] + dx,
            y: 0.0,
            z: shelter.pos[2],
            rotation_deg: 0.0,
            floor_level: 0,
            open_doors: 0,
        };
        s.world_cache.write().unwrap().sync_furniture(
            0,
            0,
            vec![piece(39, "chair", 0.0), piece(29, "table", 1.2)],
        );
        let state = Arc::new(Mutex::new(s));
        let active =
            check_schedule_transition(&state, &schedule, (Some(2), None), (Some(5), None), "Signe")
                .await;
        assert_eq!(active, (Some(5), None));
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::StopInteraction)));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(take_free_seat(&state, shelter, &Default::default(), "Signe").await);
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerFace { .. })
        ));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::InteractObject { object_type, object_id: 39 }) if object_type == "chair"
        ));
        let mut s = state.lock().await;
        assert_eq!(s.own_chair(), Some(39));
        assert!(s.refuses_play_command("/play_music"));
        assert!(s.begin_recital(&["An encore".into()]).is_err());
    }

    #[test]
    fn free_seats_skip_occupied_chairs_and_chairs_without_a_table() {
        let (mut s, _rx) = test_state();
        s.self_player = Some(test_player(0.0, 0.0));
        let piece = |id, type_id: &str, x: f32| FurniturePlacement {
            id,
            type_id: type_id.into(),
            x,
            y: 0.0,
            z: 0.0,
            rotation_deg: 0.0,
            floor_level: 0,
            open_doors: 0,
        };
        s.world_cache.write().unwrap().sync_furniture(
            0,
            0,
            vec![
                piece(1, "chair", 1.0),
                piece(2, "chair", 3.0),
                piece(3, "table", 2.0),
                piece(4, "chair", 8.0),
            ],
        );
        let mut sitter = test_player(1.5, 0.0);
        sitter.object_type = Some(crate::state::SIT_OBJECT_TYPE.into());
        sitter.object_id = Some(1);
        s.nearby_players.insert(sitter.id, sitter);
        let free: Vec<u32> = s.free_seats(0.0, 0.0, 0).iter().map(|c| c.id).collect();
        assert_eq!(free, [2]);
    }

    #[tokio::test]
    async fn a_scheduled_pose_is_adopted_on_send_and_refuses_play_music() {
        let (mut s, mut rx) = test_state();
        s.self_player = Some(test_player(0.0, 0.0));
        let entry = ScheduleEntry {
            action: Some("bed".to_string()),
            object_id: Some(23),
            ..Default::default()
        };

        send_interact_if_needed(&mut s, &entry).await;

        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::InteractObject { .. })
        ));
        assert_eq!(
            s.self_player.as_ref().unwrap().object_type.as_deref(),
            Some("bed")
        );
        assert!(s.refuses_play_command("/play_music"));
    }
}
