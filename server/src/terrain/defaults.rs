pub const TILE_DIM: usize = 64;

/// Heightmap: 64x64 uint16 = 8,192 bytes
pub const HEIGHTMAP_SIZE: usize = TILE_DIM * TILE_DIM * 2;

/// Splatmap: 64x64 RGBA uint8 = 16,384 bytes
pub const SPLATMAP_SIZE: usize = TILE_DIM * TILE_DIM * 4;

/// uint16 value for sea level (0.0m): 10000 * 0.05 - 500.0 = 0.0
pub const DEFAULT_HEIGHT_VALUE: u16 = 10000;

/// Generate a flat heightmap at sea level (all cells = 10000).
pub fn default_heightmap() -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEIGHTMAP_SIZE);
    let bytes = DEFAULT_HEIGHT_VALUE.to_le_bytes();
    for _ in 0..(TILE_DIM * TILE_DIM) {
        buf.extend_from_slice(&bytes);
    }
    buf
}

/// Generate a default splatmap (100% first layer: R=255, G=B=A=0).
pub fn default_splatmap() -> Vec<u8> {
    let mut buf = vec![0u8; SPLATMAP_SIZE];
    for i in 0..(TILE_DIM * TILE_DIM) {
        buf[i * 4] = 255; // R channel
    }
    buf
}

/// Default region metadata matching MAP_DESIGN.md specification.
pub fn default_meta_json() -> serde_json::Value {
    serde_json::json!({
      "layers": [
        { "texture": "rocky_terrain_02_1k", "tileScale": 8.0 },
        { "texture": "gravel_floor_1k", "tileScale": 6.0 },
        { "texture": "red_laterite_soil_stones_1k", "tileScale": 10.0 },
        { "texture": "snow_02_1k", "tileScale": 4.0 }
      ]
    })
}
