mod auth;
mod celestial;
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
use tokio::time::Duration;
use tracing::{error, info, warn};
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
    let auth_service = match AuthService::new(AuthService::default_db_path()) {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to initialize auth service: {}", e);
            return;
        }
    };
    let initial_game_time = match auth_service.load_world_time() {
        Ok(Some(saved)) => {
            info!(
                "Loaded game time from DB: {:04}-{:02}-{:02} {:02}:{:02}",
                saved.year, saved.month, saved.day, saved.hour, saved.minute
            );
            saved
        }
        Ok(None) => {
            let initial = GameState::default_start_datetime();
            if let Err(err) = auth_service.save_world_time(&initial) {
                warn!("Failed to persist initial game time: {}", err);
            }
            info!(
                "Initialized game time: {:04}-{:02}-{:02} {:02}:{:02}",
                initial.year, initial.month, initial.day, initial.hour, initial.minute
            );
            initial
        }
        Err(err) => {
            warn!("Failed to load game time from DB, using default: {}", err);
            GameState::default_start_datetime()
        }
    };

    let game_state = Arc::new(GameState::new(monster_defs, initial_game_time, Arc::clone(&auth_service)));
    let game_state_for_time_sync = Arc::clone(&game_state);
    let auth_service_for_time_sync = Arc::clone(&auth_service);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(8));
        loop {
            interval.tick().await;
            let datetime = game_state_for_time_sync.broadcast_game_time();
            if let Err(err) = auth_service_for_time_sync.save_world_time(&datetime) {
                warn!("Failed to persist game time: {}", err);
            }
        }
    });

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
