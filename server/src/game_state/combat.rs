use crate::game::{character_hp, combat, xp};
use crate::types::{PlayerId, ServerMessage};
use tracing::{info, warn};

impl super::GameState {
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

            // Calculate attack result
            let (result_hit, result_roll, result_damage) = {
                let def = self.monster_defs.get(&monster_type);
                let hit_threshold = def.map(|d| d.hit_threshold).unwrap_or(10);
                let damage_roll = def.map(|d| d.damage_roll.as_str()).unwrap_or("1d6");
                let result = combat::roll_attack(hit_threshold, damage_roll);
                (result.hit, result.roll, result.damage)
            };

            info!(
                "Dice roll: {}, Hit: {}, Damage: {}",
                result_roll, result_hit, result_damage
            );

            // Update player combat timestamp and damage logic
            {
                let mut players = self.players.write().await;
                if let Some(player) = players.get_mut(player_id) {
                    player.last_combat_at = Self::now_ms();
                }
            }

            // Send attack result
            self.broadcast(
                ServerMessage::PlayerAttacked {
                    player_id: player_id.clone(),
                    monster_id: monster_id.clone(),
                    hit: result_hit,
                    roll: result_roll,
                    damage: result_damage,
                },
                None,
            );

            if result_hit {
                let mut monsters = self.monsters.write().await;
                let mut is_dead = false;

                if let Some(monster) = monsters.get_mut(&monster_id) {
                    if monster.state == "dead" {
                        return; // Already dead
                    }

                    monster.health = monster.health.saturating_sub(result_damage);
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
                    self.broadcast(
                        ServerMessage::MonsterDead {
                            monster_id: monster_id.clone(),
                        },
                        None,
                    );

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
                                    for _ in 0..levels_gained {
                                        match character_hp::level_up_max_hp(
                                            updated_max_hp,
                                            &p.class,
                                            attributes.con,
                                        ) {
                                            Ok(next_max_hp) => {
                                                updated_max_hp = next_max_hp;
                                            }
                                            Err(err) => {
                                                warn!(
                                                    "Failed to roll level-up HP for player {}: {}",
                                                    player_name, err
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
                                let result = auth.update_character_xp_and_level(
                                    character_id,
                                    new_xp,
                                    new_level,
                                    new_max_hp_for_db,
                                );
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
                                player_name,
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
                                info!("Monster {} removed after 30s corpse time", id_to_remove);
                                game_state.broadcast(
                                    ServerMessage::MonsterRemoved {
                                        monster_id: id_to_remove,
                                    },
                                    None,
                                );
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
        // Also check server-side cooldown guard.
        let now = Self::now_ms();
        let mut monster_data = None;

        {
            let mut monsters = self.monsters.write().await;
            if let Some(monster) = monsters.get_mut(monster_id) {
                if monster.state != "dead"
                    && monster.owner_id.as_deref() == Some(attacker_player_id)
                {
                    let def = self.monster_defs.get(&monster.monster_type);
                    let attack_cooldown_ms =
                        def.map(|d| u64::from(d.attack_cooldown)).unwrap_or(1500);

                    if now.saturating_sub(monster.last_attack_at) >= attack_cooldown_ms {
                        monster.last_attack_at = now;
                        monster_data = Some((
                            monster.monster_type.clone(),
                            def.map(|d| d.hit_threshold).unwrap_or(10),
                            def.map(|d| d.damage_roll.as_str()).unwrap_or("1d6"),
                        ));
                    }
                }
            }
        }

        let (_monster_type, hit_threshold, damage_roll) = match monster_data {
            Some(data) => data,
            None => return,
        };

        // 2. Check if target player exists and is alive
        let target_player_name;
        {
            let players = self.players.read().await;
            match players.get(target_player_id) {
                Some(player) if player.health > 0 => {
                    target_player_name = player.name.clone();
                }
                _ => return,
            }
        }

        let result = combat::roll_attack(hit_threshold, damage_roll);

        info!(
            "Monster {} attacks player {}: Roll {}, Hit: {}, Damage: {}",
            monster_id, target_player_name, result.roll, result.hit, result.damage
        );

        // Update player HP and combat timestamp
        let mut did_die = false;
        let mut current_health = 0;

        {
            let mut players = self.players.write().await;
            if let Some(player) = players.get_mut(target_player_id) {
                if player.health == 0 {
                    return; // Already dead
                }

                player.last_combat_at = now;

                if result.hit {
                    player.health = player.health.saturating_sub(result.damage);
                    if player.health == 0 {
                        did_die = true;
                    }
                }
                current_health = player.health;
            }
        }

        // Send attack result after server-side HP update.
        self.broadcast(
            ServerMessage::MonsterAttackedPlayer {
                monster_id: monster_id.to_string(),
                player_id: target_player_id.to_string(),
                hit: result.hit,
                roll: result.roll,
                damage: result.damage,
                current_health,
            },
            None,
        );

        if did_die {
            let dead_player_id = target_player_id.to_string();
            self.apply_player_death_penalty(&dead_player_id).await;
            self.broadcast(
                ServerMessage::PlayerDead {
                    player_id: dead_player_id,
                },
                None,
            );
        }
    }

    pub async fn tick_regeneration(&self) {
        let mut updates = Vec::new();

        {
            let players = self.players.read().await;
            let player_chars = self.player_characters.read().await;
            let now = Self::now_ms();

            for (player_id, player) in players.iter() {
                // Only regenerate if alive and wounded
                if player.health > 0 && player.health < player.max_health {
                    // Check if player is "out of combat" (10s threshold = 10000ms)
                    if now.saturating_sub(player.last_combat_at) < 10000 {
                        continue;
                    }

                    let con = player_chars
                        .get(player_id)
                        .map(|(_, _, attrs)| attrs.con)
                        .unwrap_or(10); // Default to 10 if not found

                    let con_mod = (i16::from(con) - 10) / 2;
                    let amount = (1 + (player.level as i32 / 5) + con_mod as i32).max(1) as u32;

                    updates.push((player_id.clone(), amount));
                }
            }
        }

        if updates.is_empty() {
            return;
        }

        let mut players = self.players.write().await;
        for (player_id, amount) in updates {
            if let Some(player) = players.get_mut(&player_id) {
                if player.health > 0 && player.health < player.max_health {
                    let old_health = player.health;
                    player.health = (player.health + amount).min(player.max_health);

                    if player.health != old_health {
                        self.broadcast(
                            ServerMessage::PlayerHealthUpdate {
                                player_id: player_id.clone(),
                                health: player.health,
                                max_health: player.max_health,
                            },
                            None,
                        );
                    }
                }
            }
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

        let player_name = {
            let players = self.players.read().await;
            players
                .get(player_id)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| player_id.clone())
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
                    let level_one_floor = match character_hp::level_one_max_hp(
                        character_hp::DEFAULT_CHARACTER_RACE,
                        &player.class,
                        attributes.con,
                    ) {
                        Ok(value) => value,
                        Err(err) => {
                            warn!(
                                "Failed to compute level 1 HP floor for player {}: {}",
                                player_name, err
                            );
                            1
                        }
                    };

                    match character_hp::roll_level_hp_delta(&player.class, attributes.con) {
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
                                player_name, err
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
            let result =
                auth.update_character_xp_and_level(character_id, new_xp, new_level, max_hp_for_db);
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
            player_name,
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
}
