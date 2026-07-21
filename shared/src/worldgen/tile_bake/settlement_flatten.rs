//! Per-tile heightmap flatten around each settlement.
//!
//! Every settlement gets a circular flat pad on the heightmap so houses sit
//! on level ground regardless of the underlying hills/slope. Splatmap is
//! untouched (roads, sand, dirt etc. paint through normally) — this is a
//! geometry-only pass run after `sample_tile_heights`.
//!
//! Target Y is the un-carved natural sample at the settlement center
//! (`sample_natural_height_single`), so a pad centered near a river never
//! inherits the carved bed and sinks the village underwater.

use std::collections::HashMap;

use super::super::config::WorldGenConfig;
use super::super::global_map::GlobalMap;
use super::super::grid::fold_delta_f32;
use super::super::noise::{fbm_wrap_xy, smoothstep, PerlinNoise4D};
use super::super::settlements::Settlement;
use super::constants::VERTS_PER_SIDE;
use super::context::BakeContext;
use super::heightmap::sample_natural_height_single;

/// Inner radius (m) at which the heightmap is held exactly at `target_y`.
pub const SETTLEMENT_FLAT_RADIUS_M: f32 = 30.0;
/// Width (m) of the smoothstep blend ring outside the flat core, fading
/// back to natural terrain.
pub const SETTLEMENT_FLATTEN_BLEND_M: f32 = 20.0;
/// Spatial frequency (cycles/m) of the perimeter wobble noise. ~1/25 gives
/// roughly 3–4 lobes around a 30 m circle so the pad reads as an organic
/// blob rather than a perfect disc.
const BOUNDARY_NOISE_FREQ: f32 = 1.0 / 25.0;
/// Amplitude (m) added to the effective distance before the radius/blend
/// test. Perlin output sits in roughly [-0.7, 0.7], so ±5 m of perimeter
/// wobble — visible against a 30 m flat radius without breaking up the
/// pad's footprint.
const BOUNDARY_NOISE_AMP_M: f32 = 8.0;

/// Outermost reach (m) of any pad: even with the most negative noise pulling
/// the boundary inward, vertices past this distance are guaranteed outside
/// the blend ring.
const REACH_M: f32 = SETTLEMENT_FLAT_RADIUS_M + BOUNDARY_NOISE_AMP_M + SETTLEMENT_FLATTEN_BLEND_M;
/// Squared distance below which a vertex is unconditionally inside the flat
/// core regardless of noise sign — skips the Perlin sample.
const INNER_SQ: f32 = (SETTLEMENT_FLAT_RADIUS_M - BOUNDARY_NOISE_AMP_M)
    * (SETTLEMENT_FLAT_RADIUS_M - BOUNDARY_NOISE_AMP_M);
/// Squared distance above which a vertex is unconditionally outside the
/// blend ring regardless of noise sign — skips the vertex entirely.
const OUTER_SQ: f32 = REACH_M * REACH_M;

#[derive(Debug, Clone)]
pub struct SettlementFlatten {
    pub center_x: f32,
    pub center_z: f32,
    pub target_y: f32,
}

/// Build the deduped list of flatten directives — one per settlement, with
/// `target_y` resolved from the natural-terrain sample at the center. Used
/// both for per-tile bucketing (`group_flattens_by_tile`) and for queries
/// at a single world point (`flatten_height_at`, e.g. bridge-bank probes
/// that need to read the post-flatten pad surface, not the natural hill).
pub fn build_directives(
    settlements: &[Settlement],
    cfg: &WorldGenConfig,
    map: &GlobalMap,
    ctx: &BakeContext,
) -> Vec<SettlementFlatten> {
    let mpc = cfg.meters_per_cell();
    let half = cfg.world_size_m as f32 * 0.5;
    settlements
        .iter()
        .map(|s| {
            let cx = (s.cell_x as f32 + 0.5) * mpc - half;
            let cz = (s.cell_y as f32 + 0.5) * mpc - half;
            let target_y = sample_natural_height_single(map, ctx, cx, cz);
            SettlementFlatten {
                center_x: cx,
                center_z: cz,
                target_y,
            }
        })
        .collect()
}

/// Bucket pre-built directives by tile. A settlement gets cloned into
/// every tile its (radius + blend) reach overlaps; tiles without any
/// settlement reach receive nothing. Tile keys are wrapped into the
/// canonical baked range so pads straddling a world seam land in the
/// tiles the bake actually enumerates.
pub fn group_flattens_by_tile(
    directives: &[SettlementFlatten],
    world_size: f32,
) -> HashMap<(i32, i32), Vec<SettlementFlatten>> {
    let mut out: HashMap<(i32, i32), Vec<SettlementFlatten>> = HashMap::new();
    for d in directives {
        let tile_min_x = super::world_to_tile(d.center_x - REACH_M);
        let tile_max_x = super::world_to_tile(d.center_x + REACH_M);
        let tile_min_z = super::world_to_tile(d.center_z - REACH_M);
        let tile_max_z = super::world_to_tile(d.center_z + REACH_M);
        for tz in tile_min_z..=tile_max_z {
            for tx in tile_min_x..=tile_max_x {
                let key = (
                    super::wrap_tile(tx, world_size),
                    super::wrap_tile(tz, world_size),
                );
                out.entry(key).or_default().push(d.clone());
            }
        }
    }
    out
}

/// Evaluate the post-flatten height at a single world point. `natural` is
/// the un-flattened terrain at `(wx, wz)`; for points outside every pad the
/// function returns it unchanged. Used by bridge bank probes so they read
/// the same pad surface the per-tile bake will write.
pub fn flatten_height_at(
    wx: f32,
    wz: f32,
    natural: f32,
    directives: &[SettlementFlatten],
    detail_noise: &PerlinNoise4D,
    world_size: f32,
) -> f32 {
    let mut h = natural;
    for fl in directives {
        h = apply_one(h, wx, wz, fl, detail_noise, world_size);
    }
    h
}

/// Single-directive per-point pad evaluation. Shared by `flatten_height_at`
/// (one-off probes) and `apply_settlement_flatten` (per-vertex sweep) so
/// the inside / blend / outside math has one source of truth — drift here
/// would desync bridge bank probes from the heightmap they're predicting.
fn apply_one(
    h: f32,
    wx: f32,
    wz: f32,
    fl: &SettlementFlatten,
    detail_noise: &PerlinNoise4D,
    world_size: f32,
) -> f32 {
    let dx = fold_delta_f32(wx - fl.center_x, world_size);
    let dz = fold_delta_f32(wz - fl.center_z, world_size);
    let dist_sq = dx * dx + dz * dz;
    if dist_sq >= OUTER_SQ {
        return h;
    }
    if dist_sq <= INNER_SQ {
        return fl.target_y;
    }
    // Torus-periodic wobble so a pad straddling a world seam reads the same
    // boundary noise from both sides.
    let n = fbm_wrap_xy(
        detail_noise,
        wx,
        wz,
        world_size,
        BOUNDARY_NOISE_FREQ,
        1,
        2.0,
        0.5,
    );
    let dist = dist_sq.sqrt();
    let edge = dist + n * BOUNDARY_NOISE_AMP_M - SETTLEMENT_FLAT_RADIUS_M;
    if edge <= 0.0 {
        fl.target_y
    } else if edge < SETTLEMENT_FLATTEN_BLEND_M {
        let s = 1.0 - smoothstep(0.0, SETTLEMENT_FLATTEN_BLEND_M, edge);
        h + (fl.target_y - h) * s
    } else {
        h
    }
}

/// Apply each flatten directive to the tile's heights buffer. Inside the
/// flat radius the height is replaced with `target_y`; in the blend ring
/// a smoothstep eases back to the natural sampled height.
pub(super) fn apply_settlement_flatten(
    heights: &mut [f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    flattens: &[SettlementFlatten],
    detail_noise: &PerlinNoise4D,
    world_size: f32,
) {
    let last = (VERTS_PER_SIDE - 1) as i32;
    for fl in flattens {
        // Unwrap the pad center next to this tile so the vertex bounding box
        // works even when the pad sits across a world seam from the tile.
        let cx = tile_origin_x + fold_delta_f32(fl.center_x - tile_origin_x, world_size);
        let cz = tile_origin_z + fold_delta_f32(fl.center_z - tile_origin_z, world_size);
        let i0 = ((cx - REACH_M - tile_origin_x).floor() as i32).clamp(0, last) as usize;
        let i1 = ((cx + REACH_M - tile_origin_x).ceil() as i32).clamp(0, last) as usize;
        let j0 = ((cz - REACH_M - tile_origin_z).floor() as i32).clamp(0, last) as usize;
        let j1 = ((cz + REACH_M - tile_origin_z).ceil() as i32).clamp(0, last) as usize;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let wx = tile_origin_x + i as f32;
                let wz = tile_origin_z + j as f32;
                let idx = j * VERTS_PER_SIDE + i;
                heights[idx] = apply_one(heights[idx], wx, wz, fl, detail_noise, world_size);
            }
        }
    }
}
