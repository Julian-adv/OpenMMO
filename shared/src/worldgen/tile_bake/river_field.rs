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
//!                          minus `RIVER_OFF_CHANNEL_SAFETY_M` (so depth
//!                          = surfaceY − bedY stays negative even when
//!                          runtime heightmap edits lower the bed).
//!   byte   2      i8       flowX — unit downstream direction × 127
//!   byte   3      i8       flowZ — unit downstream direction × 127
//! ```
//!
//! Cross-tile consistency: both tiles touching a seam see the same
//! segment list (filtered with the global `river_margin`), so identical
//! world-XZ pixels produce identical surfaceY/flow values regardless of
//! which tile owns the segment.

use super::super::global_map::GlobalMap;
use super::super::noise::smoothstep;
use super::super::vector_features::{project_point_to_segment, RiverSegment, Segment};
use super::constants::{
    HEIGHT_BIAS, HEIGHT_STEP, RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M,
    RIVER_DEPTH_OFFSET_M, RIVER_OFF_CHANNEL_SAFETY_M, VERTS_PER_SIDE,
};
use super::context::BakeContext;
use super::heightmap::{lerp, sample_carved_bed};

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

    // Coast segments for `sample_carved_bed`'s shoreline height blend. A
    // pixel's centerline projection can sit up to `river_margin_m()`
    // outside the tile bbox (delta wedges), so extend the filter by that
    // much — the bed sampled here must byte-match what the projection's
    // owning tile baked into its own heightmap.
    let tile_max_x = tile_origin_x + (VERTS_PER_SIDE as f32 - 1.0);
    let tile_max_z = tile_origin_z + (VERTS_PER_SIDE as f32 - 1.0);
    let coast_segs = super::heightmap::coast_segments_near(
        ctx,
        map.config.world_size_m as f32,
        tile_origin_x,
        tile_origin_z,
        tile_max_x,
        tile_max_z,
        super::river_margin_m(),
    );

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
                river_segs,
                &seg_tangents,
                &coast_segs,
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
/// Surface profile around each segment, by perpendicular distance `dist`:
/// - `[0, half_width]`: `surface_full = bed_at_proj + RIVER_DEPTH_OFFSET_M`
///   (visible channel).
/// - `(half_width, bank_end]`: smoothstep down to `bed_y_pixel` — the
///   visible bank fade, matching the carve envelope so visible water
///   width = carved channel width.
/// - `(bank_end, safety_end]`: smoothstep down to
///   `bed_y_pixel − RIVER_OFF_CHANNEL_SAFETY_M`. Always clamps to
///   `depth ≤ 0` in the shader, so it's invisible at bake time. The
///   margin is a buffer against runtime height-lowering brushes (Map
///   Editor Road/dig) that would otherwise drop bed below baked surface
///   and unmask phantom water; the smooth ramp prevents a sharp on/off
///   seam at the bank if a brush straddles `bank_end`.
/// - `> safety_end`: `bed_y_pixel − SAFETY` (deep collapse).
///
/// Collapsing past the carve envelope is what stops the polyline-
/// projected `bed_at_proj` from lifting surfaceY across the whole tile
/// in low-lying regions (cliffs, deltas, estuary flats) and rendering
/// the river as a flood.
#[allow(clippy::too_many_arguments)]
fn compute_pixel(
    wx: f32,
    wz: f32,
    bed_y_pixel: f32,
    map: &GlobalMap,
    ctx: &BakeContext,
    river_segs: &[RiverSegment],
    seg_tangents: &[(f32, f32)],
    coast_segs: &[Segment],
) -> (f32, f32, f32) {
    let Some((flow_x, flow_z, idx, t, dist)) =
        weighted_flow_and_nearest(wx, wz, river_segs, seg_tangents)
    else {
        return (bed_y_pixel - RIVER_OFF_CHANNEL_SAFETY_M, 0.0, 0.0);
    };
    let seg = &river_segs[idx];

    // Surface = carved bed at the centerline projection + runtime offset.
    // Re-evaluated via the global elevation pipeline so the value is
    // independent of which tile owns the projection — in a delta wedge
    // the projection can fall outside the tile being baked.
    let proj_x = lerp(seg.ax, seg.bx, t);
    let proj_z = lerp(seg.az, seg.bz, t);
    let bed_at_proj = sample_carved_bed(map, ctx, proj_x, proj_z, river_segs, coast_segs);
    let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
    let width = lerp(seg.width_a, seg.width_b, t);
    let half_width = width * 0.5;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let surface_full = bed_at_proj + RIVER_DEPTH_OFFSET_M;
    let bank_end = half_width + taper;
    let safety_end = bank_end + taper;
    let surface_y = if dist <= bank_end {
        let s = 1.0 - smoothstep(half_width, bank_end, dist);
        bed_y_pixel + (surface_full - bed_y_pixel) * s
    } else {
        let s = smoothstep(bank_end, safety_end, dist);
        bed_y_pixel - RIVER_OFF_CHANNEL_SAFETY_M * s
    };

    (surface_y, flow_x, flow_z)
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
            bed_floor_a: 0.0,
            bed_floor_b: 0.0,
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
        // collapses to `local bed − RIVER_OFF_CHANNEL_SAFETY_M` so the
        // shader's depth-fade hides the river instead of letting it spill
        // into surrounding lower terrain — and so runtime height-lowering
        // brushes have a safety buffer before they could unmask water.
        let (map, ctx) = small_test_ctx();
        let heights = vec![5.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];
        let segs = vec![RiverSegment {
            ax: -32.0,
            az: 0.0,
            bx: 32.0,
            bz: 0.0,
            flow_norm_a: 0.5,
            flow_norm_b: 0.5,
            width_a: 4.0,
            width_b: 4.0,
            bed_floor_a: 0.0,
            bed_floor_b: 0.0,
        }];
        let bin = bake_river_field(&map, &ctx, &heights, -32.0, -32.0, &segs)
            .expect("segment present, file is written");

        let pixel_surface = |i: usize, j: usize| -> f32 {
            let off = RIVER_FIELD_HEADER_SIZE + (j * VERTS_PER_SIDE + i) * RIVER_FIELD_PIXEL_SIZE;
            let s = u16::from_le_bytes([bin[off], bin[off + 1]]);
            s as f32 * HEIGHT_STEP - HEIGHT_BIAS
        };
        // half_width = 2.0, taper = 3.0 + 7.0*0.5 = 6.5.
        // bank_end = 8.5, safety_end = 15.
        // j=32 → dist=0 (on axis), j=33 → dist=1 (inside half_width),
        // j=39 → dist=7 (visible bank), j=43 → dist=11 (safety ramp),
        // j=60 → dist=28 (deep collapse, well past safety_end).
        let on_axis = pixel_surface(32, 32);
        let inside_half_width = pixel_surface(32, 33);
        let safety_ramp = pixel_surface(32, 43);
        let far = pixel_surface(32, 60);
        // bed_at_proj is now sampled from the global elevation field at
        // the same projection point for both readings inside the
        // channel, so the surface stays flat across the channel width.
        assert!(
            (on_axis - inside_half_width).abs() < 0.01,
            "surface stays flat inside the channel: on_axis={on_axis}, inside={inside_half_width}"
        );
        assert!(
            safety_ramp < 5.0 - 0.1 && safety_ramp > 5.0 - RIVER_OFF_CHANNEL_SAFETY_M + 0.1,
            "safety ramp surface sits between local bed and bed − safety, got {safety_ramp}"
        );
        let expected_far = 5.0 - RIVER_OFF_CHANNEL_SAFETY_M;
        assert!(
            (far - expected_far).abs() < 0.1,
            "beyond safety ramp surface sits at local bed − safety ({expected_far}m), got {far}"
        );

        // Flow direction propagates to every pixel so ripples are
        // continuous. Segment runs +X so flowX≈+127.
        let off = RIVER_FIELD_HEADER_SIZE + (60 * VERTS_PER_SIDE + 32) * RIVER_FIELD_PIXEL_SIZE;
        let flow_x = bin[off + 2] as i8;
        let flow_z = bin[off + 3] as i8;
        assert!(
            flow_x > 100,
            "far pixel should still carry flowX, got {flow_x}"
        );
        assert!(flow_z.abs() < 10, "flowZ should stay near 0, got {flow_z}");
    }

    #[test]
    fn surface_continuous_across_tile_boundary() {
        // Two adjacent tiles baked with the same river polyline must
        // emit byte-identical surfaceY/flow on their shared edge.
        // Wide segments (`width_a/b = 50` ≫ natural max) force
        // `mouth_fan_bed_floor` into its fan-drop branch — the path
        // where the pre-fix bake diverged between in-tile bilinear and
        // out-of-tile fallback.
        let (map, ctx) = small_test_ctx();
        let heights = vec![5.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];
        // Polyline parallel to the tile boundary at world x=33, just
        // inside tile B (origin x=32). For pixels on tile A's right
        // edge (world x=32), the projection lands at x=33 — outside
        // tile A but inside tile B.
        let segs = vec![
            RiverSegment {
                ax: 33.0,
                az: -30.0,
                bx: 33.0,
                bz: 0.0,
                flow_norm_a: 0.8,
                flow_norm_b: 0.8,
                width_a: 50.0,
                width_b: 50.0,
                bed_floor_a: 0.0,
                bed_floor_b: 0.0,
            },
            RiverSegment {
                ax: 33.0,
                az: 0.0,
                bx: 33.0,
                bz: 30.0,
                flow_norm_a: 0.8,
                flow_norm_b: 0.8,
                width_a: 50.0,
                width_b: 50.0,
                bed_floor_a: 0.0,
                bed_floor_b: 0.0,
            },
        ];
        let bin_a = bake_river_field(&map, &ctx, &heights, -32.0, -32.0, &segs)
            .expect("non-empty segments produce a file");
        let bin_b = bake_river_field(&map, &ctx, &heights, 32.0, -32.0, &segs)
            .expect("non-empty segments produce a file");
        let last_col = VERTS_PER_SIDE - 1;
        for j in 0..VERTS_PER_SIDE {
            let a_off =
                RIVER_FIELD_HEADER_SIZE + (j * VERTS_PER_SIDE + last_col) * RIVER_FIELD_PIXEL_SIZE;
            let b_off = RIVER_FIELD_HEADER_SIZE + (j * VERTS_PER_SIDE) * RIVER_FIELD_PIXEL_SIZE;
            assert_eq!(
                &bin_a[a_off..a_off + RIVER_FIELD_PIXEL_SIZE],
                &bin_b[b_off..b_off + RIVER_FIELD_PIXEL_SIZE],
                "tile A right edge != tile B left edge at j={j}"
            );
        }
    }
}
