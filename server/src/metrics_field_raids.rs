use super::{auth_db, metrics_response, unix_now, MetricsState};
use axum::{extract::State, response::Response};
use onlinerpg_shared::field_boss::field_boss_defs;
use onlinerpg_shared::messages::FieldRaidOutcome;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct FieldRaidEvent {
    pub timestamp: i64,
    pub boss: String,
    /// `None` for the summon, then how the raid ended.
    pub outcome: Option<FieldRaidOutcome>,
}

impl FieldRaidEvent {
    pub fn storage_key(&self) -> &'static str {
        self.outcome.map_or("summoned", outcome_key)
    }
}

pub fn outcome_key(outcome: FieldRaidOutcome) -> &'static str {
    match outcome {
        FieldRaidOutcome::Defeated => "defeated",
        FieldRaidOutcome::MoonSet => "moon_set",
        FieldRaidOutcome::Dawn => "dawn",
        FieldRaidOutcome::Wiped => "wiped",
    }
}

pub fn outcome_from_key(key: &str) -> Option<FieldRaidOutcome> {
    use serde::de::{value::Error, IntoDeserializer};
    FieldRaidOutcome::deserialize(IntoDeserializer::<Error>::into_deserializer(key)).ok()
}

/// One finished raid and everyone who took part in it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldRaidRecord {
    pub boss: String,
    pub summoned_at: i64,
    pub ended_at: i64,
    pub outcome: FieldRaidOutcome,
    pub summoner_id: Option<i64>,
    /// The boss's HP when the raid ended; 0 on a defeat.
    pub boss_health: u32,
    pub boss_max_health: u32,
    pub raiders: Vec<FieldRaidRaider>,
}

/// A character who was near the summon, came into the battle zone, hit the
/// boss, was hit by it or died in the zone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FieldRaidRaider {
    pub character_id: i64,
    pub name: String,
    /// HP taken off the boss.
    pub dealt: u64,
    /// HP lost to the boss.
    pub taken: u64,
    /// Deaths in the battle zone.
    pub deaths: u32,
    /// Within the notice radius at the offering.
    pub near_at_summon: bool,
    /// Alive in the battle zone at some raid tick.
    pub in_zone: bool,
    pub in_zone_at_end: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct FieldRaidTally {
    pub summons: u64,
    pub defeated: u64,
    pub moon_set: u64,
    pub dawn: u64,
    pub wiped: u64,
    pub last_defeated_at: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FieldRaidCounts {
    boss: String,
    name: String,
    #[serde(flatten)]
    tally: FieldRaidTally,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentFieldRaid {
    name: String,
    #[serde(flatten)]
    record: FieldRaidRecord,
}

#[derive(Debug, Serialize, Deserialize)]
struct FieldRaids {
    until: i64,
    collection_started_at: i64,
    bosses: Vec<FieldRaidCounts>,
    /// Newest first.
    recent: Vec<RecentFieldRaid>,
}

const RECENT_RAIDS: usize = 10;

pub(super) async fn field_raids(State(state): State<MetricsState>) -> Response {
    state.game.flush_field_raid_events(&state.auth).await;
    let until = unix_now();
    metrics_response(
        auth_db(move || {
            let (collection_started_at, mut tallies) = state.auth.field_raid_tallies(until)?;
            let records = state.auth.recent_field_raids(until, RECENT_RAIDS)?;
            let (_, monsters, _) = state.game.loot_defs();
            let name = |monster: &str| {
                monsters
                    .get(monster)
                    .map_or_else(|| monster.to_owned(), |m| m.name.clone())
            };
            let bosses = field_boss_defs()
                .iter()
                .map(|def| FieldRaidCounts {
                    boss: def.id.clone(),
                    name: name(&def.monster),
                    tally: tallies.remove(&def.id).unwrap_or_default(),
                })
                .collect();
            let recent = records
                .into_iter()
                .map(|record| {
                    let def = field_boss_defs().iter().find(|def| def.id == record.boss);
                    RecentFieldRaid {
                        name: name(def.map_or(&record.boss, |def| &def.monster)),
                        record,
                    }
                })
                .collect();
            Ok(FieldRaids {
                until,
                collection_started_at,
                bosses,
                recent,
            })
        })
        .await,
        "Field raids",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::AuthService, game_state::tests::make_test_game_state};
    use axum::http::StatusCode;
    use std::sync::Arc;

    #[tokio::test]
    async fn field_raids_count_saved_and_pending_events_per_boss() {
        let dir = crate::test_util::unique_temp_dir("field_raids_api");
        let auth = Arc::new(AuthService::new(dir.join("game.db")).unwrap());
        let game = Arc::new(make_test_game_state("field_raids_api"));
        let now = unix_now();
        let event = |timestamp, outcome| FieldRaidEvent {
            timestamp,
            boss: "terraclash".into(),
            outcome,
        };
        auth.record_field_raid_events(&[
            event(now - 300, None),
            event(now - 200, Some(FieldRaidOutcome::Wiped)),
            event(now - 100, None),
        ])
        .unwrap();
        game.pending_field_raid_events()
            .push(event(now, Some(FieldRaidOutcome::Defeated)));
        let router = crate::metrics::metrics_routes(Arc::clone(&game), Arc::clone(&auth));
        let url = format!(
            "{}/api/metrics/field-raids",
            crate::test_util::serve(router).await
        );

        let response = reqwest::get(&url).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let data: FieldRaids = response.json().await.unwrap();
        let terraclash = data.bosses.iter().find(|b| b.boss == "terraclash").unwrap();
        assert_eq!(terraclash.name, "Terraclash");
        let tally = &terraclash.tally;
        assert_eq!(tally.summons, 2);
        assert_eq!(tally.defeated, 1);
        assert_eq!(tally.wiped, 1);
        assert_eq!(tally.moon_set + tally.dawn, 0);
        assert_eq!(tally.last_defeated_at, Some(now));
        assert!(game.pending_field_raid_events().is_empty());
    }

    #[tokio::test]
    async fn field_raids_list_recent_raids_newest_first_with_their_raiders() {
        let dir = crate::test_util::unique_temp_dir("field_raids_recent_api");
        let auth = Arc::new(AuthService::new(dir.join("game.db")).unwrap());
        let game = Arc::new(make_test_game_state("field_raids_recent_api"));
        let now = unix_now();
        let raider = |character_id, name: &str, dealt, taken| FieldRaidRaider {
            character_id,
            name: name.into(),
            dealt,
            taken,
            in_zone: true,
            ..FieldRaidRaider::default()
        };
        let raid = |ended_at, outcome, raiders| FieldRaidRecord {
            boss: "terraclash".into(),
            summoned_at: ended_at - 700,
            ended_at,
            outcome,
            summoner_id: Some(8575),
            boss_health: 61_234,
            boss_max_health: 80_000,
            raiders,
        };
        let dawn = raid(
            now - 100,
            FieldRaidOutcome::Dawn,
            vec![
                raider(8575, "해적", 0, 900),
                raider(6549, "홀로죽어가는남자", 1_200, 400),
            ],
        );
        auth.record_field_raids(&[dawn]).unwrap();
        let won = raid(now, FieldRaidOutcome::Defeated, vec![raider(1, "a", 5, 0)]);
        game.pending_field_raid_records().push(won.clone());
        let router = crate::metrics::metrics_routes(Arc::clone(&game), Arc::clone(&auth));
        let url = format!(
            "{}/api/metrics/field-raids",
            crate::test_util::serve(router).await
        );

        let data: FieldRaids = reqwest::get(&url).await.unwrap().json().await.unwrap();
        assert!(game.pending_field_raid_records().is_empty());
        let recent: Vec<_> = data.recent.iter().map(|r| &r.record).collect();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0], &won);
        let names: Vec<_> = recent[1].raiders.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["홀로죽어가는남자", "해적"], "by damage dealt");
        assert_eq!(recent[1].outcome, FieldRaidOutcome::Dawn);
        assert_eq!(recent[1].boss_health, 61_234);
        assert_eq!(data.recent[1].name, "Terraclash");
        let terraclash = data.bosses.iter().find(|b| b.boss == "terraclash").unwrap();
        let tally = &terraclash.tally;
        assert_eq!(
            (tally.summons, tally.dawn, tally.defeated),
            (0, 1, 1),
            "each saved raid is also its end event"
        );
    }

    #[test]
    fn every_outcome_round_trips_through_its_storage_key() {
        for outcome in [
            FieldRaidOutcome::Defeated,
            FieldRaidOutcome::MoonSet,
            FieldRaidOutcome::Dawn,
            FieldRaidOutcome::Wiped,
        ] {
            assert_eq!(outcome_from_key(outcome_key(outcome)), Some(outcome));
        }
        assert_eq!(outcome_from_key("summoned"), None);
    }
}
