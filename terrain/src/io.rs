use std::path::PathBuf;
use tokio::fs;
use tracing::warn;

use crate::coords;
use crate::defaults;

pub struct TerrainIO {
    base_dir: PathBuf,
}

impl TerrainIO {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn base_dir(&self) -> &PathBuf {
        &self.base_dir
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

    pub async fn read_minimap(&self, rx: i32, rz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let path = coords::minimap_path(&self.base_dir, rx, rz);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub async fn write_minimap(&self, rx: i32, rz: i32, data: &[u8]) -> std::io::Result<()> {
        let path = coords::minimap_path(&self.base_dir, rx, rz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    /// Read pre-computed grass placement data (variable-length binary).
    /// Returns None if the file does not exist.
    pub async fn read_grass(&self, tx: i32, tz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let path = coords::grass_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write pre-computed grass placement data (variable-length binary).
    pub async fn write_grass(&self, tx: i32, tz: i32, data: &[u8]) -> std::io::Result<()> {
        let path = coords::grass_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    /// Read original (pre-housing) heightmap. Returns None if not found.
    pub async fn read_original_heightmap(
        &self,
        tx: i32,
        tz: i32,
    ) -> std::io::Result<Option<Vec<u8>>> {
        let path = coords::original_heightmap_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) if data.len() == defaults::HEIGHTMAP_SIZE => Ok(Some(data)),
            Ok(data) => {
                warn!(
                    "Original heightmap {:?} has wrong size {} (expected {}), ignoring",
                    path,
                    data.len(),
                    defaults::HEIGHTMAP_SIZE
                );
                Ok(None)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write original (pre-housing) heightmap.
    pub async fn write_original_heightmap(
        &self,
        tx: i32,
        tz: i32,
        data: &[u8],
    ) -> std::io::Result<()> {
        if data.len() != defaults::HEIGHTMAP_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Original heightmap: expected {} bytes, got {}",
                    defaults::HEIGHTMAP_SIZE,
                    data.len()
                ),
            ));
        }
        let path = coords::original_heightmap_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    /// Read pre-computed tree placement data (variable-length binary).
    /// Returns None if the file does not exist.
    pub async fn read_trees(&self, tx: i32, tz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let path = coords::tree_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write pre-computed tree placement data (variable-length binary).
    pub async fn write_trees(&self, tx: i32, tz: i32, data: &[u8]) -> std::io::Result<()> {
        let path = coords::tree_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    /// Read original (pre-housing) grass placement data. Returns None if not found.
    pub async fn read_original_grass(&self, tx: i32, tz: i32) -> std::io::Result<Option<Vec<u8>>> {
        let path = coords::original_grass_path(&self.base_dir, tx, tz);
        match fs::read(&path).await {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Write original (pre-housing) grass placement data.
    pub async fn write_original_grass(&self, tx: i32, tz: i32, data: &[u8]) -> std::io::Result<()> {
        let path = coords::original_grass_path(&self.base_dir, tx, tz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&path, data).await
    }

    /// Copy current heightmap → original heightmap if original doesn't exist yet.
    /// No-op if original already exists. Returns true if a copy was made.
    pub async fn ensure_original_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<bool> {
        let orig_path = coords::original_heightmap_path(&self.base_dir, tx, tz);
        match fs::metadata(&orig_path).await {
            Ok(_) => return Ok(false), // already exists
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let data = self.read_heightmap(tx, tz).await?;
        self.write_original_heightmap(tx, tz, &data).await?;
        Ok(true)
    }

    /// Copy current grass → original grass if original doesn't exist yet.
    /// No-op if original already exists. Returns true if a copy was made.
    pub async fn ensure_original_grass(&self, tx: i32, tz: i32) -> std::io::Result<bool> {
        let orig_path = coords::original_grass_path(&self.base_dir, tx, tz);
        match fs::metadata(&orig_path).await {
            Ok(_) => return Ok(false), // already exists
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let data = match self.read_grass(tx, tz).await? {
            Some(d) => d,
            None => return Ok(false), // no grass data to snapshot
        };
        self.write_original_grass(tx, tz, &data).await?;
        Ok(true)
    }

    pub async fn delete_region(&self, rx: i32, rz: i32) -> std::io::Result<()> {
        let height_dir = coords::height_region_dir(&self.base_dir, rx, rz);
        let splat_dir = coords::splat_region_dir(&self.base_dir, rx, rz);
        let grass_dir = coords::grass_region_dir(&self.base_dir, rx, rz);
        let tree_dir = coords::tree_region_dir(&self.base_dir, rx, rz);
        let orig_height_dir = coords::original_height_region_dir(&self.base_dir, rx, rz);
        let orig_grass_dir = coords::original_grass_region_dir(&self.base_dir, rx, rz);
        let minimap_file = coords::minimap_path(&self.base_dir, rx, rz);

        for dir in [
            &height_dir,
            &splat_dir,
            &grass_dir,
            &tree_dir,
            &orig_height_dir,
            &orig_grass_dir,
        ] {
            match fs::remove_dir_all(dir).await {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        match fs::remove_file(&minimap_file).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        Ok(())
    }

    /// List all region coordinates that have zone files.
    pub async fn list_zone_regions(&self) -> std::io::Result<Vec<(i32, i32)>> {
        let zones_dir = self.base_dir.join("zones");
        let mut entries = match fs::read_dir(&zones_dir).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let mut regions = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // Parse "r+00_+00.json" pattern
            if let Some(stem) = name.strip_suffix(".json") {
                if let Some(rest) = stem.strip_prefix('r') {
                    if let Some((rx_str, rz_str)) = rest.split_once('_') {
                        if let (Ok(rx), Ok(rz)) = (rx_str.parse::<i32>(), rz_str.parse::<i32>()) {
                            regions.push((rx, rz));
                        }
                    }
                }
            }
        }
        Ok(regions)
    }

    /// Read zone data for a region. Returns empty JSON object if file not found.
    pub async fn read_zone(&self, rx: i32, rz: i32) -> std::io::Result<serde_json::Value> {
        let path = coords::zone_path(&self.base_dir, rx, rz);
        match fs::read_to_string(&path).await {
            Ok(json_str) => serde_json::from_str(&json_str)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(serde_json::Value::Object(Default::default()))
            }
            Err(e) => Err(e),
        }
    }

    /// Write zone data for a region.
    pub async fn write_zone(
        &self,
        rx: i32,
        rz: i32,
        json: &serde_json::Value,
    ) -> std::io::Result<()> {
        let path = coords::zone_path(&self.base_dir, rx, rz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json_str = serde_json::to_string_pretty(json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json_str).await
    }

    /// Read furniture data for a region. Returns empty JSON object if file not found.
    pub async fn read_furniture(&self, rx: i32, rz: i32) -> std::io::Result<serde_json::Value> {
        let path = coords::furniture_path(&self.base_dir, rx, rz);
        match fs::read_to_string(&path).await {
            Ok(json_str) => serde_json::from_str(&json_str)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Ok(serde_json::Value::Object(Default::default()))
            }
            Err(e) => Err(e),
        }
    }

    /// Write furniture data for a region.
    pub async fn write_furniture(
        &self,
        rx: i32,
        rz: i32,
        json: &serde_json::Value,
    ) -> std::io::Result<()> {
        let path = coords::furniture_path(&self.base_dir, rx, rz);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json_str = serde_json::to_string_pretty(json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json_str).await
    }
}
