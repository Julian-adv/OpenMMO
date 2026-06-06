use std::io;

use crate::{coords, io::TerrainIO};

const TILE_SIZE: f32 = crate::defaults::TILE_DIM as f32;
// V1 tree wire format — keep in sync with the client decoder in
// `client/src/lib/utils/tree-data.ts`.
const TREE_V1_MAGIC: u32 = 0x5452_3031; // "TR01"
const TREE_V1_HEADER_BYTES: usize = 12;
const TREE_V1_BYTES_PER_INSTANCE: usize = 6;

/// Axis-aligned exclusion rect [min_x, min_z, max_x, max_z] in world coords.
pub type TreeExclusionRect = [f32; 4];

#[derive(Debug)]
pub struct TreeRemovalStats {
    pub tiles_changed: usize,
    pub trees_removed: usize,
    pub changed_tiles: Vec<(i32, i32)>,
}

fn invalid_tree_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn tile_min_world(tile: i32) -> f32 {
    tile as f32 * TILE_SIZE - TILE_SIZE * 0.5
}

fn read_u32_le(data: &[u8], offset: usize) -> io::Result<u32> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| invalid_tree_data("tree data header is truncated"))?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn should_remove_tree(
    tile_x: i32,
    tile_z: i32,
    instance: &[u8],
    exclusion_rects: &[TreeExclusionRect],
) -> io::Result<bool> {
    let local_x = u16::from_le_bytes(
        instance[0..2]
            .try_into()
            .map_err(|_| invalid_tree_data("tree instance is truncated"))?,
    ) as f32
        * TILE_SIZE
        / 65535.0;
    let local_z = u16::from_le_bytes(
        instance[2..4]
            .try_into()
            .map_err(|_| invalid_tree_data("tree instance is truncated"))?,
    ) as f32
        * TILE_SIZE
        / 65535.0;
    let world_x = tile_min_world(tile_x) + local_x;
    let world_z = tile_min_world(tile_z) + local_z;

    Ok(exclusion_rects.iter().any(|[min_x, min_z, max_x, max_z]| {
        world_x >= *min_x && world_x <= *max_x && world_z >= *min_z && world_z <= *max_z
    }))
}

/// Filter V1 tree placement data by world-space exclusion rectangles.
///
/// Returns `Ok(None)` when no tree instances were removed.
pub fn filter_tree_v1_bytes_in_rects(
    tile_x: i32,
    tile_z: i32,
    data: &[u8],
    exclusion_rects: &[TreeExclusionRect],
) -> io::Result<Option<(Vec<u8>, usize)>> {
    if exclusion_rects.is_empty() {
        return Ok(None);
    }
    if data.len() < TREE_V1_HEADER_BYTES {
        return Err(invalid_tree_data("tree data header is truncated"));
    }

    let magic = read_u32_le(data, 0)?;
    if magic != TREE_V1_MAGIC {
        return Err(invalid_tree_data(format!(
            "unsupported tree data magic 0x{magic:08x}"
        )));
    }

    let original_counts = [
        read_u32_le(data, 4)? as usize,
        read_u32_le(data, 8)? as usize,
    ];
    let total = original_counts[0] + original_counts[1];
    let expected_len = TREE_V1_HEADER_BYTES + total * TREE_V1_BYTES_PER_INSTANCE;
    if data.len() != expected_len {
        return Err(invalid_tree_data(format!(
            "tree data length mismatch: expected {expected_len}, got {}",
            data.len()
        )));
    }

    let mut out = Vec::with_capacity(data.len());
    out.extend_from_slice(&TREE_V1_MAGIC.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());

    let mut kept_counts = [0usize, 0usize];
    let mut removed = 0usize;
    let mut offset = TREE_V1_HEADER_BYTES;
    for tree_type in 0..2 {
        for _ in 0..original_counts[tree_type] {
            let instance = &data[offset..offset + TREE_V1_BYTES_PER_INSTANCE];
            offset += TREE_V1_BYTES_PER_INSTANCE;
            if should_remove_tree(tile_x, tile_z, instance, exclusion_rects)? {
                removed += 1;
                continue;
            }
            kept_counts[tree_type] += 1;
            out.extend_from_slice(instance);
        }
    }

    if removed == 0 {
        return Ok(None);
    }

    out[4..8].copy_from_slice(&(kept_counts[0] as u32).to_le_bytes());
    out[8..12].copy_from_slice(&(kept_counts[1] as u32).to_le_bytes());
    Ok(Some((out, removed)))
}

fn rect_to_tile_bounds([min_x, min_z, max_x, max_z]: TreeExclusionRect) -> (i32, i32, i32, i32) {
    (
        coords::world_to_tile(min_x),
        coords::world_to_tile(max_x),
        coords::world_to_tile(min_z),
        coords::world_to_tile(max_z),
    )
}

/// Remove tree instances in the given rects from persisted terrain tree tiles.
pub async fn remove_trees_in_rects(
    terrain: &TerrainIO,
    exclusion_rects: &[TreeExclusionRect],
) -> io::Result<TreeRemovalStats> {
    let mut stats = TreeRemovalStats {
        tiles_changed: 0,
        trees_removed: 0,
        changed_tiles: Vec::new(),
    };

    // Union of all tiles touched by any rect — rooms in a house routinely share
    // a tile, so read/filter/write each tile at most once against the full rect
    // set instead of re-reading the just-written file per rect.
    let mut tiles: Vec<(i32, i32)> = Vec::new();
    for &rect in exclusion_rects {
        let (tile_min_x, tile_max_x, tile_min_z, tile_max_z) = rect_to_tile_bounds(rect);
        for tile_z in tile_min_z..=tile_max_z {
            for tile_x in tile_min_x..=tile_max_x {
                if !tiles.contains(&(tile_x, tile_z)) {
                    tiles.push((tile_x, tile_z));
                }
            }
        }
    }

    for (tile_x, tile_z) in tiles {
        let Some(data) = terrain.read_trees(tile_x, tile_z).await? else {
            continue;
        };
        let Some((filtered, removed)) =
            filter_tree_v1_bytes_in_rects(tile_x, tile_z, &data, exclusion_rects)?
        else {
            continue;
        };
        terrain.write_trees(tile_x, tile_z, &filtered).await?;
        stats.tiles_changed += 1;
        stats.trees_removed += removed;
        stats.changed_tiles.push((tile_x, tile_z));
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree_data(instances: &[(u16, u16)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&TREE_V1_MAGIC.to_le_bytes());
        out.extend_from_slice(&(instances.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        for &(x, z) in instances {
            out.extend_from_slice(&x.to_le_bytes());
            out.extend_from_slice(&z.to_le_bytes());
            out.push(0);
            out.push(0);
        }
        out
    }

    #[test]
    fn filters_tree_instances_inside_world_rect() {
        let data = tree_data(&[(32768, 32768), (65535, 65535)]);
        let filtered = filter_tree_v1_bytes_in_rects(0, 0, &data, &[[-1.0, -1.0, 1.0, 1.0]])
            .expect("valid tree data")
            .expect("one tree should be removed");

        assert_eq!(filtered.1, 1);
        assert_eq!(read_u32_le(&filtered.0, 4).unwrap(), 1);
        assert_eq!(read_u32_le(&filtered.0, 8).unwrap(), 0);
        assert_eq!(
            filtered.0.len(),
            TREE_V1_HEADER_BYTES + TREE_V1_BYTES_PER_INSTANCE
        );
    }

    #[test]
    fn returns_none_when_no_instances_match() {
        let data = tree_data(&[(65535, 65535)]);
        let filtered = filter_tree_v1_bytes_in_rects(0, 0, &data, &[[-1.0, -1.0, 1.0, 1.0]])
            .expect("valid tree data");

        assert!(filtered.is_none());
    }
}
