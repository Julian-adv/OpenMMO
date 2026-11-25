//! One game connection without a driver.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use onlinerpg_shared::{Character, ClientMessage, ServerMessage};
use onlinerpg_terrain::height::HeightSampler;
use onlinerpg_terrain::io::TerrainIO;
use tokio::sync::{mpsc, Mutex};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::google_auth::GoogleAuth;
use crate::state::{SharedState, WorldCache};
use crate::ws;

pub struct World {
    pub terrain_snapshots: Arc<crate::terrain_snapshots::TerrainSnapshots>,
    pub height_sampler: Arc<HeightSampler>,
    pub splat_sampler: Arc<crate::splat::SplatSampler>,
    pub world_cache: Arc<std::sync::RwLock<WorldCache>>,
}

impl World {
    /// `terrain` is a local terrain directory or an HTTP origin.
    pub fn open(terrain: &str, cache_dir: &str) -> Self {
        let height_sampler = Arc::new(create_height_sampler(terrain, cache_dir));
        let sweeper = Arc::clone(&height_sampler);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(onlinerpg_terrain::TILE_CACHE_SWEEP_PERIOD).await;
                sweeper.sweep_stale_tiles().await;
            }
        });
        let mut cache = WorldCache::new();
        cache.register_dungeons();
        Self {
            terrain_snapshots: Arc::new(crate::terrain_snapshots::TerrainSnapshots::new(
                terrain, cache_dir,
            )),
            height_sampler,
            splat_sampler: Arc::new(create_splat_sampler(terrain, cache_dir)),
            world_cache: Arc::new(std::sync::RwLock::new(cache)),
        }
    }
}

fn create_height_sampler(terrain: &str, cache_dir: &str) -> HeightSampler {
    if crate::is_http_source(terrain) {
        info!("Heightmaps over HTTP from {terrain} (cache: {cache_dir})");
        return HeightSampler::new(crate::terrain_http::HttpHeightTiles::new(
            terrain,
            std::path::PathBuf::from(cache_dir),
        ));
    }
    HeightSampler::new(TerrainIO::new(std::path::PathBuf::from(terrain)))
}

fn create_splat_sampler(terrain: &str, cache_dir: &str) -> crate::splat::SplatSampler {
    if crate::is_http_source(terrain) {
        return crate::splat::SplatSampler::new(crate::splat::HttpSplatTiles::new(
            terrain,
            std::path::PathBuf::from(cache_dir),
        ));
    }
    crate::splat::SplatSampler::new(TerrainIO::new(std::path::PathBuf::from(terrain)))
}

/// How sessions prove who they are. Operator NPCs share one secret; a
/// user-run agent signs in as its own Google account and mints a fresh ID
/// token per connection (they expire in an hour, sessions outlive that).
pub enum AuthSource {
    NpcToken(String),
    Google(GoogleAuth),
}

impl AuthSource {
    async fn authenticate_message(&self, account: Option<&str>) -> anyhow::Result<ClientMessage> {
        match self {
            AuthSource::NpcToken(token) => Ok(ClientMessage::AuthenticateNpc {
                account_name: account
                    .ok_or_else(|| anyhow::anyhow!("[[npcs]] account is required"))?
                    .to_string(),
                npc_token: token.clone(),
            }),
            AuthSource::Google(google) => Ok(ClientMessage::Authenticate {
                google_id_token: google.id_token().await?,
            }),
        }
    }

    /// Whether the agent may delete characters that don't match its config.
    /// Only an npc_token account exists solely for the fixture; a Google
    /// account belongs to a person (`doc/REMOTE_AGENT_CLIENT.md`).
    pub(crate) fn may_delete_mismatches(&self) -> bool {
        match self {
            AuthSource::NpcToken(_) => true,
            AuthSource::Google(_) => false,
        }
    }
}

/// An authenticated connection that has not entered the game yet.
pub struct Lobby {
    pub tx: ws::WsTx,
    pub rx: ws::WsRx,
    pub characters: Vec<Character>,
}

pub async fn login(
    server_url: &str,
    label: &str,
    client: &ws::ClientIdentity,
    auth: &AuthSource,
    account: Option<&str>,
) -> anyhow::Result<Lobby> {
    let (mut tx, mut rx) = ws::connect_ws(server_url, label).await.split();
    ws::send_client_info(&mut tx, client).await?;
    let auth = auth.authenticate_message(account).await?;

    // Not fatal on its own: a refused handshake may already have closed us,
    // and then the reason is waiting on the read side for `wait_for_auth`.
    if let Err(e) = ws::send(&mut tx, &auth).await {
        warn!("[{label}] Auth send failed ({e}); reading the server's reason");
    }

    let characters = ws::wait_for_auth(&mut rx, label).await?;
    Ok(Lobby { tx, rx, characters })
}

pub struct Session {
    pub state: Arc<Mutex<SharedState>>,
    started: Instant,
    reader: JoinHandle<()>,
    tasks: Vec<JoinHandle<()>>,
    watch: Option<Arc<crate::watch::NpcWatch>>,
}

impl Session {
    /// `configure` runs before `buffered` (from `enter_game`) is replayed.
    pub async fn start(
        lobby: Lobby,
        buffered: Vec<ServerMessage>,
        world: &World,
        label: &str,
        watch: Option<Arc<crate::watch::NpcWatch>>,
        configure: impl FnOnce(&mut SharedState),
    ) -> Self {
        let started = Instant::now();
        let Lobby {
            tx: mut ws_tx,
            rx: mut ws_rx,
            characters,
        } = lobby;
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<ClientMessage>(32);
        let mut initial = SharedState::new(
            characters,
            cmd_tx,
            Arc::clone(&world.height_sampler),
            Arc::clone(&world.splat_sampler),
            Arc::clone(&world.world_cache),
            watch.clone(),
        );
        configure(&mut initial);
        let terrain_notify = Arc::clone(&initial.terrain_notify);
        let state = Arc::new(Mutex::new(initial));
        if let Some(w) = &watch {
            w.set_state(Arc::clone(&state));
            w.push("system", "Session connected".to_string());
        }

        let tx_label = label.to_string();
        let tx_task = tokio::spawn(async move {
            while let Some(msg) = cmd_rx.recv().await {
                if let Err(e) = ws::send(&mut ws_tx, &msg).await {
                    error!("[{tx_label}] Failed to send command: {e}");
                    break;
                }
            }
        });

        let rx_state = Arc::clone(&state);
        let rx_label = label.to_string();
        let reader = tokio::spawn(async move {
            for msg in buffered {
                handle_incoming(&rx_state, &rx_label, msg).await;
            }
            loop {
                match ws::recv(&mut ws_rx).await {
                    Ok(msg) => handle_incoming(&rx_state, &rx_label, msg).await,
                    Err(e) => {
                        error!("[{rx_label}] Connection lost: {e}");
                        break;
                    }
                }
            }
        });

        let terrain_state = Arc::clone(&state);
        let terrain_source = Arc::clone(&world.terrain_snapshots);
        let terrain_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = terrain_notify.notified() => {},
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {},
                }
                apply_pending_terrain(&terrain_state, &terrain_source).await;
            }
        });

        let maintenance_state = Arc::clone(&state);
        let maintenance_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(1));
            loop {
                interval.tick().await;
                let mut state = maintenance_state.lock().await;
                if !state.in_game {
                    continue;
                }
                state.drive_player_trade().await;
                if !state.world_view.synchronized {
                    let _ = state
                        .send_background_command(ClientMessage::ResyncWorld)
                        .await;
                    continue;
                }
                state.check_music_finished();
                for command in state.drain_pending_commands() {
                    if let Err(error) = state.send_background_command(command).await {
                        tracing::warn!("Pending command failed: {error}");
                        break;
                    }
                }
            }
        });

        Self {
            state,
            started,
            reader,
            tasks: vec![tx_task, maintenance_task, terrain_task],
            watch,
        }
    }

    pub fn is_closed(&self) -> bool {
        self.reader.is_finished()
    }

    /// A task that lives and dies with the session.
    pub fn attach(&mut self, task: JoinHandle<()>) {
        self.tasks.push(task);
    }

    /// Wait for the connection to end; returns the in-game uptime.
    pub async fn closed(mut self) -> Duration {
        let _ = (&mut self.reader).await;
        for task in &self.tasks {
            task.abort();
        }
        self.state.lock().await.cancel_follow();
        if let Some(w) = &self.watch {
            w.set_disconnected();
        }
        self.started.elapsed()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.reader.abort();
        for task in &self.tasks {
            task.abort();
        }
        // The follow loop holds the state, so nothing else would end it.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let state = Arc::clone(&self.state);
            runtime.spawn(async move {
                state.lock().await.cancel_follow();
            });
        }
    }
}

/// One server message into shared state. The entry handshake reads a few
/// before the reader task starts, so both paths go through here.
async fn handle_incoming(state: &Arc<Mutex<SharedState>>, label: &str, msg: ServerMessage) {
    // Meant for whoever runs this client, so set it apart from the log lines.
    if let ServerMessage::OperatorNotice { message } = &msg {
        eprintln!("\n  ==== Notice from the server operator ({label}) ====\n  {message}\n");
    }
    let mut s = state.lock().await;
    if let ServerMessage::LandscapeChanged { tiles } = &msg {
        for tile in tiles {
            s.splat_sampler
                .update_tile(tile.tile_x, tile.tile_z, &tile.splat)
                .await;
        }
        return;
    }
    if let ServerMessage::LandscapeInvalidated { tiles } = &msg {
        for &(tx, tz) in tiles {
            s.splat_sampler.invalidate_tile(tx, tz).await;
        }
        return;
    }
    if matches!(msg, ServerMessage::GameTimeSync { .. }) {
        let _ = s.send_background_command(ClientMessage::Heartbeat).await;
        s.push_event(msg);
        return;
    }

    let player_trade_event = matches!(
        msg,
        ServerMessage::PlayerTradeRequested { .. }
            | ServerMessage::PlayerTradeRequestResult { .. }
            | ServerMessage::PlayerTradeUpdate { .. }
            | ServerMessage::PlayerTradeEnded { .. }
            | ServerMessage::PlayerTradeError { .. }
    );
    s.push_event(msg);
    if player_trade_event {
        s.drive_player_trade().await;
    }
}

async fn apply_pending_terrain(
    state: &Arc<Mutex<SharedState>>,
    source: &crate::terrain_snapshots::TerrainSnapshots,
) {
    let pending = state.lock().await.pending_terrain.clone();
    for tile in pending {
        let message = match source.load(&tile).await {
            Ok(message) => message,
            Err(error) => {
                warn!(%error, x = tile.x, z = tile.z, "Terrain snapshot remains pending");
                if error.is::<crate::terrain_snapshots::TerrainChanged>()
                    || matches!(
                        error
                            .downcast_ref::<reqwest::Error>()
                            .and_then(|error| error.status()),
                        Some(reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::CONFLICT)
                    )
                {
                    let mut s = state.lock().await;
                    if s.pending_terrain.contains(&tile) {
                        s.world_view.synchronized = false;
                        let _ = s.send_background_command(ClientMessage::ResyncWorld).await;
                    }
                }
                continue;
            }
        };
        let mut s = state.lock().await;
        if !s.pending_terrain.contains(&tile)
            || s.world_view.world_epoch != tile.epoch
            || s.world_view.generation != tile.generation
            || s.world_view
                .subjects
                .get(&format!("terrain:{},{}", tile.x, tile.z))
                != Some(&tile.revision)
            || !s.world_cache.read().unwrap().is_current_epoch(&tile.epoch)
        {
            continue;
        }
        let mut applied = source.applied.lock().await;
        if !s.world_cache.read().unwrap().is_current_epoch(&tile.epoch) {
            continue;
        }
        if applied.epoch != tile.epoch {
            s.height_sampler.clear().await;
            s.splat_sampler.clear().await;
            applied.revisions.clear();
            applied.epoch = tile.epoch.clone();
        }
        if !applied
            .revisions
            .get(&(tile.x, tile.z))
            .is_some_and(|revision| *revision > tile.revision)
        {
            let crate::terrain_snapshots::GroundTerrain { height, splat } = message;
            if let Err(error) = s.height_sampler.update_tile(tile.x, tile.z, &height).await {
                warn!(%error, "Could not apply terrain snapshot");
                continue;
            }
            s.splat_sampler.update_tile(tile.x, tile.z, &splat).await;
            applied.revisions.insert((tile.x, tile.z), tile.revision);
        }
        s.pending_terrain.retain(|pending| pending != &tile);
    }
}

/// The name to answer a rename demand with. Deriving one from the refused name
/// would only be a way around the ban, so the operator's configured
/// `character_name` is the only answer — and None means there is none.
fn rename_target<'a>(current: &str, configured: Option<&'a str>) -> Option<&'a str> {
    configured.filter(|name| !name.eq_ignore_ascii_case(current))
}

/// Enter the game and settle what the server answers with: `JoinSuccess`, or a
/// demand to rename a banned character first. Anything else that lands
/// meanwhile belongs to the session, so it comes back buffered for the reader
/// to replay in order rather than being dropped.
pub async fn enter_game(
    ws_tx: &mut ws::WsTx,
    ws_rx: &mut ws::WsRx,
    label: &str,
    char_id: i64,
    current_name: &str,
    configured_name: Option<&str>,
) -> anyhow::Result<Vec<ServerMessage>> {
    let enter = ClientMessage::EnterGame {
        character_id: char_id,
    };
    ws::send(ws_tx, &enter).await?;
    info!("[{label}] Entering game with character {char_id}...");

    let mut buffered = Vec::new();
    let mut renamed = false;
    loop {
        let msg = ws::recv(ws_rx).await?;
        match msg {
            ServerMessage::JoinSuccess { .. } => {
                // No scene to compile here, so the entry grace ends at once.
                ws::send(ws_tx, &ClientMessage::WorldReady).await?;
                buffered.push(msg);
                return Ok(buffered);
            }
            ServerMessage::CharacterRenameRequired { character_id } => {
                if renamed {
                    // One rename per connection: a second demand means the new
                    // name is refused too, and asking again would spin.
                    return Err(ws::AuthRejected(
                        "renamed the character, but entry was refused again".to_string(),
                    )
                    .into());
                }
                let Some(new_name) = rename_target(current_name, configured_name) else {
                    return Err(ws::AuthRejected(format!(
                        "'{current_name}' cannot be used on this server; set a different \
                         character_name in config.toml and restart"
                    ))
                    .into());
                };
                warn!("[{label}] Entry refused as '{current_name}' — renaming to '{new_name}'");
                ws::send(
                    ws_tx,
                    &ClientMessage::RenameCharacter {
                        character_id,
                        new_name: new_name.to_string(),
                    },
                )
                .await?;
                renamed = true;
            }
            ServerMessage::CharacterRenamed { name, .. } => {
                info!("[{label}] Renamed to '{name}'");
                ws::send(ws_tx, &enter).await?;
            }
            ServerMessage::CharacterError { message } => {
                return Err(ws::AuthRejected(format!("entry refused: {message}")).into());
            }
            other => buffered.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_world_snapshot_does_not_send_a_move_that_can_cancel_a_new_pose() {
        let (mut s, mut rx) = crate::state::tests::test_state();
        let player = crate::state::tests::test_player(0.0, 0.0);
        let position = player.position;
        s.self_player_id = Some(player.id);
        s.self_player = Some(player);
        let state = Arc::new(Mutex::new(s));
        handle_incoming(
            &state,
            "Tobin",
            ServerMessage::WorldUpdate {
                world_epoch: "snapshot-test".into(),
                generation: 1,
                sequence: 1,
                position,
                floor_level: 0,
                reset: true,
                ready: true,
                events: vec![],
            },
        )
        .await;
        assert!(state.lock().await.world_view.synchronized);
        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn terrain_http_does_not_lock_state_and_old_responses_cannot_apply() {
        use crate::terrain_snapshots::{PendingTerrain, TerrainSnapshots};
        use onlinerpg_terrain::io::TerrainIO;
        let dir = std::env::temp_dir().join(format!("terrain_async_{}", rand::random::<u64>()));
        let terrain = TerrainIO::new(dir.clone());
        let bytes = onlinerpg_terrain::defaults::default_heightmap();
        terrain.write_heightmap(0, 0, &bytes).await.unwrap();
        let ServerMessage::TerrainTileVersion { files, .. } =
            terrain.tile_manifest(0, 0).await.unwrap()
        else {
            panic!()
        };
        let started = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let request_started = started.clone();
        let request_release = release.clone();
        let app = axum::Router::new().route(
            "/api/terrain/files/{kind}/{region}/{file}",
            axum::routing::get(move || {
                let bytes = bytes.clone();
                let started = request_started.clone();
                let release = request_release.clone();
                async move {
                    started.notify_one();
                    release.notified().await;
                    bytes
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let source = Arc::new(TerrainSnapshots::new(&origin, dir.to_str().unwrap()));
        let (mut s, _rx) = crate::state::tests::test_state();
        s.world_view.world_epoch = "world".into();
        s.world_view.generation = 1;
        s.world_view.subjects.insert("terrain:0,0".into(), 1);
        s.world_cache.write().unwrap().ensure_world_epoch("world");
        let tile = PendingTerrain {
            epoch: "world".into(),
            generation: 1,
            revision: 1,
            x: 0,
            z: 0,
            files,
        };
        s.pending_terrain.push(tile.clone());
        let state = Arc::new(Mutex::new(s));
        let worker_state = state.clone();
        let worker_source = source.clone();
        let worker =
            tokio::spawn(async move { apply_pending_terrain(&worker_state, &worker_source).await });
        tokio::time::timeout(Duration::from_secs(2), started.notified())
            .await
            .unwrap();
        {
            let mut s = tokio::time::timeout(Duration::from_secs(2), state.lock())
                .await
                .expect("HTTP must not hold shared state");
            assert_eq!(s.pending_terrain.len(), 1);
            s.world_view.generation = 2;
            s.pending_terrain[0].generation = 2;
        }
        release.notify_one();
        worker.await.unwrap();
        assert!(source.applied.lock().await.revisions.is_empty());
        assert_eq!(state.lock().await.pending_terrain.len(), 1);
        apply_pending_terrain(&state, &source).await;
        assert!(state.lock().await.pending_terrain.is_empty());
        assert_eq!(source.applied.lock().await.revisions.get(&(0, 0)), Some(&1));
        server.abort();
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[test]
    fn a_rename_demand_is_answered_from_the_configured_name() {
        assert_eq!(rename_target("시스템", Some("인공지능")), Some("인공지능"));
    }

    #[test]
    fn a_config_that_cannot_answer_has_no_target() {
        // Offering the refused name back — in any case — is refused again.
        assert_eq!(rename_target("시스템", None), None);
        assert_eq!(rename_target("시스템", Some("시스템")), None);
        assert_eq!(rename_target("SYSTEM", Some("system")), None);
    }
    #[tokio::test]
    async fn a_session_logs_in_enters_and_ends_when_the_server_closes() {
        use futures_util::SinkExt;
        use onlinerpg_shared::{deserialize_client_msg, serialize_server_msg};
        use tokio_tungstenite::tungstenite::Message;

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let player = crate::state::tests::test_player(3.0, 4.0);
        let player_id = player.id;
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let mut seen = Vec::new();
            while let Some(Ok(Message::Binary(bytes))) = ws.next().await {
                let msg = deserialize_client_msg(&bytes).unwrap();
                let replies = match &msg {
                    ClientMessage::AuthenticateNpc { account_name, .. } => {
                        vec![ServerMessage::AuthSuccess {
                            account_name: account_name.clone(),
                            characters: vec![crate::orchestrator::tests::character(
                                "Me",
                                onlinerpg_shared::CharacterClass::Knight,
                                onlinerpg_shared::Gender::Male,
                            )],
                            cape_upload_token: String::new(),
                            playing: None,
                        }]
                    }
                    ClientMessage::EnterGame { .. } => vec![
                        ServerMessage::OperatorNotice {
                            message: "before entry".into(),
                        },
                        ServerMessage::JoinSuccess {
                            player: player.clone(),
                            is_admin: false,
                        },
                    ],
                    _ => Vec::new(),
                };
                let done = matches!(msg, ClientMessage::WorldReady);
                seen.push(msg);
                for reply in replies {
                    let bytes = serialize_server_msg(&reply).unwrap();
                    ws.send(Message::Binary(bytes.into())).await.unwrap();
                }
                if done {
                    break;
                }
            }
            let _ = ws.close(None).await;
            seen
        });

        let auth = AuthSource::NpcToken("secret".into());
        let mut lobby = login(
            &format!("ws://{addr}"),
            "test",
            &ws::ClientIdentity::AGENT_CLIENT,
            &auth,
            Some("acct"),
        )
        .await
        .unwrap();
        let character = lobby.characters[0].clone();
        let buffered = enter_game(
            &mut lobby.tx,
            &mut lobby.rx,
            "test",
            character.id,
            &character.name,
            None,
        )
        .await
        .unwrap();
        assert!(matches!(
            buffered[..],
            [
                ServerMessage::OperatorNotice { .. },
                ServerMessage::JoinSuccess { .. }
            ]
        ));

        let dir = std::env::temp_dir().join(format!("session_test_{}", rand::random::<u64>()));
        let world = World {
            terrain_snapshots: Arc::new(crate::terrain_snapshots::TerrainSnapshots::new(
                "http://127.0.0.1:1",
                dir.to_str().unwrap(),
            )),
            height_sampler: Arc::new(HeightSampler::new(crate::state::tests::NoTiles)),
            splat_sampler: Arc::new(crate::splat::SplatSampler::new(
                crate::state::tests::NoTiles,
            )),
            world_cache: Arc::new(std::sync::RwLock::new(WorldCache::new())),
        };
        let mut configured = false;
        let session = Session::start(lobby, buffered, &world, "test", None, |s| {
            configured = s.self_player_id.is_none();
        })
        .await;
        assert!(configured);
        let state = Arc::clone(&session.state);
        tokio::time::timeout(Duration::from_secs(5), session.closed())
            .await
            .expect("the session ends once the server closes");
        assert_eq!(state.lock().await.self_player_id, Some(player_id));

        let seen = server.await.unwrap();
        assert!(matches!(
            seen[..],
            [
                ClientMessage::ClientInfo { .. },
                ClientMessage::AuthenticateNpc { .. },
                ClientMessage::EnterGame { .. },
                ClientMessage::WorldReady
            ]
        ));
    }
}
