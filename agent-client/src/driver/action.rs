//! Agent action model and conversion to game-server commands.
//!
//! Splits responsibility into three layers: the JSON-shaped `AgentResponse`
//! the LLM is expected to emit, parsing helpers that tolerate the various
//! markdown wrappers an LLM might add, and `action_to_command` which lifts
//! a parsed `AgentAction` into a `ClientMessage` for the server.

use onlinerpg_shared::ClientMessage;
use serde::Deserialize;
use tracing::warn;

/// Parsed agent response.
#[derive(Debug, Deserialize)]
pub(super) struct AgentResponse {
    #[allow(dead_code)]
    pub thought: Option<String>,
    pub actions: Vec<AgentAction>,
    /// Optional memory update: appended to the NPC's memory file for future sessions.
    pub memory_update: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub(super) enum AgentAction {
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
        // Character name: approach them and stop a polite distance short
        // (preferred when walking up to a player or NPC)
        #[serde(alias = "player", alias = "name", alias = "character")]
        target: Option<String>,
        // Absolute coordinates (preferred for places)
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
    /// Haggling (merchants only): offer a price modifier on one item to a
    /// nearby player. The server clamps/validates; see `doc/ECONOMY.md`.
    #[serde(rename = "offer_deal")]
    OfferDeal {
        #[serde(alias = "target", alias = "player_name", alias = "target_player")]
        player: String,
        #[serde(alias = "item_def_id", alias = "item_id")]
        item: String,
        /// "buy" (player buys from you, default) or "sell" (player sells to you).
        #[serde(default)]
        kind: Option<String>,
        #[serde(
            alias = "modifier",
            alias = "modifier_percent",
            alias = "discount_pct"
        )]
        modifier_pct: i32,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Open your trade window on a nearby player's screen (traders only) —
    /// the conversational entry point for trading.
    #[serde(rename = "open_trade", alias = "trade")]
    OpenTrade {
        #[serde(alias = "target", alias = "player_name", alias = "target_player")]
        player: String,
    },
    #[serde(rename = "wait", alias = "idle", alias = "observe", alias = "none")]
    Wait,
}

/// Parse a raw text response from an LLM into structured actions.
pub(super) fn parse_agent_response(text: &str) -> anyhow::Result<AgentResponse> {
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

/// Resolve move goal coordinates from an AgentAction::Move. Supports both
/// absolute `(x, z)` and the `direction + distance` fallback some LLMs
/// prefer; the latter requires a known player position.
pub(super) fn resolve_move_goal(
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

/// Convert an AgentAction into a ClientMessage for the game server.
/// `player_pos` is needed to resolve relative move directions and to compute rotation.
pub(super) fn action_to_command(
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
            target,
            x,
            y: _,
            z,
            direction,
            distance,
        } => {
            // Name-targeted moves need SharedState to resolve the name and
            // run the approach loop; handled in `execute::handle_response`.
            if target.is_some() {
                return None;
            }
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
        // Need player-name → id resolution from SharedState; handled in
        // `execute::handle_response`, not here.
        AgentAction::OfferDeal { .. } => None,
        AgentAction::OpenTrade { .. } => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_single_action(json: &str) -> AgentAction {
        let resp = parse_agent_response(json).unwrap();
        resp.actions.into_iter().next().unwrap()
    }

    #[test]
    fn move_parses_character_target() {
        let action = parse_single_action(
            r#"{"actions": [{"type": "move", "target": "Karl"}]}"#,
        );
        let AgentAction::Move { target, .. } = action else {
            panic!("expected Move");
        };
        assert_eq!(target.as_deref(), Some("Karl"));
    }

    #[test]
    fn move_target_accepts_player_alias() {
        let action = parse_single_action(
            r#"{"actions": [{"type": "move", "player": "Karl"}]}"#,
        );
        let AgentAction::Move { target, .. } = action else {
            panic!("expected Move");
        };
        assert_eq!(target.as_deref(), Some("Karl"));
    }

    #[test]
    fn move_still_parses_coordinates() {
        let action = parse_single_action(
            r#"{"actions": [{"type": "move", "x": 10.0, "y": 0.0, "z": -5.0}]}"#,
        );
        let AgentAction::Move { target, x, z, .. } = action else {
            panic!("expected Move");
        };
        assert_eq!(target, None);
        assert_eq!(x, Some(10.0));
        assert_eq!(z, Some(-5.0));
    }
}
