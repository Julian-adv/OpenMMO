mod auth;
mod celestial;
mod connection;
mod game;
mod game_state;
mod housing;
mod item_defs;
mod monster_defs;
mod npc_schedule;
mod terrain;
mod types;
mod world_config;

use auth::AuthService;
use clap::Parser;
use connection::handle_connection;
use game_state::GameState;
use housing::routes::housing_router;
use housing::HousingIO;
use npc_schedule::routes::npc_router;
use npc_schedule::NpcIO;
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
    #[arg(short, long, default_value_t = 10006)]
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
    let item_defs = item_defs::ItemDefs::load();
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

    let housing_io = Arc::new(HousingIO::new(std::path::PathBuf::from("./data/housing")));
    let npc_io = Arc::new(NpcIO::new(std::path::PathBuf::from(
        "./agent-client/data/npcs",
    )));
    let terrain_io = Arc::new(TerrainIO::new(std::path::PathBuf::from(&args.terrain_dir)));

    // Load spawn rules and no-spawn zones from per-region zone files
    let (spawn_rules, no_spawn_zones) =
        world_config::load_spawn_config_from_regions(&terrain_io).await;

    let game_state = Arc::new(GameState::new(
        monster_defs,
        item_defs,
        initial_game_time,
        Arc::clone(&housing_io),
        spawn_rules,
        no_spawn_zones,
    ));
    // Monster spawn tick task
    let game_state_for_spawns = Arc::clone(&game_state);
    tokio::spawn(async move {
        // Use a 10-second base interval; tick_monster_spawns checks per-rule timing
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            game_state_for_spawns.tick_monster_spawns().await;
        }
    });

    // Ground item despawn tick (every 30 seconds)
    let game_state_for_ground = Arc::clone(&game_state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            game_state_for_ground.tick_ground_item_despawn().await;
        }
    });

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

            // Batch-save dirty character states every 4 ticks (32 seconds)
            if tick_count % 4 == 0 {
                let dirty_states = game_state_for_time_sync
                    .collect_dirty_character_states()
                    .await;
                if !dirty_states.is_empty() {
                    let count = dirty_states.len();
                    let auth = Arc::clone(&auth_service_for_time_sync);
                    let handle = tokio::task::spawn_blocking(move || {
                        if let Err(e) = auth.save_characters_batch(&dirty_states) {
                            warn!("Failed to batch-save character states: {}", e);
                        } else {
                            info!("Batch-saved {} character state(s)", count);
                        }
                    });
                    tokio::spawn(async move {
                        if let Err(e) = handle.await {
                            error!("Batch save task panicked: {}", e);
                        }
                    });
                }

                // Batch-save dirty inventories
                let dirty_inventories = game_state_for_time_sync
                    .collect_dirty_inventory_states()
                    .await;
                if !dirty_inventories.is_empty() {
                    let count = dirty_inventories.len();
                    let auth = Arc::clone(&auth_service_for_time_sync);
                    let handle = tokio::task::spawn_blocking(move || {
                        for (char_id, items) in &dirty_inventories {
                            if let Err(e) = auth.save_inventory(*char_id, items) {
                                warn!("Failed to save inventory for character {}: {}", char_id, e);
                            }
                        }
                        info!("Batch-saved {} inventory/inventories", count);
                    });
                    tokio::spawn(async move {
                        if let Err(e) = handle.await {
                            error!("Inventory batch save task panicked: {}", e);
                        }
                    });
                }
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
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    let terrain_app = terrain_router(terrain_io)
        .merge(housing_router(housing_io))
        .merge(npc_router(npc_io))
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
