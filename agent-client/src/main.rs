mod claude;
mod codex;
mod driver;
mod llm_scheduler;
mod mcp;
mod monster_ai;
mod openrouter;
mod orchestrator;
mod state;
mod ws;

use std::sync::Arc;

use claude::ClaudeConfig;
use codex::CodexConfig;
use futures_util::StreamExt;
use onlinerpg_shared::ClientMessage;
use onlinerpg_terrain::height::HeightSampler;
use onlinerpg_terrain::io::TerrainIO;
use openrouter::OpenRouterConfig;
use orchestrator::{NpcConfig, SharedResources};
use serde::Deserialize;
use state::{SharedState, WorldCache};
use tokio::sync::{mpsc, Mutex};
use tracing::info;

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

/// Raw config as parsed from TOML. Supports both legacy single-NPC format
/// and new `[[npcs]]` array format.
#[derive(Deserialize)]
struct Config {
    /// Server WebSocket URL
    server: String,
    /// Path to terrain data directory (for heightmap sampling)
    #[serde(default = "default_terrain_dir")]
    terrain_dir: String,

    // --- Legacy single-NPC fields (used when [[npcs]] is absent) ---
    /// Account name
    account: Option<String>,
    /// Password
    password: Option<String>,
    /// Create a new account instead of logging in
    #[serde(default)]
    create_account: bool,
    /// Character ID to enter game with (if omitted, waits for MCP connection)
    character_id: Option<i64>,
    /// MCP HTTP server port (default: 8808)
    #[serde(default = "default_mcp_port")]
    mcp_port: u16,
    /// LLM backend type: "none", "claude", "openrouter", "codex"
    #[serde(default)]
    llm: LlmType,
    /// Minimum interval between LLM prompts in seconds
    #[serde(default = "default_min_interval_secs")]
    min_interval_secs: u64,
    /// Debounce window for batching urgent events in seconds
    #[serde(default = "default_debounce_secs")]
    debounce_secs: u64,
    /// Idle interval (secs): LLM call frequency when no chat/combat activity
    #[serde(default = "default_idle_interval_secs")]
    idle_interval_secs: u64,
    /// Activity window (secs): how long after last chat/combat to stay in active mode
    #[serde(default = "default_activity_window_secs")]
    activity_window_secs: u64,
    /// Claude CLI integration config
    #[serde(default)]
    claude: ClaudeConfig,
    /// OpenRouter API integration config
    #[serde(default)]
    openrouter: OpenRouterConfig,
    /// Codex CLI integration config
    #[serde(default)]
    codex: CodexConfig,

    // --- Multi-NPC orchestrator fields ---
    /// Array of NPC configurations. When present, overrides legacy single-NPC fields.
    #[serde(default)]
    npcs: Vec<NpcConfig>,

    // --- LLM Scheduler ---
    /// Maximum number of concurrent LLM calls across all NPCs (default: 2)
    #[serde(default = "default_max_concurrent")]
    max_concurrent: usize,
}

fn default_terrain_dir() -> String {
    "../data/terrain".to_string()
}

fn default_mcp_port() -> u16 {
    8808
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

    // Determine mode: multi-NPC orchestrator vs. legacy single-NPC
    if !config.npcs.is_empty() {
        return run_orchestrator_mode(config).await;
    }

    // Legacy single-NPC mode: require account/password
    let account = config
        .account
        .clone()
        .ok_or_else(|| anyhow::anyhow!("'account' is required when [[npcs]] is not set"))?;
    let password = config
        .password
        .clone()
        .ok_or_else(|| anyhow::anyhow!("'password' is required when [[npcs]] is not set"))?;

    let llm_enabled = config.llm != LlmType::None;

    // MCP mode: single NPC without character_id and without LLM
    if config.character_id.is_none() && !llm_enabled {
        return run_mcp_mode(&config, &account, &password).await;
    }

    // Legacy single-NPC with game session: convert to orchestrator
    let npc = NpcConfig {
        account,
        password,
        create_account: config.create_account,
        character_id: config.character_id,
        llm: config.llm.clone(),
        min_interval_secs: config.min_interval_secs,
        debounce_secs: config.debounce_secs,
        idle_interval_secs: config.idle_interval_secs,
        activity_window_secs: config.activity_window_secs,
        claude: config.claude.clone(),
        openrouter: config.openrouter.clone(),
        codex: config.codex.clone(),
        template_prompt: None,
        instance_prompt: None,
        memory_file: None,
    };

    run_orchestrator_mode_with_npcs(
        config.server,
        config.terrain_dir,
        vec![npc],
        config.max_concurrent,
    )
    .await
}

async fn run_orchestrator_mode(config: Config) -> anyhow::Result<()> {
    run_orchestrator_mode_with_npcs(
        config.server,
        config.terrain_dir,
        config.npcs,
        config.max_concurrent,
    )
    .await
}

async fn run_orchestrator_mode_with_npcs(
    server_url: String,
    terrain_dir: String,
    npcs: Vec<NpcConfig>,
    max_concurrent: usize,
) -> anyhow::Result<()> {
    let ai_templates = monster_ai::MonsterAiManager::load_templates_from_json(include_str!(
        "../../data/ai_templates.json"
    ));
    let type_mapping =
        monster_ai::MonsterAiManager::load_type_mapping(include_str!("../../data/monsters.json"));

    let shared = Arc::new(SharedResources {
        height_sampler: Arc::new(create_height_sampler(&terrain_dir)),
        world_cache: Arc::new(std::sync::RwLock::new(WorldCache::new())),
        ai_templates: Arc::new(ai_templates),
        type_mapping: Arc::new(type_mapping),
        scheduler: llm_scheduler::LlmScheduler::new(max_concurrent),
    });

    orchestrator::run_orchestrator(server_url, npcs, shared).await
}

/// MCP mode: single session with HTTP server (no reconnect).
async fn run_mcp_mode(config: &Config, account: &str, password: &str) -> anyhow::Result<()> {
    let password_hash = fnv1a_hash(password);

    let ws_stream = ws::connect_ws(&config.server, account).await;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    ws::send(
        &mut ws_tx,
        &ClientMessage::Authenticate {
            account_name: account.to_string(),
            password_hash,
            create_account: config.create_account,
        },
    )
    .await?;

    let characters = ws::wait_for_auth(&mut ws_rx, account).await?;

    let (cmd_tx, _cmd_rx) = mpsc::channel::<ClientMessage>(32);
    let height_sampler = Arc::new(create_height_sampler(&config.terrain_dir));
    let world_cache = Arc::new(std::sync::RwLock::new(WorldCache::new()));
    let state = Arc::new(Mutex::new(SharedState::new(
        characters,
        cmd_tx,
        height_sampler,
        world_cache,
    )));

    info!(
        "No character_id configured -- starting MCP HTTP server on port {}...",
        config.mcp_port
    );
    mcp::run_mcp_server(state, config.mcp_port).await
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
    }
}
