//! Bridge placement at road↔river crossings.
//!
//! Run after `roads::snap_crossings_to_grid`: every interior road cell that
//! coincides with a river cell becomes one bridge. Width at the crossing is
//! interpolated from the river's per-vertex widths; the rendered ribbon is
//! `width_baked × 1.5 + 2.0` (see `client/src/lib/utils/river-geometry.ts`),
//! so `BRIDGE_WIDE_RIBBON_M` is compared against that to pick the wide
//! (`big_stone_bridge`) vs the narrow (`stone_bridge`) model.
//!
//! Bridge Y sits at the midpoint of the surface the deck ends meet:
//! heights are sampled perpendicular to the river tangent at
//! `model.deck_min_z` and `model.deck_max_z`, then run through
//! `settlement_flatten::flatten_height_at` so a deck end inside a town
//! pad reads the flattened pad surface instead of the natural hill. The
//! river carve stays excluded — the carve runs after the pad in the per-
//! tile bake and would pull the sample below the level the bridge
//! visually meets. Sampling at the deck-end distance (rather than further
//! out past the carve) keeps the flattened deck rect aligned with the
//! surrounding terrain. Rotation comes from the road tangent (perpendicular
//! to the river tangent) — converted to a three.js Y-rotation that aligns
//! the deck's local +Z with the road direction.
//!
//! Per-tile heightmap flatten replicates the editor's
//! `flattenRotatedRect` (see
//! `client/src/lib/managers/terrain-height-brushes.ts`): inside the rotated
//! deck rect `targetY = placement.y + minLocalY + buryDepth`; outside, a
//! `BRIDGE_FLATTEN_BLEND_M` smoothstep blend back to the natural height.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::super::global_map::GlobalMap;
use super::super::rivers::RiverMap;
use super::super::roads::RoadNetwork;
use super::super::vector_features::{nearest_river_segment, river_segments_near_tile};
use super::constants::{TILE_DIM, VERTS_PER_SIDE};
use super::context::BakeContext;
use super::heightmap::sample_natural_height_single;
use super::settlement_flatten::{flatten_height_at, SettlementFlatten};

/// Width threshold (rendered ribbon meters) above which the wider bridge
/// model is selected. Matches the user-facing river width, not the baked
/// segment width — `width_visible = width_baked * 1.5 + 2.0` per the client
/// ribbon geometry.
const BRIDGE_WIDE_RIBBON_M: f32 = 14.0;

/// Render-side ribbon adjustments — keep in sync with `river-geometry.ts`'s
/// `RIVER_WIDTH_SCALE` and `RIVER_WIDTH_PAD_M`.
const RIVER_RIBBON_WIDTH_SCALE: f32 = 1.5;
const RIVER_RIBBON_WIDTH_PAD_M: f32 = 1.0;

/// Smoothstep blend distance (m) past the rotated deck rect, matching the
/// editor's `FLATTEN_BLEND_RADIUS = 2`.
const BRIDGE_FLATTEN_BLEND_M: f32 = 2.0;

/// Catalog data for one bridge model. Mirrors the bridge entries in
/// `client/public/models/objects/catalog.json`. Loaded once by the bake
/// driver and cloned into per-tile flatten lists.
#[derive(Debug, Clone)]
pub struct BridgeModel {
    pub id: String,
    pub deck_min_x: f32,
    pub deck_max_x: f32,
    pub deck_min_z: f32,
    pub deck_max_z: f32,
    /// Lowest Y of the model in its local frame; targetY = placement.y +
    /// minLocalY + buryDepth.
    pub min_local_y: f32,
    pub flatten_bury_depth: f32,
}

impl BridgeModel {
    fn flatten_target_offset(&self) -> f32 {
        self.min_local_y + self.flatten_bury_depth
    }
}

/// Pair of bridge models the bake selects between — narrow for rivers
/// rendered under `BRIDGE_WIDE_RIBBON_M` meters wide, wide otherwise.
#[derive(Debug, Clone)]
pub struct BridgeCatalog {
    pub narrow: BridgeModel,
    pub wide: BridgeModel,
}

impl BridgeCatalog {
    fn pick(&self, visible_width_m: f32) -> &BridgeModel {
        if visible_width_m >= BRIDGE_WIDE_RIBBON_M {
            &self.wide
        } else {
            &self.narrow
        }
    }

    fn find(&self, id: &str) -> Option<&BridgeModel> {
        if self.narrow.id == id {
            Some(&self.narrow)
        } else if self.wide.id == id {
            Some(&self.wide)
        } else {
            None
        }
    }

    /// Catalog model IDs the bake owns. Used by region-object writers to
    /// strip stale bake-emitted bridges from `objects/r±NN_±NN.json` before
    /// writing fresh placements (so user-placed objects in the same region
    /// survive across bakes).
    pub fn model_ids(&self) -> [&str; 2] {
        [self.narrow.id.as_str(), self.wide.id.as_str()]
    }
}

/// One bridge to drop in the world. Coordinates and rotation match the
/// `placements[]` entries in `data/terrain/objects/r±NN_±NN.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BridgePlacement {
    pub model_id: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Three.js Y-rotation in degrees.
    pub rotation: f32,
}

/// Per-tile flatten directive for one bridge whose deck rect intersects the
/// tile (with the blend radius added). Parallel-tile-safe: every tile that
/// the rotated rect plus blend touches gets a copy of the same struct.
#[derive(Debug, Clone)]
pub struct BridgeFlatten {
    center_x: f32,
    center_z: f32,
    rot_rad: f32,
    deck_min_x: f32,
    deck_max_x: f32,
    deck_min_z: f32,
    deck_max_z: f32,
    target_y: f32,
    /// Pre-computed world AABB of the rotated deck rect plus blend radius;
    /// used by `apply_bridge_flatten` to skip vertices outside the rect.
    world_min_x: f32,
    world_max_x: f32,
    world_min_z: f32,
    world_max_z: f32,
}

/// Detect all road↔river crossings and emit bridge placements (one per
/// crossing). Caller passes the post-snap polylines: `snap_crossings_to_grid`
/// guarantees one cell of pure perpendicular road and pure perpendicular
/// river at every crossing, so a cell-coord coincidence is a valid bridge
/// site without further checks.
pub fn detect_bridges(
    map: &GlobalMap,
    river_map: &RiverMap,
    road_net: &RoadNetwork,
    ctx: &BakeContext,
    catalog: &BridgeCatalog,
    settlement_directives: &[SettlementFlatten],
) -> Vec<BridgePlacement> {
    let map_config = &map.config;
    let res = map_config.global_res as usize;
    let total = res * res;
    let mut river_cell: Vec<Option<(u32, u32)>> = vec![None; total];
    for (ri, poly) in river_map.rivers.iter().enumerate() {
        for (pi, &(x, y)) in poly.points.iter().enumerate() {
            let idx = (y as usize) * res + (x as usize);
            if river_cell[idx].is_none() {
                river_cell[idx] = Some((ri as u32, pi as u32));
            }
        }
    }

    let mpc = map_config.meters_per_cell();
    let half = map_config.world_size_m as f32 * 0.5;
    let cell_to_world = |cx: u32, cy: u32| -> (f32, f32) {
        (
            (cx as f32 + 0.5) * mpc - half,
            (cy as f32 + 0.5) * mpc - half,
        )
    };

    // Dedup by crossing cell — multiple roads can converge on the same
    // junction city and produce overlapping crossings, but only one bridge
    // should drop there.
    let mut placed_cells: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for road in &road_net.roads {
        let n = road.points.len();
        if n < 2 {
            continue;
        }
        // Endpoints included: on-river settlements (Phase A river-town
        // pattern) put their only road↔river contact at the endpoint. Bank
        // settlements naturally skip via the river_cell miss.
        for &(rx, ry) in &road.points {
            let cell = (ry as usize) * res + (rx as usize);
            let Some((river_idx, river_pi)) = river_cell[cell] else {
                continue;
            };
            if !placed_cells.insert(cell) {
                continue;
            }

            let (wx, wz) = cell_to_world(rx, ry);

            // River-aligned tangent. Read direction from the river polyline
            // that owns the cell so a road that crosses two parallel rivers
            // doesn't pick up the road's own tangent and rotate the deck wrong.
            let (rt_dx, rt_dz) = river_world_tangent(
                &river_map.rivers[river_idx as usize].points,
                river_pi as usize,
                map_config,
            );

            // Width at the projection point — pull only segments inside a
            // small local AABB so the nearest_river_segment search stays cheap.
            let probe_margin = TILE_DIM as f32 * 0.5;
            let local_segs = river_segments_near_tile(
                &ctx.rivers_world,
                wx - probe_margin,
                wz - probe_margin,
                wx + probe_margin,
                wz + probe_margin,
                0.0,
            );
            let baked_width = match nearest_river_segment(wx, wz, &local_segs) {
                Some((_, idx, t)) => {
                    let s = &local_segs[idx];
                    s.width_a + (s.width_b - s.width_a) * t
                }
                None => continue,
            };

            let visible_width =
                baked_width * RIVER_RIBBON_WIDTH_SCALE + RIVER_RIBBON_WIDTH_PAD_M * 2.0;
            let model = catalog.pick(visible_width);

            let perp_x = -rt_dz;
            let perp_z = rt_dx;
            let probe = |x: f32, z: f32| {
                flatten_height_at(
                    x,
                    z,
                    sample_natural_height_single(map, ctx, x, z),
                    settlement_directives,
                    &ctx.detail_noise,
                )
            };
            let h_a = probe(wx + perp_x * model.deck_max_z, wz + perp_z * model.deck_max_z);
            let h_b = probe(wx + perp_x * model.deck_min_z, wz + perp_z * model.deck_min_z);
            let bridge_y = (h_a + h_b) * 0.5;

            // Deck local +Z aligns with the road tangent (perpendicular to
            // the river). Three.js Y rotation maps (0,0,1) to
            // (sinθ, 0, cosθ), so θ = atan2(road_dx, road_dz).
            let road_dx = -rt_dz;
            let road_dz = rt_dx;
            let theta = road_dx.atan2(road_dz);
            let rotation_deg = canonical_deck_angle(theta).to_degrees();

            out.push(BridgePlacement {
                model_id: model.id.clone(),
                x: wx,
                y: bridge_y,
                z: wz,
                rotation: rotation_deg,
            });
        }
    }
    out
}

/// Reduce `θ ∈ (−π, π]` to its canonical deck representative `[0, π)`. Two
/// angles that differ by 180° produce visually identical bridge decks
/// (axial symmetry across XZ plane), so collapsing them to one value keeps
/// repeated bakes deterministic when the river tangent flips sign.
fn canonical_deck_angle(theta: f32) -> f32 {
    let pi = std::f32::consts::PI;
    let two_pi = 2.0 * pi;
    let mut a = theta.rem_euclid(two_pi);
    if a >= pi {
        a -= pi;
    }
    a
}

/// Convert a polyline cell-coord direction at index `pi` to a world unit
/// tangent. Uses central differencing (one-sided at endpoints) and folds
/// the X-wrap so polyline pieces that span the seam don't read as a
/// world-spanning jump.
fn river_world_tangent(
    points: &[(u32, u32)],
    pi: usize,
    cfg: &super::super::config::WorldGenConfig,
) -> (f32, f32) {
    let n = points.len();
    let prev = if pi == 0 { points[pi] } else { points[pi - 1] };
    let next = if pi + 1 >= n {
        points[pi]
    } else {
        points[pi + 1]
    };
    let res = cfg.global_res as f32;
    let mut dx = next.0 as f32 - prev.0 as f32;
    if dx > res * 0.5 {
        dx -= res;
    } else if dx < -res * 0.5 {
        dx += res;
    }
    let dy = next.1 as f32 - prev.1 as f32;
    let mpc = cfg.meters_per_cell();
    let wx = dx * mpc;
    let wz = dy * mpc;
    let len = (wx * wx + wz * wz).sqrt().max(1e-6);
    (wx / len, wz / len)
}

/// Build per-tile flatten directives from the global placement list. A
/// bridge gets cloned into the directive list for every tile its rotated
/// deck rect plus blend overlaps; tiles without any bridges receive nothing
/// (caller looks them up by `(tx, tz)` and falls back to an empty slice).
pub fn group_flattens_by_tile(
    placements: &[BridgePlacement],
    catalog: &BridgeCatalog,
) -> HashMap<(i32, i32), Vec<BridgeFlatten>> {
    let mut out: HashMap<(i32, i32), Vec<BridgeFlatten>> = HashMap::new();
    for p in placements {
        let Some(model) = catalog.find(&p.model_id) else {
            continue;
        };
        let rot_rad = p.rotation.to_radians();
        let cos = rot_rad.cos();
        let sin = rot_rad.sin();
        // Rotated rect AABB (mirrors `objectFootprint.ts::rotatedRectAabb`).
        let mut a_min_x = f32::INFINITY;
        let mut a_max_x = f32::NEG_INFINITY;
        let mut a_min_z = f32::INFINITY;
        let mut a_max_z = f32::NEG_INFINITY;
        for &lx in &[model.deck_min_x, model.deck_max_x] {
            for &lz in &[model.deck_min_z, model.deck_max_z] {
                let wx = lx * cos + lz * sin;
                let wz = -lx * sin + lz * cos;
                a_min_x = a_min_x.min(wx);
                a_max_x = a_max_x.max(wx);
                a_min_z = a_min_z.min(wz);
                a_max_z = a_max_z.max(wz);
            }
        }
        let blend = BRIDGE_FLATTEN_BLEND_M;
        let world_min_x = p.x + a_min_x - blend;
        let world_max_x = p.x + a_max_x + blend;
        let world_min_z = p.z + a_min_z - blend;
        let world_max_z = p.z + a_max_z + blend;

        let tile_min_x = super::world_to_tile(world_min_x);
        let tile_max_x = super::world_to_tile(world_max_x);
        let tile_min_z = super::world_to_tile(world_min_z);
        let tile_max_z = super::world_to_tile(world_max_z);

        let target_y = p.y + model.flatten_target_offset();
        let directive = BridgeFlatten {
            center_x: p.x,
            center_z: p.z,
            rot_rad,
            deck_min_x: model.deck_min_x,
            deck_max_x: model.deck_max_x,
            deck_min_z: model.deck_min_z,
            deck_max_z: model.deck_max_z,
            target_y,
            world_min_x,
            world_max_x,
            world_min_z,
            world_max_z,
        };
        for tz in tile_min_z..=tile_max_z {
            for tx in tile_min_x..=tile_max_x {
                out.entry((tx, tz)).or_default().push(directive.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod canonical_tests {
    use super::canonical_deck_angle;
    use std::f32::consts::PI;

    #[test]
    fn canonical_collapses_180_degree_pairs() {
        // The bridge deck has bilateral symmetry across the XZ plane, so
        // rotations that differ by 180° look identical. The canonical form
        // must collapse them onto the same value.
        let pairs = [
            (0.25 * PI, 1.25 * PI),
            (0.5 * PI, 1.5 * PI),
            (0.75 * PI, 1.75 * PI),
        ];
        for (a, b) in pairs {
            let ca = canonical_deck_angle(a);
            let cb = canonical_deck_angle(b);
            assert!((ca - cb).abs() < 1e-4, "pair {a} {b}: got {ca}, {cb}");
        }
    }

    #[test]
    fn canonical_in_zero_to_pi_range() {
        // Output must always sit in [0, π) so JSON diffs are stable across
        // bakes regardless of upstream tangent sign flips.
        let samples = [
            -2.0 * PI,
            -1.5 * PI,
            -0.1,
            0.0,
            0.5 * PI,
            PI - 1e-6,
            2.5 * PI,
        ];
        for theta in samples {
            let c = canonical_deck_angle(theta);
            assert!(c >= 0.0 && c < PI + 1e-4, "θ={theta} → {c}");
        }
    }
}

/// Apply each flatten directive to the tile's heights buffer. Replicates the
/// distance-to-rotated-rect blend from
/// `client/src/lib/managers/terrain-height-brushes.ts::flattenRotatedRect`.
pub(super) fn apply_bridge_flatten(
    heights: &mut [f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    flattens: &[BridgeFlatten],
) {
    let last = (VERTS_PER_SIDE - 1) as i32;
    for fl in flattens {
        let cos = fl.rot_rad.cos();
        let sin = fl.rot_rad.sin();
        let blend = BRIDGE_FLATTEN_BLEND_M;
        // Restrict the per-vertex sweep to the rotated-rect AABB; outside
        // that band the blend evaluates to zero and the loop is wasted.
        let i0 = ((fl.world_min_x - tile_origin_x).floor() as i32).clamp(0, last) as usize;
        let i1 = ((fl.world_max_x - tile_origin_x).ceil() as i32).clamp(0, last) as usize;
        let j0 = ((fl.world_min_z - tile_origin_z).floor() as i32).clamp(0, last) as usize;
        let j1 = ((fl.world_max_z - tile_origin_z).ceil() as i32).clamp(0, last) as usize;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let wx = tile_origin_x + i as f32;
                let wz = tile_origin_z + j as f32;
                let dx = wx - fl.center_x;
                let dz = wz - fl.center_z;
                // World→local with three.js's positive-Y rotation
                // (matches `flattenRotatedRect`'s lx/lz formulae).
                let lx = dx * cos - dz * sin;
                let lz = dx * sin + dz * cos;
                let ddx = (fl.deck_min_x - lx).max(0.0).max(lx - fl.deck_max_x);
                let ddz = (fl.deck_min_z - lz).max(0.0).max(lz - fl.deck_max_z);
                let dist = (ddx * ddx + ddz * ddz).sqrt();
                let idx = j * VERTS_PER_SIDE + i;
                if dist <= 0.0 {
                    heights[idx] = fl.target_y;
                } else if dist < blend {
                    let t = dist / blend;
                    let s = 1.0 - t * t * (3.0 - 2.0 * t);
                    heights[idx] = heights[idx] + (fl.target_y - heights[idx]) * s;
                }
            }
        }
    }
}
