use std::sync::Arc;

use onlinerpg_shared::ClientMessage;
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

use axum::http::HeaderName;
use tower_http::cors::{Any, CorsLayer};

use crate::state::SharedState;

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
pub struct CreateCharacterParams {
    #[schemars(description = "The character name")]
    pub character_name: String,
    #[schemars(description = "Character class: warrior, knight, thief")]
    pub character_class: String,
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
            return "No characters found. Use create_character to make one.".to_string();
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

    #[tool(description = "Create a new character on this account")]
    async fn create_character(
        &self,
        Parameters(params): Parameters<CreateCharacterParams>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let class: onlinerpg_shared::CharacterClass =
            match serde_json::from_value(serde_json::Value::String(params.character_class.clone()))
            {
                Ok(c) => c,
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "Unknown class '{}'. Valid: warrior, knight, thief",
                        params.character_class
                    ))]));
                }
            };

        // Step 1: Roll stats first (server requires this before CreateCharacter)
        let mut state = self.state.lock().await;
        if let Err(e) = state.send_command(ClientMessage::RollCharacterStats).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to send RollCharacterStats: {e}"
            ))]));
        }

        // Step 2: Send CreateCharacter
        let msg = ClientMessage::CreateCharacter {
            character_name: params.character_name.clone(),
            character_class: class,
        };
        if let Err(e) = state.send_command(msg).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "Failed to send CreateCharacter: {e}"
            ))]));
        }

        Ok(CallToolResult::success(vec![Content::text(format!(
            "Creating character '{}' as {}. Use get_events to see the result, then list_characters.",
            params.character_name, params.character_class
        ))]))
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
        let char_name = match state
            .characters
            .iter()
            .find(|c| c.id == params.character_id)
        {
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
        let agent_events = state.drain_agent_events();
        if events.is_empty() && agent_events.is_empty() {
            return "No new events.".to_string();
        }
        let mut lines: Vec<String> = events
            .iter()
            .filter_map(|e| crate::driver::format_event(&state, e))
            .collect();
        lines.extend(agent_events);
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
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "OnlineRPG agent client. Use list_characters to see available characters, \
                 enter_game to join the world, then get_events to observe what happens.",
        )
    }
}

// --- Event formatting ---

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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers([HeaderName::from_static("mcp-session-id")]);

    let router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(cors);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    info!("MCP HTTP server listening on http://0.0.0.0:{port}/mcp");

    axum::serve(listener, router)
        .with_graceful_shutdown(async move { ct.cancelled_owned().await })
        .await?;

    Ok(())
}
