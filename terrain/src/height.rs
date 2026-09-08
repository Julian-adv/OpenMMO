use crate::coords::world_to_tile;
use crate::defaults::{self, VERTS_PER_SIDE};
use crate::io::TerrainIO;
use crate::tile_cache::{TileCache, TileCacheReadGuard, TILE_CACHE_CAPACITY};
use onlinerpg_shared::worldgen::tile_bake::{HEIGHT_BIAS, HEIGHT_STEP};
use std::collections::BTreeMap;

/// Tile size in world units (must match client TERRAIN_TILE_SIZE).
const TILE_SIZE: f32 = defaults::TILE_DIM as f32;

pub type HeightRect = [f32; 4];

#[derive(Debug)]
pub struct HeightmapEdit {
    pub tile_x: i32,
    pub tile_z: i32,
    pub data: Vec<u8>,
}

/// Decode a uint16 heightmap value to meters.
/// Encoding: `round((meters + 500.0) / 0.05)` → range -500m to +3276m.
/// Also the water field's surfaceY codec.
pub fn decode_height(value: u16) -> f32 {
    value as f32 * HEIGHT_STEP - HEIGHT_BIAS
}

pub fn encode_height(value: f32) -> u16 {
    ((value + HEIGHT_BIAS) / HEIGHT_STEP)
        .round()
        .clamp(0.0, 65535.0) as u16
}

fn point_in_rect([min_x, min_z, max_x, max_z]: HeightRect, x: f32, z: f32) -> bool {
    x >= min_x && x <= max_x && z >= min_z && z <= max_z
}

fn decode_heightmap(data: Vec<u8>) -> Vec<u16> {
    data.as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u16::from_le_bytes(*bytes))
        .collect()
}

fn heightmap_edit(tile_x: i32, tile_z: i32, heights: Vec<u16>) -> HeightmapEdit {
    HeightmapEdit {
        tile_x,
        tile_z,
        data: heights.into_iter().flat_map(u16::to_le_bytes).collect(),
    }
}

fn tile_cell_bounds(tile_x: i32, tile_z: i32, rect: HeightRect) -> (i32, i32, i32, i32) {
    let tile_min_x = tile_x as f32 * TILE_SIZE - TILE_SIZE * 0.5;
    let tile_min_z = tile_z as f32 * TILE_SIZE - TILE_SIZE * 0.5;
    let [min_x, min_z, max_x, max_z] = rect;
    (
        ((min_x - tile_min_x).floor() as i32).max(0),
        ((max_x - tile_min_x).floor() as i32).min(VERTS_PER_SIDE as i32 - 1),
        ((min_z - tile_min_z).floor() as i32).max(0),
        ((max_z - tile_min_z).floor() as i32).min(VERTS_PER_SIDE as i32 - 1),
    )
}

pub fn flatten_heightmap_tile(
    heights: &mut [u16],
    tile_x: i32,
    tile_z: i32,
    rect: HeightRect,
    target_height: f32,
    blend_radius: f32,
    protected: &[HeightRect],
) -> bool {
    let [min_x, min_z, max_x, max_z] = rect;
    let expanded = [
        min_x - blend_radius,
        min_z - blend_radius,
        max_x + blend_radius,
        max_z + blend_radius,
    ];
    let (start_x, end_x, start_z, end_z) = tile_cell_bounds(tile_x, tile_z, expanded);
    let tile_min_x = tile_x as f32 * TILE_SIZE - TILE_SIZE * 0.5;
    let tile_min_z = tile_z as f32 * TILE_SIZE - TILE_SIZE * 0.5;
    let target_encoded = encode_height(target_height);
    let mut changed = false;

    for cell_z in start_z..=end_z {
        for cell_x in start_x..=end_x {
            let world_x = tile_min_x + cell_x as f32;
            let world_z = tile_min_z + cell_z as f32;
            if protected
                .iter()
                .any(|&protected_rect| point_in_rect(protected_rect, world_x, world_z))
            {
                continue;
            }

            let dx = (min_x - world_x).max(0.0).max(world_x - max_x);
            let dz = (min_z - world_z).max(0.0).max(world_z - max_z);
            let distance = dx.hypot(dz);
            let index = cell_z as usize * VERTS_PER_SIDE + cell_x as usize;
            let next = if distance == 0.0 {
                target_encoded
            } else if distance < blend_radius {
                let t = distance / blend_radius;
                let blend = 1.0 - t * t * (3.0 - 2.0 * t);
                let current = decode_height(heights[index]);
                encode_height(current + (target_height - current) * blend)
            } else {
                continue;
            };
            if heights[index] != next {
                heights[index] = next;
                changed = true;
            }
        }
    }

    changed
}

fn restore_heightmap_tile(
    current: &mut [u16],
    original: &[u16],
    tile_x: i32,
    tile_z: i32,
    rect: HeightRect,
) -> bool {
    let (start_x, end_x, start_z, end_z) = tile_cell_bounds(tile_x, tile_z, rect);
    let mut changed = false;
    for cell_z in start_z..=end_z {
        for cell_x in start_x..=end_x {
            let index = cell_z as usize * VERTS_PER_SIDE + cell_x as usize;
            if current[index] != original[index] {
                current[index] = original[index];
                changed = true;
            }
        }
    }
    changed
}

// The tile load between the check and the insert is awaited, so an `Entry`
// cannot span it.
#[allow(clippy::map_entry)]
pub async fn flatten_heightmap_rects(
    terrain: &TerrainIO,
    rects: &[HeightRect],
    target_height: f32,
    blend_radius: f32,
    protected: &[HeightRect],
) -> std::io::Result<Vec<HeightmapEdit>> {
    let mut tiles = BTreeMap::<(i32, i32), (Vec<u16>, bool)>::new();

    for &[min_x, min_z, max_x, max_z] in rects {
        let rect = [min_x, min_z, max_x, max_z];
        let expanded = [
            min_x - blend_radius,
            min_z - blend_radius,
            max_x + blend_radius,
            max_z + blend_radius,
        ];

        for tile_z in world_to_tile(expanded[1])..=world_to_tile(expanded[3]) {
            for tile_x in world_to_tile(expanded[0])..=world_to_tile(expanded[2]) {
                let key = (tile_x, tile_z);
                if !tiles.contains_key(&key) {
                    let raw = terrain.read_heightmap(tile_x, tile_z).await?;
                    tiles.insert(key, (decode_heightmap(raw), false));
                }

                let (heights, changed) = tiles.get_mut(&key).expect("tile inserted above");
                *changed |= flatten_heightmap_tile(
                    heights,
                    tile_x,
                    tile_z,
                    rect,
                    target_height,
                    blend_radius,
                    protected,
                );
            }
        }
    }

    Ok(tiles
        .into_iter()
        .filter(|(_, (_, changed))| *changed)
        .map(|((tile_x, tile_z), (heights, _))| heightmap_edit(tile_x, tile_z, heights))
        .collect())
}

// The tile load between the check and the insert is awaited, so an `Entry`
// cannot span it.
#[allow(clippy::map_entry)]
pub async fn restore_heightmap_rects(
    terrain: &TerrainIO,
    rects: &[HeightRect],
) -> std::io::Result<Vec<HeightmapEdit>> {
    let mut tiles = BTreeMap::<(i32, i32), (Vec<u16>, Vec<u16>, bool)>::new();

    for &[min_x, min_z, max_x, max_z] in rects {
        for tile_z in world_to_tile(min_z)..=world_to_tile(max_z) {
            for tile_x in world_to_tile(min_x)..=world_to_tile(max_x) {
                let key = (tile_x, tile_z);
                if !tiles.contains_key(&key) {
                    let Some(original) = terrain.read_original_heightmap(tile_x, tile_z).await?
                    else {
                        continue;
                    };
                    let current = terrain.read_heightmap(tile_x, tile_z).await?;
                    tiles.insert(
                        key,
                        (decode_heightmap(current), decode_heightmap(original), false),
                    );
                }
                let Some((current, original, changed)) = tiles.get_mut(&key) else {
                    continue;
                };
                *changed |= restore_heightmap_tile(
                    current,
                    original,
                    tile_x,
                    tile_z,
                    [min_x, min_z, max_x, max_z],
                );
            }
        }
    }

    Ok(tiles
        .into_iter()
        .filter(|(_, (_, _, changed))| *changed)
        .map(|((tile_x, tile_z), (heights, _, _))| heightmap_edit(tile_x, tile_z, heights))
        .collect())
}

/// Resolve a possibly-out-of-range tile-local vertex to its owning tile and
/// row-major index. Each tile stores the edge vertex it shares with its
/// neighbour, so ±1 steps cross at most one tile.
pub(crate) fn resolve_cell(tx: i32, tz: i32, cell_x: i32, cell_z: i32) -> ((i32, i32), usize) {
    let (mut tx, mut tz, mut cx, mut cz) = (tx, tz, cell_x, cell_z);
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
    ((tx, tz), cz as usize * VERTS_PER_SIDE + cx as usize)
}

/// Bilinear interpolation of per-vertex values supplied by `get(tx, tz, cx, cz)`.
pub(crate) fn bilinear(world_x: f32, world_z: f32, get: impl Fn(i32, i32, i32, i32) -> f32) -> f32 {
    let tx = world_to_tile(world_x);
    let tz = world_to_tile(world_z);
    let local_x = world_x - (tx as f32 * TILE_SIZE - TILE_SIZE / 2.0);
    let local_z = world_z - (tz as f32 * TILE_SIZE - TILE_SIZE / 2.0);
    let cell_x = local_x.floor() as i32;
    let cell_z = local_z.floor() as i32;
    let frac_x = local_x - local_x.floor();
    let frac_z = local_z - local_z.floor();

    let v00 = get(tx, tz, cell_x, cell_z);
    let v10 = get(tx, tz, cell_x + 1, cell_z);
    let v01 = get(tx, tz, cell_x, cell_z + 1);
    let v11 = get(tx, tz, cell_x + 1, cell_z + 1);

    let v0 = v00 + (v10 - v00) * frac_x;
    let v1 = v01 + (v11 - v01) * frac_x;
    v0 + (v1 - v0) * frac_z
}

/// Height at a tile-local vertex from a cache snapshot; a miss reads as 0.0.
fn get_height_at_cell(
    cache: &TileCacheReadGuard<'_, Vec<u16>>,
    tx: i32,
    tz: i32,
    cell_x: i32,
    cell_z: i32,
) -> f32 {
    let (key, idx) = resolve_cell(tx, tz, cell_x, cell_z);
    cache
        .get(&key)
        .and_then(|heights| heights.get(idx))
        .map_or(0.0, |v| decode_height(*v))
}

fn sample_cached(cache: &TileCacheReadGuard<'_, Vec<u16>>, world_x: f32, world_z: f32) -> f32 {
    bilinear(world_x, world_z, |tx, tz, cx, cz| {
        get_height_at_cell(cache, tx, tz, cx, cz)
    })
}

/// Where raw heightmap tiles come from. The local data directory when the
/// caller sits on the game server; something else (the server's public tile
/// API) for clients running elsewhere, which cannot carry the 3 GB tree.
#[async_trait::async_trait]
pub trait HeightTiles: Send + Sync {
    /// Raw little-endian u16 heightmap for one tile, `HEIGHTMAP_SIZE` bytes.
    /// Missing tiles yield `defaults::default_heightmap()` rather than an
    /// error — the world is larger than the baked area.
    async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>>;
}

#[async_trait::async_trait]
impl HeightTiles for TerrainIO {
    async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        TerrainIO::read_heightmap(self, tx, tz).await
    }
}

/// Provides terrain height sampling with an in-memory tile cache.
/// Loads heightmap tiles on demand from a `HeightTiles` source and caches them.
///
/// Uses interior mutability (`tokio::sync::RwLock`) so callers only need `&self`,
/// avoiding external mutex contention when multiple NPC connections share one sampler.
pub struct HeightSampler {
    cache: TileCache<Vec<u16>>,
    tiles: Box<dyn HeightTiles>,
}

impl HeightSampler {
    pub fn new(tiles: impl HeightTiles + 'static) -> Self {
        Self {
            cache: TileCache::new(TILE_CACHE_CAPACITY),
            tiles: Box::new(tiles),
        }
    }

    /// Ensure a tile's heightmap is loaded into the cache.
    /// No lock held during I/O; re-checks before decoding, first insert wins.
    async fn ensure_tile(&self, tx: i32, tz: i32) -> std::io::Result<()> {
        if self.cache.contains(&(tx, tz)).await {
            return Ok(());
        }
        let raw = self.tiles.read_heightmap(tx, tz).await?;
        if self.cache.contains(&(tx, tz)).await {
            return Ok(());
        }
        let heights: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        self.cache.insert_if_absent((tx, tz), heights).await;
        Ok(())
    }

    /// Sample terrain height at an arbitrary world position using bilinear
    /// interpolation, loading the covering tile on demand. One tile covers all
    /// four corners: `VERTS_PER_SIDE` is `TILE_DIM + 1`, so each tile stores the
    /// edge vertex it shares with its neighbour.
    pub async fn sample_height(&self, world_x: f32, world_z: f32) -> std::io::Result<f32> {
        self.ensure_tile(world_to_tile(world_x), world_to_tile(world_z))
            .await?;
        let cache = self.cache.read().await;
        Ok(sample_cached(&cache, world_x, world_z))
    }

    /// Number of tiles currently cached.
    pub async fn cached_tile_count(&self) -> usize {
        self.cache.len().await
    }

    pub async fn update_tile(&self, tx: i32, tz: i32, raw: &[u8]) -> std::io::Result<()> {
        if raw.len() != defaults::HEIGHTMAP_SIZE {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid heightmap size",
            ));
        }
        let heights = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
            .collect();
        self.cache.replace((tx, tz), heights).await;
        Ok(())
    }

    /// Evict tiles not sampled since the previous sweep.
    pub async fn sweep_stale_tiles(&self) -> usize {
        self.cache.sweep_stale().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// Tiles whose heights vary with both tile and cell index, so a grid that
    /// mis-attributes a sample to the wrong tile or cell shows up as a mismatch.
    struct CountingTiles(Arc<AtomicUsize>);

    #[async_trait::async_trait]
    impl HeightTiles for CountingTiles {
        async fn read_heightmap(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
            self.0.fetch_add(1, Ordering::Relaxed);
            let mut out = Vec::with_capacity(defaults::HEIGHTMAP_SIZE);
            for cz in 0..VERTS_PER_SIDE as i32 {
                for cx in 0..VERTS_PER_SIDE as i32 {
                    let v = 10000 + tx * 37 + tz * 11 + cx * 3 + cz;
                    out.extend_from_slice(&(v as u16).to_le_bytes());
                }
            }
            Ok(out)
        }
    }

    fn counting_sampler() -> (HeightSampler, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        (HeightSampler::new(CountingTiles(Arc::clone(&reads))), reads)
    }

    #[tokio::test]
    async fn sample_height_covers_a_cell_from_one_tile() {
        // Each tile stores VERTS_PER_SIDE = TILE_DIM + 1 vertices, so all four
        // bilinear corners live in the covering tile — no neighbour load.
        let (s, reads) = counting_sampler();
        for w in [-32.0, -31.9, 0.0, 31.9, 32.0, 95.9, -1000.5, 4740.5] {
            assert!(s.sample_height(w, w).await.is_ok());
        }
        // One read per distinct tile touched, never a neighbour on top.
        let tiles: std::collections::HashSet<i32> =
            [-32.0f32, -31.9, 0.0, 31.9, 32.0, 95.9, -1000.5, 4740.5]
                .iter()
                .map(|w| world_to_tile(*w))
                .collect();
        assert_eq!(reads.load(Ordering::Relaxed), tiles.len());
    }

    #[tokio::test]
    async fn swept_tile_is_reloaded_from_the_source_on_next_sample() {
        let (s, reads) = counting_sampler();
        assert!(s.sample_height(0.0, 0.0).await.is_ok());
        assert_eq!(reads.load(Ordering::Relaxed), 1);

        // Idle across two sweeps: evicted; the next sample hits the source.
        s.sweep_stale_tiles().await;
        assert_eq!(s.sweep_stale_tiles().await, 1);
        assert_eq!(s.cached_tile_count().await, 0);

        assert!(s.sample_height(0.0, 0.0).await.is_ok());
        assert_eq!(reads.load(Ordering::Relaxed), 2);

        // Sampled this period: the next sweep keeps it, and no re-read occurs.
        s.sweep_stale_tiles().await;
        assert!(s.sample_height(0.0, 0.0).await.is_ok());
        assert_eq!(reads.load(Ordering::Relaxed), 2);
    }

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
