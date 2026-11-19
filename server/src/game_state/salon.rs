//! Hair salon: the hairdresser changes a character's face, hair and colours for gold.

use super::{auth_db, GameState};
use crate::auth::AuthService;
use crate::metrics::GoldSink;
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::salon::{self, SalonError, HAIRDRESSER};
use onlinerpg_shared::CharacterAppearance;

impl GameState {
    pub async fn change_appearance(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        appearance: CharacterAppearance,
        auth: &AuthService,
    ) {
        let result = self
            .try_change_appearance(player_id, npc_player_id, appearance, auth)
            .await;
        self.send_direct_message(
            player_id,
            ServerMessage::AppearanceChangeResult {
                price: result.unwrap_or(0),
                error: result.err(),
            },
        )
        .await;
    }

    async fn try_change_appearance(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        wanted: CharacterAppearance,
        auth: &AuthService,
    ) -> Result<i64, SalonError> {
        if !self
            .at_registry_npc(player_id, npc_player_id, HAIRDRESSER)
            .await
        {
            return Err(SalonError::TooFar);
        }
        let (current, wanted) = {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or(SalonError::TooFar)?;
            // The registry sets an NPC's look at every join.
            if player.is_official_npc
                || !CharacterAppearance::is_styled(&player.class, player.gender)
            {
                return Err(SalonError::Unsupported);
            }
            (
                player.appearance,
                wanted.for_character(&player.class, player.gender),
            )
        };
        let price = salon::price(&current, &wanted);
        if price == 0 {
            return Err(SalonError::Unchanged);
        }
        let _persistence = self.persistence_lock.lock().await;
        if self.reject_if_trading(player_id, "change your look").await {
            return Err(SalonError::Trading);
        }
        let mut character = self
            .get_player_save_data(player_id)
            .await
            .ok_or(SalonError::Failed)?;
        {
            let mut gold = self.player_gold.write().await;
            let balance = gold.get_mut(player_id).ok_or(SalonError::Failed)?;
            if *balance < price {
                return Err(SalonError::NotEnoughGold);
            }
            *balance -= price;
            character.gold = *balance;
        }
        let db = auth.clone();
        if let Err(error) = auth_db(move || db.change_appearance(&character, &wanted)).await {
            tracing::warn!(%error, "Failed to save a salon appearance change");
            self.adjust_gold(player_id, price).await;
            return Err(SalonError::Failed);
        }
        let at = {
            let mut players = self.players.write().await;
            players.get_mut(player_id).map(|player| {
                player.appearance = wanted;
                (player.position, player.floor_level)
            })
        };
        self.send_gold_update(player_id).await;
        self.record_gold_sink(GoldSink::Salon, 1, price).await;
        if let Some((position, floor_level)) = at {
            // The owner is inside their own radius, so this reaches them too.
            self.publish_nearby(
                &position,
                floor_level,
                ServerMessage::PlayerAppearanceChanged {
                    player_id: *player_id,
                    appearance: wanted,
                },
                None,
            )
            .await;
        }
        Ok(price)
    }
}
