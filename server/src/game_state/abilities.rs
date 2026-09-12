use super::GameState;
use crate::item_defs::{ArmorType, WeaponType};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, AbilityTimer, GUARDIAN_WARD_COOLDOWN_MS,
    GUARDIAN_WARD_DURATION_MS, GUARDIAN_WARD_RADIUS,
};
use onlinerpg_shared::inventory::EquipSlot;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct Abilities {
    wards: HashMap<PlayerId, Instant>,
    cooldowns: HashMap<(i64, AbilityId), Instant>,
}

impl Abilities {
    fn cooldown_ms(&self, character: i64, ability: AbilityId) -> u64 {
        self.cooldowns
            .get(&(character, ability))
            .map(|until| until.saturating_duration_since(Instant::now()).as_millis() as u64)
            .unwrap_or(0)
    }

    pub(super) fn guard(&self, player_id: &PlayerId, base: i32) -> i32 {
        if self
            .wards
            .get(player_id)
            .is_some_and(|until| *until > Instant::now())
        {
            base + base.max(0) / 10
        } else {
            base
        }
    }

    fn buff_message(&self, player_id: &PlayerId) -> ServerMessage {
        let buffs = self
            .wards
            .get(player_id)
            .filter(|until| **until > Instant::now())
            .map(|until| AbilityTimer {
                ability: AbilityId::GuardianWard,
                remaining_ms: until.saturating_duration_since(Instant::now()).as_millis() as u64,
            })
            .into_iter()
            .collect();
        ServerMessage::BuffUpdate { buffs }
    }
}

impl GameState {
    pub async fn ability_cooldown_message(&self, player_id: &PlayerId) -> ServerMessage {
        let character = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .map(|c| c.0);
        let remaining_ms = if let Some(character) = character {
            self.abilities
                .read()
                .await
                .cooldown_ms(character, AbilityId::GuardianWard)
        } else {
            0
        };
        ServerMessage::AbilityCooldowns {
            cooldowns: vec![AbilityTimer {
                ability: AbilityId::GuardianWard,
                remaining_ms,
            }],
        }
    }

    pub async fn use_ability(&self, player_id: &PlayerId, ability: AbilityId) {
        let result = self.try_guardian_ward(player_id, ability).await;
        match result {
            Ok((position, floor_level, targets)) => {
                self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
                    .await;
                for target in &targets {
                    self.send_buff_state(target).await;
                }
                self.send_direct_message_to_players_within_position(
                    &position,
                    floor_level,
                    super::EVENT_DELIVERY_RADIUS + GUARDIAN_WARD_RADIUS,
                    ServerMessage::AbilityUsed {
                        ability,
                        player_id: *player_id,
                        position,
                        floor_level,
                        targets,
                    },
                    None,
                )
                .await;
            }
            Err(reason) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::AbilityRejected { ability, reason },
                )
                .await;
                self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
                    .await;
            }
        }
    }

    async fn try_guardian_ward(
        &self,
        player_id: &PlayerId,
        ability: AbilityId,
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let inventories = self.inventories.read().await;
        let inv = inventories
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?;
        let main = inv
            .equipped
            .get(&EquipSlot::MainHand)
            .and_then(|i| self.item_defs.get(&i.item_def_id));
        let off = inv
            .equipped
            .get(&EquipSlot::OffHand)
            .and_then(|i| self.item_defs.get(&i.item_def_id));
        if !main.is_some_and(|d| {
            matches!(d.weapon_type, Some(WeaponType::Sword | WeaponType::Mace))
                && !d.is_two_handed()
        }) || !off.is_some_and(|d| d.armor_type == Some(ArmorType::Shield))
        {
            return Err(AbilityRejectReason::Equipment);
        }
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()))
            .ok_or(AbilityRejectReason::Unavailable)?;
        let parties = self.parties.read().await;
        let party = parties.party_of(player_id);
        let targets: Vec<PlayerId> = players
            .values()
            .filter(|p| {
                (p.id == *player_id || party.is_some_and(|party| party.members.contains(&p.id)))
                    && p.health > 0
                    && p.floor_level == caster.floor_level
                    && p.position.dist_xz_sq(&caster.position) <= GUARDIAN_WARD_RADIUS.powi(2)
            })
            .map(|p| p.id)
            .collect();
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, ability) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        let now = Instant::now();
        state.cooldowns.insert(
            (character, ability),
            now + Duration::from_millis(GUARDIAN_WARD_COOLDOWN_MS),
        );
        for target in &targets {
            state.wards.insert(
                *target,
                now + Duration::from_millis(GUARDIAN_WARD_DURATION_MS),
            );
        }
        Ok((caster.position, caster.floor_level, targets))
    }

    async fn send_buff_state(&self, player_id: &PlayerId) {
        let message = self.abilities.read().await.buff_message(player_id);
        self.send_direct_message(player_id, message).await;
        self.send_direct_message(player_id, self.effective_stats(player_id).await.into())
            .await;
    }

    pub(super) async fn clear_buffs(&self, player_id: &PlayerId) {
        let removed = self
            .abilities
            .write()
            .await
            .wards
            .remove(player_id)
            .is_some();
        if removed {
            self.send_buff_state(player_id).await;
        }
    }

    pub async fn tick_buffs(&self) {
        let expired = {
            let mut state = self.abilities.write().await;
            let now = Instant::now();
            state.cooldowns.retain(|_, until| *until > now);
            let expired: Vec<PlayerId> = state
                .wards
                .iter()
                .filter(|(_, until)| **until <= now)
                .map(|(id, _)| *id)
                .collect();
            for id in &expired {
                state.wards.remove(id);
            }
            expired
        };
        for id in expired {
            self.send_buff_state(&id).await;
        }
    }
}
