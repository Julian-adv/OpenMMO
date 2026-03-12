use std::sync::Arc;
use std::time::{Duration, Instant};

use onlinerpg_shared::{ClientMessage, ServerMessage};
use serde::Deserialize;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::mcp::format_event;
use crate::state::SharedState;

/// Configuration for the Claude CLI integration.
#[derive(Debug, Clone, Deserialize)]
pub struct ClaudeConfig {
    /// Enable Claude CLI integration (default: false)
    #[serde(default)]
    pub enabled: bool,
    /// Model to use (default: "sonnet")
    #[serde(default = "default_model")]
    pub model: String,
    /// Minimum interval between prompts in seconds (default: 5)
    #[serde(default = "default_min_interval")]
    pub min_interval_secs: u64,
    /// Debounce window for batching urgent events in seconds (default: 2)
    #[serde(default = "default_debounce")]
    pub debounce_secs: u64,
    /// Path to system prompt file (default: "data/system_prompt.txt")
    #[serde(default = "default_system_prompt_file")]
    pub system_prompt_file: String,
}

fn default_model() -> String {
    "sonnet".to_string()
}
fn default_min_interval() -> u64 {
    5
}

fn default_debounce() -> u64 {
    2
}
fn default_system_prompt_file() -> String {
    "data/system_prompt.txt".to_string()
}

/// Load system prompt from file.
pub fn load_system_prompt(path: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read system prompt from {path}: {e}"))
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: default_model(),
            min_interval_secs: default_min_interval(),
            debounce_secs: default_debounce(),
            system_prompt_file: default_system_prompt_file(),
        }
    }
}

// --- JSON output type ---

/// JSON output from `claude -p --output-format json`.
#[derive(Debug, Deserialize)]
struct JsonOutput {
    result: Option<String>,
    session_id: Option<String>,
}

/// Parsed agent response.
#[derive(Debug, Deserialize)]
pub struct AgentResponse {
    pub thought: Option<String>,
    pub actions: Vec<AgentAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AgentAction {
    #[serde(rename = "say")]
    Say { message: String },
    #[serde(rename = "attack")]
    Attack {
        #[serde(alias = "targetId", alias = "target_id", alias = "target", alias = "id")]
        monster_id: String,
    },
    #[serde(rename = "move")]
    Move { x: f32, y: f32, z: f32 },
    #[serde(rename = "respawn")]
    Respawn,
    #[serde(rename = "wait", alias = "idle", alias = "observe", alias = "none")]
    Wait,
}

/// Invokes `claude -p` per prompt, using `--resume` for conversation continuity.
/// First call captures session_id, subsequent calls resume that session.
pub struct ClaudeInvoker {
    config: ClaudeConfig,
    system_prompt: String,
    session_id: Mutex<Option<String>>,
}

impl ClaudeInvoker {
    pub fn new(config: &ClaudeConfig) -> anyhow::Result<Self> {
        let system_prompt = load_system_prompt(&config.system_prompt_file)?;
        info!(
            "Claude invoker ready (model={}, prompt_file={})",
            config.model, config.system_prompt_file
        );
        Ok(Self {
            config: config.clone(),
            system_prompt,
            session_id: Mutex::new(None),
        })
    }

    /// Send a prompt and collect the response.
    pub async fn send_message(&self, content: &str) -> anyhow::Result<String> {
        info!(">>> TO CLAUDE ({} bytes):\n{}", content.len(), content);

        let session_id = self.session_id.lock().await.clone();

        let mut cmd = Command::new("claude");
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--model")
            .arg(&self.config.model);

        if let Some(ref sid) = session_id {
            cmd.arg("--resume").arg(sid);
        } else {
            cmd.arg("--system-prompt").arg(&self.system_prompt);
        }

        cmd.arg(content)
            .env_remove("CLAUDECODE")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn claude CLI: {e}"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture claude stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to capture claude stderr"))?;

        // Log stderr in background
        tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "claude_stderr", "{}", line);
            }
        });

        // Read entire stdout (single JSON object)
        let mut raw = String::new();
        let mut reader = BufReader::new(stdout);
        tokio::io::AsyncReadExt::read_to_string(&mut reader, &mut raw).await?;

        // Wait for process to finish
        let status = child.wait().await?;
        if !status.success() {
            warn!("Claude process exited with status: {status}");
        }

        // Parse JSON output
        let output: JsonOutput = serde_json::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("Failed to parse claude JSON output: {e}\nRaw: {raw}"))?;

        let full_text = output.result.unwrap_or_default();

        // Store session_id for future --resume calls
        if let Some(sid) = output.session_id {
            let mut stored = self.session_id.lock().await;
            if stored.is_none() {
                info!("Claude session established: {sid}");
                *stored = Some(sid);
            }
        }

        info!("<<< FROM CLAUDE ({} bytes):\n{}", full_text.len(), full_text);
        Ok(full_text)
    }
}

/// Parse a raw text response from Claude into structured actions.
pub fn parse_agent_response(text: &str) -> anyhow::Result<AgentResponse> {
    // Try to extract JSON from the response (Claude might wrap it in markdown)
    let json_str = extract_json(text);
    serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("Failed to parse agent response: {e}\nRaw: {text}"))
}

/// Extract JSON object from text that might contain markdown code blocks.
fn extract_json(text: &str) -> &str {
    let trimmed = text.trim();

    // Try to find ```json ... ``` block
    if let Some(start) = trimmed.find("```json") {
        let after_marker = &trimmed[start + 7..];
        if let Some(end) = after_marker.find("```") {
            return after_marker[..end].trim();
        }
    }

    // Try to find ``` ... ``` block
    if let Some(start) = trimmed.find("```") {
        let after_marker = &trimmed[start + 3..];
        if let Some(end) = after_marker.find("```") {
            return after_marker[..end].trim();
        }
    }

    // Try to find raw JSON object
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            return &trimmed[start..=end];
        }
    }

    trimmed
}

/// Convert an AgentAction into a ClientMessage for the game server.
pub fn action_to_command(action: &AgentAction) -> Option<ClientMessage> {
    match action {
        AgentAction::Say { message } => Some(ClientMessage::ChatMessage {
            message: message.clone(),
        }),
        AgentAction::Attack { monster_id } => Some(ClientMessage::PlayerAttack {
            monster_id: monster_id.clone(),
        }),
        AgentAction::Move { x, y, z } => Some(ClientMessage::PlayerMove {
            position: onlinerpg_shared::Position {
                x: *x,
                y: *y,
                z: *z,
            },
            rotation: 0.0,
        }),
        AgentAction::Respawn => Some(ClientMessage::RequestRespawn),
        AgentAction::Wait => None,
    }
}

/// Build a prompt string from current state and events.
pub fn build_prompt(state: &SharedState, events: &[ServerMessage]) -> String {
    let mut prompt = String::new();

    prompt.push_str("=== CURRENT STATE ===\n");
    prompt.push_str(&state.format_world_state());
    prompt.push('\n');

    if !events.is_empty() {
        prompt.push_str("\n=== EVENTS ===\n");
        for event in events {
            prompt.push_str(&format_event(event));
            prompt.push('\n');
        }
    }

    prompt.push_str("\nWhat do you do?");
    prompt
}

/// The main Claude driver loop. Runs as a tokio task.
///
/// Waits for urgent events (via Notify), debounces, builds prompts,
/// sends to Claude, and executes resulting actions.
pub async fn claude_driver(
    state: Arc<Mutex<SharedState>>,
    config: ClaudeConfig,
) {
    let min_interval = Duration::from_secs(config.min_interval_secs);
    let debounce = Duration::from_secs(config.debounce_secs);

    let urgent_notify = {
        let s = state.lock().await;
        Arc::clone(&s.urgent_notify)
    };

    // Wait until we're in the game
    loop {
        {
            let s = state.lock().await;
            if s.in_game {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    info!("Claude driver: in game, initializing invoker...");

    let invoker = match ClaudeInvoker::new(&config) {
        Ok(i) => i,
        Err(e) => {
            error!("Failed to create claude invoker: {e}");
            return;
        }
    };

    let mut last_prompt_at = Instant::now() - min_interval; // allow immediate first prompt

    // Send initial world state (lock must be released before calling Claude)
    let initial_prompt = {
        let s = state.lock().await;
        build_prompt(&*s, &[])
    };
    info!("Claude driver: sending initial world state");
    match invoker.send_message(&initial_prompt).await {
        Ok(response) => {
            handle_response(&state, &response).await;
            last_prompt_at = Instant::now();
        }
        Err(e) => {
            error!("Claude initial prompt failed: {e}");
        }
    }

    loop {
        // Wait for urgent notification or periodic routine check
        tokio::select! {
            _ = urgent_notify.notified() => {
                debug!("Claude driver: urgent event received");
            }
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                debug!("Claude driver: periodic check");
            }
        }

        // Enforce minimum interval (debounce is included within min_interval)
        let elapsed = last_prompt_at.elapsed();
        if elapsed < min_interval {
            let wait = min_interval - elapsed;
            debug!("Claude driver: rate limiting, waiting {wait:?}");
            tokio::time::sleep(wait).await;
        } else {
            // Already past min_interval, but still debounce briefly to batch events
            tokio::time::sleep(debounce).await;
        }

        // Drain events and build prompt
        let (prompt, has_events) = {
            let mut s = state.lock().await;
            let events = s.drain_events();
            let has_events = !events.is_empty();
            let prompt = build_prompt(&*s, &events);
            (prompt, has_events)
        };

        if !has_events {
            debug!("Claude driver: no events to process, skipping");
            continue;
        }

        // Send prompt and process response
        info!("Claude driver: sending prompt ({} chars)", prompt.len());
        match invoker.send_message(&prompt).await {
            Ok(response) => {
                handle_response(&state, &response).await;
                last_prompt_at = Instant::now();
            }
            Err(e) => {
                error!("Claude prompt failed: {e}");
            }
        }
    }
}

/// Minimum distance to a monster before attacking (matches client-side threshold).
const ATTACK_RANGE: f32 = 2.0;
/// Character movement speed in units/sec (matches client DEFAULT_MOVEMENT_CONFIG.maxSpeed).
const MOVE_SPEED: f32 = 3.0;
/// Extra buffer time (seconds) so the client-side interpolation fully arrives before attack.
const ARRIVAL_BUFFER_SECS: f32 = 0.3;

/// Parse and execute the agent's response.
async fn handle_response(state: &Arc<Mutex<SharedState>>, response: &str) {
    match parse_agent_response(response) {
        Ok(agent_resp) => {
            if let Some(ref thought) = agent_resp.thought {
                info!("Agent thought: {thought}");
            }
            for action in &agent_resp.actions {
                info!("Agent action: {action:?}");

                // For attack actions, walk to the monster first if not in range
                if let AgentAction::Attack { monster_id } = action {
                    let move_info = {
                        let s = state.lock().await;
                        compute_move_to_monster(&s, monster_id)
                    };
                    if let Some((cmd, travel_secs)) = move_info {
                        info!(
                            "Auto-moving to monster {monster_id} ({travel_secs:.1}s travel time)"
                        );
                        {
                            let mut s = state.lock().await;
                            if let Err(e) = s.send_command(cmd).await {
                                error!("Failed to send move-to-monster command: {e}");
                            }
                        }
                        // Wait for the character to arrive on other clients' screens
                        let wait = Duration::from_secs_f32(travel_secs + ARRIVAL_BUFFER_SECS);
                        tokio::time::sleep(wait).await;
                    }
                }

                if let Some(cmd) = action_to_command(action) {
                    let mut s = state.lock().await;
                    if let Err(e) = s.send_command(cmd).await {
                        error!("Failed to send agent command: {e}");
                    }
                }
            }
        }
        Err(e) => {
            warn!("Failed to parse agent response: {e}");
            warn!("Raw response: {response}");
        }
    }
}

/// If the agent is too far from the target monster, return a PlayerMove command
/// and the estimated travel time in seconds (based on client walk speed).
fn compute_move_to_monster(
    state: &SharedState,
    monster_id: &str,
) -> Option<(ClientMessage, f32)> {
    let monster = state.nearby_monsters.get(monster_id)?;
    let self_player = state.self_player.as_ref()?;

    let dx = monster.position.x - self_player.position.x;
    let dz = monster.position.z - self_player.position.z;
    let dist = (dx * dx + dz * dz).sqrt();

    if dist <= ATTACK_RANGE {
        return None; // Already in range
    }

    // Move to a point just inside ATTACK_RANGE from the monster
    let move_dist = dist - ATTACK_RANGE + 0.5;
    let ratio = move_dist / dist;
    let target_x = self_player.position.x + dx * ratio;
    let target_z = self_player.position.z + dz * ratio;

    // Estimate travel time accounting for acceleration/deceleration.
    // Client uses accel=6, decel=6, maxSpeed=3. For simplicity, use average
    // speed ≈ 0.7 * maxSpeed for short distances, approaching maxSpeed for longer ones.
    let avg_speed = if move_dist < 3.0 {
        MOVE_SPEED * 0.65
    } else {
        MOVE_SPEED * 0.85
    };
    let travel_secs = move_dist / avg_speed;

    let cmd = ClientMessage::PlayerMove {
        position: onlinerpg_shared::Position {
            x: target_x,
            y: monster.position.y,
            z: target_z,
        },
        rotation: dz.atan2(dx),
    };

    Some((cmd, travel_secs))
}
