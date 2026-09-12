//! Shared walking, door handling, and position correction for fixed and moving targets.
//! [`WalkTo`] and [`Tuning`] define arrival and retry behavior.

use std::sync::Arc;
use std::time::{Duration, Instant};

use onlinerpg_shared::housing::{WallDirection, WallVariant};
use onlinerpg_shared::pathfinding::{self, PathWaypoint};
use onlinerpg_shared::{ClientMessage, PlayerId, Position};
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::movement::{travel_ms, MAX_STEP_DIST};
use crate::dungeon::{DoorApproach, Dungeon};
use crate::geom::PlanarDelta;
use crate::state::SharedState;

/// Minimum distance to a monster before attacking (matches the web client).
const ATTACK_RANGE: f32 = 2.0;
/// Polite stop distance when walking up to another character: close enough to
/// talk, far enough that the models don't overlap.
const APPROACH_RANGE: f32 = 1.0;
/// Stop distance for ground items, inside the server's 2.5 m pickup gate
/// (`MAX_PICKUP_DISTANCE`) so overshoot and float error can't push the
/// PickupItem send back out of range.
const PICKUP_ARRIVE_RANGE: f32 = 2.0;
/// Give up chasing a monster further than this away, so an NPC guard does not
/// run off its patrol after a distant threat.
const MAX_CHASE_DISTANCE: f32 = 20.0;
/// How far the target must move from our last path goal before we re-route.
const REROUTE_THRESHOLD: f32 = 1.5;
/// How often to poll when there is no step to send.
const IDLE_TICK_MS: u64 = 200;

const MAX_CHASE_SECS: f32 = 15.0;
/// Longer than the combat chase: an approach target can be anywhere inside
/// the sight radius.
const MAX_APPROACH_SECS: f32 = 30.0;
/// Shorter than an approach: a dropped item never walks away.
const MAX_PICKUP_WALK_SECS: f32 = 12.0;
/// A point is only ever walked to from inside its own room, so this is a
/// stuck-detector, not a travel budget.
const MAX_POINT_WALK_SECS: f32 = 15.0;

/// How many shut doors one walk may unlatch on its way. A floor carries a
/// handful; the cap only stops a pathological loop.
const MAX_DOORS_PER_WALK: usize = 6;
/// A chase is short and its target is moving — two tries, then let it go.
const MAX_CHASE_DOORS: usize = 2;

/// How many times one walk restarts pathfinding after the server snaps us
/// back (`PositionCorrected`). Corrections are throttled server-side, so a
/// persistent disagreement burns through these in a few seconds and gives up.
const MAX_CORRECTION_REPATHS: usize = 3;

/// Wait for the server's door-toggle reply before re-pathing — that message,
/// not the request, is what reopens the cells for A*.
const DOOR_TOGGLE_WAIT: Duration = Duration::from_millis(400);

/// How many doors a blocked walk probes with A* before giving up. They are
/// tried nearest-first, so the one in our way is normally the first.
const MAX_DOOR_PROBES: usize = 6;

/// Ignore doors further away than this: a door across the map is not what
/// stands between us and the goal, and every probe costs a path search.
const MAX_DOOR_SEARCH_DIST: f32 = 40.0;

/// Where a walk is headed. One row here and one in [`WalkTo::tuning`] is the
/// whole of what makes a chase different from a commute.
pub(super) enum WalkTo<'a> {
    Monster(&'a str),
    Character(&'a PlayerId),
    GroundItem(u64),
    /// A fixed spot nearby (the cell beside a chest), with the distance the
    /// caller needs us to end up inside.
    Point {
        pos: Position,
        floor_level: i8,
        arrive_range: f32,
    },
    /// Somewhere on the map, however far: the commute, the patrol leg, the
    /// walk home. No leash and no deadline — only arrival, or a route that
    /// does not exist.
    Place {
        x: f32,
        z: f32,
        floor: u8,
    },
}

/// The distances and deadlines one walk runs by.
struct Tuning {
    /// The walk ends successfully within this distance of the target.
    arrive_range: f32,
    /// How far short of the target the path goal stops, so a step can never
    /// land on top of a character. Zero pathfinds straight at the target —
    /// the arrive check stops us first.
    path_pullback: f32,
    /// How close a direct step (no path) walks before it stops short.
    step_stop_dist: f32,
    /// Give up once the target is further away than this.
    max_distance: f32,
    max_secs: f32,
    /// Arriving also needs a clear attack line — stopping at range through a
    /// shut door is no arrival at all.
    needs_clear_line: bool,
    max_doors: usize,
    /// A destination that cannot move is worth giving up on the moment A*
    /// says it cannot be reached; a target that walks may stand somewhere
    /// reachable on the next poll.
    give_up_when_unreachable: bool,
    /// Whether walking the route A* found is itself the arrival. Where the
    /// goal is a cell rather than a range, it has to be: A* smooths its last
    /// waypoint onto the goal, and a walk that lands a few centimetres short
    /// would otherwise re-search the same route forever.
    route_end_is_arrival: bool,
}

impl WalkTo<'_> {
    fn position(&self, s: &SharedState) -> Option<Position> {
        match self {
            Self::Monster(id) => s.nearby_monsters.get(*id).map(|m| m.position),
            Self::Character(id) => s.nearby_players.get(*id).map(|p| p.position),
            Self::GroundItem(id) => s.ground_item(*id).map(|i| i.position),
            Self::Point { pos, .. } => Some(*pos),
            Self::Place { x, z, .. } => Some(Position {
                x: *x,
                y: 0.0,
                z: *z,
            }),
        }
    }

    /// Passability floor index to path toward, converted from the target's
    /// wire `floor_level`.
    fn floor(&self, s: &SharedState) -> u8 {
        let level = match self {
            Self::Monster(id) => s.nearby_monsters.get(*id).map(|m| m.floor_level),
            Self::Character(id) => s.nearby_players.get(*id).map(|p| p.floor_level),
            Self::GroundItem(id) => s.ground_item(*id).map(|i| i.floor_level),
            Self::Point { floor_level, .. } => Some(*floor_level),
            Self::Place { floor, .. } => return *floor,
        };
        onlinerpg_shared::dungeon::passability_floor_for_level(level.unwrap_or(0))
    }

    fn tuning(&self) -> Tuning {
        match self {
            // Characters carry a little arrive slack over APPROACH_RANGE so
            // reaching the pulled-back path goal always counts as in range.
            Self::Character(_) => Tuning {
                arrive_range: APPROACH_RANGE + 0.2,
                path_pullback: APPROACH_RANGE,
                step_stop_dist: APPROACH_RANGE,
                max_distance: crate::state::NPC_SIGHT_RADIUS,
                max_secs: MAX_APPROACH_SECS,
                needs_clear_line: false,
                max_doors: MAX_CHASE_DOORS,
                give_up_when_unreachable: false,
                route_end_is_arrival: false,
            },
            Self::Monster(_) => Tuning {
                arrive_range: ATTACK_RANGE,
                path_pullback: 0.0,
                step_stop_dist: ATTACK_RANGE - 0.5,
                max_distance: MAX_CHASE_DISTANCE,
                max_secs: MAX_CHASE_SECS,
                needs_clear_line: true,
                max_doors: MAX_CHASE_DOORS,
                give_up_when_unreachable: false,
                route_end_is_arrival: false,
            },
            Self::GroundItem(_) => Tuning {
                arrive_range: PICKUP_ARRIVE_RANGE,
                path_pullback: 0.0,
                step_stop_dist: PICKUP_ARRIVE_RANGE - 0.5,
                max_distance: crate::state::NPC_SIGHT_RADIUS,
                max_secs: MAX_PICKUP_WALK_SECS,
                needs_clear_line: false,
                max_doors: MAX_CHASE_DOORS,
                give_up_when_unreachable: false,
                route_end_is_arrival: false,
            },
            // A fixed point is only ever offered once we share its room, so
            // the sight radius is slack, not a leash.
            Self::Point { arrive_range, .. } => Tuning {
                arrive_range: *arrive_range,
                path_pullback: 0.0,
                step_stop_dist: (arrive_range - 0.5).max(0.5),
                max_distance: crate::state::NPC_SIGHT_RADIUS,
                max_secs: MAX_POINT_WALK_SECS,
                needs_clear_line: false,
                max_doors: MAX_CHASE_DOORS,
                give_up_when_unreachable: true,
                route_end_is_arrival: false,
            },
            // Arrival is the goal cell itself: A* smooths its last waypoint
            // onto it, so anything short of standing there is still walking.
            Self::Place { .. } => Tuning {
                arrive_range: 0.1,
                path_pullback: 0.0,
                step_stop_dist: 0.0,
                max_distance: f32::INFINITY,
                max_secs: f32::INFINITY,
                needs_clear_line: false,
                max_doors: MAX_DOORS_PER_WALK,
                give_up_when_unreachable: true,
                route_end_is_arrival: true,
            },
        }
    }
}

/// Log label. `Display` rather than a `-> String` helper so the monster arm
/// keeps borrowing its id instead of allocating a copy for a log line.
impl std::fmt::Display for WalkTo<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Monster(id) => f.write_str(id),
            Self::Character(id) => write!(f, "{id}"),
            Self::GroundItem(id) => write!(f, "item {id}"),
            Self::Point { pos, .. } => write!(f, "point ({:.1}, {:.1})", pos.x, pos.z),
            Self::Place { x, z, .. } => write!(f, "({x:.1}, {z:.1})"),
        }
    }
}

/// Why a walk gave up, tagged at the point where the loop knew the ground
/// truth — so callers report the real reason instead of guessing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum LostReason {
    /// The target left the world state: died, despawned, or out of sight.
    TargetGone,
    /// We died (or vanished) on the way.
    PlayerDied,
    /// The target sits this far away, beyond the walk's leash.
    TooFar(f32),
    /// The clock ran out before arriving.
    Timeout,
    /// No route on this floor — walls or shut doors all the way around.
    NoPath,
    /// The only way on is a locked door and we hold no key for it.
    LockedDoor,
    /// The server kept refusing our steps: its layout disagrees with ours.
    Desynced,
}

impl LostReason {
    /// Target-neutral clause completing "You could not reach X — {clause}."
    pub(super) fn clause(&self) -> String {
        match self {
            Self::TargetGone => "your target is no longer there".to_string(),
            Self::PlayerDied => "you died on the way".to_string(),
            Self::TooFar(d) => format!("your target is {d:.0}m away, beyond your reach"),
            Self::Timeout => "you ran out of time before arriving".to_string(),
            Self::NoPath => "no route leads there from here".to_string(),
            Self::LockedDoor => {
                "the way on is a locked door and you hold no key for it".to_string()
            }
            Self::Desynced => "the ground kept refusing your steps".to_string(),
        }
    }
}

/// How a walk ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Walked {
    Arrived,
    /// The target stopped being worth walking to.
    Lost(LostReason),
    Error,
}

async fn recover_mount(
    state: &Arc<Mutex<SharedState>>,
    goal: (f32, f32),
    background: bool,
) -> bool {
    {
        let mut s = state.lock().await;
        s.mount_recovery_id = s.mount_recovery_id.wrapping_add(1);
        s.mount_recovery_result = None;
        let request_id = s.mount_recovery_id;
        if s.send_flagged_command(
            ClientMessage::PlayerMountRecover {
                request_id,
                goal: Position {
                    x: goal.0,
                    y: 0.0,
                    z: goal.1,
                },
            },
            background,
        )
        .await
        .is_err()
        {
            return false;
        }
    }
    for _ in 0..200 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let s = state.lock().await;
        if let Some(success) = s.mount_recovery_result {
            return success;
        }
        if !s
            .self_player
            .as_ref()
            .is_some_and(|p| p.mounted && p.health > 0)
        {
            return false;
        }
    }
    let mut s = state.lock().await;
    let _ = s
        .send_flagged_command(
            ClientMessage::PlayerMountTurn {
                rotation: 0.0,
                stop: true,
                sprinting: false,
            },
            background,
        )
        .await;
    false
}

/// Walk to `to` until we are inside its arrive range.
///
/// `background` marks steps no current action asked for (the follow task), so
/// they stay out of the action-progress count.
pub(super) async fn walk(
    state: &Arc<Mutex<SharedState>>,
    to: &WalkTo<'_>,
    background: bool,
    mut sprint: Option<bool>,
) -> Walked {
    let tuning = to.tuning();
    let started = Instant::now();
    let mut route: Vec<PathWaypoint> = Vec::new();
    let mut leg = 0usize;
    let mut doors_opened = 0usize;
    let mut repaths = 0usize;
    let mut recovery_attempted = false;
    let mut unreachable = false;
    let mut last_goal: Option<(f32, f32)> = None;
    let mut corrections = state.lock().await.position_corrections;

    loop {
        if started.elapsed().as_secs_f32() > tuning.max_secs {
            warn!("Walk to {to} timed out");
            return Walked::Lost(LostReason::Timeout);
        }

        let (target_floor, goal, snapped) = {
            let s = state.lock().await;
            let Some(target_pos) = to.position(&s) else {
                return Walked::Lost(LostReason::TargetGone);
            };
            let Some(me) = s.self_player.as_ref().filter(|p| p.health > 0) else {
                return Walked::Lost(LostReason::PlayerDied);
            };
            let to_target = PlanarDelta::between(&me.position, &target_pos);
            let target_floor = to.floor(&s);
            let arrived = to_target.dist <= tuning.arrive_range
                && !(tuning.needs_clear_line && s.attack_line_blocked(target_pos.x, target_pos.z))
                && !(matches!(to, WalkTo::Place { .. }) && s.passability_floor() != target_floor);
            if arrived {
                return Walked::Arrived;
            }
            if to_target.dist > tuning.max_distance {
                info!(
                    "Giving up on {to}: {:.1}m away (>{:.1}m)",
                    to_target.dist, tuning.max_distance
                );
                return Walked::Lost(LostReason::TooFar(to_target.dist));
            }
            // The path goal, pulled back toward us so a route never ends
            // inside the target character.
            let goal = if tuning.path_pullback > 0.0 && to_target.dist > tuning.path_pullback {
                let ratio = (to_target.dist - tuning.path_pullback) / to_target.dist;
                (
                    me.position.x + to_target.dx * ratio,
                    me.position.z + to_target.dz * ratio,
                )
            } else {
                (target_pos.x, target_pos.z)
            };
            let snapped = s.position_corrections != corrections;
            (target_floor, goal, snapped)
        };

        // The server snapped us back: whatever we were walking, it walks into
        // a step it refuses, so drop the route and search again from where it
        // says we are (`relocate_self` has already moved us there).
        if snapped {
            let mounted = state
                .lock()
                .await
                .self_player
                .as_ref()
                .is_some_and(|p| p.mounted);
            if mounted && !recovery_attempted {
                recovery_attempted = true;
                sprint = Some(false);
                if !recover_mount(state, goal, background).await {
                    return Walked::Lost(LostReason::Desynced);
                }
                corrections = state.lock().await.position_corrections;
                route.clear();
                leg = 0;
                continue;
            }
            corrections = state.lock().await.position_corrections;
            repaths += 1;
            if repaths > MAX_CORRECTION_REPATHS {
                info!("Walk to {to} gave up after {repaths} position corrections");
                return Walked::Lost(LostReason::Desynced);
            }
            debug!("Re-pathing after a server position correction ({repaths})");
            route.clear();
            leg = 0;
        }

        let goal_moved = match last_goal {
            Some((x, z)) => PlanarDelta::xz(x, z, goal.0, goal.1).dist > REROUTE_THRESHOLD,
            None => true,
        };
        if route.is_empty() || leg >= route.len() || goal_moved {
            let (found, waypoints, start_floor) = {
                let s = state.lock().await;
                let r = s.find_path_to(goal.0, goal.1, target_floor);
                (r.found, r.waypoints, s.passability_floor())
            };
            // A search that cannot reach the goal still hands back the leg
            // that gets closest. That leg is worth walking — it carries us to
            // the wall or the door in the way — but only as far as it stays
            // on the floor it started on: with the way down sealed, the
            // closest node A* can reach is the *surface* above the target,
            // and walking that climbs back out of the dungeon.
            unreachable = !found;
            route = if found {
                waypoints
            } else {
                let keep = waypoints
                    .iter()
                    .position(|wp| wp.floor != start_floor)
                    .unwrap_or(waypoints.len());
                waypoints[..keep].to_vec()
            };
            leg = 0;
            last_goal = Some(goal);
        }

        match step_along(state, &route, &mut leg, background, sprint).await {
            Step::Sent(ms) => {
                tokio::time::sleep(Duration::from_millis(ms.max(50))).await;
                continue;
            }
            Step::Error => return Walked::Error,
            Step::Nothing => {}
        }

        // Nothing left to walk.
        if !unreachable {
            // A* had a route and we walked it out. Where the goal is a cell
            // that is the arrival; where it is a range around something that
            // moves, the something moved — look again.
            if tuning.route_end_is_arrival {
                return Walked::Arrived;
            }
            repaths = 0;
            route.clear();
            tokio::time::sleep(Duration::from_millis(IDLE_TICK_MS)).await;
            continue;
        }
        // No route at all, or we are standing on the closest cell A* could
        // reach — which is where a shut door usually is.
        if doors_opened < tuning.max_doors && open_blocking_door(state, background, sprint).await {
            doors_opened += 1;
            route.clear();
            continue;
        }
        // Underground a missing route is a wall or a shut door, never the
        // open ground a straight line assumes.
        {
            let s = state.lock().await;
            if s.self_floor_level < 0 {
                info!("No route to {to} on this floor");
                return Walked::Lost(if locked_door_without_key(&s) {
                    LostReason::LockedDoor
                } else {
                    LostReason::NoPath
                });
            }
        }
        // Try a clear surface step, preserving correction requests from sealed cells.
        match nudge(state, goal, tuning.step_stop_dist, background, sprint).await {
            Step::Sent(ms) => tokio::time::sleep(Duration::from_millis(ms.max(50))).await,
            Step::Error => return Walked::Error,
            Step::Nothing => tokio::time::sleep(Duration::from_millis(IDLE_TICK_MS)).await,
        }
        if tuning.give_up_when_unreachable {
            return Walked::Lost(LostReason::NoPath);
        }
    }
}

/// What one step attempt did.
enum Step {
    /// Sent, and this is how long the server takes to walk it.
    Sent(u64),
    /// Nothing to send from here.
    Nothing,
    Error,
}

/// Send one `MAX_STEP_DIST`-bounded step toward `route[leg]`, advancing `leg`
/// as waypoints are reached. Subdividing is what keeps the body walking at
/// `MOVE_SPEED` instead of teleporting.
async fn step_along(
    state: &Arc<Mutex<SharedState>>,
    route: &[PathWaypoint],
    leg: &mut usize,
    background: bool,
    sprint: Option<bool>,
) -> Step {
    while *leg < route.len() {
        let wp = &route[*leg];
        let mut s = state.lock().await;
        let Some(me) = s.self_player.as_ref().map(|p| p.position) else {
            return Step::Error;
        };
        let to_wp = PlanarDelta::to_xz(&me, wp.x, wp.z);
        if to_wp.dist < 0.1 {
            *leg += 1;
            continue;
        }
        let (x, z, dist) = if to_wp.dist <= MAX_STEP_DIST {
            *leg += 1;
            (wp.x, wp.z, to_wp.dist)
        } else {
            let ratio = MAX_STEP_DIST / to_wp.dist;
            (
                me.x + to_wp.dx * ratio,
                me.z + to_wp.dz * ratio,
                MAX_STEP_DIST,
            )
        };
        let turn_ms = s.mount_turn_delay_ms(to_wp.rotation());
        return match s
            .send_step(x, z, wp.floor, to_wp.rotation(), background, sprint)
            .await
        {
            Ok(sprinting) => {
                Step::Sent(turn_ms + travel_ms(dist, sprinting, s.movement_speed_mult()))
            }
            Err(e) => {
                error!("Failed to send a walk step: {e}");
                Step::Error
            }
        };
    }
    Step::Nothing
}

/// Fallback step when A* has no leg, respecting known obstacles except for sealed-cell rescue.
async fn nudge(
    state: &Arc<Mutex<SharedState>>,
    goal: (f32, f32),
    stop_dist: f32,
    background: bool,
    sprint: Option<bool>,
) -> Step {
    let mut s = state.lock().await;
    let Some(me) = s.self_player.as_ref().map(|p| p.position) else {
        return Step::Error;
    };
    let to_goal = PlanarDelta::to_xz(&me, goal.0, goal.1);
    if to_goal.dist <= stop_dist.max(0.1) {
        return Step::Nothing;
    }
    let dist = (to_goal.dist - stop_dist).min(MAX_STEP_DIST);
    let ratio = dist / to_goal.dist;
    let floor = s.passability_floor();
    let (x, z) = (me.x + to_goal.dx * ratio, me.z + to_goal.dz * ratio);
    {
        let world = s.world_cache.read().unwrap();
        let cache = world.passability_cache();
        if pathfinding::is_movement_blocked(cache, me.x, me.z, x, z, floor, None)
            && world.is_walkable(me.x, me.z, floor)
        {
            return Step::Nothing;
        }
    }
    let turn_ms = s.mount_turn_delay_ms(to_goal.rotation());
    match s
        .send_step(x, z, floor, to_goal.rotation(), background, sprint)
        .await
    {
        Ok(sprinting) => {
            debug!("A* had no leg to walk — nudging {dist:.1}m on");
            Step::Sent(turn_ms + travel_ms(dist, sprinting, s.movement_speed_mult()))
        }
        Err(e) => {
            error!("Failed to send a nudge step: {e}");
            Step::Error
        }
    }
}

/// A shut door standing between us and where we want to go: how to open it,
/// and the cell centers on either side — one of them is our side.
struct DoorCandidate {
    label: String,
    toggle: ClientMessage,
    sides: [(f32, f32); 2],
}

/// Walk to the nearest shut door on our floor that we can actually reach and
/// open it. Returns false when no such door exists — then the goal really is
/// unreachable. Covers dungeon corridor doors and house doors alike: a shut
/// front door leaves a resident NPC with no route out of their own house just
/// as surely as a shut crypt door hides the stairs down.
async fn open_blocking_door(
    state: &Arc<Mutex<SharedState>>,
    background: bool,
    sprint: Option<bool>,
) -> bool {
    let Some((door, route)) = pick_reachable_door(state).await else {
        return false;
    };

    // The probe already proved this route; walk the waypoints it found rather
    // than paying for the same search again.
    let corrections = state.lock().await.position_corrections;
    let mut leg = 0usize;
    loop {
        match step_along(state, &route, &mut leg, background, sprint).await {
            Step::Sent(ms) => tokio::time::sleep(Duration::from_millis(ms.max(50))).await,
            Step::Nothing => break,
            Step::Error => return false,
        }
        if state.lock().await.position_corrections != corrections {
            info!("Walk to {} refused by the server", door.label);
            return false;
        }
    }

    info!("Opening {} to get through", door.label);
    let mut s = state.lock().await;
    if let Err(e) = s.send_flagged_command(door.toggle, background).await {
        error!("Failed to send door toggle: {e}");
        return false;
    }
    drop(s);
    // The server's reply (DoorToggled / DungeonDoorToggled) is what reopens the
    // cells for A*; re-pathing before it lands would just find the same wall.
    tokio::time::sleep(DOOR_TOGGLE_WAIT).await;
    true
}

/// The closest shut door on our floor with a side we can path to, and the route
/// there. Opening it widens the reachable set; if the goal is still walled off,
/// the next round picks the next one (this one no longer counts as shut).
///
/// Each probe is a full path search, so the candidates are filtered by distance
/// and sorted nearest-first before any of them runs — the door in our way is
/// normally the first, and the rest are never searched for.
async fn pick_reachable_door(
    state: &Arc<Mutex<SharedState>>,
) -> Option<(DoorCandidate, Vec<PathWaypoint>)> {
    let s = state.lock().await;
    let position = s.self_player.as_ref()?.position;
    let floor = s.passability_floor();
    let reach = |x: f32, z: f32| PlanarDelta::xz(position.x, position.z, x, z).dist;

    let mut doors = closed_doors_on_our_floor(&s);
    let mut sides: Vec<(f32, usize, (f32, f32))> = doors
        .iter()
        .enumerate()
        .flat_map(|(i, door)| door.sides.map(|side| (reach(side.0, side.1), i, side)))
        .filter(|(dist, _, _)| *dist <= MAX_DOOR_SEARCH_DIST)
        .collect();
    sides.sort_by(|a, b| a.0.total_cmp(&b.0));

    for (_, index, side) in sides.into_iter().take(MAX_DOOR_PROBES) {
        let route = s.find_path_to(side.0, side.1, floor);
        if route.found {
            return Some((doors.swap_remove(index), route.waypoints));
        }
    }
    None
}

/// The shut doors on our dungeon floor and whether we hold the floor's key.
fn dungeon_doors_here(s: &SharedState) -> Option<(Arc<Dungeon>, u8, Vec<DoorApproach>, bool)> {
    let dungeon = s.dungeon_here()?;
    let depth = s.self_floor_level.unsigned_abs();
    let open = s
        .world_cache
        .read()
        .unwrap()
        .open_dungeon_doors(&dungeon.id, depth);
    let doors = dungeon.closed_doors(depth, &open);
    let has_key = s.holds_item(&dungeon.key_item_id(depth));
    Some((dungeon, depth, doors, has_key))
}

/// Whether the way on is a shut locked door we hold no key for — the one
/// wall a walk cannot open.
fn locked_door_without_key(s: &SharedState) -> bool {
    dungeon_doors_here(s)
        .is_some_and(|(_, _, doors, has_key)| !has_key && doors.iter().any(|d| d.locked))
}

/// Every shut door on the floor we stand on: dungeon corridor doors when we
/// are underground, house doors when we are not. The server refuses a locked
/// door's toggle without the key, so those are left out.
fn closed_doors_on_our_floor(s: &SharedState) -> Vec<DoorCandidate> {
    if s.self_floor_level < 0 {
        let Some((dungeon, depth, doors, has_key)) = dungeon_doors_here(s) else {
            return Vec::new();
        };
        return doors
            .into_iter()
            .filter(|d| !d.locked || has_key)
            .map(|d| DoorCandidate {
                label: format!("dungeon door {} on floor {depth}", d.door_id),
                toggle: ClientMessage::ToggleDungeonDoor {
                    entrance_id: dungeon.id.clone(),
                    depth,
                    door_id: d.door_id,
                },
                sides: d.sides,
            })
            .collect();
    }

    let floor = s.self_floor_level as u8;
    let world = s.world_cache.read().unwrap();
    let mut out = Vec::new();
    for house in world.houses().values() {
        // Cells are indexed from the house origin; the floor grid's own origin
        // cancels out (see `pathfinding::update_door_edge`).
        let ox = house.origin.x.floor() as i32;
        let oz = house.origin.z.floor() as i32;
        for (room_index, room) in house.rooms.iter().enumerate() {
            if room.floor_level != floor {
                continue;
            }
            for dir in [
                WallDirection::North,
                WallDirection::South,
                WallDirection::East,
                WallDirection::West,
            ] {
                for (seg, wall) in room.wall(dir).iter().enumerate() {
                    // Windows are openable too, but they are not a way through.
                    if wall.variant != WallVariant::WithDoor || wall.is_open {
                        continue;
                    }
                    let ((dx, dz, _), (adx, adz, _)) = pathfinding::door_cells(room, dir, seg);
                    let (rx, rz) = (ox + room.local_x, oz + room.local_z);
                    out.push(DoorCandidate {
                        label: format!("{} door (room {room_index}, {dir:?} {seg})", house.id),
                        toggle: ClientMessage::ToggleDoor {
                            house_id: house.id.clone(),
                            room_index: room_index as u32,
                            wall_dir: dir,
                            segment_index: seg as u32,
                        },
                        sides: [
                            ((rx + dx) as f32 + 0.5, (rz + dz) as f32 + 0.5),
                            ((rx + adx) as f32 + 0.5, (rz + adz) as f32 + 0.5),
                        ],
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::{test_player, test_state};
    use onlinerpg_shared::fence::{Fence, FenceAxis, FenceEdge};
    use onlinerpg_shared::housing::{HouseData, PassabilityGrid};
    use onlinerpg_shared::ServerMessage;

    #[tokio::test(start_paused = true)]
    async fn corrected_mounted_walk_waits_for_recovery_then_replans_once() {
        for success in [true, false] {
            let (s, mut rx) = fenced_in_at(-1360.5, 4500.5, true);
            let start = s.self_player.as_ref().unwrap().position;
            let rotation = s.self_player.as_ref().unwrap().rotation;
            let state = Arc::new(Mutex::new(s));
            let to = WalkTo::Place {
                x: -1356.5,
                z: 4500.5,
                floor: 0,
            };
            let walking = walk(&state, &to, false, Some(true));
            tokio::pin!(walking);
            let mut corrected = false;
            let mut recoveries = 0;
            let result = loop {
                tokio::select! {
                    biased;
                    Some(command) = rx.recv() => match command {
                        ClientMessage::PlayerMove { sprinting, .. } => {
                            if !corrected {
                                state.lock().await.push_event(ServerMessage::PositionCorrected { position: start, rotation, floor_level: 0 });
                                corrected = true;
                            } else {
                                assert_eq!(recoveries, 1);
                                assert!(!sprinting);
                            }
                        }
                        ClientMessage::PlayerMountRecover { request_id, .. } => {
                            recoveries += 1;
                            assert_eq!(recoveries, 1);
                            let position = Position { z: start.z - 0.5, ..start };
                            state.lock().await.push_event(ServerMessage::MountRecovery {
                                request_id, position, rotation, floor_level: 0, done: false, success: true,
                            });
                            assert!(rx.try_recv().is_err());
                            state.lock().await.push_event(ServerMessage::MountRecovery {
                                request_id, position, rotation, floor_level: 0, done: true, success,
                            });
                        }
                        other => panic!("unexpected command: {other:?}"),
                    },
                    result = &mut walking => break result,
                }
            };
            assert_eq!(recoveries, 1);
            assert_eq!(
                result,
                if success {
                    Walked::Arrived
                } else {
                    Walked::Lost(LostReason::Desynced)
                }
            );
            assert!(state.lock().await.self_player.as_ref().unwrap().mounted);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn mount_recovery_waits_for_the_completion_ack() {
        let (s, mut rx) = fenced_in_at(-1360.5, 4500.5, true);
        let state = Arc::new(Mutex::new(s));
        let copy = state.clone();
        let recovery =
            tokio::spawn(async move { recover_mount(&copy, (-1356.5, 4500.5), false).await });
        let ClientMessage::PlayerMountRecover { request_id, .. } = rx.recv().await.unwrap() else {
            panic!()
        };
        let position = state.lock().await.self_player.as_ref().unwrap().position;
        state.lock().await.push_event(ServerMessage::MountRecovery {
            request_id,
            position,
            rotation: 0.0,
            floor_level: 0,
            done: false,
            success: true,
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(!recovery.is_finished());
        assert!(rx.try_recv().is_err());
        state.lock().await.push_event(ServerMessage::MountRecovery {
            request_id,
            position,
            rotation: 0.0,
            floor_level: 0,
            done: true,
            success: true,
        });
        assert!(recovery.await.unwrap());
    }

    #[tokio::test(start_paused = true)]
    async fn mount_recovery_times_out_and_ignores_late_progress_after_new_movement() {
        let (s, mut rx) = fenced_in_at(-1360.5, 4500.5, true);
        let state = Arc::new(Mutex::new(s));
        assert!(!recover_mount(&state, (-1356.5, 4500.5), false).await);
        let ClientMessage::PlayerMountRecover { request_id, .. } = rx.try_recv().unwrap() else {
            panic!()
        };
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::PlayerMountTurn { stop: true, .. })
        ));
        let mut s = state.lock().await;
        let position = s.self_player.as_ref().unwrap().position;
        s.send_command(ClientMessage::player_move(position, 0.0, 0))
            .await
            .unwrap();
        let before = s.self_player.as_ref().unwrap().position;
        s.push_event(ServerMessage::MountRecovery {
            request_id,
            position: Position {
                x: before.x - 1.0,
                ..before
            },
            rotation: 1.0,
            floor_level: 0,
            done: true,
            success: true,
        });
        assert_eq!(s.self_player.as_ref().unwrap().position, before);
    }

    fn fenced_in_at(
        x: f32,
        z: f32,
        mounted: bool,
    ) -> (SharedState, tokio::sync::mpsc::Receiver<ClientMessage>) {
        let (mut state, rx) = test_state();
        let mut player = test_player(x, z);
        player.mounted = mounted;
        state.push_event(ServerMessage::JoinSuccess {
            player,
            is_admin: false,
        });
        let horizontal = (-1372..-1345).flat_map(|x| {
            [4480, 4512].map(|z| FenceEdge {
                x,
                z,
                axis: FenceAxis::X,
            })
        });
        let vertical = (4480..4512).flat_map(|z| {
            [-1372, -1345].map(|x| FenceEdge {
                x,
                z,
                axis: FenceAxis::Z,
            })
        });
        state.push_event(ServerMessage::FenceVisibility {
            added: horizontal
                .chain(vertical)
                .map(|edge| Fence {
                    edge,
                    y: 0.5,
                    owner_id: 1,
                })
                .collect(),
            removed: vec![],
        });
        (state, rx)
    }

    #[tokio::test(start_paused = true)]
    async fn unreachable_walks_do_not_nudge_through_known_fences() {
        for mounted in [false, true] {
            for (start, goal) in [
                ((-1371.8, 4511.4), (-1380.5, 4518.5)),
                ((-1371.1, 4480.1), (-1380.5, 4473.5)),
                ((-1361.507, 4507.2217), (-1380.5, 4518.5)),
                ((-1361.507, 4507.2217), (-1380.5, 4473.5)),
            ] {
                let (s, mut rx) = fenced_in_at(start.0, start.1, mounted);
                let path = s.find_path_to(goal.0, goal.1, 0);
                assert!(!path.found);
                let closest = path.waypoints.last().map_or(start, |wp| (wp.x, wp.z));
                assert!(s.cell_open(start.0, start.1, 0));
                let world_cache = s.world_cache.clone();
                let state = Arc::new(Mutex::new(s));
                let mut previous = start;
                for _ in 0..2 {
                    let to = WalkTo::Place {
                        x: goal.0,
                        z: goal.1,
                        floor: 0,
                    };
                    let walking = tokio::time::timeout(
                        Duration::from_secs(120),
                        walk(&state, &to, false, Some(false)),
                    );
                    tokio::pin!(walking);
                    let result = loop {
                        tokio::select! {
                            biased;
                            Some(command) = rx.recv() => {
                                let ClientMessage::PlayerMove { position, .. } = command else {
                                    panic!("unexpected command: {command:?}");
                                };
                                let world = world_cache.read().unwrap();
                                assert!(
                                    !pathfinding::is_movement_blocked(
                                        world.passability_cache(),
                                        previous.0,
                                        previous.1,
                                        position.x,
                                        position.z,
                                        0,
                                        None,
                                    ),
                                    "blocked step from {previous:?} to {position:?}",
                                );
                                previous = (position.x, position.z);
                            },
                            result = &mut walking => break result.unwrap(),
                        }
                    };
                    assert_eq!(result, Walked::Lost(LostReason::NoPath));
                    let s = state.lock().await;
                    let me = s.self_player.as_ref().unwrap();
                    assert_eq!((me.position.x, me.position.z), previous);
                    assert_eq!(previous, closest);
                }
            }
        }
    }

    #[tokio::test(start_paused = true)]
    async fn a_blocked_approach_waits_for_the_target_to_become_reachable() {
        let (mut s, mut rx) = fenced_in_at(-1371.8, 4511.4, false);
        let mut target = test_player(-1380.5, 4518.5);
        target.id = PlayerId::from(2);
        let target_id = target.id;
        s.nearby_players.insert(target_id, target);
        let state = Arc::new(Mutex::new(s));
        let to = WalkTo::Character(&target_id);
        let walking = walk(&state, &to, false, Some(false));
        tokio::pin!(walking);
        tokio::select! {
            biased;
            result = &mut walking => panic!("approach ended while waiting: {result:?}"),
            command = rx.recv() => panic!("approach crossed a known fence: {command:?}"),
            _ = tokio::time::sleep(Duration::from_secs(1)) => {}
        }
        {
            let mut s = state.lock().await;
            let position = s.self_player.as_ref().unwrap().position;
            s.nearby_players.get_mut(&target_id).unwrap().position = position;
        }
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), walking)
                .await
                .unwrap(),
            Walked::Arrived,
        );
        assert!(rx.try_recv().is_err());
    }

    /// A block of cells walled on every edge: A* will not leave one.
    fn boxed_in(x: f32, z: f32, floor_level: u8) -> HouseData {
        HouseData {
            id: "box".to_string(),
            owner_id: "test".to_string(),
            source_scroll_id: None,
            origin: Position { x, y: 0.0, z },
            rooms: Vec::new(),
            passability: vec![PassabilityGrid {
                floor_level,
                origin_x: 0,
                origin_z: 0,
                width: 2,
                depth: 2,
                cells: vec![1 | 2 | 4 | 8; 4],
            }],
        }
    }

    async fn boxed_in_at(
        x: f32,
        z: f32,
        floor_level: i8,
    ) -> (
        Arc<Mutex<SharedState>>,
        tokio::sync::mpsc::Receiver<ClientMessage>,
    ) {
        let (mut s, rx) = test_state();
        s.self_player = Some(test_player(x, z));
        s.self_player_id = Some(PlayerId::from(1));
        s.self_floor_level = floor_level;
        let floor = s.passability_floor();
        s.world_cache
            .write()
            .unwrap()
            .add_house(boxed_in(x.floor() - 1.0, z.floor() - 1.0, floor));
        assert!(
            !s.find_path_to(x + 30.0, z, floor).found,
            "A* has no leg here"
        );
        (Arc::new(Mutex::new(s)), rx)
    }

    // A refused step triggers the server's sealed-cell rescue and position correction.
    #[tokio::test(start_paused = true)]
    async fn a_walk_from_a_sealed_cell_still_requests_correction() {
        let (state, mut rx) = boxed_in_at(0.5, 0.5, 0).await;
        let floor = state.lock().await.passability_floor();
        let to = WalkTo::Place {
            x: 30.5,
            z: 0.5,
            floor,
        };
        assert_eq!(
            walk(&state, &to, false, Some(false)).await,
            Walked::Lost(LostReason::NoPath)
        );
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::PlayerMove { .. })),
            "a blocked walk must still tell the server where it wanted to go"
        );
    }

    /// Underground a missing route is a wall, not un-pathable ground.
    #[tokio::test(start_paused = true)]
    async fn a_walk_with_no_route_underground_sends_nothing() {
        let (state, mut rx) = boxed_in_at(0.5, 0.5, -1).await;
        let floor = state.lock().await.passability_floor();
        let to = WalkTo::Place {
            x: 30.5,
            z: 0.5,
            floor,
        };
        walk(&state, &to, false, Some(false)).await;
        assert!(rx.try_recv().is_err(), "no line walked through a dungeon");
    }
}
