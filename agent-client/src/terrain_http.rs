//! Hash-verified HTTP terrain with a persistent sampler cache.

use std::path::{Path, PathBuf};

use onlinerpg_terrain::defaults::{self, HEIGHTMAP_SIZE};
use onlinerpg_terrain::height::HeightTiles;
use tracing::{debug, warn};

static WORLD_EPOCH: std::sync::RwLock<String> = std::sync::RwLock::new(String::new());

pub(crate) fn set_world_epoch(epoch: &str) {
    *WORLD_EPOCH.write().unwrap() = epoch.to_owned();
}

fn world_epoch() -> String {
    WORLD_EPOCH.read().unwrap().clone()
}

pub(crate) fn http_client() -> reqwest::Client {
    static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    HTTP.get_or_init(reqwest::Client::new).clone()
}

/// Hash-verified terrain files with a revision-protected sampler cache.
pub struct HttpTiles {
    /// Server origin, e.g. `https://openmmo.to.nexus` (no trailing slash).
    base_url: String,
    cache_dir: PathBuf,
    http: reqwest::Client,
    files: crate::terrain_snapshots::TerrainSnapshots,
    kind: &'static str,
    /// Cache filename prefix — height tiles predate the prefix and use "".
    prefix: &'static str,
    expected_size: usize,
    revisions: tokio::sync::Mutex<std::collections::HashMap<(i32, i32), u64>>,
}

impl HttpTiles {
    pub fn new(
        base_url: &str,
        cache_dir: PathBuf,
        kind: &'static str,
        prefix: &'static str,
        expected_size: usize,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            files: crate::terrain_snapshots::TerrainSnapshots::new(
                base_url,
                &cache_dir.to_string_lossy(),
            ),
            cache_dir,
            http: reqwest::Client::new(),
            kind,
            prefix,
            expected_size,
            revisions: Default::default(),
        }
    }

    fn cache_path(&self, tx: i32, tz: i32) -> PathBuf {
        self.cache_dir
            .join(onlinerpg_shared::LAYOUT_VERSION)
            .join(world_epoch())
            .join(format!("{}{tx}_{tz}.bin", self.prefix))
    }

    async fn read_cached(&self, path: &Path) -> Option<Vec<u8>> {
        match tokio::fs::read(path).await {
            Ok(data) if data.len() == self.expected_size => Some(data),
            Ok(data) => {
                warn!(
                    "Cached {} tile {:?} has wrong size {} — refetching",
                    self.kind,
                    path,
                    data.len()
                );
                None
            }
            Err(_) => None,
        }
    }

    /// Write via a temp file + rename so a killed process cannot leave a
    /// half-written tile that later reads would trust.
    async fn write_cached(path: &Path, data: &[u8]) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let tmp = path.with_extension("part");
        tokio::fs::write(&tmp, data).await?;
        tokio::fs::rename(&tmp, path).await
    }

    async fn fetch(&self, tx: i32, tz: i32) -> anyhow::Result<Option<Vec<u8>>> {
        let tx = onlinerpg_terrain::coords::wrap_tile_x(tx);
        let url = format!("{}/api/terrain/manifest/{tx}/{tz}", self.base_url);
        for attempt in 0..2 {
            let files: onlinerpg_shared::terrain_files::TerrainFiles = self
                .http
                .get(&url)
                .header(reqwest::header::CACHE_CONTROL, "no-store")
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let file = if self.kind == "height" {
                files.height.as_ref()
            } else {
                files.landscape.as_ref().or(files.splat.as_ref())
            };
            let Some(file) = file else { return Ok(None) };
            let bytes = match self.files.load_file(file).await {
                Ok(bytes) => bytes,
                Err(error)
                    if attempt == 0
                        && (error.is::<crate::terrain_snapshots::TerrainChanged>()
                            || error
                                .downcast_ref::<reqwest::Error>()
                                .and_then(|e| e.status())
                                == Some(reqwest::StatusCode::NOT_FOUND)) =>
                {
                    continue
                }
                Err(error) => return Err(error),
            };
            let bytes = if self.kind == "splat" && files.landscape.is_some() {
                onlinerpg_terrain::landscaping::decode_landscaping(&bytes, tx, tz)?.splat
            } else {
                bytes
            };
            anyhow::ensure!(
                bytes.len() == self.expected_size,
                "Invalid {} tile size",
                self.kind
            );
            return Ok(Some(bytes));
        }
        unreachable!()
    }

    /// Missing files use defaults; network errors remain retryable.
    pub async fn read(&self, tx: i32, tz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let epoch = world_epoch();
        let path = self.cache_path(tx, tz);
        if let Some(cached) = self.read_cached(&path).await {
            if epoch != world_epoch() {
                return Err(std::io::Error::other(
                    "World epoch changed during cache read",
                ));
            }
            return Ok(Some(cached));
        }
        self.read_fresh(tx, tz).await
    }

    pub async fn read_fresh(&self, tx: i32, tz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let epoch = world_epoch();
        let path = self.cache_path(tx, tz);
        let revision = self
            .revisions
            .lock()
            .await
            .get(&(tx, tz))
            .copied()
            .unwrap_or(0);
        let fetched = self.fetch(tx, tz).await;
        if epoch != world_epoch() {
            return Err(std::io::Error::other(
                "World epoch changed during tile request",
            ));
        }
        let revisions = self.revisions.lock().await;
        if revisions.get(&(tx, tz)).copied().unwrap_or(0) != revision {
            return self
                .read_cached(&path)
                .await
                .map(Some)
                .ok_or_else(|| std::io::Error::other("Updated terrain cache is unavailable"));
        }
        match fetched {
            Ok(Some(data)) => {
                if let Err(e) = Self::write_cached(&path, &data).await {
                    warn!("Failed to cache {} tile {tx},{tz}: {e}", self.kind);
                }
                debug!("Fetched {} tile {tx},{tz}", self.kind);
                Ok(Some(data))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(std::io::Error::other(format!(
                "{} tile {tx},{tz} fetch failed: {e}",
                self.kind
            ))),
        }
    }
}

pub struct HttpHeightTiles(HttpTiles);

impl HttpHeightTiles {
    pub fn new(base_url: &str, cache_dir: PathBuf) -> Self {
        Self(HttpTiles::new(
            base_url,
            cache_dir,
            "height",
            "",
            HEIGHTMAP_SIZE,
        ))
    }
}

#[async_trait::async_trait]
impl HeightTiles for HttpHeightTiles {
    async fn cache_heightmap(&self, tx: i32, tz: i32, raw: &[u8]) -> std::io::Result<()> {
        let mut revisions = self.0.revisions.lock().await;
        *revisions.entry((tx, tz)).or_default() += 1;
        HttpTiles::write_cached(&self.0.cache_path(tx, tz), raw).await
    }

    async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        match self.0.read(tx, tz).await? {
            Some(data) => Ok(data),
            // Outside the baked area: the local source answers the same way.
            None => Ok(defaults::default_heightmap()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Path as AxumPath, routing::get, Router};

    #[tokio::test]
    async fn late_http_cannot_overwrite_a_stream_snapshot_or_its_disk_cache() {
        use std::sync::Arc;
        let began = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        let app = Router::new()
            .route(
                "/api/terrain/manifest/{tx}/{tz}",
                get(|| async { axum::Json(height_manifest(&vec![1; HEIGHTMAP_SIZE])) }),
            )
            .route(
                "/api/terrain/files/{kind}/{region}/{file}",
                get({
                    let began = began.clone();
                    let release = release.clone();
                    move || {
                        let began = began.clone();
                        let release = release.clone();
                        async move {
                            began.notify_one();
                            release.notified().await;
                            vec![1u8; HEIGHTMAP_SIZE]
                        }
                    }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let cache = scratch_dir();
        let tiles = Arc::new(HttpHeightTiles::new(
            &format!("http://{address}"),
            cache.clone(),
        ));
        let pending = tokio::spawn({
            let tiles = tiles.clone();
            async move { tiles.read_heightmap(1, 2).await.unwrap() }
        });
        began.notified().await;
        let fresh = vec![9u8; HEIGHTMAP_SIZE];
        tiles.cache_heightmap(1, 2, &fresh).await.unwrap();
        release.notify_one();
        assert_eq!(pending.await.unwrap(), fresh);
        server.abort();
        assert_eq!(tiles.read_heightmap(1, 2).await.unwrap(), fresh);
        tokio::fs::remove_dir_all(cache).await.unwrap();
    }

    fn scratch_dir() -> PathBuf {
        std::env::temp_dir().join(format!("onlinerpg_tiles_{}", rand::random::<u64>()))
    }

    fn height_manifest(body: &[u8]) -> onlinerpg_shared::terrain_files::TerrainFiles {
        onlinerpg_shared::terrain_files::TerrainFiles {
            height: Some(onlinerpg_shared::terrain_files::TerrainFile {
                path: "height/r+00_+00/h_+0001_+0002.bin".into(),
                hash: onlinerpg_terrain::manifest::content_hash(body),
            }),
            ..Default::default()
        }
    }

    async fn serve_one_tile(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
        let files = height_manifest(&body);
        let app = Router::new()
            .route(
                "/api/terrain/manifest/{tx}/{tz}",
                get(move |AxumPath((tx, tz)): AxumPath<(i32, i32)>| {
                    let files = files.clone();
                    async move {
                        axum::Json(if (tx, tz) == (1, 2) {
                            files
                        } else {
                            Default::default()
                        })
                    }
                }),
            )
            .route(
                "/api/terrain/files/{kind}/{region}/{file}",
                get(move || {
                    let body = body.clone();
                    async move { body }
                }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        (format!("http://{addr}"), handle)
    }

    #[tokio::test]
    async fn fetches_a_tile_then_serves_it_from_disk() {
        let expected = vec![7u8; HEIGHTMAP_SIZE];
        let (base_url, server) = serve_one_tile(expected.clone()).await;
        let cache = scratch_dir();
        let tiles = HttpHeightTiles::new(&base_url, cache.clone());

        assert_eq!(tiles.read_heightmap(1, 2).await.unwrap(), expected);

        // With the server gone, only the disk cache can answer.
        server.abort();
        assert_eq!(tiles.read_heightmap(1, 2).await.unwrap(), expected);

        let _ = tokio::fs::remove_dir_all(&cache).await;
    }

    #[tokio::test]
    async fn unbaked_tiles_fall_back_to_flat_ground() {
        let (base_url, server) = serve_one_tile(vec![0u8; HEIGHTMAP_SIZE]).await;
        let cache = scratch_dir();
        let tiles = HttpHeightTiles::new(&base_url, cache.clone());

        assert_eq!(
            tiles.read_heightmap(9, 9).await.unwrap(),
            defaults::default_heightmap()
        );
        // Missing files do not populate the sampler cache.
        assert!(!tiles.0.cache_path(9, 9).exists());

        server.abort();
        let _ = tokio::fs::remove_dir_all(&cache).await;
    }

    #[tokio::test]
    async fn unreachable_server_is_an_error_not_flat_ground() {
        let tiles = HttpHeightTiles::new("http://127.0.0.1:1", scratch_dir());
        assert!(tiles.read_heightmap(1, 2).await.is_err());
    }
}
