use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{schemars, tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;

use crate::game::Game;

pub struct Server {
    game: Game,
}

impl Server {
    pub fn new(game: Game) -> Self {
        Self { game }
    }
}

fn reply(result: anyhow::Result<String>) -> CallToolResult {
    match result {
        Ok(text) => CallToolResult::success(vec![ContentBlock::text(text)]),
        Err(error) => CallToolResult::error(vec![ContentBlock::text(format!("{error:#}"))]),
    }
}

#[derive(Deserialize, schemars::JsonSchema)]
struct EnterParams {
    /// Character name; the account's first character when omitted.
    character: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct WalkParams {
    x: f32,
    z: f32,
    /// Storey index inside a building; 0 is ground level.
    floor: Option<u8>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct LookParams {
    /// Repeat everything, including lines that have not changed.
    full: Option<bool>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct AttackParams {
    /// The id in square brackets on a `Monster:` line from look.
    monster_id: String,
    /// Stop when health falls below this share of the maximum. Defaults to 30.
    retreat_hp_percent: Option<u32>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ActParams {
    /// Action objects such as {"type": "enter_dungeon"}, run in order.
    actions: Vec<rmcp::serde_json::Value>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct ActionsParams {
    /// A word to search the action documentation for, such as "dungeon" or "eat".
    about: Option<String>,
}

#[derive(Deserialize, schemars::JsonSchema)]
struct SayParams {
    message: String,
}

#[tool_router]
impl Server {
    #[tool(
        description = "Sign in to OpenMMO with the user's Google account and list their characters. If it answers with a URL and a code, show both to the user and call again once they have signed in."
    )]
    async fn login(&self) -> CallToolResult {
        reply(self.game.characters().await)
    }

    #[tool(
        description = "Enter the game world as one of the account's characters. Returns what the character sees."
    )]
    async fn enter(&self, Parameters(p): Parameters<EnterParams>) -> CallToolResult {
        reply(self.game.enter(p.character.as_deref()).await)
    }

    #[tool(
        description = "Describe the character's current state and surroundings, and what happened since the last look. Lines that have not changed are left out unless full is set."
    )]
    async fn look(&self, Parameters(p): Parameters<LookParams>) -> CallToolResult {
        reply(self.game.look(p.full.unwrap_or(false)).await)
    }

    #[tool(
        description = "Chase and fight one monster. Returns when it is gone, when health falls below the retreat threshold, or after about 20 seconds; call again to keep fighting."
    )]
    async fn attack(&self, Parameters(p): Parameters<AttackParams>) -> CallToolResult {
        reply(self.game.attack(&p.monster_id, p.retreat_hp_percent).await)
    }

    #[tool(
        description = "Walk to world coordinates, routing around obstacles and through doors. Returns once the character arrives or gives up."
    )]
    async fn walk_to(&self, Parameters(p): Parameters<WalkParams>) -> CallToolResult {
        reply(self.game.walk_to(p.x, p.z, p.floor.unwrap_or(0)).await)
    }

    #[tool(
        description = "Run any other game action: loot, eat, equip, enter a dungeon, trade and so on. Takes the action objects that the actions tool documents and returns what happened."
    )]
    async fn act(&self, Parameters(p): Parameters<ActParams>) -> CallToolResult {
        reply(self.game.act(&p.actions).await)
    }

    #[tool(
        description = "List the actions act accepts, one line each. Pass about to read the full usage of the ones mentioning a word."
    )]
    async fn actions(&self, Parameters(p): Parameters<ActionsParams>) -> CallToolResult {
        reply(Ok(Game::actions(p.about.as_deref())))
    }

    #[tool(description = "Say something in local chat.")]
    async fn say(&self, Parameters(p): Parameters<SayParams>) -> CallToolResult {
        reply(self.game.say(p.message).await)
    }

    #[tool(description = "Leave the game world.")]
    async fn leave(&self) -> CallToolResult {
        reply(Ok(self.game.leave().await))
    }
}

#[tool_handler(
    name = "openmmo",
    instructions = "Play OpenMMO as the user's own character. Call login first, then enter. Coordinates are world metres on the x and z axes."
)]
impl ServerHandler for Server {}
