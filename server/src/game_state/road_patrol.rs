//! Road patrols: guards clear estate fences off roads, and the guard captain
//! sells first-offence seizures back (doc/ROAD_PATROL.md).

use super::{
    auth_db,
    combat::reachable_dist_sq,
    inventory::{rows_with_inserts, BagHold, BagInsert},
    trading::MAX_TRADE_DISTANCE,
    GameState,
};
use crate::auth::{unix_now, AuthService, ImpoundedStack, NewLetter, RoadSeizure};
use crate::npc_defs::npc_defs;
use crate::road_index::RoadIndex;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::fence::{self, FenceEdge};
use onlinerpg_shared::messages::{ImpoundedItem, PatrolSeizure};

/// How far around a guard one sweep reaches.
const SWEEP_RADIUS_M: f32 = 24.0;
const GUARD_CLASS: &str = "guard";
pub(crate) const GUARD_CAPTAIN: &str = "karl";

fn reclaim_fee(base_price: i64, quantity: u32) -> i64 {
    base_price * i64::from(quantity) / 2
}

fn seizure_letter(seizure: &RoadSeizure, captain: &str) -> String {
    if seizure.forfeited() {
        format!(
            "The road patrol is pulling your fences off the public road. This is offence {k}; \
             they are forfeit.\n\
             순찰대가 공용 도로를 막은 목책을 철거하고 있습니다. {k}번째 위반이라 몰수합니다.\n\
             — {captain}, Captain of the Guard",
            k = seizure.offence,
        )
    } else {
        format!(
            "The road patrol is pulling your fences off the public road. I hold them at my \
             post; pay half their price and they are yours again. Next time they are forfeit.\n\
             순찰대가 공용 도로를 막은 목책을 철거하고 있습니다. 제가 보관하니 값의 절반을 \
             내고 찾아가십시오. 다음부터는 몰수합니다.\n\
             — {captain}, Captain of the Guard"
        )
    }
}

impl GameState {
    pub fn set_roads(&self, roads: RoadIndex) {
        let _ = self.roads.set(roads);
    }

    async fn road_fences_near(&self, player_id: &PlayerId) -> Result<Vec<FenceEdge>, &'static str> {
        let position = {
            let players = self.players.read().await;
            let guard = players.get(player_id).ok_or("Character not found.")?;
            let is_guard = guard.is_official_npc
                && npc_defs()
                    .get_by_npc_name(&guard.name)
                    .is_some_and(|def| def.class == GUARD_CLASS);
            if !is_guard {
                return Err("Only the guard clears the road.");
            }
            if guard.health == 0 || guard.floor_level != 0 {
                return Err("Stand on the road to clear it.");
            }
            guard.position
        };
        let Some(roads) = self.roads.get() else {
            return Ok(Vec::new());
        };
        let fences = self.fences.read().await;
        Ok(fences
            .within(&position, SWEEP_RADIUS_M)
            .into_iter()
            .filter(|fence| {
                let centre = fence.edge.center(0.0);
                roads.on_road(centre.x, centre.z)
            })
            .map(|fence| fence.edge)
            .collect())
    }

    pub async fn patrol_sweep(&self, player_id: &PlayerId, patrol_id: i64, auth: &AuthService) {
        let (warned, seized) = match self.try_patrol_sweep(player_id, patrol_id, auth).await {
            Ok(result) => result,
            Err(reason) => {
                tracing::warn!(?player_id, reason, "patrol sweep refused");
                Default::default()
            }
        };
        self.send_direct_message(
            player_id,
            ServerMessage::PatrolSweepResult { warned, seized },
        )
        .await;
    }

    async fn try_patrol_sweep(
        &self,
        player_id: &PlayerId,
        patrol_id: i64,
        auth: &AuthService,
    ) -> Result<(Vec<String>, Vec<PatrolSeizure>), &'static str> {
        let edges = self.road_fences_near(player_id).await?;
        if edges.is_empty() {
            return Ok(Default::default());
        }
        let db = auth.clone();
        let enforce_from = auth_db(move || db.road_patrol_enforce_from())
            .await
            .map_err(|_| "Patrol records are unavailable.")?;
        let db = auth.clone();
        if unix_now() < enforce_from {
            let warned = auth_db(move || db.road_fence_owners(&edges))
                .await
                .map_err(|_| "Patrol records are unavailable.")?;
            return Ok((warned, Vec::new()));
        }
        let _persistence = self.persistence_lock.lock().await;
        let seizures = auth_db(move || db.seize_road_fences(&edges, patrol_id))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Failed to seize road fences");
                "Patrol records are unavailable."
            })?;
        let removed: Vec<FenceEdge> = seizures
            .iter()
            .flat_map(|seizure| seizure.edges.iter().copied())
            .collect();
        if !removed.is_empty() {
            let mut fences = self.fences.write().await;
            let groups = fences.drop_edges(&removed);
            let mut cache = self.passability_write();
            for (key, group) in groups {
                fence::sync_passability(&mut cache, &key, &group);
            }
            drop(cache);
            self.publish_subject_change(ServerMessage::FenceVisibility {
                added: vec![],
                removed,
            });
        }
        drop(_persistence);
        for seizure in &seizures {
            tracing::info!(
                owner = %seizure.owner_name,
                fences = seizure.edges.len(),
                offence = seizure.offence,
                forfeited = seizure.forfeited(),
                "road patrol seized fences"
            );
            if seizure.new_offence {
                self.send_seizure_letter(seizure, auth).await;
            }
        }
        Ok((
            Vec::new(),
            seizures
                .iter()
                .map(|seizure| PatrolSeizure {
                    owner: seizure.owner_name.clone(),
                    fences: seizure.edges.len() as u32,
                    forfeited: seizure.forfeited(),
                })
                .collect(),
        ))
    }

    async fn send_seizure_letter(&self, seizure: &RoadSeizure, auth: &AuthService) {
        let Some(captain) = npc_defs().npc_name_by_id(GUARD_CAPTAIN) else {
            return;
        };
        let captain = captain.to_string();
        let body = seizure_letter(seizure, &captain);
        let recipient = seizure.owner_name.clone();
        let game_seconds = self.current_total_game_seconds();
        let db = auth.clone();
        let sent = auth_db(move || {
            let sender_id = db
                .resolve_character_brief(&captain)?
                .map_or(0, |(id, _)| id);
            db.send_mail(&NewLetter {
                sender_id,
                sender: &captain,
                recipient: &recipient,
                body: &body,
                game_seconds,
            })
        })
        .await;
        match sent {
            Ok(Ok(delivered)) => self.notify_unread_mail(delivered.character_id, auth).await,
            Ok(Err(_)) => {}
            Err(error) => tracing::warn!(%error, "Failed to send seizure letter"),
        }
    }

    async fn at_guard_captain(&self, player_id: &PlayerId, npc_player_id: &PlayerId) -> bool {
        let players = self.players.read().await;
        let (Some(player), Some(npc)) = (players.get(player_id), players.get(npc_player_id)) else {
            return false;
        };
        npc.is_official_npc
            && npc_defs()
                .get_by_npc_name(&npc.name)
                .is_some_and(|def| def.id == GUARD_CAPTAIN)
            && reachable_dist_sq(
                player.position,
                player.floor_level,
                npc.position,
                npc.floor_level,
            )
            .is_some_and(|dist_sq| dist_sq <= MAX_TRADE_DISTANCE * MAX_TRADE_DISTANCE)
            && !self.is_npc_asleep(&npc.name)
    }

    fn priced(&self, stacks: &[ImpoundedStack]) -> Vec<ImpoundedItem> {
        stacks
            .iter()
            .map(|stack| ImpoundedItem {
                item_def_id: stack.item_def_id.clone(),
                quantity: stack.quantity,
                fee: reclaim_fee(
                    self.item_defs
                        .get(&stack.item_def_id)
                        .and_then(|def| def.base_price)
                        .unwrap_or(0),
                    stack.quantity,
                ),
            })
            .collect()
    }

    pub async fn impound_action(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        reclaim: bool,
        auth: &AuthService,
    ) {
        let result = self
            .try_impound_action(player_id, npc_player_id, reclaim, auth)
            .await;
        let (items, error) = match result {
            Ok(items) => (items, None),
            Err(reason) => (Vec::new(), Some(reason.to_string())),
        };
        self.send_direct_message(
            player_id,
            ServerMessage::ImpoundState {
                npc_player_id: *npc_player_id,
                reclaimed: reclaim && error.is_none(),
                items,
                error,
            },
        )
        .await;
    }

    async fn try_impound_action(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        reclaim: bool,
        auth: &AuthService,
    ) -> Result<Vec<ImpoundedItem>, &'static str> {
        if !self.at_guard_captain(player_id, npc_player_id).await {
            return Err("Visit the Captain of the Guard to reclaim seized goods.");
        }
        let character_id = self
            .character_id_of(player_id)
            .await
            .ok_or("Character not found.")?;
        let read = |db: AuthService| async move {
            auth_db(move || db.impounded_items(character_id))
                .await
                .map_err(|error| {
                    tracing::warn!(%error, "Failed to read impound");
                    "Impound records are unavailable."
                })
        };
        if !reclaim {
            return Ok(self.priced(&read(auth.clone()).await?));
        }
        let _persistence = self.persistence_lock.lock().await;
        let stacks = read(auth.clone()).await?;
        let items = self.priced(&stacks);
        if items.is_empty() {
            return Err("Nothing of yours is held here.");
        }
        if self.reject_if_trading(player_id, "reclaim goods").await {
            return Err("Finish your player trade first.");
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or("Character not found.")?;
        let fee: i64 = items.iter().map(|item| item.fee).sum();
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let mut inserts = Vec::with_capacity(items.len());
        for item in &items {
            let stackable = self.item_defs.stackable(&item.item_def_id);
            let mut insert = BagInsert::one(stackable, &item.item_def_id, 0, 0);
            insert.quantity = item.quantity;
            insert.first_instance_id = self.reserve_instance_ids(insert.ids_reserved()).await;
            inserts.push(insert);
        }
        let rows = {
            let inventories = self.inventories.read().await;
            let inventory = inventories.get(player_id).ok_or("Inventory not found.")?;
            let added: f32 = items
                .iter()
                .map(|item| self.item_defs.weight(&item.item_def_id) * item.quantity as f32)
                .sum();
            if self.calc_total_weight(inventory, armor_mult) + added > max_weight {
                return Err("Your bag is too heavy to carry these.");
            }
            rows_with_inserts(inventory, &inserts)
        };
        {
            let mut gold = self.player_gold.write().await;
            let balance = gold.get_mut(player_id).ok_or("Gold balance not found.")?;
            if *balance < fee {
                return Err("You do not have enough gold.");
            }
            *balance -= fee;
            character.gold = *balance;
        }
        let db = auth.clone();
        let outcome = auth_db(move || db.reclaim_impounds(&character, &rows, &stacks))
            .await
            .map_err(|error| {
                tracing::warn!(%error, "Failed to reclaim impound");
                "The reclaim could not be saved. Nothing was charged."
            })
            .flatten();
        if outcome.is_err() {
            self.adjust_gold(player_id, fee).await;
        }
        self.finish_bag_write(player_id, BagHold::default(), &inserts, outcome)
            .await?;
        self.mark_dirty(player_id).await;
        self.send_gold_update(player_id).await;
        tracing::info!(?player_id, fee, "impound reclaimed");
        Ok(Vec::new())
    }
}
