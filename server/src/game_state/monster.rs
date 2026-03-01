use crate::types::{Position, ServerMessage};
use tracing::{info, warn};

impl super::GameState {
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
            last_attack_at: 0,
        };

        monsters.insert(id.clone(), monster.clone());
        info!(
            "Spawned monster {} [owner #{}, spawn #{}] (Total: {})",
            id,
            owner_number,
            spawn_count,
            monsters.len()
        );

        self.broadcast(ServerMessage::MonsterSpawned { monster }, None);
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
}
