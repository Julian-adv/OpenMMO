//! 64×64×4 V2 splatmap classification: priority ladder, vegMeta encoding,
//! cliff proximity, and the per-tile classification pass.

use super::super::global_map::GlobalMap;
use super::super::grass_patches::PatchSample;
use super::super::noise::smoothstep;
use super::super::vector_features::{
    min_distance_to_segments, nearest_river_segment, RiverSegment, Segment,
};
use super::constants::{
    CLIFF_BLEND_CORE_M, CLIFF_FADE_START, CLIFF_PROXIMITY_RADIUS_M, CLIFF_PROXIMITY_SEARCH_CELLS,
    CLIFF_SLOPE_THRESHOLD, COAST_FADE_SPAN_M, COAST_SAND_M, GRASS_FALLOFF_ELEVATION_M, PAL_CLIFF,
    PAL_GROUND, PAL_RIVER_BED, PAL_ROAD, PAL_SAND, PAL_SNOW, RIVER_FADE_SPAN_M,
    RIVER_SAND_WIDTH_MULT, ROAD_FADE_SPAN_M, ROAD_HALF_WIDTH_M, SEA_MAX_DEPTH_FOR_BLEND,
    SLOPE_CLIFF_FULL, SLOPE_CLIFF_START, SNOW_ELEVATION_M, SNOW_FULL_SPAN_M, TILE_DIM,
    VERTS_PER_SIDE,
};
use super::context::BakeContext;
use super::heightmap::lerp;

/// Pack one splat cell into 4 bytes following the V2 layout
/// (`doc/SPLATMAP_V2.md`).
#[inline]
fn pack_splat(primary: u8, secondary: u8, blend: u8, veg: u8) -> [u8; 4] {
    [
        ((primary & 0x0F) << 4) | (secondary & 0x0F),
        0, // reserved (byte 1)
        blend,
        veg,
    ]
}

/// Short-grass vegMeta bytes live in 230..=239; pack a 0..=9 density there.
#[inline]
fn short_grass_veg(density: u8) -> u8 {
    230 + density.min(9)
}

/// Tall-grass vegMeta bytes live in 240..=249; pack a 0..=9 density there.
#[inline]
fn tall_grass_veg(density: u8) -> u8 {
    240 + density.min(9)
}

/// Euclidean distance (cells ≡ meters) from cell `(cx, cz)` to the nearest
/// `true` cell in the tile's cliff mask, clamped to
/// `CLIFF_PROXIMITY_RADIUS_M` when the search box contains no cliff. O(1)
/// per cell since the search box is bounded.
fn nearest_cliff_distance(mask: &[bool], cx: usize, cz: usize) -> f32 {
    let r = CLIFF_PROXIMITY_SEARCH_CELLS;
    let mut best_sq = (CLIFF_PROXIMITY_RADIUS_M * CLIFF_PROXIMITY_RADIUS_M) + 1.0;
    let x0 = (cx as i32 - r).max(0);
    let z0 = (cz as i32 - r).max(0);
    let x1 = (cx as i32 + r).min(TILE_DIM as i32 - 1);
    let z1 = (cz as i32 + r).min(TILE_DIM as i32 - 1);
    for nz in z0..=z1 {
        for nx in x0..=x1 {
            if !mask[nz as usize * TILE_DIM + nx as usize] {
                continue;
            }
            let dx = nx - cx as i32;
            let dz = nz - cz as i32;
            let d_sq = (dx * dx + dz * dz) as f32;
            if d_sq < best_sq {
                best_sq = d_sq;
            }
        }
    }
    best_sq.sqrt().min(CLIFF_PROXIMITY_RADIUS_M)
}

pub(super) fn bake_splatmap(
    map: &GlobalMap,
    ctx: &BakeContext,
    tx: i32,
    tz: i32,
    heights: &[f32],
    river_segs: &[RiverSegment],
    road_segs: &[Segment],
    coast_segs: &[Segment],
) -> Vec<u8> {
    let cfg = &map.config;
    let world_size = cfg.world_size_m as f32;
    let inv_mpc = 1.0 / cfg.meters_per_cell();
    let res = cfg.global_res as usize;

    let tile_origin_x = tx as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;
    let tile_origin_z = tz as f32 * TILE_DIM as f32 - TILE_DIM as f32 * 0.5;

    let mut out = vec![0u8; TILE_DIM * TILE_DIM * 4];

    // --- Pass 1: per-cell slope from the 4 tile vertices, plus the cliff
    // mask. Edge softness is carried entirely by pass 2's proximity blend,
    // so the mask stays faithful to the actually-steep terrain. -----------
    let mut slope_grid = vec![0.0f32; TILE_DIM * TILE_DIM];
    let mut cliff_mask = vec![false; TILE_DIM * TILE_DIM];
    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            let h00 = heights[cz * VERTS_PER_SIDE + cx];
            let h10 = heights[cz * VERTS_PER_SIDE + cx + 1];
            let h01 = heights[(cz + 1) * VERTS_PER_SIDE + cx];
            let h11 = heights[(cz + 1) * VERTS_PER_SIDE + cx + 1];
            let dzdx = ((h10 + h11) - (h00 + h01)) * 0.5;
            let dzdy = ((h01 + h11) - (h00 + h10)) * 0.5;
            let slope = (dzdx * dzdx + dzdy * dzdy).sqrt();
            let idx = cz * TILE_DIM + cx;
            slope_grid[idx] = slope;
            cliff_mask[idx] = slope >= CLIFF_SLOPE_THRESHOLD;
        }
    }

    // --- Pass 2: classification. For non-cliff cells, find distance to the
    // nearest cliff cell within `CLIFF_PROXIMITY_SEARCH_CELLS` and fold that
    // into the plain branch's blend so the cliff texture bleeds out by
    // `CLIFF_PROXIMITY_RADIUS_M` meters. -----------------------------------
    for cz in 0..TILE_DIM {
        for cx in 0..TILE_DIM {
            let wx = tile_origin_x + cx as f32 + 0.5;
            let wz = tile_origin_z + cz as f32 + 0.5;

            let gx = ((wx + world_size * 0.5) * inv_mpc).floor() as i32;
            let gy = ((wz + world_size * 0.5) * inv_mpc).floor() as i32;
            let gi =
                (gy.clamp(0, res as i32 - 1) as usize) * res + (gx.rem_euclid(res as i32) as usize);

            let h00 = heights[cz * VERTS_PER_SIDE + cx];
            let h10 = heights[cz * VERTS_PER_SIDE + cx + 1];
            let h01 = heights[(cz + 1) * VERTS_PER_SIDE + cx];
            let h11 = heights[(cz + 1) * VERTS_PER_SIDE + cx + 1];
            let h_center = (h00 + h10 + h01 + h11) * 0.25;

            let idx = cz * TILE_DIM + cx;
            let slope = slope_grid[idx];
            // 0.0 at cliff boundary (inclusive), CLIFF_PROXIMITY_RADIUS_M for
            // cells with no cliff in sight. Neighbor-tile cliffs aren't
            // visible — proximity near a tile edge stops at the edge, which
            // manifests as a slight softness asymmetry across seams. Fine at
            // the current scale.
            let cliff_proximity_m = if cliff_mask[idx] {
                0.0
            } else {
                nearest_cliff_distance(&cliff_mask, cx, cz)
            };

            let (river_d_m, river_width_m) = nearest_river_segment(wx, wz, river_segs)
                .map(|(d, idx, t)| {
                    let s = &river_segs[idx];
                    (d, lerp(s.width_a, s.width_b, t))
                })
                .unwrap_or((f32::INFINITY, 0.0));

            // `classify_splat` only consumes the patch sample in its plain
            // branch; passing a closure skips the warp + Voronoi query on
            // every sea / road / river / cliff / alpine / coast cell.
            let (primary, secondary, blend, veg) = classify_splat(
                SplatInputs {
                    is_sea: map.land_mask[gi] == 0,
                    river_d_m,
                    river_width_m,
                    road_d_m: min_distance_to_segments(wx, wz, road_segs),
                    h_center,
                    slope,
                    coast_d_m: min_distance_to_segments(wx, wz, coast_segs),
                    cliff_proximity_m,
                },
                || ctx.grass_patches.sample(wx, wz),
            );

            let off = (cz * TILE_DIM + cx) * 4;
            let bytes = pack_splat(primary, secondary, blend, veg);
            out[off..off + 4].copy_from_slice(&bytes);
        }
    }

    out
}

/// Per-cell inputs to `classify_splat`. Grouped so the priority ladder
/// doesn't become a 9-positional-arg call; also lets new biome factors
/// land as struct fields without churning every test call site.
#[derive(Debug, Clone, Copy)]
struct SplatInputs {
    is_sea: bool,
    river_d_m: f32,
    /// Width of the nearest river at its projection point (m). Zero when
    /// no river is in range — the priority ladder falls through naturally.
    river_width_m: f32,
    road_d_m: f32,
    h_center: f32,
    slope: f32,
    coast_d_m: f32,
    cliff_proximity_m: f32,
}

/// Splat priority ladder. Later branches only fire if earlier ones reject.
/// `patch` is invoked only when the plain branch fires, so non-plain cells
/// skip the warped-Voronoi query entirely.
fn classify_splat(inputs: SplatInputs, patch: impl FnOnce() -> PatchSample) -> (u8, u8, u8, u8) {
    let SplatInputs {
        is_sea,
        river_d_m,
        river_width_m,
        road_d_m,
        h_center,
        slope,
        coast_d_m,
        cliff_proximity_m,
    } = inputs;
    // 1 m floor on the sand band protects degenerate zero-width segments
    // from disappearing entirely.
    let river_sand_half_width_m = (river_width_m * RIVER_SAND_WIDTH_MULT).max(1.0);
    if road_d_m < ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M {
        // Roads override every biome so the network stays visible. Secondary
        // is PAL_GROUND so the fade outer edge (blend=255 → pure GROUND) meets
        // the plain branch's inner edge (also pure GROUND) without a
        // palette-swap pop.
        let t = ((road_d_m - ROAD_HALF_WIDTH_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        (PAL_ROAD, PAL_GROUND, blend, 0)
    } else if river_d_m < river_sand_half_width_m {
        // River bed: wet pebbles (PAL_RIVER_BED). Secondary switches to CLIFF
        // inside the cliff-proximity reach so the river-edge outer ring
        // resolves to 100% CLIFF on both sides of the palette-pair swap
        // against the adjacent plain cell; otherwise GROUND, meeting pure
        // grass outside the sand band.
        let t = (river_d_m / river_sand_half_width_m).clamp(0.0, 1.0);
        let blend = (t * t * 255.0) as u8;
        let (secondary, veg) = if cliff_proximity_m < CLIFF_PROXIMITY_RADIUS_M {
            (PAL_CLIFF, 0)
        } else {
            let rocky = (slope / SLOPE_CLIFF_START).clamp(0.0, 1.0);
            let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
            let grass_t = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
            // `* t` fades veg density from 0 at the center to plain density
            // at the edge so the short-grass mesh doesn't pop on at the
            // boundary.
            let density = (grass_t * 9.0 * t).round().clamp(0.0, 9.0) as u8;
            let v = if density > 0 {
                short_grass_veg(density)
            } else {
                0
            };
            (PAL_GROUND, v)
        };
        (PAL_RIVER_BED, secondary, blend, veg)
    } else if is_sea {
        // Secondary = GROUND so the coast line shares a palette pair with
        // the land sand-band, keeping per-texture weights continuous
        // across the shoreline (a DIRT secondary here would abruptly
        // introduce laterite on every adjacent land cell).
        let depth = (-h_center).max(0.0);
        let blend = ((depth / SEA_MAX_DEPTH_FOR_BLEND).clamp(0.0, 1.0) * 255.0) as u8;
        (PAL_SAND, PAL_GROUND, blend, 0)
    } else if slope >= CLIFF_SLOPE_THRESHOLD {
        // ≥45° face: exposed marble cliff. Placed before alpine so steep
        // faces on snowy peaks still read as rock, not snow. Secondary =
        // GROUND keeps the palette pair consistent with the cliff-fade
        // branch below (just swapped primary/secondary), so there's no
        // texture discontinuity at the threshold — both sides resolve to
        // 100% CLIFF when slope is right at 1.0.
        (PAL_CLIFF, PAL_GROUND, 0, 0)
    } else if h_center > SNOW_ELEVATION_M {
        // Alpine: snow with cliff showing through on exposed slopes, so the
        // rocky blend matches the adjacent cliff patch color rather than
        // introducing a third ground texture.
        let t = ((h_center - SNOW_ELEVATION_M) / SNOW_FULL_SPAN_M).clamp(0.0, 1.0);
        let rocky = (slope / SLOPE_CLIFF_FULL).clamp(0.0, 1.0);
        let blend = (((1.0 - t) * 120.0).max(rocky * 200.0)) as u8;
        (PAL_SNOW, PAL_CLIFF, blend, 0)
    } else if !is_sea && coast_d_m <= COAST_SAND_M {
        // Quadratic blend keeps cells right at the water line near 100%
        // SAND so per-texture weights stay continuous with the adjacent
        // sea cell; a linear ramp would introduce ~25% GROUND right at the
        // shore and read as a staircase. Grass density has a floor of 1 so
        // the mesh fringe matches the adjacent plains' density of 9.
        // (`!is_sea` guard: distance alone has no sign, so a sea cell inside
        // a narrow inlet that's <COAST_SAND_M from the coast line would
        // otherwise compete for the sand band — land_mask is the source of
        // truth for the side.)
        const DENSITY_MIN: f32 = 1.0;
        let t = (coast_d_m / COAST_SAND_M).clamp(0.0, 1.0);
        let blend_f = t * t;
        let density = (DENSITY_MIN + (9.0 - DENSITY_MIN) * t)
            .round()
            .clamp(DENSITY_MIN, 9.0) as u8;
        (
            PAL_SAND,
            PAL_GROUND,
            (blend_f * 255.0) as u8,
            short_grass_veg(density),
        )
    } else {
        // Plain / slope branch: GROUND primary, CLIFF secondary. Cliff bleeds
        // in via TWO channels and we take the max:
        //   (1) slope-based: smoothstep over [CLIFF_FADE_START, threshold].
        //       Fires on isolated steep ridges where the slope never quite
        //       reaches the cliff-primary threshold.
        //   (2) distance-based: proximity to a neighbor cell that DID cross
        //       the threshold, falling off linearly over
        //       CLIFF_PROXIMITY_RADIUS_M meters. This is what gives cliff
        //       edges a 2–3 cell gradient even when the actual slope
        //       transition is sharp enough that the slope-based channel
        //       barely fires.
        // Both channels meet the cliff-primary branch at 100% CLIFF (blend
        // 255 on this side, primary CLIFF on that side), so crossing the
        // threshold is a palette-pair swap with identical resolved colors.
        // `water_fade` holds blend near zero along coast / river / road
        // bands so banks stay clean grass.
        let rocky_slope = smoothstep(CLIFF_FADE_START, CLIFF_SLOPE_THRESHOLD, slope);
        // Dilation-then-smoothstep: cells within CORE_M stay at 1.0 so the
        // first ring around a cliff matches the cliff-primary branch's 100%
        // CLIFF exactly (no visible step), then fade smoothly to 0 over the
        // remaining `RADIUS - CORE` meters.
        let d_eff = (cliff_proximity_m - CLIFF_BLEND_CORE_M).max(0.0);
        let fade_span = (CLIFF_PROXIMITY_RADIUS_M - CLIFF_BLEND_CORE_M).max(1e-3);
        let rocky_proximity = 1.0 - smoothstep(0.0, fade_span, d_eff);
        let highland = (h_center / GRASS_FALLOFF_ELEVATION_M).clamp(0.0, 1.0);
        // Water_fade is what keeps road/river/coast banks looking like clean
        // grass rather than picking up a slope tint from noise in the height
        // field. Applying it to `rocky_slope` is fine: a shallow-gradient
        // ridge near a river shouldn't read as rocky. But applying it to the
        // distance-to-cliff channel would erase cliff influence next to
        // actual cliffs that descend into water — e.g. the river cutting at
        // the base of a cliff. So proximity bypasses water_fade entirely.
        let coast_fade = ((coast_d_m - COAST_SAND_M) / COAST_FADE_SPAN_M).clamp(0.0, 1.0);
        let river_fade =
            ((river_d_m - river_sand_half_width_m) / RIVER_FADE_SPAN_M).clamp(0.0, 1.0);
        let road_fade =
            ((road_d_m - ROAD_HALF_WIDTH_M - ROAD_FADE_SPAN_M) / ROAD_FADE_SPAN_M).clamp(0.0, 1.0);
        let water_fade = coast_fade.min(river_fade).min(road_fade);
        let rocky = (rocky_slope * water_fade).max(rocky_proximity);
        // Density combines hard exclusions (rocky, highland) with the
        // patch-field mask (0 outside patches, 1 inside, smooth fade at the
        // edge). The patch sample is only pulled here, so non-plain cells
        // skip the warp + Voronoi query entirely.
        let eligibility = (1.0 - rocky).max(0.0) * (1.0 - highland).max(0.0);
        let patch = patch();
        let density = (patch.strength * eligibility * 9.0).round().clamp(0.0, 9.0) as u8;
        let veg = if density > 0 {
            if patch.is_tall {
                tall_grass_veg(density)
            } else {
                short_grass_veg(density)
            }
        } else {
            0
        };
        let blend = (rocky * 255.0) as u8;
        (PAL_GROUND, PAL_CLIFF, blend, veg)
    }
}

#[cfg(test)]
mod tests {
    use super::super::constants::{
        RIVER_CARVE_TAPER_EXTRA_M, RIVER_CARVE_TAPER_MIN_M, RIVER_MAX_WIDTH_M,
    };
    use super::*;

    /// Flat plain far from any feature. Slope is 0 so the plain branch's
    /// rocky component is 0 and any blend value reflects only coastal fades,
    /// not terrain. Rivers and roads default to INFINITY so those branches
    /// are inactive.
    fn plain_inputs(coast_d_m: f32) -> SplatInputs {
        SplatInputs {
            is_sea: false,
            river_d_m: f32::INFINITY,
            river_width_m: 0.0,
            road_d_m: f32::INFINITY,
            h_center: 10.0,
            slope: 0.0,
            coast_d_m,
            cliff_proximity_m: CLIFF_PROXIMITY_RADIUS_M,
        }
    }

    /// Canonical width used by river-band tests. With `RIVER_SAND_WIDTH_MULT
    /// = 1.25`, a width of 4 m yields a 5 m sand band — the value the
    /// previous fixed `RIVER_SAND_HALF_WIDTH_M` constant used, so existing
    /// regression expectations carry over.
    const TEST_RIVER_WIDTH_M: f32 = 4.0;
    const TEST_RIVER_SAND_HALF_WIDTH_M: f32 = TEST_RIVER_WIDTH_M * RIVER_SAND_WIDTH_MULT;

    /// Full-coverage short-grass patch used by tests that don't care about
    /// veg output. Passed as `|| FULL_GRASS` where `classify_splat` wants a
    /// `FnOnce() -> PatchSample`.
    const FULL_GRASS: PatchSample = PatchSample {
        strength: 1.0,
        is_tall: false,
    };

    fn call_classify(inputs: SplatInputs) -> (u8, u8, u8, u8) {
        classify_splat(inputs, || FULL_GRASS)
    }

    #[test]
    fn coast_water_line_is_pure_sand() {
        // At coast_d=0 (adjacent to sea), the sand band must render 100% SAND
        // so it visually connects to the shoreline.
        let (p, s, blend, _) = call_classify(plain_inputs(0.0));
        assert_eq!((p, s), (PAL_SAND, PAL_GROUND));
        assert_eq!(blend, 0, "expected pure SAND primary at water line");
    }

    #[test]
    fn coast_outer_edge_meets_plain_branch_seamlessly() {
        // The design invariant that lets the palette pair swap
        // ((SAND,GROUND) → (GROUND,DIRT)) be visually invisible: at exactly
        // `coast_d_m = COAST_SAND_M`, the coast branch emits blend=255 (100%
        // GROUND secondary) and the plain branch emits blend=0 (100% GROUND
        // primary). Both sides render pure GROUND, so the shader's
        // corner-sampled bilerp sees no texture discontinuity.
        let (cp, cs, c_blend, _) = call_classify(plain_inputs(COAST_SAND_M));
        assert_eq!((cp, cs), (PAL_SAND, PAL_GROUND));
        assert_eq!(c_blend, 255, "coast outer edge must be pure GROUND");

        let (pp, ps, p_blend, _) = call_classify(plain_inputs(COAST_SAND_M + 1e-4));
        assert_eq!((pp, ps), (PAL_GROUND, PAL_CLIFF));
        assert_eq!(p_blend, 0, "plain at band edge must be pure GROUND");
    }

    #[test]
    fn coast_blend_monotonic_across_band() {
        // Quadratic `t²` ramp: strictly non-decreasing from 0 at water to 255
        // at the outer edge. Any regression that breaks monotonicity would
        // reintroduce visible bands inside the beach.
        let steps = 16;
        let mut prev = -1i32;
        for i in 0..=steps {
            let d = COAST_SAND_M * (i as f32) / (steps as f32);
            let (_, _, blend, _) = call_classify(plain_inputs(d));
            assert!(
                blend as i32 >= prev,
                "non-monotonic coast blend at d={}: {} < {}",
                d,
                blend,
                prev
            );
            prev = blend as i32;
        }
    }

    #[test]
    fn priority_road_beats_sea_and_river() {
        // Roads must always win so the settlement network stays visible.
        let (p, s, blend, _) = call_classify(SplatInputs {
            is_sea: true,
            river_d_m: 1.0,
            river_width_m: TEST_RIVER_WIDTH_M,
            road_d_m: 0.0,
            h_center: -5.0,
            coast_d_m: 0.0,
            ..plain_inputs(0.0)
        });
        assert_eq!((p, s), (PAL_ROAD, PAL_GROUND));
        assert_eq!(blend, 0, "road center must be 100% PAL_ROAD");
    }

    #[test]
    fn priority_river_beats_sea() {
        // River bed uses (RIVER_BED, GROUND). Regression guard so an
        // accidental swap back to DIRT (red laterite) won't slip through.
        let (p, s, _, _) = call_classify(SplatInputs {
            is_sea: true,
            river_d_m: 0.0,
            river_width_m: TEST_RIVER_WIDTH_M,
            h_center: -1.0,
            coast_d_m: 0.0,
            ..plain_inputs(0.0)
        });
        assert_eq!((p, s), (PAL_RIVER_BED, PAL_GROUND));
    }

    #[test]
    fn river_outer_edge_meets_plain_seamlessly() {
        // Same continuity invariant as `coast_outer_edge_...` but for rivers.
        let at_edge = call_classify(SplatInputs {
            river_d_m: TEST_RIVER_SAND_HALF_WIDTH_M - 1e-4,
            river_width_m: TEST_RIVER_WIDTH_M,
            ..plain_inputs(100.0)
        });
        assert_eq!((at_edge.0, at_edge.1), (PAL_RIVER_BED, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "river edge must be near-pure GROUND");

        let past_edge = call_classify(SplatInputs {
            river_d_m: TEST_RIVER_SAND_HALF_WIDTH_M + 1e-4,
            river_width_m: TEST_RIVER_WIDTH_M,
            ..plain_inputs(100.0)
        });
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_CLIFF));
        assert_eq!(
            past_edge.2, 0,
            "plain just past river edge must be pure GROUND"
        );
    }

    #[test]
    fn road_outer_edge_meets_plain_seamlessly() {
        // Continuity invariant for the road band: at the outer edge the road
        // branch must emit blend=255 (pure GROUND secondary), and the plain
        // branch just past must emit blend=0 (pure GROUND primary). If either
        // side drifts, a visible seam appears along every road.
        let at_edge = call_classify(SplatInputs {
            road_d_m: ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M - 1e-4,
            ..plain_inputs(100.0)
        });
        assert_eq!((at_edge.0, at_edge.1), (PAL_ROAD, PAL_GROUND));
        assert_eq!(at_edge.2, 254, "road edge must be near-pure GROUND");

        let past_edge = call_classify(SplatInputs {
            road_d_m: ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M + 1e-4,
            ..plain_inputs(100.0)
        });
        assert_eq!((past_edge.0, past_edge.1), (PAL_GROUND, PAL_CLIFF));
        assert_eq!(
            past_edge.2, 0,
            "plain just past road edge must be pure GROUND"
        );
    }

    #[test]
    fn road_blend_monotonic_across_band() {
        // The road branch's blend curve (t² over the fade region) must be
        // non-decreasing from 0 at the pure-road core to 255 at the outer
        // edge. Any regression that breaks monotonicity would resurrect
        // visible bands inside a single road. We sample strictly inside the
        // band — past the outer edge the palette pair swaps (ROAD,GROUND) →
        // (GROUND,DIRT) and raw blend bytes are no longer comparable.
        let steps = 24;
        let band_end = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M - 1e-3;
        let mut prev = -1i32;
        for i in 0..=steps {
            let d = band_end * (i as f32) / (steps as f32);
            let (primary, _, blend, _) = call_classify(SplatInputs {
                road_d_m: d,
                ..plain_inputs(100.0)
            });
            assert_eq!(
                primary, PAL_ROAD,
                "road branch must still be active at d={d}"
            );
            assert!(
                blend as i32 >= prev,
                "non-monotonic road blend at d={d}: {blend} < {prev}"
            );
            prev = blend as i32;
        }
    }

    #[test]
    fn road_margin_covers_plain_fade_span() {
        // At the road margin distance, the plain branch's splat output must
        // match the "no road in sight" output. Otherwise a tile whose filter
        // excludes a road segment just past its margin would render a
        // different blend than the neighbor tile that includes it — a hard
        // seam along the tile boundary.
        let road_margin = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M * 2.0;
        // Slope 0.5 < SLOPE_CLIFF_START so the plain branch fires; its rocky
        // component is non-zero so a mismatched water_fade would surface as a
        // blend diff.
        let at_margin = call_classify(SplatInputs {
            road_d_m: road_margin,
            slope: 0.5,
            ..plain_inputs(100.0)
        });
        let no_road = call_classify(SplatInputs {
            slope: 0.5,
            ..plain_inputs(100.0)
        });
        assert_eq!(
            at_margin, no_road,
            "plain branch must match 'no road' output at road_margin"
        );
    }

    #[test]
    fn adjacent_tiles_see_same_nearby_road_segment() {
        // Same boundary regression guard as the river version but with the
        // (smaller) road margin. A road segment `road_margin - 1` m outside
        // a tile bbox must still appear in that tile's filter list so the
        // plain branch's road_fade matches across tile edges.
        use super::super::super::vector_features::{segments_near_tile, WorldPolyline};
        let road_margin = ROAD_HALF_WIDTH_M + ROAD_FADE_SPAN_M * 2.0;
        let polys = vec![WorldPolyline {
            points: vec![
                [32.0 - (road_margin - 1.0), -10.0],
                [32.0 - (road_margin - 1.0), 10.0],
            ],
        }];
        let near = segments_near_tile(&polys, 32.0, -16.0, 96.0, 16.0, road_margin);
        assert_eq!(
            near.len(),
            1,
            "tile must see road segment {} m west of its bbox",
            road_margin - 1.0
        );
    }

    #[test]
    fn river_margin_covers_water_fade_span() {
        // Regression guard: the per-tile segment filter in `bake_tile` must
        // include every segment whose distance to the tile bbox could still
        // matter to the splat water-fade (max-width sand band + fade span)
        // or the heightmap carve (max-width half-width + max taper). The
        // margin is computed from the global maxima so every tile sees
        // every segment that could influence it.
        let max_half_width = RIVER_MAX_WIDTH_M * 0.5;
        let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
        let max_sand = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
        let river_margin = (max_half_width + max_taper).max(max_sand + RIVER_FADE_SPAN_M);
        assert!(
            river_margin >= max_sand + RIVER_FADE_SPAN_M,
            "margin {} does not cover fade span {}",
            river_margin,
            max_sand + RIVER_FADE_SPAN_M
        );
        assert!(
            river_margin >= max_half_width + max_taper,
            "margin {} does not cover carve taper {}",
            river_margin,
            max_half_width + max_taper
        );
    }

    #[test]
    fn adjacent_tiles_see_same_nearby_river_segment() {
        // Build a synthetic world with a single river polyline straddling
        // the tile boundary at x = 0 (tile 0's right edge = tile 1's left
        // edge). The per-tile filter in `bake_tile` uses
        // `river_segments_near_tile` with the `river_margin` constant —
        // both adjacent tiles must see the segment so their splat
        // classification agrees at the boundary.
        use super::super::super::vector_features::{
            river_segments_near_tile, RiverWorldPolyline,
        };
        let polys = vec![RiverWorldPolyline {
            points: vec![[0.5, -10.0], [0.5, 10.0]],
            flow_norm: vec![0.5, 0.5],
            width: vec![4.0, 4.0],
        }];
        let max_half_width = RIVER_MAX_WIDTH_M * 0.5;
        let max_taper = RIVER_CARVE_TAPER_MIN_M + RIVER_CARVE_TAPER_EXTRA_M;
        let max_sand = RIVER_MAX_WIDTH_M * RIVER_SAND_WIDTH_MULT;
        let margin = (max_half_width + max_taper).max(max_sand + RIVER_FADE_SPAN_M);
        let near_tile_0 = river_segments_near_tile(&polys, -32.0, -16.0, 32.0, 16.0, margin);
        assert_eq!(near_tile_0.len(), 1, "tile 0 must see segment at x=0.5");
        let polys2 = vec![RiverWorldPolyline {
            points: vec![
                [32.0 - (margin - 1.0), -10.0],
                [32.0 - (margin - 1.0), 10.0],
            ],
            flow_norm: vec![0.5, 0.5],
            width: vec![4.0, 4.0],
        }];
        let near_tile_1 = river_segments_near_tile(&polys2, 32.0, -16.0, 96.0, 16.0, margin);
        assert_eq!(
            near_tile_1.len(),
            1,
            "tile 1 must see segment {} m west of its bbox",
            margin - 1.0
        );
    }

    #[test]
    fn non_plain_branches_do_not_sample_patch() {
        // Efficiency regression guard: the warped-Voronoi query is ~the
        // dominant cost of splat classification, so every non-plain branch
        // MUST NOT invoke the patch closure. A panic-on-call closure catches
        // any branch that accidentally starts pulling the sample eagerly.
        let trip = || -> PatchSample { panic!("patch must not be sampled") };

        // Road
        classify_splat(
            SplatInputs {
                road_d_m: 0.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // River (before sea check — river wins even in sea)
        classify_splat(
            SplatInputs {
                is_sea: true,
                river_d_m: 0.0,
                river_width_m: TEST_RIVER_WIDTH_M,
                h_center: -1.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Sea
        classify_splat(
            SplatInputs {
                is_sea: true,
                h_center: -5.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Cliff (slope ≥ threshold)
        classify_splat(
            SplatInputs {
                slope: CLIFF_SLOPE_THRESHOLD + 0.1,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Alpine (elevation > snow line, slope below cliff threshold)
        classify_splat(
            SplatInputs {
                h_center: SNOW_ELEVATION_M + 100.0,
                ..plain_inputs(100.0)
            },
            trip,
        );
        // Coast sand band
        classify_splat(plain_inputs(COAST_SAND_M * 0.5), trip);
    }

    #[test]
    fn plain_branch_samples_patch() {
        // Inverse guard: if the plain branch ever stops reading the patch
        // sample, every eligible land cell loses its veg byte silently.
        use std::cell::Cell;
        let called = Cell::new(false);
        classify_splat(plain_inputs(COAST_SAND_M + 1.0), || {
            called.set(true);
            FULL_GRASS
        });
        assert!(called.get(), "plain branch must sample patch");
    }
}
