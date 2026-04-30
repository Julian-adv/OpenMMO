//! 65×65 heightmap sampling, encoding, and river-carve geometry.

use super::super::global_map::GlobalMap;
use super::super::noise::fbm_wrap_x;
use super::super::vector_features::{nearest_river_segment, river_segments_near_tile, RiverSegment};
use super::constants::{
    DETAIL_FREQUENCY, DETAIL_GAIN, DETAIL_LACUNARITY, DETAIL_MAX_AMPLITUDE, DETAIL_MIN_AMPLITUDE,
    DETAIL_OCTAVES, HEIGHT_BIAS, HEIGHT_STEP, HILLS_AMPLITUDE_M, HILLS_COASTAL_FADE_M,
    HILLS_FREQUENCY, HILLS_GAIN, HILLS_OCTAVES, RIVER_CARVE_DEPTH_EXTRA_M, RIVER_CARVE_DEPTH_MIN_M,
    RIVER_CARVE_MIN_BED_Y_M, RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M, TILE_DIM,
    VERTS_PER_SIDE,
};
use super::context::{BakeContext, MouthIsland};

/// Generate the 65×65 f32 heightmap. Shared between the uint16 heightmap
/// output and the splatmap slope computation (so both read identical heights).
pub(super) fn sample_tile_heights(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    river_segs: &[RiverSegment],
    mouth_islands: &[MouthIsland],
) -> Vec<f32> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let mut heights = vec![0.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    let sample_world = |wx: f32, wz: f32| {
        sample_elevation_m(
            map,
            ctx,
            wx,
            wz,
            world_size,
            inv_mpc,
            river_segs,
            mouth_islands,
        )
    };
    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let world_x = tile_origin_x + i as f32;
            let world_z = tile_origin_z + j as f32;
            heights[j * VERTS_PER_SIDE + i] = sample_world(world_x, world_z);
        }
    }
    smooth_island_area(
        &mut heights,
        tile_origin_x,
        tile_origin_z,
        mouth_islands,
        sample_world,
    );
    heights
}

/// 3×3 Gaussian approximation, σ ≈ 0.85, sum = 16.
const KERNEL: [[u32; 3]; 3] = [[1, 2, 1], [2, 4, 2], [1, 2, 1]];
const KERNEL_SUM: f32 = 16.0;

/// Convolve the 3×3 kernel against `src` centered at `(ci, cj)`. Caller
/// owns bounds checking — the kernel reads `ci±1, cj±1`.
#[inline]
fn blur3x3(src: &[f32], stride: usize, ci: usize, cj: usize) -> f32 {
    let mut acc = 0.0f32;
    for dj in 0..3 {
        for di in 0..3 {
            let ni = ci + di - 1;
            let nj = cj + dj - 1;
            acc += src[nj * stride + ni] * KERNEL[dj][di] as f32;
        }
    }
    acc / KERNEL_SUM
}

/// Two-pass 3×3 Gaussian blur applied only to vertices inside any
/// mouth-island's reach (plus a small margin to fade the surf-chop
/// wobble just beyond the bump). Two tight passes preserve the bar
/// crown better than a single wide kernel.
///
/// Seam-safe: the 2-vertex out-of-tile ring is re-sampled via
/// `sample_world`, and neighbouring tiles sampling the same world
/// positions for their own rings produce matching seam output.
fn smooth_island_area(
    heights: &mut [f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    islands: &[MouthIsland],
    sample_world: impl Fn(f32, f32) -> f32,
) {
    if islands.is_empty() {
        return;
    }
    const BLUR_EXTRA_M: f32 = 2.0;
    let mut mask = vec![false; VERTS_PER_SIDE * VERTS_PER_SIDE];
    let mut any_masked = false;
    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let wx = tile_origin_x + i as f32;
            let wz = tile_origin_z + j as f32;
            for island in islands {
                let r = island.reach_m + BLUR_EXTRA_M;
                let dx = wx - island.center[0];
                let dz = wz - island.center[1];
                if (dx * dx + dz * dz) <= r * r {
                    mask[j * VERTS_PER_SIDE + i] = true;
                    any_masked = true;
                    break;
                }
            }
        }
    }
    if !any_masked {
        return;
    }

    // Extended grid (VERTS+4)² with a 2-vertex ring of out-of-tile
    // samples so both 3×3 passes feed identical neighbourhoods on both
    // sides of a shared seam.
    const RING: usize = 2;
    const EXT: usize = VERTS_PER_SIDE + 2 * RING;
    let mut ext = vec![0.0f32; EXT * EXT];
    let vps_i32 = VERTS_PER_SIDE as i32;
    let ring_i32 = RING as i32;
    for ej in 0..EXT {
        for ei in 0..EXT {
            let i = ei as i32 - ring_i32;
            let j = ej as i32 - ring_i32;
            if i >= 0 && i < vps_i32 && j >= 0 && j < vps_i32 {
                ext[ej * EXT + ei] = heights[j as usize * VERTS_PER_SIDE + i as usize];
            } else {
                ext[ej * EXT + ei] =
                    sample_world(tile_origin_x + i as f32, tile_origin_z + j as f32);
            }
        }
    }

    // Pass 1: blur EXT into MID, leaving a 1-vertex border of blurred
    // values around the 65² region for pass 2.
    const MID: usize = VERTS_PER_SIDE + 2;
    let mut mid = vec![0.0f32; MID * MID];
    for mj in 0..MID {
        for mi in 0..MID {
            mid[mj * MID + mi] = blur3x3(&ext, EXT, mi + 1, mj + 1);
        }
    }

    // Pass 2: blur MID into masked positions of `heights`.
    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            if !mask[j * VERTS_PER_SIDE + i] {
                continue;
            }
            heights[j * VERTS_PER_SIDE + i] = blur3x3(&mid, MID, i + 1, j + 1);
        }
    }
}

/// Sample the baked elevation at a single world point — same pipeline as
/// `sample_tile_heights` but for one isolated query (e.g. bridge bank
/// probes). Pulls only the river segments within a small AABB around the
/// probe so the cost stays O(handful) regardless of world size. Mouth-
/// island bumps are skipped: bank probes sit past the channel edge, well
/// outside any island reach.
pub(super) fn sample_height_single(map: &GlobalMap, ctx: &BakeContext, wx: f32, wz: f32) -> f32 {
    use super::constants::{
        RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M, RIVER_FADE_SPAN_M,
        RIVER_MAX_WIDTH_M, RIVER_SAND_WIDTH_MULT,
    };
    let max_half = RIVER_MAX_WIDTH_M * 0.5;
    let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
    let max_sand_half = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
    let river_margin = (max_half + max_taper).max(max_sand_half + RIVER_FADE_SPAN_M);
    let segs = river_segments_near_tile(&ctx.rivers_world, wx, wz, wx, wz, river_margin);
    let world_size = map.config.world_size_m as f32;
    let inv_mpc = 1.0 / map.config.meters_per_cell();
    sample_elevation_m(map, ctx, wx, wz, world_size, inv_mpc, &segs, &[])
}

pub(super) fn encode_heightmap(heights: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(heights.len() * 2);
    for &h in heights {
        let v = ((h + HEIGHT_BIAS) / HEIGHT_STEP)
            .round()
            .clamp(0.0, 65535.0) as u16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// Bilinear-sample the global elevation at a world position, convert sea
/// cells into a shallow bathymetry curve, add high-frequency detail, and
/// subtract a polyline-distance river carve.
fn sample_elevation_m(
    map: &GlobalMap,
    ctx: &BakeContext,
    world_x: f32,
    world_z: f32,
    world_size: f32,
    inv_mpc: f32,
    river_segs: &[RiverSegment],
    mouth_islands: &[MouthIsland],
) -> f32 {
    // Catmull-Rom (C1-continuous bicubic) instead of bilinear here: the 8 m
    // global cells are too coarse to describe a smooth hill, and bilinear's
    // per-cell derivative jump makes isolated tall cells read as pyramidal
    // cones at 1 m tile resolution. Splat-side fields (coast distance) still
    // use bilinear; bicubic overshoot at a sharp land/sea transition would
    // distort the shoreline.
    let base = catmull_rom_wrap_x(map, world_x, world_z, world_size, inv_mpc, |i| {
        cell_elevation_m(map, &ctx.dist_to_land, i)
    });

    // Amplitude scales with relative elevation so plains stay calm and peaks
    // feel jagged. Underwater damped heavily so the water surface looks flat.
    let max_elev = map.config.max_elevation_m.max(1.0);
    let amp_t = (base.max(0.0) / max_elev).clamp(0.0, 1.0);
    let amp = DETAIL_MIN_AMPLITUDE + (DETAIL_MAX_AMPLITUDE - DETAIL_MIN_AMPLITUDE) * amp_t;
    let underwater_damp = if base < 0.0 { 0.15 } else { 1.0 };

    // Detail sampled with X-wrap so the seamless continent carries through.
    let n = fbm_wrap_x(
        &ctx.detail_noise,
        world_x + world_size * 0.5,
        world_z + world_size * 0.5,
        world_size,
        DETAIL_FREQUENCY,
        DETAIL_OCTAVES,
        DETAIL_LACUNARITY,
        DETAIL_GAIN,
    );
    let detail = n * amp * underwater_damp;

    // Universal rolling hills, land only — bathymetry should stay flat.
    // Amplitude fades in over the first `HILLS_COASTAL_FADE_M` meters of base
    // elevation so the symmetric noise can't pull 1-2 m coastal land below
    // sea level and trap water in lagoons inland of the shoreline.
    let hills = if base >= 0.0 {
        let hn = fbm_wrap_x(
            &ctx.detail_noise,
            world_x + world_size * 0.5,
            world_z + world_size * 0.5,
            world_size,
            HILLS_FREQUENCY,
            HILLS_OCTAVES,
            DETAIL_LACUNARITY,
            HILLS_GAIN,
        );
        let coastal_damp = (base / HILLS_COASTAL_FADE_M).clamp(0.0, 1.0);
        hn * HILLS_AMPLITUDE_M * coastal_damp
    } else {
        0.0
    };

    // Cap the carve depth at the source so the taper profile stays smooth
    // across its full width even in shallow areas. Without this — capping
    // the *final* carve after taper — the cap floor (0.3m above
    // `RIVER_CARVE_MIN_BED_Y_M`) clamps the channel center AND the entire
    // taper band to the same bedHeight near the estuary, killing the
    // lateral depth gradient the river shader relies on for edge fade.
    // Capping `depth` BEFORE `river_carve_m` shrinks the whole profile
    // proportionally — channel still bottoms out at the floor, but the
    // taper rises smoothly back to natural ground over its full width.
    let pre_carve = base + detail + hills;
    let max_carve_depth = (pre_carve - RIVER_CARVE_MIN_BED_Y_M).max(0.0);
    let carve = if let Some((d, idx, t)) = nearest_river_segment(world_x, world_z, river_segs) {
        let seg = &river_segs[idx];
        let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
        let width = lerp(seg.width_a, seg.width_b, t);
        let (half_width, taper, depth) = segment_carve_params(flow_norm, width);
        let local_depth = depth.min(max_carve_depth);
        river_carve_m(d, half_width, taper, local_depth)
    } else {
        0.0
    };

    // Sandy finger-islands in the delta. Applied *after* the carve
    // subtraction so the carve can't immediately re-cut the bump back
    // out — island peak rides on top of the post-carve bed floor
    // (`RIVER_CARVE_MIN_BED_Y_M = 0.3 m` near the mouth). Max over
    // overlapping islands rather than sum so adjacent bumps merge into
    // one larger bar instead of stacking into spikes.
    let mut island_bump = 0.0f32;
    for island in mouth_islands {
        if (world_x - island.center[0]).abs() > island.reach_m
            || (world_z - island.center[1]).abs() > island.reach_m
        {
            continue;
        }
        let h = island.bump_m(world_x, world_z);
        if h > island_bump {
            island_bump = h;
        }
    }

    let max_cap = map.config.max_elevation_m;
    (pre_carve - carve + island_bump).clamp(-HEIGHT_BIAS, max_cap)
}

#[inline]
pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Carve geometry at a point on the river: `(half_width, taper, depth)`.
/// Flat bed matches the visible ribbon (`half_width = width * 0.5`) so the
/// water surface sits on a consistent floor. Taper and depth grow linearly
/// in flow so sources are shallow and mouths cut deeper. See
/// RIVER_SYSTEM.md §2.5.
#[inline]
fn segment_carve_params(flow_norm: f32, width_m: f32) -> (f32, f32, f32) {
    let half_width = width_m * 0.5;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let depth = RIVER_CARVE_DEPTH_MIN_M + RIVER_CARVE_DEPTH_EXTRA_M * flow_norm;
    (half_width, taper, depth)
}

/// River channel profile: flat floor within `half_width`, smoothstep taper
/// to zero over the next `taper` meters. Flat floor avoids a kink at the
/// bank.
#[inline]
fn river_carve_m(d_m: f32, half_width: f32, taper: f32, depth: f32) -> f32 {
    let total = half_width + taper;
    if d_m >= total {
        return 0.0;
    }
    if d_m <= half_width {
        return depth;
    }
    let t = (d_m - half_width) / taper.max(1e-3);
    let s = 1.0 - t * t * (3.0 - 2.0 * t);
    depth * s
}

/// Map a single global cell to "effective elevation": the raw meters for
/// land, or a shallow negative bathymetry for sea (deeper offshore, capped
/// so depth ramps 0.5 m at the shore up to ~10 m far offshore). Shared by
/// every coarse-grid elevation sampler so all paths agree on the
/// shoreline bathymetry curve.
pub(super) fn cell_elevation_m(map: &GlobalMap, dist_to_land: &[u16], i: usize) -> f32 {
    if map.land_mask[i] == 1 {
        map.elevation_m[i]
    } else {
        let d = dist_to_land[i] as f32;
        -(0.5 + d.min(40.0) * 0.25)
    }
}

/// One-axis Catmull-Rom basis at parameter `t ∈ [0, 1]` between `p1` and `p2`,
/// with `p0` and `p3` as shoulder samples. Passes through `p1` at t=0 and `p2`
/// at t=1 with matching tangents on either side, so stitching adjacent cells
/// is C1-continuous — no per-cell gradient jump.
#[inline]
fn catmull_rom_1d(p0: f32, p1: f32, p2: f32, p3: f32, t: f32) -> f32 {
    let a = -0.5 * p0 + 1.5 * p1 - 1.5 * p2 + 0.5 * p3;
    let b = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
    let c = -0.5 * p0 + 0.5 * p2;
    let d = p1;
    ((a * t + b) * t + c) * t + d
}

/// Fractional global-cell coordinates for world position `(wx, wz)`: the
/// integer cell that contains it plus the sub-cell fractions `fx, fy ∈ [0, 1]`.
/// Y is clamped to `[0, res-1]` so top/bottom borders stay on-grid; X is
/// returned as a raw (possibly negative) `i32` since callers wrap it into the
/// cell array themselves via `rem_euclid(res)`. Shared by every fractional
/// sampler so the two must stay in lockstep — diverging on `- 0.5` or the
/// clamp between bilinear and bicubic would desync elevation from splat.
#[inline]
fn fractional_cell_coords(
    map: &GlobalMap,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
) -> (i32, i32, i32, f32, f32) {
    let res = map.config.global_res as i32;
    let res_f = res as f32;
    let gx_f = (wx + world_size * 0.5) * inv_mpc - 0.5;
    let gy_f = ((wz + world_size * 0.5) * inv_mpc - 0.5).clamp(0.0, res_f - 1.0);
    let gx0 = gx_f.floor() as i32;
    let gy0 = gy_f.floor() as i32;
    (res, gx0, gy0, gx_f - gx0 as f32, gy_f - gy0 as f32)
}

/// Catmull-Rom bicubic sample of a cell-indexed scalar field. X wraps,
/// Z clamps. Reads a 4×4 neighborhood around the fractional position, so
/// Y-border cells collapse shoulders onto the clamped row (still smooth,
/// degrades toward linear near the top/bottom edge of the world).
fn catmull_rom_wrap_x<F: Fn(usize) -> f32>(
    map: &GlobalMap,
    wx: f32,
    wz: f32,
    world_size: f32,
    inv_mpc: f32,
    f: F,
) -> f32 {
    let (res, gx0, gy0, fx, fy) = fractional_cell_coords(map, wx, wz, world_size, inv_mpc);
    let ix = |x: i32| x.rem_euclid(res) as usize;
    let iy = |y: i32| y.clamp(0, res - 1) as usize;
    let idx = |x: usize, y: usize| y * res as usize + x;
    let sample = |ox: i32, oy: i32| f(idx(ix(gx0 + ox), iy(gy0 + oy)));

    let mut rows = [0.0f32; 4];
    for (k, oy) in [-1i32, 0, 1, 2].into_iter().enumerate() {
        let p0 = sample(-1, oy);
        let p1 = sample(0, oy);
        let p2 = sample(1, oy);
        let p3 = sample(2, oy);
        rows[k] = catmull_rom_1d(p0, p1, p2, p3, fx);
    }
    catmull_rom_1d(rows[0], rows[1], rows[2], rows[3], fy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catmull_rom_passes_through_control_points() {
        // At t=0 the basis must return p1 exactly; at t=1 it must return p2.
        // This is the property that lets adjacent cells stitch without a
        // value jump — losing it would create visible step artifacts along
        // every cell boundary.
        for (p0, p1, p2, p3) in [
            (0.0, 1.0, 2.0, 3.0),
            (-5.0, 10.0, -3.0, 7.5),
            (100.0, 100.0, 100.0, 100.0),
        ] {
            assert!((catmull_rom_1d(p0, p1, p2, p3, 0.0) - p1).abs() < 1e-5);
            assert!((catmull_rom_1d(p0, p1, p2, p3, 1.0) - p2).abs() < 1e-5);
        }
    }

    #[test]
    fn catmull_rom_preserves_constant_field() {
        // A constant 1D field must stay constant at any t — no overshoot from
        // floating-point drift in the basis coefficients.
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let v = catmull_rom_1d(4.2, 4.2, 4.2, 4.2, t);
            assert!((v - 4.2).abs() < 1e-5, "constant field at t={t}: {v}");
        }
    }

    #[test]
    fn catmull_rom_reproduces_linear_ramp() {
        // Catmull-Rom through 4 samples of a line must reproduce the line
        // exactly (the cubic collapses to degree 1). If any basis coefficient
        // is off, a gentle slope in the global map would pick up spurious
        // wiggles at 1 m tile vertices — the opposite of what this change is
        // supposed to do.
        let a = 3.0;
        let b = 1.5;
        let (p0, p1, p2, p3) = (a - b, a, a + b, a + 2.0 * b);
        for t in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            let expected = a + b * t;
            let got = catmull_rom_1d(p0, p1, p2, p3, t);
            assert!(
                (got - expected).abs() < 1e-4,
                "linear ramp at t={t}: got {got}, want {expected}"
            );
        }
    }

    #[test]
    fn catmull_rom_basis_is_symmetric() {
        // Tension-0.5 Catmull-Rom is direction-agnostic:
        // `f(p0,p1,p2,p3,t) == f(p3,p2,p1,p0,1-t)`. The sampler feeds a splat
        // classifier that treats +X and -X the same; asymmetric basis would
        // silently bias elevation one way along world axes.
        for (p0, p1, p2, p3) in [(0.0, 1.0, 4.0, 9.0), (-3.0, 2.0, -1.0, 5.0)] {
            for t in [0.0, 0.3, 0.5, 0.7, 1.0] {
                let fwd = catmull_rom_1d(p0, p1, p2, p3, t);
                let bwd = catmull_rom_1d(p3, p2, p1, p0, 1.0 - t);
                assert!(
                    (fwd - bwd).abs() < 1e-5,
                    "asymmetric at t={t}: fwd={fwd} bwd={bwd}"
                );
            }
        }
    }

    #[test]
    fn catmull_rom_c1_continuous_across_windows() {
        // The motivation for switching from bilinear to bicubic: sliding the
        // 4-sample window by one cell must preserve the derivative at the
        // shared vertex (left window at t→1 ≡ right window at t→0). If this
        // regresses, per-cell slope jumps return and the 8 m lattice reads
        // as pyramidal hills again — the whole bug this change fixed.
        let samples = [0.0f32, 1.0, 3.0, 2.5, 4.0];
        let eps = 1e-3;
        let left = catmull_rom_1d(samples[0], samples[1], samples[2], samples[3], 1.0);
        let left_prev = catmull_rom_1d(samples[0], samples[1], samples[2], samples[3], 1.0 - eps);
        let right = catmull_rom_1d(samples[1], samples[2], samples[3], samples[4], 0.0);
        let right_next = catmull_rom_1d(samples[1], samples[2], samples[3], samples[4], eps);
        // Value continuity at the shared vertex (both = samples[3] = p2-of-left = p1-of-right).
        assert!(
            (left - right).abs() < 1e-5,
            "c0 value mismatch: {left} vs {right}"
        );
        // Derivative continuity via finite difference.
        let left_slope = (left - left_prev) / eps;
        let right_slope = (right_next - right) / eps;
        assert!(
            (left_slope - right_slope).abs() < 1e-2,
            "c1 slope mismatch: left={left_slope} right={right_slope}"
        );
    }
}
