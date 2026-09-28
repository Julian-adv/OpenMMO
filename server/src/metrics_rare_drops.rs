use super::{auth_db, metrics_response, unix_now, MetricsState};
use axum::{extract::State, response::Response};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const TRACKED_ITEMS: [&str; 3] = [
    "scroll_of_enchant_weapon",
    "scroll_of_enchant_armor",
    "stethoscope",
];

/// Counts keyed by hour, waiting for the next save.
#[derive(Debug, Default, Clone)]
pub struct LootTally {
    /// (hour, monster_type, effective level) → kills
    pub kills: HashMap<(i64, String, u8), u64>,
    /// (hour, item_def_id, from a kill) → quantity
    pub drops: HashMap<(i64, String, bool), u64>,
}

impl LootTally {
    pub fn is_empty(&self) -> bool {
        self.kills.is_empty() && self.drops.is_empty()
    }

    pub fn merge(&mut self, other: LootTally) {
        for (key, count) in other.kills {
            *self.kills.entry(key).or_default() += count;
        }
        for (key, count) in other.drops {
            *self.drops.entry(key).or_default() += count;
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ItemHoldings {
    inventory: u64,
    npc_inventory: u64,
    storage: u64,
    ground: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct WorldDropChance {
    chance: f32,
    low_level_chance: Option<f32>,
    low_level_max_level: Option<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
struct MonsterDropChance {
    monster_type: String,
    name: String,
    chance: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct RareDropItem {
    item_def_id: String,
    name: String,
    world_drop: Option<WorldDropChance>,
    monster_drops: Vec<MonsterDropChance>,
    /// Kills that could roll this item.
    kills: u64,
    expected_kill_drops: f64,
    kill_drops: u64,
    /// World drops from treasure chests, barrels and crates.
    other_drops: u64,
    holdings: ItemHoldings,
}

#[derive(Debug, Serialize, Deserialize)]
struct RareDrops {
    until: i64,
    collection_started_at: i64,
    total_kills: u64,
    items: Vec<RareDropItem>,
}

pub(super) async fn rare_drops(State(state): State<MetricsState>) -> Response {
    state.game.flush_loot_tally(&state.auth).await;
    let ground = state.game.ground_item_quantities(&TRACKED_ITEMS).await;
    let until = unix_now();
    metrics_response(
        auth_db(move || {
            let stored = state.auth.loot_tally(&TRACKED_ITEMS)?;
            let (world, monsters) = state.game.loot_defs();
            let item_defs = crate::item_defs::item_defs();
            let mut monster_drops_by_item: HashMap<&str, Vec<MonsterDropChance>> = HashMap::new();
            for monster_type in monsters.ids() {
                let Some(def) = monsters.get(monster_type) else {
                    continue;
                };
                for (item, chance) in def.drop_entries() {
                    monster_drops_by_item
                        .entry(item.as_str())
                        .or_default()
                        .push(MonsterDropChance {
                            monster_type: monster_type.to_owned(),
                            name: def.name.clone(),
                            chance: *chance,
                        });
                }
            }
            let items = TRACKED_ITEMS
                .iter()
                .map(|&id| {
                    let world_entry = world.entry(id);
                    let monster_drops = monster_drops_by_item.remove(id).unwrap_or_default();
                    let mut kills = 0;
                    let mut expected_kill_drops = 0.0;
                    for (monster_type, level, count) in &stored.kills {
                        let chance = f64::from(
                            world_entry.map_or(0.0, |e| e.chance_for(Some(*level)))
                                + monster_drops
                                    .iter()
                                    .find(|m| &m.monster_type == monster_type)
                                    .map_or(0.0, |m| m.chance),
                        );
                        if chance > 0.0 {
                            kills += count;
                            expected_kill_drops += chance * *count as f64;
                        }
                    }
                    let (kill_drops, other_drops) =
                        stored.drops.get(id).copied().unwrap_or_default();
                    let (inventory, npc_inventory, storage) =
                        stored.holdings.get(id).copied().unwrap_or_default();
                    RareDropItem {
                        item_def_id: id.to_owned(),
                        name: item_defs
                            .get(id)
                            .map_or_else(|| id.to_owned(), |def| def.name.clone()),
                        world_drop: world_entry.map(|e| WorldDropChance {
                            chance: e.chance,
                            low_level_chance: e.low_level_chance,
                            low_level_max_level: e.low_level_max_level,
                        }),
                        monster_drops,
                        kills,
                        expected_kill_drops,
                        kill_drops,
                        other_drops,
                        holdings: ItemHoldings {
                            inventory,
                            npc_inventory,
                            storage,
                            ground: ground.get(id).copied().unwrap_or(0),
                        },
                    }
                })
                .collect();
            Ok(RareDrops {
                until,
                collection_started_at: stored.collection_started_at,
                total_kills: stored.kills.iter().map(|(_, _, count)| count).sum(),
                items,
            })
        })
        .await,
        "Rare drops",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::AuthService, game_state::tests::make_test_game_state};
    use axum::http::StatusCode;
    use std::sync::Arc;

    #[tokio::test]
    async fn rare_drops_combine_pending_kills_with_drop_chances() {
        let dir = crate::test_util::unique_temp_dir("rare_drops_api");
        let auth = Arc::new(AuthService::new(dir.join("game.db")).unwrap());
        let game = Arc::new(make_test_game_state("rare_drops_api"));
        {
            let now = unix_now();
            let now = now - now % crate::metrics::SAMPLE_INTERVAL_SECONDS;
            let mut tally = game.pending_loot_tally();
            tally.kills.insert((now, "goblin".into(), 2), 100);
            tally.kills.insert((now, "goblin".into(), 12), 100);
            tally.kills.insert((now, "kobold".into(), 12), 50);
            tally.drops.insert((now, "stethoscope".into(), true), 1);
            tally
                .drops
                .insert((now, "scroll_of_enchant_weapon".into(), false), 3);
        }
        let router = crate::metrics::metrics_routes(Arc::clone(&game), auth);
        let url = format!(
            "{}/api/metrics/rare-drops",
            crate::test_util::serve(router).await
        );

        let response = reqwest::get(&url).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let data: RareDrops = response.json().await.unwrap();
        assert_eq!(data.total_kills, 250);

        let (world, monsters) = game.loot_defs();
        let weapon = &data.items[0];
        let entry = world.entry("scroll_of_enchant_weapon").unwrap();
        assert_eq!(weapon.kills, 250);
        assert_eq!(weapon.other_drops, 3);
        let expected = 100.0 * f64::from(entry.chance_for(Some(2)))
            + 150.0 * f64::from(entry.chance_for(Some(12)));
        assert!((weapon.expected_kill_drops - expected).abs() < 1e-9);

        let stethoscope = &data.items[2];
        let goblin_chance = monsters
            .get("goblin")
            .unwrap()
            .drop_entries()
            .iter()
            .find(|(id, _)| id == "stethoscope")
            .unwrap()
            .1;
        assert!(stethoscope.world_drop.is_none());
        assert!(stethoscope
            .monster_drops
            .iter()
            .any(|m| m.monster_type == "goblin"));
        assert_eq!(stethoscope.kills, 200);
        assert_eq!(stethoscope.kill_drops, 1);
        assert!((stethoscope.expected_kill_drops - 200.0 * f64::from(goblin_chance)).abs() < 1e-9);
    }
}
