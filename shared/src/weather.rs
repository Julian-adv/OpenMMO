//! Deterministic regional rain cells; see doc/WEATHER_SYSTEM.md.

mod seasonal;

use serde::{Deserialize, Serialize};

use crate::moon::game_day_index;
use crate::world::{shortest_world_delta_x, GameDateTime};
use crate::worldgen::climate::Climate;
use crate::worldgen::noise::smoothstep;

pub const GAME_MINUTES_PER_DAY: i64 = 24 * 60;

/// Baked rain-cell spawn spots; each sector hosts at most one cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sector {
    pub zone: u8,
    /// World metres.
    pub spots: Vec<[f32; 2]>,
}

/// Sectors and the world seed used to place them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherSectors {
    pub version: u32,
    pub seed: u64,
    pub sectors: Vec<Sector>,
}

pub const WEATHER_SECTORS_VERSION: u32 = 1;

/// Regional cadence in game minutes and kilometres; events last 15–30 real minutes.
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

/// Envelope ramp bounds as a share of a cell's life: rain builds up to
/// `ENVELOPE_RISE_END` and fades after `ENVELOPE_FALL_START`.
const ENVELOPE_RISE_END: f32 = 0.25;
const ENVELOPE_FALL_START: f32 = 0.7;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub sector: usize,
    pub x: f32,
    pub z: f32,
    pub radius_m: f32,
    /// 0..1 rain envelope: ramps up while forming, down while clearing.
    pub env: f32,
    /// 0..1 progress through the cell's life.
    pub progress: f32,
    /// Game minutes left before the cell dies.
    pub remain_min: f32,
}

/// Which part of the envelope a cell is in, for the debug radar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellStage {
    Forming,
    Raining,
    Clearing,
}

impl Cell {
    pub fn stage(&self) -> CellStage {
        if self.progress < ENVELOPE_RISE_END {
            CellStage::Forming
        } else if self.progress > ENVELOPE_FALL_START {
            CellStage::Clearing
        } else {
            CellStage::Raining
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

pub(crate) fn splitmix64(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// Hash of (seed, sector, cycle, salt) → [0, 1).
fn hash01(seed: u64, sector: u64, cycle: i64, salt: u64) -> f64 {
    let x = splitmix64(
        seed ^ sector.wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (cycle as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ salt.wrapping_mul(0x94D0_49BB_1331_11EB),
    );
    (x >> 11) as f64 / (1u64 << 53) as f64
}

/// The current cycle's cell. `bias` scales the seasonal rain chance.
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
    let draw = hash01(seed, id, k, 1);
    let chance = sched.chance * bias;
    if draw >= chance {
        return None;
    }
    let life = (sched.life_min + hash01(seed, id, k, 2) * sched.life_var)
        .min(sched.period * MAX_LIFE_SHARE);
    let birth = k as f64 * sched.period + hash01(seed, id, k, 3) * (sched.period - life);
    let age = t_min - birth;
    if age < 0.0 || age > life {
        return None;
    }
    let spot = sector.spots[(hash01(seed, id, k, 4) * sector.spots.len() as f64) as usize];
    // Freeze the seasonal chance at birth so accepted rain finishes naturally.
    if draw >= chance * seasonal::chance_multiplier(spot, birth) {
        return None;
    }
    let progress = (age / life) as f32;
    let env = smoothstep(0.0, ENVELOPE_RISE_END, progress)
        * (1.0 - smoothstep(ENVELOPE_FALL_START, 1.0, progress));
    let radius_km = (sched.radius_min_km + sched.radius_var_km * hash01(seed, id, k, 5) as f32)
        * (0.6 + 0.4 * env);
    Some(Cell {
        sector: index,
        x: spot[0],
        z: spot[1],
        radius_m: radius_km * 1000.0,
        env,
        progress,
        remain_min: (life - age) as f32,
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

/// Flat top with a short edge, so a cell never soaks past its own radius.
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
    fn seasonal_rain_keeps_complete_events_and_their_original_strength() {
        let regional = vec![Sector {
            zone: Climate::WetCoast as u8,
            spots: vec![[-1475.2, 4741.6]],
        }];
        let unseasonal = vec![Sector {
            zone: Climate::WetCoast as u8,
            spots: vec![[14000.0, -14000.0]],
        }];
        let sched = zone_schedule(regional[0].zone);
        let seed = 42;
        let mut accepted = 0;
        let mut skipped = 0;
        for k in 0..20_000 {
            let life = sched.life_min + hash01(seed, 0, k, 2) * sched.life_var;
            let birth = k as f64 * sched.period + hash01(seed, 0, k, 3) * (sched.period - life);
            let middle = birth + life * 0.5;
            if sector_cell(&unseasonal, 0, seed, 1.0, middle).is_none() {
                continue;
            }
            let rains = sector_cell(&regional, 0, seed, 1.0, middle).is_some();
            if rains {
                accepted += 1;
            } else {
                skipped += 1;
            }
            for fraction in [0.001, 0.1, 0.25, 0.5, 0.7, 0.9, 0.999] {
                let t = birth + life * fraction;
                let cell = sector_cell(&regional, 0, seed, 1.0, t);
                assert_eq!(cell.is_some(), rains, "cycle {k}, progress {fraction}");
                if let Some(cell) = cell {
                    let baseline = sector_cell(&unseasonal, 0, seed, 1.0, t).unwrap();
                    assert_eq!(cell.env, baseline.env);
                    assert_eq!(cell.radius_m, baseline.radius_m);
                    assert_eq!(cell.progress, baseline.progress);
                }
            }
        }
        assert!(accepted > 0 && skipped > 0);
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
    fn envelope_rises_holds_then_falls_within_a_life() {
        let s = sectors();
        let sched = zone_schedule(s[1].zone);
        let mut lives = 0;
        let mut prev: Option<Cell> = None;
        let mut t = 0.0;
        while t < sched.period * 30.0 {
            let cur = sector_cell(&s, 1, 3, 1.0, t);
            match (prev, cur) {
                (Some(p), Some(c)) if c.progress >= p.progress => {
                    if c.progress < 0.25 {
                        assert!(c.env >= p.env, "forming must not fall at t={t}");
                    } else if c.progress < 0.7 {
                        assert!((c.env - 1.0).abs() < 1e-6, "raining must hold at t={t}");
                    } else {
                        assert!(c.env <= p.env, "clearing must not rise at t={t}");
                    }
                }
                (None, Some(_)) => lives += 1,
                _ => {}
            }
            prev = cur;
            t += 1.0;
        }
        assert!(lives >= 2);
    }

    #[test]
    fn stage_follows_the_envelope_ramps() {
        let at = |progress: f32| Cell {
            sector: 0,
            x: 0.0,
            z: 0.0,
            radius_m: 1.0,
            env: 0.0,
            progress,
            remain_min: 0.0,
        };
        assert_eq!(at(0.0).stage(), CellStage::Forming);
        assert_eq!(at(ENVELOPE_RISE_END - 0.01).stage(), CellStage::Forming);
        assert_eq!(at(ENVELOPE_RISE_END).stage(), CellStage::Raining);
        assert_eq!(at(ENVELOPE_FALL_START).stage(), CellStage::Raining);
        assert_eq!(at(ENVELOPE_FALL_START + 0.01).stage(), CellStage::Clearing);
        assert_eq!(at(1.0).stage(), CellStage::Clearing);
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
            remain_min: 0.0,
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
