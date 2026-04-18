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

use serde::{Deserialize, Serialize};

use super::global_map::GlobalMap;
use super::grid::bfs_distance_from;
use super::rivers::RiverMap;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Settlement {
    pub cell_x: u32,
    pub cell_y: u32,
    pub score: f32,
}

/// Pick up to `settlement_target_count` settlement sites. Input is the full
/// global map (needs land_mask + elevation_m) plus the river flow field.
pub fn place_settlements(map: &GlobalMap, river_map: &RiverMap) -> Vec<Settlement> {
    let cfg = &map.config;
    let res = cfg.global_res as usize;
    let total = res * res;
    let target = cfg.settlement_target_count as usize;
    if target == 0 {
        return Vec::new();
    }

    let coast_dist = bfs_distance_from(&map.land_mask, res, 0);
    let slope = compute_slope(&map.elevation_m, res, cfg.meters_per_cell());

    let ctx = FitnessCtx {
        res,
        elev: &map.elevation_m,
        coast_dist: &coast_dist,
        slope: &slope,
        flow: &river_map.flow,
        max_slope: cfg.settlement_max_slope,
        max_elev: cfg.settlement_max_elevation_m,
        river_thresh: cfg.settlement_river_flow_threshold.max(1.0),
    };

    let mut scored: Vec<(u32, f32)> = Vec::with_capacity(total / 8);
    for i in 0..total {
        if map.land_mask[i] == 0 {
            continue;
        }
        if map.elevation_m[i] > ctx.max_elev {
            continue;
        }
        if slope[i] > ctx.max_slope {
            continue;
        }
        scored.push((i as u32, fitness(i, &ctx)));
    }

    if scored.is_empty() {
        return Vec::new();
    }

    // Greedy min-spacing rejects some candidates, so we keep a generous
    // headroom over the target count before applying it. `select_nth_unstable_by`
    // is O(n), then we sort only the top partition in place.
    const HEADROOM: usize = 40;
    let keep_top = (target * HEADROOM).min(scored.len());
    let nth = scored.len() - keep_top;
    scored.select_nth_unstable_by(nth, |a, b| a.1.total_cmp(&b.1));
    scored[nth..].sort_by(|a, b| b.1.total_cmp(&a.1));

    let res_f = res as f32;
    let min_spacing_actual = cfg
        .scaled_cells(cfg.settlement_min_spacing_cells as f32)
        .max(1.0);
    let min_sp_sq = min_spacing_actual.powi(2);
    let mut kept: Vec<Settlement> = Vec::with_capacity(target);
    for &(idx, score) in &scored[nth..] {
        if kept.len() >= target {
            break;
        }
        let cx = idx as usize % res;
        let cy = idx as usize / res;
        let x = cx as f32;
        let y = cy as f32;
        let ok = kept.iter().all(|s| {
            let dx_raw = (s.cell_x as f32 - x).abs();
            let dx = dx_raw.min(res_f - dx_raw);
            let dy = s.cell_y as f32 - y;
            dx * dx + dy * dy >= min_sp_sq
        });
        if ok {
            kept.push(Settlement {
                cell_x: cx as u32,
                cell_y: cy as u32,
                score,
            });
        }
    }
    kept
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
    res: usize,
    elev: &'a [f32],
    coast_dist: &'a [u16],
    slope: &'a [f32],
    flow: &'a [f32],
    max_slope: f32,
    max_elev: f32,
    river_thresh: f32,
}

// Coastal ideal + spread, in cells. Bell-curve ideal is ~120m inland at the
// 8m reference cell: close enough to the shore to feel coastal, far enough
// that cities don't pile on the water line.
const COAST_IDEAL_CELLS: f32 = 15.0;
const COAST_SIGMA_CELLS: f32 = 18.0;
const RIVER_LOG_CAP: f32 = 3.0;
const W_COAST: f32 = 0.8;
const W_RIVER: f32 = 1.5;
const W_PLAINS: f32 = 0.5;
const W_ELEV: f32 = 0.4;

fn fitness(i: usize, ctx: &FitnessCtx) -> f32 {
    let coast_cells = ctx.coast_dist[i] as f32;
    let coast_score = (-((coast_cells - COAST_IDEAL_CELLS).powi(2)
        / (2.0 * COAST_SIGMA_CELLS * COAST_SIGMA_CELLS)))
        .exp();

    // River score uses the max flow across the cell + 4-neighbors so a
    // settlement one cell off the actual riverbed still gets the bonus.
    let res = ctx.res;
    let x = i % res;
    let y = i / res;
    let left = if x == 0 { res - 1 } else { x - 1 };
    let right = if x + 1 == res { 0 } else { x + 1 };
    let candidates = [
        ctx.flow[i],
        ctx.flow[y * res + left],
        ctx.flow[y * res + right],
        if y > 0 {
            ctx.flow[(y - 1) * res + x]
        } else {
            0.0
        },
        if y + 1 < res {
            ctx.flow[(y + 1) * res + x]
        } else {
            0.0
        },
    ];
    let f = candidates.iter().cloned().fold(0.0f32, f32::max);
    let river_score = if f >= ctx.river_thresh {
        (f / ctx.river_thresh).ln().min(RIVER_LOG_CAP)
    } else {
        0.0
    };

    let plains_score = 1.0 - (ctx.slope[i] / ctx.max_slope).clamp(0.0, 1.0);
    let elev_score = 1.0 - (ctx.elev[i] / ctx.max_elev).clamp(0.0, 1.0);

    W_COAST * coast_score + W_RIVER * river_score + W_PLAINS * plains_score + W_ELEV * elev_score
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
