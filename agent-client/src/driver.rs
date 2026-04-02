use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::{ClientMessage, ServerMessage};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::llm_scheduler::{LlmPriority, LlmScheduler};
use crate::orchestrator::ScheduleEntry;
use crate::state::SharedState;

/// Housing chunk size in world units (must match server's CHUNK_SIZE).
const HOUSING_CHUNK_SIZE: f32 = 64.0;

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
    /// Optional memory update: appended to the NPC's memory file for future sessions.
    pub memory_update: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum AgentAction {
    #[serde(rename = "say", alias = "chat")]
    Say { message: String },
    #[serde(rename = "attack")]
    Attack {
        #[serde(
            alias = "targetId",
            alias = "target_id",
            alias = "target",
            alias = "id"
        )]
        monster_id: String,
    },
    #[serde(rename = "move")]
    Move {
        // Absolute coordinates (preferred)
        x: Option<f32>,
        #[allow(dead_code)]
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
        } => Some(format!(
            "[Chat] {}: {message}",
            player_name(state, player_id)
        )),
        ServerMessage::PlayerJoined { player } => Some(format!("[PlayerJoined] {}", player.name)),
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
            position.x,
            position.y,
            position.z
        )),
        ServerMessage::MonsterSpawned { monster } => Some(format!(
            "[MonsterSpawned] {} ({})",
            monster.id, monster.monster_type
        )),
        ServerMessage::MonsterDead { monster_id } => Some(format!("[MonsterDead] {monster_id}")),
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
        ServerMessage::CharacterError { message } => Some(format!("[CharacterError] {message}")),
        ServerMessage::CharacterCreated { character } => Some(format!(
            "[CharacterCreated] id={} {} Lv.{} {:?}",
            character.id, character.name, character.level, character.class
        )),
        ServerMessage::CharacterStatsRolled { attributes, max_hp } => Some(format!(
            "[StatsRolled] STR:{} DEX:{} CON:{} INT:{} WIS:{} CHA:{} HP:{}",
            attributes.r#str,
            attributes.dex,
            attributes.con,
            attributes.int,
            attributes.wis,
            attributes.cha,
            max_hp
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
/// Resolve move goal coordinates from an AgentAction::Move.
fn resolve_move_goal(
    x: &Option<f32>,
    z: &Option<f32>,
    direction: &Option<String>,
    distance: &Option<f32>,
    player_pos: Option<&onlinerpg_shared::Position>,
) -> Option<(f32, f32)> {
    if let (Some(x), Some(z)) = (x, z) {
        Some((*x, *z))
    } else if let (Some(dir), Some(dist), Some(pp)) = (direction.as_deref(), distance, player_pos) {
        let (dx, dz) = direction_to_offset(dir);
        Some((pp.x + dx * dist, pp.z + dz * dist))
    } else {
        None
    }
}

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
            y: _,
            z,
            direction,
            distance,
        } => {
            let (gx, gz) = resolve_move_goal(x, z, direction, distance, player_pos)?;
            let rotation = if let Some(pp) = player_pos {
                (gx - pp.x).atan2(gz - pp.z)
            } else {
                0.0
            };
            Some(ClientMessage::PlayerMove {
                position: onlinerpg_shared::Position {
                    x: gx,
                    y: player_pos.map(|p| p.y).unwrap_or(0.0),
                    z: gz,
                },
                rotation,
                floor_level: 0,
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
pub fn build_prompt(
    state: &SharedState,
    events: &[ServerMessage],
    agent_events: &[String],
    schedule: &[ScheduleEntry],
    active_schedule_idx: Option<usize>,
) -> String {
    let mut prompt = String::new();

    prompt.push_str("=== CURRENT STATE ===\n");
    prompt.push_str(&state.format_world_state());
    prompt.push('\n');

    if let Some(ctx) = format_schedule_context(schedule, active_schedule_idx) {
        prompt.push_str(&ctx);
        prompt.push('\n');
    }

    let has_server_events = events.iter().any(|e| format_event(state, e).is_some());
    if has_server_events || !agent_events.is_empty() {
        prompt.push_str("\n=== EVENTS ===\n");
        for event in events {
            if let Some(line) = format_event(state, event) {
                prompt.push_str(&line);
                prompt.push('\n');
            }
        }
        for line in agent_events {
            prompt.push_str(line);
            prompt.push('\n');
        }
    }

    prompt.push_str("\nWhat do you do?");
    prompt
}

/// Configuration for the LLM driver loop.
pub struct DriverConfig {
    pub label: String,
    pub memory_file: Option<String>,
    pub min_interval: Duration,
    pub debounce: Duration,
    pub idle_interval: Duration,
    pub activity_window: Duration,
    pub schedule: Vec<ScheduleEntry>,
    /// HTTP base URL for the game server API (e.g. "http://127.0.0.1:10015").
    pub api_base_url: String,
}

/// Resolve which schedule entry is currently active based on game time.
/// Returns `(entry_index, game_hour)` — the hour component ensures recurring
/// entries re-trigger each hour even though the index stays the same.
/// Conditions are pre-validated at load time via `ScheduleEntry::parse_condition`.
fn resolve_active_schedule(
    schedule: &[ScheduleEntry],
    is_night: Option<bool>,
    game_hour: Option<u32>,
    game_minute: Option<u32>,
) -> (Option<usize>, Option<u32>) {
    use crate::orchestrator::ScheduleCondition;

    let mut best: Option<usize> = None;

    for (i, entry) in schedule.iter().enumerate() {
        let condition = match entry.condition.as_ref() {
            Some(c) => c,
            None => continue,
        };
        let matched = match condition {
            ScheduleCondition::Day => is_night == Some(false),
            ScheduleCondition::Night => is_night == Some(true),
            ScheduleCondition::Time {
                hour: eh,
                minute: em,
            } => match (game_hour, game_minute) {
                (Some(gh), Some(gm)) => gh * 60 + gm >= eh * 60 + em,
                _ => false,
            },
            ScheduleCondition::Recurring { minute: em } => match (game_hour, game_minute) {
                (Some(_), Some(gm)) => gm >= *em,
                _ => false,
            },
        };

        if matched {
            best = Some(i);
        }
    }

    let hour_for_recurring = best.map_or(None, |i| {
        if matches!(
            schedule[i].condition,
            Some(ScheduleCondition::Recurring { .. })
        ) {
            game_hour
        } else {
            None
        }
    });
    (best, hour_for_recurring)
}

/// Format current schedule context for inclusion in LLM prompts.
fn format_schedule_context(
    schedule: &[ScheduleEntry],
    active_idx: Option<usize>,
) -> Option<String> {
    let entry = &schedule[active_idx?];
    let mut line = format!(
        "Schedule: go to {} at ({:.1}, {:.1}, {:.1})",
        entry.display_label(),
        entry.pos[0],
        entry.pos[1],
        entry.pos[2]
    );
    if let Some(ref action) = entry.action {
        line.push_str(&format!(" — using {action} (DO NOT move, you are resting)"));
    }
    Some(line)
}

/// The main LLM agent driver loop. Runs as a tokio task.
///
/// Ticks every ATTACK_COOLDOWN to send attack packets when there's an active
/// target. LLM calls are submitted to the shared scheduler so they don't block
/// combat and respect the global concurrency limit.
pub async fn llm_driver(
    state: Arc<Mutex<SharedState>>,
    invoker: Arc<dyn LlmBackend>,
    scheduler: LlmScheduler,
    config: DriverConfig,
) {
    let DriverConfig {
        label,
        memory_file,
        min_interval,
        debounce,
        idle_interval,
        activity_window,
        schedule,
        api_base_url,
    } = config;
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

    info!("[{label}] LLM driver: in game, ready.");

    let attack_cooldown = load_attack_cooldown();

    // Stagger idle polls: random offset so NPCs don't all poll at the same time
    let idle_stagger = {
        use rand::Rng;
        let secs = idle_interval.as_secs().max(1);
        Duration::from_secs(rand::thread_rng().gen_range(0..secs))
    };
    let mut last_prompt_at = Instant::now() - idle_stagger;
    let mut attack_target: Option<String> = None;
    let mut last_attack_at = Instant::now() - attack_cooldown;
    let mut llm_in_flight: Option<tokio::task::JoinHandle<anyhow::Result<String>>> = None;
    let mut prompt_pending_since: Option<Instant> = None;
    // Track last chat/combat activity to decide polling interval
    let mut last_activity_at = Instant::now() - idle_interval;
    // Track the highest urgency since the last prompt
    let mut pending_urgency = LlmPriority::Idle;
    let mut active_schedule: (Option<usize>, Option<u32>) = (None, None);

    // Execute initial schedule move (go to correct position for current time)
    if !schedule.is_empty() {
        // Wait for first GameTimeSync to arrive (up to 10s)
        for _ in 0..20 {
            let has_time = { state.lock().await.is_night.is_some() };
            if has_time {
                break;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        // Fetch housing data so pathfinding avoids buildings
        let world_cache = { Arc::clone(&state.lock().await.world_cache) };
        fetch_houses_for_schedule(&world_cache, &schedule, &api_base_url, &label).await;

        active_schedule =
            check_schedule_transition(&state, &schedule, active_schedule, &label).await;
    }

    // Send initial world state via scheduler (blocking is fine here, no combat yet)
    let initial_prompt = {
        let mut s = state.lock().await;
        let agent_events = s.drain_agent_events();
        build_prompt(&*s, &[], &agent_events, &schedule, active_schedule.0)
    };
    info!("[{label}] LLM driver: sending initial world state");
    match scheduler
        .submit(
            &label,
            LlmPriority::Routine,
            initial_prompt,
            Arc::clone(&invoker),
        )
        .await
    {
        Ok(response) => {
            let skip_move = active_schedule
                .0
                .map_or(false, |i| schedule[i].action.is_some());
            attack_target = handle_response(&state, &response, &memory_file, skip_move).await;
            last_prompt_at = Instant::now();
        }
        Err(e) => {
            error!("[{label}] LLM initial prompt failed: {e}");
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
                debug!("[{label}] LLM driver: urgent event received");
                last_activity_at = Instant::now();
                pending_urgency = LlmPriority::Urgent;
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

        // === Check schedule transitions ===
        if !schedule.is_empty() && attack_target.is_none() {
            active_schedule =
                check_schedule_transition(&state, &schedule, active_schedule, &label).await;
        }

        // === Check if LLM response arrived ===
        if let Some(ref handle) = llm_in_flight {
            if handle.is_finished() {
                let handle = llm_in_flight.take().unwrap();
                match handle.await {
                    Ok(Ok(response)) => {
                        let skip = active_schedule
                            .0
                            .map_or(false, |i| schedule[i].action.is_some());
                        let new_target =
                            handle_response(&state, &response, &memory_file, skip).await;
                        if new_target.is_some() {
                            attack_target = new_target;
                        }
                        last_prompt_at = Instant::now();
                    }
                    Ok(Err(e)) => {
                        error!("[{label}] LLM prompt failed: {e}");
                        last_prompt_at = Instant::now();
                    }
                    Err(e) => {
                        error!("[{label}] LLM task panicked: {e}");
                        last_prompt_at = Instant::now();
                    }
                }
            }
        }

        // === Maybe start a new LLM prompt ===
        if llm_in_flight.is_some() {
            continue;
        }

        // Periodic prompt — use short interval only when recently active (chat/combat)
        let active = attack_target.is_some() || last_activity_at.elapsed() < activity_window;
        let effective_interval = if active { min_interval } else { idle_interval };
        if prompt_pending_since.is_none() && last_prompt_at.elapsed() >= effective_interval {
            prompt_pending_since = Some(Instant::now());
            if pending_urgency == LlmPriority::Idle && active {
                pending_urgency = LlmPriority::Routine;
            }
        }

        // Debounce: wait at least `debounce` after the trigger before actually prompting
        let ready_to_prompt = prompt_pending_since.is_some_and(|t| t.elapsed() >= debounce);

        if !ready_to_prompt {
            continue;
        }

        // Also ensure min_interval since last prompt (keep pending state so we retry next tick)
        if last_prompt_at.elapsed() < min_interval {
            continue;
        }
        prompt_pending_since = None;

        // Drain events and build prompt, determine priority from events
        let (prompt, has_events, priority) = {
            let mut s = state.lock().await;
            let events = s.drain_events();
            let agent_events = s.drain_agent_events();
            let has_events = !events.is_empty() || !agent_events.is_empty();

            // Determine priority from the most urgent event (lower = more urgent)
            let max_urgency = events
                .iter()
                .map(|e| LlmPriority::from(s.classify_event(e)))
                .fold(pending_urgency, std::cmp::min);

            let prompt = build_prompt(&*s, &events, &agent_events, &schedule, active_schedule.0);
            (prompt, has_events, max_urgency)
        };
        pending_urgency = LlmPriority::Idle; // reset for next cycle

        if !has_events {
            continue;
        }

        // Submit to scheduler as background task (doesn't block combat ticks)
        info!(
            "[{label}] LLM driver: submitting {:?} prompt ({} chars)",
            priority,
            prompt.len()
        );
        let sched = scheduler.clone();
        let inv = Arc::clone(&invoker);
        let lbl = label.clone();
        llm_in_flight = Some(tokio::spawn(async move {
            sched.submit(&lbl, priority, prompt, inv).await
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

/// Chase the monster until we're in attack range, using A* pathfinding.
/// Polls monster position every CHASE_TICK_MS and follows waypoints.
async fn chase_monster(state: &Arc<Mutex<SharedState>>, monster_id: &str) -> ChaseResult {
    let chase_start = Instant::now();
    let mut path_waypoints: Vec<onlinerpg_shared::pathfinding::PathWaypoint> = Vec::new();
    let mut path_index = 0usize;
    let mut last_monster_x = 0.0f32;
    let mut last_monster_z = 0.0f32;

    loop {
        if chase_start.elapsed().as_secs_f32() > MAX_CHASE_SECS {
            warn!("Chase timeout for monster {monster_id}");
            return ChaseResult::Lost;
        }

        let (in_range, needs_repath, monster_pos) = {
            let s = state.lock().await;
            let monster_alive = s.nearby_monsters.contains_key(monster_id);
            let player_alive = s.self_player.as_ref().is_some_and(|p| p.health > 0);
            if !monster_alive || !player_alive {
                return ChaseResult::Lost;
            }

            let monster = &s.nearby_monsters[monster_id];
            let player = s.self_player.as_ref().unwrap();
            let dx = monster.position.x - player.position.x;
            let dz = monster.position.z - player.position.z;
            let dist_sq = dx * dx + dz * dz;
            let in_range = dist_sq <= ATTACK_RANGE * ATTACK_RANGE;

            let monster_shift = ((monster.position.x - last_monster_x).powi(2)
                + (monster.position.z - last_monster_z).powi(2))
            .sqrt();
            let needs_repath = path_waypoints.is_empty()
                || path_index >= path_waypoints.len()
                || monster_shift > REROUTE_THRESHOLD;

            (in_range, needs_repath, monster.position.clone())
        };

        if in_range {
            return ChaseResult::InRange;
        }

        if needs_repath {
            let result = {
                let s = state.lock().await;
                s.find_path_to(monster_pos.x, monster_pos.z, 0)
            };
            if result.waypoints.is_empty() {
                // No path — fall back to direct move
                let cmd = {
                    let s = state.lock().await;
                    compute_move_to_monster(&s, monster_id).map(|(cmd, _)| cmd)
                };
                if let Some(cmd) = cmd {
                    let mut s = state.lock().await;
                    if let Err(e) = s.send_command(cmd).await {
                        error!("Failed to send chase move: {e}");
                        return ChaseResult::Error;
                    }
                }
            } else {
                path_waypoints = result.waypoints;
                path_index = 0;
            }
            last_monster_x = monster_pos.x;
            last_monster_z = monster_pos.z;
        }

        // Follow next waypoint
        if path_index < path_waypoints.len() {
            let wp = &path_waypoints[path_index];
            let cmd = {
                let s = state.lock().await;
                let player = match &s.self_player {
                    Some(p) => p,
                    None => return ChaseResult::Lost,
                };
                let dx = wp.x - player.position.x;
                let dz = wp.z - player.position.z;
                ClientMessage::PlayerMove {
                    position: onlinerpg_shared::Position {
                        x: wp.x,
                        y: player.position.y,
                        z: wp.z,
                    },
                    rotation: dx.atan2(dz),
                    floor_level: wp.floor as i8,
                }
            };
            {
                let mut s = state.lock().await;
                s.self_floor_level = wp.floor;
                if let Err(e) = s.send_command(cmd).await {
                    error!("Failed to send chase move: {e}");
                    return ChaseResult::Error;
                }
            }
            path_index += 1;
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

    if let Some(duration_secs) = data.get("combat_melee").and_then(|m| m.get("slash1")) {
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
/// If `memory_file` is set and the response contains `memory_update`, appends to file.
async fn handle_response(
    state: &Arc<Mutex<SharedState>>,
    response: &str,
    memory_file: &Option<String>,
    skip_movement: bool,
) -> Option<String> {
    let agent_resp = match parse_agent_response(response) {
        Ok(r) => r,
        Err(e) => {
            warn!("Failed to parse agent response: {e}");
            warn!("Raw response: {response}");
            return None;
        }
    };

    // Process memory update if present
    if let (Some(ref update), Some(ref path)) = (&agent_resp.memory_update, memory_file) {
        let update = update.trim();
        if !update.is_empty() {
            use std::io::Write;
            match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                Ok(mut f) => {
                    if let Err(e) = writeln!(f, "\n{update}") {
                        warn!("Failed to write memory update to {path}: {e}");
                    } else {
                        info!("Memory updated: {path} (+{} bytes)", update.len());
                    }
                }
                Err(e) => {
                    warn!("Failed to open memory file {path}: {e}");
                }
            }
        }
    }

    let mut last_attack_target = None;

    for action in &agent_resp.actions {
        // Skip movement/attack when resting on furniture (schedule action active)
        if skip_movement
            && matches!(
                action,
                AgentAction::Move { .. } | AgentAction::Attack { .. }
            )
        {
            debug!(
                "Skipping {:?} action — schedule furniture interaction active",
                action
            );
            continue;
        }

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

        // Handle move actions with pathfinding
        if let AgentAction::Move {
            x,
            y: _,
            z,
            direction,
            distance,
        } = action
        {
            let goal = {
                let s = state.lock().await;
                let pp = s.self_player.as_ref().map(|p| &p.position);
                resolve_move_goal(x, z, direction, distance, pp)
            };
            if let Some((gx, gz)) = goal {
                match execute_move(state, gx, gz, 0).await {
                    MoveResult::Arrived => {
                        info!("Agent arrived at ({gx:.1}, {gz:.1})");
                    }
                    MoveResult::Blocked => {
                        warn!("Path blocked to ({gx:.1}, {gz:.1})");
                        let mut s = state.lock().await;
                        s.push_agent_event(format!(
                            "[MoveFailed] 이동 실패: ({gx:.1}, {gz:.1})까지의 경로가 건물에 의해 막혀있습니다. 다른 목표를 선택하세요."
                        ));
                    }
                    MoveResult::Error => {
                        error!("Move error to ({gx:.1}, {gz:.1})");
                    }
                }
            }
            continue;
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
        floor_level: state.self_floor_level as i8,
    })
}

/// Move result for path-following
enum MoveResult {
    Arrived,
    Blocked,
    Error,
}

/// Maximum distance per move step (units). Longer segments are subdivided
/// so the NPC walks at MOVE_SPEED instead of teleporting.
const MAX_STEP_DIST: f32 = 3.0;
const SCHEDULE_ARRIVAL_RADIUS: f32 = 2.0;

/// Check if the active schedule entry changed and execute a move if needed.
/// Returns the new active schedule index.
async fn check_schedule_transition(
    state: &Arc<Mutex<SharedState>>,
    schedule: &[ScheduleEntry],
    current: (Option<usize>, Option<u32>),
    label: &str,
) -> (Option<usize>, Option<u32>) {
    let (is_night, game_hour, game_minute) = { state.lock().await.time_context() };
    let new = resolve_active_schedule(schedule, is_night, game_hour, game_minute);
    if new != current {
        // Stop interaction from previous schedule entry if it had an action
        if let Some(prev_i) = current.0 {
            if schedule[prev_i].action.is_some() {
                let mut s = state.lock().await;
                if let Err(e) = s.send_command(ClientMessage::StopInteraction).await {
                    error!("[{label}] Failed to send StopInteraction: {e}");
                }
            }
        }

        if let Some(i) = new.0 {
            let entry = &schedule[i];
            info!(
                "[{label}] Schedule transition: moving to {}",
                entry.display_label()
            );
            execute_schedule_move(state, entry).await;
        }
    }
    new
}

/// Walk to a schedule entry's position and set the final rotation.
/// If the entry has waypoints, visits each one in order before going to `pos`.
/// Send InteractFurniture if the schedule entry has an action and furniture_id.
async fn send_interact_if_needed(s: &mut SharedState, entry: &ScheduleEntry) {
    if let (Some(ref furniture_type), Some(furniture_id)) = (&entry.action, entry.furniture_id) {
        debug!("Sending InteractFurniture: {furniture_type} (id={furniture_id})");
        let cmd = ClientMessage::InteractFurniture {
            furniture_type: furniture_type.clone(),
            furniture_id,
        };
        if let Err(e) = s.send_command(cmd).await {
            error!("Failed to send InteractFurniture: {e}");
        }
    }
}

async fn execute_schedule_move(state: &Arc<Mutex<SharedState>>, entry: &ScheduleEntry) {
    // Walk through patrol waypoints first (if any)
    for (i, wp) in entry.waypoints.iter().enumerate() {
        let (wx, wz) = (wp[0], wp[2]);
        debug!(
            "Patrol waypoint {}/{}: ({:.1}, {:.1})",
            i + 1,
            entry.waypoints.len(),
            wx,
            wz
        );
        match execute_move(state, wx, wz, entry.floor_level).await {
            MoveResult::Arrived => {}
            MoveResult::Blocked => {
                warn!("Patrol waypoint {i} blocked — skipping ({wx:.1}, {wz:.1})");
            }
            MoveResult::Error => {
                error!("Patrol waypoint {i} error");
            }
        }
    }

    // Go to final position
    let (x, y, z) = (entry.pos[0], entry.pos[1], entry.pos[2]);

    // Check if we're already near the target (including floor level)
    {
        let mut s = state.lock().await;
        if let Some(ref p) = s.self_player {
            let dx = x - p.position.x;
            let dz = z - p.position.z;
            let same_floor = s.self_floor_level == entry.floor_level;
            if same_floor && (dx * dx + dz * dz).sqrt() < SCHEDULE_ARRIVAL_RADIUS {
                debug!("Already near schedule target — skipping movement");
                send_interact_if_needed(&mut s, entry).await;
                return;
            }
        }
    }

    let arrived = match execute_move(state, x, z, entry.floor_level).await {
        MoveResult::Arrived => true,
        MoveResult::Blocked => {
            // Force-move to schedule position (e.g. cross-floor moves through
            // closed doors). NPCs must follow their schedules.
            warn!(
                "Schedule move blocked — force-moving to ({x:.1}, {z:.1}) floor {}",
                entry.floor_level
            );
            true
        }
        MoveResult::Error => {
            error!("Schedule move error");
            false
        }
    };

    if arrived {
        // Send final position with exact rotation
        let rot_rad = entry.rotation.to_radians();
        let mut s = state.lock().await;
        s.self_floor_level = entry.floor_level;
        let cmd = ClientMessage::PlayerMove {
            position: onlinerpg_shared::Position { x, y, z },
            rotation: rot_rad,
            floor_level: entry.floor_level as i8,
        };
        if let Err(e) = s.send_command(cmd).await {
            error!("Failed to send schedule move: {e}");
        }

        send_interact_if_needed(&mut s, entry).await;
    }
}

/// Execute a move to the target position using A* pathfinding.
/// Follows waypoints sequentially with appropriate timing, subdividing
/// long segments so the NPC never teleports.
async fn execute_move(
    state: &Arc<Mutex<SharedState>>,
    goal_x: f32,
    goal_z: f32,
    goal_floor: u8,
) -> MoveResult {
    let path_result = {
        let s = state.lock().await;
        s.find_path_to(goal_x, goal_z, goal_floor)
    };

    if path_result.waypoints.is_empty() {
        if !path_result.found {
            return MoveResult::Blocked;
        }
        return MoveResult::Arrived;
    }

    for wp in &path_result.waypoints {
        // Subdivide long segments into small steps
        loop {
            let travel_ms = {
                let mut s = state.lock().await;
                let player = match &s.self_player {
                    Some(p) => p,
                    None => return MoveResult::Error,
                };

                let dx = wp.x - player.position.x;
                let dz = wp.z - player.position.z;
                let dist = (dx * dx + dz * dz).sqrt();
                if dist < 0.1 {
                    break;
                }

                let (step_x, step_z, step_dist) = if dist <= MAX_STEP_DIST {
                    (wp.x, wp.z, dist)
                } else {
                    let ratio = MAX_STEP_DIST / dist;
                    (
                        player.position.x + dx * ratio,
                        player.position.z + dz * ratio,
                        MAX_STEP_DIST,
                    )
                };

                let cmd = ClientMessage::PlayerMove {
                    position: onlinerpg_shared::Position {
                        x: step_x,
                        y: player.position.y,
                        z: step_z,
                    },
                    rotation: dx.atan2(dz),
                    floor_level: wp.floor as i8,
                };
                s.self_floor_level = wp.floor;
                if let Err(e) = s.send_command(cmd).await {
                    error!("Failed to send move waypoint: {e}");
                    return MoveResult::Error;
                }
                ((step_dist / MOVE_SPEED) * 1000.0) as u64
            };

            tokio::time::sleep(Duration::from_millis(travel_ms.max(50))).await;
        }
    }

    MoveResult::Arrived
}

/// Insert a position's chunk and its 8 neighbors into the set.
fn insert_chunk_neighbors(chunks: &mut HashSet<(i32, i32)>, x: f32, z: f32) {
    let cx = (x / HOUSING_CHUNK_SIZE).floor() as i32;
    let cz = (z / HOUSING_CHUNK_SIZE).floor() as i32;
    for dx in -1..=1i32 {
        for dz in -1..=1i32 {
            chunks.insert((cx + dx, cz + dz));
        }
    }
}

/// Fetch houses from the HTTP API for all chunks that the schedule positions
/// and waypoints pass through, so pathfinding can avoid buildings.
async fn fetch_houses_for_schedule(
    world_cache: &Arc<std::sync::RwLock<crate::state::WorldCache>>,
    schedule: &[ScheduleEntry],
    api_base_url: &str,
    label: &str,
) {
    let mut chunks = HashSet::new();
    for entry in schedule {
        insert_chunk_neighbors(&mut chunks, entry.pos[0], entry.pos[2]);
        for wp in &entry.waypoints {
            insert_chunk_neighbors(&mut chunks, wp[0], wp[2]);
        }
    }

    debug!(
        "[{label}] Fetching houses for {} chunk(s): {:?}",
        chunks.len(),
        chunks
    );
    let client = reqwest::Client::new();
    let fetches = chunks.iter().map(|&(cx, cz)| {
        let client = &client;
        let url = format!("{api_base_url}/api/housing/area/{cx}/{cz}");
        async move {
            match client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    resp.json::<Vec<HouseData>>().await.unwrap_or_default()
                }
                Ok(resp) => {
                    warn!(
                        "[{label}] Housing API returned {} for chunk ({cx},{cz})",
                        resp.status()
                    );
                    Vec::new()
                }
                Err(e) => {
                    warn!("[{label}] Failed to fetch houses for chunk ({cx},{cz}): {e}");
                    Vec::new()
                }
            }
        }
    });
    let results = futures_util::future::join_all(fetches).await;

    let all_houses: Vec<HouseData> = results.into_iter().flatten().collect();
    if all_houses.is_empty() {
        info!("[{label}] No houses found in any chunk");
    } else {
        let count = all_houses.len();
        let mut world = world_cache.write().unwrap();
        for house in all_houses {
            world.add_house(house);
        }
        info!("[{label}] Loaded {count} house(s) for pathfinding");
    }
}

/// If the agent is too far from the target monster, return a PlayerMove command
/// and the estimated travel time in seconds (based on client walk speed).
fn compute_move_to_monster(state: &SharedState, monster_id: &str) -> Option<(ClientMessage, f32)> {
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
        floor_level: 0,
    };

    Some((cmd, travel_secs))
}
