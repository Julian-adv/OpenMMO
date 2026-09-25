use crate::io::{atomic_write_sync, TerrainIO};
use onlinerpg_shared::grass_format::{grass_density, GRASS_FILE_BYTES, GRASS_V4_MAGIC};
use std::{
    fs,
    io::{self, Read},
    path::Path,
};

#[derive(Default, Debug)]
pub struct GrassMigration {
    pub converted: usize,
    pub before_bytes: u64,
    pub after_bytes: u64,
}

impl TerrainIO {
    pub async fn migrate_grass_files(&self) -> io::Result<GrassMigration> {
        let base = self.base_dir().clone();
        tokio::task::spawn_blocking(move || migrate(&base))
            .await
            .map_err(io::Error::other)?
    }
}

fn migrate(base: &Path) -> io::Result<GrassMigration> {
    let mut stats = GrassMigration::default();
    for kind in ["grass", "grass-original"] {
        let regions = match fs::read_dir(base.join(kind)) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for region in regions {
            let region = region?;
            if !region.file_type()?.is_dir() {
                continue;
            }
            for file in fs::read_dir(region.path())? {
                let file = file?;
                let path = file.path();
                if !file.file_type()?.is_file() || path.extension().is_none_or(|ext| ext != "bin") {
                    continue;
                }
                let result = migrate_file(&path);
                let Some(before_bytes) = result.map_err(|error| {
                    io::Error::new(error.kind(), format!("{}: {error}", path.display()))
                })?
                else {
                    continue;
                };
                stats.converted += 1;
                stats.before_bytes += before_bytes;
                stats.after_bytes += GRASS_FILE_BYTES as u64;
            }
        }
    }
    Ok(stats)
}

fn migrate_file(path: &Path) -> io::Result<Option<u64>> {
    let mut file = fs::File::open(path)?;
    let mut magic = [0; 4];
    file.read_exact(&mut magic)?;
    if magic == GRASS_V4_MAGIC.to_le_bytes() && file.metadata()?.len() == GRASS_FILE_BYTES as u64 {
        return Ok(None);
    }
    let mut data = magic.to_vec();
    file.read_to_end(&mut data)?;
    let density = grass_density(&data)?;
    atomic_write_sync(path, &density)?;
    Ok(Some(data.len() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::grass_format::GRASS_V3_MAGIC;

    #[tokio::test]
    async fn migrates_current_and_restore_files_once_without_losing_counts() {
        let dir = std::env::temp_dir().join(format!("grass_migration_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let terrain = TerrainIO::new(dir.clone());
        let mut legacy = GRASS_V3_MAGIC.to_le_bytes().to_vec();
        legacy.extend_from_slice(&1u32.to_le_bytes());
        legacy.extend_from_slice(&[0; 14]);
        for kind in ["grass", "grass-original"] {
            let region = dir.join(kind).join("r+00_+00");
            fs::create_dir_all(&region).unwrap();
            fs::write(region.join("g_+0000_+0000.bin"), &legacy).unwrap();
        }
        let stats = terrain.migrate_grass_files().await.unwrap();
        assert_eq!(stats.converted, 2);
        let current = terrain.read_grass(0, 0).await.unwrap().unwrap();
        assert_eq!(current[4], 1);
        assert_eq!(
            current,
            terrain.read_original_grass(0, 0).await.unwrap().unwrap()
        );
        assert_eq!(terrain.migrate_grass_files().await.unwrap().converted, 0);
        let invalid = dir.join("grass/r+00_+00/g_+0001_+0000.bin");
        fs::write(&invalid, b"invalid").unwrap();
        assert!(terrain.migrate_grass_files().await.is_err());
        assert_eq!(fs::read(&invalid).unwrap(), b"invalid");
        fs::remove_dir_all(dir).unwrap();
    }
}
