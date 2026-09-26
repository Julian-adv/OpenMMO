//! Climate zones for the weather system (doc/WEATHER_SYSTEM.md). One byte per
//! 32 m land plot, derived from the baked elevation and land mask so no zone
//! is hand-authored.

use super::grid::bfs_distance_from;
use super::GlobalMap;
use crate::weather::heading_dir;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Climate {
    Sea = 0,
    WetCoast = 1,
    Temperate = 2,
    Alpine = 3,
}

impl TryFrom<u8> for Climate {
    type Error = u8;

    fn try_from(byte: u8) -> Result<Self, u8> {
        match byte {
            0 => Ok(Self::Sea),
            1 => Ok(Self::WetCoast),
            2 => Ok(Self::Temperate),
            3 => Ok(Self::Alpine),
            other => Err(other),
        }
    }
}

pub const COAST_BAND_M: f32 = 1000.0;
pub const COAST_MAX_ELEVATION_M: f32 = 900.0;
/// How far upwind a sector looks for a ridge, and the across-wind span a
/// ridge needs to block: a single spur casts no shadow.
pub const LEE_LOOKBACK_M: f32 = 14_000.0;
pub const LEE_RIDGE_SPAN_M: f32 = 1000.0;
pub const LEE_DIRECTIONS: usize = 16;
/// Open water this wide ends an upwind scan; inlets and lagoons do not.
pub const LEE_SEA_RESET_M: f32 = 1000.0;
/// Below the generator's permanent-snow line (1,800 m) so the zone covers the
/// whole summit block, not just the painted snow.
pub const ALPINE_M: f32 = 1500.0;

/// Whole-world fields the zone rules sample. Built once per bake.
pub struct ClimateFields {
    res: usize,
    coast_dist: Vec<u16>,
}

impl ClimateFields {
    pub fn new(map: &GlobalMap) -> Self {
        let res = map.config.global_res as usize;
        Self {
            res,
            coast_dist: bfs_distance_from(&map.land_mask, res, 0, None),
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
        Climate::Temperate
    }

    /// X wraps around the cylinder; Z is clamped to the map.
    pub fn climate_at_world(&self, map: &GlobalMap, x_m: f32, z_m: f32) -> Climate {
        let (x, y) = cell_of(map, x_m, z_m);
        self.climate_at_cell(map, x, y)
    }

    /// Majority zone over the square with min corner (`x_m`, `z_m`); a coastal
    /// plot counts as land unless most of it is under water.
    pub fn climate_of_square(&self, map: &GlobalMap, x_m: f32, z_m: f32, size_m: f32) -> Climate {
        let mpc = map.config.meters_per_cell();
        let n = (size_m / mpc).round().max(1.0) as u32;
        let (x0, y0) = cell_of(map, x_m + mpc * 0.5, z_m + mpc * 0.5);
        let mut counts = [0u32; 4];
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
}

/// Highest land elevation within `LEE_LOOKBACK_M` of (`x_m`, `z_m`) in each
/// of `LEE_DIRECTIONS` headings, counter-clockwise from east (north = -Z).
/// `LEE_SEA_RESET_M` of sea on the centre ray ends the scan: moisture picked
/// up over open water is not blocked by a ridge further out. Each heading
/// takes the lowest of three parallel rays `LEE_RIDGE_SPAN_M` wide over that
/// distance.
pub fn upwind_ridges(map: &GlobalMap, x_m: f32, z_m: f32) -> Vec<u16> {
    let step = map.config.meters_per_cell().max(32.0);
    let steps = (LEE_LOOKBACK_M / step) as usize;
    let half = LEE_RIDGE_SPAN_M * 0.5;
    let sea_steps = (LEE_SEA_RESET_M / step).ceil() as usize;
    (0..LEE_DIRECTIONS)
        .map(|d| {
            let (dx, dz) = heading_dir(d as f32 * std::f32::consts::TAU / LEE_DIRECTIONS as f32);
            let at = |off: f32, k: usize| {
                let r = step * k as f32;
                land_elevation(map, x_m - dz * off + dx * r, z_m + dx * off + dz * r)
            };
            let mut reach = steps;
            let mut sea_run = 0;
            for k in 1..=steps {
                sea_run = if at(0.0, k).is_some() { 0 } else { sea_run + 1 };
                if sea_run >= sea_steps {
                    reach = k - sea_run;
                    break;
                }
            }
            let ridge = [-half, 0.0, half]
                .into_iter()
                .map(|off| (1..=reach).filter_map(|k| at(off, k)).fold(0.0, f32::max))
                .fold(f32::MAX, f32::min);
            metres_u16(ridge)
        })
        .collect()
}

/// Land elevation at a world position, 0 over sea.
pub fn elevation_at(map: &GlobalMap, x_m: f32, z_m: f32) -> u16 {
    metres_u16(land_elevation(map, x_m, z_m).unwrap_or(0.0))
}

fn metres_u16(m: f32) -> u16 {
    m.round().clamp(0.0, u16::MAX as f32) as u16
}

fn land_elevation(map: &GlobalMap, x_m: f32, z_m: f32) -> Option<f32> {
    let (x, y) = cell_of(map, x_m, z_m);
    let i = map.idx(x, y);
    (map.land_mask[i] != 0).then_some(map.elevation_m[i])
}

/// X wraps around the cylinder; Z is clamped to the map.
fn cell_of(map: &GlobalMap, x_m: f32, z_m: f32) -> (u32, u32) {
    let (cx, cy) = map.config.world_m_to_cell(x_m, z_m);
    let res = map.config.global_res as i64;
    let x = (cx.floor() as i64).rem_euclid(res) as u32;
    let y = (cy.floor() as i64).clamp(0, res - 1) as u32;
    (x, y)
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
    fn upwind_ridges_need_span_and_stop_at_the_sea() {
        // 512 cells * 64 m; a 1,300 m north-south ridge 3.2 km east of x = 256.
        let res = 512;
        let mut m = map(res);
        for y in 0..res {
            let i = m.idx(306, y);
            m.elevation_m[i] = 1300.0;
        }
        // a lone 2,000 m peak 3.2 km north is a spur, not a ridge
        let i = m.idx(256, 206);
        m.elevation_m[i] = 2000.0;
        let ridges = upwind_ridges(&m, 32.0, 32.0);
        assert_eq!(ridges.len(), LEE_DIRECTIONS);
        assert_eq!(ridges[0], 1300, "east");
        assert_eq!(ridges[4], 0, "north");
        assert_eq!(ridges[8], 0, "west");
        // a bay beside the start blocks only a side ray, not the scan
        for y in 246..=250 {
            for x in 250..270 {
                let i = m.idx(x, y);
                m.land_mask[i] = 0;
            }
        }
        assert_eq!(upwind_ridges(&m, 32.0, 32.0)[0], 1300);
        // a narrow channel does not reset the scan, open sea does
        for y in 0..res {
            let i = m.idx(280, y);
            m.land_mask[i] = 0;
        }
        assert_eq!(upwind_ridges(&m, 32.0, 32.0)[0], 1300);
        for y in 0..res {
            for x in 280..=296 {
                let i = m.idx(x, y);
                m.land_mask[i] = 0;
            }
        }
        assert_eq!(upwind_ridges(&m, 32.0, 32.0)[0], 0);
        assert_eq!(elevation_at(&m, 24.0 * 64.0 + 32.0, 32.0), 0);
    }

    #[test]
    fn a_baked_ridge_shadows_cells_blowing_over_it() {
        let res = 512;
        let mut m = map(res);
        for y in 0..res {
            let i = m.idx(306, y);
            m.elevation_m[i] = 1600.0;
        }
        let sector = crate::weather::Sector {
            upwind_ridge_m: upwind_ridges(&m, 32.0, 32.0),
            ..Default::default()
        };
        let west = std::f32::consts::PI;
        assert!(crate::weather::lee(&sector, west) > 0.99);
        assert_eq!(crate::weather::lee(&sector, 0.0), 0.0);
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
        for y in 0..res {
            let i = m.idx(300, y);
            m.elevation_m[i] = 1600.0;
        }
        let fields = ClimateFields::new(&m);
        assert_eq!(fields.climate_at_cell(&m, 10, 10), Climate::Sea);
        assert_eq!(fields.climate_at_cell(&m, 45, 10), Climate::WetCoast);
        assert_eq!(fields.climate_at_cell(&m, 100, 10), Climate::Temperate);
        assert_eq!(fields.climate_at_cell(&m, 190, 10), Climate::Temperate);
        assert_eq!(fields.climate_at_cell(&m, 300, 10), Climate::Alpine);
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
        for b in 0..4u8 {
            assert_eq!(Climate::try_from(b).unwrap() as u8, b);
        }
        assert_eq!(Climate::try_from(4), Err(4));
        assert_eq!(Climate::try_from(5), Err(5));
    }
}
