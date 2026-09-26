//! The seasonal wind that steers rain cells.

use crate::moon::{GAME_DAYS_PER_MONTH, GAME_MONTHS_PER_YEAR};
use crate::worldgen::noise::smoothstep;

use super::GAME_MINUTES_PER_DAY;

/// 0 through winter, 1 through summer, 2 at the next winter; spring and
/// autumn ease between them.
fn year_turn(t_min: f64) -> f64 {
    let year_days = (GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR) as f64;
    // Winter begins on 12/30, one day before the calendar year starts.
    let phase =
        (t_min / GAME_MINUTES_PER_DAY as f64 + 1.0).rem_euclid(year_days) / (year_days / 4.0);
    let ease = f64::from(smoothstep(0.0, 1.0, phase.fract() as f32));
    match phase as u8 {
        0 => 0.0,
        1 => ease,
        2 => 1.0,
        _ => 1.0 + ease,
    }
}

/// Winter cells head east-northeast on a west-southwest wind; summer cells
/// head west-northwest on an east-southeast wind, which crosses the central
/// Valdran massif before reaching the western plain.
const WINTER_DRIFT_HEADING: f64 = std::f64::consts::PI / 8.0;
const SUMMER_DRIFT_HEADING: f64 = 7.0 * std::f64::consts::PI / 8.0;

/// Drift heading in radians counter-clockwise from east (north = -Z). It turns
/// through north in spring and through south in autumn.
pub(super) fn drift_heading(t_min: f64) -> f64 {
    let turn = year_turn(t_min);
    if turn <= 1.0 {
        WINTER_DRIFT_HEADING + (SUMMER_DRIFT_HEADING - WINTER_DRIFT_HEADING) * turn
    } else {
        SUMMER_DRIFT_HEADING
            + (WINTER_DRIFT_HEADING + std::f64::consts::TAU - SUMMER_DRIFT_HEADING) * (turn - 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_heads_ene_in_winter_wnw_in_summer_and_turns_smoothly() {
        use std::f64::consts::PI;
        let heading = |day: f64| drift_heading(day * GAME_MINUTES_PER_DAY as f64);
        for day in [0.0, 44.0, 88.0, 359.0] {
            assert!((heading(day).rem_euclid(2.0 * PI) - WINTER_DRIFT_HEADING).abs() < 1e-9);
        }
        for day in [179.0, 224.0, 268.0] {
            assert!((heading(day) - SUMMER_DRIFT_HEADING).abs() < 1e-9);
        }
        // Spring turns through north, autumn through south.
        assert!((heading(134.0) - PI / 2.0).abs() < 1e-6);
        assert!((heading(314.0) - 1.5 * PI).abs() < 1e-6);
        let mut prev = heading(-1.0);
        let mut day = -1.0;
        while day < 359.0 {
            day += 0.25;
            let h = heading(day);
            assert!((h - prev).rem_euclid(2.0 * PI) < 0.05, "day {day}");
            prev = h;
        }
    }
}
