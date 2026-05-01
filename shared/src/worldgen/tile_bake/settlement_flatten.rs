//! Per-tile heightmap flatten around each settlement.
//!
//! Every settlement gets a circular flat pad on the heightmap so houses sit
//! on level ground regardless of the underlying hills/slope. Splatmap is
//! untouched (roads, sand, dirt etc. paint through normally) — this is a
//! geometry-only pass run after `sample_tile_heights`.
//!
//! Target Y is the natural-terrain sample at the settlement center via
//! `sample_height_single`, taken once per settlement so adjacent tiles that
//! share the same circle agree at the seam.

use std::collections::HashMap;

use super::super::config::WorldGenConfig;
use super::super::global_map::GlobalMap;
use super::super::noise::{smoothstep, PerlinNoise3D};
use super::super::settlements::Settlement;
use super::constants::VERTS_PER_SIDE;
use super::context::BakeContext;
use super::heightmap::sample_height_single;

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
const INNER_SQ: f32 =
    (SETTLEMENT_FLAT_RADIUS_M - BOUNDARY_NOISE_AMP_M) * (SETTLEMENT_FLAT_RADIUS_M - BOUNDARY_NOISE_AMP_M);
/// Squared distance above which a vertex is unconditionally outside the
/// blend ring regardless of noise sign — skips the vertex entirely.
const OUTER_SQ: f32 = REACH_M * REACH_M;

#[derive(Debug, Clone)]
pub struct SettlementFlatten {
    center_x: f32,
    center_z: f32,
    target_y: f32,
}

/// Build per-tile flatten directives for every settlement. A settlement
/// gets cloned into the directive list for every tile its (radius + blend)
/// reach overlaps; tiles without any settlement reach receive nothing.
pub fn group_flattens_by_tile(
    settlements: &[Settlement],
    cfg: &WorldGenConfig,
    map: &GlobalMap,
    ctx: &BakeContext,
) -> HashMap<(i32, i32), Vec<SettlementFlatten>> {
    let mpc = cfg.meters_per_cell();
    let half = cfg.world_size_m as f32 * 0.5;

    let mut out: HashMap<(i32, i32), Vec<SettlementFlatten>> = HashMap::new();
    for s in settlements {
        let cx = (s.cell_x as f32 + 0.5) * mpc - half;
        let cz = (s.cell_y as f32 + 0.5) * mpc - half;
        let target_y = sample_height_single(map, ctx, cx, cz);

        let directive = SettlementFlatten {
            center_x: cx,
            center_z: cz,
            target_y,
        };

        let tile_min_x = super::world_to_tile(cx - REACH_M);
        let tile_max_x = super::world_to_tile(cx + REACH_M);
        let tile_min_z = super::world_to_tile(cz - REACH_M);
        let tile_max_z = super::world_to_tile(cz + REACH_M);
        for tz in tile_min_z..=tile_max_z {
            for tx in tile_min_x..=tile_max_x {
                out.entry((tx, tz)).or_default().push(directive.clone());
            }
        }
    }
    out
}

/// Apply each flatten directive to the tile's heights buffer. Inside the
/// flat radius the height is replaced with `target_y`; in the blend ring
/// a smoothstep eases back to the natural sampled height.
pub(super) fn apply_settlement_flatten(
    heights: &mut [f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    flattens: &[SettlementFlatten],
    detail_noise: &PerlinNoise3D,
) {
    let last = (VERTS_PER_SIDE - 1) as i32;
    for fl in flattens {
        let i0 = ((fl.center_x - REACH_M - tile_origin_x).floor() as i32).clamp(0, last) as usize;
        let i1 = ((fl.center_x + REACH_M - tile_origin_x).ceil() as i32).clamp(0, last) as usize;
        let j0 = ((fl.center_z - REACH_M - tile_origin_z).floor() as i32).clamp(0, last) as usize;
        let j1 = ((fl.center_z + REACH_M - tile_origin_z).ceil() as i32).clamp(0, last) as usize;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let wx = tile_origin_x + i as f32;
                let wz = tile_origin_z + j as f32;
                let dx = wx - fl.center_x;
                let dz = wz - fl.center_z;
                let dist_sq = dx * dx + dz * dz;
                let idx = j * VERTS_PER_SIDE + i;
                if dist_sq <= INNER_SQ {
                    heights[idx] = fl.target_y;
                    continue;
                }
                if dist_sq >= OUTER_SQ {
                    continue;
                }
                // Sampled in world coords so adjacent tiles agree at the seam.
                let n = detail_noise.sample(
                    wx * BOUNDARY_NOISE_FREQ,
                    wz * BOUNDARY_NOISE_FREQ,
                    0.5,
                );
                let dist = dist_sq.sqrt();
                let edge = dist + n * BOUNDARY_NOISE_AMP_M - SETTLEMENT_FLAT_RADIUS_M;
                if edge <= 0.0 {
                    heights[idx] = fl.target_y;
                } else if edge < SETTLEMENT_FLATTEN_BLEND_M {
                    let s = 1.0 - smoothstep(0.0, SETTLEMENT_FLATTEN_BLEND_M, edge);
                    heights[idx] = heights[idx] + (fl.target_y - heights[idx]) * s;
                }
            }
        }
    }
}
