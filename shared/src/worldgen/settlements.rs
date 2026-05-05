//! Phase 5: settlement placement.
//!
//! Score every habitable land cell by terrain fitness (coast proximity,
//! river proximity, low slope) and greedily pick the highest-scoring cells
//! subject to a minimum-spacing constraint. The result is a list of
//! settlement positions used by later phases (road network, splatmap
//! tinting, spawn zones).
//!
//! Habitability filters are hard cutoffs — cells above the max elevation
//! or steeper than the slope cap are excluded outright. Everything else
//! is a soft bias in the score.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::rivers::{Polyline, RiverMap};
use super::roads::RoadNetwork;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Settlement {
    pub cell_x: u32,
    pub cell_y: u32,
    pub score: f32,
}

/// Precomputed per-cell fields that both Phase 5 entry points need. Building
/// them once and sharing avoids redoing the coast BFS, slope field, and
/// river-distance BFS for every call site (two full-map BFS passes each).
pub struct HabitabilityFields {
    pub coast_dist: Vec<u16>,
    pub slope: Vec<f32>,
    pub dist_to_river: Vec<u16>,
}

pub fn compute_habitability_fields(map: &GlobalMap, river_map: &RiverMap) -> HabitabilityFields {
    let cfg = &map.config;
    let res = cfg.global_res as usize;
    let total = res * res;
    let coast_dist = bfs_distance_from(&map.land_mask, res, 0, None);
    let slope = compute_slope(&map.elevation_m, res, cfg.meters_per_cell());
    let river_thresh = cfg.settlement_river_flow_threshold.max(1.0);
    let mut river_mask = vec![0u8; total];
    for (i, &f) in river_map.flow.iter().enumerate() {
        if f >= river_thresh && map.land_mask[i] == 1 {
            river_mask[i] = 1;
        }
    }
    let dist_to_river = bfs_distance_from(&river_mask, res, 1, None);
    HabitabilityFields {
        coast_dist,
        slope,
        dist_to_river,
    }
}

/// Pick up to `settlement_target_count` settlement sites plus one guaranteed
/// settlement per isolated landmass. Input is the full global map plus the
/// river flow field; callers can also use `place_settlements_with_fields`
/// directly to avoid recomputing the habitability fields.
pub fn place_settlements(map: &GlobalMap, river_map: &RiverMap) -> Vec<Settlement> {
    let fields = compute_habitability_fields(map, river_map);
    place_settlements_with_fields(map, river_map, &fields)
}

pub fn place_settlements_with_fields(
    map: &GlobalMap,
    river_map: &RiverMap,
    fields: &HabitabilityFields,
) -> Vec<Settlement> {
    let cfg = &map.config;
    let res = cfg.global_res as usize;
    let total = res * res;
    let target = cfg.settlement_target_count as usize;
    if target == 0 {
        return Vec::new();
    }
    let HabitabilityFields {
        coast_dist,
        slope,
        dist_to_river: _,
    } = fields;

    let ctx = FitnessCtx::from_config(map, fields);

    let res_f = res as f32;
    let min_spacing_actual = cfg
        .scaled_cells(cfg.settlement_min_spacing_cells as f32)
        .max(1.0);
    let min_sp_sq = min_spacing_actual.powi(2);
    let coastal_sp_sq = (min_spacing_actual * cfg.settlement_coastal_spacing_mult.max(1.0)).powi(2);
    let coast_threshold = cfg.scaled_cells_usize(cfg.settlement_inland_buffer_cells) as u16;
    let spacing = SpacingCtx {
        res_f,
        min_sp_sq,
        coastal_sp_sq,
        coast_dist,
        coast_threshold,
    };
    // Phase-A picks cluster on the same elevation/slope contour when
    // sibling rivers share a valley plain, so inflate the per-pick spacing
    // here to push them across distinct valleys. Phases B and C use the
    // unmultiplied spacing so islands and infill villages aren't starved.
    let phase_a_mult = cfg.settlement_phase_a_spacing_mult.max(1.0);
    let phase_a_spacing = SpacingCtx {
        min_sp_sq: (min_spacing_actual * phase_a_mult).powi(2),
        ..spacing
    };
    let mut kept: Vec<Settlement> = Vec::with_capacity(target);

    // Phase A (river quota): one settlement per *drainage basin*. Picking
    // per mouth (instead of per polyline) keeps tributaries of one valley
    // from each landing their own village in a tight inland cluster. The
    // top-N basins by mouth flow get a coastal port at the mouth; the rest
    // get the inland middle-reach pick.
    let inland_buffer = cfg.scaled_cells_usize(cfg.settlement_inland_buffer_cells) as u16;
    let river_quota = ((target as f32 * 0.7) as usize).max(1).min(target);
    let mut rivers_sorted: Vec<&Polyline> = river_map.rivers.iter().collect();
    rivers_sorted.sort_by_key(|p| std::cmp::Reverse(p.points.len()));

    // Cache each polyline's mouth cell once so the unique-mouths build and
    // the per-river loop don't both walk the polyline tail.
    let polyline_mouths: Vec<Option<usize>> = rivers_sorted
        .iter()
        .map(|p| mouth_land_cell(p, map, res))
        .collect();
    let mut unique_mouths: Vec<usize> = polyline_mouths
        .iter()
        .filter_map(|m| *m)
        .collect::<HashSet<usize>>()
        .into_iter()
        .collect();
    unique_mouths.sort_by(|a, b| river_map.flow[*b].total_cmp(&river_map.flow[*a]));
    let mouth_target = (cfg.settlement_mouth_count as usize).min(unique_mouths.len());
    let mouth_pick_set: HashSet<usize> = unique_mouths.iter().take(mouth_target).copied().collect();

    let mut seen_mouths: HashSet<usize> = HashSet::new();
    for (poly, mouth_opt) in rivers_sorted.iter().zip(polyline_mouths.iter()) {
        if kept.len() >= river_quota {
            break;
        }
        let Some(mouth) = *mouth_opt else { continue };
        if !seen_mouths.insert(mouth) {
            continue;
        }
        let best = if mouth_pick_set.contains(&mouth) {
            best_mouth_cell(poly, map, slope, &ctx, res)
                .or_else(|| best_middle_cell(poly, map, slope, &ctx, inland_buffer, res))
        } else {
            best_middle_cell(poly, map, slope, &ctx, inland_buffer, res)
        };
        if let Some((idx, score)) = best {
            try_place(idx, score, res, &phase_a_spacing, &mut kept);
        }
    }

    // Phase B (interior plains): fill remaining slots using a plains-heavy
    // fitness so fertile flatlands without rivers get occasional villages.
    // Long rivers still pick up a 2nd/3rd settlement here because on-river
    // plains also score highly, just without river dominance.
    if kept.len() < target {
        let mut scored: Vec<(u32, f32)> = Vec::with_capacity(total / 8);
        for i in 0..total {
            if !habitable(i, map, slope, &ctx) {
                continue;
            }
            scored.push((i as u32, fitness_plains(i, &ctx)));
        }
        if !scored.is_empty() {
            const HEADROOM: usize = 40;
            let remaining = target - kept.len();
            let keep_top = (remaining * HEADROOM).min(scored.len());
            let nth = scored.len() - keep_top;
            scored.select_nth_unstable_by(nth, |a, b| a.1.total_cmp(&b.1));
            scored[nth..].sort_by(|a, b| b.1.total_cmp(&a.1));
            for &(idx, score) in &scored[nth..] {
                if kept.len() >= target {
                    break;
                }
                try_place(idx as usize, score, res, &spacing, &mut kept);
            }
        }
    }

    // Phase C (island seeding): guarantee every isolated landmass gets at
    // least one village. Small islands lose in Phase A (no rivers) and
    // Phase B (global greedy prefers mainland cells) so a dedicated pass
    // is required. Can push kept.len() past `target` — that's intentional.
    seed_per_component(map, &ctx, slope, &spacing, &mut kept);

    // Phase D (coverage fill): drop a settlement at the most isolated
    // habitable cell as long as some cell is farther than max_gap_m from
    // any existing settlement. Guarantees every habitable area is within
    // walking distance of a town even where Phase A's per-river picks
    // and Phase B's plains heuristic leave a gap.
    seed_coverage_gaps(map, &ctx, slope, &mut kept);

    kept
}

/// Iteratively place settlements at the habitable cell that is farthest from
/// any existing settlement (walking distance through land only) until every
/// habitable cell is within `settlement_max_gap_m`. Bypasses min-spacing —
/// the gap criterion replaces it. Sea is impassable so an isolated landmass
/// can't appear "covered" by a city across the strait.
fn seed_coverage_gaps(
    map: &GlobalMap,
    ctx: &FitnessCtx,
    slope: &[f32],
    kept: &mut Vec<Settlement>,
) {
    let cfg = &map.config;
    let max_gap_m = cfg.settlement_max_gap_m;
    if max_gap_m <= 0.0 {
        return;
    }
    let res = cfg.global_res as usize;
    let total = res * res;
    let max_gap_cells = (max_gap_m / cfg.meters_per_cell()).round() as u16;

    // Precompute the habitable mask once: the per-cell elevation/slope/coast
    // checks each touch four arrays, and we'd otherwise redo them across the
    // whole map per outer iteration.
    let habitable_mask: Vec<u8> = (0..total)
        .map(|i| habitable(i, map, slope, ctx) as u8)
        .collect();

    let mut sources = vec![0u8; total];
    for s in kept.iter() {
        let i = (s.cell_y as usize) * res + s.cell_x as usize;
        sources[i] = 1;
    }
    let mut dist = bfs_distance_from(&sources, res, 1, Some(&map.land_mask));

    // Reused single-source mask; we set/clear the picked cell each pass
    // instead of allocating a fresh `vec![0u8; total]` per iteration.
    let mut single_source = vec![0u8; total];
    loop {
        let mut farthest_idx: Option<usize> = None;
        let mut farthest_d: u16 = 0;
        for i in 0..total {
            if habitable_mask[i] == 0 {
                continue;
            }
            let d = dist[i];
            if d <= max_gap_cells || d == u16::MAX {
                continue;
            }
            if d > farthest_d {
                farthest_d = d;
                farthest_idx = Some(i);
            }
        }
        let Some(idx) = farthest_idx else {
            break;
        };
        kept.push(Settlement {
            cell_x: (idx % res) as u32,
            cell_y: (idx / res) as u32,
            score: fitness_plains(idx, ctx),
        });

        // Pointwise-min with a single-source BFS from the new pick — far
        // cells already have their final distance, so this converges fast.
        single_source[idx] = 1;
        let new_dist = bfs_distance_from(&single_source, res, 1, Some(&map.land_mask));
        single_source[idx] = 0;
        for j in 0..total {
            if new_dist[j] < dist[j] {
                dist[j] = new_dist[j];
            }
        }
    }
}

fn seed_per_component(
    map: &GlobalMap,
    ctx: &FitnessCtx,
    slope: &[f32],
    spacing: &SpacingCtx,
    kept: &mut Vec<Settlement>,
) {
    let res = map.config.global_res as usize;
    let total = res * res;

    let mut has_existing = vec![false; total];
    for s in kept.iter() {
        let i = (s.cell_y as usize) * res + s.cell_x as usize;
        has_existing[i] = true;
    }

    let mut label = vec![0u32; total];
    let mut stack: Vec<usize> = Vec::new();
    let mut next_label: u32 = 1;

    // Single flood-fill pass that both labels components and tracks the
    // best-fitness uninhabited cell per component.
    for start in 0..total {
        if map.land_mask[start] != 1 || label[start] != 0 {
            continue;
        }
        stack.clear();
        stack.push(start);
        label[start] = next_label;
        let mut best: Option<(usize, f32)> = None;
        let mut has_settlement = false;
        while let Some(ci) = stack.pop() {
            if has_existing[ci] {
                has_settlement = true;
            }
            if habitable(ci, map, slope, ctx) {
                let s = fitness_plains(ci, ctx);
                if best.map(|(_, bs)| s > bs).unwrap_or(true) {
                    best = Some((ci, s));
                }
            }
            let x = ci % res;
            let y = ci / res;
            let left = if x == 0 { res - 1 } else { x - 1 };
            let right = if x + 1 == res { 0 } else { x + 1 };
            let neighbors = [
                y * res + left,
                y * res + right,
                if y > 0 { (y - 1) * res + x } else { usize::MAX },
                if y + 1 < res {
                    (y + 1) * res + x
                } else {
                    usize::MAX
                },
            ];
            for &ni in &neighbors {
                if ni != usize::MAX && map.land_mask[ni] == 1 && label[ni] == 0 {
                    label[ni] = next_label;
                    stack.push(ni);
                }
            }
        }
        if !has_settlement {
            if let Some((idx, score)) = best {
                try_place(idx, score, res, spacing, kept);
            }
        }
        next_label += 1;
    }
}

/// Index of the polyline's last land cell, or `None` if the polyline has no
/// land cells (purely sea, shouldn't normally happen). Rivers are extracted
/// upstream→downstream so the mouth is at the end.
fn mouth_land_cell(poly: &Polyline, map: &GlobalMap, res: usize) -> Option<usize> {
    poly.points.iter().rev().find_map(|&(x, y)| {
        let i = (y as usize) * res + (x as usize);
        if map.land_mask[i] == 1 {
            Some(i)
        } else {
            None
        }
    })
}

/// Two-char base-36 settlement ID (e.g. "00", "1k", "??" for indices past
/// 36²). Used by both the worldgen preview's PNG overlay and the road
/// pipeline's diagnostic logs so grepping a log line against the overlay
/// PNG matches.
pub fn settlement_label(idx: usize) -> String {
    const ALPHA: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if idx < 36 * 36 {
        format!("{}{}", ALPHA[idx / 36] as char, ALPHA[idx % 36] as char)
    } else {
        "??".to_string()
    }
}

/// Best habitable cell within a small radius around the river mouth that
/// sits on or near the coast. Returns `None` if no such cell exists (mouth
/// in a sink, surrounding land too steep, etc.) so the caller can fall back
/// to the middle-reach pick.
fn best_mouth_cell(
    poly: &Polyline,
    map: &GlobalMap,
    slope: &[f32],
    ctx: &FitnessCtx,
    res: usize,
) -> Option<(usize, f32)> {
    const SEARCH_RADIUS: i32 = 8;
    const COAST_MAX_DIST: u16 = 6;
    let mouth = mouth_land_cell(poly, map, res)?;
    let mx = (mouth % res) as i32;
    let my = (mouth / res) as i32;
    let mut best: Option<(usize, f32)> = None;
    for dy in -SEARCH_RADIUS..=SEARCH_RADIUS {
        let ny = my + dy;
        if ny < 0 || ny >= res as i32 {
            continue;
        }
        for dx in -SEARCH_RADIUS..=SEARCH_RADIUS {
            if dx * dx + dy * dy > SEARCH_RADIUS * SEARCH_RADIUS {
                continue;
            }
            let nx = (mx + dx).rem_euclid(res as i32) as usize;
            let ni = (ny as usize) * res + nx;
            if !habitable(ni, map, slope, ctx) {
                continue;
            }
            if ctx.coast_dist[ni] > COAST_MAX_DIST {
                continue;
            }
            let s = fitness_plains(ni, ctx);
            if best.map(|(_, bs)| s > bs).unwrap_or(true) {
                best = Some((ni, s));
            }
        }
    }
    best
}

/// Best inland cell along the polyline that sits at least `inland_buffer`
/// cells from the coast. Default Phase-A behavior for rivers not picked
/// for coastal-port treatment.
fn best_middle_cell(
    poly: &Polyline,
    map: &GlobalMap,
    slope: &[f32],
    ctx: &FitnessCtx,
    inland_buffer: u16,
    res: usize,
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32)> = None;
    for &(rx, ry) in &poly.points {
        let ci = (ry as usize) * res + (rx as usize);
        if !habitable(ci, map, slope, ctx) {
            continue;
        }
        if ctx.coast_dist[ci] < inland_buffer {
            continue;
        }
        let s = fitness_plains(ci, ctx);
        if best.map(|(_, bs)| s > bs).unwrap_or(true) {
            best = Some((ci, s));
        }
    }
    best
}

fn habitable(i: usize, map: &GlobalMap, slope: &[f32], ctx: &FitnessCtx) -> bool {
    if map.land_mask[i] != 1 || map.elevation_m[i] > ctx.max_elev || slope[i] > ctx.max_slope {
        return false;
    }
    let cy = i / map.config.global_res as usize;
    cy <= ctx.max_cy
}

struct SpacingCtx<'a> {
    res_f: f32,
    min_sp_sq: f32,
    coastal_sp_sq: f32,
    coast_dist: &'a [u16],
    coast_threshold: u16,
}

fn try_place(idx: usize, score: f32, res: usize, sp: &SpacingCtx, kept: &mut Vec<Settlement>) {
    let cx = idx % res;
    let cy = idx / res;
    let x = cx as f32;
    let y = cy as f32;
    let new_coastal = sp.coast_dist[idx] < sp.coast_threshold;
    let ok = kept.iter().all(|s| {
        let si = (s.cell_y as usize) * res + s.cell_x as usize;
        // Coastal cells get enlarged spacing against any neighbor — breaks
        // the "fence of villages every N cells along the shore" pattern.
        let required_sq = if new_coastal || sp.coast_dist[si] < sp.coast_threshold {
            sp.coastal_sp_sq
        } else {
            sp.min_sp_sq
        };
        let dx_raw = (s.cell_x as f32 - x).abs();
        let dx = dx_raw.min(sp.res_f - dx_raw);
        let dy = s.cell_y as f32 - y;
        dx * dx + dy * dy >= required_sq
    });
    if ok {
        kept.push(Settlement {
            cell_x: cx as u32,
            cell_y: cy as u32,
            score,
        });
    }
}

/// Dimensionless slope (rise/run) per cell via central difference on the
/// elevation. X wraps, Y clamps.
fn compute_slope(elev: &[f32], res: usize, meters_per_cell: f32) -> Vec<f32> {
    let total = res * res;
    let mut slope = vec![0.0f32; total];
    let inv_2dx = 1.0 / (2.0 * meters_per_cell);
    for y in 0..res {
        let yu = if y > 0 { y - 1 } else { y };
        let yd = if y + 1 < res { y + 1 } else { y };
        for x in 0..res {
            let xl = if x == 0 { res - 1 } else { x - 1 };
            let xr = if x + 1 == res { 0 } else { x + 1 };
            let dzdx = (elev[y * res + xr] - elev[y * res + xl]) * inv_2dx;
            let dzdy = (elev[yd * res + x] - elev[yu * res + x]) * inv_2dx;
            slope[y * res + x] = (dzdx * dzdx + dzdy * dzdy).sqrt();
        }
    }
    slope
}

struct FitnessCtx<'a> {
    elev: &'a [f32],
    coast_dist: &'a [u16],
    slope: &'a [f32],
    dist_to_river: &'a [u16],
    max_slope: f32,
    max_elev: f32,
    /// Largest cell-y (inclusive) that passes habitability. Cells with
    /// `cy > max_cy` are inside the south-edge exclusion band. `usize::MAX`
    /// disables the check.
    max_cy: usize,
}

impl FitnessCtx<'_> {
    fn from_config<'a>(map: &'a GlobalMap, fields: &'a HabitabilityFields) -> FitnessCtx<'a> {
        let cfg = &map.config;
        let max_cy = if cfg.settlement_south_edge_exclusion_m > 0.0 {
            let res = cfg.global_res as usize;
            let excl_cells =
                (cfg.settlement_south_edge_exclusion_m / cfg.meters_per_cell()).ceil() as usize;
            res.saturating_sub(excl_cells + 1)
        } else {
            usize::MAX
        };
        FitnessCtx {
            elev: &map.elevation_m,
            coast_dist: &fields.coast_dist,
            slope: &fields.slope,
            dist_to_river: &fields.dist_to_river,
            max_slope: cfg.settlement_max_slope,
            max_elev: cfg.settlement_max_elevation_m,
            max_cy,
        }
    }
}

// Coastal Gaussian: peak ~15 cells inland (120m at the 8m reference cell),
// sigma 18 cells. Small weight because coastal cells are already abundant.
const COAST_IDEAL_CELLS: f32 = 15.0;
const COAST_SIGMA_CELLS: f32 = 18.0;
// Distance at which the river bonus decays to zero. ~80m at reference scale
// makes "on the river" an inclusive band (settlements can sit a street away
// from the water and still count).
const RIVER_INFLUENCE_CELLS: f32 = 10.0;

// Weights for the plains-emphasis scorer. River/coast barely contribute so
// agricultural heartland (flat, low, dry) competes with river-adjacent spots.
const WP_PLAINS: f32 = 2.0;
const WP_ELEV: f32 = 1.5;
const WP_RIVER: f32 = 0.5;
const WP_COAST: f32 = 0.2;

/// Seed additional villages along road cells — every road eventually grows
/// a handful of wayside settlements, so pick the highest-fitness cells on
/// the road polylines that clear min-spacing against the existing set.
pub fn place_settlements_along_roads(
    map: &GlobalMap,
    river_map: &RiverMap,
    roads: &RoadNetwork,
    existing: &[Settlement],
    target_additional: usize,
) -> Vec<Settlement> {
    let fields = compute_habitability_fields(map, river_map);
    place_settlements_along_roads_with_fields(map, roads, existing, target_additional, &fields)
}

pub fn place_settlements_along_roads_with_fields(
    map: &GlobalMap,
    roads: &RoadNetwork,
    existing: &[Settlement],
    target_additional: usize,
    fields: &HabitabilityFields,
) -> Vec<Settlement> {
    if target_additional == 0 || roads.roads.is_empty() {
        return Vec::new();
    }
    let cfg = &map.config;
    let res = cfg.global_res as usize;
    let HabitabilityFields {
        coast_dist,
        slope,
        dist_to_river: _,
    } = fields;
    let ctx = FitnessCtx::from_config(map, fields);

    let mut road_cells: Vec<u32> = Vec::new();
    for road in &roads.roads {
        for &(x, y) in &road.points {
            road_cells.push(((y as usize) * res + x as usize) as u32);
        }
    }
    road_cells.sort_unstable();
    road_cells.dedup();

    let mut scored: Vec<(u32, f32)> = road_cells
        .into_iter()
        .filter_map(|ci| {
            let ci_u = ci as usize;
            if habitable(ci_u, map, slope, &ctx) {
                Some((ci, fitness_plains(ci_u, &ctx)))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));

    let res_f = res as f32;
    let min_sp = cfg
        .scaled_cells(cfg.settlement_min_spacing_cells as f32)
        .max(1.0);
    let min_sp_sq = min_sp.powi(2);
    let coastal_sp_sq = (min_sp * cfg.settlement_coastal_spacing_mult.max(1.0)).powi(2);
    let coast_threshold = cfg.scaled_cells_usize(cfg.settlement_inland_buffer_cells) as u16;
    let spacing = SpacingCtx {
        res_f,
        min_sp_sq,
        coastal_sp_sq,
        coast_dist,
        coast_threshold,
    };

    // Seed kept with the existing settlements so min-spacing keeps the new
    // villages from overlapping them.
    let mut kept: Vec<Settlement> = existing.to_vec();
    let initial_len = kept.len();
    for (idx, score) in scored {
        if kept.len() - initial_len >= target_additional {
            break;
        }
        try_place(idx as usize, score, res, &spacing, &mut kept);
    }
    kept.split_off(initial_len)
}

fn fitness_plains(i: usize, ctx: &FitnessCtx) -> f32 {
    let coast_cells = ctx.coast_dist[i] as f32;
    let coast_score = (-((coast_cells - COAST_IDEAL_CELLS).powi(2)
        / (2.0 * COAST_SIGMA_CELLS * COAST_SIGMA_CELLS)))
        .exp();
    let dist = ctx.dist_to_river[i] as f32;
    let river_score = (1.0 - dist / RIVER_INFLUENCE_CELLS).max(0.0);
    let plains_score = 1.0 - (ctx.slope[i] / ctx.max_slope).clamp(0.0, 1.0);
    let elev_score = 1.0 - (ctx.elev[i] / ctx.max_elev).clamp(0.0, 1.0);
    WP_PLAINS * plains_score
        + WP_ELEV * elev_score
        + WP_RIVER * river_score
        + WP_COAST * coast_score
}

#[cfg(test)]
mod tests {
    use super::super::{continent, elevation, rivers};
    use super::*;
    use crate::worldgen::config::WorldGenConfig;

    fn test_config(res: u32) -> WorldGenConfig {
        WorldGenConfig {
            seed: 0xBEEF,
            global_res: res,
            reference_res: res,
            sea_ratio: 0.35,
            settlement_target_count: 8,
            settlement_min_spacing_cells: (res / 10).max(4),
            settlement_river_flow_threshold: 20.0,
            // Tests assert target_count and min-spacing invariants that
            // Phase D's coverage-fill intentionally relaxes; disable here.
            settlement_max_gap_m: 0.0,
            ..WorldGenConfig::default()
        }
    }

    fn full_map(cfg: &WorldGenConfig) -> (GlobalMap, RiverMap) {
        let mut map = continent::generate_continent_mask(cfg);
        elevation::generate_elevation(&mut map);
        let mut rm = rivers::compute_flow(&map);
        rivers::extract_rivers(&map, &mut rm, 50.0, 4);
        (map, rm)
    }

    #[test]
    fn settlements_respect_min_spacing() {
        let cfg = test_config(128);
        let (map, rm) = full_map(&cfg);
        let settlements = place_settlements(&map, &rm);
        let min_sp = cfg.settlement_min_spacing_cells as f32;
        let min_sp_sq = min_sp * min_sp;
        let res_f = cfg.global_res as f32;
        for (i, a) in settlements.iter().enumerate() {
            for b in &settlements[i + 1..] {
                let dx_raw = (a.cell_x as f32 - b.cell_x as f32).abs();
                let dx = dx_raw.min(res_f - dx_raw);
                let dy = a.cell_y as f32 - b.cell_y as f32;
                let d2 = dx * dx + dy * dy;
                assert!(
                    d2 >= min_sp_sq,
                    "settlements too close: ({}, {}) vs ({}, {}), d²={d2}",
                    a.cell_x,
                    a.cell_y,
                    b.cell_x,
                    b.cell_y
                );
            }
        }
    }

    #[test]
    fn settlements_are_on_habitable_land() {
        let cfg = test_config(128);
        let (map, rm) = full_map(&cfg);
        let settlements = place_settlements(&map, &rm);
        let res = cfg.global_res as usize;
        for s in &settlements {
            let i = (s.cell_y as usize) * res + s.cell_x as usize;
            assert_eq!(map.land_mask[i], 1, "settlement placed on sea");
            assert!(
                map.elevation_m[i] <= cfg.settlement_max_elevation_m,
                "settlement above elevation cap"
            );
        }
    }

    #[test]
    fn deterministic_for_same_seed() {
        let cfg = test_config(128);
        let (a_map, a_rm) = full_map(&cfg);
        let (b_map, b_rm) = full_map(&cfg);
        let a = place_settlements(&a_map, &a_rm);
        let b = place_settlements(&b_map, &b_rm);
        assert_eq!(a.len(), b.len());
        for (sa, sb) in a.iter().zip(b.iter()) {
            assert_eq!((sa.cell_x, sa.cell_y), (sb.cell_x, sb.cell_y));
        }
    }

    #[test]
    fn target_count_respected_when_land_available() {
        let mut cfg = test_config(256);
        cfg.settlement_target_count = 4;
        cfg.settlement_min_spacing_cells = 15;
        let (map, rm) = full_map(&cfg);
        let settlements = place_settlements(&map, &rm);
        assert!(
            settlements.len() <= cfg.settlement_target_count as usize,
            "got {} settlements, target was {}",
            settlements.len(),
            cfg.settlement_target_count
        );
        assert!(!settlements.is_empty(), "no settlements placed at all");
    }
}
