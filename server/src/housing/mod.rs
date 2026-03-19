pub mod routes;

use onlinerpg_shared::housing::HouseData;
use std::path::PathBuf;
use tokio::fs;
use tracing::{error, info};

/// File-based housing storage, organized by terrain chunk.
///
/// Layout: `{base_dir}/{chunk_x}_{chunk_z}/{house_id}.json`
///
/// Chunk coordinates are derived from house origin using CHUNK_SIZE.
#[derive(Clone)]
pub struct HousingIO {
    base_dir: PathBuf,
}

/// Chunk size in world units — matches terrain tile size.
const CHUNK_SIZE: f32 = 64.0;

pub fn world_to_chunk(x: f32, z: f32) -> (i32, i32) {
    (
        (x / CHUNK_SIZE).floor() as i32,
        (z / CHUNK_SIZE).floor() as i32,
    )
}

impl HousingIO {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn chunk_dir(&self, cx: i32, cz: i32) -> PathBuf {
        self.base_dir.join(format!("r{:+03}_{:+03}", cx, cz))
    }

    fn house_path(&self, cx: i32, cz: i32, house_id: &str) -> PathBuf {
        self.chunk_dir(cx, cz).join(format!("{}.json", house_id))
    }

    /// Read all houses in a chunk.
    pub async fn read_chunk(&self, cx: i32, cz: i32) -> std::io::Result<Vec<HouseData>> {
        let dir = self.chunk_dir(cx, cz);
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut houses = Vec::new();
        let mut entries = fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                match fs::read_to_string(&path).await {
                    Ok(content) => match serde_json::from_str::<HouseData>(&content) {
                        Ok(house) => houses.push(house),
                        Err(e) => error!("Failed to parse house file {:?}: {}", path, e),
                    },
                    Err(e) => error!("Failed to read house file {:?}: {}", path, e),
                }
            }
        }

        Ok(houses)
    }

    /// Save a house to disk. Creates chunk directory if needed.
    pub async fn write_house(&self, house: &HouseData) -> std::io::Result<()> {
        let (cx, cz) = world_to_chunk(house.origin.x, house.origin.z);
        let dir = self.chunk_dir(cx, cz);
        fs::create_dir_all(&dir).await?;

        let path = self.house_path(cx, cz, &house.id);
        let json = serde_json::to_string_pretty(house)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json).await?;
        info!("Saved house {} to {:?}", house.id, path);
        Ok(())
    }

    /// Delete a house from disk. Returns true if the file existed.
    pub async fn delete_house(&self, house_id: &str, cx: i32, cz: i32) -> std::io::Result<bool> {
        let path = self.house_path(cx, cz, house_id);
        match fs::remove_file(&path).await {
            Ok(()) => {
                info!("Deleted house {} from {:?}", house_id, path);
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Find and read a specific house by ID across all chunks.
    /// (For removal when chunk coords aren't known.)
    pub async fn find_house(&self, house_id: &str) -> std::io::Result<Option<HouseData>> {
        if !self.base_dir.exists() {
            return Ok(None);
        }
        let mut entries = fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let path = entry.path().join(format!("{}.json", house_id));
                if path.exists() {
                    let content = fs::read_to_string(&path).await?;
                    match serde_json::from_str::<HouseData>(&content) {
                        Ok(house) => return Ok(Some(house)),
                        Err(e) => error!("Failed to parse house {:?}: {}", path, e),
                    }
                }
            }
        }
        Ok(None)
    }
}
