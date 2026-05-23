//! 65×65 heightmap sampling, encoding, and river-carve geometry.

use super::super::global_map::GlobalMap;
use super::super::noise::{fbm_wrap_x, smoothstep};
use super::super::vector_features::{nearest_river_segment, RiverSegment};
use super::constants::{
    DETAIL_COAST_DAMP, DETAIL_FREQUENCY, DETAIL_GAIN, DETAIL_LACUNARITY, DETAIL_MAX_AMPLITUDE,
    DETAIL_MIN_AMPLITUDE, DETAIL_OCTAVES, HEIGHT_BIAS, HEIGHT_STEP, HILLS_AMPLITUDE_M,
    HILLS_COASTAL_FADE_M, HILLS_FREQUENCY, HILLS_GAIN, HILLS_OCTAVES, LAND_BASE_MIN_Y_M,
    RIVER_CARVE_DEPTH_EXTRA_M, RIVER_CARVE_DEPTH_MIN_M, RIVER_CARVE_MIN_BED_Y_M,
    RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M, RIVER_MAX_WIDTH_M,
    RIVER_MOUTH_BRANCH_BED_Y_M, RIVER_MOUTH_BRANCH_TAPER_M, RIVER_MOUTH_FAN_BED_DROP_M,
    RIVER_MOUTH_FAN_EXTRA, TILE_DIM, VERTS_PER_SIDE,
};
use super::context::BakeContext;

const RIVER_BEND_TURN_FULL_STRENGTH_RAD: f32 = std::f32::consts::FRAC_PI_4;
const RIVER_BEND_OUTER_DEPTH_EXTRA: f32 = 0.30;
const RIVER_BEND_INNER_DEPTH_REDUCTION: f32 = 0.35;

/// Generate the 65×65 f32 heightmap WITHOUT the river carve. Splitting the
/// carve out into a separate pass (`apply_river_carve_to_tile`) lets later
/// stages (settlement pad flatten) modify the natural surface first, so
/// the river still cuts through whatever pad sits above it. Caller must
/// follow up with `apply_river_carve_to_tile` to get the final heights.
pub(super) fn sample_tile_heights_no_carve(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
) -> Vec<f32> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let mut heights = vec![0.0f32; VERTS_PER_SIDE * VERTS_PER_SIDE];

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let world_x = tile_origin_x + i as f32;
            let world_z = tile_origin_z + j as f32;
            heights[j * VERTS_PER_SIDE + i] =
                sample_elevation_no_carve(map, ctx, world_x, world_z, world_size, inv_mpc);
        }
    }
    heights
}

/// Per-vertex river carve subtraction. Runs after settlement flattening
/// so the carve cuts through the settlement pad too — the channel keeps
/// its natural depth even where the pad raised the surrounding terrain.
pub(super) fn apply_river_carve_to_tile(
    heights: &mut [f32],
    map: &GlobalMap,
    tile_origin_x: f32,
    tile_origin_z: f32,
    river_segs: &[RiverSegment],
) {
    if river_segs.is_empty() {
        return;
    }
    let max_cap = map.config.max_elevation_m;
    for j in 0..VERTS_PER_SIDE {
        for i in 0..VERTS_PER_SIDE {
            let wx = tile_origin_x + i as f32;
            let wz = tile_origin_z + j as f32;
            let idx = j * VERTS_PER_SIDE + i;
            let natural = heights[idx];
            let carve = carve_at_point(wx, wz, natural, river_segs);
            heights[idx] = (natural - carve).clamp(-HEIGHT_BIAS, max_cap);
        }
    }
}

/// Sample the natural (un-carved) ground surface at a single world point —
/// base + detail + hills, no river carve. Used by
/// bridge deck-end probes (those points can fall *inside* the river carve
/// taper, where the carved bank reads several meters below the natural
/// surface the bridge actually wants to sit on) and by the settlement-pad
/// `target_y` so a pad center that happens to land near a wide river still
/// resolves to land elevation rather than the river's carved bed.
pub(super) fn sample_natural_height_single(
    map: &GlobalMap,
    ctx: &BakeContext,
    wx: f32,
    wz: f32,
) -> f32 {
    let world_size = map.config.world_size_m as f32;
    let inv_mpc = 1.0 / map.config.meters_per_cell();
    sample_elevation_no_carve(map, ctx, wx, wz, world_size, inv_mpc)
}

pub(super) fn probe_point_impl(
    map: &GlobalMap,
    ctx: &BakeContext,
    wx: f32,
    wz: f32,
) -> super::PointProbe {
    use super::super::vector_features::river_segments_near_tile;

    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let res = cfg.global_res as i32;
    let gx = (((wx + world_size * 0.5) * inv_mpc).floor() as i32).rem_euclid(res);
    let gy = (((wz + world_size * 0.5) * inv_mpc).floor() as i32).clamp(0, res - 1);
    let cell_idx = gy as usize * res as usize + gx as usize;

    let natural = sample_elevation_no_carve(map, ctx, wx, wz, world_size, inv_mpc);
    let segs = river_segments_near_tile(&ctx.rivers_world, wx, wz, wx, wz, super::river_margin_m());
    let river = carve_at_point_detailed(wx, wz, natural, &segs);
    let carve = river.map(|n| n.carve).unwrap_or(0.0);
    let final_height = (natural - carve).clamp(-HEIGHT_BIAS, cfg.max_elevation_m);

    super::PointProbe {
        world_x: wx,
        world_z: wz,
        global_cell: (gx, gy),
        land_mask: map.land_mask[cell_idx],
        dist_to_land: ctx.dist_to_land[cell_idx],
        natural_height: natural,
        final_height,
        river,
    }
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

/// Same pipeline as `sample_elevation_m` but without the river carve —
/// returns the natural surface (base + detail + hills).
/// The carve is applied as a separate per-vertex pass so settlement pads
/// can raise the surface first; the carve then cuts through whatever
/// height ended up above the river polyline.
fn sample_elevation_no_carve(
    map: &GlobalMap,
    ctx: &BakeContext,
    world_x: f32,
    world_z: f32,
    world_size: f32,
    inv_mpc: f32,
) -> f32 {
    let base_raw = catmull_rom_wrap_x(map, world_x, world_z, world_size, inv_mpc, |i| {
        cell_elevation_m(map, &ctx.dist_to_land, i)
    });
    // Catmull-Rom across the coast can overshoot the adjacent sea cell's
    // negative bathymetry into a vertex inside a land cell, so use the
    // cell-center mask query — not `base_raw >= 0` — as the land test.
    let mi = containing_cell_index(map, world_x, world_z, world_size, inv_mpc);
    let base = if map.land_mask[mi] == 1 {
        base_raw.max(LAND_BASE_MIN_Y_M)
    } else {
        base_raw
    };
    let max_elev = map.config.max_elevation_m.max(1.0);
    let amp_t = (base.max(0.0) / max_elev).clamp(0.0, 1.0);
    let amp = DETAIL_MIN_AMPLITUDE + (DETAIL_MAX_AMPLITUDE - DETAIL_MIN_AMPLITUDE) * amp_t;
    let inland_t = coastal_inland_t(base);
    let underwater_damp = if base < 0.0 {
        0.15
    } else {
        DETAIL_COAST_DAMP + (1.0 - DETAIL_COAST_DAMP) * inland_t
    };
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
        hn * HILLS_AMPLITUDE_M * inland_t
    } else {
        0.0
    };

    let max_cap = map.config.max_elevation_m;
    (base + detail + hills).clamp(-HEIGHT_BIAS, max_cap)
}

/// Carved bed elevation at `(wx, wz)` using the caller's pre-filtered
/// river-segment list — same formula that produced the tile heightmap.
/// `river_field` calls this for each centerline projection so the value
/// is independent of which tile owns the projection, preventing a
/// `surfaceY` step at delta tile boundaries.
#[inline]
pub(super) fn sample_carved_bed(
    map: &GlobalMap,
    ctx: &BakeContext,
    wx: f32,
    wz: f32,
    river_segs: &[RiverSegment],
) -> f32 {
    let world_size = map.config.world_size_m as f32;
    let inv_mpc = 1.0 / map.config.meters_per_cell();
    sample_elevation_m(map, ctx, wx, wz, world_size, inv_mpc, river_segs)
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
) -> f32 {
    let natural = sample_elevation_no_carve(map, ctx, world_x, world_z, world_size, inv_mpc);
    let carve = carve_at_point(world_x, world_z, natural, river_segs);
    let max_cap = map.config.max_elevation_m;
    (natural - carve).clamp(-HEIGHT_BIAS, max_cap)
}

/// River carve depth (m, ≥0) to subtract from `current_h` at the given world
/// point. Cap on `depth` (rather than the post-taper carve) keeps the
/// channel floor at `RIVER_CARVE_MIN_BED_Y_M` while preserving the lateral
/// taper gradient the river shader needs for edge fade. Inside the mouth
/// fan zone (detected via width excess over the natural max), the floor
/// drops below sea level so the channel reads as shallow-sea bathymetry
/// in the estuary fan.
#[inline]
fn carve_at_point(world_x: f32, world_z: f32, current_h: f32, segs: &[RiverSegment]) -> f32 {
    carve_at_point_detailed(world_x, world_z, current_h, segs)
        .map(|n| n.carve)
        .unwrap_or(0.0)
}

/// Same carve as `carve_at_point` but also returns the intermediate values
/// (segment match, width, taper, bed floor, etc.) for diagnostics. Used by
/// `probe_point_impl` so the CLI breakdown stays in lockstep with the carve
/// the bake actually applied. Production hot path calls `carve_at_point`
/// which discards the detail struct (LLVM elides the unused fields).
#[inline]
fn carve_at_point_detailed(
    world_x: f32,
    world_z: f32,
    current_h: f32,
    segs: &[RiverSegment],
) -> Option<super::NearestRiver> {
    let (d, idx, t) = nearest_river_segment(world_x, world_z, segs)?;
    let seg = &segs[idx];
    let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
    let width = lerp(seg.width_a, seg.width_b, t);
    let (half_width, natural_taper, depth) = segment_carve_params(flow_norm, width);
    let bed_floor = segment_bed_floor(seg, width, t);
    let taper = branch_aware_taper(natural_taper, bed_floor);
    let max_carve_depth = (current_h - bed_floor).max(0.0);
    let signed_d = signed_distance_to_segment(world_x, world_z, seg, t);
    let outside_strength = bend_outside_strength(segs, idx, t);
    let carve = river_carve_capped_m(
        d,
        signed_d,
        outside_strength,
        half_width,
        taper,
        depth,
        max_carve_depth,
    );
    Some(super::NearestRiver {
        seg_idx: idx,
        t,
        d_m: d,
        signed_d_m: signed_d,
        width,
        flow_norm,
        half_width,
        taper,
        depth_uncapped: depth,
        bed_floor,
        max_carve_depth,
        carve,
    })
}

/// Distributary branches replace the gentle 3–10 m bank with a steeper
/// ~5 m cut as the bed sinks sub-sea — a 0.15 m channel ringed by a 10 m
/// slope looks like a crater. Lerps on `bed_floor` so the apex
/// (`bed_floor ≈ 0`) keeps the natural bank and only the seaward end cuts
/// steep.
#[inline]
fn branch_aware_taper(natural_taper: f32, bed_floor: f32) -> f32 {
    let branch_t = smoothstep(0.0, -RIVER_MOUTH_BRANCH_BED_Y_M, -bed_floor);
    lerp(natural_taper, RIVER_MOUTH_BRANCH_TAPER_M, branch_t)
}

/// Effective carve taper (m) at parametric position `t` along a segment.
/// Natural reaches use `RIVER_CARVE_TAPER_MIN + flow_norm × EXTRA`; segments
/// whose `segment_bed_floor` dips sub-sea (i.e. distributary branches) lerp
/// toward the steeper `RIVER_MOUTH_BRANCH_TAPER_M` so the bank cut matches
/// what the heightmap actually carves. The full surface→bed collapse
/// envelope is `half_width + taper` on each side, but the smoothstep in
/// `river_field::compute_pixel` puts the *visible* water boundary near
/// `half_width + 0.5·taper`, so bridge placement uses `width + taper`
/// as the visible-water span.
#[inline]
pub(super) fn segment_carve_taper_at(seg: &RiverSegment, t: f32) -> f32 {
    let flow_norm = lerp(seg.flow_norm_a, seg.flow_norm_b, t);
    let natural_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let width = lerp(seg.width_a, seg.width_b, t);
    let bed_floor = segment_bed_floor(seg, width, t);
    branch_aware_taper(natural_taper, bed_floor)
}

/// Bed-floor target in meters for a river segment. Natural widths return
/// `RIVER_CARVE_MIN_BED_Y_M`; widened mouth fans can drop below sea level,
/// and tapering distributary branches carry a per-vertex floor that eases
/// from sea level toward a shallow sub-sea floor so the channel reaches the
/// ocean cleanly.
#[inline]
fn segment_bed_floor(seg: &RiverSegment, width_m: f32, t: f32) -> f32 {
    let excess = (width_m - RIVER_MAX_WIDTH_M).max(0.0);
    let max_excess = RIVER_MAX_WIDTH_M * RIVER_MOUTH_FAN_EXTRA;
    let fan_frac = (excess / max_excess.max(1e-3)).clamp(0.0, 1.0);
    let fan_floor = RIVER_CARVE_MIN_BED_Y_M - RIVER_MOUTH_FAN_BED_DROP_M * fan_frac;
    // Natural rivers have both endpoints at 0 — skip the lerp/min on the hot
    // path. fan_floor is always ≤ 0, so min(fan_floor, 0.0) == fan_floor.
    if seg.bed_floor_a == 0.0 && seg.bed_floor_b == 0.0 {
        return fan_floor;
    }
    let branch_floor = lerp(seg.bed_floor_a, seg.bed_floor_b, t);
    fan_floor.min(branch_floor)
}

#[inline]
pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear global-cell index for the cell that *contains* `(world_x,
/// world_z)` (X wraps, Y clamps). Distinct from `fractional_cell_coords`
/// which shifts by −0.5 for bilinear pixel-center sampling — callers
/// here want a discrete integer cell to look up scalar fields like
/// `land_mask` or `elevation_m`.
#[inline]
pub(super) fn containing_cell_index(
    map: &GlobalMap,
    world_x: f32,
    world_z: f32,
    world_size: f32,
    inv_mpc: f32,
) -> usize {
    let res = map.config.global_res as i32;
    let gx = (((world_x + world_size * 0.5) * inv_mpc).floor() as i32).rem_euclid(res);
    let gy = (((world_z + world_size * 0.5) * inv_mpc).floor() as i32).clamp(0, res - 1);
    gy as usize * res as usize + gx as usize
}

/// Coastal fade ramp shared by detail-noise and hills synthesis: `0`
/// when `base ≤ 0`, `1` past `HILLS_COASTAL_FADE_M`, smooth elsewhere.
#[inline]
fn coastal_inland_t(base: f32) -> f32 {
    (base / HILLS_COASTAL_FADE_M).clamp(0.0, 1.0)
}

/// Carve geometry at a point on the river: `(half_width, taper, depth)`.
/// The base bed width matches the visible ribbon (`half_width = width * 0.5`).
/// Taper and depth grow linearly in flow so sources are shallow and mouths
/// cut deeper. See RIVER_SYSTEM.md §2.5.
#[inline]
fn segment_carve_params(flow_norm: f32, width_m: f32) -> (f32, f32, f32) {
    let half_width = width_m * 0.5;
    let taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M * flow_norm;
    let depth = RIVER_CARVE_DEPTH_MIN_M + RIVER_CARVE_DEPTH_EXTRA_M * flow_norm;
    (half_width, taper, depth)
}

/// River channel profile: straight reaches keep a flat floor within
/// `half_width`; bends bias that floor so the outside bank cuts a little
/// deeper while the inside bank shelves shallower. The outer taper then
/// smoothsteps to zero over the next `taper` meters.
#[inline]
fn river_carve_capped_m(
    d_m: f32,
    signed_d_m: f32,
    outside_strength: f32,
    half_width: f32,
    taper: f32,
    depth: f32,
    max_depth: f32,
) -> f32 {
    let depth = bend_biased_depth(depth, signed_d_m, outside_strength, half_width).min(max_depth);
    river_carve_profile_m(d_m, half_width, taper, depth)
}

#[inline]
fn river_carve_profile_m(d_m: f32, half_width: f32, taper: f32, depth: f32) -> f32 {
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

/// Bend strength is signed by outside-bank direction: + means the left side
/// of the downstream segment is outside, - means the right side is outside.
fn bend_outside_strength(segs: &[RiverSegment], idx: usize, t: f32) -> f32 {
    let seg = &segs[idx];
    let start = idx
        .checked_sub(1)
        .and_then(|prev_idx| {
            let prev = &segs[prev_idx];
            segments_touch(prev.bx, prev.bz, seg.ax, seg.az)
                .then(|| -turn_strength(prev.ax, prev.az, prev.bx, prev.bz, seg.bx, seg.bz))
        })
        .unwrap_or(0.0);
    let end = segs
        .get(idx + 1)
        .and_then(|next| {
            segments_touch(seg.bx, seg.bz, next.ax, next.az)
                .then(|| -turn_strength(seg.ax, seg.az, seg.bx, seg.bz, next.bx, next.bz))
        })
        .unwrap_or(0.0);
    lerp(start, end, t).clamp(-1.0, 1.0)
}

fn turn_strength(ax: f32, az: f32, bx: f32, bz: f32, cx: f32, cz: f32) -> f32 {
    let ux = bx - ax;
    let uz = bz - az;
    let vx = cx - bx;
    let vz = cz - bz;
    let ul = (ux * ux + uz * uz).sqrt();
    let vl = (vx * vx + vz * vz).sqrt();
    if ul <= 1e-3 || vl <= 1e-3 {
        return 0.0;
    }
    let ux = ux / ul;
    let uz = uz / ul;
    let vx = vx / vl;
    let vz = vz / vl;
    let cross = ux * vz - uz * vx;
    let dot = (ux * vx + uz * vz).clamp(-1.0, 1.0);
    (cross.atan2(dot) / RIVER_BEND_TURN_FULL_STRENGTH_RAD).clamp(-1.0, 1.0)
}

fn segments_touch(ax: f32, az: f32, bx: f32, bz: f32) -> bool {
    let dx = ax - bx;
    let dz = az - bz;
    dx * dx + dz * dz <= 1e-4
}

fn signed_distance_to_segment(px: f32, pz: f32, seg: &RiverSegment, t: f32) -> f32 {
    let dx = seg.bx - seg.ax;
    let dz = seg.bz - seg.az;
    let len = (dx * dx + dz * dz).sqrt();
    if len <= 1e-3 {
        return 0.0;
    }
    let cx = seg.ax + dx * t;
    let cz = seg.az + dz * t;
    let cross = dx * (pz - cz) - dz * (px - cx);
    cross / len
}

fn bend_biased_depth(depth: f32, signed_d_m: f32, outside_strength: f32, half_width: f32) -> f32 {
    if half_width <= 1e-3 || outside_strength.abs() <= 1e-3 {
        return depth;
    }
    let lateral = (signed_d_m / half_width).clamp(-1.0, 1.0);
    let outside_alignment = (lateral * outside_strength).clamp(-1.0, 1.0);
    if outside_alignment >= 0.0 {
        depth * (1.0 + RIVER_BEND_OUTER_DEPTH_EXTRA * outside_alignment)
    } else {
        depth * (1.0 + RIVER_BEND_INNER_DEPTH_REDUCTION * outside_alignment)
    }
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

    #[test]
    fn straight_river_carve_keeps_flat_bed() {
        let depth = 2.0;
        let carve = river_carve_capped_m(3.0, 3.0, 0.0, 5.0, 4.0, depth, f32::INFINITY);
        assert!(
            (carve - depth).abs() < 1e-5,
            "straight channel should keep the legacy flat-bed depth"
        );
    }

    #[test]
    fn bend_carve_deepens_outside_and_shallows_inside_bank() {
        let depth = 2.0;
        let half_width = 5.0;
        let outside_strength = 1.0;
        let outer = river_carve_capped_m(
            half_width,
            half_width,
            outside_strength,
            half_width,
            4.0,
            depth,
            f32::INFINITY,
        );
        let center = river_carve_capped_m(
            0.0,
            0.0,
            outside_strength,
            half_width,
            4.0,
            depth,
            f32::INFINITY,
        );
        let inner = river_carve_capped_m(
            half_width,
            -half_width,
            outside_strength,
            half_width,
            4.0,
            depth,
            f32::INFINITY,
        );

        assert!(outer > center, "outside bank should cut below base depth");
        assert!(
            inner < center,
            "inside bank should become shallower toward the point bar"
        );
    }

    #[test]
    fn river_depth_cap_preserves_taper_gradient() {
        let carve = river_carve_capped_m(6.0, 0.0, 0.0, 5.0, 5.0, 2.0, 1.0);
        assert!(
            carve < 1.0,
            "cap should limit bed depth before taper so the edge still fades"
        );
    }

    #[test]
    fn bend_direction_follows_polyline_turn() {
        let segs = vec![
            RiverSegment {
                ax: 0.0,
                az: -10.0,
                bx: 0.0,
                bz: 0.0,
                flow_norm_a: 0.0,
                flow_norm_b: 0.0,
                width_a: 4.0,
                width_b: 4.0,
                bed_floor_a: 0.0,
                bed_floor_b: 0.0,
            },
            RiverSegment {
                ax: 0.0,
                az: 0.0,
                bx: 10.0,
                bz: 0.0,
                flow_norm_a: 0.0,
                flow_norm_b: 0.0,
                width_a: 4.0,
                width_b: 4.0,
                bed_floor_a: 0.0,
                bed_floor_b: 0.0,
            },
        ];

        let after_right_turn = bend_outside_strength(&segs, 1, 0.0);
        assert!(
            after_right_turn > 0.99,
            "right turn should put the outside bank on the left side"
        );
    }
}
