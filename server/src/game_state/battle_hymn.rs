//! Bard Battle Hymn (doc/abilities/CLASS_SKILLS.md §6): ten unbroken seconds
//! of playing buff the bard and nearby party members for ten minutes.

use super::GameState;
use crate::types::{CharacterClass, PlayerId, ServerMessage};
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, BATTLE_HYMN_CAST_MS, BATTLE_HYMN_DURATION_MS,
    BATTLE_HYMN_RADIUS, BATTLE_HYMN_TUNES,
};
use onlinerpg_shared::messages::MUSIC_EMOTE;
use onlinerpg_shared::Position;
use rand::Rng;
use std::time::Duration;
use tokio::time::Instant;

/// A bard still playing toward the buff.
pub(super) struct Hymn {
    done_at: Instant,
}

impl Hymn {
    /// Reported as the skill's cooldown, so the quickslot shows the cast.
    pub(super) fn remaining_ms(&self) -> u64 {
        self.done_at
            .saturating_duration_since(Instant::now())
            .as_millis() as u64
    }
}

impl GameState {
    /// MP waits for the end; a second use while playing is refused.
    pub(super) async fn try_battle_hymn(
        &self,
        player_id: &PlayerId,
    ) -> Result<(Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let holds_instrument = self.holds_instrument(player_id).await;
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| {
                p.class == CharacterClass::Bard
                    && p.is_damageable(Self::now_ms())
                    && !p.is_mounted()
            })
            .ok_or(AbilityRejectReason::Unavailable)?;
        if !holds_instrument {
            return Err(AbilityRejectReason::Equipment);
        }
        let mut state = self.abilities.write().await;
        if state.hymns.contains_key(player_id) {
            return Err(AbilityRejectReason::Cooldown);
        }
        if self
            .mana
            .read()
            .await
            .get(player_id)
            .is_none_or(|data| data.mana < AbilityId::BattleHymn.mana_cost())
        {
            return Err(AbilityRejectReason::NotEnoughMana);
        }
        state.hymns.insert(
            *player_id,
            Hymn {
                done_at: Instant::now() + Duration::from_millis(BATTLE_HYMN_CAST_MS),
            },
        );
        Ok((caster.position, caster.floor_level, Vec::new()))
    }

    /// Take up the strum, or take it over from a tune already playing; the
    /// march itself is the clients' (`BattleHymnStarted`).
    pub(super) async fn strike_up_hymn(&self, player_id: &PlayerId) {
        if self.performing(player_id).await.is_some() {
            self.music_performances.write().await.remove(player_id);
            self.remove_live_instrument(player_id).await;
        } else {
            self.set_player_interaction(player_id, Some(MUSIC_EMOTE.to_string()), None)
                .await;
        }
        let Some((position, floor_level)) = self.performing(player_id).await else {
            self.interrupt_battle_hymn(player_id).await;
            return;
        };
        let tune = rand::thread_rng().gen_range(0..BATTLE_HYMN_TUNES);
        self.publish_nearby(
            &position,
            floor_level,
            ServerMessage::BattleHymnStarted {
                player_id: *player_id,
                position,
                floor_level,
                tune,
            },
            None,
        )
        .await;
    }

    async fn performing(&self, player_id: &PlayerId) -> Option<(Position, i8)> {
        self.players
            .read()
            .await
            .get(player_id)
            .filter(|p| p.health > 0 && p.object_type.as_deref() == Some(MUSIC_EMOTE))
            .map(|p| (p.position, p.floor_level))
    }

    pub async fn tick_battle_hymns(&self) {
        let now = Instant::now();
        let done: Vec<PlayerId> = self
            .abilities
            .read()
            .await
            .hymns
            .iter()
            .filter(|(_, hymn)| hymn.done_at <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in done {
            self.finish_battle_hymn(&id).await;
        }
    }

    /// The strum ends with the hymn, paid for or not.
    async fn finish_battle_hymn(&self, bard: &PlayerId) {
        let Some((position, floor_level)) = self.performing(bard).await else {
            self.interrupt_battle_hymn(bard).await;
            return;
        };
        if !self.holds_instrument(bard).await {
            self.clear_player_pose(bard, "hymn").await;
            return;
        }
        let paid = {
            let mut state = self.abilities.write().await;
            if state.hymns.remove(bard).is_none() {
                return;
            }
            self.mana
                .write()
                .await
                .get_mut(bard)
                .is_some_and(|data| data.spend(AbilityId::BattleHymn.mana_cost()).is_ok())
        };
        self.clear_player_pose(bard, "hymn").await;
        if !paid {
            self.send_direct_message(
                bard,
                ServerMessage::AbilityRejected {
                    ability: AbilityId::BattleHymn,
                    reason: AbilityRejectReason::NotEnoughMana,
                },
            )
            .await;
            return;
        }
        self.mark_dirty(bard).await;
        self.send_mana_update(bard).await;
        let listeners = self.hymn_listeners(bard, position, floor_level).await;
        self.hear_battle_hymn(&listeners).await;
        self.log_ability_use(bard, AbilityId::BattleHymn, listeners.len())
            .await;
        self.publish_nearby(
            &position,
            floor_level,
            ServerMessage::AbilityUsed {
                ability: AbilityId::BattleHymn,
                player_id: *bard,
                position,
                floor_level,
                targets: listeners,
            },
            None,
        )
        .await;
    }

    /// The bard and the living party members in range.
    async fn hymn_listeners(
        &self,
        bard: &PlayerId,
        position: Position,
        floor_level: i8,
    ) -> Vec<PlayerId> {
        let mut members = self.other_party_member_ids(bard).await;
        members.push(*bard);
        let players = self.players.read().await;
        members
            .into_iter()
            .filter(|id| {
                players.get(id).is_some_and(|p| {
                    p.health > 0
                        && p.floor_level == floor_level
                        && p.position.dist_xz_sq(&position) <= BATTLE_HYMN_RADIUS.powi(2)
                })
            })
            .collect()
    }

    /// One `hymn_until` per player, so another hymn only renews it. Official
    /// NPCs have no hunger entry and go without.
    async fn hear_battle_hymn(&self, listeners: &[PlayerId]) {
        let now = Instant::now();
        let until = now + Duration::from_millis(BATTLE_HYMN_DURATION_MS);
        let mut heard = Vec::new();
        {
            let mut hunger = self.hunger.write().await;
            for id in listeners {
                let Some(data) = hunger.get_mut(id) else {
                    continue;
                };
                let gained = !data.hymn_active(now);
                data.hymn_until = Some(until);
                heard.push((*id, gained.then(|| data.hunger_msg(now))));
            }
        }
        for (id, speed_change) in heard {
            if let Some(message) = speed_change {
                self.wake_player_attack(&id).await;
                self.send_direct_message(&id, message).await;
            }
            self.send_buff_update(&id).await;
        }
    }

    /// Moving, swinging, being hit or starting another tune before the end:
    /// no MP is spent and the bard may start again at once. True when a hymn
    /// was cut; the strum is the caller's to end.
    pub(super) async fn interrupt_battle_hymn(&self, player_id: &PlayerId) -> bool {
        if self
            .abilities
            .write()
            .await
            .hymns
            .remove(player_id)
            .is_none()
        {
            return false;
        }
        self.send_direct_message(
            player_id,
            ServerMessage::AbilityRejected {
                ability: AbilityId::BattleHymn,
                reason: AbilityRejectReason::Interrupted,
            },
        )
        .await;
        self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
            .await;
        true
    }
}
