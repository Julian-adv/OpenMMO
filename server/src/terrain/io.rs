use std::path::PathBuf;
use tokio::fs;
use tracing::warn;

use super::coords;
use super::defaults;

pub struct TerrainIO {
    base_dir: PathBuf,
}

impl TerrainIO {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        let path = coords::heightmap_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) if data.len() == defaults::HEIGHTMAP_SIZE => Ok(data),
            Ok(data) => {
                warn!(
                    "Heightmap {:?} has wrong size {} (expected {}), returning default",
                    path,
                    data.len(),
                    defaults::HEIGHTMAP_SIZE
                );
                Ok(defaults::default_heightmap())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(defaults::default_heightmap()),
            Err(e) => Err(e),
        }
    }

    pub async fn write_heightmap(&self, tx: i32, tz: i32, data: &[u8]) -> std::io::Result<()> {
        if data.len() != defaults::HEIGHTMAP_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Heightmap: expected {} bytes, got {}",
                    defaults::HEIGHTMAP_SIZE,
                    data.len()
                ),
            ));
        }
        let path = coords::heightmap_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    pub async fn read_splatmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        let path = coords::splatmap_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) if data.len() == defaults::SPLATMAP_SIZE => Ok(data),
            Ok(data) => {
                warn!(
                    "Splatmap {:?} has wrong size {} (expected {}), returning default",
                    path,
                    data.len(),
                    defaults::SPLATMAP_SIZE
                );
                Ok(defaults::default_splatmap())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(defaults::default_splatmap()),
            Err(e) => Err(e),
        }
    }

    pub async fn write_splatmap(&self, tx: i32, tz: i32, data: &[u8]) -> std::io::Result<()> {
        if data.len() != defaults::SPLATMAP_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Splatmap: expected {} bytes, got {}",
                    defaults::SPLATMAP_SIZE,
                    data.len()
                ),
            ));
        }
        let path = coords::splatmap_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    pub async fn read_meta(&self, rx: i32, rz: i32) -> std::io::Result<serde_json::Value> {
        let path = coords::meta_path(&self.base_dir, rx, rz);
        match fs::read_to_string(&path).await {
            Ok(json_str) => serde_json::from_str(&json_str)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(defaults::default_meta_json()),
            Err(e) => Err(e),
        }
    }

    pub async fn write_meta(&self, rx: i32, rz: i32, json: &serde_json::Value) -> std::io::Result<()> {
        let layers = json.get("layers")
            .and_then(|l| l.as_array())
            .ok_or_else(|| std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "meta must contain \"layers\" array",
            ))?;
        if layers.len() != 4 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("layers must have exactly 4 entries, got {}", layers.len()),
            ));
        }
        let path = coords::meta_path(&self.base_dir, rx, rz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json_str = serde_json::to_string_pretty(json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json_str).await
    }
}
