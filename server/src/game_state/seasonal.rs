//! Seasonal props standing in the world: the in-season rows of
//! `data-src/seasonal_props.csv`, checked against the real-world date at boot
//! and every hour after (doc/SEASONAL_PROPS.md).

use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::seasonal::{seasonal_prop_defs, MonthDay, SeasonalPropDef};
use tracing::{info, warn};

use super::passability::region_of;
use super::GameState;

pub(crate) type SeasonalSite = (&'static SeasonalPropDef, FurniturePlacement);

/// Every seasonal prop on sampled ground, and the day that decides which stand.
#[derive(Default)]
pub(crate) struct SeasonalProps {
    sites: Vec<SeasonalSite>,
    today: Option<MonthDay>,
}

impl SeasonalProps {
    fn standing(&self) -> impl Iterator<Item = &FurniturePlacement> {
        self.sites
            .iter()
            .filter(|(def, _)| self.today.is_some_and(|today| def.in_season(today)))
            .map(|(_, placement)| placement)
    }
}

fn today_utc() -> MonthDay {
    let now = time::OffsetDateTime::now_utc();
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
        self.place_seasonal_props(sites, today_utc());
    }

    pub(super) fn place_seasonal_props(&self, sites: Vec<SeasonalSite>, today: MonthDay) {
        *self
            .seasonal_props
            .write()
            .unwrap_or_else(|e| e.into_inner()) = SeasonalProps {
            sites,
            today: Some(today),
        };
    }

    fn seasonal_props_read(&self) -> std::sync::RwLockReadGuard<'_, SeasonalProps> {
        self.seasonal_props
            .read()
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

    /// Move the calendar to `today`; the regions whose props went up or came down.
    pub(super) fn set_seasonal_date(&self, today: MonthDay) -> Vec<(i32, i32)> {
        let mut props = self
            .seasonal_props
            .write()
            .unwrap_or_else(|e| e.into_inner());
        let before = props.today.replace(today);
        let mut regions: Vec<_> = props
            .sites
            .iter()
            .filter(|(def, _)| before.is_some_and(|day| def.in_season(day)) != def.in_season(today))
            .map(|(_, p)| region_of(p.x, p.z))
            .collect();
        regions.sort_unstable();
        regions.dedup();
        regions
    }

    /// Put props up and take them down as the real-world date crosses a
    /// season's edge. Clients draw the change the next time they load the region.
    pub async fn refresh_seasonal_props(&self) {
        for (rx, rz) in self.set_seasonal_date(today_utc()) {
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
    }
}
