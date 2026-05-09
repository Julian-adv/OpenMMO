//! Movement execution: A*-driven walks, schedule transitions, and the
//! housing-data prefetch that lets pathfinding avoid buildings before the
//! NPC starts moving.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::ClientMessage;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::geom::PlanarDelta;
use crate::orchestrator::ScheduleEntry;
use crate::state::SharedState;

use super::prompt::resolve_active_schedule;

/// Character movement speed in units/sec (matches client DEFAULT_MOVEMENT_CONFIG.maxSpeed).
const MOVE_SPEED: f32 = 3.0;

/// Maximum distance per move step (units). Longer segments are subdivided
/// so the NPC walks at MOVE_SPEED instead of teleporting.
const MAX_STEP_DIST: f32 = 3.0;
const SCHEDULE_ARRIVAL_RADIUS: f32 = 2.0;

/// Housing chunk size in world units (must match server's CHUNK_SIZE).
const HOUSING_CHUNK_SIZE: f32 = 64.0;

/// Move result for path-following
pub(super) enum MoveResult {
    Arrived,
    Blocked,
    Error,
}

/// Check if the active schedule entry changed and execute a move if needed.
/// Returns the new active schedule index.
pub(super) async fn check_schedule_transition(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
    current: (Option<usize>, Option<u32>),
    label: &str,
) -> (Option<usize>, Option<u32>) {
    let (is_night, game_hour, game_minute) = { state.lock().await.time_context() };
    let new = resolve_active_schedule(schedule, is_night, game_hour, game_minute);
    if new != current {
        // Stop interaction from previous schedule entry if it had an action
        if let Some(prev_i) = current.0 {
            if schedule[prev_i].action.is_some() {
                let mut s = state.lock().await;
                if let Err(e) = s.send_command(ClientMessage::StopInteraction).await {
                    error!("[{label}] Failed to send StopInteraction: {e}");
                }
            }
        }

        if let Some(i) = new.0 {
            let entry = &schedule[i];
            info!(
                "[{label}] Schedule transition: moving to {}",
                entry.display_label()
            );
            execute_schedule_move(state, entry).await;
        }
    }
    new
}

/// Send InteractObject if the schedule entry has an action and object_id.
async fn send_interact_if_needed(s: &mut SharedState, entry: &ScheduleEntry) {
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

/// Walk to a schedule entry's position and set the final rotation. If the
/// entry has waypoints, visits each one in order before going to `pos`.
async fn execute_schedule_move(state: &Arc<Mutex<SharedState>>, entry: &ScheduleEntry) {
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
        match execute_move(state, wx, wz, entry.floor_level).await {
            MoveResult::Arrived => {}
            MoveResult::Blocked => {
                warn!("Patrol waypoint {i} blocked — skipping ({wx:.1}, {wz:.1})");
            }
            MoveResult::Error => {
                error!("Patrol waypoint {i} error");
            }
        }
    }

    // Go to final position
    let (x, y, z) = (entry.pos[0], entry.pos[1], entry.pos[2]);

    // Check if we're already near the target (including floor level)
    {
        let mut s = state.lock().await;
        if let Some(ref p) = s.self_player {
            let to_target = PlanarDelta::to_xz(&p.position, x, z);
            let same_floor = s.self_floor_level == entry.floor_level;
            if same_floor && to_target.dist < SCHEDULE_ARRIVAL_RADIUS {
                debug!("Already near schedule target — skipping movement");
                send_interact_if_needed(&mut s, entry).await;
                return;
            }
        }
    }

    let arrived = match execute_move(state, x, z, entry.floor_level).await {
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
        MoveResult::Error => {
            error!("Schedule move error");
            false
        }
    };

    if arrived {
        // Send final position with exact rotation
        let rot_rad = entry.rotation.to_radians();
        let mut s = state.lock().await;
        s.self_floor_level = entry.floor_level;
        let cmd = ClientMessage::PlayerMove {
            position: onlinerpg_shared::Position { x, y, z },
            rotation: rot_rad,
            floor_level: entry.floor_level as i8,
        };
        if let Err(e) = s.send_command(cmd).await {
            error!("Failed to send schedule move: {e}");
        }

        send_interact_if_needed(&mut s, entry).await;
    }
}

/// Execute a move to the target position using A* pathfinding.
/// Follows waypoints sequentially with appropriate timing, subdividing
/// long segments so the NPC never teleports.
pub(super) async fn execute_move(
    state: &Arc<Mutex<SharedState>>,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
) -> MoveResult {
    let path_result = {
        let s = state.lock().await;
        s.find_path_to(goal_x, goal_z, goal_floor)
    };

    if path_result.waypoints.is_empty() {
        if !path_result.found {
            return MoveResult::Blocked;
        }
        return MoveResult::Arrived;
    }

    for wp in &path_result.waypoints {
        // Subdivide long segments into small steps
        loop {
            let travel_ms = {
                let mut s = state.lock().await;
                let player = match &s.self_player {
                    Some(p) => p,
                    None => return MoveResult::Error,
                };

                let to_wp = PlanarDelta::to_xz(&player.position, wp.x, wp.z);
                if to_wp.dist < 0.1 {
                    break;
                }

                let (step_x, step_z, step_dist) = if to_wp.dist <= MAX_STEP_DIST {
                    (wp.x, wp.z, to_wp.dist)
                } else {
                    let ratio = MAX_STEP_DIST / to_wp.dist;
                    (
                        player.position.x + to_wp.dx * ratio,
                        player.position.z + to_wp.dz * ratio,
                        MAX_STEP_DIST,
                    )
                };

                let cmd = ClientMessage::PlayerMove {
                    position: onlinerpg_shared::Position {
                        x: step_x,
                        y: player.position.y,
                        z: step_z,
                    },
                    rotation: to_wp.rotation(),
                    floor_level: wp.floor as i8,
                };
                s.self_floor_level = wp.floor;
                if let Err(e) = s.send_command(cmd).await {
                    error!("Failed to send move waypoint: {e}");
                    return MoveResult::Error;
                }
                ((step_dist / MOVE_SPEED) * 1000.0) as u64
            };

            tokio::time::sleep(Duration::from_millis(travel_ms.max(50))).await;
        }
    }

    MoveResult::Arrived
}

/// Insert a position's chunk and its 8 neighbors into the set.
fn insert_chunk_neighbors(chunks: &mut HashSet<(i32, i32)>, x: f32, z: f32) {
    let cx = (x / HOUSING_CHUNK_SIZE).floor() as i32;
    let cz = (z / HOUSING_CHUNK_SIZE).floor() as i32;
    for dx in -1..=1i32 {
        for dz in -1..=1i32 {
            chunks.insert((cx + dx, cz + dz));
        }
    }
}

/// Fetch houses from the HTTP API for all chunks that the schedule positions
/// and waypoints pass through, so pathfinding can avoid buildings.
pub(super) async fn fetch_houses_for_schedule(
    world_cache: &Arc<std::sync::RwLock<crate::state::WorldCache>>,
    schedule: &[ScheduleEntry],
    api_base_url: &str,
    label: &str,
) {
    let mut chunks = HashSet::new();
    for entry in schedule {
        insert_chunk_neighbors(&mut chunks, entry.pos[0], entry.pos[2]);
        for wp in &entry.waypoints {
            insert_chunk_neighbors(&mut chunks, wp[0], wp[2]);
        }
    }

    debug!(
        "[{label}] Fetching houses for {} chunk(s): {:?}",
        chunks.len(),
        chunks
    );
    let client = reqwest::Client::new();
    let fetches = chunks.iter().map(|&(cx, cz)| {
        let client = &client;
        let url = format!("{api_base_url}/api/housing/area/{cx}/{cz}");
        async move {
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<Vec<HouseData>>().await.unwrap_or_default()
                }
                Ok(resp) => {
                    warn!(
                        "[{label}] Housing API returned {} for chunk ({cx},{cz})",
                        resp.status()
                    );
                    Vec::new()
                }
                Err(e) => {
                    warn!("[{label}] Failed to fetch houses for chunk ({cx},{cz}): {e}");
                    Vec::new()
                }
            }
        }
    });
    let results = futures_util::future::join_all(fetches).await;

    let all_houses: Vec<HouseData> = results.into_iter().flatten().collect();
    if all_houses.is_empty() {
        info!("[{label}] No houses found in any chunk");
    } else {
        let count = all_houses.len();
        let mut world = world_cache.write().unwrap();
        for house in all_houses {
            world.add_house(house);
        }
        info!("[{label}] Loaded {count} house(s) for pathfinding");
    }
}
