use super::GameState;
use crate::item_defs::{ArmorType, WeaponType};
use crate::types::{PlayerId, ServerMessage};
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, AbilityTimer, BOW_MARK_COOLDOWN_MS, BOW_MARK_DURATION_MS,
    GUARDIAN_WARD_COOLDOWN_MS, GUARDIAN_WARD_DURATION_MS, GUARDIAN_WARD_RADIUS,
    RADIANCE_COOLDOWN_MS, RADIANCE_DURATION_MS,
};
use onlinerpg_shared::inventory::EquipSlot;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::Instant;

#[derive(Default)]
pub(super) struct Abilities {
    wards: HashMap<PlayerId, Instant>,
    radiances: HashMap<PlayerId, Instant>,
    marks: HashMap<PlayerId, BowMark>,
    cooldowns: HashMap<(i64, AbilityId), Instant>,
}

struct BowMark {
    monster_id: String,
    floor_level: i8,
    until: Instant,
}

impl Abilities {
    pub(super) fn marks_target(&self, player_id: &PlayerId, monster_id: &str) -> bool {
        self.marks
            .get(player_id)
            .is_some_and(|mark| mark.monster_id == monster_id && mark.until > Instant::now())
    }

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
        let now = Instant::now();
        let buffs = [
            (AbilityId::GuardianWard, self.wards.get(player_id)),
            (AbilityId::Radiance, self.radiances.get(player_id)),
            (
                AbilityId::BowMark,
                self.marks.get(player_id).map(|mark| &mark.until),
            ),
        ]
        .into_iter()
        .filter_map(|(ability, until)| {
            until
                .filter(|until| **until > now)
                .map(|until| AbilityTimer {
                    ability,
                    remaining_ms: until.saturating_duration_since(now).as_millis() as u64,
                })
        })
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
        let mut cooldowns = {
            let state = self.abilities.read().await;
            [
                AbilityId::GuardianWard,
                AbilityId::Radiance,
                AbilityId::BowMark,
            ]
            .into_iter()
            .map(|ability| AbilityTimer {
                ability,
                remaining_ms: character
                    .map(|id| state.cooldown_ms(id, ability))
                    .unwrap_or(0),
            })
            .collect::<Vec<_>>()
        };
        cooldowns.push(AbilityTimer {
            ability: AbilityId::DaggerDoubleSlash,
            remaining_ms: match character {
                Some(id) => self.dagger_skill_cooldown_ms(id).await,
                None => 0,
            },
        });
        ServerMessage::AbilityCooldowns { cooldowns }
    }

    pub async fn use_ability(&self, player_id: &PlayerId, ability: AbilityId) {
        let result = match ability {
            AbilityId::GuardianWard => self.try_guardian_ward(player_id, ability).await,
            AbilityId::Radiance => self.try_radiance(player_id).await,
            AbilityId::BowMark | AbilityId::DaggerDoubleSlash => {
                Err(AbilityRejectReason::Unavailable)
            }
        };
        match result {
            Ok((position, floor_level, targets)) => {
                self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
                    .await;
                for target in &targets {
                    self.send_buff_state(target).await;
                }
                if ability == AbilityId::Radiance
                    && !self
                        .abilities
                        .read()
                        .await
                        .radiances
                        .contains_key(player_id)
                {
                    return;
                }
                self.send_direct_message_to_players_within_position(
                    &position,
                    floor_level,
                    super::EVENT_DELIVERY_RADIUS
                        + if ability == AbilityId::GuardianWard {
                            GUARDIAN_WARD_RADIUS
                        } else {
                            0.0
                        },
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

    pub async fn use_targeted_ability(
        &self,
        player_id: &PlayerId,
        ability: AbilityId,
        monster_id: Option<&str>,
    ) {
        if ability != AbilityId::BowMark {
            self.use_ability(player_id, ability).await;
            return;
        }
        match self.try_bow_mark(player_id, monster_id).await {
            Ok(()) => {
                self.send_bow_mark_state(player_id).await;
                self.send_buff_state(player_id).await;
            }
            Err(reason) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::AbilityRejected { ability, reason },
                )
                .await;
            }
        }
        self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
            .await;
    }

    async fn try_bow_mark(
        &self,
        player_id: &PlayerId,
        monster_id: Option<&str>,
    ) -> Result<(), AbilityRejectReason> {
        use super::combat::{reachable_dist_sq, wall_between};
        use crate::types::MonsterState;

        let monster_id = monster_id.ok_or(AbilityRejectReason::Unavailable)?;
        let (mut target_position, target_floor) = self
            .monsters
            .read()
            .await
            .get(monster_id)
            .filter(|monster| monster.state != MonsterState::Dead && monster.health > 0)
            .map(|monster| (monster.position, monster.floor_level))
            .ok_or(AbilityRejectReason::Unavailable)?;
        if let Some(position) = self.brain_position_now(monster_id).await {
            target_position = position;
        }
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()) && !p.mounted)
            .ok_or(AbilityRejectReason::Unavailable)?;
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        let inventories = self.inventories.read().await;
        let weapon = inventories
            .get(player_id)
            .and_then(|inv| inv.equipped.get(&EquipSlot::MainHand))
            .and_then(|item| self.item_defs.get(&item.item_def_id))
            .filter(|def| def.weapon_type == Some(WeaponType::Bow))
            .ok_or(AbilityRejectReason::Equipment)?;
        let range = weapon
            .weapon_range()
            .ok_or(AbilityRejectReason::Equipment)?;
        let distance = reachable_dist_sq(
            caster.position,
            caster.floor_level,
            target_position,
            target_floor,
        )
        .ok_or(AbilityRejectReason::Unavailable)?;
        if distance > range.powi(2) {
            return Err(AbilityRejectReason::OutOfRange);
        }
        if wall_between(
            &self.passability_read(),
            caster.position,
            target_position,
            caster.floor_level,
            true,
        ) {
            return Err(AbilityRejectReason::Unavailable);
        }
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::BowMark) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        let now = Instant::now();
        state.cooldowns.insert(
            (character, AbilityId::BowMark),
            now + Duration::from_millis(BOW_MARK_COOLDOWN_MS),
        );
        state.marks.insert(
            *player_id,
            BowMark {
                monster_id: monster_id.to_owned(),
                floor_level: target_floor,
                until: now + Duration::from_millis(BOW_MARK_DURATION_MS),
            },
        );
        Ok(())
    }

    async fn send_bow_mark_state(&self, player_id: &PlayerId) {
        let message = {
            let state = self.abilities.read().await;
            let mark = state
                .marks
                .get(player_id)
                .filter(|mark| mark.until > Instant::now());
            ServerMessage::BowMarkUpdate {
                monster_id: mark.map(|mark| mark.monster_id.clone()),
                remaining_ms: mark
                    .map(|mark| {
                        mark.until
                            .saturating_duration_since(Instant::now())
                            .as_millis() as u64
                    })
                    .unwrap_or(0),
            }
        };
        self.send_direct_message(player_id, message).await;
    }

    async fn try_radiance(
        &self,
        player_id: &PlayerId,
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()) && !p.mounted)
            .ok_or(AbilityRejectReason::Unavailable)?;
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::Radiance) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        let now = Instant::now();
        state.cooldowns.insert(
            (character, AbilityId::Radiance),
            now + Duration::from_millis(RADIANCE_COOLDOWN_MS),
        );
        if state
            .radiances
            .get(player_id)
            .is_some_and(|until| *until > now)
        {
            state.radiances.remove(player_id);
        } else {
            state.radiances.insert(
                *player_id,
                now + Duration::from_millis(RADIANCE_DURATION_MS),
            );
        }
        Ok((caster.position, caster.floor_level, vec![*player_id]))
    }

    async fn try_guardian_ward(
        &self,
        player_id: &PlayerId,
        ability: AbilityId,
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()))
            .ok_or(AbilityRejectReason::Unavailable)?;
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
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
        self.sync_radiance_state(player_id).await;
        let message = self.abilities.read().await.buff_message(player_id);
        self.send_direct_message(player_id, message).await;
        self.send_direct_message(player_id, self.effective_stats(player_id).await.into())
            .await;
    }

    async fn sync_radiance_state(&self, player_id: &PlayerId) {
        let changed = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            let enabled = self
                .abilities
                .read()
                .await
                .radiances
                .get(player_id)
                .is_some_and(|until| *until > Instant::now());
            if player.radiance_on == enabled {
                return;
            }
            player.radiance_on = enabled;
            (player.position, player.floor_level, enabled)
        };
        self.send_direct_message_to_players_within_position(
            &changed.0,
            changed.1,
            super::EVENT_DELIVERY_RADIUS,
            ServerMessage::PlayerRadianceToggled {
                player_id: *player_id,
                enabled: changed.2,
            },
            None,
        )
        .await;
    }

    pub(super) async fn clear_buffs(&self, player_id: &PlayerId) {
        let removed = {
            let mut state = self.abilities.write().await;
            let ward = state.wards.remove(player_id).is_some();
            let radiance = state.radiances.remove(player_id).is_some();
            let mark = state.marks.remove(player_id).is_some();
            ward || radiance || mark
        };
        if removed {
            self.send_bow_mark_state(player_id).await;
            self.send_buff_state(player_id).await;
        }
    }

    pub async fn tick_buffs(&self) {
        let marks: Vec<_> = self
            .abilities
            .read()
            .await
            .marks
            .iter()
            .map(|(id, mark)| (*id, mark.monster_id.clone(), mark.floor_level, mark.until))
            .collect();
        let mut invalid = HashMap::new();
        for (id, target, floor, until) in marks {
            let target_valid = self.monsters.read().await.get(&target).is_some_and(|m| {
                m.health > 0
                    && m.state != crate::types::MonsterState::Dead
                    && m.floor_level == floor
            });
            let caster_valid = self
                .players
                .read()
                .await
                .get(&id)
                .is_some_and(|p| p.health > 0 && p.floor_level == floor);
            if !target_valid || !caster_valid {
                invalid.insert(id, until);
            }
        }
        let expired = {
            let mut state = self.abilities.write().await;
            let now = Instant::now();
            state.cooldowns.retain(|_, until| *until > now);
            let mut expired: HashSet<PlayerId> = state
                .wards
                .iter()
                .chain(state.radiances.iter())
                .filter(|(_, until)| **until <= now)
                .map(|(id, _)| *id)
                .collect();
            state.wards.retain(|_, until| *until > now);
            state.radiances.retain(|_, until| *until > now);
            state.marks.retain(|id, mark| {
                let valid = mark.until > now && invalid.get(id) != Some(&mark.until);
                if !valid {
                    expired.insert(*id);
                }
                valid
            });
            expired
        };
        for id in expired {
            self.send_bow_mark_state(&id).await;
            self.send_buff_state(&id).await;
        }
    }
}
