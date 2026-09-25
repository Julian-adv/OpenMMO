use onlinerpg_shared::terrain_files::{TerrainFile, TerrainFiles};
use onlinerpg_terrain::{
    defaults,
    manifest::{content_hash, raw_file_path, valid_hash},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Weak},
};
use tokio::sync::Mutex;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingTerrain {
    pub epoch: String,
    pub generation: u64,
    pub revision: u64,
    pub x: i32,
    pub z: i32,
    pub files: TerrainFiles,
}

#[derive(Default)]
pub struct AppliedTerrain {
    pub epoch: String,
    pub revisions: HashMap<(i32, i32), u64>,
}

#[derive(Debug)]
pub struct TerrainChanged;
impl std::fmt::Display for TerrainChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Terrain file changed; resync its manifest")
    }
}
impl std::error::Error for TerrainChanged {}

#[derive(Debug)]
pub struct GroundTerrain {
    pub height: Vec<u8>,
    pub splat: Vec<u8>,
}

pub struct TerrainSnapshots {
    source: String,
    cache_dir: PathBuf,
    loading: Mutex<HashMap<String, Weak<Mutex<()>>>>,
    pub applied: Mutex<AppliedTerrain>,
}

impl TerrainSnapshots {
    pub fn new(source: &str, cache_dir: &str) -> Self {
        Self {
            source: source.trim_end_matches('/').to_owned(),
            cache_dir: PathBuf::from(cache_dir).join("files"),
            loading: Mutex::default(),
            applied: Mutex::default(),
        }
    }

    pub async fn load(&self, tile: &PendingTerrain) -> anyhow::Result<GroundTerrain> {
        let height = async {
            match &tile.files.height {
                Some(file) => self.load_file(file).await,
                None => Ok(defaults::default_heightmap()),
            }
        };
        let splat = async {
            if let Some(file) = &tile.files.landscape {
                let bytes = self.load_file(file).await?;
                Ok(
                    onlinerpg_terrain::landscaping::decode_landscaping(&bytes, tile.x, tile.z)?
                        .splat,
                )
            } else {
                match &tile.files.splat {
                    Some(file) => self.load_file(file).await,
                    None => Ok(defaults::default_splatmap()),
                }
            }
        };
        let (height, splat) = tokio::try_join!(height, splat)?;
        anyhow::ensure!(
            height.len() == defaults::HEIGHTMAP_SIZE && splat.len() == defaults::SPLATMAP_SIZE,
            "Invalid terrain file size"
        );
        Ok(GroundTerrain { height, splat })
    }

    pub(crate) async fn load_file(&self, file: &TerrainFile) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(valid_hash(&file.hash), "Invalid terrain hash");
        let relative = raw_file_path(std::path::Path::new(""), &file.path)?;
        let lock = {
            let mut loading = self.loading.lock().await;
            loading.retain(|_, lock| lock.strong_count() > 0);
            let entry = loading.entry(file.hash.clone()).or_default();
            match entry.upgrade() {
                Some(lock) => lock,
                None => {
                    let lock = Arc::new(Mutex::new(()));
                    *entry = Arc::downgrade(&lock);
                    lock
                }
            }
        };
        let _loading = lock.lock().await;
        let path = self.cache_dir.join(format!("{}.bin", file.hash));
        if let Ok(bytes) = tokio::fs::read(&path).await {
            if content_hash(&bytes) == file.hash {
                return Ok(bytes);
            }
        }
        let bytes = if crate::is_http_source(&self.source) {
            crate::terrain_http::http_client()
                .get(format!("{}/api/terrain/files/{}", self.source, file.path))
                .header(reqwest::header::CACHE_CONTROL, "no-store")
                .timeout(std::time::Duration::from_secs(15))
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?
                .to_vec()
        } else {
            match tokio::fs::read(PathBuf::from(&self.source).join(relative)).await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Err(TerrainChanged.into())
                }
                Err(error) => return Err(error.into()),
            }
        };
        if content_hash(&bytes) != file.hash {
            return Err(TerrainChanged.into());
        }
        let write = async {
            tokio::fs::create_dir_all(&self.cache_dir).await?;
            onlinerpg_terrain::io::atomic_write(&path, &bytes).await
        }
        .await;
        if let Err(error) = write {
            tracing::warn!(%error, "Could not cache terrain file");
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::ServerMessage;
    use onlinerpg_terrain::io::TerrainIO;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn downloads_are_shared_and_verified_cache_survives_a_restart() {
        let dir = std::env::temp_dir().join(format!("ground_files_{}", rand::random::<u64>()));
        let terrain = TerrainIO::new(dir.join("terrain"));
        let bytes = defaults::default_heightmap();
        terrain.write_heightmap(0, 0, &bytes).await.unwrap();
        let ServerMessage::TerrainTileVersion { files, .. } =
            terrain.tile_manifest(0, 0).await.unwrap()
        else {
            panic!()
        };
        let tile = PendingTerrain {
            epoch: "first".into(),
            generation: 1,
            revision: 1,
            x: 0,
            z: 0,
            files,
        };
        let requests = Arc::new(AtomicUsize::new(0));
        let count = requests.clone();
        let app = axum::Router::new().route(
            "/api/terrain/files/{kind}/{region}/{file}",
            axum::routing::get(move || {
                let bytes = bytes.clone();
                let count = count.clone();
                async move {
                    count.fetch_add(1, Ordering::SeqCst);
                    bytes
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let cache = dir.join("cache");
        let source = TerrainSnapshots::new(&origin, cache.to_str().unwrap());
        let (a, b) = tokio::join!(source.load(&tile), source.load(&tile));
        assert!(a.is_ok() && b.is_ok());
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        server.abort();
        let _ = server.await;
        let restarted = TerrainSnapshots::new(&origin, cache.to_str().unwrap());
        let tile = PendingTerrain {
            epoch: "second".into(),
            generation: 5,
            revision: 10,
            ..tile
        };
        assert!(restarted.load(&tile).await.is_ok());
        let path = restarted
            .cache_dir
            .join(format!("{}.bin", tile.files.height.as_ref().unwrap().hash));
        tokio::fs::write(&path, b"corrupt").await.unwrap();
        assert!(restarted.load(&tile).await.is_err());
        let local = TerrainSnapshots::new(
            terrain.base_dir().to_str().unwrap(),
            cache.to_str().unwrap(),
        );
        assert!(local.load(&tile).await.is_ok());
        tokio::fs::remove_file(&path).await.unwrap();
        let mut changed = defaults::default_heightmap();
        changed[0] ^= 1;
        terrain.write_heightmap(0, 0, &changed).await.unwrap();
        assert!(local.load(&tile).await.unwrap_err().is::<TerrainChanged>());
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn ground_uses_landscaping_and_never_downloads_visual_files() {
        let dir = std::env::temp_dir().join(format!("ground_landscape_{}", rand::random::<u64>()));
        let terrain = TerrainIO::new(dir.join("terrain"));
        let mut splat = defaults::default_splatmap();
        splat[0] = 5;
        terrain
            .write_landscaping_tile(&onlinerpg_shared::landscaping::LandscapingTile {
                tile_x: 0,
                tile_z: 0,
                splat: splat.clone(),
                cleared: vec![255; 512],
            })
            .await
            .unwrap();
        let ServerMessage::TerrainTileVersion { mut files, .. } =
            terrain.tile_manifest(0, 0).await.unwrap()
        else {
            panic!()
        };
        let missing = Some(TerrainFile {
            path: "grass/r+00_+00/g_+0000_+0000.bin".into(),
            hash: "0".repeat(64),
        });
        files.grass = missing.clone();
        files.trees = missing.clone();
        files.splat = missing;
        let loader = TerrainSnapshots::new(
            terrain.base_dir().to_str().unwrap(),
            dir.join("cache").to_str().unwrap(),
        );
        let data = loader
            .load(&PendingTerrain {
                epoch: "test".into(),
                generation: 1,
                revision: 1,
                x: 0,
                z: 0,
                files,
            })
            .await
            .unwrap();
        assert_eq!(data.splat, splat);
        assert_eq!(data.height, defaults::default_heightmap());
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }
}
