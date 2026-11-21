//! The blesser turns plain teleport scrolls into blessed ones for gold
//! (doc/TELEPORTATION.md).

use super::inventory::{hold_by_def, rows_with_inserts, BagInsert};
use super::road_patrol::{is_registry_npc, NpcReach};
use super::{auth_db, GameState};
use crate::auth::AuthService;
use crate::metrics::GoldSink;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::bookmark::{BlessError, BLESSED_SCROLL, BLESSER, BLESSING_PRICE, SCROLL};

impl GameState {
    pub async fn bless_scrolls(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        quantity: u32,
        auth: &AuthService,
    ) {
        let result = self
            .try_bless_scrolls(player_id, npc_player_id, quantity, auth)
            .await;
        self.send_direct_message(
            player_id,
            ServerMessage::BlessResult {
                blessed: result.unwrap_or(0),
                error: result.err(),
            },
        )
        .await;
    }

    /// The blesser answers a visitor who asked for a blessing in words: the
    /// blessing window opens on their screen. A refusal goes back to her as a cue.
    pub async fn offer_blessing(&self, blesser_id: &PlayerId, target_id: &PlayerId) {
        let name = {
            let players = self.players.read().await;
            if !players
                .get(blesser_id)
                .is_some_and(|npc| is_registry_npc(npc, BLESSER))
            {
                return;
            }
            players
                .get(target_id)
                .filter(|target| !target.is_official_npc)
                .map(|target| target.name.clone())
        };
        let cue = match (
            &name,
            self.blesser_reach(target_id, blesser_id).await,
        ) {
            (None, _) | (_, Err(BlessError::TooFar)) => {
                "No blessing window was opened: that visitor is not standing before you."
                    .to_string()
            }
            (Some(_), Err(_)) => {
                "No blessing window was opened: you bless scrolls only at the chapel lectern by day. Tell them so kindly."
                    .to_string()
            }
            (Some(name), Ok(())) => {
                self.send_direct_message(
                    target_id,
                    ServerMessage::BlessingOffered {
                        npc_player_id: *blesser_id,
                    },
                )
                .await;
                format!(
                    "The blessing window is now open before {name}; they choose how many scrolls and pay there. Open it again whenever they ask."
                )
            }
        };
        self.rite_cue(blesser_id, cue).await;
    }

    /// Whether `npc_player_id` is the blesser, in reach of the player and at her post.
    async fn blesser_reach(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
    ) -> Result<(), BlessError> {
        self.at_registry_npc(player_id, npc_player_id, BLESSER)
            .await
            .map_err(|reach| match reach {
                NpcReach::TooFar => BlessError::TooFar,
                NpcReach::Asleep => BlessError::Asleep,
            })?;
        let occupied = self
            .players
            .read()
            .await
            .get(npc_player_id)
            .is_some_and(|npc| self.is_npc_occupied(&npc.name));
        if occupied {
            return Err(BlessError::OffDuty);
        }
        Ok(())
    }

    async fn try_bless_scrolls(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        quantity: u32,
        auth: &AuthService,
    ) -> Result<u32, BlessError> {
        self.blesser_reach(player_id, npc_player_id).await?;
        if quantity == 0 {
            return Err(BlessError::NoScrolls);
        }
        let _persistence = self.persistence_lock.lock().await;
        if self
            .reject_if_trading(player_id, "have scrolls blessed")
            .await
        {
            return Err(BlessError::Trading);
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or(BlessError::Failed)?;
        let price = BLESSING_PRICE * i64::from(quantity);
        if character.gold < price {
            return Err(BlessError::NotEnoughGold);
        }
        character.gold -= price;
        let insert = BagInsert {
            quantity,
            ..BagInsert::one(true, BLESSED_SCROLL, 0, self.next_instance_id().await)
        };
        let (hold, rows) = {
            let mut inventories = self.inventories.write().await;
            let inventory = inventories.get_mut(player_id).ok_or(BlessError::Failed)?;
            let hold = hold_by_def(&mut inventory.bag, SCROLL, quantity, false)
                .ok_or(BlessError::NoScrolls)?;
            (
                hold,
                rows_with_inserts(inventory, std::slice::from_ref(&insert)),
            )
        };
        let character_id = character.character_id;
        let db = auth.clone();
        let saved =
            auth_db(move || db.save_batch(&[character], &[(character_id, rows)], &[], &[], None))
                .await
                .map_err(|error| {
                    tracing::warn!(%error, "Failed to save a scroll blessing");
                    "blessing not saved"
                });
        self.finish_bag_write(player_id, hold, &[insert], saved)
            .await
            .map_err(|_| BlessError::Failed)?;
        self.adjust_gold(player_id, -price).await;
        self.send_gold_update(player_id).await;
        self.record_gold_sink(GoldSink::Blessing, quantity, price)
            .await;
        Ok(quantity)
    }
}
