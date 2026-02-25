use crate::auth::AuthService;
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterAttributes, Player, PlayerId, ServerMessage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};

pub type GameStateSender = broadcast::Sender<ServerMessage>;
pub type GameStateReceiver = broadcast::Receiver<ServerMessage>;

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
}

impl GameState {
    pub fn new(
        monster_defs: MonsterDefs,
        initial_datetime: crate::types::GameDateTime,
        auth_service: Arc<AuthService>,
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
        }
    }

    pub fn subscribe(&self) -> GameStateReceiver {
        self.broadcast_tx.subscribe()
    }
}
