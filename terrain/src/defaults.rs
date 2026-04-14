pub const TILE_DIM: usize = 64;
pub const VERTS_PER_SIDE: usize = TILE_DIM + 1; // 65

/// Heightmap: 65x65 uint16 = 8,450 bytes (vertex-based, adjacent tiles overlap by 1)
pub const HEIGHTMAP_SIZE: usize = VERTS_PER_SIDE * VERTS_PER_SIDE * 2;

/// Splatmap V2: 64x64 × 4 bytes = 16,384 bytes (cell-based).
/// Per cell: [primaryIdx, secondaryIdx, blend, grassMeta]. See doc/SPLATMAP_V2.md.
pub const SPLATMAP_SIZE: usize = TILE_DIM * TILE_DIM * 4;

/// uint16 value for sea level (0.0m): 10000 * 0.05 - 500.0 = 0.0
pub const DEFAULT_HEIGHT_VALUE: u16 = 10000;

/// Generate a flat heightmap at sea level (all vertices = 10000).
pub fn default_heightmap() -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEIGHTMAP_SIZE);
    let bytes = DEFAULT_HEIGHT_VALUE.to_le_bytes();
    for _ in 0..(VERTS_PER_SIDE * VERTS_PER_SIDE) {
        buf.extend_from_slice(&bytes);
    }
    buf
}

/// Generate a default splatmap: every cell = palette slot 0, no blend, no grass.
/// All bytes are already 0, so this is just a zero buffer.
pub fn default_splatmap() -> Vec<u8> {
    vec![0u8; SPLATMAP_SIZE]
}

/// Default region metadata — single-layer palette. Regions grow this as needed (up to 16).
pub fn default_meta_json() -> serde_json::Value {
    serde_json::json!({
      "layers": [
        { "texture": "rocky_terrain_02_1k", "tileScale": 8.0 }
      ]
    })
}
