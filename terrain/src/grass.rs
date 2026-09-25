use std::io;

use crate::{coords, defaults::TILE_DIM, io::TerrainIO};
use onlinerpg_shared::grass_format::{grass_density, GRASS_CELL_TYPES, GRASS_HEADER_BYTES};

pub type GrassExclusionRect = [f32; 4];

#[derive(Debug)]
pub struct GrassRemovalStats {
    pub tiles_changed: usize,
    pub grass_removed: usize,
    pub changed_tiles: Vec<(i32, i32)>,
}

pub fn filter_grass_in_rects(
    tile_x: i32,
    tile_z: i32,
    data: &[u8],
    exclusion_rects: &[GrassExclusionRect],
) -> io::Result<Option<(Vec<u8>, usize)>> {
    if exclusion_rects.is_empty() {
        return Ok(None);
    }
    let mut output = grass_density(data)?.into_owned();
    let origin_x = tile_x as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let origin_z = tile_z as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let mut removed = 0;
    for (index, cell) in output[GRASS_HEADER_BYTES..]
        .as_chunks_mut::<GRASS_CELL_TYPES>()
        .0
        .iter_mut()
        .enumerate()
    {
        let x = origin_x + (index % TILE_DIM) as f32;
        let z = origin_z + (index / TILE_DIM) as f32;
        if exclusion_rects.iter().any(|[min_x, min_z, max_x, max_z]| {
            x <= *max_x && x + 1.0 > *min_x && z <= *max_z && z + 1.0 > *min_z
        }) {
            removed += cell.iter().map(|&count| count as usize).sum::<usize>();
            cell.fill(0);
        }
    }
    Ok((removed > 0).then_some((output, removed)))
}

pub async fn remove_grass_in_rects(
    terrain: &TerrainIO,
    exclusion_rects: &[GrassExclusionRect],
) -> io::Result<GrassRemovalStats> {
    let mut stats = GrassRemovalStats {
        tiles_changed: 0,
        grass_removed: 0,
        changed_tiles: Vec::new(),
    };
    let mut tiles = Vec::new();
    for &[min_x, min_z, max_x, max_z] in exclusion_rects {
        for tile_z in coords::world_to_tile(min_z)..=coords::world_to_tile(max_z) {
            for tile_x in coords::world_to_tile(min_x)..=coords::world_to_tile(max_x) {
                if !tiles.contains(&(tile_x, tile_z)) {
                    tiles.push((tile_x, tile_z));
                }
            }
        }
    }

    for (tile_x, tile_z) in tiles {
        let Some(data) = terrain.read_grass(tile_x, tile_z).await? else {
            continue;
        };
        let Some((filtered, removed)) =
            filter_grass_in_rects(tile_x, tile_z, &data, exclusion_rects)?
        else {
            continue;
        };
        terrain.ensure_original_grass(tile_x, tile_z).await?;
        terrain.write_grass(tile_x, tile_z, &filtered).await?;
        stats.tiles_changed += 1;
        stats.grass_removed += removed;
        stats.changed_tiles.push((tile_x, tile_z));
    }

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::grass_format::empty_grass;

    #[test]
    fn clears_all_types_in_partially_overlapping_cells() {
        let mut data = empty_grass();
        let cell = GRASS_HEADER_BYTES + (32 * TILE_DIM + 32) * 3;
        data[cell..cell + 6].copy_from_slice(&[64, 36, 1, 8, 4, 2]);
        let (filtered, removed) = filter_grass_in_rects(0, 0, &data, &[[0.1, 0.2, 0.3, 0.4]])
            .unwrap()
            .unwrap();
        assert_eq!(removed, 101);
        assert_eq!(&filtered[cell..cell + 6], &[0, 0, 0, 8, 4, 2]);
        assert!(
            filter_grass_in_rects(0, 0, &filtered, &[[0.1, 0.2, 0.3, 0.4]])
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn clearing_respects_negative_tile_origins() {
        let mut data = empty_grass();
        data[4..7].copy_from_slice(&[1, 2, 3]);
        let (filtered, removed) =
            filter_grass_in_rects(-1, -1, &data, &[[-96.0, -96.0, -95.5, -95.5]])
                .unwrap()
                .unwrap();
        assert_eq!(removed, 6);
        assert!(filtered[4..].iter().all(|&count| count == 0));
    }
}
