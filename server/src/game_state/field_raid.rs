//! Field-boss sites, altar offerings and the raid they open
//! (doc/TIER5_FIELD_RAID.md).

use super::bed_rest::within_reach;
use super::inventory::consume_one;
use super::passability::region_of;
use super::passability::regions_around;
use super::player_interaction::placement_position;
use super::GameState;
use crate::auth::{unix_now, AuthService};
use crate::metrics::{outcome_key, FieldRaidRaider, FieldRaidRecord};
use crate::types::{MonsterLifecycle, PlayerId};
use onlinerpg_shared::celestial::is_full_daylight;
use onlinerpg_shared::field_boss::{field_boss_by_altar, field_boss_defs, AltarDef, FieldBossDef};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::messages::{AltarStatus, FieldRaidOutcome};
use onlinerpg_shared::moon::{full_moons_up, Moon};
use onlinerpg_shared::world::GameDateTime;
use onlinerpg_shared::{MonsterState, Position, ServerMessage};
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use tracing::{info, warn};

/// Living characters within this of the boss keep the raid going.
const BATTLE_ZONE_RADIUS_M: f32 = 40.0;
/// Who hears how the raid ended.
const NOTICE_RADIUS_M: f32 = 100.0;
pub(crate) const FIELD_RAID_TICK: Duration = Duration::from_secs(1);
/// Raid ticks from the offering to the boss standing up, so the client's
/// crumbling stand-in has time to play.
pub(super) const WAKE_TICKS: u8 = 3;
/// When the client collapses the stand-in. The spawn lands on the last wake
/// tick, up to one tick period later, inside the dust.
const WAKE_IN_MS: u32 = (WAKE_TICKS as u32 - 1) * FIELD_RAID_TICK.as_millis() as u32;

/// A field boss placed in the world, standing on sampled ground.
pub(crate) struct FieldBossSite {
    boss: &'static FieldBossDef,
    spawn: Position,
    placements: Vec<FurniturePlacement>,
}

#[derive(Clone)]
pub(crate) struct FieldRaid {
    boss: &'static FieldBossDef,
    position: Position,
    phase: Phase,
    /// The offering character, by character id since a reconnect changes
    /// the player id mid-raid.
    summoner: Option<i64>,
    /// Who was around and what each did, by character id for the same
    /// reason.
    raiders: HashMap<i64, FieldRaidRaider>,
    /// Whose hit brought the boss to 0 HP; official NPCs never count.
    last_hit: Option<i64>,
    /// The full moons in the sky at the offering; the raid lasts until all
    /// have set or the sky turns fully blue, through the dawn glow and the
    /// waning past full.
    moons: Vec<Moon>,
    summoned_at: i64,
}

impl FieldRaid {
    /// Apply `update` to each of `who`, adding those not yet among the raiders.
    fn mark(&mut self, who: Vec<(i64, String)>, update: impl Fn(&mut FieldRaidRaider)) {
        for (character_id, name) in who {
            update(
                self.raiders
                    .entry(character_id)
                    .or_insert_with(|| FieldRaidRaider {
                        character_id,
                        name,
                        ..FieldRaidRaider::default()
                    }),
            );
        }
    }

    /// How the sky ends the raid, if it has.
    fn sky_outcome(&self, now: &GameDateTime) -> Option<FieldRaidOutcome> {
        self.boss.condition?;
        if !self.moons.iter().any(|m| m.is_up(now)) {
            Some(FieldRaidOutcome::MoonSet)
        } else if is_full_daylight(now) {
            Some(FieldRaidOutcome::Dawn)
        } else {
            None
        }
    }
}

#[derive(Clone)]
enum Phase {
    /// Offered; the boss spawns outside the offering's locks on the tick
    /// that brings this to zero.
    Waking {
        ticks_left: u8,
    },
    Fighting {
        monster_id: String,
        max_health: u32,
    },
}

impl GameState {
    /// Place every field-boss site on the baked ground. A site whose ground
    /// cannot be read stays unplaced, so a missing bake does not stop boot.
    pub(super) async fn load_field_boss_sites(&self) {
        let mut grounds = Vec::new();
        for boss in field_boss_defs() {
            let (ax, az) = boss.altar.as_ref().map_or((boss.x, boss.z), |a| (a.x, a.z));
            // One after the other: both points usually share a tile.
            let ground = async {
                let altar = self.height_sampler.sample_height(ax, az).await?;
                let spawn = self.height_sampler.sample_height(boss.x, boss.z).await?;
                std::io::Result::Ok((altar, spawn))
            };
            match ground.await {
                Ok(ground) => grounds.push((boss, ground)),
                Err(e) => warn!("Field boss {}: no ground to stand on: {e}", boss.id),
            }
        }
        self.place_field_bosses(grounds);
    }

    /// Each boss with its (altar, spawn) ground heights.
    pub(super) fn place_field_bosses(
        &self,
        grounds: impl IntoIterator<Item = (&'static FieldBossDef, (f32, f32))>,
    ) {
        let sites = grounds
            .into_iter()
            .map(|(boss, (altar_ground, spawn_ground))| FieldBossSite {
                boss,
                spawn: Position {
                    x: boss.x,
                    y: spawn_ground,
                    z: boss.z,
                },
                placements: boss.placements(altar_ground, spawn_ground),
            })
            .collect();
        *self
            .field_boss_sites
            .write()
            .unwrap_or_else(|e| e.into_inner()) = sites;
    }

    fn field_boss_sites_read(&self) -> std::sync::RwLockReadGuard<'_, Vec<FieldBossSite>> {
        self.field_boss_sites
            .read()
            .unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn field_boss_placements_in(&self, rx: i32, rz: i32) -> Vec<FurniturePlacement> {
        self.field_boss_sites_read()
            .iter()
            .flat_map(|s| &s.placements)
            .filter(|p| region_of(p.x, p.z) == (rx, rz))
            .cloned()
            .collect()
    }

    pub(super) fn field_boss_regions(&self) -> Vec<(i32, i32)> {
        let mut regions: Vec<_> = self
            .field_boss_sites_read()
            .iter()
            .flat_map(|s| &s.placements)
            .map(|p| region_of(p.x, p.z))
            .collect();
        regions.sort_unstable();
        regions.dedup();
        regions
    }

    fn field_boss_spawn(&self, boss: &FieldBossDef) -> Option<Position> {
        self.field_boss_sites_read()
            .iter()
            .find(|s| std::ptr::eq(s.boss, boss))
            .map(|s| s.spawn)
    }

    /// The altar placement `object_id` the player can reach, with its boss.
    async fn reachable_altar(
        &self,
        player_id: &PlayerId,
        object_id: u32,
    ) -> Option<(FurniturePlacement, &'static FieldBossDef, &'static AltarDef)> {
        let players = self.players.read().await;
        let player = players.get(player_id)?;
        let index = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner());
        regions_around(player.position.x, player.position.z)
            .filter_map(|key| index.get(&key))
            .flatten()
            .find(|p| {
                p.id == object_id
                    && within_reach(player, placement_position(p), p.floor_level as i8)
            })
            .and_then(|p| {
                let (boss, altar) = field_boss_by_altar(p.id)?;
                Some((p.clone(), boss, altar))
            })
    }

    /// What the altar would do for the player now, and the bag item it would take.
    async fn altar_status(
        &self,
        player_id: &PlayerId,
        boss: &FieldBossDef,
        def: &AltarDef,
    ) -> (AltarStatus, Option<u64>) {
        if self.field_raid.lock().await.is_some() {
            return (AltarStatus::RaidActive, None);
        }
        // FullMoon is the only condition so far, so it is the only refusal.
        if !boss.condition_holds(&self.current_game_datetime()) {
            return (AltarStatus::NotFullMoon, None);
        }
        let inventories = self.inventories.read().await;
        let Some(inv) = inventories.get(player_id) else {
            return (AltarStatus::NoOffering, None);
        };
        if let Some(item) = inv
            .bag
            .iter()
            .find(|i| i.item_def_id == def.offering && i.quantity > 0)
        {
            let status = AltarStatus::Ready {
                offering: def.offering.clone(),
                monster: boss.monster.clone(),
            };
            (status, Some(item.instance_id))
        } else if inv.equipped.values().any(|i| i.item_def_id == def.offering) {
            let status = AltarStatus::OfferingEquipped {
                offering: def.offering.clone(),
            };
            (status, None)
        } else {
            (AltarStatus::NoOffering, None)
        }
    }

    pub async fn inspect_altar(&self, player_id: &PlayerId, object_id: u32) {
        let Some((_, boss, def)) = self.reachable_altar(player_id, object_id).await else {
            return;
        };
        let (status, _) = self.altar_status(player_id, boss, def).await;
        self.send_direct_message(player_id, ServerMessage::AltarStatus { object_id, status })
            .await;
    }

    pub async fn offer_at_altar(&self, player_id: &PlayerId, object_id: u32) {
        if self
            .reject_if_defeated(player_id, "You can't do that while defeated")
            .await
        {
            return;
        }
        let Some((altar, boss, def)) = self.reachable_altar(player_id, object_id).await else {
            return;
        };
        let Some(position) = self.field_boss_spawn(boss) else {
            return;
        };
        let (status, instance_id) = self.altar_status(player_id, boss, def).await;
        let Some(instance_id) = instance_id else {
            self.send_direct_message(player_id, ServerMessage::AltarStatus { object_id, status })
                .await;
            return;
        };
        if self
            .reject_if_trade_reserved(player_id, instance_id, "offer")
            .await
        {
            return;
        }
        let summoner = self.character_id_of(player_id).await;
        let moons = full_moons_up(&self.current_game_datetime()).collect();
        let summoned_at = unix_now();
        // Raid state and the item go together, so a valid offering always
        // opens exactly one raid and a refused one keeps its item.
        let snapshot = {
            let mut raid = self.field_raid.lock().await;
            let mut inventories = self.inventories.write().await;
            let consumed = inventories
                .get_mut(player_id)
                .filter(|_| raid.is_none())
                .filter(|inv| {
                    inv.bag
                        .iter()
                        .any(|i| i.instance_id == instance_id && i.item_def_id == def.offering)
                })
                .and_then(|inv| consume_one(inv, instance_id).map(|_| inv.clone()));
            if consumed.is_some() {
                *raid = Some(FieldRaid {
                    boss,
                    position,
                    phase: Phase::Waking {
                        ticks_left: WAKE_TICKS,
                    },
                    summoner,
                    raiders: HashMap::new(),
                    last_hit: None,
                    moons,
                    summoned_at,
                });
            }
            consumed
        };
        let Some(snapshot) = snapshot else {
            // Another offering got there first, or the item moved: say why.
            self.inspect_altar(player_id, object_id).await;
            return;
        };
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.log_consumed(player_id, [def.offering.as_str()]).await;
        self.record_field_raid_summon(&boss.id, summoned_at);
        let nearby = self
            .player_ids_within_position(&position, 0, NOTICE_RADIUS_M)
            .await;
        self.mark_raiders(&nearby, |r| r.near_at_summon = true)
            .await;
        info!(
            "{player_id} offered at {} ({:.1},{:.1}); {} wakes at ({:.1},{:.1}); summoner {}, {} nearby",
            altar.type_id,
            altar.x,
            altar.z,
            boss.monster,
            position.x,
            position.z,
            self.player_name_of(player_id).await,
            nearby.len(),
        );
        self.send_direct_message_to_players(
            &nearby,
            ServerMessage::FieldBossWaking {
                monster: boss.monster.clone(),
                position,
                wake_in_ms: WAKE_IN_MS,
            },
        )
        .await;
    }

    /// Character id and name of each player who plays a character.
    async fn raider_identities(&self, ids: &[PlayerId]) -> Vec<(i64, String)> {
        let players = self.players.read().await;
        let characters = self.player_characters.read().await;
        ids.iter()
            .filter_map(|id| Some((characters.get(id)?.0, players.get(id)?.name.clone())))
            .collect()
    }

    async fn mark_raiders(&self, ids: &[PlayerId], update: impl Fn(&mut FieldRaidRaider)) {
        let who = self.raider_identities(ids).await;
        if let Some(raid) = self.field_raid.lock().await.as_mut() {
            raid.mark(who, update);
        }
    }

    /// HP a hit took off the raid boss.
    pub(super) async fn record_field_raid_hit(&self, player_id: &PlayerId, damage: u32) {
        if damage > 0 {
            self.mark_raiders(&[*player_id], |r| r.dealt += u64::from(damage))
                .await;
        }
    }

    /// HP the raid boss took off a player, who stood in its reach and so in
    /// the battle zone.
    pub(super) async fn record_field_raid_taken(&self, player_id: &PlayerId, lost: u32) {
        if lost > 0 {
            self.mark_raiders(&[*player_id], |r| {
                r.taken += u64::from(lost);
                r.in_zone = true;
            })
            .await;
        }
    }

    /// A death, whatever its cause, in an open raid's battle zone.
    pub(super) async fn record_field_raid_death(&self, player_id: &PlayerId) {
        let center = self.field_raid.lock().await.as_ref().map(|r| r.position);
        let Some(center) = center else {
            return;
        };
        if self
            .player_ids_within_position(&center, 0, BATTLE_ZONE_RADIUS_M)
            .await
            .contains(player_id)
        {
            self.mark_raiders(&[*player_id], |r| {
                r.deaths += 1;
                r.in_zone = true;
            })
            .await;
        }
    }

    /// The killing blow: its hit and the last hit, then the defeat. The tick
    /// leaves a dead boss to this, so the blow always counts.
    pub(super) async fn settle_field_raid_kill(
        &self,
        player_id: &PlayerId,
        damage: u32,
        auth: Option<&AuthService>,
    ) {
        self.record_field_raid_hit(player_id, damage).await;
        let last_hit = self.eligible_title_character_id(player_id).await;
        let Some(monster) = self.field_raid.lock().await.as_mut().map(|raid| {
            raid.last_hit = last_hit;
            raid.boss.monster.clone()
        }) else {
            return;
        };
        info!(
            "Field raid {monster}: last hit by {}",
            self.player_name_of(player_id).await
        );
        self.end_field_raid(FieldRaidOutcome::Defeated, auth).await;
    }

    /// Wake the boss after an offering; end the raid on a set moon or an empty
    /// battle zone. It works from a copy and never holds `field_raid` across
    /// the spawn and despawn it sets off.
    pub async fn tick_field_raid(&self) {
        let Some(raid) = self.field_raid.lock().await.clone() else {
            return;
        };
        match raid.phase {
            Phase::Waking { ticks_left } if ticks_left > 1 => {
                if let Some(raid) = self.field_raid.lock().await.as_mut() {
                    raid.phase = Phase::Waking {
                        ticks_left: ticks_left - 1,
                    };
                }
            }
            Phase::Waking { .. } => match self.wake_field_boss(&raid).await {
                Some((monster_id, max_health)) => {
                    if let Some(raid) = self.field_raid.lock().await.as_mut() {
                        raid.phase = Phase::Fighting {
                            monster_id,
                            max_health,
                        };
                    }
                    self.set_stand_in_woken(raid.boss, true);
                }
                None => {
                    warn!(
                        "Field raid: {} failed to spawn; raid dropped",
                        raid.boss.monster
                    );
                    *self.field_raid.lock().await = None;
                }
            },
            Phase::Fighting { ref monster_id, .. } => {
                let dead = self
                    .monsters
                    .read()
                    .await
                    .get(monster_id)
                    .is_some_and(|m| m.state == MonsterState::Dead);
                if dead {
                    return;
                }
                let living = self.living_near(&raid.position).await;
                self.mark_raiders(&living, |r| r.in_zone = true).await;
                let outcome = if let Some(outcome) = raid.sky_outcome(&self.current_game_datetime())
                {
                    outcome
                } else if living.is_empty() {
                    FieldRaidOutcome::Wiped
                } else {
                    return;
                };
                self.end_field_raid(outcome, None).await;
            }
        }
    }

    /// `placements` minus the stand-in the raid boss stands on.
    pub(super) fn without_woken_stand_in<'a>(
        &self,
        placements: &'a [FurniturePlacement],
    ) -> Cow<'a, [FurniturePlacement]> {
        let woken = *self
            .woken_stand_in
            .read()
            .unwrap_or_else(|e| e.into_inner());
        match woken {
            Some(id) if placements.iter().any(|p| p.id == id) => {
                placements.iter().filter(|p| p.id != id).cloned().collect()
            }
            _ => Cow::Borrowed(placements),
        }
    }

    /// Lift the boss's stand-in out of collision while it is up, or restore it.
    fn set_stand_in_woken(&self, boss: &FieldBossDef, woken: bool) {
        *self
            .woken_stand_in
            .write()
            .unwrap_or_else(|e| e.into_inner()) = woken.then(|| boss.dormant_placement_id());
        // The stand-in lies at the boss's own spot.
        let (rx, rz) = region_of(boss.x, boss.z);
        let placements = self
            .interaction_furniture
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(&(rx, rz))
            .cloned()
            .unwrap_or_default();
        self.rebuild_region_furniture_passability(rx, rz, &placements);
    }

    /// Living characters in the battle zone.
    async fn living_near(&self, center: &Position) -> Vec<PlayerId> {
        let mut ids = self
            .player_ids_within_position(center, 0, BATTLE_ZONE_RADIUS_M)
            .await;
        let players = self.players.read().await;
        ids.retain(|id| players.get(id).is_some_and(|p| p.health > 0));
        ids
    }

    async fn wake_field_boss(&self, raid: &FieldRaid) -> Option<(String, u32)> {
        self.spawn_monster(
            raid.boss.monster.clone(),
            raid.position,
            raid.boss.rotation.to_radians(),
            0,
            MonsterLifecycle::FieldRaid,
            None,
            true,
        )
        .await
        .map(|m| (m.id, m.max_health))
    }

    /// Taking the raid ends it once, whichever of the tick and the kill gets
    /// there first. A defeated boss is left to the kill's own corpse timer;
    /// any other sinks back into its stand-in. `auth` stores a defeat's titles.
    pub(super) async fn end_field_raid(
        &self,
        outcome: FieldRaidOutcome,
        auth: Option<&AuthService>,
    ) {
        let Some(mut raid) = self
            .field_raid
            .lock()
            .await
            .take_if(|r| matches!(r.phase, Phase::Fighting { .. }))
        else {
            return;
        };
        let Phase::Fighting {
            monster_id,
            max_health,
        } = raid.phase.clone()
        else {
            unreachable!("taken only while fighting");
        };
        // A defeated boss lies at 0 HP until its corpse is swept.
        let health = self
            .monsters
            .read()
            .await
            .get(&monster_id)
            .map_or(0, |m| m.health);
        // The raiders gathered before the offering; only those around hear it.
        let nearby = self
            .player_ids_within_position(&raid.position, 0, NOTICE_RADIUS_M)
            .await;
        if outcome != FieldRaidOutcome::Defeated {
            // Sent ahead of the removal so the client can hold the body for
            // its sink instead of dropping it.
            self.send_direct_message_to_players(
                &nearby,
                ServerMessage::FieldBossSleeping {
                    monster_id: monster_id.clone(),
                    monster: raid.boss.monster.clone(),
                    position: raid.position,
                },
            )
            .await;
            self.despawn_monsters(vec![monster_id]).await;
        }
        self.set_stand_in_woken(raid.boss, false);
        info!("Field raid {} ended: {outcome:?}", raid.boss.monster);
        let living = self.living_near(&raid.position).await;
        raid.mark(self.raider_identities(&living).await, |r| {
            r.in_zone = true;
            r.in_zone_at_end = true;
        });
        self.record_finished_raid(&raid, outcome, health, max_health);
        let rewarded = match outcome {
            FieldRaidOutcome::Defeated => self.reward_raiders(&raid, living, auth).await,
            _ => HashSet::new(),
        };
        let bystanders: Vec<_> = nearby
            .into_iter()
            .filter(|id| !rewarded.contains(id))
            .collect();
        let notice = ServerMessage::FieldRaidNotice {
            monster: raid.boss.monster.clone(),
            outcome,
            reward: None,
            tokens: None,
        };
        self.send_direct_message_to_players(&bystanders, notice)
            .await;
    }

    /// Raiders are those who hit the boss and stand alive in the battle zone
    /// as it falls. Each earns tokens by the damage they dealt, and the
    /// summoner the altar's reward too, and each a raid title; the last blow's
    /// owner, raider or not, also the final-blow title (doc/TIER5_FIELD_RAID.md).
    async fn reward_raiders(
        &self,
        raid: &FieldRaid,
        living: Vec<PlayerId>,
        auth: Option<&AuthService>,
    ) -> HashSet<PlayerId> {
        let mut raiders = Vec::new();
        for player_id in living {
            let Some(character) = self.character_id_of(&player_id).await else {
                continue;
            };
            if let Some(damage) = raid
                .raiders
                .get(&character)
                .map(|r| r.dealt)
                .filter(|&d| d > 0)
            {
                raiders.push((player_id, character, damage));
            }
        }
        raiders.sort_by_key(|r| std::cmp::Reverse(r.2));
        let token = raid.boss.token.as_ref();
        let altar_reward = raid.boss.altar.as_ref().and_then(|a| a.reward.as_deref());
        let mut rewarded = HashSet::new();
        for (rank, &(player_id, character, damage)) in raiders.iter().enumerate() {
            let tokens = token.map(|t| (t.item.clone(), t.count_for(damage)));
            if let Some((token, count)) = &tokens {
                self.award_items(&player_id, token, *count).await;
            }
            let item = altar_reward.filter(|_| Some(character) == raid.summoner);
            if let Some(item) = item {
                self.award_item(&player_id, item).await;
            }
            info!(
                "Field raid {}: #{} {} dealt {damage}, tokens {tokens:?}, item {item:?}",
                raid.boss.monster,
                rank + 1,
                self.player_name_of(&player_id).await,
            );
            let notice = ServerMessage::FieldRaidNotice {
                monster: raid.boss.monster.clone(),
                outcome: FieldRaidOutcome::Defeated,
                reward: item.map(str::to_string),
                tokens,
            };
            self.send_direct_message(&player_id, notice).await;
            rewarded.insert(player_id);
        }
        let titled: Vec<i64> = {
            let players = self.players.read().await;
            raiders
                .iter()
                .filter(|r| players.get(&r.0).is_some_and(|p| !p.is_official_npc))
                .map(|r| r.1)
                .collect()
        };
        self.grant_field_raid_titles(&raid.boss.monster, &titled, raid.last_hit, auth)
            .await;
        rewarded
    }

    /// One summary line and one line per raider, then the record for the DB,
    /// which also stores the end event.
    fn record_finished_raid(
        &self,
        raid: &FieldRaid,
        outcome: FieldRaidOutcome,
        boss_health: u32,
        boss_max_health: u32,
    ) {
        let ended_at = unix_now();
        let mut raiders: Vec<_> = raid.raiders.values().cloned().collect();
        raiders.sort_by(|a, b| {
            (b.dealt, b.taken, a.character_id).cmp(&(a.dealt, a.taken, b.character_id))
        });
        let boss = raid.boss.id.as_str();
        info!(
            boss,
            outcome = outcome_key(outcome),
            lasted_s = ended_at - raid.summoned_at,
            boss_health,
            boss_max_health,
            raiders = raiders.len(),
            "Field raid summary"
        );
        for r in &raiders {
            info!(
                boss,
                name = r.name.as_str(),
                character_id = r.character_id,
                dealt = r.dealt,
                taken = r.taken,
                deaths = r.deaths,
                near_at_summon = r.near_at_summon,
                in_zone = r.in_zone,
                in_zone_at_end = r.in_zone_at_end,
                "Field raid raider"
            );
        }
        self.pending_field_raid_records().push(FieldRaidRecord {
            boss: raid.boss.id.clone(),
            summoned_at: raid.summoned_at,
            ended_at,
            outcome,
            summoner_id: raid.summoner,
            boss_health,
            boss_max_health,
            raiders,
        });
    }

    pub(super) fn full_moon_command(&self) -> Result<String, String> {
        match self.debug_jump_to_full_moon() {
            Some(at) => Ok(format!("Fullmoon: clock moved to {at}.")),
            None => Err("Fullmoon: none within 60 days.".to_string()),
        }
    }
}
