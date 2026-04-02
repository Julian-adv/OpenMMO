use std::path::{Path, PathBuf};

/// Convert tile coordinate to region coordinate (floor division).
/// Region = 16x16 tiles. Negative coords round toward negative infinity.
pub fn tile_to_region(tile: i32) -> i32 {
    tile.div_euclid(16)
}

/// Format region directory name: "r+00_+00", "r-01_+02"
fn region_dir_name(rx: i32, rz: i32) -> String {
    format!("r{:+03}_{:+03}", rx, rz)
}

/// Build filesystem path for a heightmap tile file.
pub fn heightmap_path(base: &Path, tx: i32, tz: i32) -> PathBuf {
    let (rx, rz) = (tile_to_region(tx), tile_to_region(tz));
    height_region_dir(base, rx, rz).join(format!("h_{:+05}_{:+05}.bin", tx, tz))
}

/// Build filesystem path for a splatmap tile file.
pub fn splatmap_path(base: &Path, tx: i32, tz: i32) -> PathBuf {
    let (rx, rz) = (tile_to_region(tx), tile_to_region(tz));
    splat_region_dir(base, rx, rz).join(format!("s_{:+05}_{:+05}.bin", tx, tz))
}

/// Build filesystem path for a region metadata JSON file.
pub fn meta_path(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("meta")
        .join(format!("r{:+03}_{:+03}.json", rx, rz))
}

/// Build filesystem path for a region's height tile directory.
pub fn height_region_dir(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("height").join(region_dir_name(rx, rz))
}

/// Build filesystem path for a region's splat tile directory.
pub fn splat_region_dir(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("splat").join(region_dir_name(rx, rz))
}

/// Build filesystem path for a grass placement data file.
pub fn grass_path(base: &Path, tx: i32, tz: i32) -> PathBuf {
    let (rx, rz) = (tile_to_region(tx), tile_to_region(tz));
    grass_region_dir(base, rx, rz).join(format!("g_{:+05}_{:+05}.bin", tx, tz))
}

/// Build filesystem path for a region's grass tile directory.
pub fn grass_region_dir(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("grass").join(region_dir_name(rx, rz))
}

/// Build filesystem path for an original (pre-housing) heightmap tile file.
pub fn original_heightmap_path(base: &Path, tx: i32, tz: i32) -> PathBuf {
    let (rx, rz) = (tile_to_region(tx), tile_to_region(tz));
    original_height_region_dir(base, rx, rz).join(format!("o_{:+05}_{:+05}.bin", tx, tz))
}

/// Build filesystem path for a region's original height tile directory.
pub fn original_height_region_dir(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("height-original").join(region_dir_name(rx, rz))
}

/// Build filesystem path for an original (pre-housing) grass placement data file.
pub fn original_grass_path(base: &Path, tx: i32, tz: i32) -> PathBuf {
    let (rx, rz) = (tile_to_region(tx), tile_to_region(tz));
    original_grass_region_dir(base, rx, rz).join(format!("g_{:+05}_{:+05}.bin", tx, tz))
}

/// Build filesystem path for a region's original grass tile directory.
pub fn original_grass_region_dir(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("grass-original").join(region_dir_name(rx, rz))
}

/// Build filesystem path for a region zone JSON file.
pub fn zone_path(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("zones")
        .join(format!("r{:+03}_{:+03}.json", rx, rz))
}

/// Build filesystem path for a region furniture JSON file.
pub fn furniture_path(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("furniture")
        .join(format!("r{:+03}_{:+03}.json", rx, rz))
}

/// Build filesystem path for a region minimap PNG file.
pub fn minimap_path(base: &Path, rx: i32, rz: i32) -> PathBuf {
    base.join("minimap")
        .join(format!("r{:+03}_{:+03}.png", rx, rz))
}
