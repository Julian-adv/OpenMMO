use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct MonsterDefinition {
    pub id: String,
    pub name: String,
    pub model: String,
    pub health: u32,
    #[serde(rename = "walkSpeed")]
    pub walk_speed: f32,
    #[serde(rename = "runSpeed")]
    pub run_speed: f32,
    #[serde(rename = "attackRange")]
    pub attack_range: f32,
    #[serde(rename = "chaseRange")]
    pub chase_range: f32,
    #[serde(rename = "attackCooldown")]
    pub attack_cooldown: u32,
    #[serde(rename = "damageRoll")]
    pub damage_roll: String,
    pub level: u8,
    pub guard: u8,
    #[serde(rename = "hitThreshold")]
    pub hit_threshold: u8,
    #[serde(rename = "animIdle")]
    pub anim_idle: String,
    #[serde(rename = "animWalk")]
    pub anim_walk: String,
    #[serde(rename = "animRun")]
    pub anim_run: String,
    #[serde(rename = "animAttack")]
    pub anim_attack: String,
    #[serde(rename = "animHit")]
    pub anim_hit: String,
    #[serde(rename = "animDie")]
    pub anim_die: String,
    #[serde(rename = "animDead")]
    pub anim_dead: String,
}

#[derive(Debug, Clone)]
pub struct MonsterDefs {
    defs: Arc<HashMap<String, MonsterDefinition>>,
}

impl MonsterDefs {
    pub fn load() -> Self {
        let data = include_str!("../../data/monsters.json");
        let defs: HashMap<String, MonsterDefinition> =
            serde_json::from_str(data).expect("Failed to parse monsters.json");

        info!("Loaded {} monster definitions", defs.len());
        for (id, def) in &defs {
            info!(
                "  {} - HP:{} walkSpeed:{} runSpeed:{} attackRange:{} chaseRange:{} cooldown:{}ms damage:{} hitThreshold:{}",
                id, def.health, def.walk_speed, def.run_speed,
                def.attack_range, def.chase_range, def.attack_cooldown,
                def.damage_roll, def.hit_threshold
            );
        }

        Self {
            defs: Arc::new(defs),
        }
    }

    pub fn get(&self, monster_type: &str) -> Option<&MonsterDefinition> {
        self.defs.get(monster_type)
    }
}
