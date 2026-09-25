use crate::{
    coords, defaults,
    io::{write_terrain_file, TerrainIO},
};
use onlinerpg_shared::{
    terrain_files::{TerrainFile, TerrainFiles},
    ServerMessage,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

const MANIFEST_FORMAT: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct SourceStamp {
    size: u64,
    seconds: u64,
    nanos: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ManifestEntry {
    format: u32,
    x: i32,
    z: i32,
    files: TerrainFiles,
    sources: [Option<SourceStamp>; 5],
}

impl ManifestEntry {
    fn message(&self) -> ServerMessage {
        ServerMessage::TerrainTileVersion {
            tile_x: self.x,
            tile_z: self.z,
            files: self.files.clone(),
        }
    }
}

pub fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn content_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn tile_paths(base: &Path, x: i32, z: i32) -> [PathBuf; 5] {
    [
        coords::heightmap_path(base, x, z),
        coords::splatmap_path(base, x, z),
        coords::tree_path(base, x, z),
        coords::grass_path(base, x, z),
        coords::landscaping_path(base, x, z),
    ]
}

pub fn raw_file_path(base: &Path, relative: &str) -> io::Result<PathBuf> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Invalid terrain file path");
    let parts: Vec<_> = relative.split('/').collect();
    if parts.len() != 3 {
        return Err(invalid());
    }
    let index = match parts[0] {
        "height" => 0,
        "splat" => 1,
        "trees" => 2,
        "grass" => 3,
        "landscaping" => 4,
        _ => return Err(invalid()),
    };
    let (_, coordinates) = parts[2]
        .strip_suffix(".bin")
        .and_then(|s| s.split_once('_'))
        .ok_or_else(invalid)?;
    let (x, z) = coordinates.split_once('_').ok_or_else(invalid)?;
    let x: i32 = x.parse().map_err(|_| invalid())?;
    let z: i32 = z.parse().map_err(|_| invalid())?;
    if !(coords::WORLD_MIN_TILE_X..coords::WORLD_MAX_TILE_X).contains(&x) {
        return Err(invalid());
    }
    let expected = tile_paths(Path::new(""), x, z)[index].clone();
    if expected.to_string_lossy().replace('\\', "/") != relative {
        return Err(invalid());
    }
    Ok(base.join(expected))
}

async fn stamp(path: &Path) -> io::Result<Option<SourceStamp>> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.is_file() => {
            let time = metadata
                .modified()?
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?;
            Ok(Some(SourceStamp {
                size: metadata.len(),
                seconds: time.as_secs(),
                nanos: time.subsec_nanos(),
            }))
        }
        Ok(_) => Err(io::Error::other(format!(
            "Not a terrain file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

impl TerrainIO {
    pub fn manifest_dir(&self) -> PathBuf {
        self.base_dir().join("manifests")
    }

    fn manifest_path(&self, x: i32, z: i32) -> PathBuf {
        self.manifest_dir()
            .join(x.to_string())
            .join(format!("{z}.json"))
    }

    async fn source_stamps(&self, x: i32, z: i32) -> io::Result<[Option<SourceStamp>; 5]> {
        let mut sources = std::array::from_fn(|_| None);
        for (source, path) in sources.iter_mut().zip(tile_paths(self.base_dir(), x, z)) {
            *source = stamp(&path).await?;
        }
        Ok(sources)
    }

    pub async fn tile_manifest(&self, x: i32, z: i32) -> io::Result<ServerMessage> {
        self.ensure_manifest(x, z, false).await
    }

    pub async fn rebuild_manifest(&self, x: i32, z: i32) -> io::Result<ServerMessage> {
        self.ensure_manifest(x, z, true).await
    }

    async fn ensure_manifest(&self, x: i32, z: i32, rebuild: bool) -> io::Result<ServerMessage> {
        let x = coords::wrap_tile_x(x);
        let mut cache = self.manifests.lock().await;
        if !rebuild {
            if let Some(entry) = cache.get(&(x, z)) {
                return Ok(entry.message());
            }
        }
        cache.remove(&(x, z));
        let sources = self.source_stamps(x, z).await?;
        let index_path = self.manifest_path(x, z);
        if !rebuild {
            let saved = match tokio::fs::read(&index_path).await {
                Ok(bytes) => serde_json::from_slice::<ManifestEntry>(&bytes).ok(),
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error),
            };
            if let Some(entry) = saved {
                let files = [
                    &entry.files.height,
                    &entry.files.splat,
                    &entry.files.trees,
                    &entry.files.grass,
                    &entry.files.landscape,
                ];
                let valid = files
                    .iter()
                    .zip(tile_paths(self.base_dir(), x, z))
                    .enumerate()
                    .all(|(index, (file, path))| {
                        let present = sources[index].as_ref().is_some_and(|source| match index {
                            0 => source.size == defaults::HEIGHTMAP_SIZE as u64,
                            1 => source.size == defaults::SPLATMAP_SIZE as u64,
                            _ => true,
                        });
                        file.is_some() == present
                            && file.as_ref().is_none_or(|file| {
                                valid_hash(&file.hash)
                                    && raw_file_path(self.base_dir(), &file.path).ok() == Some(path)
                            })
                    });
                if entry.format == MANIFEST_FORMAT
                    && entry.x == x
                    && entry.z == z
                    && entry.sources == sources
                    && valid
                {
                    let message = entry.message();
                    cache.insert((x, z), entry);
                    return Ok(message);
                }
            }
        }
        let mut files = TerrainFiles::default();
        let outputs = [
            &mut files.height,
            &mut files.splat,
            &mut files.trees,
            &mut files.grass,
            &mut files.landscape,
        ];
        for (index, (output, path)) in outputs
            .into_iter()
            .zip(tile_paths(self.base_dir(), x, z))
            .enumerate()
        {
            if sources[index].is_none() {
                continue;
            }
            let bytes = tokio::fs::read(&path).await?;
            if (index == 0 && bytes.len() != defaults::HEIGHTMAP_SIZE)
                || (index == 1 && bytes.len() != defaults::SPLATMAP_SIZE)
            {
                tracing::warn!(
                    ?path,
                    "Invalid terrain size; clients will use the default tile"
                );
                continue;
            }
            if index == 3 {
                onlinerpg_shared::grass_format::grass_density(&bytes)?;
            }
            if index == 4 {
                crate::landscaping::decode_landscaping(&bytes, x, z)?;
            }
            *output = Some(TerrainFile {
                path: path
                    .strip_prefix(self.base_dir())
                    .map_err(io::Error::other)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                hash: content_hash(&bytes),
            });
        }
        if self.source_stamps(x, z).await? != sources {
            return Err(io::Error::other(
                "Terrain changed while preparing a manifest; retry",
            ));
        }
        let entry = ManifestEntry {
            format: MANIFEST_FORMAT,
            x,
            z,
            files,
            sources,
        };
        write_terrain_file(&index_path, &serde_json::to_vec(&entry)?).await?;
        let message = entry.message();
        cache.insert((x, z), entry);
        Ok(message)
    }

    pub async fn prepare_manifests(&self) -> io::Result<usize> {
        let base = self.base_dir().clone();
        let tiles = tokio::task::spawn_blocking(move || discover_tiles(&base))
            .await
            .map_err(io::Error::other)??;
        for &(x, z) in &tiles {
            self.tile_manifest(x, z).await?;
        }
        Ok(tiles.len())
    }
}
fn entries(path: &Path) -> io::Result<Vec<std::fs::DirEntry>> {
    match std::fs::read_dir(path) {
        Ok(entries) => entries.collect(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error),
    }
}

fn discover_tiles(base: &Path) -> io::Result<BTreeSet<(i32, i32)>> {
    let mut tiles = BTreeSet::new();
    for (kind, prefix) in [
        ("height", "h_"),
        ("splat", "s_"),
        ("trees", "t_"),
        ("grass", "g_"),
        ("landscaping", "l_"),
    ] {
        for region in entries(&base.join(kind))? {
            if !region.file_type()?.is_dir() {
                continue;
            }
            for file in entries(&region.path())? {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let Some(coords) = name
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(".bin"))
                else {
                    continue;
                };
                let (x, z) = coords
                    .split_once('_')
                    .and_then(|(x, z)| Some((x.parse::<i32>().ok()?, z.parse::<i32>().ok()?)))
                    .ok_or_else(|| io::Error::other(format!("Invalid terrain tile: {name}")))?;
                tiles.insert((coords::wrap_tile_x(x), z));
            }
        }
    }
    for column in entries(&base.join("manifests"))? {
        if !column.file_type()?.is_dir() {
            continue;
        }
        let Ok(x) = column.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        for file in entries(&column.path())? {
            if let Some(z) = file
                .path()
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<i32>().ok())
            {
                tiles.insert((coords::wrap_tile_x(x), z));
            }
        }
    }
    Ok(tiles)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::grass_format::{empty_grass, GRASS_V3_MAGIC};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn terrain() -> TerrainIO {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        TerrainIO::new(std::env::temp_dir().join(format!(
            "terrain_manifests_{}_{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }

    fn files(message: ServerMessage) -> TerrainFiles {
        let ServerMessage::TerrainTileVersion { files, .. } = message else {
            panic!()
        };
        files
    }

    #[tokio::test]
    async fn runtime_lookup_only_reads_the_requested_tile_and_creates_no_payloads() {
        let io = terrain();
        let unrelated = coords::heightmap_path(io.base_dir(), 100, 100);
        tokio::fs::create_dir_all(unrelated).await.unwrap();
        assert_eq!(
            files(io.tile_manifest(0, 0).await.unwrap()),
            TerrainFiles::default()
        );
        assert!(io.manifest_path(0, 0).is_file());
        assert!(!io.base_dir().join("snapshots").exists());
        assert!(!io.manifest_path(100, 100).exists());
        assert_eq!(io.manifests.lock().await.len(), 1);
        assert!(io.prepare_manifests().await.is_err());
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn hashes_original_bytes_including_legacy_grass_without_filtering_or_rewriting() {
        let io = terrain();
        let mut grass = GRASS_V3_MAGIC.to_le_bytes().to_vec();
        grass.extend_from_slice(&1u32.to_le_bytes());
        grass.extend_from_slice(&[0; 14]);
        let path = coords::grass_path(io.base_dir(), -1, -1);
        write_terrain_file(&path, &grass).await.unwrap();
        let landscape = onlinerpg_shared::landscaping::LandscapingTile {
            tile_x: -1,
            tile_z: -1,
            splat: defaults::default_splatmap(),
            cleared: vec![255; 512],
        };
        io.write_landscaping_tile(&landscape).await.unwrap();
        let first = files(io.tile_manifest(511, -1).await.unwrap());
        let file = first.grass.unwrap();
        assert_eq!(file.path, "grass/r-01_-01/g_-0001_-0001.bin");
        assert_eq!(file.hash, content_hash(&grass));
        assert_eq!(tokio::fs::read(path).await.unwrap(), grass);
        assert_ne!(
            content_hash(&io.read_grass(-1, -1).await.unwrap().unwrap()),
            file.hash
        );
        let land = first.landscape.unwrap();
        assert_eq!(
            land.hash,
            content_hash(
                &tokio::fs::read(io.base_dir().join(land.path))
                    .await
                    .unwrap()
            )
        );
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn persisted_hashes_survive_restart_and_only_changed_files_change_version() {
        let io = terrain();
        io.write_heightmap(0, 0, &defaults::default_heightmap())
            .await
            .unwrap();
        let mut grass = empty_grass();
        grass[4] = 255;
        io.write_grass(0, 0, &grass).await.unwrap();
        io.write_splatmap(1, 0, &defaults::default_splatmap())
            .await
            .unwrap();
        assert_eq!(io.prepare_manifests().await.unwrap(), 2);
        let first = files(io.tile_manifest(0, 0).await.unwrap());
        let other = files(io.tile_manifest(1, 0).await.unwrap());
        let index = io.manifest_path(0, 0);
        let before = stamp(&index).await.unwrap();
        let restarted = TerrainIO::new(io.base_dir().clone());
        assert_eq!(restarted.prepare_manifests().await.unwrap(), 2);
        assert_eq!(stamp(&index).await.unwrap(), before);
        assert_eq!(files(restarted.tile_manifest(0, 0).await.unwrap()), first);
        grass[4] = 128;
        io.write_grass(0, 0, &grass).await.unwrap();
        let changed = files(io.rebuild_manifest(0, 0).await.unwrap());
        assert_ne!(first.grass, changed.grass);
        assert_eq!(first.height, changed.height);
        assert_eq!(files(io.tile_manifest(1, 0).await.unwrap()), other);
        io.delete_region(0, 0).await.unwrap();
        let restarted = TerrainIO::new(io.base_dir().clone());
        assert_eq!(restarted.prepare_manifests().await.unwrap(), 2);
        assert_eq!(
            files(restarted.tile_manifest(0, 0).await.unwrap()),
            TerrainFiles::default()
        );
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[tokio::test]
    async fn cache_hits_do_not_read_sources_and_failed_updates_do_not_replace_the_index() {
        let io = terrain();
        let original = files(io.tile_manifest(0, 0).await.unwrap());
        let path = coords::heightmap_path(io.base_dir(), 0, 0);
        tokio::fs::create_dir_all(&path).await.unwrap();
        assert_eq!(files(io.tile_manifest(0, 0).await.unwrap()), original);
        assert!(io.rebuild_manifest(0, 0).await.is_err());
        let saved: ManifestEntry =
            serde_json::from_slice(&tokio::fs::read(io.manifest_path(0, 0)).await.unwrap())
                .unwrap();
        assert_eq!(saved.files, original);
        tokio::fs::remove_dir(&path).await.unwrap();
        write_terrain_file(&path, b"invalid heightmap")
            .await
            .unwrap();
        assert_eq!(files(io.rebuild_manifest(0, 0).await.unwrap()).height, None);
        tokio::fs::remove_dir_all(io.base_dir()).await.unwrap();
    }

    #[test]
    fn public_paths_only_address_canonical_current_terrain_files() {
        let base = Path::new("terrain");
        assert!(raw_file_path(base, "height/r-16_-01/h_-0256_-0001.bin").is_ok());
        for path in [
            "../secrets",
            "height/../Cargo.toml",
            "height-original/r+00_+00/o_+0000_+0000.bin",
            "grass/r+01_+00/g_+0000_+0000.bin",
            "grass/r+00_+00/h_+0000_+0000.bin",
            "height/r+16_+00/h_+0256_+0000.bin",
            "height/r+00_+00/h_2147483647_0.bin",
            "height/r+00_+00/h_+0000_+0000.bin/extra",
        ] {
            assert!(raw_file_path(base, path).is_err(), "{path}");
        }
    }
}
