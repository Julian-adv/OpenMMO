//! Monster AI adapter — delegates to `onlinerpg_shared::monster_ai`.

use onlinerpg_shared::monster_ai::{
    self, AiCommand, AiTemplate, CachePathProvider, MonsterBrain, NearbyPlayer,
};
use onlinerpg_shared::pathfinding::PassabilityCache;
use onlinerpg_shared::{ClientMessage, Monster, Player};
use std::collections::HashMap;
use tracing::info;

/// Manages all monster brains assigned to this agent-client.
pub struct MonsterAiManager {
    brains: HashMap<String, MonsterBrain>,
    templates: HashMap<String, AiTemplate>,
    /// Maps monster_type -> template name
    type_to_template: HashMap<String, String>,
}

impl MonsterAiManager {
    pub fn new() -> Self {
        Self {
            brains: HashMap::new(),
            templates: HashMap::new(),
            type_to_template: HashMap::new(),
        }
    }

    /// Load AI templates from JSON (data/ai_templates.json).
    pub fn load_templates_from_json(json: &str) -> HashMap<String, AiTemplate> {
        monster_ai::load_templates(json).unwrap_or_default()
    }

    /// Load monster type -> template name mapping from monsters.json.
    pub fn load_type_mapping(monsters_json: &str) -> HashMap<String, String> {
        #[derive(serde::Deserialize)]
        struct RawMonster {
            #[serde(rename = "aiTemplate", default = "default_template")]
            ai_template: String,
        }
        fn default_template() -> String {
            "default".to_string()
        }

        let raw: HashMap<String, RawMonster> =
            serde_json::from_str(monsters_json).unwrap_or_default();
        raw.into_iter().map(|(id, r)| (id, r.ai_template)).collect()
    }

    pub fn set_templates(&mut self, templates: HashMap<String, AiTemplate>) {
        self.templates = templates;
    }

    pub fn set_type_mapping(&mut self, mapping: HashMap<String, String>) {
        self.type_to_template = mapping;
    }

    fn get_template(&self, monster_type: &str) -> &AiTemplate {
        resolve_template(&self.type_to_template, &self.templates, monster_type)
    }

    /// Register a newly assigned monster.
    pub fn add_monster(&mut self, monster: &Monster) {
        info!(
            "Monster AI: managing {} (type={})",
            monster.id, monster.monster_type
        );
        let template = self.get_template(&monster.monster_type);
        let brain = MonsterBrain::new(
            monster.id.clone(),
            monster.monster_type.clone(),
            monster.position.clone(),
            monster.health,
            monster.max_health,
            template,
        );
        self.brains.insert(monster.id.clone(), brain);
    }

    /// Remove a monster (died or removed).
    pub fn remove_monster(&mut self, monster_id: &str) {
        if self.brains.remove(monster_id).is_some() {
            info!("Monster AI: stopped managing {}", monster_id);
        }
    }

    /// Notify that a monster was hit by a player.
    pub fn handle_monster_hit(
        &mut self,
        monster_id: &str,
        attacker_id: &str,
        hit: bool,
        damage: u32,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        // Get template name before mutable borrow
        let template = if let Some(brain) = self.brains.get(monster_id) {
            self.get_template(&brain.monster_type).clone()
        } else {
            return vec![];
        };

        let brain = self.brains.get_mut(monster_id).unwrap();
        let path_provider = CachePathProvider {
            cache: passability_cache,
        };
        let mut rng = rand::thread_rng();
        let cmds = brain.handle_hit(
            attacker_id,
            hit,
            damage,
            &template,
            &path_provider,
            &mut rng,
        );
        cmds.into_iter().map(command_to_client_msg).collect()
    }

    /// Notify that a monster died.
    pub fn handle_monster_dead(&mut self, monster_id: &str) {
        if let Some(brain) = self.brains.get_mut(monster_id) {
            brain.handle_death();
        }
    }

    /// Tick all managed monster brains. Returns commands to send.
    pub fn tick_all(
        &mut self,
        delta_ms: f32,
        nearby_players: &HashMap<String, Player>,
        passability_cache: &PassabilityCache,
    ) -> Vec<ClientMessage> {
        let players: Vec<NearbyPlayer> = nearby_players
            .values()
            .map(|p| NearbyPlayer {
                id: p.id.clone(),
                position: p.position.clone(),
                health: p.health,
            })
            .collect();

        let path_provider = CachePathProvider {
            cache: passability_cache,
        };
        let mut rng = rand::thread_rng();

        let mut all_commands = Vec::new();
        for brain in self.brains.values_mut() {
            let template =
                resolve_template(&self.type_to_template, &self.templates, &brain.monster_type);
            let result = brain.tick(delta_ms, &players, template, &path_provider, &mut rng);
            all_commands.extend(result.commands.into_iter().map(command_to_client_msg));
        }
        all_commands
    }

    /// Check if we manage a given monster.
    pub fn manages(&self, monster_id: &str) -> bool {
        self.brains.contains_key(monster_id)
    }
}

fn resolve_template<'a>(
    type_to_template: &HashMap<String, String>,
    templates: &'a HashMap<String, AiTemplate>,
    monster_type: &str,
) -> &'a AiTemplate {
    static DEFAULT: std::sync::LazyLock<AiTemplate> = std::sync::LazyLock::new(AiTemplate::default);
    let name = type_to_template
        .get(monster_type)
        .map(|s| s.as_str())
        .unwrap_or("default");
    templates.get(name).unwrap_or(&DEFAULT)
}

fn command_to_client_msg(cmd: AiCommand) -> ClientMessage {
    match cmd {
        AiCommand::Move {
            monster_id,
            position,
            rotation,
            state,
            target_position,
        } => ClientMessage::MonsterMove {
            monster_id,
            position,
            rotation,
            state,
            target_position,
        },
        AiCommand::Attack {
            monster_id,
            target_player_id,
        } => ClientMessage::MonsterAttack {
            monster_id,
            target_player_id,
        },
    }
}
