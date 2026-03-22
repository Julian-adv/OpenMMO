use crate::auth::AuthService;
use crate::housing::HousingIO;
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterAttributes, Player, PlayerId, ServerMessage};
use bytes::Bytes;
use onlinerpg_shared::housing::WallVariant;
use onlinerpg_shared::serialize_server_msg;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{error, warn};

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
    auth_service: Arc<AuthService>,
    // player_id → (character_id, current_xp, attributes)
    player_characters: Arc<RwLock<HashMap<PlayerId, (i64, u64, CharacterAttributes)>>>,
    housing_io: Arc<HousingIO>,
}

impl GameState {
    pub fn new(
        monster_defs: MonsterDefs,
        initial_datetime: crate::types::GameDateTime,
        auth_service: Arc<AuthService>,
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
            auth_service,
            player_characters: Arc::new(RwLock::new(HashMap::new())),
            housing_io,
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

    /// Toggle a door's is_open state. Returns the new state, or None if invalid.
    pub async fn toggle_door(
        &self,
        house_id: &str,
        room_index: u32,
        wall_dir: &str,
        segment_index: u32,
    ) -> Option<bool> {
        let mut house = match self.housing_io.find_house(house_id).await {
            Ok(Some(h)) => h,
            _ => {
                warn!("toggle_door: house {} not found", house_id);
                return None;
            }
        };

        let room = house.rooms.get_mut(room_index as usize)?;
        let wall = match wall_dir {
            "north" => &mut room.wall_north,
            "south" => &mut room.wall_south,
            "east" => &mut room.wall_east,
            "west" => &mut room.wall_west,
            _ => return None,
        };

        let seg = wall.get_mut(segment_index as usize)?;
        if seg.variant != WallVariant::WithDoor {
            return None;
        }

        seg.is_open = !seg.is_open;
        let new_state = seg.is_open;

        if let Err(e) = self.housing_io.write_house(&house).await {
            error!("toggle_door: failed to save house {}: {}", house_id, e);
        }

        Some(new_state)
    }
}
