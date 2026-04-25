//! Shared tuning constants for the Phase 7 tile baker.

/// Cell-count side of the splatmap (64×64 cells per tile).
pub const TILE_DIM: usize = 64;
/// Vertex-count side of the heightmap (65×65, overlaps neighbor by 1).
pub const VERTS_PER_SIDE: usize = TILE_DIM + 1;

/// Heightmap encoding: 10000 → 0.0 m, step 0.05 m. Covers -500..+2776 m.
pub(super) const HEIGHT_BIAS: f32 = 500.0;
pub(super) const HEIGHT_STEP: f32 = 0.05;

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
pub const RIVER_MIN_WIDTH_M: f32 = 1.5;
pub const RIVER_MAX_WIDTH_M: f32 = 10.0;
pub(super) const RIVER_CARVE_TAPER_MIN_M: f32 = 3.0;
pub(super) const RIVER_CARVE_TAPER_EXTRA_M: f32 = 7.0;
pub(super) const RIVER_CARVE_DEPTH_MIN_M: f32 = 0.6;
pub(super) const RIVER_CARVE_DEPTH_EXTRA_M: f32 = 1.4;
/// Lower bound on post-carve terrain elevation inside a river channel
/// (meters). Stops carving from dragging the bed deep enough that the
/// ocean shader floods the channel with shore/wet-sand patterns. Tuned
/// just below sea level — at -0.1 m the river bed sits in the sea
/// shader's hard-cut zone (depth < 0.01 m → α=0) so the ocean doesn't
/// render inside the channel, while still leaving enough headroom under
/// the river surface (centerY = bed + RIVER_DEPTH_OFFSET_M = 0.4 m at
/// the estuary) for the river shader's depth-based edge fade to produce
/// a visible bank gradient instead of stamping a uniform-opaque slab.
/// See RIVER_SYSTEM.md §10.
pub(super) const RIVER_CARVE_MIN_BED_Y_M: f32 = -0.1;
/// River-bed splat switches from `PAL_RIVER_BED` (ganges pebbles — wet
/// inland bed look) to `PAL_SAND` (sandy_gravel_02 — matches coast) where
/// the cell is within this horizontal distance of the ocean coast
/// polyline. Horizontal distance rather than elevation because the bake's
/// carve floor holds h_center flat for ~40 m inland at gentle mouths; an
/// elevation cutoff clipped sand to just the first few meters while this
/// directly caps the delta length at the ocean-facing side.
pub(super) const RIVER_MOUTH_SAND_COAST_DIST_M: f32 = 50.0;
/// Width-fan window (meters of base-cell elevation). Below `LOW` the
/// polyline vertex is widened to `1 + EXTRA` of its natural width; above
/// `HIGH` it keeps the natural width. Applied globally to `rivers_world`
/// in `BakeContext::new` so heightmap carving, splatmap classification,
/// and the client ribbon all see the same fan-scaled widths — otherwise
/// the water surface plane widens past the carved banks.
///
/// Window is tuned so the fan opens close to the coast rather than
/// widening the river far upstream — a wider window reads as "river
/// is just wider here" instead of as a localized 부채꼴 delta. The
/// `~4 m of approach elevation` window translates to roughly
/// 20–40 m of horizontal polyline at typical coastal slopes. Past
/// the coast, the client sea extension tapers the wedge back to a
/// point (see `SEA_EXTEND_*` in `river-geometry.ts`), producing the
/// symmetric spindle-shaped delta centered on the coastline.
///
/// The fan is unbounded below `LOW_M` by design: `apply_mouth_fan_widths`
/// lets the J-curve keep climbing for underwater polyline vertices so
/// the rate of widening stays monotonic (no plateau at the coastline),
/// and the wedge taper bounds the final visual width regardless of
/// tip multiplier.
pub(super) const RIVER_MOUTH_FAN_BASE_LOW_M: f32 = 0.0;
pub(super) const RIVER_MOUTH_FAN_BASE_HIGH_M: f32 = 4.0;
pub(super) const RIVER_MOUTH_FAN_EXTRA: f32 = 2.5;
pub(super) const RIVER_SAND_WIDTH_MULT: f32 = 0.7;

// --- Mouth finger-islands ------------------------------------------------
// Procedural sandy bars scattered inside each river's estuary fan. Rise
// above the carved channel floor (sitting at `RIVER_CARVE_MIN_BED_Y_M`
// near the mouth) as elongated capsules aligned with the apex flow
// direction. The splatmap's mouth-pebble retraction already paints them
// SAND since they sit inside the river band with `coast_d_m` well below
// `RIVER_MOUTH_SAND_COAST_DIST_M`.
pub(super) const MOUTH_ISLAND_COUNT_MIN: u32 = 4;
pub(super) const MOUTH_ISLAND_COUNT_MAX: u32 = 5;
pub(super) const MOUTH_ISLAND_RADIUS_MIN_M: f32 = 3.0;
pub(super) const MOUTH_ISLAND_RADIUS_MAX_M: f32 = 5.0;
/// Normalised axis position (0=upstream tip, 1=downstream tip) at which
/// the island reaches its widest radius. 0.75 gives a teardrop with the
/// fat end pointing seaward — delta bars erode to a point on the
/// flow-facing edge and settle sediment on the downstream lee side.
pub(super) const MOUTH_ISLAND_WIDEST_AXIS_T: f32 = 0.75;
/// Axial height boost at the land-side tip (u=0), ramped linearly to
/// zero at the sea-side tip. Sediment stacks at the upstream head where
/// flow plants it; the downstream tail is drift-thinned.
pub(super) const MOUTH_ISLAND_LAND_HEIGHT_BOOST: f32 = 0.3;
/// Peak elevation range (m) ABOVE the post-carve sample. Sea cells sit
/// on bathymetry around −0.5 m, so the peak must budget for lifting the
/// surface up through the waterline AND leaving a visible dry crown.
/// The upper end of the range also budgets for the height attenuation
/// from `smooth_island_area`'s Gaussian pass.
pub(super) const MOUTH_ISLAND_PEAK_MIN_M: f32 = 1.1;
pub(super) const MOUTH_ISLAND_PEAK_MAX_M: f32 = 1.5;
/// Base elevation (m) at which a polyline vertex is considered the
/// "apex" of its mouth — the last point still on land. Islands spawn
/// downstream of here.
pub(super) const MOUTH_ISLAND_APEX_ELEV_M: f32 = 0.4;
/// Upstream-tip along-tangent bound from the apex. Small range bundles
/// all tips into a "wrist" cluster — the fingers-of-a-hand silhouette.
pub(super) const MOUTH_ISLAND_TIP_ALONG_MAX_M: f32 = 4.0;
/// Downstream-end along-tangent range from the apex. Kept inside the
/// pebble-wedge retraction zone (`RIVER_MOUTH_SAND_COAST_DIST_M = 50 m`)
/// so the whole bar classifies as sand.
pub(super) const MOUTH_ISLAND_END_ALONG_MIN_M: f32 = 16.0;
pub(super) const MOUTH_ISLAND_END_ALONG_MAX_M: f32 = 30.0;
/// Half-angle (radians) of the fan across which island slots are
/// evenly distributed. Slot-based angles guarantee angular separation;
/// pure random scatter collapses neighbours into overlap.
pub(super) const MOUTH_ISLAND_FAN_HALF_ANGLE_RAD: f32 = 0.7;
/// Small per-island angle jitter (radians) on top of the slot angle to
/// break perfect symmetry.
pub(super) const MOUTH_ISLAND_ANGLE_JITTER_RAD: f32 = 0.09;
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
