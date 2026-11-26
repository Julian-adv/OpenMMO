//! Seasonal events from `data-src/seasonal_events.csv` and the props each puts
//! up from `data-src/seasonal_props.csv`: decorations the world wears between
//! two real-world calendar dates every year (doc/SEASONAL_PROPS.md).
//! Props are placed from these tables, not from region object files.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::furniture::{FurniturePlacement, SEASONAL_PLACEMENT_ID_BASE};
use crate::world::wrap_world_x;

static SEASONAL_EVENTS_JSON: &str = include_str!("../../data/seasonal_events.json");
static SEASONAL_PROPS_JSON: &str = include_str!("../../data/seasonal_props.json");
static CATALOG_JSON: &str = include_str!("../../data/object_catalog.json");

const DAYS_IN_MONTH: [u8; 12] = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// A day of the real-world year, ordered through the year.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct MonthDay {
    pub month: u8,
    pub day: u8,
}

impl MonthDay {
    pub fn new(month: u8, day: u8) -> Option<Self> {
        let days = *DAYS_IN_MONTH.get(usize::from(month).checked_sub(1)?)?;
        (1..=days).contains(&day).then_some(Self { month, day })
    }

    /// `MM-DD` or `MM/DD`.
    pub fn parse(text: &str) -> Option<Self> {
        let (month, day) = text.split_once(['-', '/'])?;
        Self::new(month.parse().ok()?, day.parse().ok()?)
    }
}

impl std::fmt::Display for MonthDay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}-{:02}", self.month, self.day)
    }
}

/// First and last day of a run, both included. It may run past new year.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Season {
    pub start: MonthDay,
    pub end: MonthDay,
}

impl Season {
    /// `None` when either end is not a calendar day, as a client may send.
    pub fn checked(self) -> Option<Self> {
        Some(Self {
            start: MonthDay::new(self.start.month, self.start.day)?,
            end: MonthDay::new(self.end.month, self.end.day)?,
        })
    }

    pub fn contains(&self, date: MonthDay) -> bool {
        if self.start <= self.end {
            self.start <= date && date <= self.end
        } else {
            date >= self.start || date <= self.end
        }
    }
}

impl std::fmt::Display for Season {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// When an event runs: on its table dates, on an operator's dates, or never.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SeasonSchedule {
    #[default]
    Default,
    Period(Season),
    Off,
}

impl SeasonSchedule {
    pub fn season(self, default: Season) -> Option<Season> {
        match self {
            Self::Default => Some(default),
            Self::Period(season) => Some(season),
            Self::Off => None,
        }
    }
}

/// An event as the operator panel lists it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SeasonalEventState {
    pub id: String,
    pub default_season: Season,
    pub schedule: SeasonSchedule,
    pub props: u32,
    /// Whether its props stand today.
    pub active: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventRow {
    id: String,
    start_month: u8,
    start_day: u8,
    end_month: u8,
    end_day: u8,
}

#[derive(Debug, serde::Deserialize)]
struct Row {
    id: String,
    event: String,
    object: String,
    x: f32,
    z: f32,
    #[serde(default)]
    rotation: f32,
}

#[derive(serde::Deserialize)]
struct CatalogEntry {
    id: String,
}

#[derive(Debug)]
pub struct SeasonalEventDef {
    pub id: String,
    /// The run from the table, used until an operator sets another.
    pub season: Season,
}

#[derive(Debug)]
pub struct SeasonalPropDef {
    pub id: String,
    /// The event whose run puts this prop up.
    pub event: String,
    pub object: String,
    pub x: f32,
    pub z: f32,
    /// Degrees, the placement yaw convention.
    pub rotation: f32,
    index: u32,
}

impl SeasonalPropDef {
    pub fn placement(&self, ground: f32) -> FurniturePlacement {
        FurniturePlacement {
            id: SEASONAL_PLACEMENT_ID_BASE + self.index,
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

fn parse_event(row: EventRow) -> SeasonalEventDef {
    let id = row.id;
    let date = |month, day, which| {
        MonthDay::new(month, day)
            .unwrap_or_else(|| panic!("{id}: {which} {month}/{day} is not a calendar day"))
    };
    SeasonalEventDef {
        season: Season {
            start: date(row.start_month, row.start_day, "start"),
            end: date(row.end_month, row.end_day, "end"),
        },
        id,
    }
}

/// Every seasonal event, ordered by id.
pub fn seasonal_event_defs() -> &'static [SeasonalEventDef] {
    static DEFS: OnceLock<Vec<SeasonalEventDef>> = OnceLock::new();
    DEFS.get_or_init(|| {
        let mut events: Vec<SeasonalEventDef> =
            serde_json::from_str::<HashMap<String, EventRow>>(SEASONAL_EVENTS_JSON)
                .expect("seasonal_events.json is malformed")
                .into_values()
                .map(parse_event)
                .collect();
        events.sort_by(|a, b| a.id.cmp(&b.id));
        events
    })
}

pub fn seasonal_event_def(id: &str) -> Option<&'static SeasonalEventDef> {
    seasonal_event_defs().iter().find(|event| event.id == id)
}

fn parse(row: Row, index: u32, catalog: &HashSet<String>) -> SeasonalPropDef {
    let id = row.id;
    assert!(
        catalog.contains(&row.object),
        "{id}: {} is not a catalog object",
        row.object
    );
    assert!(
        seasonal_event_def(&row.event).is_some(),
        "{id}: {} is not a seasonal event",
        row.event
    );
    SeasonalPropDef {
        event: row.event,
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

    fn season(start: MonthDay, end: MonthDay) -> Season {
        Season { start, end }
    }

    #[test]
    fn a_season_across_new_year_covers_both_ends() {
        let winter = season(day(12, 10), day(1, 5));
        for date in [day(12, 10), day(12, 31), day(1, 1), day(1, 5)] {
            assert!(winter.contains(date), "{date:?}");
        }
        for date in [day(12, 9), day(1, 6), day(7, 1)] {
            assert!(!winter.contains(date), "{date:?}");
        }
    }

    #[test]
    fn a_season_inside_one_year_stops_at_its_last_day() {
        let summer = season(day(6, 1), day(8, 31));
        assert!(summer.contains(day(6, 1)) && summer.contains(day(8, 31)));
        assert!(!summer.contains(day(5, 31)) && !summer.contains(day(9, 1)));
    }

    #[test]
    fn a_schedule_picks_the_table_run_an_operator_run_or_none() {
        let table = season(day(12, 10), day(1, 5));
        let early = season(day(11, 20), day(12, 26));
        assert_eq!(SeasonSchedule::Default.season(table), Some(table));
        assert_eq!(SeasonSchedule::Period(early).season(table), Some(early));
        assert_eq!(SeasonSchedule::Off.season(table), None);
    }

    #[test]
    fn only_calendar_days_parse() {
        assert!(MonthDay::new(2, 29).is_some());
        for (month, day) in [(0, 1), (13, 1), (2, 30), (4, 31), (1, 0)] {
            assert!(MonthDay::new(month, day).is_none(), "{month}/{day}");
        }
        assert_eq!(MonthDay::parse("12-10"), Some(day(12, 10)));
        assert_eq!(MonthDay::parse("1/5"), Some(day(1, 5)));
        assert_eq!(day(1, 5).to_string(), "01-05");
        for text in ["", "12", "13-01", "02-30", "a-b", "12-10-1"] {
            assert_eq!(MonthDay::parse(text), None, "{text}");
        }
        let sent = Season {
            start: MonthDay { month: 2, day: 30 },
            end: day(3, 1),
        };
        assert_eq!(sent.checked(), None);
    }

    #[test]
    fn the_table_places_fixed_props_from_the_catalog() {
        let defs = seasonal_prop_defs();
        assert!(!defs.is_empty());
        let mut ids = HashSet::new();
        for def in defs {
            assert!(seasonal_event_def(&def.event).is_some(), "{}", def.id);
            let placement = def.placement(3.5);
            assert!(is_fixed_placement(placement.id));
            assert!(ids.insert(placement.id), "{} shares a placement id", def.id);
            assert_eq!((placement.y, placement.floor_level), (3.5, 0));
        }
    }
}
