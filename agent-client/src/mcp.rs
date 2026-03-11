use std::sync::Arc;

use onlinerpg_shared::{ClientMessage, ServerMessage};
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServerHandler,
};
use serde::Deserialize;
use tokio::sync::Mutex;
use tracing::info;

use crate::SharedState;

#[derive(Clone)]
pub struct AgentMcpServer {
    tool_router: ToolRouter<Self>,
    state: Arc<Mutex<SharedState>>,
}

impl AgentMcpServer {
    pub fn new(state: Arc<Mutex<SharedState>>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            state,
        }
    }
}

// --- Tool parameter types ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EnterGameParams {
    #[schemars(description = "The character ID to enter the game with")]
    pub character_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SayParams {
    #[schemars(description = "The message to say in chat")]
    pub message: String,
}

// --- Tool implementations ---

#[tool_router]
impl AgentMcpServer {
    #[tool(description = "List available characters on this account")]
    async fn list_characters(&self) -> String {
        let state = self.state.lock().await;
        let chars = &state.characters;
        if chars.is_empty() {
            return "No characters found. Create one in the game client first.".to_string();
        }
        let mut lines = Vec::new();
        for c in chars {
            lines.push(format!(
                "[id={}] {} — Lv.{} {:?} (STR:{} DEX:{} CON:{} INT:{} WIS:{} CHA:{})",
                c.id,
                c.name,
                c.level,
                c.class,
                c.attributes.r#str,
                c.attributes.dex,
                c.attributes.con,
                c.attributes.int,
                c.attributes.wis,
                c.attributes.cha,
            ));
        }
        lines.join("\n")
    }

    #[tool(description = "Enter the game world with a specific character")]
    async fn enter_game(
        &self,
        Parameters(params): Parameters<EnterGameParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut state = self.state.lock().await;

        if state.in_game {
            return Ok(CallToolResult::success(vec![Content::text(
                "Already in the game.",
            )]));
        }

        // Validate character_id
        let char_name = match state.characters.iter().find(|c| c.id == params.character_id) {
            Some(c) => c.name.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "Character with id {} not found. Use list_characters to see available characters.",
                    params.character_id
                ))]));
            }
        };

        // Send EnterGame
        let msg = ClientMessage::EnterGame {
            character_id: params.character_id,
        };
        if let Err(e) = state.send_command(msg).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to send EnterGame: {e}"
            ))]));
        }

        info!(
            "Entering game with character {} (id={})",
            char_name, params.character_id
        );

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Entering game as {} (id={}). Use get_events to see what happens.",
            char_name, params.character_id
        ))]))
    }

    #[tool(description = "Get recent game events since last check")]
    async fn get_events(&self) -> String {
        let mut state = self.state.lock().await;
        let events = state.drain_events();
        if events.is_empty() {
            return "No new events.".to_string();
        }
        let lines: Vec<String> = events.iter().map(format_event).collect();
        lines.join("\n")
    }

    #[tool(description = "Send a chat message in the game")]
    async fn say(
        &self,
        Parameters(params): Parameters<SayParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let mut state = self.state.lock().await;
        if !state.in_game {
            return Ok(CallToolResult::error(vec![Content::text(
                "Not in game yet. Use enter_game first.",
            )]));
        }
        let msg = ClientMessage::ChatMessage {
            message: params.message.clone(),
        };
        if let Err(e) = state.send_command(msg).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to send chat: {e}"
            ))]));
        }
        Ok(CallToolResult::success(vec![Content::text(format!(
            "Said: {}",
            params.message
        ))]))
    }
}

// --- ServerHandler ---

#[tool_handler]
impl ServerHandler for AgentMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(
                "OnlineRPG agent client. Use list_characters to see available characters, \
                 enter_game to join the world, then get_events to observe what happens.",
            )
    }
}

// --- Event formatting ---

fn format_event(msg: &ServerMessage) -> String {
    match msg {
        ServerMessage::JoinSuccess { player } => {
            format!(
                "[Join] You joined as {} at ({:.1}, {:.1}, {:.1})",
                player.name, player.position.x, player.position.y, player.position.z
            )
        }
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
                    m.monster_type, m.state, m.health, m.max_health
                ));
            }
            lines.join("\n")
        }
        ServerMessage::GameTimeSync { datetime, is_night } => {
            format!(
                "[Time] Y{} M{} D{} {:02}:{:02} ({})",
                datetime.year,
                datetime.month,
                datetime.day,
                datetime.hour,
                datetime.minute,
                if *is_night { "night" } else { "day" }
            )
        }
        ServerMessage::ChatMessage {
            player_id, message, ..
        } => {
            format!("[Chat] {player_id}: {message}")
        }
        ServerMessage::PlayerJoined { player } => {
            format!("[PlayerJoined] {}", player.name)
        }
        ServerMessage::PlayerLeft { player_id } => {
            format!("[PlayerLeft] {player_id}")
        }
        ServerMessage::PlayerMoved {
            player_id,
            position,
            ..
        } => {
            format!(
                "[Move] Player {player_id} -> ({:.1}, {:.1}, {:.1})",
                position.x, position.y, position.z
            )
        }
        ServerMessage::MonsterSpawned { monster } => {
            format!(
                "[MonsterSpawned] {} ({})",
                monster.id, monster.monster_type
            )
        }
        ServerMessage::MonsterDead { monster_id } => {
            format!("[MonsterDead] {monster_id}")
        }
        ServerMessage::PlayerAttacked {
            player_id,
            monster_id,
            hit,
            damage,
            ..
        } => {
            format!("[Attack] Player {player_id} -> {monster_id}: hit={hit} dmg={damage}")
        }
        ServerMessage::MonsterAttackedPlayer {
            monster_id,
            player_id,
            hit,
            damage,
            current_health,
            ..
        } => {
            format!(
                "[MonsterAttack] {monster_id} -> {player_id}: hit={hit} dmg={damage} hp={current_health}"
            )
        }
        ServerMessage::PlayerDead { player_id } => {
            format!("[PlayerDead] {player_id}")
        }
        ServerMessage::PlayerRespawned { player } => {
            format!(
                "[Respawn] {} HP {}/{}",
                player.name, player.health, player.max_health
            )
        }
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
            s
        }
        ServerMessage::Kicked { reason, .. } => {
            format!("[Kicked] {reason}")
        }
        _ => format!("[Event] {:?}", std::mem::discriminant(msg)),
    }
}

/// Start the MCP server as an HTTP (Streamable HTTP) endpoint.
pub async fn run_mcp_server(state: Arc<Mutex<SharedState>>, port: u16) -> anyhow::Result<()> {
    let config = StreamableHttpServerConfig::default();
    let ct = config.cancellation_token.clone();

    let service: StreamableHttpService<AgentMcpServer, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(AgentMcpServer::new(state.clone())),
            Default::default(),
            config,
        );

    let router = axum::Router::new().nest_service("/mcp", service);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    info!("MCP HTTP server listening on http://0.0.0.0:{port}/mcp");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move { ct.cancelled_owned().await })
        .await?;

    Ok(())
}
