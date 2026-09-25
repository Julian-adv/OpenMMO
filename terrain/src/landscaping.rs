use crate::{
    coords, defaults,
    io::{write_terrain_file, TerrainIO},
};
use onlinerpg_shared::grass_format::{into_grass_density, GRASS_V3_MAGIC, GRASS_V4_MAGIC};
use onlinerpg_shared::landscaping::{is_cleared, LandscapingTile, CLEARED_BYTES};
use onlinerpg_shared::tree_format::{
    TREE_V1_BYTES_PER_INSTANCE, TREE_V1_HEADER_BYTES, TREE_V1_MAGIC,
};
use std::io;

const MAGIC: &[u8; 4] = b"LND1";

impl TerrainIO {
    pub async fn read_landscaping_tile(
        &self,
        tx: i32,
        tz: i32,
    ) -> io::Result<Option<LandscapingTile>> {
        let path = coords::landscaping_path(self.base_dir(), tx, tz);
        let data = match tokio::fs::read(path).await {
            Ok(data) => data,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        decode_landscaping(&data, tx, tz).map(Some)
    }

    pub async fn write_landscaping_tile(&self, tile: &LandscapingTile) -> io::Result<()> {
        if tile.splat.len() != defaults::SPLATMAP_SIZE || tile.cleared.len() != CLEARED_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid landscaping tile size",
            ));
        }
        let mut data = Vec::with_capacity(4 + defaults::SPLATMAP_SIZE + CLEARED_BYTES);
        data.extend_from_slice(MAGIC);
        data.extend_from_slice(&tile.splat);
        data.extend_from_slice(&tile.cleared);
        write_terrain_file(
            &coords::landscaping_path(self.base_dir(), tile.tile_x, tile.tile_z),
            &data,
        )
        .await
    }
}

pub fn decode_landscaping(data: &[u8], tx: i32, tz: i32) -> io::Result<LandscapingTile> {
    if data.len() != 4 + defaults::SPLATMAP_SIZE + CLEARED_BYTES || &data[..4] != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid landscaping tile",
        ));
    }
    Ok(LandscapingTile {
        tile_x: coords::wrap_tile_x(tx),
        tile_z: tz,
        splat: data[4..4 + defaults::SPLATMAP_SIZE].to_vec(),
        cleared: data[4 + defaults::SPLATMAP_SIZE..].to_vec(),
    })
}

pub fn filter_vegetation(data: Vec<u8>, cleared: &[u8]) -> io::Result<Vec<u8>> {
    if data.starts_with(&GRASS_V4_MAGIC.to_le_bytes())
        || data.starts_with(&GRASS_V3_MAGIC.to_le_bytes())
    {
        let mut data = into_grass_density(data)?;
        for (index, cell) in data[4..].as_chunks_mut::<3>().0.iter_mut().enumerate() {
            if is_cleared(cleared, index) {
                cell.fill(0);
            }
        }
        return Ok(data);
    }
    if cleared.iter().all(|b| *b == 0) {
        return Ok(data);
    }
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid vegetation tile");
    if data.len() < TREE_V1_HEADER_BYTES || !data.starts_with(&TREE_V1_MAGIC.to_le_bytes()) {
        return Err(invalid());
    }
    let read = |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let counts = [read(4) as usize, read(8) as usize];
    let total: usize = counts.iter().sum();
    if data.len() != TREE_V1_HEADER_BYTES + total * TREE_V1_BYTES_PER_INSTANCE {
        return Err(invalid());
    }
    let mut output = data[..TREE_V1_HEADER_BYTES].to_vec();
    let mut offset = TREE_V1_HEADER_BYTES;
    for (kind, count) in counts.into_iter().enumerate() {
        let mut kept = 0u32;
        for _ in 0..count {
            let instance = &data[offset..offset + TREE_V1_BYTES_PER_INSTANCE];
            offset += TREE_V1_BYTES_PER_INSTANCE;
            let x = u16::from_le_bytes(instance[..2].try_into().unwrap()) as usize * 64 / 65535;
            let z = u16::from_le_bytes(instance[2..4].try_into().unwrap()) as usize * 64 / 65535;
            if x < 64 && z < 64 && is_cleared(cleared, z * 64 + x) {
                continue;
            }
            output.extend_from_slice(instance);
            kept += 1;
        }
        output[4 + kind * 4..8 + kind * 4].copy_from_slice(&kept.to_le_bytes());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use onlinerpg_shared::landscaping::clear_cell;

    fn vegetation(magic: u32, types: usize) -> Vec<u8> {
        let mut bytes = magic.to_le_bytes().to_vec();
        for _ in 0..types {
            bytes.extend_from_slice(&2u32.to_le_bytes());
        }
        for _ in 0..types {
            for position in [100u16, 40000u16] {
                bytes.extend_from_slice(&position.to_le_bytes());
                bytes.extend_from_slice(&position.to_le_bytes());
                bytes.extend_from_slice(&[5, 6]);
            }
        }
        bytes
    }

    #[test]
    fn filters_every_vegetation_type_only_inside_cleared_cells() {
        let mut mask = vec![0; CLEARED_BYTES];
        clear_cell(&mut mask, 0);
        for (magic, types) in [(TREE_V1_MAGIC, 2), (GRASS_V3_MAGIC, 3)] {
            let data = filter_vegetation(vegetation(magic, types), &mask).unwrap();
            if magic == GRASS_V3_MAGIC {
                assert_eq!(data.len(), onlinerpg_shared::grass_format::GRASS_FILE_BYTES);
                assert_eq!(&data[4..7], &[0; 3]);
                assert_eq!(data[4..].iter().map(|&n| n as usize).sum::<usize>(), 3);
                continue;
            }
            assert_eq!(data.len(), 4 + types * 4 + types * 6);
            for kind in 0..types {
                assert_eq!(
                    u32::from_le_bytes(data[4 + kind * 4..8 + kind * 4].try_into().unwrap()),
                    1
                );
            }
        }
    }

    #[tokio::test]
    async fn a_single_atomic_tile_preserves_paint_and_removal_across_reload() {
        let dir = std::env::temp_dir().join(format!("landscaping_tile_{}", std::process::id()));
        let terrain = TerrainIO::new(dir.clone());
        let mut tile = LandscapingTile {
            tile_x: 0,
            tile_z: 0,
            splat: defaults::default_splatmap(),
            cleared: vec![0; CLEARED_BYTES],
        };
        tile.splat[0] = 0x55;
        clear_cell(&mut tile.cleared, 0);
        terrain
            .write_grass(0, 0, &vegetation(GRASS_V3_MAGIC, 3))
            .await
            .unwrap();
        terrain
            .write_trees(0, 0, &vegetation(TREE_V1_MAGIC, 2))
            .await
            .unwrap();
        terrain.write_landscaping_tile(&tile).await.unwrap();
        let restarted = TerrainIO::new(dir.clone());
        assert_eq!(restarted.read_splatmap(0, 0).await.unwrap()[0], 0x55);
        let grass = restarted.read_grass(0, 0).await.unwrap().unwrap();
        assert_eq!(grass[4..].iter().map(|&n| n as usize).sum::<usize>(), 3);
        assert_eq!(restarted.read_trees(0, 0).await.unwrap().unwrap().len(), 24);
        restarted
            .write_splatmap(0, 0, &defaults::default_splatmap())
            .await
            .unwrap();
        assert_eq!(restarted.read_splatmap(0, 0).await.unwrap()[0], 0);
        assert_eq!(restarted.read_trees(0, 0).await.unwrap().unwrap().len(), 24);
        restarted.delete_region(0, 0).await.unwrap();
        assert!(restarted
            .read_landscaping_tile(0, 0)
            .await
            .unwrap()
            .is_none());
        let _ = tokio::fs::remove_dir_all(dir).await;
    }
}
