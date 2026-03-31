use crate::types::{MonsterState, Position, ServerMessage};
use tracing::{info, warn};

impl super::GameState {
    fn find_spawn_rule(
        &self,
        monster_type: &str,
    ) -> Option<&crate::world_config::MonsterSpawnRule> {
        self.spawn_rules
            .iter()
            .find(|r| r.monster_type == monster_type)
    }

    /// Create a monster, broadcast to all, and return it (or None if limit reached).
    pub async fn spawn_monster(
        &self,
        monster_type: String,
        position: Position,
        rotation: f32,
        owner_id: Option<String>,
    ) -> Option<crate::types::Monster> {
        let max_total = crate::world_config::world_config().max_monsters_total as usize;
        let max_per_player = self
            .find_spawn_rule(&monster_type)
            .map(|r| r.max_per_player as usize);

        // Read lock: single-pass check of both global and per-player limits
        {
            let monsters = self.monsters.read().await;
            let mut alive_count = 0usize;
            let mut owned_alive = 0usize;
            for m in monsters.values() {
                if m.state != MonsterState::Dead {
                    alive_count += 1;
                    if let Some(ref owner) = owner_id {
                        if m.owner_id.as_deref() == Some(owner.as_str())
                            && m.monster_type == monster_type
                        {
                            owned_alive += 1;
                        }
                    }
                }
            }
            if alive_count >= max_total {
                warn!("Monster spawn rejected: limit reached ({})", alive_count);
                return None;
            }
            if let Some(max) = max_per_player {
                if owned_alive >= max {
                    warn!(
                        "Monster spawn rejected: player {:?} already owns {} alive {}",
                        owner_id, owned_alive, monster_type
                    );
                    return None;
                }
            }
        }

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

        let def = self.monster_defs.get(&monster_type);
        let health = def.map(|d| d.health).unwrap_or(10);
        let monster = crate::types::Monster {
            id: id.clone(),
            monster_type: monster_type.clone(),
            position,
            rotation,
            state: MonsterState::Idle,
            owner_id,
            health,
            max_health: health,
            last_attack_at: 0,
        };

        let mut monsters = self.monsters.write().await;
        monsters.insert(id.clone(), monster.clone());
        let alive = monsters
            .values()
            .filter(|m| m.state != MonsterState::Dead)
            .count();
        info!(
            "Spawned monster {} [owner #{}, spawn #{}] (Alive: {})",
            id, owner_number, spawn_count, alive
        );

        self.broadcast(
            ServerMessage::MonsterSpawned {
                monster: monster.clone(),
            },
            None,
        );
        Some(monster)
    }

    pub async fn update_monster_position(
        &self,
        monster_id: String,
        new_position: Position,
        rotation: f32,
        state: MonsterState,
        target_position: Position,
    ) {
        let mut monsters = self.monsters.write().await;

        if let Some(monster) = monsters.get_mut(&monster_id) {
            if monster.state == MonsterState::Dead {
                return;
            }
            monster.position = new_position.clone();
            monster.rotation = rotation;
            monster.state = state;

            self.broadcast(
                ServerMessage::MonsterMoved {
                    monster_id,
                    position: new_position,
                    rotation,
                    state,
                    target_position,
                    owner_id: monster.owner_id.clone(),
                },
                monster.owner_id.clone(),
            );
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
            self.broadcast(
                ServerMessage::MonsterRemoved {
                    monster_id: monster_id.clone(),
                },
                None,
            );
        }
    }

    /// Server-driven monster spawn tick. For each player, checks if they need
    /// more monsters per spawn rules and sends SpawnMonsterRequest so the client
    /// can pick a valid position (avoiding water, interiors, cliffs).
    pub async fn tick_monster_spawns(&self) {
        if self.spawn_rules.is_empty() {
            return;
        }

        let max_total = crate::world_config::world_config().max_monsters_total as usize;

        let player_ids: Vec<String> = {
            let players = self.players.read().await;
            players.keys().cloned().collect()
        };
        if player_ids.is_empty() {
            return;
        }

        // Single lock: count alive monsters per (owner, type) and total
        let (owner_type_counts, total_alive) = {
            let monsters = self.monsters.read().await;
            let mut counts = std::collections::HashMap::new();
            let mut alive = 0usize;
            for m in monsters.values() {
                if m.state != MonsterState::Dead {
                    alive += 1;
                    if let Some(ref owner) = m.owner_id {
                        *counts
                            .entry((owner.clone(), m.monster_type.clone()))
                            .or_insert(0) += 1;
                    }
                }
            }
            (counts, alive)
        };

        let mut requested_this_tick = 0usize;

        for rule in &self.spawn_rules {
            for player_id in &player_ids {
                if total_alive + requested_this_tick >= max_total {
                    return;
                }

                let owned = owner_type_counts
                    .get(&(player_id.clone(), rule.monster_type.clone()))
                    .copied()
                    .unwrap_or(0);

                if owned >= rule.max_per_player {
                    continue;
                }

                // Ask the client to find a valid position and spawn
                self.send_direct_message(
                    player_id,
                    ServerMessage::SpawnMonsterRequest {
                        monster_type: rule.monster_type.clone(),
                        min_x: rule.min_x,
                        min_z: rule.min_z,
                        max_x: rule.max_x,
                        max_z: rule.max_z,
                    },
                )
                .await;

                requested_this_tick += 1;
            }
        }
    }

    /// Validate that a spawn position is within the spawn zone rectangle
    /// and not inside any no-spawn zone.
    pub fn validate_spawn_position(&self, monster_type: &str, position: &Position) -> bool {
        let rule = match self.find_spawn_rule(monster_type) {
            Some(r) => r,
            None => return false,
        };

        // Check position is within the spawn zone rectangle (with small tolerance)
        let tol = 1.0;
        if position.x < rule.min_x - tol
            || position.x > rule.max_x + tol
            || position.z < rule.min_z - tol
            || position.z > rule.max_z + tol
        {
            return false;
        }

        // Reject if inside any no-spawn zone
        for zone in &self.no_spawn_zones {
            if zone.contains(position.x, position.z) {
                return false;
            }
        }

        true
    }
}
