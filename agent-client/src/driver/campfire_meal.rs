use std::time::Duration;

use onlinerpg_shared::hunger::CAMPFIRE_GRILL_RADIUS;
use onlinerpg_shared::schedule::ScheduleEntry;
use onlinerpg_shared::{ClientMessage, Position};
use tokio::time::Instant;

use crate::item_defs;
use crate::state::SharedState;

#[derive(Default)]
enum Step {
    #[default]
    Cook,
    Eat(String),
    Finished,
}

#[derive(Default)]
pub(super) struct CampfireMeal {
    step: Step,
    retry_at: Option<Instant>,
}

impl CampfireMeal {
    pub(super) async fn tick(
        &mut self,
        s: &mut SharedState,
        entry: &ScheduleEntry,
    ) -> anyhow::Result<()> {
        if matches!(self.step, Step::Finished)
            || self.retry_at.is_some_and(|at| Instant::now() < at)
            || !s.in_game
            || s.self_fishing
            || s.trade_busy
        {
            return Ok(());
        }
        let Some(player) = s.self_player.as_ref() else {
            return Ok(());
        };
        let target = Position {
            x: entry.pos[0],
            y: entry.pos[1],
            z: entry.pos[2],
        };
        if player.health == 0
            || player.mount.is_some()
            || player.object_type.is_some()
            || s.self_floor_level != 0
            || player.position.dist_xz_sq(&target) > 1.0
        {
            return Ok(());
        }
        self.retry_at = Some(Instant::now() + Duration::from_secs(10));
        if let Step::Eat(cooked_id) = &self.step {
            if let Some(item) = s
                .self_bag
                .iter()
                .find(|item| item.item_def_id == *cooked_id && item.quantity > 0 && !item.locked)
            {
                let instance_id = item.instance_id;
                s.send_background_command(ClientMessage::UseItem { instance_id })
                    .await?;
                self.step = Step::Finished;
            } else {
                self.step = Step::Cook;
            }
            return Ok(());
        }

        let has_fire = s.campfires.values().any(|fire| {
            fire.floor_level == s.self_floor_level
                && fire.position.dist_xz_sq(&player.position) <= CAMPFIRE_GRILL_RADIUS.powi(2)
        });
        if !has_fire {
            let command = if player.is_official_npc {
                Some(ClientMessage::ChatMessage {
                    message: "/light_campfire".into(),
                })
            } else {
                s.self_bag
                    .iter()
                    .find(|item| {
                        item.item_def_id == "campfire_kit" && item.quantity > 0 && !item.locked
                    })
                    .map(|item| ClientMessage::UseItem {
                        instance_id: item.instance_id,
                    })
            };
            if let Some(command) = command {
                s.send_background_command(command).await?;
            }
            return Ok(());
        }

        let fish = s
            .self_bag
            .iter()
            .filter(|item| item.quantity > 0 && !item.locked)
            .filter_map(|item| {
                let def = item_defs::get(&item.item_def_id)?;
                Some((
                    item.instance_id,
                    def.grills_into.as_ref()?,
                    def.base_price.unwrap_or(i64::MAX),
                ))
            })
            .min_by_key(|(_, _, price)| *price);
        if let Some((instance_id, cooked_id, _)) = fish {
            let cooked_id = cooked_id.clone();
            s.send_background_command(ClientMessage::UseItem { instance_id })
                .await?;
            self.step = Step::Eat(cooked_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::{test_player, test_state};
    use onlinerpg_shared::hunger::Campfire;
    use onlinerpg_shared::inventory::ItemInstance;
    use onlinerpg_shared::ServerMessage;

    fn item(instance_id: u64, id: &str) -> ItemInstance {
        ItemInstance {
            instance_id,
            item_def_id: id.into(),
            quantity: 1,
            enchant: 0,
            cape_color: None,
            cape_texture: None,
            locked: false,
        }
    }

    fn breakfast() -> (
        SharedState,
        tokio::sync::mpsc::Receiver<ClientMessage>,
        ScheduleEntry,
    ) {
        let (mut state, rx) = test_state();
        let mut player = test_player(100.0, 50.0);
        player.is_official_npc = true;
        state.self_player_id = Some(player.id);
        state.self_player = Some(player);
        state.in_game = true;
        let entry = ScheduleEntry {
            pos: [100.0, 0.0, 50.0],
            action: Some(onlinerpg_shared::schedule::CAMPFIRE_MEAL_ACTION.into()),
            ..Default::default()
        };
        (state, rx, entry)
    }

    fn light_fire(state: &mut SharedState) {
        state.push_event(ServerMessage::CampfireSpawned {
            campfire: Campfire {
                id: 1,
                position: Position {
                    x: 101.5,
                    y: 0.0,
                    z: 50.0,
                },
                floor_level: 0,
            },
        });
    }

    #[tokio::test(start_paused = true)]
    async fn breakfast_lights_a_fire_cooks_a_cheap_fish_and_eats_only_once() {
        let (mut state, mut rx, entry) = breakfast();
        state.self_bag = vec![item(1, "trophy_raw_minnow"), item(2, "raw_minnow")];
        let mut meal = CampfireMeal::default();
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::ChatMessage { message }) if message == "/light_campfire")
        );
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(rx.try_recv().is_err());
        light_fire(&mut state);
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::UseItem { instance_id: 2 })
        ));
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(
            rx.try_recv().is_err(),
            "wait for the server to cook the fish"
        );

        state.self_bag[1] = item(3, "grilled_minnow");
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::UseItem { instance_id: 3 })
        ));
        state.self_bag.pop();
        tokio::time::advance(Duration::from_secs(60)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(
            rx.try_recv().is_err(),
            "rest after one fish, preserving the trophy"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn breakfast_waits_for_usable_fish_and_retries_an_interrupted_grill() {
        let (mut state, mut rx, entry) = breakfast();
        light_fire(&mut state);
        let mut meal = CampfireMeal::default();
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(rx.try_recv().is_err());
        let mut fish = item(4, "raw_perch");
        fish.locked = true;
        state.self_bag.push(fish);
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(rx.try_recv().is_err());
        state.self_bag[0].locked = false;
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::UseItem { instance_id: 4 })
        ));

        state.push_event(ServerMessage::GrillEnded {
            grilled_item_def_id: None,
        });
        state.push_event(ServerMessage::CampfireRemoved { campfire_id: 1 });
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(
            rx.try_recv().is_err(),
            "do not eat raw fish after the fire goes out"
        );
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(
            matches!(rx.try_recv(), Ok(ClientMessage::ChatMessage { message }) if message == "/light_campfire")
        );
        light_fire(&mut state);
        tokio::time::advance(Duration::from_secs(10)).await;
        meal.tick(&mut state, &entry).await.unwrap();
        assert!(matches!(
            rx.try_recv(),
            Ok(ClientMessage::UseItem { instance_id: 4 })
        ));
    }
}
