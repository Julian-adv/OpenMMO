use std::collections::HashMap;

use crate::defaults::{self, VERTS_PER_SIDE};
use crate::io::TerrainIO;

/// Tile size in world units (must match client TERRAIN_TILE_SIZE).
const TILE_SIZE: f32 = defaults::TILE_DIM as f32;

/// Decode a uint16 heightmap value to meters.
/// Encoding: `round((meters + 500.0) / 0.05)` → range -500m to +3276m.
fn decode_height(value: u16) -> f32 {
    value as f32 * 0.05 - 500.0
}

/// Convert a world coordinate to the tile coordinate that contains it.
fn world_to_tile(world_coord: f32) -> i32 {
    ((world_coord + TILE_SIZE / 2.0) / TILE_SIZE).floor() as i32
}

/// Cache key for a tile.
fn tile_key(tx: i32, tz: i32) -> (i32, i32) {
    (tx, tz)
}

/// Provides terrain height sampling with an in-memory tile cache.
/// Loads heightmap tiles on demand via `TerrainIO` and caches them.
pub struct HeightSampler {
    cache: HashMap<(i32, i32), Vec<u16>>,
    terrain_io: TerrainIO,
}

impl HeightSampler {
    pub fn new(terrain_io: TerrainIO) -> Self {
        Self {
            cache: HashMap::new(),
            terrain_io,
        }
    }

    /// Ensure a tile's heightmap is loaded into the cache.
    async fn ensure_tile(&mut self, tx: i32, tz: i32) -> std::io::Result<()> {
        if self.cache.contains_key(&tile_key(tx, tz)) {
            return Ok(());
        }
        let raw = self.terrain_io.read_heightmap(tx, tz).await?;
        // Convert raw bytes to u16 array
        let heights: Vec<u16> = raw
            .chunks_exact(2)
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        self.cache.insert(tile_key(tx, tz), heights);
        Ok(())
    }

    /// Get height at a specific cell vertex. Handles cross-tile lookups.
    fn get_height_at_cell(&self, tx: i32, tz: i32, cell_x: i32, cell_z: i32) -> f32 {
        let (mut tx, mut tz, mut cx, mut cz) = (tx, tz, cell_x, cell_z);

        // Handle cross-tile boundary
        if cx >= VERTS_PER_SIDE as i32 {
            tx += 1;
            cx -= defaults::TILE_DIM as i32;
        } else if cx < 0 {
            tx -= 1;
            cx += defaults::TILE_DIM as i32;
        }
        if cz >= VERTS_PER_SIDE as i32 {
            tz += 1;
            cz -= defaults::TILE_DIM as i32;
        } else if cz < 0 {
            tz -= 1;
            cz += defaults::TILE_DIM as i32;
        }

        let Some(heights) = self.cache.get(&tile_key(tx, tz)) else {
            return 0.0;
        };
        let idx = cz as usize * VERTS_PER_SIDE + cx as usize;
        if idx < heights.len() {
            decode_height(heights[idx])
        } else {
            0.0
        }
    }

    /// Sample terrain height at an arbitrary world position using bilinear interpolation.
    /// Loads required tiles on demand.
    pub async fn sample_height(&mut self, world_x: f32, world_z: f32) -> std::io::Result<f32> {
        let tx = world_to_tile(world_x);
        let tz = world_to_tile(world_z);

        // Ensure the primary tile and potential neighbor tiles are loaded
        self.ensure_tile(tx, tz).await?;

        let tile_min_x = tx as f32 * TILE_SIZE - TILE_SIZE / 2.0;
        let tile_min_z = tz as f32 * TILE_SIZE - TILE_SIZE / 2.0;
        let local_x = world_x - tile_min_x;
        let local_z = world_z - tile_min_z;

        let cell_x = local_x.floor() as i32;
        let cell_z = local_z.floor() as i32;
        let frac_x = local_x - local_x.floor();
        let frac_z = local_z - local_z.floor();

        // Load neighbor tiles if we're at the edge and need cross-tile samples
        if cell_x + 1 >= VERTS_PER_SIDE as i32 {
            let _ = self.ensure_tile(tx + 1, tz).await;
        }
        if cell_z + 1 >= VERTS_PER_SIDE as i32 {
            let _ = self.ensure_tile(tx, tz + 1).await;
        }

        // Bilinear interpolation
        let h00 = self.get_height_at_cell(tx, tz, cell_x, cell_z);
        let h10 = self.get_height_at_cell(tx, tz, cell_x + 1, cell_z);
        let h01 = self.get_height_at_cell(tx, tz, cell_x, cell_z + 1);
        let h11 = self.get_height_at_cell(tx, tz, cell_x + 1, cell_z + 1);

        let h0 = h00 + (h10 - h00) * frac_x;
        let h1 = h01 + (h11 - h01) * frac_x;
        Ok(h0 + (h1 - h0) * frac_z)
    }

    /// Evict a tile from the cache (e.g. when moving far away).
    pub fn evict_tile(&mut self, tx: i32, tz: i32) {
        self.cache.remove(&tile_key(tx, tz));
    }

    /// Number of tiles currently cached.
    pub fn cached_tile_count(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_sea_level() {
        assert!((decode_height(10000) - 0.0).abs() < 0.001);
    }

    #[test]
    fn decode_negative() {
        // 6000 → 6000 * 0.05 - 500 = -200.0
        assert!((decode_height(6000) - (-200.0)).abs() < 0.001);
    }

    #[test]
    fn world_to_tile_center() {
        // Position (0, 0) should be tile (0, 0)
        assert_eq!(world_to_tile(0.0), 0);
    }

    #[test]
    fn world_to_tile_boundary() {
        // Tile 0 spans [-32, 32), tile 1 spans [32, 96)
        assert_eq!(world_to_tile(31.9), 0);
        assert_eq!(world_to_tile(32.0), 1);
        assert_eq!(world_to_tile(-32.0), 0);
        assert_eq!(world_to_tile(-32.1), -1);
    }
}
