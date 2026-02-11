use crate::game::combat;
use crate::monster_defs::MonsterDefs;
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
    monster_defs: MonsterDefs,
}

impl GameState {
    pub fn new(monster_defs: MonsterDefs) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            monsters: Arc::new(RwLock::new(HashMap::new())),
            broadcast_tx,
            monster_defs,
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

    pub async fn respawn_player(&self, player_id: &PlayerId) {
        let respawned_player = {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                if player.health > 0 {
                    info!(
                        "Ignored respawn request for alive player {} ({})",
                        player.name, player.id
                    );
                    return;
                }
                player.health = player.max_health;
                player.position = Position {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                };
                player.rotation = 0.0;
                Some(player.clone())
            } else {
                None
            }
        };

        if let Some(player) = respawned_player {
            info!("Player {} ({}) respawned", player.name, player.id);
            let _ = self
                .broadcast_tx
                .send(ServerMessage::PlayerRespawned { player });
        } else {
            warn!("Attempted to respawn non-existent player: {}", player_id);
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
        // 1. Check if monster exists and is alive first, get its type
        let monster_type = {
            let monsters = self.monsters.read().await;
            let monster = monsters.get(&monster_id);
            if monster.is_none() || monster.unwrap().state == "dead" {
                return;
            }
            monster.unwrap().monster_type.clone()
        };

        let players = self.players.read().await;

        if let Some(player) = players.get(player_id) {
            info!("Player {} attacking monster {}", player.name, monster_id);

            let def = self.monster_defs.get(&monster_type);
            let hit_threshold = def.map(|d| d.hit_threshold).unwrap_or(10);
            let damage_roll = def.map(|d| d.damage_roll.as_str()).unwrap_or("1d6");
            let result = combat::roll_attack(hit_threshold, damage_roll);

            info!(
                "Dice roll: {}, Hit: {}, Damage: {}",
                result.roll, result.hit, result.damage
            );

            // Send attack result
            let _ = self.broadcast_tx.send(ServerMessage::PlayerAttacked {
                player_id: player_id.clone(),
                monster_id: monster_id.clone(),
                hit: result.hit,
                roll: result.roll,
                damage: result.damage,
            });

            // If hit, update monster HP
            if result.hit {
                let mut monsters = self.monsters.write().await;
                let mut is_dead = false;

                if let Some(monster) = monsters.get_mut(&monster_id) {
                    if monster.state == "dead" {
                        return; // Already dead
                    }

                    monster.health = monster.health.saturating_sub(result.damage);
                    info!(
                        "Monster {} HP: {}/{}",
                        monster_id, monster.health, monster.max_health
                    );

                    if monster.health == 0 {
                        monster.state = "dead".to_string();
                        is_dead = true;
                    }
                }

                if is_dead {
                    info!("Monster {} died, broadcasting dead state", monster_id);
                    let _ = self.broadcast_tx.send(ServerMessage::MonsterDead {
                        monster_id: monster_id.clone(),
                    });

                    // Schedule removal after 30 seconds
                    let game_state = self.clone();
                    let id_to_remove = monster_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                        let mut monsters = game_state.monsters.write().await;
                        if let Some(monster) = monsters.get(&id_to_remove) {
                            if monster.state == "dead" {
                                monsters.remove(&id_to_remove);
                                info!("Monster {} removed after 30s corpse time", id_to_remove);
                                let _ =
                                    game_state.broadcast_tx.send(ServerMessage::MonsterRemoved {
                                        monster_id: id_to_remove,
                                    });
                            }
                        }
                    });
                }
            }
        } else {
            warn!("Attack from non-existent player: {}", player_id);
        }
    }

    pub async fn broadcast_monster_attack(
        &self,
        monster_id: &str,
        target_player_id: &str,
    ) {
        // 1. Check if monster exists and is alive, get its type
        let monster_type = {
            let monsters = self.monsters.read().await;
            let monster = monsters.get(monster_id);
            if monster.is_none() || monster.unwrap().state == "dead" {
                return;
            }
            monster.unwrap().monster_type.clone()
        };

        // 2. Check if target player exists and is alive
        {
            let players = self.players.read().await;
            match players.get(target_player_id) {
                Some(player) if player.health > 0 => {}
                _ => return,
            }
        }

        let def = self.monster_defs.get(&monster_type);
        let hit_threshold = def.map(|d| d.hit_threshold).unwrap_or(10);
        let damage_roll = def.map(|d| d.damage_roll.as_str()).unwrap_or("1d6");
        let result = combat::roll_attack(hit_threshold, damage_roll);

        info!(
            "Monster {} attacks player {}: Roll {}, Hit: {}, Damage: {}",
            monster_id, target_player_id, result.roll, result.hit, result.damage
        );

        // Send attack result
        let _ = self
            .broadcast_tx
            .send(ServerMessage::MonsterAttackedPlayer {
                monster_id: monster_id.to_string(),
                player_id: target_player_id.to_string(),
                hit: result.hit,
                roll: result.roll,
                damage: result.damage,
            });

        // If hit, update player HP
        if result.hit {
            let mut players = self.players.write().await;

            if let Some(player) = players.get_mut(target_player_id) {
                if player.health == 0 {
                    return; // Already dead
                }

                player.health = player.health.saturating_sub(result.damage);
                info!(
                    "Player {} HP: {}/{}",
                    target_player_id, player.health, player.max_health
                );

                if player.health == 0 {
                    info!("Player {} died", target_player_id);
                    let _ = self.broadcast_tx.send(ServerMessage::PlayerDead {
                        player_id: target_player_id.to_string(),
                    });
                }
            }
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

        let def = self.monster_defs.get(&monster_type);
        let health = def.map(|d| d.health).unwrap_or(10);

        let id = format!("monster_{}", Uuid::new_v4());
        let monster = crate::types::Monster {
            id: id.clone(),
            monster_type: monster_type.clone(),
            position,
            rotation,
            state: "idle".to_string(),
            owner_id,
            health,
            max_health: health,
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
            if monster.state == "dead" {
                return;
            }
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
