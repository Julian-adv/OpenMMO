//! Seasonal props standing in the world: the props of every event running
//! today. An event runs on its dates in `data-src/seasonal_events.csv` unless
//! an operator set another schedule. Today is the real-world date in Korea
//! (UTC+9), read at boot and every hour after (doc/SEASONAL_PROPS.md).

use std::collections::HashMap;

use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::seasonal::{
    seasonal_event_def, seasonal_event_defs, seasonal_prop_defs, MonthDay, Season, SeasonSchedule,
    SeasonalEventState, SeasonalPropDef,
};
use onlinerpg_shared::{PlayerId, ServerMessage};
use tracing::{error, info, warn};

use super::passability::region_of;
use super::{auth_db, GameState};
use crate::auth::AuthService;

pub(crate) type SeasonalSite = (&'static SeasonalPropDef, FurniturePlacement);

const SEASON_USAGE: &str = "Season: /season, /season <event> <MM-DD> <MM-DD>, \
     /season <event> off, /season <event> default";

/// Every seasonal prop on sampled ground, with the day and the operator
/// schedules that decide which stand.
#[derive(Default)]
pub(crate) struct SeasonalProps {
    sites: Vec<SeasonalSite>,
    today: Option<MonthDay>,
    /// By event id. An event without one runs on its table dates.
    schedules: HashMap<String, SeasonSchedule>,
}

impl SeasonalProps {
    fn schedule(&self, event: &str) -> SeasonSchedule {
        self.schedules.get(event).copied().unwrap_or_default()
    }

    fn runs(&self, event: &str) -> bool {
        let (Some(today), Some(def)) = (self.today, seasonal_event_def(event)) else {
            return false;
        };
        self.schedule(event)
            .season(def.season)
            .is_some_and(|season| season.contains(today))
    }

    fn standing(&self) -> impl Iterator<Item = &FurniturePlacement> {
        self.sites
            .iter()
            .filter(|(def, _)| self.runs(&def.event))
            .map(|(_, placement)| placement)
    }
}

fn today_kst() -> MonthDay {
    let now = time::OffsetDateTime::now_utc() + time::Duration::hours(9);
    MonthDay::new(u8::from(now.month()), now.day()).expect("the clock reads a calendar day")
}

impl GameState {
    /// Stand every seasonal prop on the baked ground. One whose ground cannot
    /// be read stays out, so a missing bake does not stop boot.
    pub(super) async fn load_seasonal_props(&self) {
        let mut sites = Vec::new();
        for def in seasonal_prop_defs() {
            match self.height_sampler.sample_height(def.x, def.z).await {
                Ok(ground) => sites.push((def, def.placement(ground))),
                Err(e) => warn!("Seasonal prop {}: no ground to stand on: {e}", def.id),
            }
        }
        self.place_seasonal_props(sites, today_kst());
    }

    pub(super) fn place_seasonal_props(&self, sites: Vec<SeasonalSite>, today: MonthDay) {
        let mut props = self.seasonal_props_write();
        props.sites = sites;
        props.today = Some(today);
    }

    /// Operator schedules an earlier run stored. Boot only, before the
    /// passability cache is built: nothing is re-synced here.
    pub fn load_seasonal_schedules(&self, stored: Vec<(String, SeasonSchedule)>) {
        let mut props = self.seasonal_props_write();
        for (event, schedule) in stored {
            if seasonal_event_def(&event).is_some() {
                props.schedules.insert(event, schedule);
            } else {
                warn!("Seasonal schedule for unknown event {event} ignored");
            }
        }
    }

    fn seasonal_props_read(&self) -> std::sync::RwLockReadGuard<'_, SeasonalProps> {
        self.seasonal_props
            .read()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn seasonal_props_write(&self) -> std::sync::RwLockWriteGuard<'_, SeasonalProps> {
        self.seasonal_props
            .write()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Placements no region file holds: field-boss sites and the seasonal
    /// props standing today.
    pub(crate) fn fixed_placements_in(&self, rx: i32, rz: i32) -> Vec<FurniturePlacement> {
        let mut placements = self.field_boss_placements_in(rx, rz);
        placements.extend(
            self.seasonal_props_read()
                .standing()
                .filter(|p| region_of(p.x, p.z) == (rx, rz))
                .cloned(),
        );
        placements
    }

    /// Regions a fixed placement can stand in, in or out of season.
    pub(super) fn fixed_regions(&self) -> Vec<(i32, i32)> {
        let mut regions = self.field_boss_regions();
        regions.extend(
            self.seasonal_props_read()
                .sites
                .iter()
                .map(|(_, p)| region_of(p.x, p.z)),
        );
        regions.sort_unstable();
        regions.dedup();
        regions
    }

    /// Apply `edit`; the regions whose props went up or came down.
    fn change_seasonal(&self, edit: impl FnOnce(&mut SeasonalProps)) -> Vec<(i32, i32)> {
        let mut props = self.seasonal_props_write();
        let stood: Vec<bool> = props
            .sites
            .iter()
            .map(|(def, _)| props.runs(&def.event))
            .collect();
        edit(&mut props);
        let mut regions: Vec<_> = props
            .sites
            .iter()
            .zip(stood)
            .filter(|((def, _), stood)| props.runs(&def.event) != *stood)
            .map(|((_, p), _)| region_of(p.x, p.z))
            .collect();
        regions.sort_unstable();
        regions.dedup();
        regions
    }

    /// Move the calendar to `today`.
    pub(super) fn set_seasonal_date(&self, today: MonthDay) -> Vec<(i32, i32)> {
        self.change_seasonal(|props| props.today = Some(today))
    }

    /// Put `event` on `schedule`, in memory only.
    pub(super) fn apply_seasonal_schedule(
        &self,
        event: &str,
        schedule: SeasonSchedule,
    ) -> Vec<(i32, i32)> {
        self.change_seasonal(|props| match schedule {
            SeasonSchedule::Default => {
                props.schedules.remove(event);
            }
            schedule => {
                props.schedules.insert(event.to_string(), schedule);
            }
        })
    }

    /// Rebuild collision for `regions` and have every client holding one
    /// re-read it, so props go up and come down in front of whoever is there.
    async fn resync_seasonal_regions(&self, regions: Vec<(i32, i32)>) {
        if regions.is_empty() {
            return;
        }
        for &(rx, rz) in &regions {
            let placements = match self.terrain_io.read_object(rx, rz).await {
                Ok(json) => Self::parse_region_furniture(&json),
                Err(e) => {
                    warn!("Seasonal props: region ({rx},{rz}) unreadable: {e}");
                    continue;
                }
            };
            match placements {
                Ok(placements) => {
                    self.sync_region_furniture(rx, rz, &placements);
                    info!("Seasonal props changed in region ({rx},{rz})");
                }
                Err(e) => warn!("Seasonal props: bad region objects ({rx},{rz}): {e}"),
            }
        }
        self.broadcast(ServerMessage::ObjectRegionsChanged { regions });
    }

    /// Put props up and take them down as the real-world date crosses the
    /// edge of a run.
    pub async fn refresh_seasonal_props(&self) {
        let regions = self.set_seasonal_date(today_kst());
        self.resync_seasonal_regions(regions).await;
    }

    /// Run `event` on `schedule` from now on. Stored first, so a schedule
    /// the world shows is one a restart keeps.
    pub async fn set_seasonal_schedule(
        &self,
        admin_id: &PlayerId,
        event: &str,
        schedule: SeasonSchedule,
        auth: &AuthService,
    ) -> Result<(), String> {
        let Some(def) = seasonal_event_def(event) else {
            let known: Vec<&str> = seasonal_event_defs()
                .iter()
                .map(|def| def.id.as_str())
                .collect();
            return Err(format!(
                "Season: no event called {event}. Events: {}.",
                known.join(", ")
            ));
        };
        let schedule = match schedule {
            SeasonSchedule::Period(season) => SeasonSchedule::Period(
                season
                    .checked()
                    .ok_or_else(|| "Season: that is not a calendar day.".to_string())?,
            ),
            other => other,
        };
        let auth = auth.clone();
        if let Err(err) = auth_db(move || auth.save_seasonal_schedule(&def.id, schedule)).await {
            error!("Seasonal schedule write failed: {err}");
            return Err("Season: could not store the schedule.".to_string());
        }
        let regions = self.apply_seasonal_schedule(&def.id, schedule);
        info!(admin = ?admin_id, event = %def.id, ?schedule, "seasonal schedule set");
        self.resync_seasonal_regions(regions).await;
        Ok(())
    }

    fn seasonal_event_states(&self) -> (MonthDay, Vec<SeasonalEventState>) {
        let props = self.seasonal_props_read();
        let events = seasonal_event_defs()
            .iter()
            .map(|def| SeasonalEventState {
                id: def.id.clone(),
                default_season: def.season,
                schedule: props.schedule(&def.id),
                props: props
                    .sites
                    .iter()
                    .filter(|(prop, _)| prop.event == def.id)
                    .count()
                    .try_into()
                    .unwrap_or(u32::MAX),
                active: props.runs(&def.id),
            })
            .collect();
        (props.today.unwrap_or_else(today_kst), events)
    }

    pub fn seasonal_events_message(&self, error: Option<String>) -> ServerMessage {
        let (today, events) = self.seasonal_event_states();
        ServerMessage::SeasonalEvents {
            today,
            events,
            error,
        }
    }

    fn seasonal_summary(&self) -> String {
        let (today, events) = self.seasonal_event_states();
        let lines: Vec<String> = events
            .iter()
            .map(|event| {
                let run = match event.schedule {
                    SeasonSchedule::Default => format!("{} (default)", event.default_season),
                    SeasonSchedule::Period(season) => season.to_string(),
                    SeasonSchedule::Off => "off".to_string(),
                };
                let state = if event.active { "up" } else { "down" };
                format!("{} {run}, {state}", event.id)
            })
            .collect();
        format!("Season (KST {today}): {}", lines.join("; "))
    }

    pub(super) async fn season_command(
        &self,
        admin_id: &PlayerId,
        args: &str,
        auth: &AuthService,
    ) -> Result<String, String> {
        let mut words = args.split_whitespace();
        let Some(event) = words.next() else {
            return Ok(self.seasonal_summary());
        };
        let schedule = match (words.next(), words.next(), words.next()) {
            (Some("off"), None, None) => SeasonSchedule::Off,
            (Some("default"), None, None) => SeasonSchedule::Default,
            (Some(start), Some(end), None) => {
                let day = |text: &str| {
                    MonthDay::parse(text)
                        .ok_or_else(|| format!("Season: {text} is not a day; write MM-DD."))
                };
                SeasonSchedule::Period(Season {
                    start: day(start)?,
                    end: day(end)?,
                })
            }
            _ => return Err(SEASON_USAGE.to_string()),
        };
        self.set_seasonal_schedule(admin_id, event, schedule, auth)
            .await?;
        // Keeps the operator's open panel in step with the command.
        self.send_direct_message(admin_id, self.seasonal_events_message(None))
            .await;
        Ok(self.seasonal_summary())
    }
}
