mod auth;
mod connection;
mod game;
mod game_state;
mod monster_defs;
mod types;

use auth::AuthService;
use clap::Parser;
use connection::handle_connection;
use game_state::GameState;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(name = "onlinerpg-server")]
#[command(about = "MMORPG Game Server", long_about = None)]
struct Args {
    /// Port number to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let monster_defs = monster_defs::MonsterDefs::load();
    let game_state = Arc::new(GameState::new(monster_defs));
    let auth_service = match AuthService::new(AuthService::default_db_path()) {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to initialize auth service: {}", e);
            return;
        }
    };

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = match TcpListener::bind(addr.as_str()).await {
        Ok(listener) => {
            info!("MMORPG Server listening on: {}", addr);
            listener
        }
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            return;
        }
    };

    info!("🎮 MMORPG Server started successfully!");
    info!("📡 WebSocket server ready for connections");
    info!("🌐 Connect clients to: ws://{}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("New connection from: {}", addr);
                let game_state_clone = Arc::clone(&game_state);
                let auth_service_clone = Arc::clone(&auth_service);

                tokio::spawn(async move {
                    handle_connection(stream, game_state_clone, auth_service_clone).await;
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}
