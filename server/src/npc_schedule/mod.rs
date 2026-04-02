pub mod routes;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleEntry {
    pub at: String,
    pub pos: [f32; 3],
    #[serde(default)]
    pub rotation: f32,
    #[serde(default)]
    pub floor_level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub waypoints: Vec<[f32; 3]>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScheduleFile {
    pub schedule: Vec<ScheduleEntry>,
}

pub struct NpcIO {
    base_dir: PathBuf,
}

impl NpcIO {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// List NPC names (subdirectories that contain a schedule.toml).
    pub async fn list_npcs(&self) -> std::io::Result<Vec<String>> {
        let mut names = Vec::new();
        let mut entries = fs::read_dir(&self.base_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_type().await?.is_dir() {
                let schedule_path = entry.path().join("schedule.toml");
                if fs::try_exists(&schedule_path).await.unwrap_or(false) {
                    if let Some(name) = entry.file_name().to_str() {
                        names.push(name.to_string());
                    }
                }
            }
        }
        names.sort();
        Ok(names)
    }

    fn validate_name(name: &str) -> std::io::Result<()> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid NPC name",
            ));
        }
        Ok(())
    }

    /// Read and parse a schedule.toml for the given NPC.
    pub async fn read_schedule(&self, name: &str) -> std::io::Result<ScheduleFile> {
        Self::validate_name(name)?;
        let path = self.base_dir.join(name).join("schedule.toml");
        let content = fs::read_to_string(&path).await?;
        let schedule: ScheduleFile = toml::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(schedule)
    }

    /// Write schedule data back as TOML.
    pub async fn write_schedule(&self, name: &str, data: &ScheduleFile) -> std::io::Result<()> {
        Self::validate_name(name)?;
        let path = self.base_dir.join(name).join("schedule.toml");
        let content = toml::to_string_pretty(data)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(&path, content).await
    }
}
