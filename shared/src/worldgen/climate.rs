//! Climate zones for the weather system (doc/WEATHER_SYSTEM.md). One byte per
//! 32 m land plot, derived from the baked elevation and land mask so no zone
//! is hand-authored.

use std::collections::VecDeque;

use super::grid::bfs_distance_from;
use super::GlobalMap;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Climate {
    Sea = 0,
    WetCoast = 1,
    Temperate = 2,
    RainShadow = 3,
    Alpine = 4,
}

impl TryFrom<u8> for Climate {
    type Error = u8;

    fn try_from(byte: u8) -> Result<Self, u8> {
        match byte {
            0 => Ok(Self::Sea),
            1 => Ok(Self::WetCoast),
            2 => Ok(Self::Temperate),
            3 => Ok(Self::RainShadow),
            4 => Ok(Self::Alpine),
            other => Err(other),
        }
    }
}

pub const COAST_BAND_M: f32 = 1000.0;
pub const COAST_MAX_ELEVATION_M: f32 = 900.0;
/// Prevailing wind is westerly: a ridge this high within the lookback to the
/// west leaves the lowland east of it in a rain shadow. Set above the coastal
/// ranges (about 1,000 m on seed 42) so only the central massif casts one.
pub const SHADOW_RIDGE_M: f32 = 1200.0;
pub const SHADOW_LOOKBACK_M: f32 = 14_000.0;
/// The ridge must span this much north-south to block: a single spur casts
/// no shadow, so the zone stays a band instead of one-cell-high streaks.
pub const SHADOW_RIDGE_SPAN_M: f32 = 1000.0;
pub const SHADOW_MAX_ELEVATION_M: f32 = 700.0;
/// Below the generator's permanent-snow line (1,800 m) so the zone covers the
/// whole summit block, not just the painted snow.
pub const ALPINE_M: f32 = 1500.0;

/// Whole-world fields the zone rules sample. Built once per bake.
pub struct ClimateFields {
    res: usize,
    coast_dist: Vec<u16>,
    upwind_max: Vec<f32>,
}

impl ClimateFields {
    pub fn new(map: &GlobalMap) -> Self {
        let res = map.config.global_res as usize;
        let mpc = map.config.meters_per_cell();
        let window = (SHADOW_LOOKBACK_M / mpc).round() as usize;
        let half_span = (SHADOW_RIDGE_SPAN_M * 0.5 / mpc).round() as usize;
        let upwind = upwind_max(&map.elevation_m, &map.land_mask, res, window);
        Self {
            res,
            coast_dist: bfs_distance_from(&map.land_mask, res, 0, None),
            upwind_max: vertical_min(&upwind, res, half_span),
        }
    }

    pub fn climate_at_cell(&self, map: &GlobalMap, x: u32, y: u32) -> Climate {
        let i = map.idx(x, y);
        if map.land_mask[i] == 0 {
            return Climate::Sea;
        }
        let elev = map.elevation_m[i];
        if elev >= ALPINE_M {
            return Climate::Alpine;
        }
        let coast_cells = COAST_BAND_M / map.config.meters_per_cell();
        if f32::from(self.coast_dist[i]) <= coast_cells && elev < COAST_MAX_ELEVATION_M {
            return Climate::WetCoast;
        }
        if self.upwind_max[i] >= SHADOW_RIDGE_M && elev < SHADOW_MAX_ELEVATION_M {
            return Climate::RainShadow;
        }
        Climate::Temperate
    }

    /// X wraps around the cylinder; Z is clamped to the map.
    pub fn climate_at_world(&self, map: &GlobalMap, x_m: f32, z_m: f32) -> Climate {
        let (x, y) = self.cell_of(map, x_m, z_m);
        self.climate_at_cell(map, x, y)
    }

    /// Majority zone over the square with min corner (`x_m`, `z_m`); a coastal
    /// plot counts as land unless most of it is under water.
    pub fn climate_of_square(&self, map: &GlobalMap, x_m: f32, z_m: f32, size_m: f32) -> Climate {
        let mpc = map.config.meters_per_cell();
        let n = (size_m / mpc).round().max(1.0) as u32;
        let (x0, y0) = self.cell_of(map, x_m + mpc * 0.5, z_m + mpc * 0.5);
        let mut counts = [0u32; 5];
        for dy in 0..n {
            for dx in 0..n {
                let x = (x0 + dx) % self.res as u32;
                let y = (y0 + dy).min(self.res as u32 - 1);
                counts[self.climate_at_cell(map, x, y) as usize] += 1;
            }
        }
        if counts[0] * 2 > n * n {
            return Climate::Sea;
        }
        let land = counts[1..]
            .iter()
            .enumerate()
            .max_by_key(|&(_, c)| *c)
            .map(|(i, _)| i as u8 + 1)
            .unwrap_or(0);
        Climate::try_from(land).unwrap_or(Climate::Temperate)
    }

    fn cell_of(&self, map: &GlobalMap, x_m: f32, z_m: f32) -> (u32, u32) {
        let (cx, cy) = map.config.world_m_to_cell(x_m, z_m);
        let res = self.res as i64;
        let x = (cx.floor() as i64).rem_euclid(res) as u32;
        let y = (cy.floor() as i64).clamp(0, res - 1) as u32;
        (x, y)
    }
}

/// Per cell, the highest land elevation in the `window` cells to its west
/// (excluding itself), wrapping in X. Sea resets the scan: moisture picked up
/// over open water is not blocked by a ridge further upwind. Sliding-window
/// maximum per row.
fn upwind_max(elev: &[f32], land: &[u8], res: usize, window: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; elev.len()];
    let mut deque: VecDeque<(usize, f32)> = VecDeque::new();
    for y in 0..res {
        let row = &elev[y * res..(y + 1) * res];
        let row_land = &land[y * res..(y + 1) * res];
        deque.clear();
        for p in 0..2 * res {
            if p >= res {
                while deque.front().is_some_and(|&(pos, _)| pos + window < p) {
                    deque.pop_front();
                }
                out[y * res + p - res] = deque.front().map_or(0.0, |&(_, v)| v);
            }
            if row_land[p % res] == 0 {
                deque.clear();
                continue;
            }
            let v = row[p % res];
            while deque.back().is_some_and(|&(_, back)| back <= v) {
                deque.pop_back();
            }
            deque.push_back((p, v));
        }
    }
    out
}

/// Per cell, the lowest value within `half` rows above and below (clamped at
/// the map edge). Sliding-window minimum per column.
fn vertical_min(field: &[f32], res: usize, half: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; field.len()];
    let mut deque: VecDeque<(usize, f32)> = VecDeque::new();
    for x in 0..res {
        deque.clear();
        let mut fed = 0;
        for y in 0..res {
            while fed < res && fed <= y + half {
                let v = field[fed * res + x];
                while deque.back().is_some_and(|&(_, back)| back >= v) {
                    deque.pop_back();
                }
                deque.push_back((fed, v));
                fed += 1;
            }
            while deque.front().is_some_and(|&(pos, _)| pos + half < y) {
                deque.pop_front();
            }
            out[y * res + x] = deque.front().map_or(0.0, |&(_, v)| v);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worldgen::WorldGenConfig;

    /// 64 m cells keep the 14 km upwind window (219 cells) inside a small map.
    fn map(res: u32) -> GlobalMap {
        let total = (res * res) as usize;
        GlobalMap {
            config: WorldGenConfig {
                global_res: res,
                world_size_m: res * 64,
                ..Default::default()
            },
            continent_potential: vec![1.0; total],
            land_mask: vec![1; total],
            sea_level_potential: 0.0,
            elevation_m: vec![0.0; total],
            water_after_erosion: Vec::new(),
        }
    }

    #[test]
    fn upwind_max_looks_west_and_wraps() {
        let row = [0.0, 5.0, 0.0, 0.0, 9.0, 0.0, 0.0, 0.0];
        let grid: Vec<f32> = row.iter().cycle().take(64).copied().collect();
        let land = vec![1u8; 64];
        let out = upwind_max(&grid, &land, 8, 2);
        assert_eq!(out[..8], [0.0, 0.0, 5.0, 5.0, 0.0, 9.0, 9.0, 0.0]);
        assert_eq!(out[56..], out[..8]);
        let wide = upwind_max(&grid, &land, 8, 5);
        assert_eq!(wide[0], 9.0);
        assert_eq!(wide[1], 9.0);
        assert_eq!(wide[2], 5.0);
    }

    #[test]
    fn sea_between_ridge_and_cell_clears_the_shadow() {
        let row = [0.0, 9.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let grid: Vec<f32> = row.iter().cycle().take(64).copied().collect();
        let mut land = vec![1u8; 64];
        for y in 0..8 {
            land[y * 8 + 3] = 0;
        }
        let out = upwind_max(&grid, &land, 8, 6);
        assert_eq!(out[2], 9.0);
        assert_eq!(out[3], 9.0);
        assert_eq!(out[4], 0.0);
        assert_eq!(out[7], 0.0);
    }

    #[test]
    fn vertical_min_needs_the_ridge_on_neighbouring_rows_too() {
        // one column, 8 rows: a single tall row is erased by half = 1,
        // a three-row block keeps its middle row
        let col = [0.0, 9.0, 0.0, 0.0, 7.0, 7.0, 7.0, 0.0];
        let grid: Vec<f32> = col.iter().flat_map(|&v| [v; 8]).collect();
        let out = vertical_min(&grid, 8, 1);
        let column: Vec<f32> = (0..8).map(|y| out[y * 8]).collect();
        assert_eq!(column, [0.0, 0.0, 0.0, 0.0, 0.0, 7.0, 0.0, 0.0]);
        // half = 0 is the identity
        let same = vertical_min(&grid, 8, 0);
        assert_eq!(same, grid);
    }

    #[test]
    fn square_majority_keeps_a_harbour_plot_on_land() {
        let res = 64;
        let mut m = map(res);
        for y in 0..res {
            for x in 0..2 {
                let i = m.idx(x, y);
                m.land_mask[i] = 0;
            }
        }
        let fields = ClimateFields::new(&m);
        let mpc = m.config.meters_per_cell();
        let west_edge = -(res as f32) * 0.5 * mpc;
        // 4-cell square starting one cell into the sea: 3/4 land -> wet coast
        assert_eq!(
            fields.climate_of_square(&m, west_edge + mpc, 0.0, mpc * 4.0),
            Climate::WetCoast
        );
        assert_eq!(
            fields.climate_of_square(&m, west_edge, 0.0, mpc * 2.0),
            Climate::Sea
        );
    }

    #[test]
    fn zones_follow_the_rules() {
        // 512 cells * 64 m = 32 km wide; sea in the west column band.
        let res = 512;
        let mut m = map(res);
        for y in 0..res {
            for x in 0..40 {
                let i = m.idx(x, y);
                m.land_mask[i] = 0;
            }
        }
        // a 1,300 m ridge at x = 200 shadows the lowland east of it
        for y in 0..res {
            let i = m.idx(200, y);
            m.elevation_m[i] = 1300.0;
            let i = m.idx(300, y);
            m.elevation_m[i] = 1600.0;
        }
        let fields = ClimateFields::new(&m);
        assert_eq!(fields.climate_at_cell(&m, 10, 10), Climate::Sea);
        assert_eq!(fields.climate_at_cell(&m, 45, 10), Climate::WetCoast);
        assert_eq!(fields.climate_at_cell(&m, 100, 10), Climate::Temperate);
        assert_eq!(fields.climate_at_cell(&m, 190, 10), Climate::Temperate);
        assert_eq!(fields.climate_at_cell(&m, 210, 10), Climate::RainShadow);
        assert_eq!(fields.climate_at_cell(&m, 300, 10), Climate::Alpine);
        // the ridge itself is above the shadow ceiling
        assert_eq!(fields.climate_at_cell(&m, 200, 10), Climate::Temperate);
    }

    #[test]
    fn world_lookup_wraps_x_and_clamps_z() {
        let res = 64;
        let mut m = map(res);
        let i = m.idx(0, 0);
        m.land_mask[i] = 0;
        let fields = ClimateFields::new(&m);
        let mpc = m.config.meters_per_cell();
        let west_edge = -(res as f32) * 0.5 * mpc;
        let x_wrapped = west_edge - mpc;
        let z_below = west_edge - 100.0 * mpc;
        assert_eq!(
            fields.climate_at_world(&m, x_wrapped, z_below),
            Climate::WetCoast
        );
        assert_eq!(
            fields.climate_at_world(&m, west_edge, west_edge),
            Climate::Sea
        );
    }

    #[test]
    fn byte_round_trip() {
        for b in 0..5u8 {
            assert_eq!(Climate::try_from(b).unwrap() as u8, b);
        }
        assert_eq!(Climate::try_from(5), Err(5));
    }
}
