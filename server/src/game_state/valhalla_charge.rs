use super::combat::{reachable_dist_sq, wall_between};
use super::goal_movement::swept_blocked;
use super::GameState;
use crate::auth::AuthService;
use crate::item_defs::WeaponType;
use crate::types::{CharacterClass, MonsterState, PlayerId, Position, ServerMessage};
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, VALHALLA_CHARGE_COOLDOWN_MS, VALHALLA_CHARGE_DASH_MS,
    VALHALLA_CHARGE_MAX_RANGE, VALHALLA_CHARGE_MIN_RANGE, VALHALLA_CHARGE_SHORT_SLOW_MS,
    VALHALLA_CHARGE_SLOW_MS, VALHALLA_CHARGE_SLOW_MULT,
};
use onlinerpg_shared::reach::PLAYER_MELEE_REACH_METERS;
use onlinerpg_shared::{dungeon, shortest_world_delta_x, wrap_world_x};
use std::time::Duration;

/// The charge stops this far inside the weapon's reach.
const LANDING_INSIDE_REACH: f32 = 0.3;

struct Charge {
    monster_id: String,
    character: i64,
    from: Position,
    to: Position,
    rotation: f32,
    floor_level: i8,
}

impl GameState {
    pub(super) async fn valhalla_charge(
        &self,
        player_id: &PlayerId,
        monster_id: &str,
        auth: Option<&AuthService>,
    ) {
        self.stop_bed_rest(player_id).await;
        match self.try_valhalla_charge(player_id, monster_id).await {
            Ok(charge) => {
                self.log_ability_use(player_id, AbilityId::ValhallaCharge, 1)
                    .await;
                self.run_valhalla_charge(player_id, charge, auth).await
            }
            Err(reason) => {
                self.send_direct_message(
                    player_id,
                    ServerMessage::AbilityRejected {
                        ability: AbilityId::ValhallaCharge,
                        reason,
                    },
                )
                .await;
            }
        }
        self.send_direct_message(player_id, self.ability_cooldown_message(player_id).await)
            .await;
    }

    async fn try_valhalla_charge(
        &self,
        player_id: &PlayerId,
        monster_id: &str,
    ) -> Result<Charge, AbilityRejectReason> {
        let (from, floor_level) = self
            .players
            .read()
            .await
            .get(player_id)
            .filter(|p| {
                p.class == CharacterClass::Valkyrie
                    && p.is_damageable(Self::now_ms())
                    && !p.is_mounted()
                    && p.position.y.is_finite()
            })
            .map(|p| (p.position, p.floor_level))
            .ok_or(AbilityRejectReason::Unavailable)?;
        let character = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .0;
        let reach = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inv| self.main_hand_def(inv))
            .filter(|(_, def)| {
                matches!(def.weapon_type, Some(WeaponType::Sword | WeaponType::Spear))
                    && !def.is_two_handed()
            })
            .map(|(_, def)| def.weapon_range().unwrap_or(PLAYER_MELEE_REACH_METERS))
            .ok_or(AbilityRejectReason::Equipment)?;
        if self
            .abilities
            .read()
            .await
            .cooldown_ms(character, AbilityId::ValhallaCharge)
            > 0
            || !self.player_attack_ready(player_id).await
        {
            return Err(AbilityRejectReason::Cooldown);
        }
        let target = self
            .live_monster_target(monster_id)
            .await
            .ok_or(AbilityRejectReason::Unavailable)?;
        let surface = target.surface(&from);
        let distance = reachable_dist_sq(from, floor_level, surface, target.floor_level)
            .ok_or(AbilityRejectReason::Unavailable)?
            .sqrt();
        if distance > VALHALLA_CHARGE_MAX_RANGE {
            return Err(AbilityRejectReason::OutOfRange);
        }
        if distance < VALHALLA_CHARGE_MIN_RANGE {
            return Err(AbilityRejectReason::TooClose);
        }
        let dx = shortest_world_delta_x(from.x, surface.x);
        let dz = surface.z - from.z;
        let travel = (distance - (reach - LANDING_INSIDE_REACH)) / distance;
        let (step_x, step_z) = (dx * travel, dz * travel);
        let blocked = {
            let cache = self.passability_read();
            swept_blocked(
                &cache,
                from,
                step_x,
                step_z,
                dungeon::passability_floor_for_level(floor_level),
            ) || wall_between(&cache, from, surface, floor_level, false)
        };
        if blocked {
            return Err(AbilityRejectReason::Blocked);
        }
        let mut to = Position {
            x: wrap_world_x(from.x + step_x),
            y: from.y,
            z: from.z + step_z,
        };
        to.y = self
            .dash_landing_y(floor_level, to)
            .await
            .filter(|y| y.is_finite())
            .ok_or(AbilityRejectReason::Blocked)?;
        let mut state = self.abilities.write().await;
        if state.cooldown_ms(character, AbilityId::ValhallaCharge) > 0 {
            return Err(AbilityRejectReason::Cooldown);
        }
        self.mana
            .write()
            .await
            .get_mut(player_id)
            .ok_or(AbilityRejectReason::Unavailable)?
            .spend(AbilityId::ValhallaCharge.mana_cost())?;
        state.start_cooldown(
            character,
            AbilityId::ValhallaCharge,
            VALHALLA_CHARGE_COOLDOWN_MS,
        );
        Ok(Charge {
            monster_id: monster_id.to_owned(),
            character,
            from,
            to,
            rotation: dx.atan2(dz),
            floor_level,
        })
    }

    async fn run_valhalla_charge(
        &self,
        player_id: &PlayerId,
        charge: Charge,
        auth: Option<&AuthService>,
    ) {
        self.mark_dirty(player_id).await;
        self.send_mana_update(player_id).await;
        self.reveal(player_id).await;
        let interval = self.player_attack_interval_ms(player_id).await;
        let hold = self
            .hold_player_attack(player_id, VALHALLA_CHARGE_DASH_MS + interval)
            .await;
        self.apply_player_position(
            player_id,
            charge.to,
            charge.rotation,
            charge.floor_level,
            ServerMessage::PlayerCharged {
                player_id: *player_id,
                monster_id: charge.monster_id.clone(),
                from: charge.from,
                position: charge.to,
                rotation: charge.rotation,
                floor_level: charge.floor_level,
                duration_ms: VALHALLA_CHARGE_DASH_MS,
            },
        )
        .await;
        let version = self.action_version(player_id).await;
        let game = self.clone();
        let player_id = *player_id;
        let auth = auth.cloned();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(VALHALLA_CHARGE_DASH_MS)).await;
            if !game
                .land_valhalla_charge(&player_id, &charge, version, auth.as_ref())
                .await
            {
                game.release_player_attack_hold(&player_id, hold).await;
            }
        });
    }

    /// The arrival swing; false when the charge was cut short.
    async fn land_valhalla_charge(
        &self,
        player_id: &PlayerId,
        charge: &Charge,
        version: u64,
        auth: Option<&AuthService>,
    ) -> bool {
        let same_character = self
            .player_characters
            .read()
            .await
            .get(player_id)
            .is_some_and(|entry| entry.0 == charge.character);
        let ready = self.players.read().await.get(player_id).is_some_and(|p| {
            p.health > 0 && !p.is_mounted() && p.floor_level == charge.floor_level
        });
        if !same_character || !ready || self.action_version(player_id).await != version {
            return false;
        }
        let Ok(context) = self
            .validate_player_attack(player_id, &charge.monster_id)
            .await
        else {
            return false;
        };
        self.restart_player_attack_interval(player_id).await;
        if self
            .resolve_player_attack(
                player_id,
                charge.monster_id.clone(),
                context,
                auth,
                None,
                true,
            )
            .await
        {
            self.slow_monster(&charge.monster_id).await;
        }
        true
    }

    pub(super) async fn slow_monster(&self, monster_id: &str) {
        let Some((position, floor_level, short)) = self
            .monsters
            .read()
            .await
            .get(monster_id)
            .filter(|m| m.state != MonsterState::Dead)
            .map(|m| {
                (
                    m.position,
                    m.floor_level,
                    self.monster_defs
                        .get(&m.monster_type)
                        .is_some_and(|d| d.boss || d.stagger_immune),
                )
            })
        else {
            return;
        };
        let duration_ms = if short {
            VALHALLA_CHARGE_SHORT_SLOW_MS
        } else {
            VALHALLA_CHARGE_SLOW_MS
        };
        if self
            .brain_slow(monster_id, VALHALLA_CHARGE_SLOW_MULT, duration_ms)
            .await
        {
            self.publish_nearby(
                &position,
                floor_level,
                ServerMessage::MonsterSlowed {
                    monster_id: monster_id.to_owned(),
                    move_mult: VALHALLA_CHARGE_SLOW_MULT,
                    duration_ms,
                },
                None,
            )
            .await;
        }
    }
}
