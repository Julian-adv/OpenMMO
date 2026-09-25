use super::*;
use std::collections::{BTreeSet, HashMap};
use std::sync::{RwLock, Weak};
use tokio::sync::{OwnedRwLockReadGuard, OwnedRwLockWriteGuard};

const REGION_SIZE: f32 = 128.0;
const BODY_RADIUS: f32 = 0.31;

type StateLock = MovementMutex<Option<GoalMovement>>;

#[derive(Default)]
pub(in crate::game_state) struct MovementStates {
    entries: RwLock<HashMap<PlayerId, Arc<StateLock>>>,
    #[cfg(test)]
    pub(super) measurement: Arc<super::lock_measurement::LockMeasurement>,
}

impl MovementStates {
    fn new_entry(&self) -> Arc<StateLock> {
        #[cfg(test)]
        let lock = MovementMutex::measured(None, self.measurement.clone());
        #[cfg(not(test))]
        let lock = MovementMutex::new(None);
        Arc::new(lock)
    }

    pub(in crate::game_state) fn register(&self, id: PlayerId) {
        self.entries
            .write()
            .expect("movement state registry poisoned")
            .entry(id)
            .or_insert_with(|| self.new_entry());
    }

    pub(super) fn entry(&self, id: PlayerId) -> Arc<StateLock> {
        self.entries
            .read()
            .expect("movement state registry poisoned")
            .get(&id)
            .cloned()
            .unwrap_or_else(|| self.new_entry())
    }

    pub(in crate::game_state) async fn lock(&self, id: PlayerId) -> MovementStateGuard {
        self.entry(id).lock_owned().await
    }

    pub(in crate::game_state) fn remove(&self, id: &PlayerId) {
        self.entries
            .write()
            .expect("movement state registry poisoned")
            .remove(id);
    }

    pub(super) fn ids(&self) -> Vec<PlayerId> {
        self.entries
            .read()
            .expect("movement state registry poisoned")
            .keys()
            .copied()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::game_state) enum MovementRegion {
    Surface(i32, i32),
    Dungeon(String),
}

type RegionLock = tokio::sync::RwLock<()>;

#[derive(Default)]
pub(in crate::game_state) struct MovementRegions {
    entries: std::sync::Mutex<HashMap<MovementRegion, Weak<RegionLock>>>,
    #[cfg(test)]
    pub(super) measurement: Arc<super::lock_measurement::LockMeasurement>,
}

enum RegionGuard {
    Read { _guard: OwnedRwLockReadGuard<()> },
    Write { _guard: OwnedRwLockWriteGuard<()> },
}

pub(in crate::game_state) struct MovementRegionGuard {
    _guards: Vec<RegionGuard>,
    #[cfg(test)]
    _timing: Option<super::lock_measurement::LockTiming>,
}

impl MovementRegions {
    pub(super) async fn lock(
        &self,
        keys: &BTreeSet<MovementRegion>,
        exclusive: bool,
    ) -> MovementRegionGuard {
        let locks: Vec<_> = {
            let mut entries = self
                .entries
                .lock()
                .expect("movement region registry poisoned");
            if entries.len() > 1024 {
                entries.retain(|_, entry| entry.strong_count() > 0);
            }
            keys.iter()
                .map(|key| {
                    if let Some(lock) = entries.get(key).and_then(Weak::upgrade) {
                        return lock;
                    }
                    let lock = Arc::new(RegionLock::new(()));
                    entries.insert(key.clone(), Arc::downgrade(&lock));
                    lock
                })
                .collect()
        };
        #[cfg(test)]
        let started = self.measurement.start();
        let mut guards = Vec::with_capacity(locks.len());
        for lock in locks {
            guards.push(if exclusive {
                RegionGuard::Write {
                    _guard: lock.write_owned().await,
                }
            } else {
                RegionGuard::Read {
                    _guard: lock.read_owned().await,
                }
            });
        }
        MovementRegionGuard {
            _guards: guards,
            #[cfg(test)]
            _timing: self.measurement.acquired(started),
        }
    }
}

pub(in crate::game_state) struct LockedMovement {
    pub state: MovementStateGuard,
    pub regions: MovementRegionGuard,
    pub player: Player,
    pub at: Instant,
}

impl GameState {
    pub(in crate::game_state) fn movement_region_keys(
        &self,
        from: Position,
        to: Position,
        radius: f32,
    ) -> BTreeSet<MovementRegion> {
        use onlinerpg_shared::{WORLD_MIN_X, WORLD_WIDTH_X};
        let x = wrap_world_x(from.x);
        let end_x = x + shortest_world_delta_x(x, to.x);
        let (min_x, max_x) = (x.min(end_x) - radius, x.max(end_x) + radius);
        let (min_z, max_z) = (from.z.min(to.z) - radius, from.z.max(to.z) + radius);
        let mut keys = BTreeSet::new();
        let columns = (WORLD_WIDTH_X / REGION_SIZE) as i32;
        for cx in ((min_x - WORLD_MIN_X) / REGION_SIZE).floor() as i32
            ..=((max_x - WORLD_MIN_X) / REGION_SIZE).floor() as i32
        {
            for cz in (min_z / REGION_SIZE).floor() as i32..=(max_z / REGION_SIZE).floor() as i32 {
                keys.insert(MovementRegion::Surface(cx.rem_euclid(columns), cz));
            }
        }
        for entrance in self.dungeon_defs.all() {
            let (ox, oz) = dungeon::dungeon_origin(entrance.x, entrance.z);
            let grid = dungeon::GRID as f32;
            let ox = x + shortest_world_delta_x(x, ox + grid * 0.5) - grid * 0.5;
            if min_x <= ox + grid && max_x >= ox && min_z <= oz + grid && max_z >= oz {
                keys.insert(MovementRegion::Dungeon(entrance.id.clone()));
            }
        }
        keys
    }

    pub(super) fn movement_path_regions(
        &self,
        from: Position,
        points: &[MoveWaypoint],
    ) -> BTreeSet<MovementRegion> {
        let mut min = from;
        let mut max = from;
        for point in points {
            let x = from.x + shortest_world_delta_x(from.x, point.position.x);
            min.x = min.x.min(x);
            max.x = max.x.max(x);
            min.z = min.z.min(point.position.z);
            max.z = max.z.max(point.position.z);
        }
        self.movement_region_keys(min, max, BODY_RADIUS)
    }

    pub(in crate::game_state) fn house_movement_regions(
        &self,
        house: &onlinerpg_shared::housing::HouseData,
    ) -> BTreeSet<MovementRegion> {
        let mut keys = self.movement_region_keys(house.origin, house.origin, BODY_RADIUS);
        for room in &house.rooms {
            let from = Position {
                x: house.origin.x.floor() + room.local_x as f32,
                z: house.origin.z.floor() + room.local_z as f32,
                ..house.origin
            };
            let to = Position {
                x: from.x + room.size_x as f32,
                z: from.z + room.size_z as f32,
                ..from
            };
            keys.extend(self.movement_region_keys(from, to, BODY_RADIUS));
        }
        keys
    }

    pub(in crate::game_state) async fn lock_player_movement(
        &self,
        id: PlayerId,
        destinations: &[Position],
        radius: f32,
        exclusive: bool,
        advance_mult: Option<f32>,
    ) -> Option<LockedMovement> {
        let mut keys = BTreeSet::new();
        loop {
            let regions = self.movement_regions.lock(&keys, exclusive).await;
            let state = self.goal_moves.lock(id).await;
            let player = self.players.read().await.get(&id).cloned()?;
            let at = Instant::now();
            let mut reach = radius;
            if let (Some(mult), Some(direction)) = (
                advance_mult,
                state.as_ref().and_then(|s| s.direction.as_ref()),
            ) {
                let dt = at
                    .min(direction.expires_at)
                    .saturating_duration_since(direction.advanced_at)
                    .as_secs_f32();
                let distance =
                    move_speed(mult, true) * player.mount.map_or(1.0, |m| m.speed_mult()) * dt;
                reach = reach.max(distance);
            }
            let mut required =
                self.movement_region_keys(player.position, player.position, reach + BODY_RADIUS);
            for &to in destinations {
                required.extend(self.movement_region_keys(to, to, BODY_RADIUS));
            }
            if advance_mult.is_some() {
                if let Some(plan) = state.as_ref().and_then(|s| s.plan.as_ref()) {
                    required.extend(
                        self.movement_path_regions(player.position, &plan.waypoints[plan.next..]),
                    );
                }
            }
            if required.is_subset(&keys) {
                return Some(LockedMovement {
                    state,
                    regions,
                    player,
                    at,
                });
            }
            drop(state);
            drop(regions);
            keys = required;
        }
    }
}

#[cfg(test)]
pub(in crate::game_state) type MovementStateGuard =
    super::lock_measurement::MovementOwnedMutexGuard<Option<GoalMovement>>;
#[cfg(not(test))]
pub(in crate::game_state) type MovementStateGuard =
    tokio::sync::OwnedMutexGuard<Option<GoalMovement>>;
