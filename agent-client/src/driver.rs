use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use onlinerpg_shared::{ClientMessage, ServerMessage};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::state::SharedState;

/// Trait for LLM backends that can send a prompt and return a text response.
#[async_trait]
pub trait LlmBackend: Send + Sync {
    async fn send_message(&self, content: &str) -> anyhow::Result<String>;
}

/// Parsed agent response.
#[derive(Debug, Deserialize)]
pub struct AgentResponse {
    #[allow(dead_code)]
    pub thought: Option<String>,
    pub actions: Vec<AgentAction>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AgentAction {
    #[serde(rename = "say", alias = "chat")]
    Say { message: String },
    #[serde(rename = "attack")]
    Attack {
        #[serde(alias = "targetId", alias = "target_id", alias = "target", alias = "id")]
        monster_id: String,
    },
    #[serde(rename = "move")]
    Move {
        // Absolute coordinates (preferred)
        x: Option<f32>,
        y: Option<f32>,
        z: Option<f32>,
        // Direction + distance fallback (LLMs sometimes use this)
        direction: Option<String>,
        distance: Option<f32>,
    },
    #[serde(rename = "respawn")]
    Respawn,
    #[serde(rename = "wait", alias = "idle", alias = "observe", alias = "none")]
    Wait,
}

/// Load system prompt from file.
pub fn load_system_prompt(path: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read system prompt from {path}: {e}"))
}

/// Resolve a player_id to a display name using SharedState.
/// Falls back to the raw ID if the player is not found.
fn player_name(state: &SharedState, player_id: &str) -> String {
    if state.self_player_id.as_deref() == Some(player_id) {
        if let Some(ref p) = state.self_player {
            return p.name.clone();
        }
    }
    if let Some(p) = state.nearby_players.get(player_id) {
        return p.name.clone();
    }
    player_id.to_string()
}

/// Format a server event as a human-readable line for LLM prompts.
/// Returns `None` for events that should not be forwarded to the LLM.
pub fn format_event(state: &SharedState, msg: &ServerMessage) -> Option<String> {
    match msg {
        ServerMessage::JoinSuccess { player } => Some(format!(
            "[Join] You joined as {} at ({:.1}, {:.1}, {:.1})",
            player.name, player.position.x, player.position.y, player.position.z
        )),
        ServerMessage::GameState {
            players, monsters, ..
        } => {
            let mut lines = vec![format!(
                "[World] {} player(s), {} monster(s)",
                players.len(),
                monsters.len()
            )];
            for p in players.values() {
                lines.push(format!(
                    "  Player: {} Lv.{} HP {}/{}",
                    p.name, p.level, p.health, p.max_health
                ));
            }
            for m in monsters.values() {
                lines.push(format!(
                    "  Monster: {} [{}] HP {}/{}",
                    m.monster_type, m.id, m.health, m.max_health
                ));
            }
            Some(lines.join("\n"))
        }
        ServerMessage::GameTimeSync { datetime, is_night } => Some(format!(
            "[Time] Y{} M{} D{} {:02}:{:02} ({})",
            datetime.year,
            datetime.month,
            datetime.day,
            datetime.hour,
            datetime.minute,
            if *is_night { "night" } else { "day" }
        )),
        ServerMessage::ChatMessage {
            player_id, message, ..
        } => Some(format!("[Chat] {}: {message}", player_name(state, player_id))),
        ServerMessage::PlayerJoined { player } => {
            Some(format!("[PlayerJoined] {}", player.name))
        }
        ServerMessage::PlayerLeft { player_id } => {
            Some(format!("[PlayerLeft] {}", player_name(state, player_id)))
        }
        ServerMessage::PlayerMoved {
            player_id,
            position,
            ..
        } => Some(format!(
            "[Move] {} -> ({:.1}, {:.1}, {:.1})",
            player_name(state, player_id),
            position.x, position.y, position.z
        )),
        ServerMessage::MonsterSpawned { monster } => Some(format!(
            "[MonsterSpawned] {} ({})",
            monster.id, monster.monster_type
        )),
        ServerMessage::MonsterDead { monster_id } => {
            Some(format!("[MonsterDead] {monster_id}"))
        }
        ServerMessage::PlayerAttacked {
            player_id,
            monster_id,
            hit,
            damage,
            ..
        } => Some(format!(
            "[Attack] {} -> {monster_id}: hit={hit} dmg={damage}",
            player_name(state, player_id)
        )),
        ServerMessage::MonsterAttackedPlayer {
            monster_id,
            player_id,
            hit,
            damage,
            current_health,
            ..
        } => Some(format!(
            "[MonsterAttack] {monster_id} -> {}: hit={hit} dmg={damage} hp={current_health}",
            player_name(state, player_id)
        )),
        ServerMessage::PlayerDead { player_id } => {
            Some(format!("[PlayerDead] {}", player_name(state, player_id)))
        }
        ServerMessage::PlayerRespawned { player } => Some(format!(
            "[Respawn] {} HP {}/{}",
            player.name, player.health, player.max_health
        )),
        ServerMessage::XpGained {
            xp_amount,
            total_xp,
            new_level,
            leveled_up,
            ..
        } => {
            let mut s = format!("[XP] +{xp_amount} (total: {total_xp}, level: {new_level})");
            if *leveled_up {
                s.push_str(" LEVEL UP!");
            }
            Some(s)
        }
        ServerMessage::CharacterError { message } => {
            Some(format!("[CharacterError] {message}"))
        }
        ServerMessage::CharacterCreated { character } => Some(format!(
            "[CharacterCreated] id={} {} Lv.{} {:?}",
            character.id, character.name, character.level, character.class
        )),
        ServerMessage::CharacterStatsRolled {
            attributes,
            max_hp,
        } => Some(format!(
            "[StatsRolled] STR:{} DEX:{} CON:{} INT:{} WIS:{} CHA:{} HP:{}",
            attributes.r#str, attributes.dex, attributes.con,
            attributes.int, attributes.wis, attributes.cha, max_hp
        )),
        ServerMessage::MonsterMoved {
            monster_id,
            position,
            state: monster_state,
            ..
        } => Some(format!(
            "[MonsterMoved] {monster_id} -> ({:.1}, {:.1}, {:.1}) state={monster_state}",
            position.x, position.y, position.z
        )),
        ServerMessage::Kicked { reason, .. } => Some(format!("[Kicked] {reason}")),
        // Skip unknown/unhandled event types
        _ => None,
    }
}

/// Parse a raw text response from an LLM into structured actions.
pub fn parse_agent_response(text: &str) -> anyhow::Result<AgentResponse> {
    let json_str = extract_json(text);
    serde_json::from_str(json_str)
        .map_err(|e| anyhow::anyhow!("Failed to parse agent response: {e}\nRaw: {text}"))
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
/// `player_pos` is needed to resolve relative move directions and to compute rotation.
pub fn action_to_command(
    action: &AgentAction,
    player_pos: Option<&onlinerpg_shared::Position>,
) -> Option<ClientMessage> {
    match action {
        AgentAction::Say { message } => Some(ClientMessage::ChatMessage {
            message: message.clone(),
        }),
        AgentAction::Attack { monster_id } => Some(ClientMessage::PlayerAttack {
            monster_id: monster_id.clone(),
        }),
        AgentAction::Move {
            x,
            y,
            z,
            direction,
            distance,
        } => {
            let pos = if let (Some(x), Some(z)) = (x, z) {
                // Absolute coordinates
                let y = y.unwrap_or_else(|| {
                    player_pos.map(|p| p.y).unwrap_or(0.0)
                });
                onlinerpg_shared::Position { x: *x, y, z: *z }
            } else if let (Some(dir), Some(dist), Some(pp)) =
                (direction.as_deref(), distance, player_pos)
            {
                // Direction + distance relative to current position
                let (dx, dz) = direction_to_offset(dir);
                onlinerpg_shared::Position {
                    x: pp.x + dx * dist,
                    y: pp.y,
                    z: pp.z + dz * dist,
                }
            } else {
                warn!("Move action missing coordinates and direction/distance, skipping");
                return None;
            };
            // Compute rotation toward target
            let rotation = if let Some(pp) = player_pos {
                let dx = pos.x - pp.x;
                let dz = pos.z - pp.z;
                dx.atan2(dz)
            } else {
                0.0
            };
            Some(ClientMessage::PlayerMove {
                position: pos,
                rotation,
            })
        }
        AgentAction::Respawn => Some(ClientMessage::RequestRespawn),
        AgentAction::Wait => None,
    }
}

/// Convert a cardinal/ordinal direction string to a (dx, dz) unit offset.
fn direction_to_offset(dir: &str) -> (f32, f32) {
    match dir.to_lowercase().as_str() {
        "north" | "n" => (0.0, -1.0),
        "south" | "s" => (0.0, 1.0),
        "east" | "e" => (1.0, 0.0),
        "west" | "w" => (-1.0, 0.0),
        "northeast" | "ne" => (0.707, -0.707),
        "northwest" | "nw" => (-0.707, -0.707),
        "southeast" | "se" => (0.707, 0.707),
        "southwest" | "sw" => (-0.707, 0.707),
        _ => {
            warn!("Unknown direction '{dir}', defaulting to north");
            (0.0, -1.0)
        }
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
            if let Some(line) = format_event(state, event) {
                prompt.push_str(&line);
                prompt.push('\n');
            }
        }
    }

    prompt.push_str("\nWhat do you do?");
    prompt
}

/// The main LLM agent driver loop. Runs as a tokio task.
///
/// Ticks every ATTACK_COOLDOWN to send attack packets when there's an active
/// target. LLM calls are spawned as background tasks so they don't block combat.
pub async fn llm_driver(
    state: Arc<Mutex<SharedState>>,
    invoker: Arc<dyn LlmBackend>,
    min_interval: Duration,
    debounce: Duration,
) {
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

    info!("LLM driver: in game, ready.");

    let attack_cooldown = load_attack_cooldown();

    let mut last_prompt_at = Instant::now() - min_interval;
    let mut attack_target: Option<String> = None;
    let mut last_attack_at = Instant::now() - attack_cooldown;
    let mut llm_in_flight: Option<tokio::task::JoinHandle<anyhow::Result<String>>> = None;
    let mut prompt_pending_since: Option<Instant> = None;

    // Send initial world state (blocking is fine here, no combat yet)
    let initial_prompt = {
        let s = state.lock().await;
        build_prompt(&*s, &[])
    };
    info!("LLM driver: sending initial world state");
    match invoker.send_message(&initial_prompt).await {
        Ok(response) => {
            attack_target = handle_response(&state, &response).await;
            last_prompt_at = Instant::now();
        }
        Err(e) => {
            error!("LLM initial prompt failed: {e}");
        }
    }

    loop {
        // Tick interval: ATTACK_COOLDOWN when in combat, otherwise 1s (responsive to events)
        let tick_duration = if attack_target.is_some() {
            attack_cooldown.saturating_sub(last_attack_at.elapsed())
        } else {
            Duration::from_secs(1)
        };

        tokio::select! {
            _ = urgent_notify.notified() => {
                debug!("LLM driver: urgent event received");
                // Mark that we want to prompt soon (start debounce window)
                if prompt_pending_since.is_none() && llm_in_flight.is_none() {
                    prompt_pending_since = Some(Instant::now());
                }
            }
            _ = tokio::time::sleep(tick_duration) => {}
        }

        // === Combat tick ===
        if attack_target.is_some() && last_attack_at.elapsed() >= attack_cooldown {
            attack_target = tick_combat(&state, attack_target.unwrap()).await;
            last_attack_at = Instant::now();
        }

        // === Check if LLM response arrived ===
        if let Some(ref handle) = llm_in_flight {
            if handle.is_finished() {
                let handle = llm_in_flight.take().unwrap();
                match handle.await {
                    Ok(Ok(response)) => {
                        let new_target = handle_response(&state, &response).await;
                        if new_target.is_some() {
                            attack_target = new_target;
                        }
                        last_prompt_at = Instant::now();
                    }
                    Ok(Err(e)) => {
                        error!("LLM prompt failed: {e}");
                        last_prompt_at = Instant::now();
                    }
                    Err(e) => {
                        error!("LLM task panicked: {e}");
                        last_prompt_at = Instant::now();
                    }
                }
            }
        }

        // === Maybe start a new LLM prompt ===
        if llm_in_flight.is_some() {
            continue;
        }

        // Periodic prompt if min_interval has passed
        if prompt_pending_since.is_none() && last_prompt_at.elapsed() >= min_interval {
            prompt_pending_since = Some(Instant::now());
        }

        // Debounce: wait at least `debounce` after the trigger before actually prompting
        let ready_to_prompt = prompt_pending_since
            .is_some_and(|t| t.elapsed() >= debounce);

        if !ready_to_prompt {
            continue;
        }
        prompt_pending_since = None;

        // Also ensure min_interval since last prompt
        if last_prompt_at.elapsed() < min_interval {
            continue;
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
            continue;
        }

        // Spawn LLM call as background task (doesn't block combat ticks)
        info!("LLM driver: sending prompt ({} chars)", prompt.len());
        let inv = Arc::clone(&invoker);
        llm_in_flight = Some(tokio::spawn(async move {
            inv.send_message(&prompt).await
        }));
    }
}

/// Execute one combat tick: check if target is alive and in range, chase or attack.
/// Returns Some(monster_id) to keep targeting, or None if combat ended.
async fn tick_combat(state: &Arc<Mutex<SharedState>>, monster_id: String) -> Option<String> {
    // Chase until in range (handles monster movement during chase)
    match chase_monster(state, &monster_id).await {
        ChaseResult::InRange => {}
        ChaseResult::Lost | ChaseResult::Error => {
            info!("Combat ended: monster {monster_id} lost or error during chase");
            return None;
        }
    }

    // Face the monster before attacking (matches web client behavior)
    {
        let mut s = state.lock().await;
        if let Some(face_cmd) = compute_face_monster(&s, &monster_id) {
            if let Err(e) = s.send_command(face_cmd).await {
                error!("Failed to send face-monster move: {e}");
                return None;
            }
        }
    }

    // Send attack
    {
        let mut s = state.lock().await;
        let cmd = ClientMessage::PlayerAttack {
            monster_id: monster_id.clone(),
        };
        if let Err(e) = s.send_command(cmd).await {
            error!("Failed to send attack: {e}");
            return None;
        }
    }

    Some(monster_id)
}

enum ChaseResult {
    InRange,
    Lost,
    Error,
}

/// How often to check monster position during chase (ms).
const CHASE_TICK_MS: u64 = 200;
/// Maximum chase duration before giving up (seconds).
const MAX_CHASE_SECS: f32 = 15.0;
/// How far the monster must move from our last target before we re-route.
const REROUTE_THRESHOLD: f32 = 1.5;

/// Chase the monster until we're in attack range, re-routing if it moves.
/// Polls monster position every CHASE_TICK_MS and sends new move commands
/// when the monster has moved significantly from our current destination.
async fn chase_monster(state: &Arc<Mutex<SharedState>>, monster_id: &str) -> ChaseResult {
    let chase_start = Instant::now();
    let mut last_target = onlinerpg_shared::Position {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    loop {
        // Timeout guard
        if chase_start.elapsed().as_secs_f32() > MAX_CHASE_SECS {
            warn!("Chase timeout for monster {monster_id}");
            return ChaseResult::Lost;
        }

        let move_info = {
            let s = state.lock().await;
            let monster_alive = s.nearby_monsters.contains_key(monster_id);
            let player_alive = s.self_player.as_ref().is_some_and(|p| p.health > 0);
            if !monster_alive || !player_alive {
                return ChaseResult::Lost;
            }
            compute_move_to_monster(&s, monster_id)
        };

        let Some((cmd, _travel_secs)) = move_info else {
            // Already in attack range
            return ChaseResult::InRange;
        };

        // Check if monster moved enough to warrant a new move command
        let new_target = match &cmd {
            ClientMessage::PlayerMove { position, .. } => position.clone(),
            _ => unreachable!(),
        };

        let dx = new_target.x - last_target.x;
        let dz = new_target.z - last_target.z;
        let shift = (dx * dx + dz * dz).sqrt();

        if shift > REROUTE_THRESHOLD || last_target.x == 0.0 && last_target.z == 0.0 {
            debug!(
                "Chase {monster_id}: sending move (monster shifted {shift:.1} units)"
            );
            last_target = new_target;
            let mut s = state.lock().await;
            if let Err(e) = s.send_command(cmd).await {
                error!("Failed to send chase move: {e}");
                return ChaseResult::Error;
            }
        }

        tokio::time::sleep(Duration::from_millis(CHASE_TICK_MS)).await;
    }
}

/// Minimum distance to a monster before attacking (matches client-side threshold).
const ATTACK_RANGE: f32 = 2.0;
/// Character movement speed in units/sec (matches client DEFAULT_MOVEMENT_CONFIG.maxSpeed).
const MOVE_SPEED: f32 = 3.0;
/// Fallback attack cooldown if animation data is unavailable.
const DEFAULT_ATTACK_COOLDOWN_MS: u64 = 1500;

/// Path to animation durations JSON (generated by tools/extract-animation-durations.mjs).
/// Relative to agent-client working directory.
const ANIMATION_DURATIONS_PATH: &str = "data/animation_durations.json";

/// Load the attack (slash1) animation duration from the shared JSON file.
/// Returns the duration as milliseconds, or the default if loading fails.
fn load_attack_cooldown() -> Duration {
    let Ok(text) = std::fs::read_to_string(ANIMATION_DURATIONS_PATH) else {
        warn!("Could not read {ANIMATION_DURATIONS_PATH}, using default attack cooldown");
        return Duration::from_millis(DEFAULT_ATTACK_COOLDOWN_MS);
    };

    // Structure: { "combat_melee": { "slash1": 1.533, ... }, ... }
    let Ok(data) = serde_json::from_str::<HashMap<String, HashMap<String, f64>>>(&text) else {
        warn!("Could not parse {ANIMATION_DURATIONS_PATH}, using default attack cooldown");
        return Duration::from_millis(DEFAULT_ATTACK_COOLDOWN_MS);
    };

    if let Some(duration_secs) = data
        .get("combat_melee")
        .and_then(|m| m.get("slash1"))
    {
        let ms = (*duration_secs * 1000.0) as u64;
        info!("Loaded attack cooldown from animation data: {ms}ms");
        Duration::from_millis(ms)
    } else {
        warn!("slash1 animation not found in {ANIMATION_DURATIONS_PATH}, using default");
        Duration::from_millis(DEFAULT_ATTACK_COOLDOWN_MS)
    }
}

/// Parse and execute the agent's response.
/// Returns the monster_id if the last action was an attack (for combat loop).
async fn handle_response(state: &Arc<Mutex<SharedState>>, response: &str) -> Option<String> {
    let agent_resp = match parse_agent_response(response) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to parse agent response: {e}");
            warn!("Raw response: {response}");
            return None;
        }
    };

    let mut last_attack_target = None;

    for action in &agent_resp.actions {
        // For attack actions, chase the monster and attack
        if let AgentAction::Attack { monster_id } = action {
            info!("Agent attacking monster {monster_id}, chasing...");
            match chase_monster(state, monster_id).await {
                ChaseResult::InRange => {
                    // Face the monster before attacking
                    let mut s = state.lock().await;
                    if let Some(face_cmd) = compute_face_monster(&s, monster_id) {
                        if let Err(e) = s.send_command(face_cmd).await {
                            error!("Failed to send face-monster move: {e}");
                        }
                    }
                }
                ChaseResult::Lost | ChaseResult::Error => {
                    warn!("Could not reach monster {monster_id}, skipping attack");
                    continue;
                }
            }
            last_attack_target = Some(monster_id.clone());
        }

        {
            let mut s = state.lock().await;
            let player_pos = s.self_player.as_ref().map(|p| &p.position).cloned();
            if let Some(cmd) = action_to_command(action, player_pos.as_ref()) {
                if let Err(e) = s.send_command(cmd).await {
                    error!("Failed to send agent command: {e}");
                }
            }
        }
    }

    last_attack_target
}

/// Return a PlayerMove command at the current position but rotated to face the monster.
/// Matches the web client's behavior of sending a position sync with facing rotation
/// before each attack.
fn compute_face_monster(state: &SharedState, monster_id: &str) -> Option<ClientMessage> {
    let monster = state.nearby_monsters.get(monster_id)?;
    let self_player = state.self_player.as_ref()?;

    let dx = monster.position.x - self_player.position.x;
    let dz = monster.position.z - self_player.position.z;
    let rotation = dx.atan2(dz);

    Some(ClientMessage::PlayerMove {
        position: self_player.position.clone(),
        rotation,
    })
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
        rotation: dx.atan2(dz),
    };

    Some((cmd, travel_secs))
}
