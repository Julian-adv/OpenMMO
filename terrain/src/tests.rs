use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::coords;
use crate::defaults;

static TEST_TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_dir(name: &str) -> std::path::PathBuf {
    let counter = TEST_TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "_onlinerpg_{name}_{}_{}",
        std::process::id(),
        counter
    ))
}

#[test]
fn tile_to_region_positive() {
    assert_eq!(coords::tile_to_region(0), 0);
    assert_eq!(coords::tile_to_region(15), 0);
    assert_eq!(coords::tile_to_region(16), 1);
    assert_eq!(coords::tile_to_region(249), 15);
}

#[test]
fn tile_to_region_negative() {
    assert_eq!(coords::tile_to_region(-1), -1);
    assert_eq!(coords::tile_to_region(-16), -1);
    assert_eq!(coords::tile_to_region(-17), -2);
    assert_eq!(coords::tile_to_region(-250), -16);
}

#[test]
fn tile_x_wraps_across_baked_file_range() {
    assert_eq!(coords::wrap_tile_x(-256), -256);
    assert_eq!(coords::wrap_tile_x(255), 255);
    assert_eq!(coords::wrap_tile_x(256), -256);
    assert_eq!(coords::wrap_tile_x(-257), 255);
    assert_eq!(coords::wrap_tile_x(768), -256);
}

#[test]
fn region_x_wraps_across_baked_file_range() {
    assert_eq!(coords::wrap_region_x(-16), -16);
    assert_eq!(coords::wrap_region_x(15), 15);
    assert_eq!(coords::wrap_region_x(16), -16);
    assert_eq!(coords::wrap_region_x(-17), 15);
}

#[test]
fn heightmap_path_positive() {
    let p = coords::heightmap_path(Path::new("terrain"), 5, 3);
    let expected: std::path::PathBuf = ["terrain", "height", "r+00_+00", "h_+0005_+0003.bin"]
        .iter()
        .collect();
    assert_eq!(p, expected);
}

#[test]
fn heightmap_path_negative() {
    let p = coords::heightmap_path(Path::new("terrain"), -5, -20);
    let expected: std::path::PathBuf = ["terrain", "height", "r-01_-02", "h_-0005_-0020.bin"]
        .iter()
        .collect();
    assert_eq!(p, expected);
}

#[test]
fn splatmap_path_format() {
    let p = coords::splatmap_path(Path::new("t"), 0, 0);
    let expected: std::path::PathBuf = ["t", "splat", "r+00_+00", "s_+0000_+0000.bin"]
        .iter()
        .collect();
    assert_eq!(p, expected);
}

#[test]
fn periodic_tile_paths_alias_opposite_world_edge() {
    let base = Path::new("terrain");
    assert_eq!(
        coords::heightmap_path(base, -257, 3),
        coords::heightmap_path(base, 255, 3)
    );
    assert_eq!(
        coords::splatmap_path(base, 256, -2),
        coords::splatmap_path(base, -256, -2)
    );
    assert_eq!(
        coords::grass_path(base, -257, 4),
        coords::grass_path(base, 255, 4)
    );
    assert_eq!(
        coords::tree_path(base, 256, 5),
        coords::tree_path(base, -256, 5)
    );
    assert_eq!(
        coords::river_field_path(base, -257, 6),
        coords::river_field_path(base, 255, 6)
    );
    assert_eq!(
        coords::water_field_path(base, 256, 7),
        coords::water_field_path(base, -256, 7)
    );
}

#[test]
fn periodic_minimap_path_aliases_opposite_world_edge() {
    let base = Path::new("terrain");
    assert_eq!(
        coords::minimap_path(base, -17, 0),
        coords::minimap_path(base, 15, 0)
    );
    assert_eq!(
        coords::minimap_path(base, 16, 0),
        coords::minimap_path(base, -16, 0)
    );
    assert_eq!(
        coords::fantasy_minimap_path(base, -17, 0),
        coords::fantasy_minimap_path(base, 15, 0)
    );
}

#[test]
fn fantasy_minimap_paths_use_fixed_root_and_lod() {
    let base = Path::new("terrain");
    assert_eq!(
        coords::fantasy_minimap_path(base, -2, 4),
        base.join("minimap-fantasy").join("r-02_+04.webp")
    );
    assert_eq!(
        coords::fantasy_minimap_lod_path(base, -2, 4, 256),
        base.join("minimap-fantasy")
            .join("256")
            .join("r-02_+04.webp")
    );
}

#[test]
fn default_heightmap_size() {
    assert_eq!(
        defaults::default_heightmap().len(),
        defaults::HEIGHTMAP_SIZE
    );
}

#[test]
fn default_heightmap_value() {
    let data = defaults::default_heightmap();
    let value = u16::from_le_bytes([data[0], data[1]]);
    assert_eq!(value, defaults::DEFAULT_HEIGHT_VALUE);
}

#[test]
fn default_splatmap_size() {
    assert_eq!(defaults::default_splatmap().len(), defaults::SPLATMAP_SIZE);
}

#[test]
fn default_splatmap_first_cell_is_slot0() {
    let data = defaults::default_splatmap();
    // V2: primaryIdx=0, secondaryIdx=0, blend=0, grassMeta=0 → 100% palette slot 0.
    assert_eq!(data[0], 0);
    assert_eq!(data[1], 0);
    assert_eq!(data[2], 0);
    assert_eq!(data[3], 0);
}

#[tokio::test]
async fn read_missing_heightmap_returns_default() {
    let io =
        crate::io::TerrainIO::new(std::path::PathBuf::from("/tmp/_onlinerpg_test_nonexistent"));
    let data = io.read_heightmap(999, 999).await.unwrap();
    assert_eq!(data.len(), defaults::HEIGHTMAP_SIZE);
    let value = u16::from_le_bytes([data[0], data[1]]);
    assert_eq!(value, defaults::DEFAULT_HEIGHT_VALUE);
}

#[tokio::test]
async fn read_missing_splatmap_returns_default() {
    let io =
        crate::io::TerrainIO::new(std::path::PathBuf::from("/tmp/_onlinerpg_test_nonexistent"));
    let data = io.read_splatmap(999, 999).await.unwrap();
    assert_eq!(data.len(), defaults::SPLATMAP_SIZE);
    assert_eq!(data[0], 0);
}

#[tokio::test]
async fn fantasy_minimap_takes_priority_per_requested_lod() {
    let dir = unique_temp_dir("fantasy_minimap_priority");
    let io = crate::io::TerrainIO::new(dir.clone());
    let rx = -2;
    let rz = 4;
    let files = [
        (
            coords::minimap_path(&dir, rx, rz),
            b"legacy-base".as_slice(),
        ),
        (
            coords::minimap_lod_path(&dir, rx, rz, 256),
            b"legacy-256".as_slice(),
        ),
        (
            coords::fantasy_minimap_path(&dir, rx, rz),
            b"fantasy-base".as_slice(),
        ),
        (
            coords::fantasy_minimap_lod_path(&dir, rx, rz, 256),
            b"fantasy-256".as_slice(),
        ),
    ];
    for (path, data) in files {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, data).await.unwrap();
    }

    assert_eq!(
        io.read_minimap(rx, rz).await.unwrap().unwrap(),
        b"fantasy-base"
    );
    assert_eq!(
        io.read_minimap_lod(rx, rz, 256).await.unwrap().unwrap(),
        b"fantasy-256"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn missing_fantasy_minimap_lod_falls_back_without_losing_requested_size() {
    let dir = unique_temp_dir("fantasy_minimap_fallback");
    let io = crate::io::TerrainIO::new(dir.clone());
    let rx = 3;
    let rz = -5;
    let fantasy_base = coords::fantasy_minimap_path(&dir, rx, rz);
    let legacy_base = coords::minimap_path(&dir, rx, rz);
    let legacy_lod = coords::minimap_lod_path(&dir, rx, rz, 128);
    for path in [&legacy_base, &legacy_lod] {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
    }
    tokio::fs::write(&legacy_base, b"legacy-base")
        .await
        .unwrap();
    tokio::fs::write(&legacy_lod, b"legacy-128").await.unwrap();

    assert_eq!(
        io.read_minimap(rx, rz).await.unwrap().unwrap(),
        b"legacy-base"
    );
    assert_eq!(
        io.read_minimap_lod(rx, rz, 128).await.unwrap().unwrap(),
        b"legacy-128"
    );
    tokio::fs::create_dir_all(fantasy_base.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&fantasy_base, b"fantasy-base")
        .await
        .unwrap();
    assert_eq!(
        io.read_minimap(rx, rz).await.unwrap().unwrap(),
        b"fantasy-base"
    );
    assert_eq!(
        io.read_minimap_lod(rx, rz, 128).await.unwrap().unwrap(),
        b"legacy-128"
    );
    tokio::fs::remove_file(&legacy_lod).await.unwrap();
    assert_eq!(
        io.read_minimap_lod(rx, rz, 128).await.unwrap().unwrap(),
        b"fantasy-base"
    );

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn heightmap_write_read_roundtrip() {
    let dir = std::env::temp_dir().join("_onlinerpg_test_roundtrip_h");
    let _ = tokio::fs::remove_dir_all(&dir).await;

    let io = crate::io::TerrainIO::new(dir.clone());
    let mut data = defaults::default_heightmap();
    // Set first cell to 6000 (= -200.0m)
    let custom: u16 = 6000;
    data[0] = custom.to_le_bytes()[0];
    data[1] = custom.to_le_bytes()[1];

    io.write_heightmap(0, 0, &data).await.unwrap();
    let read_back = io.read_heightmap(0, 0).await.unwrap();
    assert_eq!(read_back, data);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn heightmap_read_uses_periodic_x_tile_alias() {
    let dir = std::env::temp_dir().join("_onlinerpg_test_periodic_height_alias");
    let _ = tokio::fs::remove_dir_all(&dir).await;

    let io = crate::io::TerrainIO::new(dir.clone());
    let mut east_data = defaults::default_heightmap();
    east_data[0..2].copy_from_slice(&12_345u16.to_le_bytes());
    io.write_heightmap(255, 3, &east_data).await.unwrap();

    let west_render_copy = io.read_heightmap(-257, 3).await.unwrap();
    assert_eq!(west_render_copy, east_data);

    let mut west_data = defaults::default_heightmap();
    west_data[0..2].copy_from_slice(&23_456u16.to_le_bytes());
    io.write_heightmap(-256, 3, &west_data).await.unwrap();

    let east_render_copy = io.read_heightmap(256, 3).await.unwrap();
    assert_eq!(east_render_copy, west_data);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn splatmap_write_read_roundtrip() {
    let dir = std::env::temp_dir().join("_onlinerpg_test_roundtrip_s");
    let _ = tokio::fs::remove_dir_all(&dir).await;

    let io = crate::io::TerrainIO::new(dir.clone());
    let mut data = defaults::default_splatmap();
    // Paint second pixel to 100% snow (A channel)
    data[4] = 0;
    data[7] = 255;

    io.write_splatmap(0, 0, &data).await.unwrap();
    let read_back = io.read_splatmap(0, 0).await.unwrap();
    assert_eq!(read_back, data);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn atomic_write_replaces_file_only_after_success() {
    let dir = unique_temp_dir("atomic_success");
    let target = dir.join("nested").join("world.bin");
    let replacement = b"complete replacement";

    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();
    tokio::fs::write(&target, b"old complete file")
        .await
        .unwrap();
    crate::io::atomic_write(&target, replacement).await.unwrap();

    assert_eq!(tokio::fs::read(&target).await.unwrap(), replacement);
    assert_no_atomic_temp_files(target.parent().unwrap(), "world.bin").await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[cfg(unix)]
#[tokio::test]
async fn atomic_write_preserves_existing_unix_mode() {
    use std::os::unix::fs::PermissionsExt;

    let dir = unique_temp_dir("atomic_permissions");
    let target = dir.join("world.bin");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(&target, b"old").await.unwrap();
    tokio::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640))
        .await
        .unwrap();

    crate::io::atomic_write(&target, b"new").await.unwrap();

    let mode = tokio::fs::metadata(&target)
        .await
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o640);
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn injected_partial_atomic_write_failure_keeps_existing_target_unchanged() {
    let dir = unique_temp_dir("atomic_failure_existing");
    let target = dir.join("world.bin");
    let original = b"original bytes";
    let replacement = b"new bytes that must not reach final path";

    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(&target, original).await.unwrap();

    let result = crate::io::atomic_write_with_injected_failure(&target, replacement, 9);

    assert!(result.is_err());
    assert_eq!(tokio::fs::read(&target).await.unwrap(), original);
    assert_no_atomic_temp_files(&dir, "world.bin").await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn injected_partial_atomic_write_failure_does_not_create_final_path() {
    let dir = unique_temp_dir("atomic_failure_missing");
    let target = dir.join("world.bin");

    tokio::fs::create_dir_all(&dir).await.unwrap();
    let result = crate::io::atomic_write_with_injected_failure(&target, b"partial", 3);

    assert!(result.is_err());
    assert!(tokio::fs::metadata(&target).await.is_err());
    assert_no_atomic_temp_files(&dir, "world.bin").await;
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

async fn assert_no_atomic_temp_files(dir: &Path, file_name: &str) {
    let mut entries = tokio::fs::read_dir(dir).await.unwrap();
    let temp_prefix = format!(".{file_name}.");
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !(name.starts_with(&temp_prefix) && name.ends_with(".tmp")),
            "unexpected atomic temp file left behind: {name}"
        );
    }
}

#[tokio::test]
async fn write_invalid_size_returns_error() {
    let io =
        crate::io::TerrainIO::new(std::path::PathBuf::from("/tmp/_onlinerpg_test_nonexistent"));
    let bad_data = vec![0u8; 100];
    let result = io.write_heightmap(0, 0, &bad_data).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
}

#[tokio::test]
async fn height_sampler_flat_terrain() {
    // Default heightmap is all sea level (0.0m)
    let dir = std::path::PathBuf::from("/tmp/_onlinerpg_test_sampler_nonexistent");
    let terrain_io = crate::io::TerrainIO::new(dir);
    let sampler = crate::height::HeightSampler::new(terrain_io);

    let h = sampler.sample_height(0.0, 0.0).await.unwrap();
    assert!((h - 0.0).abs() < 0.001, "Expected sea level, got {h}");

    let h2 = sampler.sample_height(10.5, -5.3).await.unwrap();
    assert!((h2 - 0.0).abs() < 0.001, "Expected sea level, got {h2}");
}

async fn seed_minimap_files(dir: &std::path::Path, rx: i32, rz: i32) -> Vec<std::path::PathBuf> {
    let mut paths = vec![
        coords::minimap_path(dir, rx, rz),
        coords::fantasy_minimap_path(dir, rx, rz),
    ];
    for size in coords::MINIMAP_LOD_SIZES {
        paths.push(coords::minimap_lod_path(dir, rx, rz, size));
        paths.push(coords::fantasy_minimap_lod_path(dir, rx, rz, size));
    }
    for path in &paths {
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, b"stale").await.unwrap();
    }
    paths
}

#[tokio::test]
async fn write_minimap_drops_stale_fantasy_tiles() {
    let dir = unique_temp_dir("write_minimap_fantasy");
    let io = crate::io::TerrainIO::new(dir.clone());
    let (rx, rz) = (-2, 4);
    seed_minimap_files(&dir, rx, rz).await;

    io.write_minimap(rx, rz, b"fresh-legacy").await.unwrap();

    assert_eq!(
        io.read_minimap(rx, rz).await.unwrap().unwrap(),
        b"fresh-legacy"
    );
    assert_eq!(
        io.read_minimap_lod(rx, rz, 256).await.unwrap().unwrap(),
        b"fresh-legacy"
    );
    assert!(!coords::fantasy_minimap_path(&dir, rx, rz).exists());
    for size in coords::MINIMAP_LOD_SIZES {
        assert!(!coords::fantasy_minimap_lod_path(&dir, rx, rz, size).exists());
    }

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn delete_region_removes_fantasy_tiles() {
    let dir = unique_temp_dir("delete_region_fantasy");
    let io = crate::io::TerrainIO::new(dir.clone());
    let (rx, rz) = (3, -1);
    let paths = seed_minimap_files(&dir, rx, rz).await;

    io.delete_region(rx, rz).await.unwrap();

    for path in paths {
        assert!(!path.exists(), "{path:?} survived delete_region");
    }
    assert!(io.read_minimap(rx, rz).await.unwrap().is_none());

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn stat_minimap_lod_matches_read_preference() {
    let dir = unique_temp_dir("stat_minimap_pref");
    let io = crate::io::TerrainIO::new(dir.clone());
    let (rx, rz) = (1, 2);
    seed_minimap_files(&dir, rx, rz).await;

    let (tile, meta) = io.stat_minimap_lod(rx, rz, 256).await.unwrap().unwrap();
    assert_eq!(
        tile.path,
        coords::fantasy_minimap_lod_path(&dir, rx, rz, 256)
    );
    assert_eq!(tile.family, coords::MinimapFamily::Fantasy);
    assert_eq!(meta.len(), b"stale".len() as u64);

    tokio::fs::remove_file(&tile.path).await.unwrap();
    let (tile, _) = io.stat_minimap_lod(rx, rz, 256).await.unwrap().unwrap();
    assert_eq!(tile.path, coords::minimap_lod_path(&dir, rx, rz, 256));
    assert_eq!(tile.family, coords::MinimapFamily::Legacy);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[test]
fn climate_path_format() {
    let base = Path::new("/data/terrain");
    assert_eq!(
        coords::climate_path(base, -2, 4),
        base.join("climate").join("r-02_+04.bin")
    );
}

#[tokio::test]
async fn climate_write_read_roundtrip() {
    let dir = unique_temp_dir("climate_roundtrip");
    let io = crate::io::TerrainIO::new(dir.clone());
    let data: Vec<u8> = (0..crate::land::REGION_PLOTS)
        .map(|i| (i % 5) as u8)
        .collect();

    io.write_climate(-2, 4, &data).await.unwrap();
    assert_eq!(io.read_climate(-2, 4).await.unwrap(), Some(data));
    assert_eq!(io.read_climate(0, 0).await.unwrap(), None);

    let _ = tokio::fs::remove_dir_all(&dir).await;
}

#[tokio::test]
async fn climate_write_rejects_bad_size_and_bytes() {
    let io = crate::io::TerrainIO::new(unique_temp_dir("climate_invalid"));
    let short = vec![0u8; 100];
    assert_eq!(
        io.write_climate(0, 0, &short).await.unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    let mut bad_zone = vec![2u8; crate::land::REGION_PLOTS];
    bad_zone[7] = 9;
    assert_eq!(
        io.write_climate(0, 0, &bad_zone).await.unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
}

#[tokio::test]
async fn weather_sectors_read_missing_and_present() {
    let dir = unique_temp_dir("weather_sectors");
    let io = crate::io::TerrainIO::new(dir.clone());
    assert_eq!(io.read_weather_sectors().await.unwrap(), None);

    let ws = onlinerpg_shared::weather::WeatherSectors {
        version: onlinerpg_shared::weather::WEATHER_SECTORS_VERSION,
        seed: 42,
        sectors: vec![onlinerpg_shared::weather::Sector {
            zone: 1,
            spots: vec![[16.0, -48.0]],
        }],
    };
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(
        coords::weather_sectors_path(&dir),
        serde_json::to_vec(&ws).unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(io.read_weather_sectors().await.unwrap(), Some(ws));

    tokio::fs::write(coords::weather_sectors_path(&dir), b"not json")
        .await
        .unwrap();
    assert_eq!(
        io.read_weather_sectors().await.unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    let _ = tokio::fs::remove_dir_all(&dir).await;
}

/// Share of time a plot in each zone is under rain, from a real bake. Sectors
/// are placed here from the baked climate files so schedule constants can be
/// tuned without re-running the world sim:
/// `WEATHER_BAKE_DIR=<dir> cargo test -p onlinerpg-terrain --release weather_zone_shares -- --ignored --nocapture`
#[tokio::test]
#[ignore]
async fn weather_zone_shares_from_bake() {
    use onlinerpg_shared::weather::{cells_at, rain_at};
    use onlinerpg_shared::worldgen::weather_sectors::{place_sectors, ClimatePlotGrid};
    let dir =
        std::path::PathBuf::from(std::env::var("WEATHER_BAKE_DIR").expect("WEATHER_BAKE_DIR"));
    let io = crate::io::TerrainIO::new(dir.clone());
    let mut regions: Vec<(i32, i32, Vec<u8>)> = Vec::new();
    for entry in std::fs::read_dir(dir.join("climate")).unwrap() {
        let name = entry.unwrap().file_name().into_string().unwrap();
        let rx: i32 = name[1..4].parse().unwrap();
        let rz: i32 = name[5..8].parse().unwrap();
        regions.push((rx, rz, io.read_climate(rx, rz).await.unwrap().unwrap()));
    }
    let rx0 = regions.iter().map(|r| r.0).min().unwrap();
    let rz0 = regions.iter().map(|r| r.1).min().unwrap();
    let rx1 = regions.iter().map(|r| r.0).max().unwrap();
    let rz1 = regions.iter().map(|r| r.1).max().unwrap();
    let plot = crate::land::PLOT_SIZE as f32;
    let (w, h) = (
        ((rx1 - rx0 + 1) * 32) as usize,
        ((rz1 - rz0 + 1) * 32) as usize,
    );
    let (x0, z0) = crate::land::plot_origin(rx0, rz0, 0);
    let mut grid = ClimatePlotGrid {
        w,
        h,
        plot_m: plot,
        x0: x0 as f32,
        z0: z0 as f32,
        zones: vec![0; w * h],
    };
    let mut samples: Vec<(u8, f32, f32)> = Vec::new();
    for (rx, rz, zones) in &regions {
        for (i, &z) in zones.iter().enumerate() {
            let (x, zz) = crate::land::plot_origin(*rx, *rz, i);
            let gx = ((x - x0) / crate::land::PLOT_SIZE) as usize;
            let gz = ((zz - z0) / crate::land::PLOT_SIZE) as usize;
            grid.zones[gz * w + gx] = z;
            if z != 0 && i % 16 == 0 {
                samples.push((z, x as f32 + 16.0, zz as f32 + 16.0));
            }
        }
    }
    let sectors = place_sectors(&grid, 42);
    let mut per_zone = [0usize; 5];
    for s in &sectors.sectors {
        per_zone[s.zone as usize] += 1;
    }

    let mut wet = [0u64; 5];
    // wet time by the zone of the strongest contributing cell, per sample zone
    let mut source = [[0u64; 5]; 5];
    let mut total = [0u64; 5];
    let mut max_cells = 0;
    let days = 30.0;
    let mut t = 0.0;
    while t < days * 1440.0 {
        let cells = cells_at(&sectors.sectors, 42, t);
        max_cells = max_cells.max(cells.len());
        for &(zone, x, z) in &samples {
            total[zone as usize] += 1;
            if rain_at(&cells, x, z) > 0.35 {
                wet[zone as usize] += 1;
                let strongest = cells
                    .iter()
                    .max_by(|a, b| {
                        rain_at(std::slice::from_ref(a), x, z).total_cmp(&rain_at(
                            std::slice::from_ref(b),
                            x,
                            z,
                        ))
                    })
                    .unwrap();
                source[zone as usize][sectors.sectors[strongest.sector].zone as usize] += 1;
            }
        }
        t += 30.0;
    }
    let names = ["sea", "wetCoast", "temperate", "rainShadow", "alpine"];
    eprintln!(
        "sectors {:?} (max live cells {max_cells}), samples {}",
        &per_zone[1..],
        samples.len()
    );
    for (zone, name) in names.iter().enumerate().skip(1) {
        if total[zone] > 0 {
            let by: Vec<String> = (1..5)
                .map(|src| {
                    format!(
                        "{}={:.1}",
                        names[src],
                        100.0 * source[zone][src] as f64 / total[zone] as f64
                    )
                })
                .collect();
            eprintln!(
                "{name:<12} wet {:5.1} %  from {}",
                100.0 * wet[zone] as f64 / total[zone] as f64,
                by.join(" ")
            );
        }
    }
}
