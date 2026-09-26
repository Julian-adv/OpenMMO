//! Which cells fall as snow, and how long snow lies on the ground.

use crate::worldgen::noise::smoothstep;

use super::{
    precip_at, seasonal, sector_event, zone_schedule, Event, Sector, GAME_MINUTES_PER_DAY,
};

/// Midwinter snow share north of Aldermark (z 4,742) and on the southern
/// lowlands, which still see winter rain about half the time.
const NORTH_SNOW_SHARE: f64 = 0.95;
const SOUTH_SNOW_SHARE: f64 = 0.5;
const SNOW_SHARE_Z: (f32, f32) = (5_500.0, 8_500.0);
/// Sectors this high snow whenever it is winter, whatever the latitude.
const HIGH_SNOW_ELEVATION_M: (f32, f32) = (600.0, 1_200.0);

/// Game minutes of full snowfall that cover the ground completely.
const SNOW_FILL_MIN: f64 = 90.0;
/// Game minutes a full cover takes to melt, from the mildest to the coldest
/// winter spot.
const MELT_MIN: (f64, f64) = (180.0, 1_000.0);
/// Full rain alone clears a full cover in this many game minutes.
const RAIN_MELT_MIN: f64 = 150.0;
const STEP_MIN: f64 = 5.0;
/// Melt rate at night, rising by `SUN_MELT` at noon.
const NIGHT_MELT: f64 = 0.6;
const SUN_MELT: f64 = 0.9;
/// Snow older than this has melted even at the coldest spot on the coldest
/// nights.
const LOOKBACK_MIN: f64 = MELT_MIN.1 / NIGHT_MELT + 300.0;

fn latitude_share(z: f32) -> f64 {
    let south = f64::from(smoothstep(SNOW_SHARE_Z.0, SNOW_SHARE_Z.1, z));
    NORTH_SNOW_SHARE + (SOUTH_SNOW_SHARE - NORTH_SNOW_SHARE) * south
}

/// Chance a cell born at `t_min` over latitude `z` falls as snow.
pub(super) fn snow_chance(t_min: f64, z: f32, elevation_m: u16) -> f64 {
    let high = f64::from(smoothstep(
        HIGH_SNOW_ELEVATION_M.0,
        HIGH_SNOW_ELEVATION_M.1,
        f32::from(elevation_m),
    ));
    seasonal::winter_depth(t_min) * latitude_share(z).max(high)
}

/// 0..1: how cold a spot is, which sets how slowly its snow melts.
fn chill(t_min: f64, z: f32) -> f64 {
    seasonal::winter_depth(t_min) * latitude_share(z)
}

fn melt_per_min(t_min: f64, z: f32) -> f64 {
    let hour = t_min.rem_euclid(GAME_MINUTES_PER_DAY as f64) / 60.0;
    let sun = (std::f64::consts::PI * (hour - 6.0) / 12.0).sin().max(0.0);
    (NIGHT_MELT + SUN_MELT * sun) / (MELT_MIN.0 + (MELT_MIN.1 - MELT_MIN.0) * chill(t_min, z))
}

/// 0..1 snow lying on the ground at `(x, z)`: past snow cells integrated with
/// melt, so it needs no history and matches for every client.
pub fn snow_cover_at(sectors: &[Sector], seed: u64, bias: f64, x: f32, z: f32, t_min: f64) -> f32 {
    let from = t_min - LOOKBACK_MIN;
    let mut events: Vec<Event> = Vec::new();
    let mut start = f64::INFINITY;
    for index in 0..sectors.len() {
        let sched = zone_schedule(sectors[index].zone);
        if sched.chance <= 0.0 {
            continue;
        }
        let first = (from / sched.period).floor() as i64;
        let last = (t_min / sched.period).floor() as i64;
        for k in first..=last {
            let Some(event) = sector_event(sectors, index, seed, bias, k, |birth, life| {
                birth + life >= from && birth <= t_min
            }) else {
                continue;
            };
            if !event.reaches(x, z) {
                continue;
            }
            if event.snow {
                start = start.min(event.birth);
            }
            events.push(event);
        }
    }
    if !start.is_finite() {
        return 0.0;
    }

    let mut cover = 0.0f64;
    let mut t = start.max(from);
    while t < t_min {
        let dt = STEP_MIN.min(t_min - t);
        let mid = t + dt * 0.5;
        let precip = precip_at(events.iter().filter_map(|e| e.cell_at(mid)), x, z);
        cover = (cover + f64::from(precip.snow) * dt / SNOW_FILL_MIN).min(1.0);
        let melt = melt_per_min(mid, z) + f64::from(precip.rain) / RAIN_MELT_MIN;
        cover = (cover - melt * dt).max(0.0);
        t += dt;
    }
    cover as f32
}

#[cfg(test)]
mod tests {
    use super::super::{cells_at, precip_at, rain_at};
    use super::*;
    use crate::worldgen::climate::Climate;

    const DAY: f64 = GAME_MINUTES_PER_DAY as f64;

    fn sectors(z: f32) -> Vec<Sector> {
        vec![Sector {
            zone: Climate::WetCoast as u8,
            spots: vec![[0.0, z]],
            ..Default::default()
        }]
    }

    #[test]
    fn midwinter_snows_in_the_north_rains_in_the_south_and_summer_never_snows() {
        // Midwinter, repeated across years so enough cells form.
        let midwinter = |z: f32| {
            let s = sectors(z);
            let (mut all, mut snowy) = (0, 0);
            for year in 0..40 {
                let mut t = (year as f64 * 360.0 + 20.0) * DAY;
                while t < (year as f64 * 360.0 + 70.0) * DAY {
                    if let Some(c) = super::super::sector_cell(&s, 0, 42, 1.0, t) {
                        all += 1;
                        snowy += usize::from(c.snow);
                    }
                    t += 60.0;
                }
            }
            snowy as f64 / all as f64
        };
        let north = midwinter(4_700.0);
        let south = midwinter(10_000.0);
        assert!(north > 0.85, "north {north}");
        assert!((0.35..0.65).contains(&south), "south {south}");
        let summer_cells = {
            let s = sectors(4_700.0);
            (0..(40.0 * DAY / 30.0) as i64)
                .filter_map(|i| {
                    super::super::sector_cell(&s, 0, 42, 1.0, 190.0 * DAY + i as f64 * 30.0)
                })
                .filter(|c| c.snow)
                .count()
        };
        assert_eq!(summer_cells, 0);
    }

    #[test]
    fn snow_at_is_the_snow_part_of_rain_at() {
        let s = sectors(4_700.0);
        let mut saw_snow = false;
        let mut t = 20.0 * DAY;
        while t < 80.0 * DAY {
            let cells = cells_at(&s, 42, 1.0, t);
            let (x, z) = (0.0, 4_700.0);
            let snow = precip_at(&cells, x, z).snow;
            assert!(snow <= rain_at(&cells, x, z) + 1e-6);
            saw_snow |= snow > 0.5;
            t += 30.0;
        }
        assert!(saw_snow);
    }

    #[test]
    fn snow_builds_under_snowfall_then_melts_away() {
        let s = sectors(4_700.0);
        let (x, z) = (0.0, 4_700.0);
        let mut peak = 0.0f32;
        let mut prev = 0.0f32;
        let mut rose_while_snowing = false;
        let mut t = 20.0 * DAY;
        while t < 60.0 * DAY {
            let cover = snow_cover_at(&s, 42, 1.0, x, z, t);
            assert!((0.0..=1.0).contains(&cover));
            let snowing = precip_at(cells_at(&s, 42, 1.0, t), x, z).snow > 0.5;
            if snowing && cover > prev + 1e-4 {
                rose_while_snowing = true;
            }
            if !snowing && rain_at(&cells_at(&s, 42, 1.0, t), x, z) == 0.0 {
                assert!(cover <= prev + 1e-4, "cover rose without snow at t={t}");
            }
            peak = peak.max(cover);
            prev = cover;
            t += 20.0;
        }
        assert!(rose_while_snowing);
        assert!(peak > 0.9, "peak {peak}");
        assert_eq!(snow_cover_at(&s, 42, 1.0, x, z, 200.0 * DAY), 0.0);
        assert_eq!(snow_cover_at(&s, 42, 1.0, 9_000.0, z, 40.0 * DAY), 0.0);
    }

    #[test]
    fn snow_cover_is_continuous_in_time() {
        let s = sectors(4_700.0);
        let mut prev = snow_cover_at(&s, 42, 1.0, 0.0, 4_700.0, 20.0 * DAY);
        let mut t = 20.0 * DAY;
        while t < 40.0 * DAY {
            t += 1.0;
            let cover = snow_cover_at(&s, 42, 1.0, 0.0, 4_700.0, t);
            assert!(
                (cover - prev).abs() < 0.03,
                "jump {prev} -> {cover} at t={t}"
            );
            prev = cover;
        }
    }
}
