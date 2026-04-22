//! Per-tile river segment buckets and the `RIV1` binary serialization
//! consumed by the runtime ribbon mesh. See `RIVER_SYSTEM.md` §2.2.

use super::super::vector_features::RiverSegment;
use super::constants::TILE_DIM;
use super::context::BakeContext;

/// River segments keyed by their midpoint-owning tile. Built once per bake
/// (amortizing the world-wide scan across all tiles); segments in the same
/// polyline share endpoints so neighboring tiles' ribbons still meet even
/// when they own different halves.
pub type RiverSegmentBuckets = std::collections::HashMap<(i32, i32), Vec<RiverSegment>>;

pub fn bucket_river_segments_by_owner(ctx: &BakeContext) -> RiverSegmentBuckets {
    let mut buckets: RiverSegmentBuckets = RiverSegmentBuckets::new();
    for poly in &ctx.rivers_world {
        if poly.points.len() < 2 {
            continue;
        }
        for i in 0..poly.points.len() - 1 {
            let a = poly.points[i];
            let b = poly.points[i + 1];
            let mx = (a[0] + b[0]) * 0.5;
            let mz = (a[1] + b[1]) * 0.5;
            // World → tile: tile 0 spans [-32, 32), tile 1 [32, 96), …
            let tx = ((mx + TILE_DIM as f32 * 0.5) / TILE_DIM as f32).floor() as i32;
            let tz = ((mz + TILE_DIM as f32 * 0.5) / TILE_DIM as f32).floor() as i32;
            buckets.entry((tx, tz)).or_default().push(RiverSegment {
                ax: a[0],
                az: a[1],
                bx: b[0],
                bz: b[1],
                flow_norm_a: poly.flow_norm[i],
                flow_norm_b: poly.flow_norm[i + 1],
                width_a: poly.width[i],
                width_b: poly.width[i + 1],
            });
        }
    }
    buckets
}

pub const RIVER_BIN_MAGIC: &[u8; 4] = b"RIV1";
pub const RIVER_BIN_VERSION: u16 = 1;
pub const RIVER_BIN_HEADER_SIZE: usize = 16;
pub const RIVER_BIN_SEGMENT_SIZE: usize = 32;

/// Serialize a per-tile river segment list to the on-disk binary format
/// documented in `RIVER_SYSTEM.md` §2.2. Returns `None` when the segment
/// list is empty so callers can skip writing a file for tiles with no
/// rivers (matches the vegetation pattern — missing file = none).
///
/// Layout:
///
/// ```text
/// header (16 bytes):
///   bytes 0..4   magic        b"RIV1"
///   bytes 4..6   u16          version (currently 1)
///   bytes 6..8   u16          segment_count
///   bytes 8..12  f32          reserved (0.0)
///   bytes 12..16 f32          reserved (0.0)
///
/// per-segment (32 bytes):
///   bytes  0..4   f32          ax  — world-space start x
///   bytes  4..8   f32          az  — world-space start z
///   bytes  8..12  f32          bx  — world-space end x
///   bytes 12..16  f32          bz  — world-space end z
///   bytes 16..20  f32          width_a      — surface width at vertex A
///   bytes 20..24  f32          width_b      — surface width at vertex B
///   bytes 24..28  f32          flow_norm_a  — normalized flow at A
///   bytes 28..32  f32          flow_norm_b  — normalized flow at B
/// ```
pub fn bake_rivers_binary(segments: &[RiverSegment]) -> Option<Vec<u8>> {
    if segments.is_empty() {
        return None;
    }
    let n = segments.len();
    assert!(
        n <= u16::MAX as usize,
        "river segment count {n} exceeds u16 capacity"
    );
    let mut out = Vec::with_capacity(RIVER_BIN_HEADER_SIZE + n * RIVER_BIN_SEGMENT_SIZE);
    out.extend_from_slice(RIVER_BIN_MAGIC);
    out.extend_from_slice(&RIVER_BIN_VERSION.to_le_bytes());
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    for s in segments {
        out.extend_from_slice(&s.ax.to_le_bytes());
        out.extend_from_slice(&s.az.to_le_bytes());
        out.extend_from_slice(&s.bx.to_le_bytes());
        out.extend_from_slice(&s.bz.to_le_bytes());
        out.extend_from_slice(&s.width_a.to_le_bytes());
        out.extend_from_slice(&s.width_b.to_le_bytes());
        out.extend_from_slice(&s.flow_norm_a.to_le_bytes());
        out.extend_from_slice(&s.flow_norm_b.to_le_bytes());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn river_binary_round_trip() {
        // Empty segment list returns None so the baker can skip writing a
        // file for tiles without rivers.
        assert!(bake_rivers_binary(&[]).is_none());

        let segs = vec![
            RiverSegment {
                ax: -10.5,
                az: 7.25,
                bx: 20.0,
                bz: -8.0,
                flow_norm_a: 0.125,
                flow_norm_b: 0.875,
                width_a: 2.0,
                width_b: 8.0,
            },
            RiverSegment {
                ax: 20.0,
                az: -8.0,
                bx: 22.5,
                bz: -10.0,
                flow_norm_a: 0.875,
                flow_norm_b: 1.0,
                width_a: 8.0,
                width_b: 10.0,
            },
        ];
        let bytes = bake_rivers_binary(&segs).expect("non-empty segments encode");
        assert_eq!(
            bytes.len(),
            RIVER_BIN_HEADER_SIZE + segs.len() * RIVER_BIN_SEGMENT_SIZE
        );
        // Magic + version + segment_count.
        assert_eq!(&bytes[0..4], RIVER_BIN_MAGIC);
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), RIVER_BIN_VERSION);
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), segs.len() as u16);

        // First segment payload (offset 16).
        let read_f32 = |off: usize| -> f32 {
            f32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]])
        };
        assert_eq!(read_f32(16), segs[0].ax);
        assert_eq!(read_f32(20), segs[0].az);
        assert_eq!(read_f32(24), segs[0].bx);
        assert_eq!(read_f32(28), segs[0].bz);
        assert_eq!(read_f32(32), segs[0].width_a);
        assert_eq!(read_f32(36), segs[0].width_b);
        assert_eq!(read_f32(40), segs[0].flow_norm_a);
        assert_eq!(read_f32(44), segs[0].flow_norm_b);
        // Second segment starts at 16 + 32 = 48.
        assert_eq!(read_f32(48), segs[1].ax);
    }
}
