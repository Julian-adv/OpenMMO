use crate::types::{Player, PlayerId, Position, ServerMessage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

pub type GameStateSender = broadcast::Sender<ServerMessage>;
pub type GameStateReceiver = broadcast::Receiver<ServerMessage>;

#[derive(Clone)]
pub struct GameState {
    players: Arc<RwLock<HashMap<PlayerId, Player>>>,
    monsters: Arc<RwLock<HashMap<String, crate::types::Monster>>>,
    broadcast_tx: GameStateSender,
}

impl GameState {
    pub fn new() -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            monsters: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
        }
    }

    pub fn subscribe(&self) -> GameStateReceiver {
        self.broadcast_tx.subscribe()
    }

    pub async fn add_player(&self, player: Player) -> Option<ServerMessage> {
        let player_id = player.id.clone();
        let player_name = player.name.clone();

        {
            let mut players = self.players.write().await;
            players.insert(player_id.clone(), player.clone());
        }

        info!("Player {} ({}) joined the game", player_name, player_id);

        let _ = self
            .broadcast_tx
            .send(ServerMessage::PlayerJoined { player });

        // Return game_state to be sent directly to the new player only
        let current_players = self.players.read().await;
        let other_players: HashMap<String, Player> = current_players
            .iter()
            .filter(|(id, _)| *id != &player_id)
            .map(|(id, player)| (id.clone(), player.clone()))
            .collect();

        let monsters = self.monsters.read().await.clone();

        if !other_players.is_empty() || !monsters.is_empty() {
            return Some(ServerMessage::GameState {
                players: other_players,
                monsters,
            });
        }

        None
    }

    pub async fn remove_player(&self, player_id: &PlayerId) {
        self.remove_monsters_by_owner(player_id).await;

        let mut players = self.players.write().await;

        if let Some(player) = players.remove(player_id) {
            info!("Player {} ({}) left the game", player.name, player_id);
            let _ = self.broadcast_tx.send(ServerMessage::PlayerLeft {
                player_id: player_id.clone(),
            });
        } else {
            warn!("Attempted to remove non-existent player: {}", player_id);
        }
    }

    pub async fn remove_monsters_by_owner(&self, owner_id: &str) {
        let mut monsters = self.monsters.write().await;

        let owned_ids: Vec<String> = monsters
            .iter()
            .filter(|(_, m)| m.owner_id.as_deref() == Some(owner_id))
            .map(|(id, _)| id.clone())
            .collect();

        for monster_id in owned_ids {
            monsters.remove(&monster_id);
            info!(
                "Removed monster {} (owner {} disconnected)",
                monster_id, owner_id
            );
            let _ = self
                .broadcast_tx
                .send(ServerMessage::MonsterRemoved { monster_id });
        }
    }

    pub async fn update_player_position(
        &self,
        player_id: &PlayerId,
        new_position: Position,
        new_rotation: f32,
    ) {
        let mut players = self.players.write().await;

        if let Some(player) = players.get_mut(player_id) {
            player.position = new_position.clone();
            player.rotation = new_rotation;
            let _ = self.broadcast_tx.send(ServerMessage::PlayerMoved {
                player_id: player_id.clone(),
                position: new_position,
                rotation: new_rotation,
            });
        } else {
            warn!("Attempted to move non-existent player: {}", player_id);
        }
    }

    pub async fn send_chat_message(&self, player_id: &PlayerId, message: String) {
        let players = self.players.read().await;

        if let Some(player) = players.get(player_id) {
            info!("Chat message from {}: {}", player.name, message);
            let _ = self.broadcast_tx.send(ServerMessage::ChatMessage {
                player_id: player_id.clone(),
                message,
            });
        } else {
            warn!("Chat message from non-existent player: {}", player_id);
        }
    }

    pub async fn broadcast_player_attack(&self, player_id: &PlayerId, monster_id: String) {
        let players = self.players.read().await;

        if let Some(player) = players.get(player_id) {
            info!("Player {} attacking monster {}", player.name, monster_id);

            // Roll d20: 1-20
            let roll = rand::random::<u8>() % 20 + 1;
            let hit = roll > 10;

            info!("Dice roll: {}, Hit: {}", roll, hit);

            let _ = self.broadcast_tx.send(ServerMessage::PlayerAttacked {
                player_id: player_id.clone(),
                monster_id,
                hit,
                roll,
            });
        } else {
            warn!("Attack from non-existent player: {}", player_id);
        }
    }

    pub async fn spawn_monster(
        &self,
        monster_type: String,
        position: Position,
        rotation: f32,
        owner_id: Option<String>,
    ) {
        let mut monsters = self.monsters.write().await;

        if monsters.len() >= 10 {
            warn!("Monster spawn rejected: limit reached ({})", monsters.len());
            return;
        }

        let id = format!("monster_{}", Uuid::new_v4());
        let monster = crate::types::Monster {
            id: id.clone(),
            monster_type: monster_type.clone(),
            position,
            rotation,
            state: "idle".to_string(),
            owner_id,
        };

        monsters.insert(id.clone(), monster.clone());
        info!("Spawned monster {} (Total: {})", id, monsters.len());

        let _ = self
            .broadcast_tx
            .send(ServerMessage::MonsterSpawned { monster });
    }

    pub async fn update_monster_position(
        &self,
        monster_id: String,
        new_position: Position,
        rotation: f32,
        state: String,
        target_position: Position,
    ) {
        let mut monsters = self.monsters.write().await;

        if let Some(monster) = monsters.get_mut(&monster_id) {
            monster.position = new_position.clone();
            monster.rotation = rotation;
            monster.state = state.clone();

            let _ = self.broadcast_tx.send(ServerMessage::MonsterMoved {
                monster_id,
                position: new_position,
                rotation,
                state,
                target_position,
                owner_id: monster.owner_id.clone(),
            });
        }
    }

    #[allow(dead_code)]
    pub async fn get_player_count(&self) -> usize {
        self.players.read().await.len()
    }

    #[allow(dead_code)]
    pub async fn get_all_players(&self) -> HashMap<PlayerId, Player> {
        self.players.read().await.clone()
    }
}
