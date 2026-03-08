mod auth;
mod celestial;
mod connection;
mod game;
mod game_state;
mod monster_defs;
mod terrain;
mod types;
mod world_config;

use auth::AuthService;
use clap::Parser;
use connection::handle_connection;
use game_state::GameState;
use std::sync::Arc;
use terrain::io::TerrainIO;
use terrain::routes::terrain_router;
use tokio::net::TcpListener;
use tokio::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(name = "onlinerpg-server")]
#[command(about = "MMORPG Game Server", long_about = None)]
struct Args {
    /// Port number to listen on
    #[arg(short, long, default_value_t = 8080)]
    port: u16,

    /// Port for terrain REST API (default: game port + 1)
    #[arg(long)]
    terrain_port: Option<u16>,

    /// Directory for terrain data files
    #[arg(long, default_value = "./data/terrain")]
    terrain_dir: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    world_config::log_world_config();
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

    let game_state = Arc::new(GameState::new(
        monster_defs,
        initial_game_time,
        Arc::clone(&auth_service),
    ));
    let game_state_for_time_sync = Arc::clone(&game_state);
    let auth_service_for_time_sync = Arc::clone(&auth_service);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(8));
        let mut tick_count = 0u64;
        loop {
            interval.tick().await;
            tick_count = tick_count.wrapping_add(1);

            // Regenerate player health every 2 ticks (16 seconds)
            if tick_count % 2 == 0 {
                game_state_for_time_sync.tick_regeneration().await;
            }

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

    // Start terrain REST API server
    let terrain_port = args.terrain_port.unwrap_or(args.port + 1);
    let terrain_io = Arc::new(TerrainIO::new(std::path::PathBuf::from(&args.terrain_dir)));
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let terrain_app = terrain_router(terrain_io)
        .layer(cors)
        .layer(CompressionLayer::new());
    let terrain_addr = format!("0.0.0.0:{}", terrain_port);
    match TcpListener::bind(&terrain_addr).await {
        Ok(terrain_listener) => {
            info!("Terrain REST API listening on: {}", terrain_addr);
            tokio::spawn(async move {
                if let Err(e) = axum::serve(terrain_listener, terrain_app).await {
                    error!("Terrain API server error: {}", e);
                }
            });
        }
        Err(e) => {
            error!("Failed to bind terrain API to {}: {}", terrain_addr, e);
            return;
        }
    }

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
