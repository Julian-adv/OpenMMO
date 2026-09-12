use crate::types::{Player, PlayerId, Position};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const HISTORY_LIMIT: usize = 16;
const DETAIL_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Pose {
    pub position: Position,
    pub rotation: f32,
    pub floor: i8,
    pub mounted: bool,
}

impl From<&Player> for Pose {
    fn from(player: &Player) -> Self {
        Self {
            position: player.position,
            rotation: player.rotation,
            floor: player.floor_level,
            mounted: player.mounted,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub(super) struct Request {
    pub id: u64,
    pub received_ms: u64,
    pub queued_ms: u64,
    pub corrections_issued_before_queue: u64,
    pub raw: super::player::MoveCommand,
    pub received_pose: Pose,
    pub leg_start: Position,
    pub leg_floor: i8,
    pub target: Position,
    pub queue_before: usize,
    pub replaced: bool,
    pub dropped: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct Tick {
    pub at_ms: u64,
    pub first_request_id: Option<u64>,
    pub last_request_id: Option<u64>,
    pub dt: f32,
    pub speed: f32,
    pub from: Pose,
    pub to: Pose,
    pub outcome: &'static str,
    pub queue_remaining: usize,
}

#[derive(Clone, Copy, Debug, Serialize)]
struct Correction {
    at_ms: u64,
    position: Position,
    floor: i8,
}

#[derive(Default)]
struct History {
    requests: VecDeque<Request>,
    ticks: VecDeque<Tick>,
    requests_evicted: u64,
    ticks_evicted: u64,
    corrections: u64,
    last_correction: Option<Correction>,
    last_detail: Option<Instant>,
}

impl History {
    fn detail_due(&self, now: Instant) -> bool {
        self.last_detail
            .is_none_or(|at| now.saturating_duration_since(at) >= DETAIL_INTERVAL)
    }
}

#[derive(Serialize)]
pub(super) struct Snapshot {
    requests: Vec<Request>,
    ticks: Vec<Tick>,
    requests_evicted: u64,
    ticks_evicted: u64,
    corrections: u64,
    last_correction: Option<Correction>,
}

#[derive(Default)]
pub(super) struct MovementAudit {
    histories: Mutex<HashMap<PlayerId, History>>,
}

impl MovementAudit {
    pub fn register(&self, id: PlayerId) {
        self.histories
            .lock()
            .expect("movement audit")
            .entry(id)
            .or_default();
    }

    pub fn request(&self, id: PlayerId, mut request: Request) -> Request {
        let mut histories = self.histories.lock().expect("movement audit");
        let Some(history) = histories.get_mut(&id) else {
            return request;
        };
        request.corrections_issued_before_queue = history.corrections;
        if history.requests.len() == HISTORY_LIMIT {
            history.requests.pop_front();
            history.requests_evicted += 1;
        }
        history.requests.push_back(request);
        request
    }

    pub fn tick(&self, id: PlayerId, tick: Tick) {
        let mut histories = self.histories.lock().expect("movement audit");
        let Some(history) = histories.get_mut(&id) else {
            return;
        };
        if history.ticks.len() == HISTORY_LIMIT {
            history.ticks.pop_front();
            history.ticks_evicted += 1;
        }
        history.ticks.push_back(tick);
    }

    pub fn correction(&self, id: PlayerId, position: Position, floor: i8) {
        let mut histories = self.histories.lock().expect("movement audit");
        let Some(history) = histories.get_mut(&id) else {
            return;
        };
        history.corrections += 1;
        history.last_correction = Some(Correction {
            at_ms: super::GameState::now_ms(),
            position,
            floor,
        });
    }

    pub fn detail_due(&self, id: PlayerId, now: Instant) -> bool {
        self.histories
            .lock()
            .expect("movement audit")
            .get(&id)
            .is_some_and(|history| history.detail_due(now))
    }

    pub fn snapshot(&self, id: PlayerId, now: Instant) -> Option<Snapshot> {
        let mut histories = self.histories.lock().expect("movement audit");
        let history = histories.get_mut(&id)?;
        if !history.detail_due(now) {
            return None;
        }
        history.last_detail = Some(now);
        Some(Snapshot {
            requests: history.requests.iter().copied().collect(),
            ticks: history.ticks.iter().cloned().collect(),
            requests_evicted: history.requests_evicted,
            ticks_evicted: history.ticks_evicted,
            corrections: history.corrections,
            last_correction: history.last_correction,
        })
    }

    pub fn remove(&self, id: &PlayerId) {
        self.histories.lock().expect("movement audit").remove(id);
    }
}

pub(super) fn next_request_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub(super) fn cells_near(
    entry: &onlinerpg_shared::pathfinding::RuntimePassability,
    floor: u8,
    position: Position,
) -> serde_json::Value {
    let x = (position.x - entry.house_origin_x).floor() as i32;
    let z = (position.z - entry.house_origin_z).floor() as i32;
    let grid = entry.floors.iter().find(|grid| grid.floor_level == floor);
    let cells = grid.map(|grid| {
        (z - 1..=z + 1)
            .flat_map(|cz| {
                (x - 1..=x + 1).map(move |cx| {
                    let gx = cx - grid.origin_x;
                    let gz = cz - grid.origin_z;
                    let mask =
                        (gx >= 0 && gz >= 0 && gx < grid.width as i32 && gz < grid.depth as i32)
                            .then(|| {
                                grid.cells
                                    .get(gz as usize * grid.width as usize + gx as usize)
                                    .copied()
                            })
                            .flatten();
                    serde_json::json!({"cell": [cx, cz], "mask": mask})
                })
            })
            .collect::<Vec<_>>()
    });
    serde_json::json!({"origin": [entry.house_origin_x, entry.house_origin_z], "cell": [x,z], "floor": floor,
        "y_base": grid.map(|g| g.y_base), "wall_height": grid.map(|g| g.wall_height), "neighbors": cells})
}
