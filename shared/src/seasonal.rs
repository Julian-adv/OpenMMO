//! Seasonal props from `data-src/seasonal_props.csv`: decorations the world
//! wears between two real-world calendar dates every year (doc/SEASONAL_PROPS.md).
//! They are placed from this table, not from region object files.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::furniture::{FurniturePlacement, FIXED_PLACEMENT_ID_BASE};
use crate::world::wrap_world_x;

static SEASONAL_PROPS_JSON: &str = include_str!("../../data/seasonal_props.json");
static CATALOG_JSON: &str = include_str!("../../data/object_catalog.json");

/// Clear of the field-boss ids, which start at `FIXED_PLACEMENT_ID_BASE`.
const PLACEMENT_ID_BASE: u32 = FIXED_PLACEMENT_ID_BASE + 100_000_000;

const DAYS_IN_MONTH: [u8; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A day of the real-world year, ordered through the year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonthDay {
    pub month: u8,
    pub day: u8,
}

impl MonthDay {
    pub fn new(month: u8, day: u8) -> Option<Self> {
        let days = *DAYS_IN_MONTH.get(usize::from(month).checked_sub(1)?)?;
        (1..=days).contains(&day).then_some(Self { month, day })
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    object: String,
    x: f32,
    z: f32,
    #[serde(default)]
    rotation: f32,
    start_month: u8,
    start_day: u8,
    end_month: u8,
    end_day: u8,
}

#[derive(serde::Deserialize)]
struct CatalogEntry {
    id: String,
}

#[derive(Debug)]
pub struct SeasonalPropDef {
    pub id: String,
    pub object: String,
    pub x: f32,
    pub z: f32,
    /// Degrees, the placement yaw convention.
    pub rotation: f32,
    /// First and last day standing, both included.
    pub start: MonthDay,
    pub end: MonthDay,
    index: u32,
}

impl SeasonalPropDef {
    /// Whether the prop stands on `date`. A season may run past new year.
    pub fn in_season(&self, date: MonthDay) -> bool {
        if self.start <= self.end {
            self.start <= date && date <= self.end
        } else {
            date >= self.start || date <= self.end
        }
    }

    pub fn placement(&self, ground: f32) -> FurniturePlacement {
        FurniturePlacement {
            id: PLACEMENT_ID_BASE + self.index,
            type_id: self.object.clone(),
            x: self.x,
            y: ground,
            z: self.z,
            rotation_deg: self.rotation,
            floor_level: 0,
            open_doors: 0,
        }
    }
}

fn parse(row: Row, index: u32, catalog: &HashSet<String>) -> SeasonalPropDef {
    let id = row.id;
    assert!(
        catalog.contains(&row.object),
        "{id}: {} is not a catalog object",
        row.object
    );
    let date = |month, day, which| {
        MonthDay::new(month, day)
            .unwrap_or_else(|| panic!("{id}: {which} {month}/{day} is not a calendar day"))
    };
    SeasonalPropDef {
        start: date(row.start_month, row.start_day, "start"),
        end: date(row.end_month, row.end_day, "end"),
        object: row.object,
        x: wrap_world_x(row.x),
        z: row.z,
        rotation: row.rotation,
        index,
        id,
    }
}

/// Every seasonal prop, ordered by id so placement ids stay stable.
pub fn seasonal_prop_defs() -> &'static [SeasonalPropDef] {
    static DEFS: OnceLock<Vec<SeasonalPropDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut rows: Vec<Row> = serde_json::from_str::<HashMap<String, Row>>(SEASONAL_PROPS_JSON)
            .expect("seasonal_props.json is malformed")
            .into_values()
            .collect();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let catalog: HashSet<String> = serde_json::from_str::<Vec<CatalogEntry>>(CATALOG_JSON)
            .expect("object_catalog.json is malformed")
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        rows.into_iter()
            .zip(0..)
            .map(|(row, index)| parse(row, index, &catalog))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furniture::is_fixed_placement;

    fn day(month: u8, day: u8) -> MonthDay {
        MonthDay::new(month, day).unwrap()
    }

    fn prop(start: MonthDay, end: MonthDay) -> SeasonalPropDef {
        SeasonalPropDef {
            id: "test".into(),
            object: "winter_tree".into(),
            x: 0.0,
            z: 0.0,
            rotation: 0.0,
            start,
            end,
            index: 0,
        }
    }

    #[test]
    fn a_season_across_new_year_covers_both_ends() {
        let winter = prop(day(12, 10), day(1, 5));
        for date in [day(12, 10), day(12, 31), day(1, 1), day(1, 5)] {
            assert!(winter.in_season(date), "{date:?}");
        }
        for date in [day(12, 9), day(1, 6), day(7, 1)] {
            assert!(!winter.in_season(date), "{date:?}");
        }
    }

    #[test]
    fn a_season_inside_one_year_stops_at_its_last_day() {
        let summer = prop(day(6, 1), day(8, 31));
        assert!(summer.in_season(day(6, 1)) && summer.in_season(day(8, 31)));
        assert!(!summer.in_season(day(5, 31)) && !summer.in_season(day(9, 1)));
    }

    #[test]
    fn only_calendar_days_parse() {
        assert!(MonthDay::new(2, 29).is_some());
        for (month, day) in [(0, 1), (13, 1), (2, 30), (4, 31), (1, 0)] {
            assert!(MonthDay::new(month, day).is_none(), "{month}/{day}");
        }
    }

    #[test]
    fn the_table_places_fixed_props_from_the_catalog() {
        let defs = seasonal_prop_defs();
        assert!(!defs.is_empty());
        let mut ids = HashSet::new();
        for def in defs {
            let placement = def.placement(3.5);
            assert!(is_fixed_placement(placement.id));
            assert!(ids.insert(placement.id), "{} shares a placement id", def.id);
            assert_eq!((placement.y, placement.floor_level), (3.5, 0));
        }
    }
}
