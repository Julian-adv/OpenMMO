use std::{borrow::Cow, io};

pub const GRASS_V4_MAGIC: u32 = 0x4752_3034;
pub const GRASS_HEADER_BYTES: usize = 4;
pub const GRASS_CELL_TYPES: usize = 3;
pub const GRASS_DIM: usize = 64;
pub const GRASS_FILE_BYTES: usize = GRASS_HEADER_BYTES + GRASS_DIM * GRASS_DIM * GRASS_CELL_TYPES;

pub const GRASS_V3_MAGIC: u32 = 0x4752_3033;
const V3_HEADER_BYTES: usize = 16;
const V3_INSTANCE_BYTES: usize = 6;

pub fn empty_grass() -> Vec<u8> {
    let mut data = vec![0; GRASS_FILE_BYTES];
    data[..GRASS_HEADER_BYTES].copy_from_slice(&GRASS_V4_MAGIC.to_le_bytes());
    data
}

pub fn into_grass_density(data: Vec<u8>) -> io::Result<Vec<u8>> {
    match grass_density(&data)? {
        Cow::Borrowed(_) => Ok(data),
        Cow::Owned(density) => Ok(density),
    }
}

/// Upgrade legacy placements to per-cell [short, tall, flower] counts.
pub fn grass_density(data: &[u8]) -> io::Result<Cow<'_, [u8]>> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "Invalid grass tile");
    let magic = data.get(..4).ok_or_else(invalid)?;
    if magic == GRASS_V4_MAGIC.to_le_bytes() && data.len() == GRASS_FILE_BYTES {
        return Ok(Cow::Borrowed(data));
    }
    if magic != GRASS_V3_MAGIC.to_le_bytes() || data.len() < V3_HEADER_BYTES {
        return Err(invalid());
    }
    let counts = [4, 8, 12]
        .map(|offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize);
    let total = counts
        .iter()
        .try_fold(0usize, |total, count| total.checked_add(*count))
        .ok_or_else(invalid)?;
    let size = total
        .checked_mul(V3_INSTANCE_BYTES)
        .and_then(|n| n.checked_add(V3_HEADER_BYTES));
    if size != Some(data.len()) {
        return Err(invalid());
    }
    let mut output = empty_grass();
    let mut instances = data[V3_HEADER_BYTES..]
        .as_chunks::<V3_INSTANCE_BYTES>()
        .0
        .iter();
    for (kind, count) in counts.into_iter().enumerate() {
        for instance in instances.by_ref().take(count) {
            let cell = |bytes: &[u8]| {
                (u16::from_le_bytes(bytes.try_into().unwrap()) as usize * GRASS_DIM / 65535)
                    .min(GRASS_DIM - 1)
            };
            let x = cell(&instance[..2]);
            let z = cell(&instance[2..4]);
            let index = GRASS_HEADER_BYTES + (z * GRASS_DIM + x) * GRASS_CELL_TYPES + kind;
            output[index] = output[index].saturating_add(1);
        }
    }
    Ok(Cow::Owned(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_legacy_types_and_tile_edges_with_saturation() {
        let mut data = GRASS_V3_MAGIC.to_le_bytes().to_vec();
        for count in [300u32, 1, 1] {
            data.extend_from_slice(&count.to_le_bytes());
        }
        for _ in 0..301 {
            data.extend_from_slice(&[0; V3_INSTANCE_BYTES]);
        }
        data.extend_from_slice(&[255, 255, 255, 255, 0, 0]);
        let result = grass_density(&data).unwrap();
        assert_eq!(result.len(), GRASS_FILE_BYTES);
        assert_eq!(&result[4..7], &[255, 1, 0]);
        assert_eq!(result[GRASS_FILE_BYTES - 1], 1);
        assert!(matches!(grass_density(&result).unwrap(), Cow::Borrowed(_)));
        let owned = result.into_owned();
        let allocation = owned.as_ptr();
        let normalized = into_grass_density(owned).unwrap();
        assert_eq!(normalized.as_ptr(), allocation);
    }

    #[test]
    fn rejects_truncated_unknown_and_mismatched_tiles() {
        for data in [vec![], vec![0; 16], GRASS_V4_MAGIC.to_le_bytes().to_vec()] {
            assert!(grass_density(&data).is_err());
        }
        let mut legacy = GRASS_V3_MAGIC.to_le_bytes().to_vec();
        legacy.extend_from_slice(&u32::MAX.to_le_bytes());
        legacy.extend_from_slice(&[0; 8]);
        assert!(grass_density(&legacy).is_err());
    }
}
