//! Decode an LLM response and run each action against the game server.
//! Returns the monster_id of the last attack action so `llm_driver` can
//! enter its combat loop. Also persists `memory_update` snippets to the
//! NPC's per-instance memory file when configured.

use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use crate::state::SharedState;

use super::action::{action_to_command, parse_agent_response, resolve_move_goal, AgentAction};
use super::combat::{chase_monster, ChaseResult};
use super::movement::{execute_move, MoveResult};

/// Parse and execute the agent's response.
/// Returns the monster_id if the last action was an attack (for combat loop).
/// If `memory_file` is set and the response contains `memory_update`, appends to file.
pub(super) async fn handle_response(
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
        // Skip movement/attack when resting on object (schedule action active)
        if skip_movement
            && matches!(
                action,
                AgentAction::Move { .. } | AgentAction::Attack { .. }
            )
        {
            debug!(
                "Skipping {:?} action — schedule object interaction active",
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
                    if let Some(face_cmd) = s.face_monster_command(monster_id) {
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
