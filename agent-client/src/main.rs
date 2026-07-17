mod claude;
mod codex;
mod driver;
mod geom;
mod llm_scheduler;
mod monster_ai;
mod openrouter;
mod orchestrator;
mod shop_info;
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
    /// NPC auth token; defaults to reading the server-generated
    /// ../data/npc_token file (same machine).
    npc_token: Option<String>,
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

fn resolve_npc_token(config_value: Option<String>) -> anyhow::Result<String> {
    if let Some(token) = config_value {
        return Ok(token);
    }
    // Server writes the token at the repo root; our cwd is one level down.
    let path = format!("../{}", onlinerpg_shared::NPC_TOKEN_PATH_FROM_ROOT);
    let token = std::fs::read_to_string(&path)
        .map_err(|e| {
            anyhow::anyhow!(
                "no npc_token in {CONFIG_PATH} and failed to read {path} \
                 (start the game server once to generate it): {e}"
            )
        })?
        .trim()
        .to_string();
    if token.is_empty() {
        anyhow::bail!("{path} is empty");
    }
    Ok(token)
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
        resolve_from_registry(npc)?;
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

    let behavior_trees = monster_ai::MonsterAiManager::load_behavior_trees_from_json(include_str!(
        "../../data-src/behavior_trees.json"
    ));
    let (type_mapping, movement_speeds) =
        monster_ai::MonsterAiManager::load_monster_data(include_str!("../../data/monsters.json"));

    let shared = Arc::new(SharedResources {
        height_sampler: Arc::new(create_height_sampler(&config.terrain_dir)),
        world_cache: Arc::new(std::sync::RwLock::new(state::WorldCache::new())),
        behavior_trees: Arc::new(behavior_trees),
        type_mapping: Arc::new(type_mapping),
        movement_speeds: Arc::new(movement_speeds),
        scheduler: llm_scheduler::LlmScheduler::new(config.max_concurrent),
        npc_token: resolve_npc_token(config.npc_token)?,
    });

    orchestrator::run_orchestrator(config.server, npcs, shared).await
}

fn create_height_sampler(terrain_dir: &str) -> HeightSampler {
    HeightSampler::new(TerrainIO::new(std::path::PathBuf::from(terrain_dir)))
}

/// Fill an `[[npcs]]` entry from the game-data registry (`data-src/npcs.csv`,
/// the single source of truth for who an NPC is). `id` selects the registry
/// row; the character name and class come from it, and the prompt/schedule
/// files follow the `data/npcs/{id}/` directory convention. Explicit config
/// fields still win, so a deployment can override any of them. Entries
/// without `id` keep working fully spelled out (ad-hoc NPCs).
fn resolve_from_registry(npc: &mut NpcConfig) -> anyhow::Result<()> {
    let Some(id) = npc.id.clone() else {
        return Ok(());
    };
    let row = shop_info::npc_by_id(&id).ok_or_else(|| {
        anyhow::anyhow!("[[npcs]] id \"{id}\" is not in the NPC registry (data-src/npcs.csv)")
    })?;

    npc.character_name
        .get_or_insert_with(|| row.npc_name.clone());
    if npc.character_class.is_none() && !row.class.is_empty() {
        npc.character_class = Some(row.class.clone());
    }
    if npc.template_prompt.is_none() {
        let class = npc.character_class.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "registry NPC \"{id}\" has no class; add one in data-src/npcs.csv \
                 or set character_class/template_prompt in config.toml"
            )
        })?;
        npc.template_prompt = Some(format!("data/templates/{class}.txt"));
    }
    npc.instance_prompt
        .get_or_insert_with(|| format!("data/npcs/{id}/instance.txt"));
    npc.memory_file
        .get_or_insert_with(|| format!("data/npcs/{id}/memory.txt"));
    if npc.schedule_file.is_none() {
        // Schedules are optional, and a missing path is logged as an error
        // downstream — only derive it when the conventional file exists.
        let path = format!("data/npcs/{id}/schedule.json");
        if std::path::Path::new(&path).exists() {
            npc.schedule_file = Some(path);
        }
    }
    Ok(())
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
        ServerMessage::PlayerAppeared { .. } => "PlayerAppeared",
        ServerMessage::PlayerDisappeared { .. } => "PlayerDisappeared",
        ServerMessage::PlayerMoved { .. } => "PlayerMoved",
        ServerMessage::PlayerTeleported { .. } => "PlayerTeleported",
        ServerMessage::DungeonChestOpened { .. } => "DungeonChestOpened",
        ServerMessage::DungeonPropBroken { .. } => "DungeonPropBroken",
        ServerMessage::DungeonPropOpened { .. } => "DungeonPropOpened",
        ServerMessage::DungeonPropsState { .. } => "DungeonPropsState",
        ServerMessage::DungeonDoorToggled { .. } => "DungeonDoorToggled",
        ServerMessage::DungeonDoorsState { .. } => "DungeonDoorsState",
        ServerMessage::ChatMessage { .. } => "ChatMessage",
        ServerMessage::GameState { .. } => "GameState",
        ServerMessage::GameTimeSync { .. } => "GameTimeSync",
        ServerMessage::MonsterSpawned { .. } => "MonsterSpawned",
        ServerMessage::MonsterMoved { .. } => "MonsterMoved",
        ServerMessage::MonsterRemoved { .. } => "MonsterRemoved",
        ServerMessage::MonsterDead { .. } => "MonsterDead",
        ServerMessage::PlayerAttacked { .. } => "PlayerAttacked",
        ServerMessage::MonsterProvoked { .. } => "MonsterProvoked",
        ServerMessage::MonsterAttackedPlayer { .. } => "MonsterAttackedPlayer",
        ServerMessage::PlayerDead { .. } => "PlayerDead",
        ServerMessage::PlayerRespawned { .. } => "PlayerRespawned",
        ServerMessage::PlayerHealthUpdate { .. } => "PlayerHealthUpdate",
        ServerMessage::XpGained { .. } => "XpGained",
        ServerMessage::Kicked { .. } => "Kicked",
        ServerMessage::PlayerTorchToggled { .. } => "PlayerTorchToggled",
        ServerMessage::HouseSpawned { .. } => "HouseSpawned",
        ServerMessage::HouseUpdated { .. } => "HouseUpdated",
        ServerMessage::TreeTilesInvalidated { .. } => "TreeTilesInvalidated",
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
        ServerMessage::GroundItemAppeared { .. } => "GroundItemAppeared",
        ServerMessage::GroundItemRemoved { .. } => "GroundItemRemoved",
        ServerMessage::InventoryError { .. } => "InventoryError",
        ServerMessage::ShopState { .. } => "ShopState",
        ServerMessage::GoldUpdate { .. } => "GoldUpdate",
        ServerMessage::GuardUpdated { .. } => "GuardUpdated",
        ServerMessage::GoldGained { .. } => "GoldGained",
        ServerMessage::TradeError { .. } => "TradeError",
        ServerMessage::DealUpdated { .. } => "DealUpdated",
        ServerMessage::DealResult { .. } => "DealResult",
        ServerMessage::TradeNotice { .. } => "TradeNotice",
        ServerMessage::TradeBusy { .. } => "TradeBusy",
    }
}
