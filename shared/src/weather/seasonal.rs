use std::sync::LazyLock;

use serde::Deserialize;

use crate::moon::{GAME_DAYS_PER_MONTH, GAME_MONTHS_PER_YEAR};
use crate::world::shortest_world_delta_x;
use crate::worldgen::noise::smoothstep;

use super::GAME_MINUTES_PER_DAY;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeasonalRainRegion {
    center: [f32; 2],
    inner_radius_m: f32,
    outer_radius_m: f32,
    summer_chance_multiplier: f64,
}

static REGIONS: LazyLock<Vec<SeasonalRainRegion>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../data-src/weather.json"))
        .expect("Invalid seasonal rain regions")
});

fn summer_weight(t_min: f64) -> f64 {
    let year_days = (GAME_DAYS_PER_MONTH * GAME_MONTHS_PER_YEAR) as f64;
    // Winter begins on 12/30, one day before the calendar year starts.
    let phase =
        (t_min / GAME_MINUTES_PER_DAY as f64 + 1.0).rem_euclid(year_days) / (year_days / 4.0);
    match phase as u8 {
        0 => 0.0,
        1 => f64::from(smoothstep(0.0, 1.0, phase.fract() as f32)),
        2 => 1.0,
        _ => 1.0 - f64::from(smoothstep(0.0, 1.0, phase.fract() as f32)),
    }
}

pub(super) fn chance_multiplier(spot: [f32; 2], birth_min: f64) -> f64 {
    let summer = summer_weight(birth_min);
    if summer == 0.0 {
        return 1.0;
    }
    REGIONS.iter().fold(1.0, |multiplier, region| {
        let dx = shortest_world_delta_x(region.center[0], spot[0]);
        let dz = spot[1] - region.center[1];
        let distance = dx.hypot(dz);
        let weight = 1.0 - smoothstep(region.inner_radius_m, region.outer_radius_m, distance);
        multiplier.min(1.0 - (1.0 - region.summer_chance_multiplier) * summer * f64::from(weight))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_day(spot: [f32; 2], day: f64) -> f64 {
        chance_multiplier(spot, day * GAME_MINUTES_PER_DAY as f64)
    }

    #[test]
    fn winter_stays_wet_and_summer_stays_dry() {
        let region = &REGIONS[0];
        for day in [0.0, 44.0, 88.0, 359.0] {
            assert_eq!(at_day(region.center, day), 1.0);
        }
        for day in [179.0, 224.0, 268.0] {
            assert!((at_day(region.center, day) - region.summer_chance_multiplier).abs() < 1e-9);
        }
        for day in [134.0, 314.0] {
            let multiplier = at_day(region.center, day);
            assert!(multiplier > region.summer_chance_multiplier && multiplier < 1.0);
        }
    }

    #[test]
    fn seasonal_transitions_are_continuous_and_repeat_each_year() {
        let spot = REGIONS[0].center;
        for day in [89.0, 179.0, 269.0, 359.0, 360.0] {
            assert!((at_day(spot, day - 0.001) - at_day(spot, day + 0.001)).abs() < 1e-6);
        }
        for day in 0..360 {
            let current = at_day(spot, day as f64);
            assert!((current - at_day(spot, day as f64 + 360.0)).abs() < 1e-9);
            if (89..179).contains(&day) {
                assert!(current >= at_day(spot, day as f64 + 1.0));
            }
            if (269..359).contains(&day) {
                assert!(current <= at_day(spot, day as f64 + 1.0));
            }
        }
    }

    #[test]
    fn regional_falloff_preserves_other_climates_and_world_wrap() {
        let region = &REGIONS[0];
        let sample = |dx| at_day([region.center[0] + dx, region.center[1]], 224.0);
        let inner = sample(region.inner_radius_m);
        let middle = sample((region.inner_radius_m + region.outer_radius_m) / 2.0);
        assert!(inner < middle && middle < 1.0);
        assert_eq!(sample(region.outer_radius_m), 1.0);
        assert_eq!(sample(region.outer_radius_m + 1000.0), 1.0);
        assert_eq!(sample(crate::world::WORLD_WIDTH_X), sample(0.0));
    }
}
