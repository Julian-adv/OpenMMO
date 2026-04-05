mod claude;
mod codex;
mod driver;
mod llm_scheduler;
mod monster_ai;
mod openrouter;
mod orchestrator;
mod state;
mod ws;

use std::sync::Arc;

use onlinerpg_terrain::height::HeightSampler;
use onlinerpg_terrain::io::TerrainIO;
use orchestrator::{NpcConfig, SharedResources};
use serde::Deserialize;

/// Which LLM backend to use for the agent driver.
#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LlmType {
    /// No LLM driver (MCP or direct mode)
    #[default]
    None,
    /// Claude CLI (stdio subprocess)
    Claude,
    /// OpenRouter API (HTTP)
    Openrouter,
    /// Codex CLI (stdio subprocess)
    Codex,
}

/// Config parsed from TOML. Uses `[[npcs]]` array for multi-NPC orchestrator.
#[derive(Deserialize)]
struct Config {
    /// Server WebSocket URL
    server: String,
    /// Path to terrain data directory (for heightmap sampling)
    #[serde(default = "default_terrain_dir")]
    terrain_dir: String,

    /// Array of NPC configurations.
    #[serde(default)]
    npcs: Vec<NpcConfig>,

    /// Maximum number of concurrent LLM calls across all NPCs (default: 2)
    #[serde(default = "default_max_concurrent")]
    max_concurrent: usize,

    /// Claude CLI integration config (shared across NPCs that don't override)
    #[serde(default)]
    claude: claude::ClaudeConfig,
    /// OpenRouter API integration config
    #[serde(default)]
    openrouter: openrouter::OpenRouterConfig,
    /// Codex CLI integration config
    #[serde(default)]
    codex: codex::CodexConfig,
}

fn default_terrain_dir() -> String {
    "../data/terrain".to_string()
}

pub fn default_min_interval_secs() -> u64 {
    5
}

pub fn default_debounce_secs() -> u64 {
    2
}

pub fn default_idle_interval_secs() -> u64 {
    3600
}

pub fn default_activity_window_secs() -> u64 {
    30
}

fn default_max_concurrent() -> usize {
    2
}

const CONFIG_PATH: &str = "data/config.toml";

/// FNV-1a 32-bit hash (matches the JS client implementation)
pub fn fnv1a_hash(input: &str) -> String {
    let mut hash: u32 = 2_166_136_261;
    for byte in input.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }
    format!("{hash:08x}")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let config_text = std::fs::read_to_string(CONFIG_PATH)
        .map_err(|e| anyhow::anyhow!("Failed to read {CONFIG_PATH}: {e}"))?;
    let config: Config = toml::from_str(&config_text)
        .map_err(|e| anyhow::anyhow!("Failed to parse {CONFIG_PATH}: {e}"))?;

    if config.npcs.is_empty() {
        anyhow::bail!("No [[npcs]] configured in {CONFIG_PATH}");
    }

    // NPCs inherit root-level backend configs when they don't override them
    let mut npcs = config.npcs;
    for npc in &mut npcs {
        if npc.claude == claude::ClaudeConfig::default() {
            npc.claude = config.claude.clone();
        }
        if npc.openrouter == openrouter::OpenRouterConfig::default() {
            npc.openrouter = config.openrouter.clone();
        }
        if npc.codex == codex::CodexConfig::default() {
            npc.codex = config.codex.clone();
        }
    }

    let ai_templates = monster_ai::MonsterAiManager::load_templates_from_json(include_str!(
        "../../data/ai_templates.json"
    ));
    let type_mapping =
        monster_ai::MonsterAiManager::load_type_mapping(include_str!("../../data/monsters.json"));

    let shared = Arc::new(SharedResources {
        height_sampler: Arc::new(create_height_sampler(&config.terrain_dir)),
        world_cache: Arc::new(std::sync::RwLock::new(state::WorldCache::new())),
        ai_templates: Arc::new(ai_templates),
        type_mapping: Arc::new(type_mapping),
        scheduler: llm_scheduler::LlmScheduler::new(config.max_concurrent),
    });

    orchestrator::run_orchestrator(config.server, npcs, shared).await
}

fn create_height_sampler(terrain_dir: &str) -> HeightSampler {
    HeightSampler::new(TerrainIO::new(std::path::PathBuf::from(terrain_dir)))
}

pub fn msg_name(msg: &onlinerpg_shared::ServerMessage) -> &'static str {
    use onlinerpg_shared::ServerMessage;
    match msg {
        ServerMessage::AuthSuccess { .. } => "AuthSuccess",
        ServerMessage::AuthError { .. } => "AuthError",
        ServerMessage::JoinSuccess { .. } => "JoinSuccess",
        ServerMessage::CharacterCreated { .. } => "CharacterCreated",
        ServerMessage::CharacterStatsRolled { .. } => "CharacterStatsRolled",
        ServerMessage::CharacterDeleted { .. } => "CharacterDeleted",
        ServerMessage::CharacterError { .. } => "CharacterError",
        ServerMessage::PlayerJoined { .. } => "PlayerJoined",
        ServerMessage::PlayerLeft { .. } => "PlayerLeft",
        ServerMessage::PlayerMoved { .. } => "PlayerMoved",
        ServerMessage::PlayerTeleported { .. } => "PlayerTeleported",
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::GameState { .. } => "GameState",
        ServerMessage::GameTimeSync { .. } => "GameTimeSync",
        ServerMessage::MonsterSpawned { .. } => "MonsterSpawned",
        ServerMessage::MonsterMoved { .. } => "MonsterMoved",
        ServerMessage::MonsterRemoved { .. } => "MonsterRemoved",
        ServerMessage::MonsterDead { .. } => "MonsterDead",
        ServerMessage::PlayerAttacked { .. } => "PlayerAttacked",
        ServerMessage::MonsterAttackedPlayer { .. } => "MonsterAttackedPlayer",
        ServerMessage::PlayerDead { .. } => "PlayerDead",
        ServerMessage::PlayerRespawned { .. } => "PlayerRespawned",
        ServerMessage::PlayerHealthUpdate { .. } => "PlayerHealthUpdate",
        ServerMessage::XpGained { .. } => "XpGained",
        ServerMessage::Kicked { .. } => "Kicked",
        ServerMessage::PlayerTorchToggled { .. } => "PlayerTorchToggled",
        ServerMessage::HouseSpawned { .. } => "HouseSpawned",
        ServerMessage::HouseUpdated { .. } => "HouseUpdated",
        ServerMessage::HouseRemoved { .. } => "HouseRemoved",
        ServerMessage::HousesInArea { .. } => "HousesInArea",
        ServerMessage::DoorToggled { .. } => "DoorToggled",
        ServerMessage::MonsterAssigned { .. } => "MonsterAssigned",
        ServerMessage::SpawnMonsterRequest { .. } => "SpawnMonsterRequest",
        ServerMessage::NoSpawnZones { .. } => "NoSpawnZones",
        ServerMessage::PlayerInteractionChanged { .. } => "PlayerInteractionChanged",
        ServerMessage::InteractionRejected { .. } => "InteractionRejected",
        ServerMessage::InventoryState { .. } => "InventoryState",
        ServerMessage::InventoryUpdated { .. } => "InventoryUpdated",
        ServerMessage::GroundItemSpawned { .. } => "GroundItemSpawned",
        ServerMessage::GroundItemRemoved { .. } => "GroundItemRemoved",
        ServerMessage::InventoryError { .. } => "InventoryError",
    }
}
