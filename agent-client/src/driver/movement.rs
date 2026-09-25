//! Schedule transitions, forced moves, and the housing-data prefetch that
//! lets pathfinding avoid buildings before the NPC starts moving. The walking
//! itself belongs to `walk`.

use std::collections::HashSet;
use std::sync::Arc;

use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::{ClientMessage, Position};
use onlinerpg_terrain::coords::{tile_to_region, world_to_tile};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::walk;
use crate::geom::PlanarDelta;
use crate::state::SharedState;
use crate::terrain_http::http_client;
use onlinerpg_shared::schedule::{ScheduleCondition, ScheduleEntry};

use onlinerpg_shared::schedule::resolve_active_schedule;

pub(super) const MOVE_SPEED: f32 = onlinerpg_shared::PLAYER_MOVE_SPEED;

/// How long a step takes at the speed the server actually moves us: the
/// hunger/debuff move multiplier the server folds into its step budget, times
/// the sprint multiplier. Pace faster than the server walks and every step
/// leaves from a stale position.
pub(super) fn travel_ms(step_dist: f32, sprinting: bool, move_mult: f32) -> u64 {
    let speed =
        MOVE_SPEED * move_mult.max(0.01) * onlinerpg_shared::hunger::sprint_move_mult(sprinting);
    ((step_dist / speed) * 1000.0) as u64
}

/// Maximum distance per move step (units). Longer segments are subdivided
/// so the NPC walks at MOVE_SPEED instead of teleporting.
pub(super) const MAX_STEP_DIST: f32 = 3.0;
const SCHEDULE_ARRIVAL_RADIUS: f32 = 2.0;

/// Forced moves are split into legs under the server's target-distance cap;
/// the margin absorbs whatever the two sims still disagree by.
const FORCE_MOVE_LEG_DIST: f32 = onlinerpg_shared::MAX_MOVE_TARGET_DISTANCE * 0.8;

/// Move result for path-following
pub(super) enum MoveResult {
    Arrived,
    Blocked,
    Died,
    Error,
}

/// Which schedule entry is due at the current game time.
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

/// Execute the move to a newly due schedule entry (from
/// [`resolve_due_schedule`]). Returns the new active schedule index.
pub(super) async fn check_schedule_transition(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
    current: (Option<usize>, Option<u32>),
    new: (Option<usize>, Option<u32>),
    label: &str,
) -> (Option<usize>, Option<u32>) {
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

/// Leave-taking before any driven walk: stop the current entry's
/// interaction and pack up placeables. Shared by schedule transitions and
/// sick-room bedside visits.
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

/// Walk to a schedule entry's position and set the final rotation. If the
/// entry has waypoints, visits each one in order before going to `pos`.
pub(super) async fn execute_schedule_move(state: &Arc<Mutex<SharedState>>, entry: &ScheduleEntry) {
    // Walk through patrol waypoints first (if any)
    for (i, wp) in entry.waypoints.iter().enumerate() {
        let (wx, wz) = (wp[0], wp[2]);
        debug!(
            "Patrol waypoint {}/{}: ({:.1}, {:.1})",
            i + 1,
            entry.waypoints.len(),
            wx,
            wz
        );
        match execute_move(state, wx, wz, entry.floor_level, Some(false)).await {
            MoveResult::Arrived => {}
            MoveResult::Blocked => {
                warn!("Patrol waypoint {i} blocked — skipping ({wx:.1}, {wz:.1})");
            }
            MoveResult::Died => return,
            MoveResult::Error => {
                error!("Patrol waypoint {i} error");
            }
        }
    }

    // Go to final position
    let (x, y, z) = (entry.pos[0], entry.pos[1], entry.pos[2]);

    // Already near the target (same floor)? Skip the walk but still fall
    // through to the exact-position send — the entry's spot and rotation
    // apply even without a walk (a maid already standing at the table must
    // still turn to face the guest).
    let already_near = {
        let s = state.lock().await;
        s.self_player.as_ref().is_some_and(|p| {
            s.passability_floor() == entry.floor_level
                && PlanarDelta::to_xz(&p.position, x, z).dist < SCHEDULE_ARRIVAL_RADIUS
        })
    };

    let arrived = if already_near {
        debug!("Already near schedule target — skipping the walk");
        true
    } else {
        // A pose position may sit on the furniture itself (a bed swallows its
        // own cells); walk beside it and let the exact-position send below
        // cross the last metre.
        let (walk_x, walk_z) = {
            let s = state.lock().await;
            s.walkable_near(x, z, entry.floor_level)
        };
        match execute_move(state, walk_x, walk_z, entry.floor_level, Some(false)).await {
            MoveResult::Arrived => true,
            MoveResult::Blocked => {
                // Force-move to schedule position (e.g. cross-floor moves through
                // closed doors). NPCs must follow their schedules.
                warn!(
                    "Schedule move blocked — force-moving to ({x:.1}, {z:.1}) floor {}",
                    entry.floor_level
                );
                true
            }
            MoveResult::Died => false,
            MoveResult::Error => {
                error!("Schedule move error");
                false
            }
        }
    };

    if arrived {
        // Send final position with exact rotation
        let rot_rad = entry.rotation.to_radians();
        let mut s = state.lock().await;
        // Schedules are authored in housing floors, which the wire and the
        // passability cache number the same way. adopt_floor_level so a
        // cross-floor force-move still purges the left floor's monsters.
        s.adopt_floor_level(entry.floor_level as i8);
        let target = Position { x, y, z };
        // A forced move can span the whole map; legs keep every target under
        // the server's distance cap so none is silently refused.
        let from = s.self_player.as_ref().map_or(target, |p| p.position);
        for (i, leg) in force_move_legs(&from, target).into_iter().enumerate() {
            let cmd = ClientMessage::PlayerMove {
                position: leg,
                rotation: rot_rad,
                floor_level: entry.floor_level as i8,
                append: i > 0,
                // Catch-up legs are free: forced moves are routine, not asked
                // for, and must not price the schedule in satiation.
                sprinting: false,
            };
            if let Err(e) = s.send_command(cmd).await {
                error!("Failed to send schedule move: {e}");
                break;
            }
        }

        send_interact_if_needed(&mut s, entry).await;
    }
}

/// Straight-line legs from `from` to `to`, each under the server's move
/// target cap. The last leg is exactly `to`.
fn force_move_legs(from: &Position, to: Position) -> Vec<Position> {
    let delta = PlanarDelta::between(from, &to);
    let legs = (delta.dist / FORCE_MOVE_LEG_DIST).ceil().max(1.0) as u32;
    (1..legs)
        .map(|i| {
            let t = i as f32 / legs as f32;
            Position {
                x: from.x + delta.dx * t,
                y: from.y + (to.y - from.y) * t,
                z: from.z + delta.dz * t,
            }
        })
        .chain(std::iter::once(to))
        .collect()
}

/// Walk to a fixed spot on the map. A thin view over [`walk::walk`]: this
/// caller cares whether its walk finished, not which target it was following.
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
    match walk::walk(state, &to, false, sprint).await {
        walk::Walked::Arrived => MoveResult::Arrived,
        walk::Walked::Error => MoveResult::Error,
        walk::Walked::Lost(walk::LostReason::PlayerDied) => MoveResult::Died,
        walk::Walked::Lost(_) => MoveResult::Blocked,
    }
}

/// Raw region object placements, as served by `/api/terrain/objects/{rx}/{rz}`.
#[derive(serde::Deserialize)]
struct RegionObjects {
    #[serde(default)]
    placements: Vec<FurniturePlacement>,
}

/// Insert the region (16×16 tiles) containing a world position into the set.
fn insert_region(regions: &mut HashSet<(i32, i32)>, x: f32, z: f32) {
    regions.insert((
        tile_to_region(world_to_tile(x)),
        tile_to_region(world_to_tile(z)),
    ));
}

/// Every position pathfinding data should cover: a schedule's stops and patrol
/// waypoints, or — for an agent with no schedule — wherever it currently is.
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

/// Fetch region object placements around `positions` and register their solid
/// furniture in the passability cache, so the bot paths around furniture
/// exactly like the browser client (both go through the shared `furniture`
/// resolution). A region is ~1024m, so this usually touches one or two.
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
    use crate::state::tests::{test_player, test_state};
    use onlinerpg_shared::fishing::{FishState, FishingAction, FishingOutcome};
    use onlinerpg_shared::inventory::{EquipSlot, ItemInstance};
    use onlinerpg_shared::{PlayerId, ServerMessage};

    fn npc_schedule(json: &str) -> Vec<ScheduleEntry> {
        #[derive(serde::Deserialize)]
        struct File {
            schedule: Vec<ScheduleEntry>,
        }
        let mut schedule = serde_json::from_str::<File>(json).unwrap().schedule;
        assert!(onlinerpg_shared::schedule::parse_conditions(&mut schedule).is_empty());
        schedule
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
        s.self_equipped.insert(
            EquipSlot::MainHand,
            ItemInstance {
                instance_id: 1,
                item_def_id: "fishing_rod".into(),
                quantity: 1,
                enchant: 0,
                cape_color: None,
                cape_texture: None,
                locked: false,
            },
        );
        assert!(rx.try_recv().is_err());
        (Arc::new(Mutex::new(s)), rx)
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
            matches!(rx.try_recv(), Ok(ClientMessage::PlayerMove { position, rotation, .. })
                if position.x == entry.pos[0] && position.y == entry.pos[1]
                    && position.z == entry.pos[2] && rotation == entry.rotation.to_radians()
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
            Ok(ClientMessage::PlayerMove { .. })
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
            Ok(ClientMessage::PlayerMove { .. })
        ));
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::InteractObject { object_type, object_id: 111 })
                if object_type == "rustic_bed"
            )
        );
        state.lock().await.sync_height().await.unwrap();
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
            Ok(ClientMessage::PlayerMove { .. })
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
            Ok(ClientMessage::PlayerMove { .. })
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
            Ok(ClientMessage::PlayerMove { .. })
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
            Ok(ClientMessage::PlayerMove { .. })
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
            Ok(ClientMessage::PlayerMove { .. })
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
        let state = Arc::new(Mutex::new(s));
        let active =
            check_schedule_transition(&state, &schedule, (Some(2), None), (Some(5), None), "Signe")
                .await;
        assert_eq!(active, (Some(5), None));
        assert!(matches!(rx.try_recv(), Ok(ClientMessage::StopInteraction)));
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerMove { .. })
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

    /// The pose is adopted when the InteractObject is sent, not on the
    /// server's echo — a stale LLM response handled in the same tick must
    /// already find the bed under us, or its /play_music replaces the pose
    /// and the NPC sleeps standing.
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

    /// Mispaced steps leave from a stale position and get snapped back.
    #[test]
    fn a_step_is_paced_by_the_speed_the_server_moves_us() {
        let walk = travel_ms(MAX_STEP_DIST, false, 1.0);
        let sprint = travel_ms(MAX_STEP_DIST, true, 1.0);
        assert_eq!(walk, ((MAX_STEP_DIST / MOVE_SPEED) * 1000.0) as u64);
        assert_eq!(
            sprint,
            (walk as f32 / onlinerpg_shared::hunger::SPRINT_MOVE_MULT) as u64
        );
        assert!(sprint < walk);
        // A Weak walker is slowed server-side; pacing at full speed would
        // leave every step from a stale position.
        let weak = travel_ms(
            MAX_STEP_DIST,
            false,
            onlinerpg_shared::hunger::WEAK_MOVE_MULT,
        );
        assert_eq!(
            weak,
            ((MAX_STEP_DIST / (MOVE_SPEED * onlinerpg_shared::hunger::WEAK_MOVE_MULT)) * 1000.0)
                as u64
        );
        assert!(weak > walk);
    }

    #[test]
    fn force_move_legs_stay_under_cap_and_end_exactly() {
        let from = Position {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        };
        let to = Position {
            x: 90.0,
            y: 4.0,
            z: 90.0,
        };
        let legs = force_move_legs(&from, to);
        assert!(legs.len() > 1);
        let mut prev = from;
        for leg in &legs {
            assert!(
                PlanarDelta::between(&prev, leg).dist < onlinerpg_shared::MAX_MOVE_TARGET_DISTANCE
            );
            prev = *leg;
        }
        assert_eq!(*legs.last().unwrap(), to);
    }

    #[test]
    fn short_force_move_is_a_single_exact_leg() {
        let from = Position {
            x: 10.0,
            y: 0.0,
            z: 10.0,
        };
        let to = Position {
            x: 13.0,
            y: 0.0,
            z: 14.0,
        };
        let legs = force_move_legs(&from, to);
        assert_eq!(legs, vec![to]);
    }
}
