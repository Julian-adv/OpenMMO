//! Heightmap tiles over HTTP, for agent-clients that do not sit on the game
//! server's filesystem. Uses the same public endpoint the web client reads
//! (`GET /api/terrain/height/{tx}/{tz}`), backed by a disk cache so a restart
//! does not re-download what it already has.

use std::path::{Path, PathBuf};

use onlinerpg_terrain::defaults::{self, HEIGHTMAP_SIZE};
use onlinerpg_terrain::height::HeightTiles;
use tracing::{debug, warn};

pub struct HttpHeightTiles {
    /// Server origin, e.g. `https://openmmo.to.nexus` (no trailing slash).
    base_url: String,
    cache_dir: PathBuf,
    http: reqwest::Client,
}

impl HttpHeightTiles {
    pub fn new(base_url: &str, cache_dir: PathBuf) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            cache_dir,
            http: reqwest::Client::new(),
        }
    }

    fn cache_path(&self, tx: i32, tz: i32) -> PathBuf {
        self.cache_dir.join(format!("{tx}_{tz}.bin"))
    }

    async fn read_cached(path: &Path) -> Option<Vec<u8>> {
        match tokio::fs::read(path).await {
            Ok(data) if data.len() == HEIGHTMAP_SIZE => Some(data),
            Ok(data) => {
                warn!(
                    "Cached heightmap {:?} has wrong size {} — refetching",
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
        let url = format!("{}/api/terrain/height/{tx}/{tz}", self.base_url);
        let response = self.http.get(&url).send().await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let response = response.error_for_status()?;
        let bytes = response.bytes().await?.to_vec();
        if bytes.len() != HEIGHTMAP_SIZE {
            anyhow::bail!(
                "{url} returned {} bytes, expected {HEIGHTMAP_SIZE}",
                bytes.len()
            );
        }
        Ok(Some(bytes))
    }
}

#[async_trait::async_trait]
impl HeightTiles for HttpHeightTiles {
    async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        let path = self.cache_path(tx, tz);
        if let Some(cached) = Self::read_cached(&path).await {
            return Ok(cached);
        }

        match self.fetch(tx, tz).await {
            Ok(Some(data)) => {
                if let Err(e) = Self::write_cached(&path, &data).await {
                    warn!("Failed to cache heightmap {tx},{tz}: {e}");
                }
                debug!("Fetched heightmap tile {tx},{tz}");
                Ok(data)
            }
            // Outside the baked area: the local source answers the same way.
            Ok(None) => Ok(defaults::default_heightmap()),
            // Surfaced as an error so the caller retries later instead of
            // walking on a flat plane it believes in.
            Err(e) => Err(std::io::Error::other(format!(
                "heightmap {tx},{tz} fetch failed: {e}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::Path as AxumPath, routing::get, Router};

    fn scratch_dir() -> PathBuf {
        std::env::temp_dir().join(format!("onlinerpg_tiles_{}", rand::random::<u64>()))
    }

    /// Serves one tile at (1, 2); everything else 404s, like the real API does
    /// outside the baked area. Returns the origin and a handle to stop it.
    async fn serve_one_tile(body: Vec<u8>) -> (String, tokio::task::JoinHandle<()>) {
        let app = Router::new().route(
            "/api/terrain/height/{tx}/{tz}",
            get(move |AxumPath((tx, tz)): AxumPath<(i32, i32)>| async move {
                match (tx, tz) {
                    (1, 2) => Ok(body),
                    _ => Err(axum::http::StatusCode::NOT_FOUND),
                }
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
        // A 404 is an answer, not a tile: nothing should be cached for it.
        assert!(!tiles.cache_path(9, 9).exists());

        server.abort();
        let _ = tokio::fs::remove_dir_all(&cache).await;
    }

    #[tokio::test]
    async fn unreachable_server_is_an_error_not_flat_ground() {
        let tiles = HttpHeightTiles::new("http://127.0.0.1:1", scratch_dir());
        assert!(tiles.read_heightmap(1, 2).await.is_err());
    }
}
