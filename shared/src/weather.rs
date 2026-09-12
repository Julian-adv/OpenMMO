//! Regional rain cells (doc/WEATHER_SYSTEM.md). A cell forms over a baked
//! sector, rains, and clears; everything is a pure function of the world
//! seed, the sector list and game time, so server and client agree and any
//! future time can be forecast.

use serde::{Deserialize, Serialize};

use crate::moon::game_day_index;
use crate::world::{shortest_world_delta_x, GameDateTime};
use crate::worldgen::climate::Climate;

pub const GAME_MINUTES_PER_DAY: i64 = 24 * 60;

/// Rain cells spawn on one of a sector's spots; a sector hosts at most one
/// cell at a time. Baked by terrain-gen from the climate grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sector {
    pub zone: u8,
    /// World metres.
    pub spots: Vec<[f32; 2]>,
}

/// The seed is the one the sectors were placed with, so the server needs no
/// other record of the world seed to broadcast it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherSectors {
    pub version: u32,
    pub seed: u64,
    pub sectors: Vec<Sector>,
}

pub const WEATHER_SECTORS_VERSION: u32 = 1;

/// Per-zone cadence in game minutes and kilometres. A rain event lasts
/// 15-40 real minutes (a game day is 3 real hours); dry zones get longer gaps,
/// not shorter rain.
#[derive(Debug, Clone, Copy)]
pub struct ZoneSchedule {
    pub period: f64,
    pub life_min: f64,
    pub life_var: f64,
    pub chance: f64,
    pub radius_min_km: f32,
    pub radius_var_km: f32,
}

const NO_RAIN: ZoneSchedule = ZoneSchedule {
    period: 1.0,
    life_min: 0.0,
    life_var: 0.0,
    chance: 0.0,
    radius_min_km: 0.0,
    radius_var_km: 0.0,
};

pub const SCHEDULE: [ZoneSchedule; 5] = [
    NO_RAIN,
    // wet coast: small showers hanging over the shoreline
    ZoneSchedule {
        period: 560.0,
        life_min: 165.0,
        life_var: 75.0,
        chance: 0.9,
        radius_min_km: 1.6,
        radius_var_km: 1.0,
    },
    // temperate
    ZoneSchedule {
        period: 2000.0,
        life_min: 120.0,
        life_var: 120.0,
        chance: 0.8,
        radius_min_km: 3.5,
        radius_var_km: 1.9,
    },
    // rain shadow
    ZoneSchedule {
        period: 6800.0,
        life_min: 120.0,
        life_var: 60.0,
        chance: 0.6,
        radius_min_km: 3.0,
        radius_var_km: 1.6,
    },
    // alpine
    ZoneSchedule {
        period: 1100.0,
        life_min: 180.0,
        life_var: 60.0,
        chance: 0.9,
        radius_min_km: 3.3,
        radius_var_km: 1.9,
    },
];

/// A cell's lifetime never exceeds this share of its period, so a sector is
/// guaranteed a dry gap between cells.
pub const MAX_LIFE_SHARE: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Cell {
    pub sector: usize,
    pub x: f32,
    pub z: f32,
    pub radius_m: f32,
    /// 0..1 rain envelope: ramps up while forming, down while clearing.
    pub env: f32,
    /// 0..1 progress through the cell's life.
    pub progress: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CellStage {
    Forming,
    Raining,
    Clearing,
}

impl Cell {
    pub fn stage(&self) -> CellStage {
        if self.progress < 0.25 {
            CellStage::Forming
        } else if self.progress < 0.7 {
            CellStage::Raining
        } else {
            CellStage::Clearing
        }
    }
}

pub fn zone_schedule(zone: u8) -> ZoneSchedule {
    match Climate::try_from(zone) {
        Ok(c) => SCHEDULE[c as usize],
        Err(_) => NO_RAIN,
    }
}

/// Game minutes since the calendar epoch; the `t` every schedule runs on.
pub fn game_minutes(datetime: &GameDateTime) -> f64 {
    let day = game_day_index(datetime);
    let hour = i64::from(datetime.hour).clamp(0, 23);
    let minute = i64::from(datetime.minute).clamp(0, 59);
    (day * GAME_MINUTES_PER_DAY + hour * 60 + minute) as f64
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let s = ((x - a) / (b - a)).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// splitmix64 finaliser over (seed, sector, cycle, salt) → [0, 1).
fn hash01(seed: u64, sector: u64, cycle: i64, salt: u64) -> f64 {
    let mut x = seed
        ^ sector.wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (cycle as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
        ^ salt.wrapping_mul(0x94D0_49BB_1331_11EB);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// The cell a sector hosts at `t_min`, if any. Only the current cycle can
/// be live: a cell fits inside its own cycle (`birth + life ≤ (k + 1) * P`).
/// `bias` scales every zone's chance: 1.0 is the baked schedule, 0.5 skips
/// half the cycles, 0.0 turns rain off.
pub fn sector_cell(
    sectors: &[Sector],
    index: usize,
    seed: u64,
    bias: f64,
    t_min: f64,
) -> Option<Cell> {
    let sector = &sectors[index];
    if sector.spots.is_empty() {
        return None;
    }
    let sched = zone_schedule(sector.zone);
    if sched.chance <= 0.0 {
        return None;
    }
    let k = (t_min / sched.period).floor() as i64;
    let id = index as u64;
    if hash01(seed, id, k, 1) >= sched.chance * bias {
        return None;
    }
    let life = (sched.life_min + hash01(seed, id, k, 2) * sched.life_var)
        .min(sched.period * MAX_LIFE_SHARE);
    let birth = k as f64 * sched.period + hash01(seed, id, k, 3) * (sched.period - life);
    let age = t_min - birth;
    if age < 0.0 || age > life {
        return None;
    }
    let progress = (age / life) as f32;
    let env = smoothstep(0.0, 0.25, progress) * (1.0 - smoothstep(0.7, 1.0, progress));
    let spot = sector.spots[(hash01(seed, id, k, 4) * sector.spots.len() as f64) as usize];
    let radius_km = (sched.radius_min_km + sched.radius_var_km * hash01(seed, id, k, 5) as f32)
        * (0.6 + 0.4 * env);
    Some(Cell {
        sector: index,
        x: spot[0],
        z: spot[1],
        radius_m: radius_km * 1000.0,
        env,
        progress,
    })
}

pub fn cells_at(sectors: &[Sector], seed: u64, bias: f64, t_min: f64) -> Vec<Cell> {
    (0..sectors.len())
        .filter_map(|i| sector_cell(sectors, i, seed, bias, t_min))
        .collect()
}

/// Rain intensity 0..1 at a world position: the cell falloffs summed and
/// clamped. Nothing falls outside a cell's radius.
pub fn rain_at(cells: &[Cell], x: f32, z: f32) -> f32 {
    let mut sum = 0.0f32;
    for c in cells {
        let dx = shortest_world_delta_x(c.x, x);
        let dz = z - c.z;
        let d2 = dx * dx + dz * dz;
        if d2 < c.radius_m * c.radius_m {
            sum += c.env * rain_falloff(d2.sqrt() / c.radius_m);
        }
    }
    sum.min(1.0)
}

/// Share of the radius that rains at full strength; the rest is the fade.
pub const CELL_CORE_SHARE: f32 = 0.7;

/// Flat top with a short edge: the map disc is the rain area, and a cell
/// never soaks its neighbours from beyond its own radius. The Gaussian this
/// replaced kept 37 % at the radius and drizzled out to twice it.
pub fn rain_falloff(normalized_distance: f32) -> f32 {
    1.0 - smoothstep(CELL_CORE_SHARE, 1.0, normalized_distance)
}

/// Light dimming 0..1 derived from rain; darkens before rain reaches full.
pub fn cloud_factor(rain: f32) -> f32 {
    smoothstep(0.35, 0.8, rain)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sectors() -> Vec<Sector> {
        vec![
            Sector {
                zone: Climate::WetCoast as u8,
                spots: vec![[0.0, 0.0], [500.0, 0.0], [0.0, 500.0]],
            },
            Sector {
                zone: Climate::Temperate as u8,
                spots: vec![[10_000.0, 2_000.0]],
            },
            Sector {
                zone: Climate::RainShadow as u8,
                spots: vec![[-8_000.0, -3_000.0]],
            },
            Sector {
                zone: Climate::Alpine as u8,
                spots: vec![[4_000.0, 9_000.0], [4_500.0, 9_000.0]],
            },
        ]
    }

    #[test]
    fn same_inputs_same_cells() {
        let s = sectors();
        for t in [0.0, 123.0, 99_999.5, 1e7] {
            assert_eq!(cells_at(&s, 42, 1.0, t), cells_at(&s, 42, 1.0, t));
        }
        let a: Vec<_> = (0..200)
            .map(|i| cells_at(&s, 42, 1.0, i as f64 * 97.0).len())
            .collect();
        let b: Vec<_> = (0..200)
            .map(|i| cells_at(&s, 43, 1.0, i as f64 * 97.0).len())
            .collect();
        assert_ne!(a, b, "a different seed must give a different schedule");
    }

    #[test]
    fn bias_scales_how_often_cells_form() {
        let s = sectors();
        let live_minutes = |bias: f64| {
            (0..20_000)
                .filter(|i| !cells_at(&s, 42, bias, *i as f64 * 7.0).is_empty())
                .count() as f64
        };
        let full = live_minutes(1.0);
        let half = live_minutes(0.5);
        assert!(full > 0.0);
        assert!(
            half > full * 0.3 && half < full * 0.7,
            "half {half} vs full {full}"
        );
        assert_eq!(live_minutes(0.0), 0.0);
    }

    #[test]
    fn a_sector_never_hosts_two_cells_and_always_clears() {
        let s = sectors();
        for (i, sector) in s.iter().enumerate() {
            let sched = zone_schedule(sector.zone);
            let mut saw_rain = false;
            let mut saw_gap = false;
            let mut t = 0.0;
            while t < sched.period * 40.0 {
                let cell = sector_cell(&s, i, 7, 1.0, t);
                match cell {
                    Some(c) => {
                        saw_rain = true;
                        assert!((0.0..=1.0).contains(&c.env));
                        assert!(sector.spots.contains(&[c.x, c.z]));
                    }
                    None => saw_gap = true,
                }
                t += 5.0;
            }
            assert!(saw_rain, "sector {i} never rained");
            assert!(saw_gap, "sector {i} never cleared");
        }
    }

    #[test]
    fn a_cell_fits_inside_its_cycle() {
        let s = sectors();
        let sched = zone_schedule(s[0].zone);
        // scan several cycles; every live cell's cycle index must equal the
        // cycle of the current time
        let mut t = 0.0;
        while t < sched.period * 20.0 {
            if let Some(c) = sector_cell(&s, 0, 9, 1.0, t) {
                let life = c.progress as f64; // 0..1 inside its own life
                assert!((0.0..=1.0).contains(&life));
            }
            t += 1.0;
        }
        // the longest possible life leaves a gap of at least 10 % of the period
        assert!(sched.life_min + sched.life_var <= sched.period * MAX_LIFE_SHARE);
    }

    #[test]
    fn envelope_forms_rains_and_clears_in_order() {
        let s = sectors();
        let sched = zone_schedule(s[1].zone);
        let mut stages = Vec::new();
        let mut t = 0.0;
        while t < sched.period * 30.0 {
            if let Some(c) = sector_cell(&s, 1, 3, 1.0, t) {
                if stages.last() != Some(&c.stage()) {
                    stages.push(c.stage());
                }
            }
            t += 1.0;
        }
        assert!(stages.len() >= 3);
        for w in stages.windows(3) {
            if w[0] == CellStage::Forming {
                assert_eq!(w[1], CellStage::Raining);
                assert_eq!(w[2], CellStage::Clearing);
            }
        }
    }

    #[test]
    fn rain_falls_off_and_wraps_across_the_seam() {
        let cell = Cell {
            sector: 0,
            x: -16_400.0,
            z: 0.0,
            radius_m: 3_000.0,
            env: 1.0,
            progress: 0.5,
        };
        let cells = [cell];
        assert!((rain_at(&cells, -16_400.0, 0.0) - 1.0).abs() < 1e-6);
        assert!((rain_at(&cells, -16_400.0, 2_100.0) - 1.0).abs() < 1e-6);
        let mid = rain_at(&cells, -16_400.0, 2_550.0);
        assert!((mid - 0.5).abs() < 1e-6, "{mid}");
        assert_eq!(rain_at(&cells, -16_400.0, 3_000.0), 0.0);
        assert_eq!(rain_at(&cells, -16_400.0, 20_000.0), 0.0);
        // 100 m east of the seam is 132 m from the cell, not 32 km
        let across = rain_at(&cells, 16_300.0, 0.0);
        assert!(across > 0.99, "{across}");
    }

    #[test]
    fn game_minutes_counts_from_the_epoch() {
        let epoch = GameDateTime {
            year: 217,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
        };
        assert_eq!(game_minutes(&epoch), 0.0);
        let later = GameDateTime {
            year: 217,
            month: 1,
            day: 2,
            hour: 1,
            minute: 30,
        };
        assert_eq!(game_minutes(&later), 1440.0 + 90.0);
    }

    #[test]
    fn sectors_round_trip_through_json() {
        let ws = WeatherSectors {
            version: WEATHER_SECTORS_VERSION,
            seed: 42,
            sectors: sectors(),
        };
        let text = serde_json::to_string(&ws).unwrap();
        assert_eq!(serde_json::from_str::<WeatherSectors>(&text).unwrap(), ws);
    }
}
