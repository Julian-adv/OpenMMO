//! Phase 4: flow accumulation and river extraction.
//!
//! On a heightmap, water flows from high elevation toward low. Each land
//! cell has a single "downstream" neighbor (the 8-connected neighbor with
//! the steepest descent). Flow accumulation counts, for every cell, how
//! many upstream cells drain into it — i.e. how much rain reaches that
//! point along the gradient field.
//!
//! Cells whose accumulation exceeds a threshold are rivers. Tracing each
//! such cell's downstream chain to the sea yields river polylines, which
//! subsequent phases (splatmap, vegetation) use to paint riverbeds and
//! tint vegetation density.

use std::collections::{BinaryHeap, HashMap};

use super::global_map::GlobalMap;
use super::grid::{MinF32, fold_x_delta};

/// Peak elevation threshold (as a fraction of `max_elevation_m`) for
/// `extract_rivers` to treat a local maximum as a river source. Shared
/// between the bake and preview pipelines so the same map produces the
/// same polylines in both, and used by Phase 4b's gap-fill to size its
/// seeded mountains comfortably above this bar.
pub const RIVER_PEAK_ELEVATION_FRAC: f32 = 0.3;

/// Nearby rivers flowing in nearly the same direction are visually read as
/// duplicate channels. Let the first traced main stem claim a small corridor;
/// later traces that enter it snap to the existing stem and stop there.
const PARALLEL_MERGE_RADIUS_FRAC: f32 = 0.003;
const PARALLEL_MERGE_RADIUS_MIN: i32 = 2;
const PARALLEL_MERGE_RADIUS_MAX: i32 = 12;
const PARALLEL_MERGE_MIN_DOT: f32 = 0.7;

/// D8 flow over broad lowlands locks onto a single cardinal / diagonal
/// direction for hundreds of cells. After extraction, displace each
/// non-anchor vertex perpendicular to its windowed tangent by a sum of
/// two octaves of sine noise indexed on arc-length. Amplitude scales
/// with flow (small streams barely wave, big rivers swing wide) and
/// tapers smoothly to zero on approach to *any* anchor — start, end,
/// or interior junction with another polyline — so confluence cells
/// stay shared across the trunk and its tributary. One-shot:
/// displacement is bounded a priori by the amplitude constants below.
const MEANDER_MIN_LENGTH_CELLS: f32 = 40.0;
/// Window radius for tangent / slope estimation. Larger = smoother
/// direction estimate but coarser local response.
const MEANDER_TANGENT_WINDOW: usize = 6;
/// Base displacement amplitude in cells, applied to every river that
/// passes the slope gate.
const MEANDER_BASE_AMPLITUDE_CELLS: f32 = 3.0;
/// Additional amplitude (cells) at the global max flow. The total
/// per-river amplitude is `base + flow · log_norm`, so a tributary
/// with 1% of the max flow swings only a fraction of the trunk's
/// amplitude. Choose generous enough to read as a bend at preview
/// scale; 28 cells ≈ 224 m at the typical 8 m/cell spacing.
const MEANDER_FLOW_AMPLITUDE_CELLS: f32 = 28.0;
/// Wavelength (cells) of the dominant noise octave. The visible bend
/// scale is roughly `wavelength / 2` between zero-crossings; at 240
/// cells / ~1.9 km that's a single S over each kilometre or so —
/// big enough to read as a clear meander at world scale rather than
/// a per-screen wiggle.
const MEANDER_PRIMARY_WAVELENGTH_CELLS: f32 = 240.0;
/// Secondary octave wavelength and weight. A small fraction of a
/// shorter wave breaks the perfect sine symmetry without re-introducing
/// the busy "lots of small curves" feel. Set weight to 0 for a pure
/// single-frequency meander.
const MEANDER_SECONDARY_WAVELENGTH_CELLS: f32 = 110.0;
const MEANDER_SECONDARY_WEIGHT: f32 = 0.18;
/// Arc-distance (cells) over which displacement smoothly tapers to
/// zero at every anchor (endpoints + interior junctions). Scale with
/// wavelength: too short and the taper bends sharply at confluences;
/// too long and only the middle of each polyline gets to swing.
const MEANDER_ANCHOR_TAPER_CELLS: f32 = 80.0;
const MEANDER_SLOPE_LOW: f32 = 0.025;
const MEANDER_SLOPE_HIGH: f32 = 0.14;

/// Only rivers with apex flow at or above this fraction of the global
/// max get deltaized. Small streams reach the sea as a single channel.
const DISTRIBUTARY_FLOW_FRAC_OF_MAX: f32 = 0.05;
/// Forward log2 flow→width mapping range used when assigning child-branch
/// flows. Must match `tile_bake/constants.rs::RIVER_{MIN,MAX}_WIDTH_M`,
/// which is what the bake re-derives per vertex. Defined here (not
/// imported) because `tile_bake` already depends on `rivers` — importing
/// back would create a cycle.
const DISTRIBUTARY_WIDTH_MIN_M: f32 = 1.5;
const DISTRIBUTARY_WIDTH_MAX_M: f32 = 10.0;
/// Apex sits this many cells back from the river mouth (measured along
/// the polyline arc, not Euclidean). The original trunk is truncated at
/// the apex; distributary branches sprout from there.
const DISTRIBUTARY_APEX_OFFSET_CELLS: f32 = 8.5;
/// Maximum apex-to-mouth slope for a delta. Real deltas form on near-flat
/// coastal plains where the channel can't choose a single dominant path.
const DISTRIBUTARY_SLOPE_MAX: f32 = 0.015;
/// Branch endpoints must lie within this radius (Euclidean cells) of
/// the apex. Smaller radius = tighter fan, larger = wider spread.
const DISTRIBUTARY_SEARCH_RADIUS_CELLS: f32 = 35.0;
/// Branch endpoints must be at least this far from the apex; prevents
/// branches collapsing into a single short stub when the apex is right
/// on the coast.
const DISTRIBUTARY_SEARCH_MIN_RADIUS_CELLS: f32 = 16.0;
/// Half-angle (radians) of the forward arc inside which we look for
/// branch endpoints. ~0.7 rad ≈ 40° → total fan span of ~80°.
const DISTRIBUTARY_HALF_ARC_RAD: f32 = 0.7;
/// Number of distributary branches to spawn per delta. Real systems run
/// from 2 (simple bifurcation) through 4–5 (full birdfoot). Three is a
/// readable middle ground that avoids both the "Y-fork" simplicity of 2
/// and the visual clutter of 5.
const DISTRIBUTARY_BRANCH_COUNT: usize = 3;

#[derive(Clone, Copy)]
struct MergeClaim {
    target: u32,
    dir_x: i8,
    dir_y: i8,
    dist_sq: u16,
}

/// Priority-queue-based pit fill (Barnes et al. 2014). Starting from the
/// sea / Y-border cells, flood inward; each cell is raised just above the
/// highest point on the least-costly path back to an outlet, guaranteeing
/// that every land cell has a downhill path to the boundary.
fn fill_pits(elev: &[f32], mask: &[u8], res: usize) -> Vec<f32> {
    let total = res * res;
    let mut filled = elev.to_vec();
    let mut visited = vec![false; total];
    let mut pq: BinaryHeap<MinF32> = BinaryHeap::new();

    for i in 0..total {
        let y = i / res;
        let is_border = y == 0 || y == res - 1;
        if mask[i] == 0 || is_border {
            pq.push(MinF32(filled[i], i as u32));
            visited[i] = true;
        }
    }

    while let Some(MinF32(hi, iu)) = pq.pop() {
        let i = iu as usize;
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx).rem_euclid(res as i32) as usize;
                let ny = y + dy;
                if ny < 0 || ny >= res as i32 {
                    continue;
                }
                let ni = ny as usize * res + nx;
                if visited[ni] {
                    continue;
                }
                visited[ni] = true;
                // Raise neighbor's filled height to hi + tiny increment so
                // it drains back toward the outlet we just came from.
                let raised = elev[ni].max(hi + 1e-3);
                filled[ni] = raised;
                pq.push(MinF32(raised, ni as u32));
            }
        }
    }
    filled
}

fn parallel_merge_radius_cells(res: usize) -> i32 {
    ((res as f32 * PARALLEL_MERGE_RADIUS_FRAC).round() as i32)
        .clamp(PARALLEL_MERGE_RADIUS_MIN, PARALLEL_MERGE_RADIUS_MAX)
}

fn step_dir(from: (u32, u32), to: (u32, u32), res: usize) -> Option<(i8, i8)> {
    let dx = fold_x_delta(to.0 as i32 - from.0 as i32, res as i32);
    let dy = to.1 as i32 - from.1 as i32;
    if dx == 0 && dy == 0 {
        None
    } else {
        Some((dx.signum() as i8, dy.signum() as i8))
    }
}

fn downstream_dir(cell: usize, downstream: &[Option<u32>], res: usize) -> Option<(i8, i8)> {
    let target = downstream[cell]? as usize;
    let from = ((cell % res) as u32, (cell / res) as u32);
    let to = ((target % res) as u32, (target / res) as u32);
    step_dir(from, to, res)
}

fn polyline_dir(points: &[(u32, u32)], pi: usize, res: usize) -> Option<(i8, i8)> {
    if pi + 1 < points.len() {
        step_dir(points[pi], points[pi + 1], res)
    } else if pi > 0 {
        step_dir(points[pi - 1], points[pi], res)
    } else {
        None
    }
}

fn directions_parallel_downstream(a: (i8, i8), b: (i8, i8)) -> bool {
    let ax = a.0 as f32;
    let ay = a.1 as f32;
    let bx = b.0 as f32;
    let by = b.1 as f32;
    let dot = ax * bx + ay * by;
    if dot <= 0.0 {
        return false;
    }
    let len = (ax * ax + ay * ay).sqrt() * (bx * bx + by * by).sqrt();
    len > 0.0 && dot / len >= PARALLEL_MERGE_MIN_DOT
}

fn parallel_merge_target(
    claims: &[Option<MergeClaim>],
    cell: usize,
    dir: Option<(i8, i8)>,
) -> Option<u32> {
    let claim = claims[cell]?;
    let dir = dir?;
    if directions_parallel_downstream(dir, (claim.dir_x, claim.dir_y)) {
        Some(claim.target)
    } else {
        None
    }
}

fn smoothstep01(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if x >= edge1 { 1.0 } else { 0.0 };
    }
    smoothstep01((x - edge0) / (edge1 - edge0))
}

fn hash_unit(mut x: u64) -> f32 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    ((x >> 40) as f32) / ((1u64 << 24) as f32)
}

fn cumulative_lengths_cells(points: &[(u32, u32)], res: usize) -> Vec<f32> {
    let mut lengths = Vec::with_capacity(points.len());
    lengths.push(0.0);
    let res_i = res as i32;
    for i in 1..points.len() {
        let dx = fold_x_delta(points[i].0 as i32 - points[i - 1].0 as i32, res_i) as f32;
        let dy = points[i].1 as f32 - points[i - 1].1 as f32;
        lengths.push(lengths[i - 1] + (dx * dx + dy * dy).sqrt());
    }
    lengths
}

/// Windowed unit tangent at vertex `i` from the cell-coordinate polyline.
/// Symmetric `±window` window when not near an endpoint; X-folded so a
/// segment that crosses the world wrap doesn't return a backward tangent.
fn windowed_tangent_cells(
    points: &[(u32, u32)],
    i: usize,
    window: usize,
    res_i: i32,
) -> Option<(f32, f32)> {
    let n = points.len();
    let lo = i.saturating_sub(window);
    let hi = (i + window).min(n - 1);
    if lo == hi {
        return None;
    }
    let dx = fold_x_delta(points[hi].0 as i32 - points[lo].0 as i32, res_i) as f32;
    let dy = points[hi].1 as f32 - points[lo].1 as f32;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 {
        return None;
    }
    Some((dx / len, dy / len))
}

/// Per-vertex arc distance (in cells) to the nearest anchor along the
/// polyline. Two passes (forward sweep, backward sweep) compute the
/// minimum distance from each vertex to any earlier or later anchor;
/// anchors themselves get distance 0. Used to taper the displacement
/// amplitude smoothly across confluences.
fn anchor_distance_along_polyline(cumulative: &[f32], anchors: &[bool]) -> Vec<f32> {
    let n = anchors.len();
    let mut out = vec![f32::INFINITY; n];
    let mut last = f32::NEG_INFINITY;
    for i in 0..n {
        if anchors[i] {
            last = cumulative[i];
            out[i] = 0.0;
        } else if last.is_finite() {
            out[i] = cumulative[i] - last;
        }
    }
    let mut next = f32::INFINITY;
    for i in (0..n).rev() {
        if anchors[i] {
            next = cumulative[i];
        } else if next.is_finite() {
            let d = next - cumulative[i];
            if d < out[i] {
                out[i] = d;
            }
        }
    }
    out
}

fn append_wrapped_line(
    points: &mut Vec<(u32, u32)>,
    flow: &mut Vec<f32>,
    from: (u32, u32),
    to: (u32, u32),
    flow_from: f32,
    flow_to: f32,
    res: usize,
) {
    let dx = fold_x_delta(to.0 as i32 - from.0 as i32, res as i32);
    let dy = to.1 as i32 - from.1 as i32;
    let steps = dx.abs().max(dy.abs()) as usize;
    if steps == 0 {
        return;
    }
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let x = (from.0 as i32 + (dx as f32 * t).round() as i32).rem_euclid(res as i32) as u32;
        let y = (from.1 as i32 + (dy as f32 * t).round() as i32).clamp(0, res as i32 - 1) as u32;
        let p = (x, y);
        let f = flow_from + (flow_to - flow_from) * t;
        if points.last().copied() == Some(p) {
            if let Some(last_flow) = flow.last_mut() {
                *last_flow = f;
            }
        } else {
            points.push(p);
            flow.push(f);
        }
    }
}

fn naturalize_river_meanders(map: &GlobalMap, rivers: &mut [Polyline]) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mut occurrences = vec![0u8; total];
    for poly in rivers.iter() {
        for &(x, y) in &poly.points {
            let idx = y as usize * res + x as usize;
            occurrences[idx] = occurrences[idx].saturating_add(1);
        }
    }

    // Junction cells (shared by ≥2 polylines) and per-polyline endpoints
    // are anchored: they stay fixed during migration so a tributary's
    // confluence with its main stem doesn't drift apart by a few cells.
    let mut anchors = occurrences
        .iter()
        .map(|&count| count > 1)
        .collect::<Vec<bool>>();
    for poly in rivers.iter() {
        if let Some(&(x, y)) = poly.points.first() {
            anchors[y as usize * res + x as usize] = true;
        }
        if let Some(&(x, y)) = poly.points.last() {
            anchors[y as usize * res + x as usize] = true;
        }
    }

    let max_flow = polylines_max_flow(rivers);
    let log_max_flow = max_flow.log2().max(1.0);

    let res_i = res as i32;
    let mpc = map.config.meters_per_cell();

    for (ri, poly) in rivers.iter_mut().enumerate() {
        let n = poly.points.len();
        if n < 5 || n != poly.flow.len() {
            continue;
        }

        let cumulative = cumulative_lengths_cells(&poly.points, res);
        let total_len = *cumulative.last().unwrap_or(&0.0);
        if total_len < MEANDER_MIN_LENGTH_CELLS {
            continue;
        }

        // Per-vertex anchor flags. Junction cells (occurrences > 1 in the
        // global map) plus this polyline's own endpoints. Anchors stay
        // exactly where they are; the displacement amplitude tapers as we
        // approach them.
        let mut anchor_flags = vec![false; n];
        for i in 0..n {
            let (px, py) = poly.points[i];
            if anchors[py as usize * res + px as usize] {
                anchor_flags[i] = true;
            }
        }
        anchor_flags[0] = true;
        anchor_flags[n - 1] = true;

        // Arc distance from each vertex to its nearest anchor — drives the
        // taper that prevents kinks at confluences.
        let anchor_dist = anchor_distance_along_polyline(&cumulative, &anchor_flags);
        let taper_len = MEANDER_ANCHOR_TAPER_CELLS
            .min(total_len * 0.5)
            .max(1.0);

        // Phase-randomize per polyline so adjacent rivers don't bend in
        // lockstep — the seed mix is the same recipe the previous code used.
        let phase = hash_unit(
            map.config.seed
                ^ ((ri as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                ^ 0xA11C_E5ED_5EA_u64,
        ) * std::f32::consts::TAU;

        let mut target_x = vec![0.0f32; n];
        let mut target_y = vec![0.0f32; n];
        for i in 0..n {
            target_x[i] = poly.points[i].0 as f32;
            target_y[i] = poly.points[i].1 as f32;
        }

        for i in 1..n - 1 {
            if anchor_flags[i] {
                continue;
            }
            // Anchor-proximity taper: 0 at any anchor, smoothly to 1 by
            // `MEANDER_ANCHOR_TAPER_CELLS`. Both ends of the polyline and
            // every junction in the middle get the same gentle ramp.
            let taper = smoothstep01(anchor_dist[i] / taper_len);
            if taper <= 0.01 {
                continue;
            }

            // Slope gate over a tangent window: lowland reaches only.
            let lo = i.saturating_sub(MEANDER_TANGENT_WINDOW);
            let hi = (i + MEANDER_TANGENT_WINDOW).min(n - 1);
            let elev_lo =
                map.elevation_m[poly.points[lo].1 as usize * res + poly.points[lo].0 as usize];
            let elev_hi =
                map.elevation_m[poly.points[hi].1 as usize * res + poly.points[hi].0 as usize];
            let arc = cumulative[hi] - cumulative[lo];
            let slope = (elev_lo - elev_hi).abs() / (arc * mpc).max(1e-3);
            let slope_gate = 1.0 - smoothstep(MEANDER_SLOPE_LOW, MEANDER_SLOPE_HIGH, slope);
            if slope_gate <= 0.01 {
                continue;
            }

            let Some((tx, ty)) =
                windowed_tangent_cells(&poly.points, i, MEANDER_TANGENT_WINDOW, res_i)
            else {
                continue;
            };
            // Image-coord normal: CCW-rotated tangent (Y-down convention).
            let nx = -ty;
            let ny = tx;

            let flow_norm = (poly.flow[i].max(1.0).log2() / log_max_flow).clamp(0.0, 1.0);
            let amp_cells = MEANDER_BASE_AMPLITUDE_CELLS
                + MEANDER_FLOW_AMPLITUDE_CELLS * flow_norm;

            // Two-octave sinusoidal noise on arc length: long-wavelength
            // primary carries the visible bend, optional shorter-wavelength
            // harmonic breaks the sine symmetry. Avoid a third octave —
            // any wavelength much shorter than ~1 km reads as small
            // wiggles rather than a meander at world scale.
            let s = cumulative[i];
            let wave_a = (s / MEANDER_PRIMARY_WAVELENGTH_CELLS * std::f32::consts::TAU + phase)
                .sin();
            let wave_b = (s / MEANDER_SECONDARY_WAVELENGTH_CELLS * std::f32::consts::TAU
                + phase * 1.73)
                .sin()
                * MEANDER_SECONDARY_WEIGHT;
            let wave = wave_a + wave_b; // bound: |wave| ≤ 1 + secondary_weight

            let offset = wave * amp_cells * taper * slope_gate;
            if offset.abs() < 0.5 {
                continue;
            }

            // Try the full offset first, then half — back off rather than
            // skip if the candidate cell is sea, so we still get *some*
            // displacement near coastal margins.
            for scale in [1.0f32, 0.5] {
                let cand_x = target_x[i] + nx * offset * scale;
                let cand_y = target_y[i] + ny * offset * scale;
                let cell_x_i = cand_x.round() as i32;
                let cell_y_i = cand_y.round() as i32;
                if cell_y_i < 0 || cell_y_i >= res_i {
                    continue;
                }
                let cell_x = cell_x_i.rem_euclid(res_i) as usize;
                let cell_y = cell_y_i as usize;
                if map.land_mask[cell_y * res + cell_x] != 1 {
                    continue;
                }
                target_x[i] = cand_x;
                target_y[i] = cand_y;
                break;
            }
        }

        let mut new_points: Vec<(u32, u32)> = Vec::with_capacity(n);
        let mut new_flow: Vec<f32> = Vec::with_capacity(n);
        let p0 = (
            (target_x[0].round() as i32).rem_euclid(res_i) as u32,
            target_y[0].round().clamp(0.0, (res_i - 1) as f32) as u32,
        );
        new_points.push(p0);
        new_flow.push(poly.flow[0]);
        for i in 1..n {
            let pi = (
                (target_x[i].round() as i32).rem_euclid(res_i) as u32,
                target_y[i].round().clamp(0.0, (res_i - 1) as f32) as u32,
            );
            let last = *new_points.last().unwrap();
            let last_flow = *new_flow.last().unwrap();
            append_wrapped_line(
                &mut new_points,
                &mut new_flow,
                last,
                pi,
                last_flow,
                poly.flow[i],
                res,
            );
        }

        if new_points.len() >= 2 && new_points.len() == new_flow.len() {
            poly.points = new_points;
            poly.flow = new_flow;
        }
    }
}

fn add_parallel_merge_claims(
    claims: &mut [Option<MergeClaim>],
    points: &[(u32, u32)],
    downstream: &[Option<u32>],
    res: usize,
    radius: i32,
) {
    let radius_sq = radius * radius;
    for (pi, &(x, y)) in points.iter().enumerate() {
        let cell = y as usize * res + x as usize;
        let Some((dir_x, dir_y)) =
            downstream_dir(cell, downstream, res).or_else(|| polyline_dir(points, pi, res))
        else {
            continue;
        };

        for dy in -radius..=radius {
            let ny = y as i32 + dy;
            if ny < 0 || ny >= res as i32 {
                continue;
            }
            for dx in -radius..=radius {
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > radius_sq {
                    continue;
                }
                let nx = (x as i32 + dx).rem_euclid(res as i32) as usize;
                let ni = ny as usize * res + nx;
                let claim = MergeClaim {
                    target: cell as u32,
                    dir_x,
                    dir_y,
                    dist_sq: dist_sq as u16,
                };
                match claims[ni] {
                    Some(existing) if existing.dist_sq <= claim.dist_sq => {}
                    _ => claims[ni] = Some(claim),
                }
            }
        }
    }
}

pub struct RiverMap {
    /// For each cell, the index of its downstream neighbor, or `None` if it
    /// has no descent (sink / sea cell). Length = res².
    pub downstream: Vec<Option<u32>>,

    /// Flow accumulation in arbitrary cell-rain units. Higher = more water
    /// passes through. Length = res².
    pub flow: Vec<f32>,

    /// Extracted river polylines — each is a sequence of cell coordinates
    /// from the source (highest upstream point) to the mouth (sea or sink).
    pub rivers: Vec<Polyline>,
}

impl RiverMap {
    /// Maximum per-vertex flow across every extracted polyline, clamped to
    /// ≥ 1 so log-normalization downstream never divides by zero. Recomputed
    /// on demand (rivers vector mutates rarely, callers are offline bake /
    /// preview).
    pub fn max_flow(&self) -> f32 {
        polylines_max_flow(&self.rivers)
    }
}

/// Peak per-vertex flow on a single polyline (0 if empty). Used as a
/// per-polyline priority key in `merge_overlapping_polylines`.
fn polyline_peak_flow(poly: &Polyline) -> f32 {
    poly.flow.iter().copied().fold(0.0f32, f32::max)
}

/// Peak per-vertex flow across every polyline, clamped to ≥ 1 so callers
/// can divide by `log2(max_flow)` without guarding for log of 1 or below.
fn polylines_max_flow(polys: &[Polyline]) -> f32 {
    polys.iter().map(polyline_peak_flow).fold(1.0f32, f32::max)
}

#[derive(Debug, Clone)]
pub struct Polyline {
    pub points: Vec<(u32, u32)>,
    /// Per-vertex flow accumulation (raw units, same scale as `RiverMap.flow`).
    /// Same length as `points`. Drives downstream width growth.
    pub flow: Vec<f32>,
}

/// Strength of the post-erosion water-field bias on downstream selection.
/// Each downhill candidate's slope is multiplied by `(water_norm + base)`
/// with `water_norm ∈ [0, 1]` (channels ≈ 1, off-channel ≈ 0); a cell
/// fully claimed by a sim-carved channel beats a near-flat alternative
/// by `(1 + WATER_FIELD_BIAS_BASE) / WATER_FIELD_BIAS_BASE`. Tuning
/// rationale: large enough that flat reaches reliably snap to the
/// sim-carved meander, small enough that genuine steepest-descent on
/// mountain flanks (where water_norm is near zero outside channels) is
/// still chosen over a high-water diagonal that climbs.
const WATER_FIELD_BIAS_BASE: f32 = 0.05;

/// Compute downstream pointers + flow accumulation for every land cell.
/// Fills pits first so every land cell has a downhill path to the ocean.
pub fn compute_flow(map: &GlobalMap) -> RiverMap {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mask = &map.land_mask;
    // Pit-filled elevation for flow computation. Original elevation is
    // preserved in `map.elevation_m`; flow pretends pits are already full.
    let filled = fill_pits(&map.elevation_m, mask, res);
    let elev = &filled;
    // Sim-carved channel signal from Phase 3, normalized to ~[0, 1].
    // Empty when erosion was skipped — the lookup below treats that as
    // "no preference" so the selection collapses back to pure steepest
    // descent.
    let water_field = if map.water_after_erosion.len() == total {
        Some(map.water_after_erosion.as_slice())
    } else {
        None
    };

    // 8-connected offsets with their Euclidean distance (for slope calc).
    const OFFSETS: [(i32, i32, f32); 8] = [
        (-1, -1, std::f32::consts::SQRT_2),
        (0, -1, 1.0),
        (1, -1, std::f32::consts::SQRT_2),
        (-1, 0, 1.0),
        (1, 0, 1.0),
        (-1, 1, std::f32::consts::SQRT_2),
        (0, 1, 1.0),
        (1, 1, std::f32::consts::SQRT_2),
    ];

    // --- Downstream pointer per land cell, on the pit-filled surface.
    let mut downstream: Vec<Option<u32>> = vec![None; total];
    for i in 0..total {
        if mask[i] == 0 {
            continue;
        }
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        let h = elev[i];
        let mut best_score = 0.0f32;
        let mut best: Option<u32> = None;
        for &(dx, dy, dist) in &OFFSETS {
            let nx = (x + dx).rem_euclid(res as i32) as usize;
            let ny = y + dy;
            if ny < 0 || ny >= res as i32 {
                continue;
            }
            let ni = ny as usize * res + nx;
            let dh = h - elev[ni];
            if dh > 0.0 {
                let slope = dh / dist;
                // Slope-only score when no sim signal is available; on
                // flats with uniform slope this still falls back to the
                // first downhill neighbor (matching pre-water behavior).
                let bias = match water_field {
                    Some(w) => w[ni] + WATER_FIELD_BIAS_BASE,
                    None => 1.0,
                };
                let score = slope * bias;
                if score > best_score {
                    best_score = score;
                    best = Some(ni as u32);
                }
            }
        }
        downstream[i] = best;
    }

    // --- Flow accumulation.
    // Sort cells by elevation (descending). Each land cell contributes 1
    // unit of rain to itself, and then passes its total downstream. By
    // processing high-to-low we guarantee each cell is finalized before
    // its downstream is visited.
    let mut order: Vec<u32> = (0..total as u32).collect();
    order.sort_by(|&a, &b| elev[b as usize].total_cmp(&elev[a as usize]));

    let mut flow = vec![0.0f32; total];
    for &iu in &order {
        let i = iu as usize;
        if mask[i] == 0 {
            continue;
        }
        flow[i] += 1.0; // rain on this cell
        if let Some(d) = downstream[i] {
            flow[d as usize] += flow[i];
        }
    }

    RiverMap {
        downstream,
        flow,
        rivers: Vec::new(),
    }
}

/// Extract river polylines. Sources are **local elevation peaks** above
/// `min_peak_elevation` — i.e. cells whose elevation is strictly greater
/// than all 8 neighbors. Peaks are processed from highest to lowest; each
/// traces downstream until it either reaches sea, hits a sink, or merges
/// into a previously-traced river (giving natural tree structure).
///
/// Also carves water upstream from every "sharp sea inlet" — sea cells that
/// jut far into land become river mouths by extending the existing flow
/// path to reach them. (This catches the visual association users make
/// between pointy coastal inlets and river mouths.)
///
/// `min_length` drops very short chains so the preview stays readable.
pub fn extract_rivers(
    map: &GlobalMap,
    rivers: &mut RiverMap,
    min_peak_elevation: f32,
    min_length: usize,
) {
    let res = map.config.global_res as usize;
    let total = res * res;
    let mask = &map.land_mask;
    let elev = &map.elevation_m;

    rivers.rivers.clear();

    // --- 1. Gather candidate sources: local elevation maxima above
    // threshold. Candidates are filtered by a minimum-spacing pass so the
    // resulting river network has a handful of main stems rather than one
    // "river" per every rocky bump.
    // Exclude ~2× the wall band from peak candidacy. The wall's uniform
    // southward slope generates parallel peaks that trace as straight-line
    // rivers; the 2× cushion catches peaks just past the wall where the
    // wall-to-natural-terrain transition still produces a uniform gradient.
    let wall_margin = map
        .config
        .scaled_cells_usize(map.config.y_border_wall_cells)
        * 2;
    let mut candidates: Vec<(u32, f32)> = Vec::new();
    for i in 0..total {
        if mask[i] == 0 || elev[i] < min_peak_elevation {
            continue;
        }
        let iy = i / res;
        if iy < wall_margin || iy + wall_margin >= res {
            continue;
        }
        let x = (i % res) as i32;
        let y = (i / res) as i32;
        let h = elev[i];
        let mut is_peak = true;
        for dy in -1..=1i32 {
            for dx in -1..=1i32 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let nx = (x + dx).rem_euclid(res as i32) as usize;
                let ny = y + dy;
                if ny < 0 || ny >= res as i32 {
                    continue;
                }
                let ni = ny as usize * res + nx;
                if elev[ni] >= h {
                    is_peak = false;
                    break;
                }
            }
            if !is_peak {
                break;
            }
        }
        if is_peak {
            candidates.push((i as u32, h));
        }
    }

    // Sort by elevation descending — tallest peaks claim main stems first,
    // shorter peaks become tributaries at junctions.
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

    // Spatial filter: keep a peak only if it's at least `min_peak_spacing`
    // cells from every already-kept peak (X-wrap aware). Prevents dozens of
    // near-identical tributaries originating on the same massif.
    let min_peak_spacing_sq = ((res as f32 * 0.015).max(20.0).powi(2)) as i64;
    let res_i = res as i64;
    let mut peaks: Vec<(u32, f32)> = Vec::with_capacity(candidates.len().min(100));
    for (idx, h) in candidates {
        let px = (idx as usize % res) as i64;
        let py = (idx as usize / res) as i64;
        let ok = peaks.iter().all(|&(qidx, _)| {
            let qx = (qidx as usize % res) as i64;
            let qy = (qidx as usize / res) as i64;
            let dx = (px - qx).abs();
            let dx = dx.min(res_i - dx); // X-wrap
            let dy = py - qy;
            dx * dx + dy * dy >= min_peak_spacing_sq
        });
        if ok {
            peaks.push((idx, h));
        }
    }

    // --- 2. Trace each peak downstream until sea / sink / merge.
    let mut visited = vec![false; total];
    let merge_radius = parallel_merge_radius_cells(res);
    let mut merge_claims: Vec<Option<MergeClaim>> = vec![None; total];
    for (peak_idx, _) in peaks {
        let start = peak_idx as usize;
        if visited[start] {
            continue;
        }
        let mut points: Vec<(u32, u32)> = Vec::new();
        let mut flow_vals: Vec<f32> = Vec::new();
        let mut cur: Option<u32> = Some(start as u32);
        while let Some(ci32) = cur {
            let ci = ci32 as usize;
            let x = (ci % res) as u32;
            let y = (ci / res) as u32;
            points.push((x, y));
            flow_vals.push(rivers.flow[ci]);
            if visited[ci] {
                // Merge into an earlier-traced polyline — include this
                // junction point so the tributary visibly connects.
                break;
            }
            if let Some(target) = parallel_merge_target(
                &merge_claims,
                ci,
                downstream_dir(ci, &rivers.downstream, res),
            ) {
                visited[ci] = true;
                let ti = target as usize;
                let target_point = ((ti % res) as u32, (ti / res) as u32);
                if points.last().copied() != Some(target_point) {
                    points.push(target_point);
                    flow_vals.push(rivers.flow[ti]);
                }
                break;
            }
            visited[ci] = true;
            cur = rivers.downstream[ci];
        }
        if points.len() >= min_length {
            add_parallel_merge_claims(
                &mut merge_claims,
                &points,
                &rivers.downstream,
                res,
                merge_radius,
            );
            rivers.rivers.push(Polyline {
                points,
                flow: flow_vals,
            });
        }
    }

    synthesize_distributary_deltas(map, &mut rivers.rivers);
    naturalize_river_meanders(map, &mut rivers.rivers);
    remove_polyline_self_overlaps(&mut rivers.rivers);
    merge_overlapping_polylines(map, &mut rivers.rivers);
}

/// Minimum arc-length (in vertices) between two revisits of the same cell
/// for it to count as a self-overlap loop. Short revisits are normal —
/// Bresenham rasterization between meander targets can briefly back-track.
const SELF_OVERLAP_MIN_LOOP_VERTICES: usize = 8;

/// Excise hairpin loops where the polyline revisits a cell after a non-
/// trivial detour. Caused by `naturalize_river_meanders` pushing two arc-
/// length-distant sections onto the same cell — the loop renders fine
/// geometrically (both arms fit inside the river's width) but the baked
/// flow field reads as two parallel arms of opposite direction, smearing
/// the water shader's normalmap scroll.
///
/// Pass walks each polyline once with a `cell -> first_visit_index` hash.
/// On revisit at index `i` of a cell first seen at `j`, if `i − j ≥
/// SELF_OVERLAP_MIN_LOOP_VERTICES` we drop indices `j+1..=i` so the
/// polyline crosses the area exactly once. The first-visit map is built
/// fresh after each excision so the scan is robust against cascading loops.
fn remove_polyline_self_overlaps(rivers: &mut [Polyline]) {
    for poly in rivers.iter_mut() {
        loop {
            let n = poly.points.len();
            if n < 2 * SELF_OVERLAP_MIN_LOOP_VERTICES {
                break;
            }
            let mut first_visit: HashMap<(u32, u32), usize> = HashMap::with_capacity(n);
            let mut drain_range: Option<(usize, usize)> = None;
            for i in 0..n {
                let cell = poly.points[i];
                if let Some(&j) = first_visit.get(&cell) {
                    if i - j >= SELF_OVERLAP_MIN_LOOP_VERTICES {
                        drain_range = Some((j + 1, i + 1));
                        break;
                    }
                } else {
                    first_visit.insert(cell, i);
                }
            }
            let Some((lo, hi)) = drain_range else {
                break;
            };
            poly.points.drain(lo..hi);
            if poly.flow.len() >= hi {
                poly.flow.drain(lo..hi);
            }
        }
    }
}

/// Cells on either side of a polyline that the carve still reaches —
/// max half-width (~5 m) plus the lateral fade taper (~10 m) ≈ 15 m, so
/// at the typical 8 m/cell spacing two polylines whose vertex paths run
/// within ~2 cells of each other read as a single visually-overlapping
/// channel even when no vertex is exactly shared.
const MERGE_PROXIMITY_RADIUS_CELLS: i32 = 2;

/// Number of vertices at each polyline endpoint exempt from the
/// proximity check. Distributary fans share an apex cell across 2–3
/// branches that diverge by ~40°; the first few cells of those branches
/// are unavoidably close to each other. With a ±40° fan, by the eighth
/// cell from apex adjacent branches are ~5.4 cells apart, comfortably
/// outside `MERGE_PROXIMITY_RADIUS`. Keep this >= the radius times the
/// distance ratio for the tightest fan you expect to support.
const MERGE_ENDPOINT_EXEMPT_VERTICES: usize = 8;

/// Resolve crossings and visually-overlapping parallel reaches caused
/// by meander displacement: when this polyline runs within
/// `MERGE_PROXIMITY_RADIUS_CELLS` of any cell already claimed by a
/// higher-priority polyline, truncate it at that point so the two
/// merge into a single channel rather than meeting and splitting
/// again. Priority is "max flow descending" so trunks claim cells
/// before their tributaries.
///
/// The first / last `MERGE_ENDPOINT_EXEMPT_VERTICES` vertices on each
/// polyline are exempt from the check — junction cells are *meant* to
/// be shared between trunks and tributaries, and distributary fans by
/// construction have all branches start very close to a shared apex
/// before fanning out.
fn merge_overlapping_polylines(map: &GlobalMap, rivers: &mut Vec<Polyline>) {
    if rivers.is_empty() {
        return;
    }
    let res = map.config.global_res as usize;
    let res_i = res as i32;
    let total = res * res;

    // Priority = peak per-vertex flow on each polyline. Cache the peaks
    // up front; the comparator is called O(R log R) times so a per-call
    // fold over each polyline's flows would be O(R V log R).
    let peak_flow: Vec<f32> = rivers.iter().map(polyline_peak_flow).collect();
    let mut order: Vec<usize> = (0..rivers.len()).collect();
    order.sort_by(|&a, &b| peak_flow[b].total_cmp(&peak_flow[a]));

    // u32::MAX = unclaimed; otherwise the cell's owning polyline index.
    let mut claimer: Vec<u32> = vec![u32::MAX; total];

    for &ri in &order {
        let n = rivers[ri].points.len();
        if n < 2 {
            continue;
        }

        // Skip the endpoint-exempt vertices on each side.
        let exempt = MERGE_ENDPOINT_EXEMPT_VERTICES.min(n.saturating_sub(1) / 2);
        let lo = exempt;
        let hi = n.saturating_sub(exempt);

        // Walk vertices in order; the first one whose proximity disk
        // contains a cell claimed by another polyline marks where this
        // polyline merges into that channel. Truncate inclusive of the
        // current vertex so the river's last cell sits squarely on
        // the path it joined.
        let mut truncate_at: Option<usize> = None;
        'outer: for i in lo..hi {
            let (x, y) = rivers[ri].points[i];
            for ddy in -MERGE_PROXIMITY_RADIUS_CELLS..=MERGE_PROXIMITY_RADIUS_CELLS {
                let cy = y as i32 + ddy;
                if cy < 0 || cy >= res_i {
                    continue;
                }
                for ddx in -MERGE_PROXIMITY_RADIUS_CELLS..=MERGE_PROXIMITY_RADIUS_CELLS {
                    let cx = (x as i32 + ddx).rem_euclid(res_i);
                    let cell = cy as usize * res + cx as usize;
                    let owner = claimer[cell];
                    if owner != u32::MAX && owner as usize != ri {
                        truncate_at = Some(i);
                        break 'outer;
                    }
                }
            }
        }
        if let Some(cut) = truncate_at {
            rivers[ri].points.truncate(cut + 1);
            rivers[ri].flow.truncate(cut + 1);
        }

        // First claim wins so the priority ordering above governs ownership.
        for &(x, y) in &rivers[ri].points {
            let cell = y as usize * res + x as usize;
            if claimer[cell] == u32::MAX {
                claimer[cell] = ri as u32;
            }
        }
    }

    // Drop polylines that collapsed to a single point during truncation
    // (e.g. one whose interior overlapped another from vertex 1).
    rivers.retain(|p| p.points.len() >= 2);
}

/// Branch each qualifying main river into a distributary fan at its
/// mouth. A river qualifies if it (a) reaches the sea, (b) carries
/// flow above a fraction of the global max, and (c) the apex-to-mouth
/// reach is near-flat. The original trunk is truncated at the apex
/// and 2–`DISTRIBUTARY_BRANCH_COUNT` straight branches are appended,
/// each ending at a sea cell within the forward fan. Subsequent
/// `naturalize_river_meanders` smooths the new branches into curves.
fn synthesize_distributary_deltas(map: &GlobalMap, rivers: &mut Vec<Polyline>) {
    let res = map.config.global_res as usize;
    let res_i = res as i32;
    let mpc = map.config.meters_per_cell();
    let max_flow = polylines_max_flow(rivers);
    let flow_threshold = max_flow * DISTRIBUTARY_FLOW_FRAC_OF_MAX;
    let log_max = max_flow.max(1.0).log2().max(1e-3);
    let width_span = DISTRIBUTARY_WIDTH_MAX_M - DISTRIBUTARY_WIDTH_MIN_M;
    let cos_half_arc = DISTRIBUTARY_HALF_ARC_RAD.cos();
    let r_max = DISTRIBUTARY_SEARCH_RADIUS_CELLS;
    let r_min = DISTRIBUTARY_SEARCH_MIN_RADIUS_CELLS;
    let r_max_i = r_max.ceil() as i32;
    let slice_width = 2.0 * DISTRIBUTARY_HALF_ARC_RAD / DISTRIBUTARY_BRANCH_COUNT as f32;

    let mut new_polys: Vec<Polyline> = Vec::new();
    let mut truncate_to: Vec<Option<usize>> = vec![None; rivers.len()];

    for (ri, poly) in rivers.iter().enumerate() {
        let n = poly.points.len();
        if n < 6 {
            continue;
        }
        // Mouth must sit on a sea cell.
        let mouth = poly.points[n - 1];
        if map.land_mask[mouth.1 as usize * res + mouth.0 as usize] != 0 {
            continue;
        }

        let cumulative = cumulative_lengths_cells(&poly.points, res);
        let total_len = *cumulative.last().unwrap_or(&0.0);
        let target_len = total_len - DISTRIBUTARY_APEX_OFFSET_CELLS;
        if target_len <= 5.0 {
            continue;
        }

        // Walk back to the first vertex whose cumulative arc-length is
        // at or below `target_len` — that's the apex.
        let mut apex_idx = 0usize;
        for i in (0..n).rev() {
            if cumulative[i] <= target_len {
                apex_idx = i;
                break;
            }
        }
        if apex_idx == 0 || apex_idx >= n - 1 {
            continue;
        }
        let apex = poly.points[apex_idx];
        if map.land_mask[apex.1 as usize * res + apex.0 as usize] != 1 {
            continue;
        }

        // Flow gate: only sizable rivers form distributary deltas.
        if poly.flow[apex_idx] < flow_threshold {
            continue;
        }

        // Forward direction = apex → mouth (tangent at apex toward sea).
        let dx_fwd = fold_x_delta(mouth.0 as i32 - apex.0 as i32, res_i) as f32;
        let dy_fwd = mouth.1 as f32 - apex.1 as f32;
        let fwd_len = (dx_fwd * dx_fwd + dy_fwd * dy_fwd).sqrt();
        if fwd_len < 1.0 {
            continue;
        }
        let fx = dx_fwd / fwd_len;
        let fy = dy_fwd / fwd_len;

        // Slope gate: deltas only on near-flat coastal plains.
        let elev_apex = map.elevation_m[apex.1 as usize * res + apex.0 as usize];
        let slope = elev_apex / (fwd_len * mpc).max(1.0);
        if slope > DISTRIBUTARY_SLOPE_MAX {
            continue;
        }

        // Sweep the forward sector for sea cells; for each angular slice
        // keep the closest valid candidate. Annulus filter prevents
        // collapsing branches and absurdly long ones; arc filter keeps
        // them in front of the apex rather than behind.
        let mut slice_best: [Option<((u32, u32), f32)>; DISTRIBUTARY_BRANCH_COUNT] =
            [None; DISTRIBUTARY_BRANCH_COUNT];
        for ddy in -r_max_i..=r_max_i {
            let cy_i = apex.1 as i32 + ddy;
            if cy_i < 0 || cy_i >= res_i {
                continue;
            }
            for ddx in -r_max_i..=r_max_i {
                let cx_i = (apex.0 as i32 + ddx).rem_euclid(res_i);
                if map.land_mask[cy_i as usize * res + cx_i as usize] != 0 {
                    continue;
                }
                let dx = ddx as f32;
                let dy = ddy as f32;
                let d = (dx * dx + dy * dy).sqrt();
                if d < r_min || d > r_max {
                    continue;
                }
                let cos_angle = (dx * fx + dy * fy) / d;
                if cos_angle < cos_half_arc {
                    continue;
                }
                let sin_angle = (fx * dy - fy * dx) / d;
                let angle = sin_angle.atan2(cos_angle);
                let slice_idx = (((angle + DISTRIBUTARY_HALF_ARC_RAD) / slice_width).floor()
                    as i32)
                    .clamp(0, DISTRIBUTARY_BRANCH_COUNT as i32 - 1)
                    as usize;
                let take = slice_best[slice_idx].map_or(true, |(_, prev_d)| d < prev_d);
                if take {
                    slice_best[slice_idx] = Some(((cx_i as u32, cy_i as u32), d));
                }
            }
        }

        let valid_count = slice_best.iter().filter(|s| s.is_some()).count();
        if valid_count < 2 {
            // Coastline geometry doesn't support a fan here; leave the
            // river as a single mouth.
            continue;
        }

        truncate_to[ri] = Some(apex_idx);

        // Width-conserving split: every branch carries parent flow at the
        // apex then tapers to child flow (= parent width / N inverted back
        // through the log2 mapping) so the bake reads a single wide cell
        // at the bifurcation and N narrower channels downstream. Without
        // this the per-vertex log2 mapping renders all branches at near-
        // trunk width, overlapping into a single wide stream.
        let parent_flow_at_apex = poly.flow[apex_idx];
        let parent_width_m = {
            let t = (parent_flow_at_apex.max(1.0).log2() / log_max).clamp(0.0, 1.0);
            DISTRIBUTARY_WIDTH_MIN_M + width_span * t
        };
        let child_width_m =
            (parent_width_m / valid_count as f32).max(DISTRIBUTARY_WIDTH_MIN_M);
        let child_flow = {
            let t = ((child_width_m - DISTRIBUTARY_WIDTH_MIN_M) / width_span).clamp(0.0, 1.0);
            (2.0f32).powf(t * log_max).max(1.0)
        };

        // `spread_step_cells` minimum of 1.5 cells: diagonal river orientations
        // put perpendicular axes at ~0.7, so smaller spacing rounds adjacent
        // branch starts onto the same cell and they visually stack.
        let spread_step_cells = (child_width_m * 1.5 / mpc.max(0.1)).max(1.5);
        let forward_taper_cells = 2.0f32;
        let nx = -fy;
        let ny = fx;
        let center_idx = (DISTRIBUTARY_BRANCH_COUNT as f32 - 1.0) * 0.5;
        let apex_x = apex.0 as f32;
        let apex_y = apex.1 as f32;

        for (slice_idx, slot) in slice_best.iter().enumerate() {
            let Some((sea_pt, _)) = *slot else { continue };
            let lat = (slice_idx as f32 - center_idx) * spread_step_cells;
            let mid_xf = apex_x + forward_taper_cells * fx + lat * nx;
            let mid_yf = apex_y + forward_taper_cells * fy + lat * ny;
            let mid_x = (mid_xf.round() as i32).rem_euclid(res_i) as u32;
            let mid_y = (mid_yf.round() as i32).clamp(0, res_i - 1) as u32;
            let mid_pt = (mid_x, mid_y);

            let mut pts: Vec<(u32, u32)> = vec![apex];
            let mut flows: Vec<f32> = vec![parent_flow_at_apex];
            if mid_pt != apex {
                append_wrapped_line(
                    &mut pts,
                    &mut flows,
                    apex,
                    mid_pt,
                    parent_flow_at_apex,
                    child_flow,
                    res,
                );
            }
            let last = *pts.last().unwrap();
            if last != sea_pt {
                append_wrapped_line(
                    &mut pts,
                    &mut flows,
                    last,
                    sea_pt,
                    child_flow,
                    child_flow,
                    res,
                );
            }
            if pts.len() >= 2 {
                new_polys.push(Polyline { points: pts, flow: flows });
            }
        }
    }

    // Apply truncations after the analysis pass so the loop above never
    // sees a half-edited polyline.
    for (ri, cut) in truncate_to.iter().enumerate() {
        if let Some(end_inclusive) = *cut {
            rivers[ri].points.truncate(end_inclusive + 1);
            rivers[ri].flow.truncate(end_inclusive + 1);
        }
    }
    rivers.extend(new_polys);
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;
    use crate::worldgen::global_map::GlobalMap;

    fn test_config(res: u32) -> WorldGenConfig {
        let mut cfg = WorldGenConfig::default();
        cfg.seed = 0xBEEF;
        cfg.world_size_m = 4096;
        cfg.global_res = res;
        cfg.reference_res = res;
        cfg.sea_ratio = 0.3;
        cfg.continent_frequency = 1.0 / 64.0;
        cfg.min_island_cells = 0;
        cfg.min_strait_width_cells = 0;
        cfg.continent_seed_count = 3;
        cfg.continent_seed_min_distance_cells = 20;
        cfg.target_continent_count = 1;
        cfg.continent_gap_cells = 0;
        cfg.small_island_count = 0;
        cfg.y_border_wall_cells = 0;
        cfg.y_border_wall_height_m = 0.0;
        cfg.river_gap_max_m = 0.0;
        // Default wavelength is sized for a 4096-cell production map; in
        // the small test resolutions a 700-cell wavelength is wider than
        // the world and degenerates to one monotonic gradient. Pick a
        // wavelength that produces several peaks at this scale.
        cfg.initial_relief_wavelength_cells = (res as f32 / 4.0).max(8.0);
        cfg
    }

    fn cell_idx(res: u32, x: u32, y: u32) -> usize {
        y as usize * res as usize + x as usize
    }

    fn blank_map(res: u32) -> GlobalMap {
        let cfg = test_config(res);
        let total = res as usize * res as usize;
        GlobalMap {
            config: cfg,
            continent_potential: vec![1.0; total],
            land_mask: vec![1; total],
            sea_level_potential: 0.0,
            elevation_m: vec![0.0; total],
            water_after_erosion: Vec::new(),
        }
    }

    fn blank_river_map(res: u32) -> RiverMap {
        let total = res as usize * res as usize;
        RiverMap {
            downstream: vec![None; total],
            flow: vec![1.0; total],
            rivers: Vec::new(),
        }
    }

    fn seed_manual_river(
        map: &mut GlobalMap,
        rivers: &mut RiverMap,
        points: &[(u32, u32)],
        source_elev: f32,
    ) {
        let res = map.config.global_res;
        for (pi, &(x, y)) in points.iter().enumerate() {
            let idx = cell_idx(res, x, y);
            map.elevation_m[idx] = source_elev - pi as f32;
            rivers.flow[idx] = (pi + 1) as f32;
            if let Some(&(nx, ny)) = points.get(pi + 1) {
                rivers.downstream[idx] = Some(cell_idx(res, nx, ny) as u32);
            }
        }
    }

    #[test]
    fn flow_accumulates_downhill() {
        // On a land cell with a strictly downhill neighbor, flow at the
        // source should equal 1 (its own rain) and flow at the downstream
        // cell should be strictly greater.
        let cfg = test_config(64);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let rm = compute_flow(&map);
        let total = rm.flow.len();
        for i in 0..total {
            if map.land_mask[i] == 0 {
                continue;
            }
            let Some(d) = rm.downstream[i] else {
                continue;
            };
            let di = d as usize;
            assert!(
                rm.flow[di] >= rm.flow[i],
                "downstream flow {} not ≥ upstream {}",
                rm.flow[di],
                rm.flow[i]
            );
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(64);
        let mut a = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut a);
        let mut b = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut b);
        let ra = compute_flow(&a);
        let rb = compute_flow(&b);
        assert_eq!(ra.flow, rb.flow);
        assert_eq!(ra.downstream, rb.downstream);
    }

    #[test]
    fn rivers_extracted() {
        let cfg = test_config(128);
        let mut map = continent::generate_continent_mask(&cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = compute_flow(&map);
        // min_peak_elevation low so small test maps still produce sources.
        extract_rivers(&map, &mut rm, 50.0, 4);
        assert!(!rm.rivers.is_empty(), "no rivers extracted");
        for r in &rm.rivers {
            assert!(r.points.len() >= 4);
        }
    }

    #[test]
    fn near_parallel_rivers_merge_into_existing_stem() {
        let res = 128;
        let mut map = blank_map(res);
        let mut rm = blank_river_map(res);

        let mut main: Vec<(u32, u32)> = (10u32..=44).map(|y| (30, y)).collect();
        main.extend((1u32..=14).map(|k| (30 + k, 44 + k)));
        main.extend((59u32..=100).map(|y| (44, y)));

        let mut parallel: Vec<(u32, u32)> = (10u32..=44).map(|y| (70, y)).collect();
        parallel.extend((1u32..=24).map(|k| (70 - k, 44 + k)));
        parallel.extend((69u32..=100).map(|y| (46, y)));

        seed_manual_river(&mut map, &mut rm, &main, 240.0);
        seed_manual_river(&mut map, &mut rm, &parallel, 220.0);

        extract_rivers(&map, &mut rm, 50.0, 4);

        let main_poly = rm
            .rivers
            .iter()
            .find(|poly| poly.points.first() == Some(&(30, 10)))
            .expect("main river was not extracted");
        assert_eq!(main_poly.points.last(), Some(&(44, 100)));

        let merged_poly = rm
            .rivers
            .iter()
            .find(|poly| poly.points.first() == Some(&(70, 10)))
            .expect("parallel river was not extracted");
        assert_eq!(merged_poly.points.last(), Some(&(44, 68)));
        assert!(
            !merged_poly.points.contains(&(46, 90)),
            "parallel river should stop once it joins the main stem"
        );
    }

    #[test]
    fn meander_keeps_tributary_junctions_connected() {
        let res = 128;
        let map = blank_map(res);
        let junction = (64, 64);
        let main: Vec<(u32, u32)> = (20u32..=100).map(|y| (64, y)).collect();
        let tributary: Vec<(u32, u32)> = (0u32..=34).map(|k| (30 + k, 30 + k)).collect();

        let mut rivers = vec![
            Polyline {
                flow: vec![200.0; main.len()],
                points: main,
            },
            Polyline {
                flow: vec![100.0; tributary.len()],
                points: tributary,
            },
        ];

        naturalize_river_meanders(&map, &mut rivers);

        assert!(
            rivers[0].points.contains(&junction),
            "main stem must keep shared junction anchored"
        );
        assert_eq!(
            rivers[1].points.last(),
            Some(&junction),
            "tributary endpoint must remain on the same junction"
        );
    }
}
