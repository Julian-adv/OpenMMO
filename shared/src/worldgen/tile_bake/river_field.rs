//! Per-tile baked river field: pixel-aligned surface elevation + flow
//! direction. The runtime renders one quad per river-bearing tile and
//! derives every visual effect from this field plus the heightmap.
//!
//! Format `RFD1` (= River Field Data, version 1):
//!
//! ```text
//! header (16 bytes):
//!   bytes  0..4   magic    b"RFD1"
//!   bytes  4..6   u16      version (currently 1)
//!   bytes  6..8   u16      grid_x  (== VERTS_PER_SIDE = 65)
//!   bytes  8..10  u16      grid_z  (== VERTS_PER_SIDE = 65)
//!   bytes 10..16  u8[6]    reserved (zero)
//!
//! per-pixel (4 bytes, row-major over 65×65, X then Z):
//!   bytes  0..2   u16      surfaceY — encoded same as heightmap
//!                          (HEIGHT_BIAS / HEIGHT_STEP). Holds the river
//!                          *water surface* at this world XZ within the
//!                          field's reach, otherwise the natural ground
//!                          (so depth = surfaceY − bedY collapses to 0).
//!   byte   2      i8       flowX — unit downstream direction × 127
//!   byte   3      i8       flowZ — unit downstream direction × 127
//! ```
//!
//! Cross-tile consistency: both tiles touching a seam see the same
//! segment list (filtered with the global `river_margin`), so identical
//! world-XZ pixels produce identical surfaceY/flow values regardless of
//! which tile owns the segment.

use super::super::global_map::GlobalMap;
use super::super::vector_features::{project_point_to_segment, RiverSegment};
use super::constants::{
    HEIGHT_BIAS, HEIGHT_STEP, RIVER_CARVE_DEPTH_EXTRA_M, RIVER_CARVE_DEPTH_MIN_M,
    RIVER_CARVE_MIN_BED_Y_M, RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M,
    RIVER_DEPTH_OFFSET_M, VERTS_PER_SIDE,
};
use super::context::BakeContext;
use super::heightmap::{lerp, sample_natural_height_single};

pub const RIVER_FIELD_BIN_MAGIC: &[u8; 4] = b"RFD1";
pub const RIVER_FIELD_BIN_VERSION: u16 = 1;
const RIVER_FIELD_HEADER_SIZE: usize = 16;
const RIVER_FIELD_PIXEL_SIZE: usize = 4;
const RIVER_FIELD_PAYLOAD_SIZE: usize = VERTS_PER_SIDE * VERTS_PER_SIDE * RIVER_FIELD_PIXEL_SIZE;
const RIVER_FIELD_TOTAL_SIZE: usize = RIVER_FIELD_HEADER_SIZE + RIVER_FIELD_PAYLOAD_SIZE;

/// Bake the per-tile river field. Returns `None` when the tile carries
/// no river segments — caller skips writing a file.
pub fn bake_river_field(
    map: &GlobalMap,
    ctx: &BakeContext,
    heights: &[f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    river_segs: &[RiverSegment],
) -> Option<Vec<u8>> {
    if river_segs.is_empty() {
        return None;
    }

    // Unit tangent per segment — used by every pixel's flow accumulation,
    // so amortize the sqrt over the tile instead of paying it per pixel.
    // Zero-length segments produce (0, 0) which the weighting loop skips.
    let seg_tangents: Vec<(f32, f32)> = river_segs
        .iter()
        .map(|s| {
            let dx = s.bx - s.ax;
            let dz = s.bz - s.az;
            let len_sq = dx * dx + dz * dz;
            if len_sq < 1e-6 {
                (0.0, 0.0)
            } else {
                let inv = 1.0 / len_sq.sqrt();
                (dx * inv, dz * inv)
            }
        })
        .collect();

    let mut out = Vec::with_capacity(RIVER_FIELD_TOTAL_SIZE);
    out.extend_from_slice(RIVER_FIELD_BIN_MAGIC);
    out.extend_from_slice(&RIVER_FIELD_BIN_VERSION.to_le_bytes());
    out.extend_from_slice(&(VERTS_PER_SIDE as u16).to_le_bytes());
    out.extend_from_slice(&(VERTS_PER_SIDE as u16).to_le_bytes());
    out.extend_from_slice(&[0u8; 6]);

    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let wx = tile_origin_x + i as f32;
            let wz = tile_origin_z + j as f32;
            let bed_y = heights[j * VERTS_PER_SIDE + i];
            let (surface_y, flow_x, flow_z) = compute_pixel(
                wx,
                wz,
                bed_y,
                map,
                ctx,
                heights,
                tile_origin_x,
                tile_origin_z,
                river_segs,
                &seg_tangents,
            );
            let v = ((surface_y + HEIGHT_BIAS) / HEIGHT_STEP)
                .round()
                .clamp(0.0, 65535.0) as u16;
            out.extend_from_slice(&v.to_le_bytes());
            out.push(encode_unit(flow_x) as u8);
            out.push(encode_unit(flow_z) as u8);
        }
    }
    Some(out)
}

#[inline]
fn encode_unit(v: f32) -> i8 {
    (v.clamp(-1.0, 1.0) * 127.0).round().clamp(-127.0, 127.0) as i8
}

/// Single-pass query that returns both the inverse-distance-weighted flow
/// direction (averaged across all segments with weight `1/(d² + 1)`) and
/// the nearest segment's `(idx, t)` for surface elevation. Near a Voronoi
/// boundary two segments have comparable weights so the blended direction
/// crosses smoothly; away from boundaries the squared falloff makes the
/// nearest segment dominate. Avoids a separate post-smoothing pass.
fn weighted_flow_and_nearest(
    px: f32,
    pz: f32,
    segs: &[RiverSegment],
    tangents: &[(f32, f32)],
) -> Option<(f32, f32, usize, f32, f32)> {
    if segs.is_empty() {
        return None;
    }
    let mut sx = 0.0f32;
    let mut sz = 0.0f32;
    let mut w_total = 0.0f32;
    let mut best_sq = f32::INFINITY;
    let mut best_idx = 0usize;
    let mut best_t = 0.0f32;
    for (i, s) in segs.iter().enumerate() {
        let (d_sq, t) = project_point_to_segment(px, pz, s.ax, s.az, s.bx, s.bz);
        if d_sq < best_sq {
            best_sq = d_sq;
            best_idx = i;
            best_t = t;
        }
        let (tx, tz) = tangents[i];
        if tx == 0.0 && tz == 0.0 {
            continue;
        }
        let w = 1.0 / (d_sq + 1.0);
        sx += tx * w;
        sz += tz * w;
        w_total += w;
    }
    let best_d = best_sq.sqrt();
    if w_total < 1e-6 {
        return Some((0.0, 0.0, best_idx, best_t, best_d));
    }
    let fx = sx / w_total;
    let fz = sz / w_total;
    let mag = (fx * fx + fz * fz).sqrt();
    let (fx, fz) = if mag > 1e-4 {
        (fx / mag, fz / mag)
    } else {
        // Cancellation (opposing tangents balanced) — fall back to the
        // dominant segment so the pixel still carries a meaningful flow
        // direction instead of stalling the shader's ripple/scroll.
        tangents[best_idx]
    };
    Some((fx, fz, best_idx, best_t, best_d))
}

/// Compute the field record for one pixel: river surface elevation +
/// downstream-unit flow direction.
///
/// `surface_y = bed_at_proj + OFFSET` for every pixel — the surface
/// stays horizontal across the entire tile because pixels at any
/// perpendicular distance project to the same centerline point and
/// share the same `bed_at_proj`. Visibility is left entirely to the
/// shader's `depth = surface − bed` fade: where the bed (terrain) rises
/// above `surface`, alpha goes to 0 and the river terminates naturally
/// at the bank. This relies on `apply_min_land_floor` keeping land
/// outside the carve > `RIVER_DEPTH_OFFSET_M` so the river doesn't
/// bleed across the whole tile.
#[allow(clippy::too_many_arguments)]
fn compute_pixel(
    wx: f32,
    wz: f32,
    bed_y_pixel: f32,
    map: &GlobalMap,
    ctx: &BakeContext,
    heights: &[f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    river_segs: &[RiverSegment],
    seg_tangents: &[(f32, f32)],
) -> (f32, f32, f32) {
    let Some((flow_x, flow_z, idx, t, dist)) =
        weighted_flow_and_nearest(wx, wz, river_segs, seg_tangents)
    else {
        return (bed_y_pixel, 0.0, 0.0);
    };
    let seg = &river_segs[idx];

    // Surface = carved bed at the centerline projection + runtime offset.
    // For in-tile projections, bilinear-sample the already-baked
    // (post-carve) `heights` directly — at the centerline the carve has
    // applied the full depth, so the post-carve sample IS the bed.
    // For projections that escape the tile, fall back to natural sampling
    // + the carve formula (rebuilds bed = natural − capped_depth).
    let proj_x = lerp(seg.ax, seg.bx, t);
    let proj_z = lerp(seg.az, seg.bz, t);
    let bed_at_proj = sample_in_tile(heights, tile_origin_x, tile_origin_z, proj_x, proj_z)
        .unwrap_or_else(|| {
            let natural = sample_natural_height_single(map, ctx, proj_x, proj_z);
            let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
            let depth = RIVER_CARVE_DEPTH_MIN_M + RIVER_CARVE_DEPTH_EXTRA_M * flow_norm;
            let capped = depth.min((natural - RIVER_CARVE_MIN_BED_Y_M).max(0.0));
            natural - capped
        });
    // Collapse the surface back to local bed beyond the carve's lateral
    // influence. Without this the polyline-projected `bed_at_proj` lifts
    // surfaceY above any pixel that happens to be far from the river but
    // sits in a lower geographic region (cliffs, deltas, estuary flats),
    // and the shader's `depth = surface − local_bed` then renders the
    // river as a flood. Use the same `half_width + taper` envelope as
    // `segment_carve_params` so the visible water matches the carve width.
    let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
    let width = lerp(seg.width_a, seg.width_b, t);
    let half_width = width * 0.5;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let surface_full = bed_at_proj + RIVER_DEPTH_OFFSET_M;
    let surface_y = if dist <= half_width {
        surface_full
    } else if dist >= half_width + taper {
        bed_y_pixel
    } else {
        let u = (dist - half_width) / taper.max(1e-3);
        let s = 1.0 - u * u * (3.0 - 2.0 * u);
        bed_y_pixel + (surface_full - bed_y_pixel) * s
    };

    (surface_y, flow_x, flow_z)
}

/// Bilinear-sample the local `heights` array at world `(wx, wz)`. Returns
/// `None` when the position lies outside the tile — caller falls back
/// to a global sampler.
#[inline]
fn sample_in_tile(
    heights: &[f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    wx: f32,
    wz: f32,
) -> Option<f32> {
    let lx = wx - tile_origin_x;
    let lz = wz - tile_origin_z;
    let max = (VERTS_PER_SIDE - 1) as f32;
    if lx < 0.0 || lz < 0.0 || lx > max || lz > max {
        return None;
    }
    let i0 = lx.floor() as usize;
    let j0 = lz.floor() as usize;
    let fx = lx - i0 as f32;
    let fz = lz - j0 as f32;
    let i1 = (i0 + 1).min(VERTS_PER_SIDE - 1);
    let j1 = (j0 + 1).min(VERTS_PER_SIDE - 1);
    let h00 = heights[j0 * VERTS_PER_SIDE + i0];
    let h10 = heights[j0 * VERTS_PER_SIDE + i1];
    let h01 = heights[j1 * VERTS_PER_SIDE + i0];
    let h11 = heights[j1 * VERTS_PER_SIDE + i1];
    let h0 = h00 * (1.0 - fx) + h10 * fx;
    let h1 = h01 * (1.0 - fx) + h11 * fx;
    Some(h0 * (1.0 - fz) + h1 * fz)
}

#[cfg(test)]
mod tests {
    use super::super::super::vector_features::RiverSegment;
    use super::*;

    fn fake_segments() -> Vec<RiverSegment> {
        vec![RiverSegment {
            ax: -10.0,
            az: 0.0,
            bx: 10.0,
            bz: 0.0,
            flow_norm_a: 0.5,
            flow_norm_b: 0.5,
            width_a: 4.0,
            width_b: 4.0,
        }]
    }

    fn small_test_ctx() -> (
        crate::worldgen::global_map::GlobalMap,
        crate::worldgen::tile_bake::BakeContext,
    ) {
        // Tiny world keeps the test fast.
        let cfg = crate::worldgen::config::WorldGenConfig {
            seed: 7,
            world_size_m: 256,
            global_res: 32,
            ..Default::default()
        };
        let mut map = crate::worldgen::continent::generate_continent_mask(&cfg);
        crate::worldgen::elevation::generate_elevation(&mut map);
        let rm = crate::worldgen::rivers::compute_flow(&map);
        let net = crate::worldgen::roads::compute_roads(&map, &[], &rm);
        let coast =
            crate::worldgen::coasts::extract_coasts(&map.land_mask, map.config.global_res as usize);
        let ctx = BakeContext::new(&map, &rm, &net, &coast);
        (map, ctx)
    }

    #[test]
    fn empty_segments_returns_none() {
        let (map, ctx) = small_test_ctx();
        let heights = vec![0.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];
        let bin = bake_river_field(&map, &ctx, &heights, 0.0, 0.0, &[]);
        assert!(bin.is_none());
    }

    #[test]
    fn encode_unit_round_trip() {
        // Linear quantization to i8 must clip cleanly at ±1, preserve sign,
        // and round to the nearest integer step.
        assert_eq!(encode_unit(0.0), 0);
        assert_eq!(encode_unit(1.0), 127);
        assert_eq!(encode_unit(-1.0), -127);
        assert_eq!(encode_unit(2.0), 127);
        assert_eq!(encode_unit(-2.0), -127);
        // 0.5 × 127 = 63.5 → rounds to 64.
        assert_eq!(encode_unit(0.5), 64);
    }

    #[test]
    fn binary_size_matches_layout() {
        // Pin the on-disk layout — runtime decoders hard-code these offsets
        // and any drift would silently corrupt every loader.
        let (map, ctx) = small_test_ctx();
        let heights = vec![5.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];
        let segs = fake_segments();
        let bin = bake_river_field(&map, &ctx, &heights, -32.0, -32.0, &segs)
            .expect("non-empty segments produce a file");
        assert_eq!(bin.len(), RIVER_FIELD_TOTAL_SIZE);
        assert_eq!(&bin[0..4], RIVER_FIELD_BIN_MAGIC);
        assert_eq!(
            u16::from_le_bytes([bin[4], bin[5]]),
            RIVER_FIELD_BIN_VERSION
        );
        assert_eq!(u16::from_le_bytes([bin[6], bin[7]]), VERTS_PER_SIDE as u16);
        assert_eq!(u16::from_le_bytes([bin[8], bin[9]]), VERTS_PER_SIDE as u16);
    }

    #[test]
    fn surface_collapses_to_local_bed_beyond_carve_envelope() {
        // Within the carve's `half_width + taper` envelope the surface sits
        // at `bed_at_proj + RIVER_DEPTH_OFFSET_M`; beyond it the surface
        // collapses to the local bed so the shader's depth-fade hides the
        // river instead of letting it spill into surrounding lower terrain.
        let (map, ctx) = small_test_ctx();
        let mut heights = vec![5.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];
        // Stamp a deeper centerline so bed_at_proj differs from bed_y_pixel:
        // row z=32 (the segment's z) carved to 1.0m, rest stays at 5m.
        for i in 0..VERTS_PER_SIDE {
            heights[32 * VERTS_PER_SIDE + i] = 1.0;
        }
        let segs = vec![RiverSegment {
            ax: -32.0,
            az: 0.0,
            bx: 32.0,
            bz: 0.0,
            flow_norm_a: 0.5,
            flow_norm_b: 0.5,
            width_a: 4.0,
            width_b: 4.0,
        }];
        let bin = bake_river_field(&map, &ctx, &heights, -32.0, -32.0, &segs)
            .expect("segment present, file is written");

        let pixel_surface = |i: usize, j: usize| -> f32 {
            let off = RIVER_FIELD_HEADER_SIZE
                + (j * VERTS_PER_SIDE + i) * RIVER_FIELD_PIXEL_SIZE;
            let s = u16::from_le_bytes([bin[off], bin[off + 1]]);
            s as f32 * HEIGHT_STEP - HEIGHT_BIAS
        };
        // half_width = 2.0, taper = 3.0 + 7.0*0.5 = 6.5, envelope = 8.5.
        // j=32 → dist=0 (on axis), j=33 → dist=1 (inside half_width),
        // j=60 → dist=28 (well past envelope).
        let on_axis = pixel_surface(32, 32);
        let inside_half_width = pixel_surface(32, 33);
        let far = pixel_surface(32, 60);
        assert!(
            (on_axis - 1.5).abs() < 0.1,
            "on-axis surface should be bed_at_proj (1.0m) + offset (0.5m), got {on_axis}"
        );
        assert!(
            (inside_half_width - 1.5).abs() < 0.1,
            "inside half_width surface stays at the river level, got {inside_half_width}"
        );
        assert!(
            (far - 5.0).abs() < 0.1,
            "beyond carve envelope surface collapses to local bed (5.0m), got {far}"
        );

        // Flow direction propagates to every pixel so ripples are
        // continuous. Segment runs +X so flowX≈+127.
        let off = RIVER_FIELD_HEADER_SIZE
            + (60 * VERTS_PER_SIDE + 32) * RIVER_FIELD_PIXEL_SIZE;
        let flow_x = bin[off + 2] as i8;
        let flow_z = bin[off + 3] as i8;
        assert!(flow_x > 100, "far pixel should still carry flowX, got {flow_x}");
        assert!(flow_z.abs() < 10, "flowZ should stay near 0, got {flow_z}");
    }
}
