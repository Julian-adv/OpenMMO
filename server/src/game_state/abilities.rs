use super::GameState;
use crate::item_defs::{ArmorType, WeaponType};
use crate::types::{CharacterClass, PlayerId, ServerMessage};
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, AbilityTimer, InspectedEquipment, InspectionResult,
    InspectionTarget, AUSCULTATION_COOLDOWN_MS, AUSCULTATION_RANGE, BOW_MARK_COOLDOWN_MS,
    BOW_MARK_DURATION_MS, GUARDIAN_WARD_COOLDOWN_MS, GUARDIAN_WARD_DURATION_MS,
    GUARDIAN_WARD_GUARD_BONUS, GUARDIAN_WARD_RADIUS, PRIMAL_ROAR_CAST_MS, PRIMAL_ROAR_COOLDOWN_MS,
    PRIMAL_ROAR_DURATION_MS, PRIMAL_ROAR_RADIUS, PRIMAL_ROAR_STAGGER_MS, PRIMAL_ROAR_STOMP_MS,
    RADIANCE_COOLDOWN_MS, RADIANCE_DURATION_MS, RAGE_COOLDOWN_MS, RAGE_DURATION_MS,
    RAGE_GUARD_PENALTY,
};
use onlinerpg_shared::inventory::{EquipSlot, PlayerInventory};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tokio::time::Instant;
use tracing::info;

#[derive(Default)]
pub(super) struct Abilities {
    wards: HashMap<PlayerId, Instant>,
    radiances: HashMap<PlayerId, Instant>,
    marks: HashMap<PlayerId, BowMark>,
    rages: HashMap<PlayerId, Instant>,
    roars: HashMap<PlayerId, Instant>,
    pub(super) hymns: HashMap<PlayerId, super::battle_hymn::Hymn>,
    cooldowns: HashMap<(i64, AbilityId), Instant>,
}

pub(super) struct MonsterTarget {
    pub(super) position: onlinerpg_shared::Position,
    pub(super) floor_level: i8,
    pub(super) monster_type: String,
    pub(super) level_override: Option<u8>,
    rotation: f32,
    body: Option<onlinerpg_shared::monster_body::BodyFootprint>,
}

impl MonsterTarget {
    /// The nearest point of its body to `from`, where reach is measured.
    pub(super) fn surface(&self, from: &onlinerpg_shared::Position) -> onlinerpg_shared::Position {
        onlinerpg_shared::monster_body::reach_point(self.body, self.position, self.rotation, from)
    }
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

    pub(super) fn start_cooldown(&mut self, character: i64, ability: AbilityId, ms: u64) {
        self.cooldowns.insert(
            (character, ability),
            Instant::now() + Duration::from_millis(ms),
        );
    }

    pub(super) fn cooldown_ms(&self, character: i64, ability: AbilityId) -> u64 {
        self.cooldowns
            .get(&(character, ability))
            .map(|until| until.saturating_duration_since(Instant::now()).as_millis() as u64)
            .unwrap_or(0)
    }

    pub(super) fn guard(&self, player_id: &PlayerId, base: i32) -> i32 {
        let warded = self
            .wards
            .get(player_id)
            .is_some_and(|until| *until > Instant::now());
        let ward = if warded { GUARDIAN_WARD_GUARD_BONUS } else { 0 };
        let rage = if self.raging(player_id) {
            RAGE_GUARD_PENALTY
        } else {
            0
        };
        base + ward - rage
    }

    pub(super) fn raging(&self, player_id: &PlayerId) -> bool {
        self.rages
            .get(player_id)
            .is_some_and(|until| *until > Instant::now())
    }

    /// Battle Hymn's `hymn_until` lives with the hunger multipliers.
    pub(super) fn buff_message(
        &self,
        player_id: &PlayerId,
        hymn_until: Option<Instant>,
    ) -> ServerMessage {
        let now = Instant::now();
        let buffs = [
            (AbilityId::GuardianWard, self.wards.get(player_id)),
            (AbilityId::Radiance, self.radiances.get(player_id)),
            (
                AbilityId::BowMark,
                self.marks.get(player_id).map(|mark| &mark.until),
            ),
            (AbilityId::Rage, self.rages.get(player_id)),
            (AbilityId::PrimalRoar, self.roars.get(player_id)),
            (AbilityId::BattleHymn, hymn_until.as_ref()),
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
    /// One journal line per successful cast, so skill use can be counted.
    pub(super) async fn log_ability_use(
        &self,
        player_id: &PlayerId,
        ability: AbilityId,
        targets: usize,
    ) {
        let Some((name, floor)) = self
            .players
            .read()
            .await
            .get(player_id)
            .map(|p| (p.name.clone(), p.floor_level))
        else {
            return;
        };
        info!(target: "ability_use", %name, ?ability, targets, floor, "ability cast");
    }

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
                AbilityId::Auscultation,
                AbilityId::Rage,
                AbilityId::PrimalRoar,
                AbilityId::ValhallaCharge,
                AbilityId::HealingPrayer,
            ]
            .into_iter()
            .map(|ability| AbilityTimer {
                ability,
                remaining_ms: character
                    .map(|id| state.cooldown_ms(id, ability))
                    .unwrap_or(0),
            })
            .chain([AbilityTimer {
                ability: AbilityId::BattleHymn,
                remaining_ms: state.hymns.get(player_id).map_or(0, |h| h.remaining_ms()),
            }])
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
        self.stop_bed_rest(player_id).await;
        let result = match ability {
            AbilityId::GuardianWard => self.try_guardian_ward(player_id).await,
            AbilityId::Radiance => self.try_radiance(player_id).await,
            AbilityId::Rage => self.try_rage(player_id).await,
            AbilityId::PrimalRoar => self.try_primal_roar(player_id).await,
            AbilityId::BattleHymn => self.try_battle_hymn(player_id).await,
            AbilityId::BowMark
            | AbilityId::DaggerDoubleSlash
            | AbilityId::Auscultation
            | AbilityId::ValhallaCharge
            | AbilityId::HealingPrayer => Err(AbilityRejectReason::Unavailable),
        };
        match result {
            Ok((position, floor_level, targets)) => {
                // Paid when the playing finishes.
                if ability == AbilityId::BattleHymn {
                    self.send_direct_message(
                        player_id,
                        self.ability_cooldown_message(player_id).await,
                    )
                    .await;
                    self.strike_up_hymn(player_id).await;
                    return;
                }
                if ability.mana_cost() > 0 {
                    self.mark_dirty(player_id).await;
                    self.send_mana_update(player_id).await;
                }
                self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
                    .await;
                for target in &targets {
                    self.send_buff_state(target).await;
                }
                if ability == AbilityId::PrimalRoar {
                    self.send_temp_health(player_id).await;
                    self.hold_player_attack(player_id, PRIMAL_ROAR_CAST_MS)
                        .await;
                    let game = self.clone();
                    let player_id = *player_id;
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(PRIMAL_ROAR_STOMP_MS)).await;
                        game.roar_at_monsters(&player_id).await;
                    });
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
                self.log_ability_use(player_id, ability, targets.len())
                    .await;
                self.publish_nearby(
                    &position,
                    floor_level,
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
        target_player_id: Option<PlayerId>,
        auth: Option<&crate::auth::AuthService>,
    ) {
        if ability == AbilityId::DaggerDoubleSlash {
            match (monster_id, target_player_id) {
                (Some(monster_id), None) => {
                    self.dagger_double_slash(player_id, monster_id.to_owned(), auth)
                        .await;
                }
                _ => self.use_ability(player_id, ability).await,
            }
            return;
        }
        if ability == AbilityId::HealingPrayer {
            self.healing_prayer(player_id, monster_id, target_player_id, auth)
                .await;
            return;
        }
        if let (AbilityId::ValhallaCharge, Some(monster_id), None) =
            (ability, monster_id, target_player_id)
        {
            self.valhalla_charge(player_id, monster_id, auth).await;
            return;
        }
        if ability == AbilityId::Auscultation {
            self.bump_action_version(player_id).await;
            self.stop_bed_rest(player_id).await;
            let result = self
                .try_auscultation(player_id, monster_id, target_player_id)
                .await;
            let message = match result {
                Ok(inspection) => {
                    self.log_ability_use(player_id, ability, 1).await;
                    ServerMessage::InspectionResult { inspection }
                }
                Err(reason) => ServerMessage::AbilityRejected { ability, reason },
            };
            self.send_direct_message(player_id, message).await;
            self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
                .await;
            return;
        }
        if ability != AbilityId::BowMark {
            self.use_ability(player_id, ability).await;
            return;
        }
        self.stop_bed_rest(player_id).await;
        match self.try_bow_mark(player_id, monster_id).await {
            Ok(()) => {
                self.log_ability_use(player_id, ability, 1).await;
                self.reveal(player_id).await;
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

    async fn try_auscultation(
        &self,
        player_id: &PlayerId,
        monster_id: Option<&str>,
        target_player_id: Option<PlayerId>,
    ) -> Result<InspectionResult, AbilityRejectReason> {
        use super::combat::{reachable_dist_sq, wall_between};
        use crate::types::MonsterState;

        let target = match (monster_id, target_player_id) {
            (Some(id), None) => InspectionTarget::Monster {
                monster_id: id.to_owned(),
            },
            (None, Some(id)) if id != *player_id => InspectionTarget::Player { player_id: id },
            _ => return Err(AbilityRejectReason::Unavailable),
        };
        let monster = if let InspectionTarget::Monster { monster_id } = &target {
            let mut monster = self
                .monsters
                .read()
                .await
                .get(monster_id)
                .filter(|m| m.health > 0 && m.state != MonsterState::Dead)
                .cloned()
                .ok_or(AbilityRejectReason::Unavailable)?;
            if let Some(position) = self.brain_position_now(monster_id).await {
                monster.position = position;
            }
            Some(monster)
        } else {
            None
        };
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()) && !p.is_mounted())
            .ok_or(AbilityRejectReason::Unavailable)?;
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        let inventories = self.inventories.read().await;
        if !inventories
            .get(player_id)
            .is_some_and(Self::wears_stethoscope)
        {
            return Err(AbilityRejectReason::Equipment);
        }
        let (position, floor, mut inspection) = match &target {
            InspectionTarget::Monster { .. } => {
                let monster = monster.as_ref().ok_or(AbilityRejectReason::Unavailable)?;
                let def = self
                    .monster_defs
                    .get(&monster.monster_type)
                    .ok_or(AbilityRejectReason::Unavailable)?;
                let equipment = def
                    .weapon
                    .iter()
                    .map(|id| InspectedEquipment {
                        slot: EquipSlot::MainHand,
                        item_def_id: id.clone(),
                        enchant: 0,
                    })
                    .collect();
                (
                    monster.position,
                    monster.floor_level,
                    InspectionResult {
                        target: target.clone(),
                        name: def.name.clone(),
                        level: u32::from(monster.level_override.unwrap_or(def.level)),
                        health: monster.health,
                        max_health: monster.max_health,
                        guard: i32::from(def.guard),
                        equipment,
                    },
                )
            }
            InspectionTarget::Player { player_id: id } => {
                let player = players
                    .get(id)
                    .filter(|p| p.is_damageable(Self::now_ms()) && !p.invisible)
                    .ok_or(AbilityRejectReason::Unavailable)?;
                let inv = inventories.get(id);
                let equipment = inv
                    .into_iter()
                    .flat_map(|inv| &inv.equipped)
                    .map(|(slot, item)| InspectedEquipment {
                        slot: *slot,
                        item_def_id: item.item_def_id.clone(),
                        enchant: item.enchant,
                    })
                    .collect();
                let guard = chars
                    .get(id)
                    .map(|(_, _, a)| i32::from(a.guard))
                    .unwrap_or(10)
                    + inv.map(|inv| self.equipped_guard(inv)).unwrap_or(0);
                (
                    player.position,
                    player.floor_level,
                    InspectionResult {
                        target: target.clone(),
                        name: player.name.clone(),
                        level: player.level,
                        health: player.health,
                        max_health: player.max_health,
                        guard,
                        equipment,
                    },
                )
            }
        };
        let distance = reachable_dist_sq(caster.position, caster.floor_level, position, floor)
            .filter(|_| caster.position.y.is_finite() && position.y.is_finite())
            .ok_or(AbilityRejectReason::Unavailable)?;
        if distance > AUSCULTATION_RANGE.powi(2) {
            return Err(AbilityRejectReason::OutOfRange);
        }
        let furniture_cache = if let InspectionTarget::Player { player_id: id } = &target {
            self.furniture_passability(&players[id]).await
        } else {
            None
        };
        let blocked = {
            let passability = self.passability_read();
            wall_between(
                furniture_cache.as_ref().unwrap_or(&passability),
                caster.position,
                position,
                floor,
                true,
            )
        };
        if blocked {
            return Err(AbilityRejectReason::Unavailable);
        }
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::Auscultation) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        if let InspectionTarget::Player { player_id } = target {
            inspection.guard = state.guard(&player_id, inspection.guard);
        }
        inspection.equipment.sort_by_key(|item| item.slot.as_str());
        state.start_cooldown(character, AbilityId::Auscultation, AUSCULTATION_COOLDOWN_MS);
        Ok(inspection)
    }

    /// Passability around the furniture a player sits or lies on, so a line
    /// to them is not blocked by it.
    pub(super) async fn furniture_passability(
        &self,
        player: &crate::types::Player,
    ) -> Option<onlinerpg_shared::pathfinding::PassabilityCache> {
        let (kind, id) = (player.object_type.as_deref()?, player.object_id?);
        self.interaction_placement(player, kind, id)
            .await
            .map(|(_, cache, _)| cache)
    }

    /// A living monster where its brain has it now, for an ability aimed at it.
    pub(super) async fn live_monster_target(&self, monster_id: &str) -> Option<MonsterTarget> {
        let mut target = self
            .monsters
            .read()
            .await
            .get(monster_id)
            .filter(|m| m.state != crate::types::MonsterState::Dead && m.health > 0)
            .map(|m| MonsterTarget {
                position: m.position,
                floor_level: m.floor_level,
                monster_type: m.monster_type.clone(),
                level_override: m.level_override,
                rotation: m.rotation,
                body: self.monster_defs.get(&m.monster_type).and_then(|d| d.body),
            })?;
        if let Some(position) = self.brain_position_now(monster_id).await {
            target.position = position;
        }
        Some(target)
    }

    async fn try_bow_mark(
        &self,
        player_id: &PlayerId,
        monster_id: Option<&str>,
    ) -> Result<(), AbilityRejectReason> {
        use super::combat::{reachable_dist_sq, wall_between};

        let monster_id = monster_id.ok_or(AbilityRejectReason::Unavailable)?;
        let target = self
            .live_monster_target(monster_id)
            .await
            .ok_or(AbilityRejectReason::Unavailable)?;
        let target_floor = target.floor_level;
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| p.is_damageable(Self::now_ms()) && !p.is_mounted())
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
        let target_position = target.surface(&caster.position);
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
        state.start_cooldown(character, AbilityId::BowMark, BOW_MARK_COOLDOWN_MS);
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

    pub(super) async fn send_bow_mark_state(&self, player_id: &PlayerId) {
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
            .filter(|p| p.is_damageable(Self::now_ms()) && !p.is_mounted())
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
        state.start_cooldown(character, AbilityId::Radiance, RADIANCE_COOLDOWN_MS);
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
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| {
                p.class == CharacterClass::Knight
                    && p.is_damageable(Self::now_ms())
                    && !p.is_mounted()
            })
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
        if state.cooldown_ms(character, AbilityId::GuardianWard) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        let mut mana = self.mana.write().await;
        let data = mana
            .get_mut(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?;
        data.spend(AbilityId::GuardianWard.mana_cost())?;
        let now = Instant::now();
        state.start_cooldown(
            character,
            AbilityId::GuardianWard,
            GUARDIAN_WARD_COOLDOWN_MS,
        );
        for target in &targets {
            state.wards.insert(
                *target,
                now + Duration::from_millis(GUARDIAN_WARD_DURATION_MS),
            );
        }
        Ok((caster.position, caster.floor_level, targets))
    }

    async fn try_rage(
        &self,
        player_id: &PlayerId,
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let players = self.players.read().await;
        let caster = players
            .get(player_id)
            .filter(|p| {
                p.class == CharacterClass::Barbarian
                    && p.is_damageable(Self::now_ms())
                    && !p.is_mounted()
            })
            .ok_or(AbilityRejectReason::Unavailable)?;
        let chars = self.player_characters.read().await;
        let character = chars
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        if !self
            .inventories
            .read()
            .await
            .get(player_id)
            .is_some_and(|inv| self.wields_rage_weapon(inv))
        {
            return Err(AbilityRejectReason::Equipment);
        }
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::Rage) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        let mut mana = self.mana.write().await;
        mana.get_mut(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .spend(AbilityId::Rage.mana_cost())?;
        let now = Instant::now();
        state.start_cooldown(character, AbilityId::Rage, RAGE_COOLDOWN_MS);
        state
            .rages
            .insert(*player_id, now + Duration::from_millis(RAGE_DURATION_MS));
        Ok((caster.position, caster.floor_level, vec![*player_id]))
    }

    async fn try_primal_roar(
        &self,
        player_id: &PlayerId,
    ) -> Result<(onlinerpg_shared::Position, i8, Vec<PlayerId>), AbilityRejectReason> {
        let (position, floor_level) = {
            let players = self.players.read().await;
            let caster = players
                .get(player_id)
                .filter(|p| {
                    p.class == CharacterClass::Caveman
                        && p.is_damageable(Self::now_ms())
                        && !p.is_mounted()
                })
                .ok_or(AbilityRejectReason::Unavailable)?;
            let character = self
                .player_characters
                .read()
                .await
                .get(player_id)
                .ok_or(AbilityRejectReason::Unavailable)?
                .0;
            let mut state = self.abilities.write().await;
            if state.cooldown_ms(character, AbilityId::PrimalRoar) > 0 {
                return Err(AbilityRejectReason::Cooldown);
            }
            self.mana
                .write()
                .await
                .get_mut(player_id)
                .ok_or(AbilityRejectReason::Unavailable)?
                .spend(AbilityId::PrimalRoar.mana_cost())?;
            let now = Instant::now();
            state.start_cooldown(character, AbilityId::PrimalRoar, PRIMAL_ROAR_COOLDOWN_MS);
            state.roars.insert(
                *player_id,
                now + Duration::from_millis(PRIMAL_ROAR_DURATION_MS),
            );
            (caster.position, caster.floor_level)
        };
        if let Some(caster) = self
            .players
            .write()
            .await
            .get_mut(player_id)
            .filter(|p| p.health > 0)
        {
            caster.temp_health =
                caster
                    .temp_health
                    .max(onlinerpg_shared::ability::primal_roar_temp_health(
                        caster.max_health,
                    ));
        }
        Ok((position, floor_level, vec![*player_id]))
    }

    /// Stagger and provoke the living monsters within reach of the roar.
    async fn roar_at_monsters(&self, player_id: &PlayerId) {
        let Some((position, floor_level)) = self
            .players
            .read()
            .await
            .get(player_id)
            .filter(|p| p.health > 0)
            .map(|p| (p.position, p.floor_level))
        else {
            return;
        };
        let monster_ids: Vec<String> = {
            let monsters = self.monsters.read().await;
            let passability = self.passability_read();
            monsters
                .near(&position)
                .filter(|m| {
                    m.state != crate::types::MonsterState::Dead
                        && m.health > 0
                        && super::combat::reachable_dist_sq(
                            position,
                            floor_level,
                            m.position,
                            m.floor_level,
                        )
                        .is_some_and(|d| d <= PRIMAL_ROAR_RADIUS.powi(2))
                        && !super::combat::wall_between(
                            &passability,
                            position,
                            m.position,
                            floor_level,
                            false,
                        )
                })
                .map(|m| m.id.clone())
                .collect()
        };
        if monster_ids.is_empty() {
            return;
        }
        self.mark_in_combat(player_id).await;
        let stunned = self.brain_roar(&monster_ids, player_id).await;
        if !stunned.is_empty() {
            self.publish_nearby(
                &position,
                floor_level,
                ServerMessage::MonsterStunned {
                    monster_ids: stunned,
                    duration_ms: PRIMAL_ROAR_STAGGER_MS,
                },
                None,
            )
            .await;
        }
    }

    /// Tell the owner their shield; a spent one ends the buff.
    pub(super) async fn send_temp_health(&self, player_id: &PlayerId) {
        let Some(amount) = self
            .players
            .read()
            .await
            .get(player_id)
            .map(|p| p.temp_health)
        else {
            return;
        };
        self.send_direct_message(player_id, ServerMessage::TempHealthUpdate { amount })
            .await;
        if amount == 0
            && self
                .abilities
                .write()
                .await
                .roars
                .remove(player_id)
                .is_some()
        {
            self.send_buff_state(player_id).await;
        }
    }

    fn wields_rage_weapon(&self, inv: &PlayerInventory) -> bool {
        self.main_hand_def(inv).is_some_and(|(_, def)| {
            matches!(
                def.weapon_type,
                Some(
                    WeaponType::Sword
                        | WeaponType::GreatSword
                        | WeaponType::Axe
                        | WeaponType::Mace
                        | WeaponType::Club
                        | WeaponType::Spear
                        | WeaponType::Dagger
                )
            )
        })
    }

    pub(super) async fn end_rage_if_weapon_lost(
        &self,
        player_id: &PlayerId,
        inv: &PlayerInventory,
    ) {
        if self.wields_rage_weapon(inv)
            || !self.abilities.read().await.rages.contains_key(player_id)
        {
            return;
        }
        if self
            .abilities
            .write()
            .await
            .rages
            .remove(player_id)
            .is_some()
        {
            self.send_buff_state(player_id).await;
        }
    }

    async fn send_buff_state(&self, player_id: &PlayerId) {
        self.sync_visible_buffs(player_id).await;
        self.send_buff_update(player_id).await;
        self.send_direct_message(player_id, self.effective_stats(player_id).await.into())
            .await;
    }

    /// The badge list alone, for a timer that moved without a stat change.
    pub(super) async fn send_buff_update(&self, player_id: &PlayerId) {
        let hymn_until = self
            .hunger
            .read()
            .await
            .get(player_id)
            .and_then(|data| data.hymn_until);
        let message = self
            .abilities
            .read()
            .await
            .buff_message(player_id, hymn_until);
        self.send_direct_message(player_id, message).await;
    }

    async fn sync_visible_buffs(&self, player_id: &PlayerId) {
        let mut shield_gone = false;
        let (position, floor_level, changes) = {
            let mut players = self.players.write().await;
            let Some(player) = players.get_mut(player_id) else {
                return;
            };
            let (radiance, raging, roaring) = {
                let state = self.abilities.read().await;
                let now = Instant::now();
                let radiance = state
                    .radiances
                    .get(player_id)
                    .is_some_and(|until| *until > now);
                let roaring = state.roars.get(player_id).is_some_and(|until| *until > now);
                (radiance, state.raging(player_id), roaring)
            };
            if !roaring && player.temp_health > 0 {
                player.temp_health = 0;
                shield_gone = true;
            }
            let mut changes = Vec::new();
            if player.radiance_on != radiance {
                player.radiance_on = radiance;
                changes.push(ServerMessage::PlayerRadianceToggled {
                    player_id: *player_id,
                    enabled: radiance,
                });
            }
            if player.raging != raging {
                player.raging = raging;
                changes.push(ServerMessage::PlayerRageToggled {
                    player_id: *player_id,
                    enabled: raging,
                });
            }
            (player.position, player.floor_level, changes)
        };
        if shield_gone {
            self.send_direct_message(player_id, ServerMessage::TempHealthUpdate { amount: 0 })
                .await;
        }
        for message in changes {
            self.publish_nearby(&position, floor_level, message, None)
                .await;
        }
    }

    pub(super) async fn clear_buffs(&self, player_id: &PlayerId) {
        let removed = {
            let mut state = self.abilities.write().await;
            let ward = state.wards.remove(player_id).is_some();
            let radiance = state.radiances.remove(player_id).is_some();
            let mark = state.marks.remove(player_id).is_some();
            let rage = state.rages.remove(player_id).is_some();
            let roar = state.roars.remove(player_id).is_some();
            state.hymns.remove(player_id);
            ward || radiance || mark || rage || roar
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
                .chain(state.rages.iter())
                .chain(state.roars.iter())
                .filter(|(_, until)| **until <= now)
                .map(|(id, _)| *id)
                .collect();
            state.wards.retain(|_, until| *until > now);
            state.radiances.retain(|_, until| *until > now);
            state.rages.retain(|_, until| *until > now);
            state.roars.retain(|_, until| *until > now);
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
