use std::path::Path;

use crate::coords;
use crate::defaults;

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
fn heightmap_path_positive() {
    let p = coords::heightmap_path(Path::new("terrain"), 5, 3);
    assert_eq!(
        p.to_str().unwrap(),
        "terrain/height/r+00_+00/h_+0005_+0003.bin"
    );
}

#[test]
fn heightmap_path_negative() {
    let p = coords::heightmap_path(Path::new("terrain"), -5, -20);
    assert_eq!(
        p.to_str().unwrap(),
        "terrain/height/r-01_-02/h_-0005_-0020.bin"
    );
}

#[test]
fn splatmap_path_format() {
    let p = coords::splatmap_path(Path::new("t"), 0, 0);
    assert_eq!(p.to_str().unwrap(), "t/splat/r+00_+00/s_+0000_+0000.bin");
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
    let mut sampler = crate::height::HeightSampler::new(terrain_io);

    let h = sampler.sample_height(0.0, 0.0).await.unwrap();
    assert!((h - 0.0).abs() < 0.001, "Expected sea level, got {h}");

    let h2 = sampler.sample_height(10.5, -5.3).await.unwrap();
    assert!((h2 - 0.0).abs() < 0.001, "Expected sea level, got {h2}");
}
