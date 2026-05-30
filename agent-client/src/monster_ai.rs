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
    type_to_movement: HashMap<String, MonsterMovement>,
}

#[derive(Debug, Clone, Copy)]
pub struct MonsterMovement {
    pub walk_speed: f32,
    pub run_speed: f32,
}

impl Default for MonsterMovement {
    fn default() -> Self {
        Self {
            walk_speed: 1.0,
            run_speed: 8.0,
        }
    }
}

impl MonsterAiManager {
    pub fn new() -> Self {
        Self {
            brains: HashMap::new(),
            templates: HashMap::new(),
            type_to_template: HashMap::new(),
            type_to_movement: HashMap::new(),
        }
    }

    /// Load AI templates from JSON (data/ai_templates.json).
    pub fn load_templates_from_json(json: &str) -> HashMap<String, AiTemplate> {
        monster_ai::load_templates(json).unwrap_or_default()
    }

    /// Load per-type AI template names and movement speeds from monsters.json
    /// in a single parse.
    pub fn load_monster_data(
        monsters_json: &str,
    ) -> (HashMap<String, String>, HashMap<String, MonsterMovement>) {
        #[derive(serde::Deserialize)]
        struct RawMonster {
            #[serde(rename = "aiTemplate", default = "default_template")]
            ai_template: String,
            #[serde(rename = "walkSpeed", default = "default_walk_speed")]
            walk_speed: f32,
            #[serde(rename = "runSpeed", default = "default_run_speed")]
            run_speed: f32,
        }
        fn default_template() -> String {
            "default".to_string()
        }
        fn default_walk_speed() -> f32 {
            MonsterMovement::default().walk_speed
        }
        fn default_run_speed() -> f32 {
            MonsterMovement::default().run_speed
        }

        let raw: HashMap<String, RawMonster> =
            serde_json::from_str(monsters_json).unwrap_or_default();
        let mut type_to_template = HashMap::with_capacity(raw.len());
        let mut type_to_movement = HashMap::with_capacity(raw.len());
        for (id, r) in raw {
            type_to_template.insert(id.clone(), r.ai_template);
            type_to_movement.insert(
                id,
                MonsterMovement {
                    walk_speed: r.walk_speed,
                    run_speed: r.run_speed,
                },
            );
        }
        (type_to_template, type_to_movement)
    }

    pub fn set_templates(&mut self, templates: HashMap<String, AiTemplate>) {
        self.templates = templates;
    }

    pub fn set_type_mapping(&mut self, mapping: HashMap<String, String>) {
        self.type_to_template = mapping;
    }

    pub fn set_movement_speeds(&mut self, movement: HashMap<String, MonsterMovement>) {
        self.type_to_movement = movement;
    }

    /// Resolve the template name for a monster type, falling back to "default".
    fn template_name_for(&self, monster_type: &str) -> String {
        self.type_to_template
            .get(monster_type)
            .cloned()
            .unwrap_or_else(|| "default".to_string())
    }

    /// Register a newly assigned monster.
    pub fn add_monster(&mut self, monster: &Monster) {
        info!(
            "Monster AI: managing {} (type={})",
            monster.id, monster.monster_type
        );
        let template_name = self.template_name_for(&monster.monster_type);
        let template = template_by_name(&self.templates, &template_name);
        let movement = self
            .type_to_movement
            .get(&monster.monster_type)
            .copied()
            .unwrap_or_default();
        let brain = MonsterBrain::new(
            monster.id.clone(),
            monster.monster_type.clone(),
            template_name,
            monster.position.clone(),
            monster.health,
            monster.max_health,
            movement.walk_speed,
            movement.run_speed,
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
        // Get template before mutable borrow
        let template = if let Some(brain) = self.brains.get(monster_id) {
            template_by_name(&self.templates, &brain.template_name).clone()
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
            let template = template_by_name(&self.templates, &brain.template_name);
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

/// Look up a template by its resolved name, falling back to a shared default.
/// Takes `&templates` (not `&self`) so it can be called inside a `values_mut`
/// loop over `self.brains` without a borrow conflict.
fn template_by_name<'a>(
    templates: &'a HashMap<String, AiTemplate>,
    template_name: &str,
) -> &'a AiTemplate {
    static DEFAULT: std::sync::LazyLock<AiTemplate> = std::sync::LazyLock::new(AiTemplate::default);
    templates.get(template_name).unwrap_or(&DEFAULT)
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
