mod game;
mod server;
mod update;

use rmcp::ServiceExt;

const DEFAULT_SERVER: &str = "wss://openmmo.to.nexus/ws";
const USAGE: &str = "usage: openmmo-mcp [probe [character-name]]
  no arguments  serve MCP on stdio
  probe         enter the game once, print what the character sees, and exit
environment: OPENMMO_SERVER (ws url), OPENMMO_TERRAIN (terrain directory or origin; defaults
  to the server's own),
  OPENMMO_GOOGLE_CACHE, and OPENMMO_ACCOUNT + OPENMMO_NPC_TOKEN for a dev server";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let server = std::env::var("OPENMMO_SERVER").unwrap_or_else(|_| DEFAULT_SERVER.into());
    let terrain = std::env::var("OPENMMO_TERRAIN")
        .unwrap_or_else(|_| agent_client::ws::api_base_url(&server));
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        None => {
            if let Some(code) = update::hand_over() {
                std::process::exit(code);
            }
            server::Server::new(game::Game::new(server, &terrain))
                .serve(rmcp::transport::stdio())
                .await?
                .waiting()
                .await?;
        }
        Some("probe") => {
            let game = game::Game::new(server, &terrain);
            println!("{}", game.enter(args.get(1).map(String::as_str)).await?);
        }
        Some(_) => anyhow::bail!("{USAGE}\nprotocol {}", onlinerpg_shared::PROTOCOL_VERSION),
    }
    Ok(())
}
