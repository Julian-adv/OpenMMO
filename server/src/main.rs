mod api_auth;
mod auth;
mod celestial;
mod connection;
mod dungeon_defs;
mod game;
mod game_state;
mod google_auth;
mod housing;
mod item_defs;
mod merchant_defs;
mod monster_defs;
mod npc_defs;
mod npc_schedule;
mod terrain;
mod types;
mod world_config;
mod world_drop_defs;

use auth::AuthService;
use clap::Parser;
use connection::{handle_connection, AuthContext};
use futures_util::FutureExt;
use game_state::GameState;
use google_auth::GoogleAuthVerifier;
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
use tracing::{error, info, warn};

/// Catches and logs a panic from one tick round so the loop task survives.
async fn guard_tick(name: &str, tick: impl std::future::Future<Output = ()>) {
    if std::panic::AssertUnwindSafe(tick)
        .catch_unwind()
        .await
        .is_err()
    {
        error!("{name} tick panicked; loop continues");
    }
}

async fn time_sync_tick(game_state: &GameState, auth_service: &Arc<AuthService>, tick_count: u64) {
    // Regenerate player health every 2 ticks (16 seconds)
    if tick_count.is_multiple_of(2) {
        game_state.tick_regeneration().await;
    }

    // Count down trade-window holds; releases an NPC ~32s (4 ticks)
    // after a customer opened its window, even if still open.
    game_state.tick_shop_holds().await;

    // Pay NPC trader salaries on game-day rollover (economy phase 3)
    game_state.tick_npc_salaries().await;

    // Batch-save dirty character states every 4 ticks (32 seconds)
    if tick_count.is_multiple_of(4) {
        let dirty_states = game_state.collect_dirty_character_states().await;
        if !dirty_states.is_empty() {
            let count = dirty_states.len();
            let auth = Arc::clone(auth_service);
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
        let dirty_inventories = game_state.collect_dirty_inventory_states().await;
        if !dirty_inventories.is_empty() {
            let count = dirty_inventories.len();
            let auth = Arc::clone(auth_service);
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

    let datetime = game_state.broadcast_game_time();
    if let Err(err) = auth_service.save_world_time(&datetime) {
        warn!("Failed to persist game time: {}", err);
    }
}

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

    /// Bind address for the REST API. Loopback by default: the vite proxy
    /// and local bots are the only intended callers.
    #[arg(long, default_value = "127.0.0.1")]
    api_bind: String,

    /// Comma-separated Google emails allowed to use REST write endpoints
    /// (map editor)
    #[arg(long, env = "ADMIN_EMAILS", default_value = "")]
    admin_emails: String,

    /// Directory for terrain data files
    #[arg(long, default_value = "./data/terrain")]
    terrain_dir: String,

    /// Google OAuth client ID used to verify browser sign-in tokens
    #[arg(long, env = "GOOGLE_CLIENT_ID")]
    google_client_id: Option<String>,

    /// Shared secret for headless NPC clients (default: data/npc_token,
    /// generated on first run)
    #[arg(long, env = "NPC_AUTH_TOKEN")]
    npc_token: Option<String>,
}

/// Read the NPC token file, generating a random one on first run so local
/// bots work with zero config (they read the same file).
fn load_or_create_npc_token() -> std::io::Result<String> {
    let path = std::path::Path::new(onlinerpg_shared::NPC_TOKEN_PATH_FROM_ROOT);
    if let Ok(existing) = std::fs::read_to_string(path) {
        let existing = existing.trim().to_string();
        if !existing.is_empty() {
            return Ok(existing);
        }
    }

    let token = uuid::Uuid::new_v4().simple().to_string();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &token)?;
    // Shared secret: keep it owner-only so other local users can't read it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    info!("Generated NPC auth token at {}", path.display());
    Ok(token)
}

/// Minimum length for an operator-supplied NPC token; the auto-generated one
/// is 32 hex chars.
const MIN_NPC_TOKEN_LEN: usize = 16;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    world_config::log_world_config();
    let monster_defs = monster_defs::MonsterDefs::load();
    let item_defs = item_defs::ItemDefs::load();
    let world_drop_defs = world_drop_defs::WorldDropDefs::load(&item_defs);
    let auth_service = match AuthService::new(AuthService::default_db_path()) {
        Ok(service) => Arc::new(service),
        Err(e) => {
            error!("Failed to initialize auth service: {}", e);
            return;
        }
    };

    let google = match &args.google_client_id {
        Some(client_id) => Some(GoogleAuthVerifier::new(client_id.clone())),
        None => {
            warn!("No --google-client-id / GOOGLE_CLIENT_ID set: browser sign-in disabled");
            None
        }
    };
    let npc_token = match args.npc_token.clone() {
        Some(token) => token,
        None => match load_or_create_npc_token() {
            Ok(token) => token,
            Err(e) => {
                error!("failed to load/create NPC token: {e}");
                return;
            }
        },
    };
    if npc_token.trim().len() < MIN_NPC_TOKEN_LEN {
        error!(
            "NPC token is shorter than {MIN_NPC_TOKEN_LEN} chars; refusing to start. \
             Unset --npc-token / NPC_AUTH_TOKEN to auto-generate a secure one."
        );
        return;
    }
    let admin_emails: Vec<String> = args
        .admin_emails
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if admin_emails.is_empty() {
        warn!("No --admin-emails / ADMIN_EMAILS set: REST writes require the NPC token");
    }
    let auth_ctx = Arc::new(AuthContext {
        google,
        npc_token,
        admin_emails,
    });
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

    // Load no-spawn zones (towns) from per-region zone files. Monster spawn
    // areas come from world.json `ambientSpawns`, not per-region rectangles.
    let no_spawn_zones = world_config::load_no_spawn_zones_from_regions(&terrain_io).await;

    let game_state = Arc::new(GameState::new(
        monster_defs,
        item_defs,
        world_drop_defs,
        initial_game_time,
        Arc::clone(&housing_io),
        no_spawn_zones,
        dungeon_defs::DungeonDefs::load(),
    ));
    // Monster spawn tick task
    let game_state_for_spawns = Arc::clone(&game_state);
    tokio::spawn(async move {
        // Every 10s, top up each player's ambient monsters toward their caps.
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;
            guard_tick("monster spawn", game_state_for_spawns.tick_monster_spawns()).await;
        }
    });

    // Dungeon spawn-slot refill tick (respawns on occupied floors)
    let game_state_for_dungeons = Arc::clone(&game_state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            guard_tick("dungeon refill", game_state_for_dungeons.tick_dungeons()).await;
        }
    });

    // Ground item despawn tick (every 30 seconds)
    let game_state_for_ground = Arc::clone(&game_state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(30));
        loop {
            interval.tick().await;
            guard_tick(
                "ground item despawn",
                game_state_for_ground.tick_ground_item_despawn(),
            )
            .await;
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
            guard_tick(
                "time sync",
                time_sync_tick(
                    &game_state_for_time_sync,
                    &auth_service_for_time_sync,
                    tick_count,
                ),
            )
            .await;
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

    // Start terrain REST API server. No CORS layer on purpose: browsers only
    // reach this API same-origin through the vite proxy.
    let terrain_port = args.terrain_port.unwrap_or(args.port + 1);
    let terrain_app = terrain_router(Arc::clone(&terrain_io))
        .merge(housing_router(
            Arc::clone(&housing_io),
            terrain_io,
            Arc::clone(&game_state),
        ))
        .merge(npc_router(npc_io))
        .layer(axum::middleware::from_fn_with_state(
            Arc::clone(&auth_ctx),
            api_auth::require_admin_for_writes,
        ))
        .layer(CompressionLayer::new());
    let terrain_addr = format!("{}:{}", args.api_bind, terrain_port);
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
                let auth_ctx_clone = Arc::clone(&auth_ctx);

                tokio::spawn(async move {
                    handle_connection(stream, game_state_clone, auth_service_clone, auth_ctx_clone)
                        .await;
                });
            }
            Err(e) => {
                error!("Failed to accept connection: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn guard_tick_swallows_panic() {
        super::guard_tick("test", async { panic!("boom") }).await;
    }
}
