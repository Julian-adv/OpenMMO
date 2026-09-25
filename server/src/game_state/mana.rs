use super::GameState;
use crate::types::{Player, PlayerId, ServerMessage};
use onlinerpg_shared::ability::AbilityRejectReason;
use onlinerpg_shared::mana::{mana_regen_amount, max_mana, MANA_REGEN_DELAY_MS};
use std::time::Duration;
use tokio::time::Instant;

pub(super) struct ManaData {
    pub mana: u32,
    pub max_mana: u32,
    regen_after: Instant,
}

impl ManaData {
    fn message(&self) -> ServerMessage {
        ServerMessage::ManaUpdate {
            mana: self.mana,
            max_mana: self.max_mana,
        }
    }

    pub(super) fn spend(&mut self, amount: u32) -> Result<(), AbilityRejectReason> {
        self.mana = self
            .mana
            .checked_sub(amount)
            .ok_or(AbilityRejectReason::NotEnoughMana)?;
        if amount > 0 {
            self.delay_regeneration();
        }
        Ok(())
    }

    fn delay_regeneration(&mut self) {
        self.regen_after = Instant::now() + Duration::from_millis(MANA_REGEN_DELAY_MS);
    }

    fn recalculate(&mut self, player: &Player, wis: u8) -> bool {
        let previous = (self.mana, self.max_mana);
        self.max_mana = max_mana(&player.class, wis, player.level);
        self.mana = self.mana.min(self.max_mana);
        previous != (self.mana, self.max_mana)
    }
}

impl GameState {
    pub(crate) async fn register_mana(
        &self,
        player: &Player,
        wis: u8,
        saved: Option<u32>,
    ) -> Option<ServerMessage> {
        if player.is_official_npc {
            return None;
        }
        let maximum = max_mana(&player.class, wis, player.level);
        let data = ManaData {
            mana: saved.unwrap_or(maximum).min(maximum),
            max_mana: maximum,
            regen_after: Instant::now() + Duration::from_millis(MANA_REGEN_DELAY_MS),
        };
        let message = data.message();
        self.mana.write().await.insert(player.id, data);
        Some(message)
    }

    pub(super) async fn send_mana_update(&self, player_id: &PlayerId) {
        let mana = self.mana.read().await;
        if let Some(data) = mana.get(player_id) {
            self.send_direct_message(player_id, data.message()).await;
        }
    }

    pub(super) async fn delay_mana_regeneration(&self, player_id: &PlayerId) {
        if let Some(data) = self.mana.write().await.get_mut(player_id) {
            data.delay_regeneration();
        }
    }

    pub(super) async fn refresh_player_mana(&self, player_id: &PlayerId) {
        let changed = {
            let players = self.players.read().await;
            let chars = self.player_characters.read().await;
            let mut mana = self.mana.write().await;
            match (
                players.get(player_id),
                chars.get(player_id),
                mana.get_mut(player_id),
            ) {
                (Some(player), Some((_, _, attrs)), Some(data)) => {
                    data.recalculate(player, attrs.wis)
                }
                _ => false,
            }
        };
        if changed {
            self.mark_dirty(player_id).await;
            self.send_mana_update(player_id).await;
        }
    }

    pub(super) async fn tick_mana_regeneration(&self) {
        let mut changed = Vec::new();
        {
            let players = self.players.read().await;
            let chars = self.player_characters.read().await;
            let mut mana = self.mana.write().await;
            let now = Instant::now();
            let now_ms = Self::now_ms();
            for (id, data) in mana.iter_mut() {
                let (Some(player), Some((_, _, attrs))) = (players.get(id), chars.get(id)) else {
                    continue;
                };
                let recalculated = data.recalculate(player, attrs.wis);
                let before = data.mana;
                if player.is_damageable(now_ms)
                    && now_ms.saturating_sub(player.last_combat_at) >= super::OUT_OF_COMBAT_MS
                    && now >= data.regen_after
                {
                    let multiplier = if now
                        >= data.regen_after
                            + Duration::from_millis(onlinerpg_shared::mana::MANA_REGEN_INTERVAL_MS)
                    {
                        self.bed_rest_multiplier(player).await
                    } else {
                        1
                    };
                    data.mana = data
                        .mana
                        .saturating_add(mana_regen_amount(attrs.wis, player.level) * multiplier)
                        .min(data.max_mana);
                }
                if recalculated || before != data.mana {
                    changed.push(*id);
                }
            }
        }
        for id in changed {
            self.mark_dirty(&id).await;
            self.send_mana_update(&id).await;
        }
    }
}
