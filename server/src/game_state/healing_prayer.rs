use super::abilities::MonsterTarget;
use super::combat::{reachable_dist_sq, wall_between, StruckMonster};
use super::GameState;
use crate::auth::AuthService;
use crate::types::{CharacterClass, PlayerId, ServerMessage};
use onlinerpg_shared::ability::{
    healing_prayer_amount, AbilityId, AbilityRejectReason, HEALING_PRAYER_COOLDOWN_MS,
    HEALING_PRAYER_DICE, HEALING_PRAYER_RANGE,
};
use std::time::Duration;

enum Target {
    Player(PlayerId),
    Monster(String, MonsterTarget),
}

struct Prayer {
    target: Target,
    amount: u32,
    caster_name: String,
}

impl GameState {
    /// Heal a player, the caster by default, or harm an undead monster.
    pub(super) async fn healing_prayer(
        &self,
        player_id: &PlayerId,
        monster_id: Option<&str>,
        target_player_id: Option<PlayerId>,
        auth: Option<&AuthService>,
    ) {
        self.stop_bed_rest(player_id).await;
        match self
            .try_healing_prayer(player_id, monster_id, target_player_id)
            .await
        {
            Ok(prayer) => {
                self.log_ability_use(player_id, AbilityId::HealingPrayer, 1)
                    .await;
                self.mark_dirty(player_id).await;
                self.send_mana_update(player_id).await;
                self.answer_prayer(player_id, prayer, auth).await;
            }
            Err(reason) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::AbilityRejected {
                        ability: AbilityId::HealingPrayer,
                        reason,
                    },
                )
                .await;
            }
        }
        self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
            .await;
    }

    async fn try_healing_prayer(
        &self,
        player_id: &PlayerId,
        monster_id: Option<&str>,
        target_player_id: Option<PlayerId>,
    ) -> Result<Prayer, AbilityRejectReason> {
        let target = match (monster_id, target_player_id) {
            (Some(id), None) => {
                let monster = self
                    .live_monster_target(id)
                    .await
                    .ok_or(AbilityRejectReason::Unavailable)?;
                if !self
                    .monster_defs
                    .get(&monster.monster_type)
                    .is_some_and(|def| def.undead)
                {
                    return Err(AbilityRejectReason::NotUndead);
                }
                Target::Monster(id.to_owned(), monster)
            }
            (None, id) => Target::Player(id.unwrap_or(*player_id)),
            (Some(_), Some(_)) => return Err(AbilityRejectReason::Unavailable),
        };
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| {
                p.class == CharacterClass::Priest
                    && p.is_damageable(Self::now_ms())
                    && !p.is_mounted()
            })
            .ok_or(AbilityRejectReason::Unavailable)?;
        let (character, wis) = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .map(|(id, _, attrs)| (*id, attrs.wis))
            .ok_or(AbilityRejectReason::Unavailable)?;
        // Where to reach for, unless praying for yourself.
        let reach = match &target {
            Target::Player(id) => {
                let player = players
                    .get(id)
                    .filter(|p| p.health > 0 && (id == player_id || !p.invisible))
                    .ok_or(AbilityRejectReason::Unavailable)?;
                if player.health >= player.max_health {
                    return Err(AbilityRejectReason::FullHealth);
                }
                (id != player_id).then_some((player.position, player.floor_level, Some(player)))
            }
            Target::Monster(_, monster) => {
                Some((monster.surface(&caster.position), monster.floor_level, None))
            }
        };
        if let Some((position, floor_level, seated)) = reach {
            let distance =
                reachable_dist_sq(caster.position, caster.floor_level, position, floor_level)
                    .ok_or(AbilityRejectReason::Unavailable)?;
            if distance > HEALING_PRAYER_RANGE.powi(2) {
                return Err(AbilityRejectReason::OutOfRange);
            }
            let furniture = match seated {
                Some(player) => self.furniture_passability(player).await,
                None => None,
            };
            let blocked = {
                let passability = self.passability_read();
                wall_between(
                    furniture.as_ref().unwrap_or(&passability),
                    caster.position,
                    position,
                    floor_level,
                    true,
                )
            };
            if blocked {
                return Err(AbilityRejectReason::Blocked);
            }
        }
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::HealingPrayer) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        self.mana
            .write()
            .await
            .get_mut(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .spend(AbilityId::HealingPrayer.mana_cost())?;
        state.start_cooldown(
            character,
            AbilityId::HealingPrayer,
            HEALING_PRAYER_COOLDOWN_MS,
        );
        let roll = crate::game::combat::roll_dice(HEALING_PRAYER_DICE);
        Ok(Prayer {
            target,
            amount: healing_prayer_amount(roll, caster.level, wis),
            caster_name: caster.name.clone(),
        })
    }

    async fn answer_prayer(
        &self,
        player_id: &PlayerId,
        prayer: Prayer,
        auth: Option<&AuthService>,
    ) {
        match prayer.target {
            Target::Player(target_id) => {
                let Some((amount, target)) = self
                    .heal_player(&target_id, prayer.amount, "healing_prayer", |_| {})
                    .await
                else {
                    return;
                };
                self.publish_nearby(
                    &target.position,
                    target.floor_level,
                    ServerMessage::HealingPrayed {
                        player_id: *player_id,
                        target_player_id: Some(target_id),
                        monster_id: None,
                        amount,
                    },
                    None,
                )
                .await;
                // Healing a fighter is fighting: no MP regen behind the lines.
                if Self::in_combat(&target) {
                    self.mark_in_combat(player_id).await;
                }
            }
            Target::Monster(id, monster) => {
                self.reveal(player_id).await;
                self.mark_in_combat(player_id).await;
                self.publish_nearby(
                    &monster.position,
                    monster.floor_level,
                    ServerMessage::HealingPrayed {
                        player_id: *player_id,
                        target_player_id: None,
                        monster_id: Some(id.clone()),
                        amount: prayer.amount,
                    },
                    None,
                )
                .await;
                self.damage_monster(
                    player_id,
                    &prayer.caster_name,
                    StruckMonster {
                        monster_id: &id,
                        monster_type: &monster.monster_type,
                        position: monster.position,
                        floor_level: monster.floor_level,
                        level_override: monster.level_override,
                    },
                    prayer.amount,
                    Duration::ZERO,
                    auth,
                )
                .await;
            }
        }
    }
}
