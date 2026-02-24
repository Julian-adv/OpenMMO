use crate::auth::AuthService;
use crate::game::{character_hp, combat, xp};
use crate::monster_defs::MonsterDefs;
use crate::types::{CharacterAttributes, GameDateTime, Player, PlayerId, Position, ServerMessage};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, RwLock};
use tracing::{info, warn};

pub type GameStateSender = broadcast::Sender<ServerMessage>;
pub type GameStateReceiver = broadcast::Receiver<ServerMessage>;
const REAL_DAY_DURATION_SECONDS: f64 = 3.0 * 60.0 * 60.0;
const GAME_HOURS_PER_DAY: i64 = 24;
const GAME_MINUTES_PER_HOUR: i64 = 60;
const GAME_DAYS_PER_MONTH: i64 = 30;
const GAME_MONTHS_PER_YEAR: i64 = 12;
const GAME_DAYS_PER_YEAR: i64 = GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR;
const GAME_START_YEAR: i64 = 217;
const GAME_SECONDS_PER_REAL_SECOND: f64 =
    (GAME_HOURS_PER_DAY as f64 * GAME_MINUTES_PER_HOUR as f64 * 60.0) / REAL_DAY_DURATION_SECONDS;

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
    monster_last_attack_at: Arc<RwLock<HashMap<String, Instant>>>,
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

#[cfg(test)]
mod tests;

impl GameState {
    pub fn default_start_datetime() -> GameDateTime {
        GameDateTime {
            year: GAME_START_YEAR as u32,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
        }
    }

    pub fn new(
        monster_defs: MonsterDefs,
        initial_datetime: GameDateTime,
        auth_service: Arc<AuthService>,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(1000);

        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            monsters: Arc::new(RwLock::new(HashMap::new())),
            monster_last_attack_at: Arc::new(RwLock::new(HashMap::new())),
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

    fn datetime_to_total_game_seconds(datetime: &GameDateTime) -> i64 {
        let year = i64::from(datetime.year).max(GAME_START_YEAR);
        let month = i64::from(datetime.month).clamp(1, GAME_MONTHS_PER_YEAR);
        let day = i64::from(datetime.day).clamp(1, GAME_DAYS_PER_MONTH);
        let hour = i64::from(datetime.hour).clamp(0, GAME_HOURS_PER_DAY - 1);
        let minute = i64::from(datetime.minute).clamp(0, GAME_MINUTES_PER_HOUR - 1);

        let years_since_start = year - GAME_START_YEAR;
        let total_days =
            years_since_start * GAME_DAYS_PER_YEAR + (month - 1) * GAME_DAYS_PER_MONTH + (day - 1);
        let total_minutes = total_days * GAME_HOURS_PER_DAY * GAME_MINUTES_PER_HOUR
            + hour * GAME_MINUTES_PER_HOUR
            + minute;
        total_minutes * 60
    }

    fn total_game_seconds_to_datetime(total_game_seconds: i64) -> GameDateTime {
        let total_seconds = total_game_seconds.max(0);
        let total_minutes = total_seconds / 60;
        let total_days = total_minutes / (GAME_HOURS_PER_DAY * GAME_MINUTES_PER_HOUR);

        let minutes_in_day = total_minutes % (GAME_HOURS_PER_DAY * GAME_MINUTES_PER_HOUR);
        let hour = (minutes_in_day / GAME_MINUTES_PER_HOUR) as u8;
        let minute = (minutes_in_day % GAME_MINUTES_PER_HOUR) as u8;

        let year = GAME_START_YEAR + (total_days / GAME_DAYS_PER_YEAR);
        let day_of_year = total_days % GAME_DAYS_PER_YEAR;
        let month = (day_of_year / GAME_DAYS_PER_MONTH) + 1;
        let day = (day_of_year % GAME_DAYS_PER_MONTH) + 1;

        GameDateTime {
            year: year as u32,
            month: month as u8,
            day: day as u8,
            hour,
            minute,
        }
    }

    fn current_total_game_seconds(&self) -> i64 {
        let elapsed_real_seconds = self.game_clock_start_real.elapsed().as_secs_f64();
        let elapsed_game_seconds =
            (elapsed_real_seconds * GAME_SECONDS_PER_REAL_SECOND).floor() as i64;
        self.game_clock_start_game_seconds + elapsed_game_seconds
    }

    pub fn current_game_datetime(&self) -> GameDateTime {
        Self::total_game_seconds_to_datetime(self.current_total_game_seconds())
    }

    pub fn is_night(datetime: &GameDateTime) -> bool {
        crate::celestial::is_night(datetime)
    }

    pub fn broadcast_game_time(&self) -> GameDateTime {
        let datetime = self.current_game_datetime();
        let _ = self.broadcast_tx.send(ServerMessage::GameTimeSync {
            is_night: Self::is_night(&datetime),
            datetime: datetime.clone(),
        });
        datetime
    }

    async fn get_or_assign_player_number(&self, player_id: &str) -> u32 {
        let mut id_state = self.id_state.write().await;
        if let Some(player_number) = id_state.player_numbers.get(player_id).copied() {
            player_number
        } else {
            id_state.next_player_number = id_state.next_player_number.saturating_add(1);
            let player_number = id_state.next_player_number;
            id_state
                .player_numbers
                .insert(player_id.to_string(), player_number);
            player_number
        }
    }

    pub fn subscribe(&self) -> GameStateReceiver {
        self.broadcast_tx.subscribe()
    }

    pub async fn register_direct_channel(
        &self,
        player_id: &PlayerId,
    ) -> mpsc::UnboundedReceiver<ServerMessage> {
        let (tx, rx) = mpsc::unbounded_channel();
        let mut channels = self.direct_channels.write().await;
        channels.insert(player_id.clone(), tx);
        rx
    }

    pub async fn unregister_direct_channel(&self, player_id: &PlayerId) {
        let mut channels = self.direct_channels.write().await;
        channels.remove(player_id);
    }

    pub async fn send_direct_message(&self, player_id: &PlayerId, msg: ServerMessage) {
        let channels = self.direct_channels.read().await;
        if let Some(tx) = channels.get(player_id) {
            let _ = tx.send(msg);
        }
    }

    pub async fn register_player_character(
        &self,
        player_id: &PlayerId,
        character_id: i64,
        xp: u64,
        attributes: CharacterAttributes,
    ) {
        let mut map = self.player_characters.write().await;
        map.insert(player_id.clone(), (character_id, xp, attributes));
    }

    pub async fn unregister_player_character(&self, player_id: &PlayerId) {
        let mut map = self.player_characters.write().await;
        map.remove(player_id);
    }

    pub async fn kick_player_by_name(&self, name: &str) -> Option<PlayerId> {
        let old_player_id = {
            let players = self.players.read().await;
            players
                .iter()
                .find(|(_, p)| p.name == name)
                .map(|(id, _)| id.clone())
        };

        if let Some(ref player_id) = old_player_id {
            info!("Kicking existing player '{}' ({})", name, player_id);

            self.send_direct_message(
                player_id,
                ServerMessage::Kicked {
                    player_id: player_id.clone(),
                    reason: "Another session logged in with the same account".to_string(),
                },
            )
            .await;

            self.remove_player(player_id).await;
        }

        old_player_id
    }

    pub async fn add_player(&self, player: Player) -> Option<ServerMessage> {
        let player_id = player.id.clone();
        let player_name = player.name.clone();
        let player_number = self.get_or_assign_player_number(&player_id).await;

        {
            let mut players = self.players.write().await;
            players.insert(player_id.clone(), player.clone());
        }

        info!(
            "Player {} ({}) joined the game [#{}]",
            player_name, player_id, player_number
        );

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

        let removed_player_number = {
            let mut id_state = self.id_state.write().await;
            let removed = id_state.player_numbers.remove(player_id);
            if let Some(player_number) = removed {
                id_state.owner_spawn_counts.remove(&player_number);
            }
            removed
        };

        let mut players = self.players.write().await;

        if let Some(player) = players.remove(player_id) {
            info!(
                "Player {} ({}) left the game{}",
                player.name,
                player_id,
                removed_player_number
                    .map(|n| format!(" [#{}]", n))
                    .unwrap_or_default()
            );
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

        for monster_id in &owned_ids {
            monsters.remove(monster_id);
            info!(
                "Removed monster {} (owner {} disconnected)",
                monster_id, owner_id
            );
            let _ = self.broadcast_tx.send(ServerMessage::MonsterRemoved {
                monster_id: monster_id.clone(),
            });
        }

        let mut last_attack_at = self.monster_last_attack_at.write().await;
        for monster_id in &owned_ids {
            last_attack_at.remove(monster_id);
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
                        "Ignored respawn request for alive player {} ({}) HP: {}/{}",
                        player.name, player.id, player.health, player.max_health
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

    async fn apply_player_death_penalty(&self, player_id: &PlayerId) {
        let (character_id, old_xp, attributes) = {
            let map = self.player_characters.read().await;
            match map.get(player_id).cloned() {
                Some(entry) => entry,
                None => return,
            }
        };

        let penalty = xp::apply_death_penalty(old_xp);
        let progression_changed =
            penalty.new_xp != penalty.old_xp || penalty.new_level != penalty.old_level;
        if !progression_changed {
            return;
        }

        {
            let mut map = self.player_characters.write().await;
            if let Some(entry) = map.get_mut(player_id) {
                entry.1 = penalty.new_xp;
            }
        }

        let mut max_hp_for_db = None;
        let mut current_hp_for_msg = 0;
        let mut max_hp_for_msg = 0;
        let mut level_for_msg = penalty.new_level;

        {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(player_id) {
                player.level = penalty.new_level;

                if penalty.leveled_down {
                    let class_name = player.class.as_str();

                    let level_one_floor = match character_hp::level_one_max_hp(
                        character_hp::DEFAULT_CHARACTER_RACE,
                        class_name,
                        attributes.con,
                    ) {
                        Ok(value) => value,
                        Err(err) => {
                            warn!(
                                "Failed to compute level 1 HP floor for player {}: {}",
                                player_id, err
                            );
                            1
                        }
                    };

                    match character_hp::roll_level_hp_delta(class_name, attributes.con) {
                        Ok(hp_loss) => {
                            let candidate = i64::from(player.max_health) - i64::from(hp_loss);
                            let bounded = candidate
                                .max(i64::from(level_one_floor))
                                .clamp(1, i64::from(u32::MAX))
                                as u32;

                            if bounded != player.max_health {
                                player.max_health = bounded;
                                max_hp_for_db = Some(bounded);
                            }
                        }
                        Err(err) => {
                            warn!(
                                "Failed to roll level-down HP delta for player {}: {}",
                                player_id, err
                            );
                        }
                    }
                }

                if player.health > player.max_health {
                    player.health = player.max_health;
                }

                current_hp_for_msg = player.health;
                max_hp_for_msg = player.max_health;
                level_for_msg = player.level;
            }
        }

        let auth = self.auth_service.clone();
        let new_xp = penalty.new_xp;
        let new_level = level_for_msg;
        tokio::task::spawn_blocking(move || {
            let result = if let Some(max_hp) = max_hp_for_db {
                auth.update_character_xp_level_and_max_hp(character_id, new_xp, new_level, max_hp)
            } else {
                auth.update_character_xp_and_level(character_id, new_xp, new_level)
            };
            if let Err(e) = result {
                tracing::warn!("Failed to persist death penalty: {}", e);
            }
        });

        self.send_direct_message(
            player_id,
            ServerMessage::XpGained {
                player_id: player_id.clone(),
                xp_amount: 0,
                xp_lost: penalty.old_xp.saturating_sub(penalty.new_xp),
                total_xp: penalty.new_xp,
                new_level: level_for_msg,
                leveled_up: false,
                max_hp: max_hp_for_msg,
                current_hp: current_hp_for_msg,
            },
        )
        .await;

        info!(
            "Player {} death penalty: XP {} -> {} (penalty {}), level {} -> {}{}",
            player_id,
            penalty.old_xp,
            penalty.new_xp,
            penalty.xp_penalty,
            penalty.old_level,
            level_for_msg,
            if penalty.leveled_down {
                ", level down"
            } else {
                ""
            }
        );
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

        let player_name = {
            let players = self.players.read().await;
            players.get(player_id).map(|p| p.name.clone())
        };

        if let Some(player_name) = player_name {
            info!("Player {} attacking monster {}", player_name, monster_id);

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

                    // Award XP to the player who killed the monster
                    let xp_def = self.monster_defs.get(&monster_type);
                    if let Some(def) = xp_def {
                        let xp_amount = xp::monster_xp(def.level, def.guard);
                        let player_char = {
                            let map = self.player_characters.read().await;
                            map.get(player_id).cloned()
                        };
                        if let Some((character_id, old_xp, attributes)) = player_char {
                            let new_xp = old_xp + xp_amount as u64;
                            let old_level = xp::level_from_xp(old_xp);
                            let new_level = xp::level_from_xp(new_xp);
                            let leveled_up = new_level > old_level;
                            let levels_gained = new_level.saturating_sub(old_level);

                            // Update in-memory XP
                            {
                                let mut map = self.player_characters.write().await;
                                if let Some(entry) = map.get_mut(player_id) {
                                    entry.1 = new_xp;
                                }
                            }

                            // Update level/max HP in player map if leveled up
                            let mut new_max_hp = None;
                            let mut new_current_hp = None;
                            if leveled_up {
                                let mut players_write = self.players.write().await;
                                if let Some(p) = players_write.get_mut(player_id) {
                                    p.level = new_level;
                                    let mut updated_max_hp = p.max_health;
                                    let class_name = p.class.as_str().to_string();
                                    for _ in 0..levels_gained {
                                        match character_hp::level_up_max_hp(
                                            updated_max_hp,
                                            &class_name,
                                            attributes.con,
                                        ) {
                                            Ok(next_max_hp) => {
                                                updated_max_hp = next_max_hp;
                                            }
                                            Err(err) => {
                                                warn!(
                                                    "Failed to roll level-up HP for player {}: {}",
                                                    player_id, err
                                                );
                                                break;
                                            }
                                        }
                                    }

                                    if updated_max_hp != p.max_health {
                                        p.max_health = updated_max_hp;
                                        new_max_hp = Some(updated_max_hp);
                                    }

                                    // Level-up always fully restores current HP to max HP.
                                    p.health = p.max_health;
                                    new_current_hp = Some(p.health);
                                }
                            }

                            // Persist to DB
                            let auth = self.auth_service.clone();
                            let new_max_hp_for_db = new_max_hp;
                            tokio::task::spawn_blocking(move || {
                                let result = if let Some(max_hp) = new_max_hp_for_db {
                                    auth.update_character_xp_level_and_max_hp(
                                        character_id,
                                        new_xp,
                                        new_level,
                                        max_hp,
                                    )
                                } else {
                                    auth.update_character_xp_and_level(
                                        character_id,
                                        new_xp,
                                        new_level,
                                    )
                                };
                                if let Err(e) = result {
                                    tracing::warn!("Failed to persist XP: {}", e);
                                }
                            });

                            // Notify the player directly
                            let max_hp_for_msg = if let Some(max_hp) = new_max_hp {
                                max_hp
                            } else {
                                self.players
                                    .read()
                                    .await
                                    .get(player_id)
                                    .map(|p| p.max_health)
                                    .unwrap_or(0)
                            };
                            let current_hp_for_msg = if let Some(current_hp) = new_current_hp {
                                current_hp
                            } else {
                                self.players
                                    .read()
                                    .await
                                    .get(player_id)
                                    .map(|p| p.health)
                                    .unwrap_or(0)
                            };
                            self.send_direct_message(
                                player_id,
                                ServerMessage::XpGained {
                                    player_id: player_id.clone(),
                                    xp_amount,
                                    xp_lost: 0,
                                    total_xp: new_xp,
                                    new_level,
                                    leveled_up,
                                    max_hp: max_hp_for_msg,
                                    current_hp: current_hp_for_msg,
                                },
                            )
                            .await;

                            info!(
                                "Player {} gained {} XP (total: {}, level: {}{})",
                                player_id,
                                xp_amount,
                                new_xp,
                                new_level,
                                if leveled_up { " LEVEL UP!" } else { "" }
                            );
                        }
                    }

                    // Schedule removal after 30 seconds
                    let game_state = self.clone();
                    let id_to_remove = monster_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                        let mut monsters = game_state.monsters.write().await;
                        if let Some(monster) = monsters.get(&id_to_remove) {
                            if monster.state == "dead" {
                                monsters.remove(&id_to_remove);
                                game_state
                                    .monster_last_attack_at
                                    .write()
                                    .await
                                    .remove(&id_to_remove);
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
        attacker_player_id: &PlayerId,
        monster_id: &str,
        target_player_id: &str,
    ) {
        // 1. Check if monster exists, is alive, and is owned by the requester.
        let monster_type = {
            let monsters = self.monsters.read().await;
            let monster = monsters.get(monster_id);
            if monster.is_none() || monster.unwrap().state == "dead" {
                return;
            }
            if monster.unwrap().owner_id.as_deref() != Some(attacker_player_id) {
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
        let attack_cooldown_ms = def.map(|d| u64::from(d.attack_cooldown)).unwrap_or(1500);

        // Server-side cooldown guard to prevent duplicate attack resolution
        // if multiple identical MonsterAttack requests arrive close together.
        {
            let now = Instant::now();
            let mut last_attack_at = self.monster_last_attack_at.write().await;
            if let Some(last) = last_attack_at.get(monster_id) {
                if now.duration_since(*last).as_millis() < u128::from(attack_cooldown_ms) {
                    return;
                }
            }
            last_attack_at.insert(monster_id.to_string(), now);
        }

        let result = combat::roll_attack(hit_threshold, damage_roll);

        info!(
            "Monster {} attacks player {}: Roll {}, Hit: {}, Damage: {}",
            monster_id, target_player_id, result.roll, result.hit, result.damage
        );

        // If hit, update player HP first so subsequent respawn checks observe the new state.
        let mut did_die = false;
        let mut current_health = 0;
        if result.hit {
            let mut players = self.players.write().await;

            if let Some(player) = players.get_mut(target_player_id) {
                if player.health == 0 {
                    return; // Already dead
                }

                player.health = player.health.saturating_sub(result.damage);
                current_health = player.health;
                info!(
                    "Player {} HP: {}/{}",
                    target_player_id, player.health, player.max_health
                );

                if player.health == 0 {
                    did_die = true;
                    info!("Player {} died", target_player_id);
                }
            }
        } else {
            // If missed, we still need current health for the message.
            let players = self.players.read().await;
            if let Some(player) = players.get(target_player_id) {
                current_health = player.health;
            }
        }

        // Send attack result after server-side HP update.
        let _ = self
            .broadcast_tx
            .send(ServerMessage::MonsterAttackedPlayer {
                monster_id: monster_id.to_string(),
                player_id: target_player_id.to_string(),
                hit: result.hit,
                roll: result.roll,
                damage: result.damage,
                current_health,
            });

        if did_die {
            let dead_player_id = target_player_id.to_string();
            self.apply_player_death_penalty(&dead_player_id).await;
            let _ = self.broadcast_tx.send(ServerMessage::PlayerDead {
                player_id: dead_player_id,
            });
        }
    }

    pub async fn spawn_monster(
        &self,
        monster_type: String,
        position: Position,
        rotation: f32,
        owner_id: Option<String>,
    ) {
        let owner_number = match owner_id.as_deref() {
            Some(owner_id) => self.get_or_assign_player_number(owner_id).await,
            None => 0,
        };
        let spawn_count = {
            let mut id_state = self.id_state.write().await;
            let counter = id_state.owner_spawn_counts.entry(owner_number).or_insert(0);
            *counter = counter.saturating_add(1);
            *counter
        };
        let id = format!("m{}_{}", owner_number, spawn_count);

        let mut monsters = self.monsters.write().await;
        if monsters.len() >= 10 {
            warn!("Monster spawn rejected: limit reached ({})", monsters.len());
            return;
        }

        let def = self.monster_defs.get(&monster_type);
        let health = def.map(|d| d.health).unwrap_or(10);
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
        info!(
            "Spawned monster {} [owner #{}, spawn #{}] (Total: {})",
            id,
            owner_number,
            spawn_count,
            monsters.len()
        );

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
