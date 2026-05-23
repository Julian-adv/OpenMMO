//! Shared tuning constants for the Phase 7 tile baker.

/// Cell-count side of the splatmap (64×64 cells per tile).
pub const TILE_DIM: usize = 64;
/// Vertex-count side of the heightmap (65×65, overlaps neighbor by 1).
pub const VERTS_PER_SIDE: usize = TILE_DIM + 1;

/// Heightmap encoding: 10000 → 0.0 m, step 0.05 m. Covers -500..+2776 m.
pub const HEIGHT_BIAS: f32 = 500.0;
pub const HEIGHT_STEP: f32 = 0.05;

/// Fixed palette slot indices used by this baker. Must match slot order in
/// `shared/palette.json`.
pub const PAL_GROUND: u8 = 0; // rocky_terrain_02 — general ground under grass
pub const PAL_SAND: u8 = 1; // sandy_gravel_02 — coast, river bed, shore
pub const PAL_DIRT: u8 = 2; // red_laterite — barren mid-altitude, gentle cliff base
pub const PAL_SNOW: u8 = 3; // snow_02 — alpine peaks
pub const PAL_ROAD: u8 = 4; // gravel_road — settlement road surfaces
pub const PAL_CLIFF: u8 = 5; // rocky_trail — exposed rock face on ≥45° slopes
pub const PAL_RIVER_BED: u8 = 6; // ganges_river_pebbles — wet rocky river bottom

/// Source of truth for the global terrain palette. Each slot: `texture`
/// (GLB under `client/public/textures/`), `tileScale` (m per repeat),
/// `minimapColor` (RGB 0..=255). The Rust baker only needs to know slot
/// order (via `PAL_*` constants) — the actual fields are consumed by the
/// client at bundle time. The embed here is just to keep the test below
/// honest about this file's schema.
#[cfg(test)]
const PALETTE_JSON: &str = include_str!("../../../palette.json");

// --- Detail noise tuning -------------------------------------------------
pub(super) const DETAIL_OCTAVES: u32 = 4;
pub(super) const DETAIL_LACUNARITY: f32 = 2.0;
pub(super) const DETAIL_GAIN: f32 = 0.5;
/// Base frequency: cycles per meter. 1/16 = 16 m wavelength; with 4 octaves
/// the finest harmonic lands near 1 m, matching the tile vertex spacing.
pub(super) const DETAIL_FREQUENCY: f32 = 1.0 / 16.0;
/// Max detail amplitude (m) on tall mountains.
pub(super) const DETAIL_MAX_AMPLITUDE: f32 = 6.0;
/// Min detail amplitude (m) on lowland plains.
pub(super) const DETAIL_MIN_AMPLITUDE: f32 = 0.4;
/// Minimum baseline elevation for land vertices (m). Every cell the
/// global `land_mask` marks as land sits at ≥ this height before detail
/// noise / hills are added on top, so coastal noise can't flicker below
/// sea level (and the sea shader's `edgeCutoff` keeps water hidden on
/// land).
pub(super) const LAND_BASE_MIN_Y_M: f32 = 0.2;
/// Detail-noise damp at the coast (`base = 0`), ramping linearly to 1
/// past `HILLS_COASTAL_FADE_M`. Chosen so `DETAIL_MIN_AMPLITUDE *
/// DETAIL_COAST_DAMP == LAND_BASE_MIN_Y_M` — that's the safety
/// invariant: coastal noise can drag down by exactly the floor's worth,
/// landing the worst case at sea level instead of crossing it.
pub(super) const DETAIL_COAST_DAMP: f32 = LAND_BASE_MIN_Y_M / DETAIL_MIN_AMPLITUDE;

// --- Rolling hills layer -------------------------------------------------
// Universal hills applied to every land vertex, independent of the Phase 2
// plain/mountain classification. Lives in Phase 7 rather than Phase 2
// because Phase 3 erosion's 24 m brush blurs 60 m-wavelength features into
// flat plateaus before they ever reach the tile baker.
pub(super) const HILLS_OCTAVES: u32 = 3;
pub(super) const HILLS_GAIN: f32 = 0.5;
pub(super) const HILLS_FREQUENCY: f32 = 1.0 / 60.0;
pub(super) const HILLS_AMPLITUDE_M: f32 = 5.0;
/// Base elevation (m) over which the hills amplitude fades in from 0 to full.
/// At base = 0 m (sea level) the hills are zero, ramping linearly to full
/// amplitude at `HILLS_COASTAL_FADE_M`. Prevents the symmetric hills noise
/// from pulling coastal lowlands below sea level and creating lagoons /
/// standing-water pockets inland of the shoreline.
pub(super) const HILLS_COASTAL_FADE_M: f32 = 3.0;

// --- River carve / splat ------------------------------------------------
// Width, taper, and carve depth all grow linearly in `flow_norm ∈ [0, 1]`.
// See RIVER_SYSTEM.md §2.4 / §2.5.
pub const RIVER_MIN_WIDTH_M: f32 = 1.0;
pub const RIVER_MAX_WIDTH_M: f32 = 10.0;
pub(super) const RIVER_CARVE_TAPER_MIN_M: f32 = 3.0;
pub(super) const RIVER_CARVE_TAPER_EXTRA_M: f32 = 7.0;
pub(super) const RIVER_CARVE_DEPTH_MIN_M: f32 = 1.5;
pub(super) const RIVER_CARVE_DEPTH_EXTRA_M: f32 = 2.5;
/// Lower bound on post-carve terrain elevation inside a river channel
/// (meters). Sits exactly at sea level so the sea shader's
/// `edgeCutoff = smoothstep(0, 0.01, depth)` cuts ocean alpha to 0
/// inside the channel. The river shader still renders a visible body
/// because `surfaceY = bed + RIVER_DEPTH_OFFSET_M = 0.5 m` at the
/// estuary, leaving headroom for its own depth-based bank fade.
pub(super) const RIVER_CARVE_MIN_BED_Y_M: f32 = 0.0;
/// River water surface offset above the carved bed (m). The bake fills a
/// per-tile field with `surfaceY = bed + RIVER_DEPTH_OFFSET_M` along each
/// segment so the runtime shader can compute `depth = surfaceY − bedY`
/// directly from texture lookups, with no polyline dependence at draw
/// time. Must agree with the runtime's expected channel depth — set just
/// large enough to give the depth-fade headroom past the 0.05 m hard cut.
pub(super) const RIVER_DEPTH_OFFSET_M: f32 = 0.5;
/// River-bed splat switches from `PAL_RIVER_BED` (ganges pebbles — wet
/// inland bed look) to `PAL_SAND` (sandy_gravel_02 — matches shallow sea)
/// near river mouths. The trigger is either a per-segment width above
/// `RIVER_MAX_WIDTH_M` (legacy wide-fan code paths) or a sub-sea bed-floor
/// target carried by mouth distributary branches. Smoothstep from `BASE_M`
/// (still pebble) to `BASE_M + FADE_M` (fully sand) keeps the swap a couple
/// of cells wide so the seam doesn't pop.
pub(super) const RIVER_FAN_SAND_BASE_WIDTH_M: f32 = RIVER_MAX_WIDTH_M;
pub(super) const RIVER_FAN_SAND_FADE_M: f32 = 3.0;
/// Distributary window (cells of arc-length measured back from the river
/// mouth). The original river is kept up to this apex, then the baker
/// replaces the sea-bound tail with several narrow S-curved branches.
pub const RIVER_MOUTH_FAN_ARC_CELLS: f32 = 8.5;
/// Bridge / road avoidance buffer (cells of arc-length back from a sea
/// mouth). Larger than [`RIVER_MOUTH_FAN_ARC_CELLS`] so the chosen crossing
/// lands a few cells UPSTREAM of where the distributary fan starts, not at
/// the polyline apex. The narrower fan constant still drives branch
/// generation and settlement habitability.
pub const RIVER_DELTA_BUFFER_ARC_CELLS: f32 = 14.0;
/// Fan scale used for road A* avoidance and tile river margins around
/// delta mouths, and also to size the distributary endpoint spread
/// (`RIVER_MOUTH_BRANCH_SPREAD_M`). Bake polylines themselves are no
/// longer widened by this factor — they are split into distributaries
/// instead.
pub(super) const RIVER_MOUTH_FAN_EXTRA: f32 = 10.0;
pub(super) const RIVER_MOUTH_FAN_SHARPNESS: f32 = 1.5;
pub(super) const RIVER_MOUTH_BRANCH_COUNT_MIN: u32 = 4;
pub(super) const RIVER_MOUTH_BRANCH_COUNT_MAX: u32 = 6;
/// Half-width (m) of the fan-shaped endpoint spread for distributaries.
pub(super) const RIVER_MOUTH_BRANCH_SPREAD_M: f32 =
    RIVER_MAX_WIDTH_M * RIVER_MOUTH_FAN_EXTRA * 0.42;
/// Endpoint lateral jitter (m) so branches do not form a perfect comb.
pub(super) const RIVER_MOUTH_BRANCH_END_JITTER_M: f32 = 4.0;
/// Maximum signed S-curve offset (m) applied to each branch centerline.
pub(super) const RIVER_MOUTH_BRANCH_MEANDER_M: f32 = 15.0;
/// Range of full sine cycles per branch length. Each branch picks a value
/// in [MIN, MAX) so neighbours don't share an obvious meander wavelength.
/// >1.0 gives multiple S-bends instead of one broad arc. Sample density
/// (`RIVER_MOUTH_BRANCH_SAMPLES`) must satisfy Nyquist for `MAX` to render
/// — keep ≥ 4× MAX.
pub(super) const RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MIN: f32 = 1.5;
pub(super) const RIVER_MOUTH_BRANCH_MEANDER_CYCLES_MAX: f32 = 3.0;
/// Pre-Chaikin control point count along each distributary branch. Needs
/// to be dense enough to resolve `RIVER_MOUTH_BRANCH_MEANDER_CYCLES`.
pub(super) const RIVER_MOUTH_BRANCH_SAMPLES: usize = 21;
/// Branch-fraction over which the meander amplitude ramps from 0 (at apex)
/// to full. Keeps the branch nearly straight where it splits off the trunk
/// so all branches stay anchored cleanly, then lets it wander toward the
/// sea where current is slow and deposition is high.
pub(super) const RIVER_MOUTH_BRANCH_MEANDER_RAMP_T: f32 = 0.5;
/// End width as a fraction of the original mouth width, clamped to
/// `[RIVER_MOUTH_BRANCH_END_WIDTH_MIN_M, RIVER_MOUTH_BRANCH_END_WIDTH_MAX_M]`.
pub(super) const RIVER_MOUTH_BRANCH_END_WIDTH_SCALE: f32 = 0.04;
pub(super) const RIVER_MOUTH_BRANCH_END_WIDTH_MIN_M: f32 = 0.2;
pub(super) const RIVER_MOUTH_BRANCH_END_WIDTH_MAX_M: f32 = 0.65;
/// Bed-floor target for tapering mouth distributary branches. These branches
/// narrow as they approach the sea, so the carve needs a small sub-sea floor
/// to keep the channel visibly connected to ocean water.
pub(super) const RIVER_MOUTH_BRANCH_BED_Y_M: f32 = -0.5;
/// Bank taper width (m) for distributary branches, replacing the natural
/// 3–10 m bank as the bed sinks sub-sea. Narrow branches with a 0.15 m
/// half-width look absurd with a 10 m gentle slope; this gives them a
/// steeper cut more like a stream incising the coast.
pub(super) const RIVER_MOUTH_BRANCH_TAPER_M: f32 = 5.0;
/// Drop of the river-bed floor below `RIVER_CARVE_MIN_BED_Y_M` in the fan
/// zone, proportional to width excess. At the fan peak the bed sits at
/// `-RIVER_MOUTH_FAN_BED_DROP_M` so the channel reads as shallow sea.
pub(super) const RIVER_MOUTH_FAN_BED_DROP_M: f32 = 1.5;
pub(super) const RIVER_SAND_WIDTH_MULT: f32 = 0.7;
/// Extra sand-band reach (m) past the water edge for mouth distributary
/// branches. Needs to clear the carve taper (`RIVER_CARVE_TAPER_MIN/EXTRA`
/// = 3–10 m) plus give a visible sand collar on natural-elevation land —
/// otherwise the sand sits entirely inside the depressed bank and reads
/// as still-water-adjacent grass.
pub(super) const RIVER_MOUTH_BRANCH_SAND_MARGIN_M: f32 = 5.0;

/// Base spatial frequency (cycles per meter) of the along-river noise that
/// widens and narrows the pebble/sand band so it doesn't read as a constant
/// ribbon parallel to the centerline. ~1/22 gives ~22 m wavelength — short
/// enough to see a few cycles across one screen width at typical camera
/// zoom, long enough that each bulge still reads as a point bar and not
/// as jittery band-edge noise.
pub(super) const RIVER_BAND_NOISE_FREQ: f32 = 1.0 / 22.0;
/// Band-width scale amplitude around 1.0. With noise in [-1, 1] and AMP
/// 0.45 the band scales over [0.55, 1.45] — a point bar can grow to ~45%
/// wider than the baseline, or tighten to ~55% of it. Clamped below
/// against `water_half + 0.5 m` so the water edge always has a minimal
/// sand strip regardless of dips.
pub(super) const RIVER_BAND_NOISE_AMP: f32 = 0.45;
/// Octave count for the along-river band noise. 2 octaves give a smooth
/// primary wave with one layer of fine jitter; more octaves push the
/// variation down into sub-10 m wobble that reads as texture noise rather
/// than geomorphic shape.
pub(super) const RIVER_BAND_NOISE_OCTAVES: u32 = 2;
/// Two rounds smooth 8 m source vertices into a visible curve at 1 m tile
/// resolution.
pub(super) const RIVER_CHAIKIN_ITERATIONS: u32 = 2;

// --- Road splat ---------------------------------------------------------
/// Half-width (m) of the pure road surface. Points within this distance of the
/// road polyline render as 100% PAL_ROAD.
pub(super) const ROAD_HALF_WIDTH_M: f32 = 2.0;
/// Distance (m) past the pure-road band over which the splat fades to pure
/// GROUND. Matches the plain branch's inner edge so crossing the outer edge is
/// a weight shift, not a palette swap.
pub(super) const ROAD_FADE_SPAN_M: f32 = 2.0;
pub(super) const ROAD_CHAIKIN_ITERATIONS: u32 = 2;

// --- Splat classification thresholds -------------------------------------
/// Distance (m) from the coast polyline within which a land cell renders as
/// the sand band. Replaces the prior `COAST_SAND_CELLS = 1.33 cells × 8 m =
/// 10.67 m` threshold; equivalent radius, no longer locked to the 8 m
/// global-cell lattice so the sand line follows the smoothed polyline at
/// sub-meter precision.
pub(super) const COAST_SAND_M: f32 = 10.0;
/// Distance (m) past the sand band over which the plain branch's slope-based
/// dirt fades in from 0. Width 0 at the band edge → full at `COAST_SAND_M +
/// COAST_FADE_SPAN_M`. Keeps the SAND→DIRT palette swap hidden (both sides
/// 100% GROUND at the swap point).
pub(super) const COAST_FADE_SPAN_M: f32 = 16.0;
/// Chaikin iterations applied to each coast polyline. Marching-squares
/// emits axis-aligned segments at 8 m cell spacing; two rounds soften
/// those into a curve at 1 m tile resolution, matching rivers/roads.
pub(super) const COAST_CHAIKIN_ITERATIONS: u32 = 2;
/// Distance (m) past the river sand band over which plain dirt fades in.
/// Matches the river carve taper so slope returns to plain baseline right
/// as the fade completes.
pub(super) const RIVER_FADE_SPAN_M: f32 = 10.0;
/// Radius (m) the sea shader uses to fade its shoreline-foam band
/// toward zero near river mouths. Encoded into the splatmap's byte-1
/// channel as a 0..255 linear ramp (0 on the river centerline, 255 at
/// or past this radius). Larger = wider foam-free zone around every
/// river outlet. Sized to match / exceed the client ribbon's sea
/// extension (`SEA_EXTEND_METERS`) so the full extended delta sits in
/// the suppression zone.
pub(super) const RIVER_FOAM_SUPPRESS_RADIUS_M: f32 = 30.0;
/// Absolute elevation (m) at which the snow→rock blend starts fading in.
pub(super) const SNOW_ELEVATION_M: f32 = 1800.0;
/// Elevation (m) above `SNOW_ELEVATION_M` at which snow is fully dominant.
pub(super) const SNOW_FULL_SPAN_M: f32 = 400.0;
/// Slope (Δm per 1 m horizontal) at which rock is fully dominant in the
/// alpine branch's snow→cliff blend.
pub(super) const SLOPE_CLIFF_FULL: f32 = 2.5;
/// Slope at which bare marble cliff (PAL_CLIFF) takes over as primary. 1.0 ≈
/// tan(45°). Placed before alpine in the priority ladder, so a vertical face
/// on a snowy peak reads as rock rather than snow.
pub(super) const CLIFF_SLOPE_THRESHOLD: f32 = 1.0;
/// Slope at which non-cliff land cells start tinting with CLIFF as their
/// secondary (secondary path for isolated steep ridges that don't cross the
/// cliff-primary threshold). Fade spans ≈ 35°→45°.
pub(super) const CLIFF_FADE_START: f32 = 0.7;
/// Reach (m) of the cliff-proximity influence on non-cliff cells. Beyond
/// this the cliff texture contributes nothing.
pub(super) const CLIFF_PROXIMITY_RADIUS_M: f32 = 5.0;
/// "Core" distance (m) within which non-cliff cells still render as 100%
/// cliff texture. The distance grid is quantized at 1 m so cells adjacent
/// to the cliff sit at d ≈ 1 — without this core zone a linear/smoothstep
/// falloff at d = 1 gives only ~75% cliff, which reads as a visible step
/// against the cliff-primary branch's 100%. 1.5 m covers the 8-way
/// neighborhood (diagonal ≈ 1.41 m) with a little slack.
pub(super) const CLIFF_BLEND_CORE_M: f32 = 1.5;
/// Per-tile search radius (cells) for the nearest cliff when computing
/// proximity. Covers `CLIFF_PROXIMITY_RADIUS_M` plus a diagonal cell of
/// slack so boundary cells along diagonals still resolve correctly.
pub(super) const CLIFF_PROXIMITY_SEARCH_CELLS: i32 = 6;
/// Max depth (m) used to map sea bathymetry blend 0..=255.
pub(super) const SEA_MAX_DEPTH_FOR_BLEND: f32 = 10.0;
/// Elevation band (m) for grass-density falloff: grass thins toward this height.
pub(super) const GRASS_FALLOFF_ELEVATION_M: f32 = 1600.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_json_schema_matches_constants() {
        let meta: serde_json::Value =
            serde_json::from_str(PALETTE_JSON).expect("shared/palette.json is valid JSON");
        let layers = meta
            .get("layers")
            .and_then(|l| l.as_array())
            .expect("layers array");
        assert_eq!(layers.len(), PAL_RIVER_BED as usize + 1);
        for layer in layers {
            assert!(layer.get("texture").and_then(|t| t.as_str()).is_some());
            assert!(layer.get("tileScale").and_then(|t| t.as_f64()).is_some());
            let color = layer
                .get("minimapColor")
                .and_then(|c| c.as_array())
                .expect("minimapColor array");
            assert_eq!(color.len(), 3);
            for c in color {
                let v = c.as_u64().expect("minimapColor channel is u8");
                assert!(v <= 255);
            }
        }
    }
}
