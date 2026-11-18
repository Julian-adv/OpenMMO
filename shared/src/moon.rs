//! Eldor and Serin. Mirrors `getMoonPhaseState` in client
//! `celestialSimulation.ts`. Serin's dark evening hosts the merchants' price
//! meeting (doc/PRICING.md); a full moon in the sky opens the Terraclash
//! altar (doc/TIER5_FIELD_RAID.md).

use crate::world::GameDateTime;

pub const GAME_START_YEAR: i64 = 217;
pub const GAME_DAYS_PER_MONTH: i64 = 30;
pub const GAME_MONTHS_PER_YEAR: i64 = 12;
pub const SERIN_PERIOD_DAYS: i64 = 20;
const SERIN_PHASE_OFFSET_DAYS: i64 = 5;
const ELDOR_PERIOD_DAYS: i64 = 30;
const ELDOR_PHASE_OFFSET_DAYS: i64 = 0;
/// The client's "Full" phase label threshold.
const FULL_ILLUMINATION: f64 = 0.95;

/// Days since the calendar epoch (year 217, month 1, day 1).
pub fn game_day_index(datetime: &GameDateTime) -> i64 {
    let year = i64::from(datetime.year).max(GAME_START_YEAR);
    let month = i64::from(datetime.month).clamp(1, GAME_MONTHS_PER_YEAR);
    let day = i64::from(datetime.day).clamp(1, GAME_DAYS_PER_MONTH);
    (year - GAME_START_YEAR) * GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR
        + (month - 1) * GAME_DAYS_PER_MONTH
        + (day - 1)
}

/// The game day on which Serin stays dark through the evening.
pub fn is_serin_dark_day(game_day: i64) -> bool {
    (game_day + SERIN_PHASE_OFFSET_DAYS).rem_euclid(SERIN_PERIOD_DAYS) == SERIN_PERIOD_DAYS - 1
}

/// Game days until the next dark day (0 = today).
pub fn days_until_serin_dark(game_day: i64) -> i64 {
    (SERIN_PERIOD_DAYS - 1 - (game_day + SERIN_PHASE_OFFSET_DAYS).rem_euclid(SERIN_PERIOD_DAYS))
        .rem_euclid(SERIN_PERIOD_DAYS)
}

/// Illumination (0–1) and whether the moon is above the horizon.
fn moon_phase(period_days: i64, phase_offset_days: i64, game_day: i64, hour: f64) -> (f64, bool) {
    let period = period_days as f64;
    let cycle_day =
        (game_day as f64 + hour / 24.0 + phase_offset_days as f64).rem_euclid(period) + 1.0;
    let full = period / 2.0;
    let waxing = cycle_day <= full;
    let (illumination, orbital) = if waxing {
        let t = (cycle_day - 1.0) / (full - 1.0).max(1.0);
        (t, t * 0.5)
    } else {
        let t = (cycle_day - full) / (period - full).max(1.0);
        (1.0 - t, 0.5 + t * 0.5)
    };
    let transit = (12.0 + orbital * 24.0).rem_euclid(24.0);
    let rise = (transit - 6.0).rem_euclid(24.0);
    let above_horizon = (hour - rise).rem_euclid(24.0) <= 12.0;
    (illumination.clamp(0.0, 1.0), above_horizon)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moon {
    Eldor,
    Serin,
}

impl Moon {
    const ALL: [Self; 2] = [Self::Eldor, Self::Serin];

    fn phase(self, datetime: &GameDateTime) -> (f64, bool) {
        let (period, offset) = match self {
            Self::Eldor => (ELDOR_PERIOD_DAYS, ELDOR_PHASE_OFFSET_DAYS),
            Self::Serin => (SERIN_PERIOD_DAYS, SERIN_PHASE_OFFSET_DAYS),
        };
        let hour = crate::celestial::hour_of_day(datetime);
        moon_phase(period, offset, game_day_index(datetime), hour)
    }

    /// Above the horizon; the sky widget hides the disc the moment this ends.
    pub fn is_up(self, datetime: &GameDateTime) -> bool {
        self.phase(datetime).1
    }
}

/// The moons full and above the horizon.
pub fn full_moons_up(datetime: &GameDateTime) -> impl Iterator<Item = Moon> + '_ {
    Moon::ALL.into_iter().filter(|m| {
        let (illumination, up) = m.phase(datetime);
        illumination >= FULL_ILLUMINATION && up
    })
}

fn is_full_moon_up(datetime: &GameDateTime) -> bool {
    full_moons_up(datetime).next().is_some()
}

/// A full moon the player can see: in the sky after sunset. A summer full
/// moon rises before the sun sets, still hidden in the daylit sky.
pub fn is_full_moon_night(datetime: &GameDateTime) -> bool {
    crate::celestial::is_night(datetime) && is_full_moon_up(datetime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn serin_illumination(game_day: i64, hour: f64) -> f64 {
        let period = SERIN_PERIOD_DAYS as f64;
        let cycle_day = (game_day + SERIN_PHASE_OFFSET_DAYS).rem_euclid(SERIN_PERIOD_DAYS) as f64
            + hour / 24.0
            + 1.0;
        let full = period / 2.0;
        let raw = if cycle_day <= full {
            (cycle_day - 1.0) / (full - 1.0)
        } else {
            1.0 - (cycle_day - full) / (period - full)
        };
        raw.clamp(0.0, 1.0)
    }

    #[test]
    fn dark_day_is_the_evening_with_no_serin_light() {
        let dark = (0..40)
            .filter(|d| is_serin_dark_day(*d))
            .collect::<Vec<_>>();
        assert_eq!(dark, vec![14, 34]);
        // Day 13's evening is a thin sliver; day 14 is dark all day.
        assert_eq!(serin_illumination(14, 20.0), 0.0);
        assert!(serin_illumination(13, 20.0) > 0.0);
        assert!(serin_illumination(15, 20.0) > 0.05);
        assert!((serin_illumination(4, 0.0) - 1.0).abs() < 1e-9);
        assert_eq!(days_until_serin_dark(14), 0);
        assert_eq!(days_until_serin_dark(15), 19);
        assert_eq!(days_until_serin_dark(10), 4);
    }

    #[test]
    fn moon_phase_matches_the_client() {
        // (day, hour, Eldor illumination, up, Serin illumination, up) from
        // `getMoonPhaseState` in celestialSimulation.ts.
        let cases = [
            (14, 0.0, 1.0, true, 0.0, false),
            (14, 18.5, 0.9486, false, 0.0, true),
            (14, 23.0, 0.9361, true, 0.0, false),
            (15, 5.5, 0.9181, true, 0.0255, false),
            (15, 6.5, 0.9153, true, 0.0301, true),
            (9, 0.0, 0.6429, true, 0.5, true),
            (9, 19.0, 0.6994, true, 0.4208, false),
            (10, 4.0, 0.7262, false, 0.3833, true),
            (0, 12.0, 0.0357, true, 0.6111, false),
            (29, 21.25, 0.0, false, 0.4115, false),
            (44, 1.0, 0.9972, true, 0.9958, true),
        ];
        for (day, hour, eldor, eldor_up, serin, serin_up) in cases {
            let e = moon_phase(ELDOR_PERIOD_DAYS, ELDOR_PHASE_OFFSET_DAYS, day, hour);
            let s = moon_phase(SERIN_PERIOD_DAYS, SERIN_PHASE_OFFSET_DAYS, day, hour);
            assert!(
                (e.0 - eldor).abs() < 1e-3 && e.1 == eldor_up,
                "Eldor {day} {hour}: {e:?}"
            );
            assert!(
                (s.0 - serin).abs() < 1e-3 && s.1 == serin_up,
                "Serin {day} {hour}: {s:?}"
            );
        }
    }

    #[test]
    fn full_moon_windows_match_the_client() {
        // The client counts 3026 full-moon minutes in 4 windows over 60 days.
        let (mut minutes, mut runs, mut prev) = (0, 0, false);
        for m in 0..60 * 24 * 60 {
            let day = m / 1440;
            let dt = GameDateTime {
                year: GAME_START_YEAR as u32 + (day / 360) as u32,
                month: ((day % 360) / 30 + 1) as u8,
                day: (day % 30 + 1) as u8,
                hour: ((m % 1440) / 60) as u8,
                minute: (m % 60) as u8,
            };
            let up = is_full_moon_up(&dt);
            minutes += i32::from(up);
            runs += i32::from(up && !prev);
            prev = up;
        }
        assert_eq!((minutes, runs), (3026, 4));
    }

    #[test]
    fn a_full_moon_before_sunset_does_not_count() {
        let dt = |hour, minute| GameDateTime {
            year: 222,
            month: 7,
            day: 14,
            hour,
            minute,
        };
        assert!(is_full_moon_up(&dt(17, 47)));
        assert!(!is_full_moon_night(&dt(17, 47)));
        assert!(is_full_moon_night(&dt(23, 0)));
    }

    #[test]
    fn day_index_counts_from_the_epoch() {
        let dt = |year, month, day| GameDateTime {
            year,
            month,
            day,
            hour: 0,
            minute: 0,
        };
        assert_eq!(game_day_index(&dt(217, 1, 1)), 0);
        assert_eq!(game_day_index(&dt(217, 2, 1)), 30);
        assert_eq!(game_day_index(&dt(218, 1, 15)), 360 + 14);
    }
}
