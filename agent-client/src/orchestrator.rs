//! Orchestrator: manages multiple NPC connections in parallel.
//!
//! Each NPC gets its own WebSocket connection and session loop, but they share
//! terrain data (HeightSampler) and world cache (PassabilityCache + houses).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use onlinerpg_shared::monster_ai::AiTemplate;
use onlinerpg_shared::{ClientMessage, ServerMessage};
use onlinerpg_terrain::height::HeightSampler;
use serde::Deserialize;
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

use crate::claude::{self, ClaudeConfig};
use crate::codex::{self, CodexConfig};
use crate::driver;
use crate::llm_scheduler::LlmScheduler;
use crate::openrouter::{self, OpenRouterConfig};
use crate::state::{SharedState, WorldCache};
use crate::ws;
use crate::{fnv1a_hash, LlmType};

const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Parsed schedule condition (validated at load time).
#[derive(Debug, Clone, PartialEq)]
pub enum ScheduleCondition {
    Day,
    Night,
    Time {
        hour: u32,
        minute: u32,
    },
    /// Recurring: fires every hour at the given minute (e.g. `"*:00"`).
    Recurring {
        minute: u32,
    },
}

/// A single schedule entry: go to a position at a specific time condition.
#[derive(Debug, Clone, Deserialize)]
pub struct ScheduleEntry {
    /// When to activate: "day", "night", or "H:MM" / "HH:MM" (game time).
    pub at: String,
    /// Target position [x, y, z] (final/rest position).
    pub pos: [f32; 3],
    /// Facing rotation in degrees.
    #[serde(default)]
    pub rotation: f32,
    /// Floor level (0 = ground, 1 = 2nd floor, etc.).
    #[serde(default)]
    pub floor_level: u8,
    /// Human-readable label for LLM prompt context.
    pub label: Option<String>,
    /// Furniture type to interact with after arriving (e.g. "bed").
    pub action: Option<String>,
    /// Furniture placement ID to interact with.
    pub furniture_id: Option<u32>,
    /// Optional patrol route: list of [x, y, z] waypoints to visit before going to `pos`.
    #[serde(default)]
    pub waypoints: Vec<[f32; 3]>,
    /// Parsed condition (set after deserialization).
    #[serde(skip)]
    pub condition: Option<ScheduleCondition>,
}

impl ScheduleEntry {
    pub fn is_sleeping(&self) -> bool {
        self.action.as_deref() == Some("bed")
    }

    pub fn display_label(&self) -> &str {
        self.label.as_deref().unwrap_or("schedule position")
    }

    /// Parse the `at` field into a `ScheduleCondition`. Returns error for invalid formats.
    /// Supports: `"day"`, `"night"`, `"H:MM"` / `"HH:MM"`, or `"*:MM"` (recurring every hour).
    pub fn parse_condition(&mut self) -> Result<(), String> {
        self.condition = Some(match self.at.as_str() {
            "day" => ScheduleCondition::Day,
            "night" => ScheduleCondition::Night,
            time_str => {
                let (h, m) = time_str
                    .split_once(':')
                    .ok_or_else(|| format!("invalid schedule condition: {time_str}"))?;
                let minute = m
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| format!("invalid minute in: {time_str}"))?;
                if h.trim() == "*" {
                    ScheduleCondition::Recurring { minute }
                } else {
                    let hour = h
                        .trim()
                        .parse::<u32>()
                        .map_err(|_| format!("invalid hour in: {time_str}"))?;
                    ScheduleCondition::Time { hour, minute }
                }
            }
        });
        Ok(())
    }
}

/// Wrapper for deserializing a schedule file.
#[derive(Debug, Deserialize)]
struct ScheduleFile {
    schedule: Vec<ScheduleEntry>,
}

/// Per-NPC configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct NpcConfig {
    pub account: String,
    pub password: String,
    #[serde(default)]
    pub create_account: bool,
    #[serde(default)]
    pub llm: LlmType,
    #[serde(default = "super::default_min_interval_secs")]
    pub min_interval_secs: u64,
    #[serde(default = "super::default_debounce_secs")]
    pub debounce_secs: u64,
    #[serde(default = "super::default_idle_interval_secs")]
    pub idle_interval_secs: u64,
    #[serde(default = "super::default_activity_window_secs")]
    pub activity_window_secs: u64,
    #[serde(default)]
    pub claude: ClaudeConfig,
    #[serde(default)]
    pub openrouter: OpenRouterConfig,
    #[serde(default)]
    pub codex: CodexConfig,

    // --- Auto-provisioning ---
    /// Character name to create if no characters exist on this account.
    pub character_name: Option<String>,
    /// Character class for auto-creation (e.g. "merchant"). Defaults to "knight".
    pub character_class: Option<String>,

    // --- 3-tier prompt system ---
    /// Path to template prompt file (role-specific behavior rules).
    /// When set, overrides backend-specific system_prompt_file.
    pub template_prompt: Option<String>,
    /// Path to instance prompt file (individual NPC personality).
    pub instance_prompt: Option<String>,
    /// Path to memory file (accumulated experiences, auto-updated by LLM).
    pub memory_file: Option<String>,
    /// Path to schedule file (time-based positioning).
    pub schedule_file: Option<String>,
}

/// Resources shared across all NPC connections.
pub struct SharedResources {
    pub height_sampler: Arc<HeightSampler>,
    pub world_cache: Arc<std::sync::RwLock<WorldCache>>,
    pub ai_templates: Arc<HashMap<String, AiTemplate>>,
    pub type_mapping: Arc<HashMap<String, String>>,
    pub scheduler: LlmScheduler,
}

/// Run the orchestrator: spawn all NPC sessions in parallel.
pub async fn run_orchestrator(
    server_url: String,
    npcs: Vec<NpcConfig>,
    shared: Arc<SharedResources>,
) -> anyhow::Result<()> {
    info!(
        "Orchestrator starting with {} NPC connection(s)",
        npcs.len()
    );

    let mut handles = Vec::new();
    for (i, npc) in npcs.into_iter().enumerate() {
        let url = server_url.clone();
        let shared = Arc::clone(&shared);
        let handle = tokio::spawn(async move {
            info!("[NPC {}] Starting session loop for '{}'", i, npc.account);
            run_npc_loop(&url, &npc, &shared).await;
        });
        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

/// Reconnect loop for a single NPC.
async fn run_npc_loop(server_url: &str, npc: &NpcConfig, shared: &SharedResources) {
    loop {
        match run_npc_session(server_url, npc, shared).await {
            Ok(()) => {
                info!(
                    "[{}] Session ended cleanly. Reconnecting in {}s...",
                    npc.account,
                    RECONNECT_DELAY.as_secs()
                );
            }
            Err(e) => {
                warn!(
                    "[{}] Session failed: {e}. Reconnecting in {}s...",
                    npc.account,
                    RECONNECT_DELAY.as_secs()
                );
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// Run a single game session for one NPC: connect, authenticate, enter game, run until disconnected.
async fn run_npc_session(
    server_url: &str,
    npc: &NpcConfig,
    shared: &SharedResources,
) -> anyhow::Result<()> {
    let password_hash = fnv1a_hash(&npc.password);

    let ws_stream = ws::connect_ws(server_url, &npc.account).await;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    // --- Authentication (auto-create account if needed) ---
    ws::send(
        &mut ws_tx,
        &ClientMessage::Authenticate {
            account_name: npc.account.clone(),
            password_hash: password_hash.clone(),
            create_account: npc.create_account,
            is_npc: true,
        },
    )
    .await?;

    let auth_result = ws::wait_for_auth(&mut ws_rx, &npc.account).await;
    let mut characters = match auth_result {
        Ok(chars) => chars,
        Err(e) => {
            let err_msg = e.to_string();
            if !npc.create_account && err_msg.contains("Account not found") {
                info!("[{}] Account not found, creating account...", npc.account);
                // Reconnect since the server may have closed the connection
                drop(ws_rx);
                let ws_stream = ws::connect_ws(server_url, &npc.account).await;
                let (new_tx, new_rx) = ws_stream.split();
                ws_tx = new_tx;
                ws_rx = new_rx;
                ws::send(
                    &mut ws_tx,
                    &ClientMessage::Authenticate {
                        account_name: npc.account.clone(),
                        password_hash: password_hash.clone(),
                        create_account: true,
                        is_npc: true,
                    },
                )
                .await?;
                ws::wait_for_auth(&mut ws_rx, &npc.account).await?
            } else {
                return Err(e);
            }
        }
    };

    // --- Delete characters whose class or name doesn't match config ---
    let desired_class = npc
        .character_class
        .as_deref()
        .map(|c| {
            c.parse::<onlinerpg_shared::CharacterClass>().map_err(|_| {
                anyhow::anyhow!(
                    "invalid character_class {c:?} in config for [{}]",
                    npc.account
                )
            })
        })
        .transpose()?;
    let desired_name = npc.character_name.as_deref();

    let should_delete = |c: &onlinerpg_shared::Character| {
        desired_class.as_ref().is_some_and(|d| c.class != *d)
            || desired_name.is_some_and(|n| c.name != n)
    };

    for c in characters.iter().filter(|c| should_delete(c)) {
        info!(
            "[{}] Deleting character '{}' (id={}, {:?}) — mismatch (want name={:?}, class={:?})",
            npc.account, c.name, c.id, c.class, desired_name, desired_class
        );
        ws::send(
            &mut ws_tx,
            &ClientMessage::DeleteCharacter { character_id: c.id },
        )
        .await?;
        ws::wait_for_msg(&mut ws_rx, &npc.account, "CharacterDeleted", |msg| {
            matches!(
                msg,
                ServerMessage::CharacterDeleted { .. } | ServerMessage::CharacterError { .. }
            )
        })
        .await?;
    }
    characters.retain(|c| !should_delete(c));

    // --- Auto-create character if needed ---
    if characters.is_empty() {
        if let Some(ref char_name) = npc.character_name {
            let class = desired_class.unwrap_or(onlinerpg_shared::CharacterClass::Knight);

            info!(
                "[{}] No characters found. Creating '{}' ({:?})...",
                npc.account, char_name, class
            );

            // Roll stats
            ws::send(
                &mut ws_tx,
                &ClientMessage::RollCharacterStats {
                    character_class: class.clone(),
                    gender: onlinerpg_shared::Gender::default(),
                },
            )
            .await?;
            ws::wait_for_msg(&mut ws_rx, &npc.account, "CharacterStatsRolled", |msg| {
                matches!(msg, ServerMessage::CharacterStatsRolled { .. })
            })
            .await?;

            // Create character
            ws::send(
                &mut ws_tx,
                &ClientMessage::CreateCharacter {
                    character_name: char_name.clone(),
                    character_class: class,
                    gender: onlinerpg_shared::Gender::default(),
                },
            )
            .await?;
            let created = ws::wait_for_msg(&mut ws_rx, &npc.account, "CharacterCreated", |msg| {
                matches!(
                    msg,
                    ServerMessage::CharacterCreated { .. } | ServerMessage::CharacterError { .. }
                )
            })
            .await?;
            match created {
                ServerMessage::CharacterCreated { character } => {
                    info!(
                        "[{}] Created character '{}' (id={}, {:?})",
                        npc.account, character.name, character.id, character.class
                    );
                    characters.push(character);
                }
                ServerMessage::CharacterError { message } => {
                    anyhow::bail!("[{}] Failed to create character: {message}", npc.account);
                }
                _ => unreachable!(),
            }
        }
    }

    let llm_enabled = npc.llm != LlmType::None;
    let enter_char_id = if llm_enabled {
        characters.first().map(|c| c.id)
    } else {
        None
    };

    if let Some(char_id) = enter_char_id {
        ws::send(
            &mut ws_tx,
            &ClientMessage::EnterGame {
                character_id: char_id,
            },
        )
        .await?;
        info!(
            "[{}] Entering game with character {char_id}...",
            npc.account
        );
    }

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientMessage>(32);
    let state = Arc::new(Mutex::new(SharedState::new(
        characters,
        cmd_tx,
        Arc::clone(&shared.height_sampler),
        Arc::clone(&shared.world_cache),
    )));

    let account_for_tx = npc.account.clone();
    let tx_task = tokio::spawn(async move {
        while let Some(msg) = cmd_rx.recv().await {
            if let Err(e) = ws::send(&mut ws_tx, &msg).await {
                error!("[{}] Failed to send command: {e}", account_for_tx);
                break;
            }
        }
    });

    let state_for_rx = Arc::clone(&state);
    let account_for_rx = npc.account.clone();
    let rx_task = tokio::spawn(async move {
        loop {
            match ws::recv(&mut ws_rx).await {
                Ok(msg) => {
                    if matches!(msg, onlinerpg_shared::ServerMessage::GameTimeSync { .. }) {
                        let mut s = state_for_rx.lock().await;
                        let _ = s.send_command(ClientMessage::Heartbeat).await;
                        s.push_event(msg);
                        continue;
                    }

                    let needs_height_sync = matches!(
                        msg,
                        onlinerpg_shared::ServerMessage::JoinSuccess { .. }
                            | onlinerpg_shared::ServerMessage::PlayerRespawned { .. }
                    );

                    let mut s = state_for_rx.lock().await;
                    s.push_event(msg);

                    if needs_height_sync {
                        if let Err(e) = s.sync_height().await {
                            warn!(
                                "[{}] Failed to sync height after spawn: {e}",
                                account_for_rx
                            );
                        }
                    }
                }
                Err(e) => {
                    error!("[{}] Connection lost: {e}", account_for_rx);
                    break;
                }
            }
        }
    });

    let llm_task = spawn_llm_task(npc, &state, &shared.scheduler, server_url);

    // Monster AI tick task (1Hz)
    let state_for_ai = Arc::clone(&state);
    let templates_for_ai = Arc::clone(&shared.ai_templates);
    let mapping_for_ai = Arc::clone(&shared.type_mapping);
    let ai_task = tokio::spawn(async move {
        let tick_interval = Duration::from_secs(1);
        let mut interval = tokio::time::interval(tick_interval);
        let delta_ms = 1000.0_f32;

        {
            let mut s = state_for_ai.lock().await;
            s.monster_ai.set_templates((*templates_for_ai).clone());
            s.monster_ai.set_type_mapping((*mapping_for_ai).clone());
        }

        loop {
            interval.tick().await;
            let mut s = state_for_ai.lock().await;
            if !s.in_game {
                continue;
            }

            // Clone Arc to avoid borrow conflict: world_cache (immutable) vs monster_ai (mutable).
            // Must drop the RwLockReadGuard before any .await (not Send).
            let (commands, pending) = {
                let wc = Arc::clone(&s.world_cache);
                let world = wc.read().unwrap();
                let SharedState {
                    ref nearby_players,
                    ref mut monster_ai,
                    ..
                } = *s;
                let cmds = monster_ai.tick_all(delta_ms, nearby_players, world.passability_cache());
                drop(world);
                let pending = s.drain_pending_commands();
                (cmds, pending)
            };

            for cmd in commands.into_iter().chain(pending) {
                if let Err(e) = s.send_command(cmd).await {
                    tracing::warn!("Monster AI command failed: {e}");
                    break;
                }
            }
        }
    });

    if llm_enabled {
        info!("[{}] Running in LLM-driven mode", npc.account);
    } else {
        info!("[{}] Running in direct mode", npc.account);
    }

    // Wait until the WebSocket reader dies (connection lost)
    let _ = rx_task.await;

    tx_task.abort();
    ai_task.abort();
    if let Some(t) = llm_task {
        t.abort();
    }

    Ok(())
}

impl NpcConfig {
    /// Get the backend-specific system_prompt_file path.
    fn system_prompt_file(&self) -> Option<&str> {
        match &self.llm {
            LlmType::Claude => Some(&self.claude.system_prompt_file),
            LlmType::Openrouter => Some(&self.openrouter.system_prompt_file),
            LlmType::Codex => Some(&self.codex.system_prompt_file),
            LlmType::None => None,
        }
    }
}

/// Build the system prompt for an NPC.
///
/// If `template_prompt` is set, uses the 3-tier system (template + instance + memory).
/// Otherwise falls back to the backend-specific `system_prompt_file`.
fn build_system_prompt(npc: &NpcConfig) -> anyhow::Result<String> {
    if let Some(ref template_path) = npc.template_prompt {
        let mut parts = vec![driver::load_system_prompt(template_path)?];
        if let Some(ref instance_path) = npc.instance_prompt {
            parts.push(driver::load_system_prompt(instance_path)?);
        }
        if let Some(ref memory_path) = npc.memory_file {
            match std::fs::read_to_string(memory_path) {
                Ok(content) if !content.trim().is_empty() => {
                    parts.push(format!("=== YOUR MEMORIES ===\n{content}"));
                }
                Ok(_) => {}
                Err(_) => {
                    let _ = std::fs::write(memory_path, "");
                }
            }
        }
        info!(
            "[{}] Using 3-tier prompt: template={template_path}{}{}",
            npc.account,
            npc.instance_prompt
                .as_deref()
                .map(|p| format!(", instance={p}"))
                .unwrap_or_default(),
            npc.memory_file
                .as_deref()
                .map(|p| format!(", memory={p}"))
                .unwrap_or_default(),
        );
        Ok(parts.join("\n\n"))
    } else {
        match npc.system_prompt_file() {
            Some(path) => driver::load_system_prompt(path),
            None => Ok(String::new()),
        }
    }
}

/// Spawn the appropriate LLM driver task based on NPC config.
fn spawn_llm_task(
    npc: &NpcConfig,
    state: &Arc<Mutex<SharedState>>,
    scheduler: &LlmScheduler,
    server_url: &str,
) -> Option<tokio::task::JoinHandle<()>> {
    let min_interval = Duration::from_secs(npc.min_interval_secs);
    let debounce = Duration::from_secs(npc.debounce_secs);
    let idle_interval = Duration::from_secs(npc.idle_interval_secs);
    let activity_window = Duration::from_secs(npc.activity_window_secs);

    let system_prompt = match build_system_prompt(npc) {
        Ok(p) => p,
        Err(e) => {
            error!("[{}] Failed to build system prompt: {e}", npc.account);
            return None;
        }
    };

    let invoker: Arc<dyn driver::LlmBackend> = match npc.llm {
        LlmType::Claude => {
            info!(
                "[{}] Claude CLI integration enabled (model={})",
                npc.account, npc.claude.model
            );
            match claude::ClaudeInvoker::new(&npc.claude, system_prompt) {
                Ok(inv) => Arc::new(inv),
                Err(e) => {
                    error!("[{}] Failed to create Claude invoker: {e}", npc.account);
                    return None;
                }
            }
        }
        LlmType::Openrouter => {
            info!(
                "[{}] OpenRouter API integration enabled (model={})",
                npc.account, npc.openrouter.model
            );
            match openrouter::OpenRouterInvoker::new(&npc.openrouter, system_prompt) {
                Ok(inv) => Arc::new(inv),
                Err(e) => {
                    error!("[{}] Failed to create OpenRouter invoker: {e}", npc.account);
                    return None;
                }
            }
        }
        LlmType::Codex => {
            info!(
                "[{}] Codex CLI integration enabled (model={})",
                npc.account, npc.codex.model
            );
            match codex::CodexInvoker::new(&npc.codex, system_prompt) {
                Ok(inv) => Arc::new(inv),
                Err(e) => {
                    error!("[{}] Failed to create Codex invoker: {e}", npc.account);
                    return None;
                }
            }
        }
        LlmType::None => return None,
    };

    let state = Arc::clone(state);
    let scheduler = scheduler.clone();
    let schedule = if let Some(ref path) = npc.schedule_file {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<ScheduleFile>(&content) {
                Ok(mut f) => {
                    // Validate all conditions at load time
                    let mut valid = true;
                    for entry in &mut f.schedule {
                        if let Err(e) = entry.parse_condition() {
                            error!("[{}] Schedule entry error: {e}", npc.account);
                            valid = false;
                        }
                    }
                    if valid {
                        info!(
                            "[{}] Loaded {} schedule entries from {path}",
                            npc.account,
                            f.schedule.len()
                        );
                        f.schedule
                    } else {
                        Vec::new()
                    }
                }
                Err(e) => {
                    error!(
                        "[{}] Failed to parse schedule file {path}: {e}",
                        npc.account
                    );
                    Vec::new()
                }
            },
            Err(e) => {
                error!("[{}] Failed to read schedule file {path}: {e}", npc.account);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Derive HTTP API base URL from WebSocket URL.
    // The terrain/housing REST API runs on game port + 1.
    let api_base_url = {
        let http_url = server_url
            .replace("wss://", "https://")
            .replace("ws://", "http://");
        // Bump the port by 1 (e.g. ws://host:10006 → http://host:10007)
        if let Some(colon_pos) = http_url.rfind(':') {
            if let Ok(port) = http_url[colon_pos + 1..].parse::<u16>() {
                format!("{}{}", &http_url[..colon_pos + 1], port + 1)
            } else {
                http_url
            }
        } else {
            http_url
        }
    };

    let driver_config = driver::DriverConfig {
        label: npc.account.clone(),
        memory_file: npc.memory_file.clone(),
        min_interval,
        debounce,
        idle_interval,
        activity_window,
        schedule,
        api_base_url,
    };
    Some(tokio::spawn(async move {
        driver::llm_driver(state, invoker, scheduler, driver_config).await;
    }))
}
