use crate::housing::HousingIO;
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterAttributes, Player, PlayerId, ServerMessage};
use bytes::Bytes;
use onlinerpg_shared::housing::{RoomData, WallDirection, WallVariant};
use onlinerpg_shared::serialize_server_msg;
use onlinerpg_shared::Position;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, warn};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DoorKey {
    house_id: String,
    room_index: u32,
    wall_dir: WallDirection,
    segment_index: u32,
}

#[derive(Debug, Clone)]
pub struct BroadcastMessage {
    pub bytes: Bytes,
    /// If set, skip sending to this player (used for MonsterMoved owner filtering).
    pub skip_player_id: Option<PlayerId>,
}

pub type GameStateSender = broadcast::Sender<BroadcastMessage>;
pub type GameStateReceiver = broadcast::Receiver<BroadcastMessage>;

mod chat;
mod combat;
mod monster;
mod player;
mod time;

#[cfg(test)]
mod tests;

#[derive(Default)]
struct IdState {
    next_player_number: u32,
    player_numbers: HashMap<PlayerId, u32>,
    owner_spawn_counts: HashMap<u32, u32>,
}

#[derive(Clone)]
pub struct GameState {
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    monsters: Arc<RwLock<HashMap<String, crate::types::Monster>>>,
    broadcast_tx: GameStateSender,
    game_clock_start_real: Instant,
    game_clock_start_game_seconds: i64,
    monster_defs: MonsterDefs,
    id_state: Arc<RwLock<IdState>>,
    direct_channels: Arc<RwLock<HashMap<PlayerId, mpsc::UnboundedSender<ServerMessage>>>>,
    // player_id → (character_id, current_xp, attributes)
    player_characters: Arc<RwLock<HashMap<PlayerId, (i64, u64, CharacterAttributes)>>>,
    housing_io: Arc<HousingIO>,
    /// Players whose state has changed since the last periodic save.
    dirty_players: Arc<RwLock<HashSet<PlayerId>>>,
    /// In-memory set of currently open doors.
    open_doors: Arc<RwLock<HashSet<DoorKey>>>,
}

impl GameState {
    pub fn new(
        monster_defs: MonsterDefs,
        initial_datetime: crate::types::GameDateTime,
        housing_io: Arc<HousingIO>,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            monsters: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            game_clock_start_real: Instant::now(),
            game_clock_start_game_seconds: Self::datetime_to_total_game_seconds(&initial_datetime),
            monster_defs,
            id_state: Arc::new(RwLock::new(IdState::default())),
            direct_channels: Arc::new(RwLock::new(HashMap::new())),
            player_characters: Arc::new(RwLock::new(HashMap::new())),
            housing_io,
            dirty_players: Arc::new(RwLock::new(HashSet::new())),
            open_doors: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    pub fn subscribe(&self) -> GameStateReceiver {
        self.broadcast_tx.subscribe()
    }

    pub(crate) fn broadcast(&self, msg: ServerMessage, skip_player_id: Option<PlayerId>) {
        match serialize_server_msg(&msg) {
            Ok(bytes) => {
                let _ = self.broadcast_tx.send(BroadcastMessage {
                    bytes: Bytes::from(bytes),
                    skip_player_id,
                });
            }
            Err(e) => error!("Failed to serialize broadcast message: {}", e),
        }
    }

    /// Toggle a door's is_open state (in-memory only, no disk I/O).
    /// Validates that the player is within 1.5m (XZ) and on the same floor.
    pub async fn toggle_door(
        &self,
        player_id: &PlayerId,
        house_id: &str,
        room_index: u32,
        wall_dir: WallDirection,
        segment_index: u32,
    ) -> Option<bool> {
        let (player_pos, player_floor) = {
            let players = self.players.read().await;
            let p = players.get(player_id)?;
            (p.position.clone(), p.floor_level)
        };

        let house = match self.housing_io.find_house(house_id).await {
            Ok(Some(h)) => h,
            _ => {
                warn!("toggle_door: house {} not found", house_id);
                return None;
            }
        };

        let room = house.rooms.get(room_index as usize)?;

        // Validate door exists
        let seg = room.wall(wall_dir).get(segment_index as usize)?;
        if seg.variant != WallVariant::WithDoor && seg.variant != WallVariant::WithWindow {
            return None;
        }

        // Validate distance and floor
        if !is_player_near_door(
            room,
            &house.origin,
            wall_dir,
            segment_index,
            &player_pos,
            player_floor,
        ) {
            return None;
        }

        // Toggle in-memory state
        let key = DoorKey {
            house_id: house_id.to_string(),
            room_index,
            wall_dir,
            segment_index,
        };
        let mut open_doors = self.open_doors.write().await;
        let is_open = if open_doors.contains(&key) {
            open_doors.remove(&key);
            false
        } else {
            open_doors.insert(key);
            true
        };

        Some(is_open)
    }
}

const MAX_DOOR_DISTANCE: f32 = 2.0;

/// Check that the player is within range of a door and on the same floor.
fn is_player_near_door(
    room: &RoomData,
    house_origin: &Position,
    wall_dir: WallDirection,
    segment_index: u32,
    player_pos: &Position,
    player_floor: i8,
) -> bool {
    // Floor check (-1 means outside, allow interaction with any floor)
    if player_floor != -1 && player_floor != room.floor_level as i8 {
        warn!(
            "toggle_door: wrong floor — player floor={} door floor={}",
            player_floor, room.floor_level
        );
        return false;
    }

    let seg_center = segment_index as f32 + 0.5;
    let local_x = room.local_x as f32;
    let local_z = room.local_z as f32;
    let size_x = room.size_x as f32;
    let size_z = room.size_z as f32;

    // Door world position (center of 1m segment along the wall)
    let (door_x, door_z) = match wall_dir {
        WallDirection::North => (local_x + seg_center, local_z),
        WallDirection::South => (local_x + seg_center, local_z + size_z),
        WallDirection::East => (local_x + size_x, local_z + seg_center),
        WallDirection::West => (local_x, local_z + seg_center),
    };
    let world_x = house_origin.x + door_x;
    let world_z = house_origin.z + door_z;

    // XZ distance check
    let dx = player_pos.x - world_x;
    let dz = player_pos.z - world_z;
    let dist_sq = dx * dx + dz * dz;
    if dist_sq > MAX_DOOR_DISTANCE * MAX_DOOR_DISTANCE {
        warn!(
            "toggle_door: too far — player ({:.1},{:.1}) door ({:.1},{:.1}) dist={:.2}",
            player_pos.x,
            player_pos.z,
            world_x,
            world_z,
            dist_sq.sqrt()
        );
        return false;
    }

    true
}
