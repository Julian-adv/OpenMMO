//! Bridge placement at road↔river crossings.
//!
//! Run after `roads::snap_crossings_to_grid`: every interior road cell that
//! coincides with a river cell becomes one bridge. Visible water width at
//! the crossing is `baked_width + segment_carve_taper_at(...)` — the taper
//! is added once because the `river_field::compute_pixel` smoothstep puts
//! the *visible* water boundary near `half_width + 0.5·taper` per side,
//! inside the alpha-zero contour `half_width + taper`. That width is
//! compared against `BRIDGE_WIDE_RIBBON_M` to pick the wide
//! (`bridge_wood_long`) vs the narrow (`stone_bridge`) model.
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
use super::super::grid::fold_delta_f32;
use super::super::rivers::RiverMap;
use super::super::roads::RoadNetwork;
use super::super::vector_features::{nearest_river_segment, river_segments_near_tile_wrap_xy};
use super::constants::{TILE_DIM, VERTS_PER_SIDE};
use super::context::BakeContext;
use super::heightmap::{sample_natural_height_single, segment_carve_taper_at};
use super::river_geom::{
    polyline_arcs, PolylineArcs, BRIDGE_MAX_VISIBLE_WIDTH_M, RIVER_DELTA_BUFFER_ARC_CELLS,
};
use super::settlement_flatten::{flatten_height_at, SettlementFlatten};

/// Width threshold (visible water meters — `width + carve_taper`) above
/// which the wider `bridge_wood_long` is selected over the narrow
/// `stone_bridge`. Natural reaches sit at `4 + 16·flow_norm` m visible
/// (max ~20 m at full flow). 12 m maps to `flow_norm ≈ 0.5`, so the upper
/// half of the flow distribution lands on the wide model while headwater
/// stubs and tapering distributary branches keep the narrow stone bridge.
const BRIDGE_WIDE_RIBBON_M: f32 = 12.0;

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
    /// Half-deck length (m, along local Z) over which the heightmap is
    /// flattened at each deck end. The arch span between the two foot zones
    /// is left alone so the river carve survives — otherwise the raised
    /// channel bed pushes `surfaceY = bed + 0.5` above the dipping deck
    /// profile and engulfs the foot. Loaded from the catalog's explicit
    /// `flattenFootLengthZ` field when present; otherwise derived from
    /// `deckYSamples` via [`foot_length_from_samples`].
    pub flatten_foot_length_z: f32,
    /// Half-width (m, along local X) of the foot flatten rect — symmetric
    /// around the deck centreline. The flatten can extend past the deck's
    /// own X bounds when the abutment should spread laterally onto the
    /// bank. Loaded from the catalog's `flattenFootWidthX` field
    /// (interpreted as full width); defaults to the deck's X half-width
    /// when absent.
    pub flatten_foot_half_width_x: f32,
}

impl BridgeModel {
    fn flatten_target_offset(&self) -> f32 {
        self.min_local_y + self.flatten_bury_depth
    }
}

/// Deck Y (local) below which a sample is considered to be part of the
/// foot — the deck rests on the abutment, not the arch span. Tuned so the
/// stone bridges' last 1–3 m of deck at each end (where the arch finishes
/// curving down) reads as foot.
const FOOT_DECK_Y_THRESHOLD_M: f32 = 0.5;

/// Derive the foot length (m along local Z) from the per-bridge
/// `deckYSamples` array. Samples are expected to run from the deck centre
/// (`samples[0]`, peak of the arch) outward to the deck end
/// (`samples[len-1]`, foot), evenly spaced across `[0, deck_max_z]`. Returns
/// the length of the outermost run of samples whose deck Y stays below
/// [`FOOT_DECK_Y_THRESHOLD_M`].
pub fn foot_length_from_samples(samples: &[f32], deck_max_z: f32) -> f32 {
    if samples.len() < 2 {
        return 0.0;
    }
    let foot_count = samples
        .iter()
        .rev()
        .take_while(|&&y| y < FOOT_DECK_Y_THRESHOLD_M)
        .count();
    if foot_count == 0 {
        return 0.0;
    }
    let z_per_sample = deck_max_z / (samples.len() - 1) as f32;
    foot_count as f32 * z_per_sample
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
    /// survive across bakes). Includes [`LEGACY_BRIDGE_MODEL_IDS`] so that
    /// re-bakes after a bridge model rename clean up the previous model's
    /// placements instead of leaving them overlapping the new ones.
    pub fn model_ids(&self) -> Vec<&str> {
        let mut ids = vec![self.narrow.id.as_str(), self.wide.id.as_str()];
        ids.extend(LEGACY_BRIDGE_MODEL_IDS.iter().copied());
        ids
    }
}

/// Bridge model IDs that the bake used to place but no longer does. Kept here
/// so `BridgeCatalog::model_ids()` strips them from region object files on the
/// next bake — without this, a model swap (e.g. `big_stone_bridge` →
/// `bridge_wood_long` for the wide slot) leaves the previous model's
/// placements in `objects/r*_*.json` and they render alongside the new ones.
pub const LEGACY_BRIDGE_MODEL_IDS: &[&str] = &["big_stone_bridge"];

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
    deck_min_z: f32,
    deck_max_z: f32,
    foot_length_z: f32,
    foot_half_width_x: f32,
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

    // Arc table indexed by polyline; populated only for sea-bound polylines
    // so the delta-buffer gate below is a single Option lookup per crossing.
    let poly_arcs: Vec<Option<PolylineArcs>> = river_map
        .rivers
        .iter()
        .enumerate()
        .map(|(i, poly)| polyline_arcs(poly, map, i).filter(|a| a.mouth_in_sea))
        .collect();

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

            // Skip bridges inside the sea-bound delta buffer. Road A* marks
            // the same band MASK_WIDE so it only enters as start/goal, but
            // when a river-mouth settlement lives on a delta cell that
            // exemption would otherwise drop a bridge mid-distributary-fan.
            // The settlement pad covers the river at that cell so the
            // missing bridge leaves no visual gap.
            if let Some(arcs) = &poly_arcs[river_idx as usize] {
                if arcs.arc_to_mouth(river_pi as usize) < RIVER_DELTA_BUFFER_ARC_CELLS {
                    continue;
                }
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
            let local_segs = river_segments_near_tile_wrap_xy(
                &ctx.rivers_world,
                wx - probe_margin,
                wz - probe_margin,
                wx + probe_margin,
                wz + probe_margin,
                0.0,
                map_config.world_size_m as f32,
            );
            // Visible water width = baked + taper. See the module doc for
            // why the taper enters once (smoothstep boundary, not
            // alpha-zero contour). Branches carry a shorter taper than
            // natural reaches; pulling it from the segment keeps both
            // kinds measured by the same yardstick.
            let visible_width = match nearest_river_segment(wx, wz, &local_segs) {
                Some((_, idx, t)) => {
                    let s = &local_segs[idx];
                    let baked = s.width_a + (s.width_b - s.width_a) * t;
                    let taper = segment_carve_taper_at(s, t);
                    baked + taper
                }
                None => continue,
            };
            // Safety net for the rare case where a road still threads a
            // wide cell — A* exempts start/goal endpoints from its
            // wide-river impassability mask.
            if visible_width > BRIDGE_MAX_VISIBLE_WIDTH_M {
                continue;
            }
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
                    map.config.world_size_m as f32,
                )
            };
            let h_a = probe(
                wx + perp_x * model.deck_max_z,
                wz + perp_z * model.deck_max_z,
            );
            let h_b = probe(
                wx + perp_x * model.deck_min_z,
                wz + perp_z * model.deck_min_z,
            );
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
/// both axes so polyline pieces that span a seam don't read as a
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
    let dx = fold_delta_f32(next.0 as f32 - prev.0 as f32, res);
    let dy = fold_delta_f32(next.1 as f32 - prev.1 as f32, res);
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
    world_size: f32,
) -> HashMap<(i32, i32), Vec<BridgeFlatten>> {
    let mut out: HashMap<(i32, i32), Vec<BridgeFlatten>> = HashMap::new();
    for p in placements {
        let Some(model) = catalog.find(&p.model_id) else {
            continue;
        };
        let rot_rad = p.rotation.to_radians();
        let cos = rot_rad.cos();
        let sin = rot_rad.sin();
        // Rotated AABB of the foot-flatten footprint (the two foot rects
        // share a common X half-width, so the union's local-space corners
        // sit at (±foot_half_width_x, ±deck_max_z)). The arch span in the
        // middle isn't flattened, but it's bounded by the same envelope so
        // the sweep AABB is identical.
        let half_x = model.flatten_foot_half_width_x;
        let mut a_min_x = f32::INFINITY;
        let mut a_max_x = f32::NEG_INFINITY;
        let mut a_min_z = f32::INFINITY;
        let mut a_max_z = f32::NEG_INFINITY;
        for &lx in &[-half_x, half_x] {
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
            deck_min_z: model.deck_min_z,
            deck_max_z: model.deck_max_z,
            foot_length_z: model.flatten_foot_length_z,
            foot_half_width_x: half_x,
            target_y,
            world_min_x,
            world_max_x,
            world_min_z,
            world_max_z,
        };
        for tz in tile_min_z..=tile_max_z {
            for tx in tile_min_x..=tile_max_x {
                let key = (
                    super::wrap_tile(tx, world_size),
                    super::wrap_tile(tz, world_size),
                );
                out.entry(key).or_default().push(directive.clone());
            }
        }
    }
    out
}

/// Apply each flatten directive to the tile's heights buffer. The flatten
/// hits only the two foot rects at the deck ends — `foot_length_z` along
/// local Z at each end, full deck width along local X. The arch span between
/// the foot zones keeps its river-carved channel so the runtime's
/// `surfaceY = bed + 0.5` doesn't rise into the dipping deck profile and
/// engulf the foot. Replicates the distance-to-rotated-rect blend from
/// `client/src/lib/managers/terrain-height-brushes.ts::flattenRotatedRect`
/// but with two rects instead of one.
pub(super) fn apply_bridge_flatten(
    heights: &mut [f32],
    tile_origin_x: f32,
    tile_origin_z: f32,
    flattens: &[BridgeFlatten],
    world_size: f32,
) {
    let last = (VERTS_PER_SIDE - 1) as i32;
    for fl in flattens {
        let cos = fl.rot_rad.cos();
        let sin = fl.rot_rad.sin();
        let blend = BRIDGE_FLATTEN_BLEND_M;
        // World-shift that brings a seam-straddling directive next to this
        // tile (one of -W / 0 / +W per axis).
        let sx =
            fold_delta_f32(fl.center_x - tile_origin_x, world_size) - (fl.center_x - tile_origin_x);
        let sz =
            fold_delta_f32(fl.center_z - tile_origin_z, world_size) - (fl.center_z - tile_origin_z);
        // Two foot rects: one at each deck end, `foot_length_z` long along
        // local Z, full deck width along local X.
        let foot_a_lo = fl.deck_min_z;
        let foot_a_hi = fl.deck_min_z + fl.foot_length_z;
        let foot_b_lo = fl.deck_max_z - fl.foot_length_z;
        let foot_b_hi = fl.deck_max_z;
        // Restrict the per-vertex sweep to the rotated-rect AABB; outside
        // that band the blend evaluates to zero and the loop is wasted.
        let i0 = ((fl.world_min_x + sx - tile_origin_x).floor() as i32).clamp(0, last) as usize;
        let i1 = ((fl.world_max_x + sx - tile_origin_x).ceil() as i32).clamp(0, last) as usize;
        let j0 = ((fl.world_min_z + sz - tile_origin_z).floor() as i32).clamp(0, last) as usize;
        let j1 = ((fl.world_max_z + sz - tile_origin_z).ceil() as i32).clamp(0, last) as usize;
        for j in j0..=j1 {
            for i in i0..=i1 {
                let wx = tile_origin_x + i as f32;
                let wz = tile_origin_z + j as f32;
                let dx = wx - (fl.center_x + sx);
                let dz = wz - (fl.center_z + sz);
                // World→local with three.js's positive-Y rotation
                // (matches `flattenRotatedRect`'s lx/lz formulae).
                let lx = dx * cos - dz * sin;
                let lz = dx * sin + dz * cos;
                let ddx = (lx.abs() - fl.foot_half_width_x).max(0.0);
                let ddz_a = (foot_a_lo - lz).max(0.0).max(lz - foot_a_hi);
                let ddz_b = (foot_b_lo - lz).max(0.0).max(lz - foot_b_hi);
                let ddz = ddz_a.min(ddz_b);
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
            assert!((0.0..PI + 1e-4).contains(&c), "θ={theta} → {c}");
        }
    }
}
