//! War-horn estate defenses (doc/ESTATE_DEFENSE.md): five waves march on a
//! banner planted where the horn was blown; holding it pays out a chest.

use super::inventory::consume_one;
use super::localization::{localized, PlayerMessage};
use super::{auth_db, GameState};
use crate::auth::{unix_now, AuthService, DefenseSite, Disgraces};
use crate::estate_defense_defs::{grade_for_horn, DefenseGrade};
use crate::types::{MonsterLifecycle, PlayerId, Position};
use onlinerpg_shared::celestial::is_night;
use onlinerpg_shared::pathfinding::{get_floor_y_base, is_circle_blocked_on_floor};
use onlinerpg_shared::{MonsterState, Siege};
use onlinerpg_terrain::land::plot_addr;
use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

pub(crate) const ESTATE_DEFENSE_TICK: Duration = Duration::from_secs(1);
/// No player-placed obstacle may stand this close to the banner; raiders
/// rise on its edge.
const CLEAR_RADIUS_M: f32 = 10.0;
pub(super) const BANNER_LOCKED: &str =
    "Nothing can be placed by the banner while the estate is under attack.";
/// Defenders count while within this of the banner.
const ZONE_RADIUS_M: f32 = 30.0;
/// Seconds from the horn to each wave.
const WAVE_AT: [u32; 5] = [30, 110, 190, 270, 350];
const WAVE_SIZE: usize = 6;
const FINAL_WAVE_PER_SIDE: usize = 3;
/// No success is judged before this, so a defense always takes as long.
const EARLIEST_SUCCESS: u32 = 420;
const TIME_LIMIT: u32 = 600;
const BANNER_DURABILITY: u8 = 5;
const LEADER_BREACH: u8 = 3;
const BREACH_RADIUS_M: f32 = 2.0;
const BREACH_SECS: u32 = 3;
/// Seconds with no living defender in the zone before the defense is lost.
const WIPE_SECS: u32 = 10;
/// Ten game days.
const DISGRACE_SECS: i64 = 10 * super::time::REAL_DAY_DURATION_SECONDS as i64;
/// How often disgraces are matched against who is online.
const DISGRACE_CHECK_SECS: i64 = 10;
const DISGRACE_TITLE: &str = "estate_disgrace";
/// A chest's tier-gear roll pays this share of a dungeon chest's chances.
const TIER_CHANCE_SHARE: f32 = 1.0 / 3.0;
/// Damage, in the grade's average raider HP, a defender must deal to earn a share.
const SHARE_THRESHOLD_RAIDERS: u64 = 2;
/// Screen up, right, down, left under the fixed -45° yaw, clockwise.
const SIDE_ANGLES_DEG: [f32; 4] = [-45.0, 45.0, 135.0, 225.0];
const SIDE_NAMES: [&str; 4] = ["up", "right", "down", "left"];
/// How far a side's spawn may slide along the circle to find ground.
const SIDE_SLIDE_DEG: [f32; 7] = [0.0, 10.0, -10.0, 20.0, -20.0, 30.0, -30.0];
const RAIDER_SLIDE_DEG: [f32; 5] = [0.0, 5.0, -5.0, 10.0, -10.0];
/// Raiders of one side spread this far apart along the circle.
const RAIDER_SPACING_DEG: f32 = 9.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Held,
    BannerFell,
    Wiped,
    TimedOut,
}

impl Outcome {
    fn notice(self) -> PlayerMessage {
        let (code, message) = match self {
            Self::Held => (
                "held",
                "The banner held. A treasure chest spills its spoils.",
            ),
            Self::Wiped => ("wiped", "No defender is left standing. The banner fell."),
            Self::TimedOut => ("timedOut", "Time ran out. The banner fell."),
            Self::BannerFell => ("fell", "The banner fell."),
        };
        localized(&format!("server.estateDefense.{code}"), message)
    }
}

struct Raider {
    leader: bool,
    near_secs: u32,
}

pub(crate) struct EstateDefense {
    grade: &'static DefenseGrade,
    banner: Position,
    family_id: Option<i64>,
    summoner: i64,
    summoner_name: String,
    /// An admin's `/warhorn`: no title, no disgrace.
    practice: bool,
    /// Characters who may earn a share: the estate's family, the summoner's party.
    eligible: Arc<HashSet<i64>>,
    targets: Arc<HashSet<PlayerId>>,
    /// Each side's spawn angle in degrees, or `None` where no ground was found.
    sides: [Option<f32>; 4],
    first_side: usize,
    elapsed: u32,
    next_wave: usize,
    raiders: HashMap<String, Raider>,
    banner_hp: u8,
    dealt: HashMap<i64, u64>,
    spawned: u64,
    spawned_health: u64,
    empty_secs: u32,
}

impl EstateDefense {
    /// The side a wave meant for `side` lands on: the next one clockwise
    /// that has ground.
    fn landing_side(&self, side: usize) -> Option<usize> {
        (0..4)
            .map(|i| (side + i) % 4)
            .find(|s| self.sides[*s].is_some())
    }

    /// Spawn angles and leader flags of `wave`, and the side it comes from
    /// (`None` for the last, which comes from every side).
    fn wave_plan(&self, wave: usize) -> (Vec<(f32, bool)>, Option<usize>) {
        let mut plan = Vec::new();
        let mut place = |side: usize, count: usize, leader: bool| {
            let landing = self.landing_side(side);
            if let Some(angle) = landing.and_then(|s| self.sides[s]) {
                for i in 0..count {
                    let offset = (i as f32 - (count as f32 - 1.0) / 2.0) * RAIDER_SPACING_DEG;
                    plan.push((angle + offset, leader));
                }
            }
            landing
        };
        if wave + 1 < WAVE_AT.len() {
            let side = place((self.first_side + wave) % 4, WAVE_SIZE, false);
            (plan, side)
        } else {
            for side in 0..4 {
                place(side, FINAL_WAVE_PER_SIDE, false);
            }
            place(rand::thread_rng().gen_range(0..4), 1, true);
            (plan, None)
        }
    }
}

/// Active disgraces and, for each online player under one, the title they
/// had chosen.
#[derive(Default)]
pub(crate) struct DisgraceState {
    active: Disgraces,
    shown: HashMap<PlayerId, Option<String>>,
    next_check: i64,
}

fn refusal(code: &'static str) -> PlayerMessage {
    let message = match code {
        "notNight" => "The war horn only carries at night.",
        "notOwnEstate" => "Blow the war horn on your own estate.",
        "arrears" => "An estate in arrears can't be defended.",
        "notHead" => "Only the family head may blow the war horn on a family estate.",
        "notOutdoors" => "Blow the war horn outdoors on dry ground.",
        "notClear" => "Clear the ground first: something placed stands within 10 m.",
        "busy" => "This estate is already under attack.",
        "noRoom" => "There is no open ground around you for the enemy to gather.",
        "inCombat" => "Catch your breath before blowing the war horn.",
        _ => "You can't blow the war horn here.",
    };
    localized(&format!("server.estateDefense.{code}"), message)
}

fn ring_point(banner: &Position, angle_deg: f32) -> (f32, f32) {
    let angle = angle_deg.to_radians();
    (
        onlinerpg_shared::wrap_world_x(banner.x + CLEAR_RADIUS_M * angle.cos()),
        banner.z + CLEAR_RADIUS_M * angle.sin(),
    )
}

/// One chest share: each line rolled once, one of its items `quantity` times.
fn roll_share(
    grade: &DefenseGrade,
    tier_rolls: &[(String, f32)],
    rng: &mut impl Rng,
) -> Vec<String> {
    let mut items = Vec::new();
    for line in &grade.chest {
        if rng.gen::<f32>() >= line.chance {
            continue;
        }
        if let Some(item) = line.items.choose(rng) {
            items.extend(std::iter::repeat_n(item.clone(), line.quantity as usize));
        }
    }
    items.extend(crate::world_drop_defs::roll_independent(
        tier_rolls
            .iter()
            .map(|(id, chance)| (id.as_str(), chance * TIER_CHANCE_SHARE)),
        rng,
    ));
    items
}

impl GameState {
    fn defenses(&self) -> std::sync::MutexGuard<'_, HashMap<i64, EstateDefense>> {
        self.estate_defenses
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    fn disgrace_state(&self) -> std::sync::MutexGuard<'_, DisgraceState> {
        self.disgraces.lock().unwrap_or_else(|e| e.into_inner())
    }

    async fn notify(&self, ids: &[PlayerId], notice: &PlayerMessage) {
        for id in ids {
            self.send_system_message(id, notice.clone()).await;
        }
    }

    async fn notify_zone(&self, banner: &Position, notice: &PlayerMessage) {
        let ids = self
            .player_ids_within_position(banner, 0, ZONE_RADIUS_M)
            .await;
        self.notify(&ids, notice).await;
    }

    pub async fn blow_war_horn(&self, player_id: &PlayerId, instance_id: u64, auth: &AuthService) {
        if let Err(code) = self.try_blow_war_horn(player_id, instance_id, auth).await {
            self.send_system_message(player_id, refusal(code)).await;
        }
    }

    async fn try_blow_war_horn(
        &self,
        player_id: &PlayerId,
        instance_id: u64,
        auth: &AuthService,
    ) -> Result<(), &'static str> {
        if self
            .reject_if_trade_reserved(player_id, instance_id, "use")
            .await
            || self
                .reject_if_defeated(player_id, "You can't do that while defeated")
                .await
        {
            return Ok(());
        }
        let Some(grade) = self
            .inventories
            .read()
            .await
            .get(player_id)
            .and_then(|inv| inv.bag.iter().find(|i| i.instance_id == instance_id))
            .and_then(|item| grade_for_horn(&item.item_def_id))
        else {
            return Ok(());
        };
        if !is_night(&self.current_game_datetime()) {
            return Err("notNight");
        }
        let banner = self.banner_ground(player_id).await?;
        let character_id = self
            .character_id_of(player_id)
            .await
            .ok_or("notOwnEstate")?;
        let plot = super::land::plot_key(plot_addr(banner.x, banner.z));
        let lookup = auth.clone();
        let site: DefenseSite =
            match auth_db(move || lookup.estate_defense_site(character_id, plot)).await {
                Ok(Ok(site)) => site,
                Ok(Err(code)) => return Err(code),
                Err(error) => {
                    warn!(%error, "Failed to read estate defense site");
                    return Err("notOwnEstate");
                }
            };
        let defense = self
            .prepare_defense(player_id, character_id, grade, banner, &site, false)
            .await?;
        // The horn and the defense go together: a refused one keeps its horn.
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let Some(inv) = inventories.get_mut(player_id) else {
                return Ok(());
            };
            if !inv
                .bag
                .iter()
                .any(|i| i.instance_id == instance_id && i.item_def_id == grade.horn)
            {
                return Ok(());
            }
            let mut defenses = self.defenses();
            if defenses.contains_key(&site.estate_id) {
                return Err("busy");
            }
            consume_one(inv, instance_id);
            defenses.insert(site.estate_id, defense);
            inv.clone()
        };
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        self.log_consumed(player_id, [grade.horn.as_str()]).await;
        self.announce_defense(site.estate_id).await;
        Ok(())
    }

    /// `/warhorn N`: an admin's practice defense of grade N where they stand,
    /// on any ground and at any hour. It pays its chest but no title, and a
    /// loss leaves no disgrace.
    pub(super) async fn practice_defense_command(
        &self,
        admin_id: &PlayerId,
        grade: &str,
    ) -> Result<String, String> {
        let grades = crate::estate_defense_defs::estate_defense_grades();
        let index: usize = if grade.is_empty() { "1" } else { grade }
            .parse()
            .unwrap_or(0);
        let grade = index
            .checked_sub(1)
            .and_then(|i| grades.get(i))
            .ok_or_else(|| format!("Usage: /warhorn 1-{}", grades.len()))?;
        let character_id = self
            .character_id_of(admin_id)
            .await
            .ok_or("Character not found.")?;
        let failed = |code| refusal(code).message;
        let banner = self.banner_ground(admin_id).await.map_err(failed)?;
        let site = DefenseSite {
            estate_id: -character_id,
            family_id: None,
            members: vec![character_id],
        };
        let defense = self
            .prepare_defense(admin_id, character_id, grade, banner, &site, true)
            .await
            .map_err(failed)?;
        {
            let mut defenses = self.defenses();
            if defenses.contains_key(&site.estate_id) {
                return Err(failed("busy"));
            }
            defenses.insert(site.estate_id, defense);
        }
        self.announce_defense(site.estate_id).await;
        Ok(format!("Practice defense {} opened.", grade.id))
    }

    /// The outdoor dry ground the player stands on, out of combat.
    async fn banner_ground(&self, player_id: &PlayerId) -> Result<Position, &'static str> {
        let banner = {
            let players = self.players.read().await;
            let player = players.get(player_id).ok_or("notOutdoors")?;
            if Self::in_combat(player) {
                return Err("inCombat");
            }
            if player.floor_level != 0 {
                return Err("notOutdoors");
            }
            player.position
        };
        let y = self
            .outdoor_dry_ground(banner.x, banner.z)
            .await
            .ok_or("notOutdoors")?;
        Ok(Position { y, ..banner })
    }

    /// Ground height outside any house floor and out of water.
    async fn outdoor_dry_ground(&self, x: f32, z: f32) -> Option<f32> {
        if get_floor_y_base(&self.passability_read(), x, z, 0).is_some() {
            return None;
        }
        let (y, depth) = self.ground_and_depth_at(x, z).await?;
        (y.is_finite() && depth <= 0.1).then_some(y)
    }

    /// Everything about a defense but opening it: clear ground, the sides
    /// raiders can rise on, and who defends.
    async fn prepare_defense(
        &self,
        player_id: &PlayerId,
        character_id: i64,
        grade: &'static DefenseGrade,
        banner: Position,
        site: &DefenseSite,
        practice: bool,
    ) -> Result<EstateDefense, &'static str> {
        if self.defenses().contains_key(&site.estate_id) {
            return Err("busy");
        }
        if !self.ground_clear_around(&banner).await {
            return Err("notClear");
        }
        let mut sides = [None; 4];
        for (side, base) in SIDE_ANGLES_DEG.into_iter().enumerate() {
            sides[side] = self
                .first_ground(&banner, base, &SIDE_SLIDE_DEG)
                .await
                .map(|(angle, _)| angle);
        }
        if sides.iter().filter(|s| s.is_some()).count() < 3 {
            return Err("noRoom");
        }
        let summoner_name = self.player_name_of(player_id).await;
        let party: Vec<PlayerId> = self
            .parties
            .read()
            .await
            .party_of(player_id)
            .map(|p| p.members.clone())
            .unwrap_or_default();
        let mut eligible: HashSet<i64> = site.members.iter().copied().collect();
        eligible.insert(character_id);
        let mut targets = HashSet::new();
        for (pid, entry) in self.player_characters.read().await.iter() {
            if party.contains(pid) {
                eligible.insert(entry.0);
            }
            if eligible.contains(&entry.0) {
                targets.insert(*pid);
            }
        }
        Ok(EstateDefense {
            grade,
            banner,
            family_id: site.family_id,
            summoner: character_id,
            summoner_name,
            practice,
            eligible: Arc::new(eligible),
            targets: Arc::new(targets),
            sides,
            first_side: rand::thread_rng().gen_range(0..4),
            elapsed: 0,
            next_wave: 0,
            raiders: HashMap::new(),
            banner_hp: BANNER_DURABILITY,
            dealt: HashMap::new(),
            spawned: 0,
            spawned_health: 0,
            empty_secs: 0,
        })
    }

    async fn announce_defense(&self, estate_id: i64) {
        let Some((grade, banner, family_id, summoner_name, practice)) =
            self.defenses().get(&estate_id).map(|d| {
                (
                    d.grade,
                    d.banner,
                    d.family_id,
                    d.summoner_name.clone(),
                    d.practice,
                )
            })
        else {
            return;
        };
        info!(
            "Estate defense {} on estate {estate_id}: {summoner_name} blew {} at ({:.1},{:.1}); family {family_id:?}{}",
            grade.id,
            grade.horn,
            banner.x,
            banner.z,
            if practice { " (practice)" } else { "" }
        );
        let notice = localized(
            "server.estateDefense.started",
            format!("{summoner_name} blew the war horn. The enemy arrives in 30 seconds."),
        )
        .with_param("name", summoner_name);
        let audience = self.defense_audience(family_id, &banner).await;
        self.notify(&audience, &notice).await;
    }

    /// Family members online and anyone within the zone.
    async fn defense_audience(&self, family_id: Option<i64>, banner: &Position) -> Vec<PlayerId> {
        let mut audience = self
            .player_ids_within_position(banner, 0, ZONE_RADIUS_M)
            .await;
        if let Some(family_id) = family_id {
            for member in self.online_family_player_ids(family_id).await {
                if !audience.contains(&member) {
                    audience.push(member);
                }
            }
        }
        audience
    }

    /// No fence, house or furnishing stands within `CLEAR_RADIUS_M`.
    async fn ground_clear_around(&self, banner: &Position) -> bool {
        if !self
            .fences
            .read()
            .await
            .within(banner, CLEAR_RADIUS_M)
            .is_empty()
            || !self
                .estate_chests
                .read()
                .await
                .around(banner, CLEAR_RADIUS_M)
                .is_empty()
        {
            return false;
        }
        let (cx, cz) = crate::housing::world_to_chunk(banner.x, banner.z);
        let chunks = (-2..=2).flat_map(|dx| (-2..=2).map(move |dz| (cx + dx, cz + dz)));
        let reads =
            futures_util::future::join_all(chunks.map(|(x, z)| self.housing_io.read_chunk(x, z)))
                .await;
        reads.into_iter().all(|houses| {
            houses.is_ok_and(|houses| {
                houses
                    .iter()
                    .all(|h| super::house_building::distance_to_house(h, banner) >= CLEAR_RADIUS_M)
            })
        })
    }

    /// Ground a raider can rise on at `angle_deg` on the clear circle.
    async fn raider_ground(&self, banner: &Position, angle_deg: f32) -> Option<Position> {
        let (x, z) = ring_point(banner, angle_deg);
        if is_circle_blocked_on_floor(&self.passability_read(), x, z, 0.4, 0, None) {
            return None;
        }
        let y = self.outdoor_dry_ground(x, z).await?;
        Some(Position { x, y, z })
    }

    /// The first `angle + slide` with raider ground, and that ground.
    async fn first_ground(
        &self,
        banner: &Position,
        angle: f32,
        slides: &[f32],
    ) -> Option<(f32, Position)> {
        for slide in slides {
            if let Some(ground) = self.raider_ground(banner, angle + slide).await {
                return Some((angle + slide, ground));
            }
        }
        None
    }

    /// A hit on a raider counts toward its defense's shares.
    pub(super) async fn record_estate_defense_hit(
        &self,
        monster_id: &str,
        player_id: &PlayerId,
        dealt: u32,
    ) {
        if dealt == 0 {
            return;
        }
        let Some(character_id) = self.character_id_of(player_id).await else {
            return;
        };
        if let Some(defense) = self
            .defenses()
            .values_mut()
            .find(|d| d.raiders.contains_key(monster_id))
        {
            *defense.dealt.entry(character_id).or_default() += u64::from(dealt);
        }
    }

    /// Whether `(x, z)` lies in an open defense's clear circle, where nothing
    /// may be placed until it ends.
    pub(super) fn in_estate_defense_circle(&self, x: f32, z: f32) -> bool {
        let point = Position { x, y: 0.0, z };
        self.defenses()
            .values()
            .any(|d| d.banner.dist_xz_sq(&point) <= CLEAR_RADIUS_M * CLEAR_RADIUS_M)
    }

    /// Ambient spawns hold off defenders while a defense runs.
    pub(super) fn is_defending(&self, player_id: &PlayerId) -> bool {
        self.estate_defenders
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .contains(player_id)
    }

    pub async fn tick_estate_defenses(&self, auth: &AuthService) {
        let ids: Vec<i64> = self.defenses().keys().copied().collect();
        if !ids.is_empty() || self.is_defending_any() {
            let mut defenders = HashSet::new();
            for estate_id in ids {
                if let Some(outcome) = self.tick_estate_defense(estate_id, &mut defenders).await {
                    self.end_estate_defense(estate_id, outcome, auth).await;
                }
            }
            *self
                .estate_defenders
                .write()
                .unwrap_or_else(|e| e.into_inner()) = defenders;
        }
        self.tick_disgraces(false).await;
    }

    fn is_defending_any(&self) -> bool {
        !self
            .estate_defenders
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    async fn tick_estate_defense(
        &self,
        estate_id: i64,
        defenders: &mut HashSet<PlayerId>,
    ) -> Option<Outcome> {
        let (banner, eligible, elapsed, wave) = {
            let mut defenses = self.defenses();
            let defense = defenses.get_mut(&estate_id)?;
            defense.elapsed += ESTATE_DEFENSE_TICK.as_secs() as u32;
            let wave = (defense.next_wave < WAVE_AT.len()
                && defense.elapsed >= WAVE_AT[defense.next_wave])
                .then(|| {
                    defense.next_wave += 1;
                    defense.next_wave - 1
                });
            (
                defense.banner,
                defense.eligible.clone(),
                defense.elapsed,
                wave,
            )
        };
        if let Some(wave) = wave {
            self.spawn_wave(estate_id, wave).await;
        }

        let living = self.living_defenders(&banner, &eligible).await;
        defenders.extend(living.iter().copied());

        let raider_ids: Vec<String> = self
            .defenses()
            .get(&estate_id)?
            .raiders
            .keys()
            .cloned()
            .collect();
        // A raider held up fighting by the banner has not taken it: only one
        // standing or walking there unopposed counts.
        let positions: Vec<(String, Option<(Position, bool)>)> = {
            let monsters = self.monsters.read().await;
            raider_ids
                .into_iter()
                .map(|id| {
                    let alive = monsters
                        .get(&id)
                        .filter(|m| m.state != MonsterState::Dead)
                        .map(|m| {
                            let unopposed =
                                matches!(m.state, MonsterState::Idle | MonsterState::Walk);
                            (m.position, unopposed)
                        });
                    (id, alive)
                })
                .collect()
        };
        let mut breached = Vec::new();
        let (banner_hp, waves_done, raiders_left, empty_secs) = {
            let mut defenses = self.defenses();
            let defense = defenses.get_mut(&estate_id)?;
            for (id, raider_at) in positions {
                let Some((position, unopposed)) = raider_at else {
                    defense.raiders.remove(&id);
                    continue;
                };
                let Some(raider) = defense.raiders.get_mut(&id) else {
                    continue;
                };
                if unopposed && position.dist_xz_sq(&banner) <= BREACH_RADIUS_M * BREACH_RADIUS_M {
                    raider.near_secs += 1;
                    if raider.near_secs >= BREACH_SECS {
                        let blow = if raider.leader { LEADER_BREACH } else { 1 };
                        defense.banner_hp = defense.banner_hp.saturating_sub(blow);
                        breached.push(id);
                    }
                } else {
                    raider.near_secs = 0;
                }
            }
            for id in &breached {
                defense.raiders.remove(id);
            }
            if elapsed >= WAVE_AT[0] && living.is_empty() {
                defense.empty_secs += 1;
            } else {
                defense.empty_secs = 0;
            }
            (
                defense.banner_hp,
                defense.next_wave >= WAVE_AT.len(),
                defense.raiders.len(),
                defense.empty_secs,
            )
        };
        if !breached.is_empty() {
            self.despawn_monsters(breached).await;
            let notice = localized(
                "server.estateDefense.breach",
                format!("An enemy reached the banner. Banner {banner_hp}/{BANNER_DURABILITY}."),
            )
            .with_param("hp", banner_hp.to_string())
            .with_param("max", BANNER_DURABILITY.to_string());
            self.notify_zone(&banner, &notice).await;
        }
        if banner_hp == 0 {
            Some(Outcome::BannerFell)
        } else if empty_secs >= WIPE_SECS {
            Some(Outcome::Wiped)
        } else if waves_done && raiders_left == 0 && elapsed >= EARLIEST_SUCCESS {
            Some(Outcome::Held)
        } else if elapsed >= TIME_LIMIT {
            Some(Outcome::TimedOut)
        } else {
            None
        }
    }

    /// Eligible characters standing alive in the zone.
    async fn living_defenders(&self, banner: &Position, eligible: &HashSet<i64>) -> Vec<PlayerId> {
        let mut living = self.living_near(banner, ZONE_RADIUS_M).await;
        let characters = self.player_characters.read().await;
        living.retain(|id| characters.get(id).is_some_and(|c| eligible.contains(&c.0)));
        living
    }

    async fn spawn_wave(&self, estate_id: i64, wave: usize) {
        let Some((grade, banner, targets, (plan, side))) = self
            .defenses()
            .get(&estate_id)
            .map(|d| (d.grade, d.banner, d.targets.clone(), d.wave_plan(wave)))
        else {
            return;
        };
        let mut spawned = Vec::new();
        for (angle, leader) in plan {
            let Some((_, position)) = self.first_ground(&banner, angle, &RAIDER_SLIDE_DEG).await
            else {
                continue;
            };
            let (monster_type, level) = if leader {
                (grade.leader.clone(), grade.leader_level)
            } else {
                let mut rng = rand::thread_rng();
                (
                    grade
                        .monsters
                        .choose(&mut rng)
                        .cloned()
                        .unwrap_or_else(|| grade.leader.clone()),
                    rng.gen_range(grade.min_level..=grade.max_level),
                )
            };
            let rotation = position.bearing_xz_to(&banner).unwrap_or(0.0);
            if let Some(monster) = self
                .spawn_monster_with(
                    monster_type,
                    position,
                    rotation,
                    0,
                    MonsterLifecycle::EstateDefense,
                    Some(level),
                    false,
                    Some(Siege {
                        banner,
                        place_bonus: grade.place_bonus,
                        targets: targets.clone(),
                    }),
                )
                .await
            {
                spawned.push((monster.id, monster.max_health, leader));
            }
        }
        let count = spawned.len();
        if let Some(defense) = self.defenses().get_mut(&estate_id) {
            for (id, max_health, leader) in spawned {
                defense.spawned += 1;
                defense.spawned_health += u64::from(max_health);
                defense.raiders.insert(
                    id,
                    Raider {
                        leader,
                        near_secs: 0,
                    },
                );
            }
        }
        let (code, side) = match side {
            Some(side) => (
                format!("server.estateDefense.wave.{}", SIDE_NAMES[side]),
                SIDE_NAMES[side],
            ),
            None => ("server.estateDefense.waveAll".to_string(), "all"),
        };
        info!(
            "Estate defense on estate {estate_id}: wave {} ({side}), {count} raiders",
            wave + 1
        );
        let notice = localized(
            &code,
            format!("Wave {} of 5 comes from the {side}.", wave + 1),
        )
        .with_param("wave", (wave + 1).to_string());
        self.notify_zone(&banner, &notice).await;
    }

    async fn end_estate_defense(&self, estate_id: i64, outcome: Outcome, auth: &AuthService) {
        let Some(defense) = self.defenses().remove(&estate_id) else {
            return;
        };
        let leftover: Vec<String> = defense.raiders.keys().cloned().collect();
        if !leftover.is_empty() {
            self.despawn_monsters(leftover).await;
        }
        info!(
            "Estate defense {} on estate {estate_id} ended: {outcome:?} after {}s, banner {}/{BANNER_DURABILITY}, {} raiders",
            defense.grade.id, defense.elapsed, defense.banner_hp, defense.spawned
        );
        if outcome == Outcome::Held {
            self.reward_defenders(&defense, auth).await;
        }
        let audience = self
            .defense_audience(defense.family_id, &defense.banner)
            .await;
        self.notify(&audience, &outcome.notice()).await;
        if outcome != Outcome::Held && !defense.practice {
            self.disgrace(defense.family_id, defense.summoner, auth)
                .await;
        }
    }

    async fn reward_defenders(&self, defense: &EstateDefense, auth: &AuthService) {
        let threshold = defense.spawned_health / defense.spawned.max(1) * SHARE_THRESHOLD_RAIDERS;
        let living = self
            .living_defenders(&defense.banner, &defense.eligible)
            .await;
        let shares: Vec<(PlayerId, i64, u64)> = {
            let characters = self.player_characters.read().await;
            living
                .into_iter()
                .filter_map(|id| {
                    let character = characters.get(&id)?.0;
                    let dealt = defense.dealt.get(&character).copied().unwrap_or(0);
                    (dealt >= threshold).then_some((id, character, dealt))
                })
                .collect()
        };
        let tier_rolls = self.item_defs.chest_roll_table(defense.grade.chest_tier);
        for &(player_id, character_id, dealt) in &shares {
            let items = roll_share(defense.grade, &tier_rolls, &mut rand::thread_rng());
            info!(
                "Estate defense {}: {} dealt {dealt} (threshold {threshold}), share {items:?}",
                defense.grade.id,
                self.player_name_of(&player_id).await,
            );
            self.record_item_drops(&items, false);
            self.spawn_scattered_items(items, defense.banner, 0, 1.0, 3.0, player_id)
                .await;
            if !defense.practice {
                self.grant_title(character_id, &defense.grade.title, Some(auth))
                    .await;
            }
        }
        info!(
            "Estate defense {}: {} of {} eligible earned a share; summoner {}",
            defense.grade.id,
            shares.len(),
            defense.eligible.len(),
            defense.summoner_name
        );
    }

    async fn disgrace(&self, family_id: Option<i64>, summoner: i64, auth: &AuthService) {
        let until = unix_now() + DISGRACE_SECS;
        let store = auth.clone();
        if let Err(error) = auth_db(move || store.set_disgrace(family_id, summoner, until)).await {
            warn!(%error, "Failed to store estate defense disgrace");
        }
        {
            let mut state = self.disgrace_state();
            match family_id {
                Some(id) => state.active.families.insert(id, until),
                None => state.active.characters.insert(summoner, until),
            };
        }
        info!(
            "Estate defense disgrace for family {family_id:?} / character {summoner} until {until}"
        );
        self.tick_disgraces(true).await;
    }

    pub async fn load_disgraces(&self, auth: &AuthService) {
        let store = auth.clone();
        match auth_db(move || store.active_disgraces(unix_now())).await {
            Ok(active) => self.disgrace_state().active = active,
            Err(error) => warn!(%error, "Failed to load disgraces"),
        }
    }

    /// Every `DISGRACE_CHECK_SECS`, or at once when `now` is set, put the
    /// disgrace over each disgraced online player's title and lift the ones
    /// that ran out. The work scales with the disgraced, not with who is
    /// online.
    async fn tick_disgraces(&self, now: bool) {
        let time = unix_now();
        let (families, characters, any_shown) = {
            let mut state = self.disgrace_state();
            if !now && time < state.next_check {
                return;
            }
            state.next_check = time + DISGRACE_CHECK_SECS;
            state.active.families.retain(|_, until| *until > time);
            state.active.characters.retain(|_, until| *until > time);
            (
                state.active.families.keys().copied().collect::<Vec<_>>(),
                state
                    .active
                    .characters
                    .keys()
                    .copied()
                    .collect::<HashSet<_>>(),
                !state.shown.is_empty(),
            )
        };
        if families.is_empty() && characters.is_empty() && !any_shown {
            return;
        }
        let mut wanted = HashSet::new();
        for family_id in families {
            wanted.extend(self.online_family_player_ids(family_id).await);
        }
        if !characters.is_empty() {
            wanted.extend(
                self.player_characters
                    .read()
                    .await
                    .iter()
                    .filter(|(_, entry)| characters.contains(&entry.0))
                    .map(|(pid, _)| *pid),
            );
        }
        let (apply, lift) = {
            let mut state = self.disgrace_state();
            let apply: Vec<PlayerId> = wanted
                .iter()
                .filter(|id| !state.shown.contains_key(id))
                .copied()
                .collect();
            let lift: Vec<(PlayerId, Option<String>)> = state
                .shown
                .extract_if(|id, _| !wanted.contains(id))
                .collect();
            (apply, lift)
        };
        let notice = localized(
            "server.estateDefense.disgraced",
            "The banner was lost: for 10 days you are known as Who Lost the Banner.",
        );
        for player_id in apply {
            let chosen = self.shown_title(&player_id).await;
            self.disgrace_state().shown.insert(player_id, chosen);
            self.show_title(&player_id, Some(DISGRACE_TITLE.to_string()))
                .await;
            self.send_system_message(&player_id, notice.clone()).await;
        }
        for (player_id, chosen) in lift {
            self.show_title(&player_id, chosen).await;
        }
    }

    /// The title a disgrace hides, if one is shown.
    pub(super) fn title_behind_disgrace(&self, player_id: &PlayerId) -> Option<Option<String>> {
        self.disgrace_state().shown.get(player_id).cloned()
    }

    /// The disgrace hides the chosen title; a pick made meanwhile waits
    /// behind it. False when no disgrace is shown.
    pub(super) fn hold_title_behind_disgrace(
        &self,
        player_id: &PlayerId,
        title: &Option<String>,
    ) -> bool {
        match self.disgrace_state().shown.get_mut(player_id) {
            Some(chosen) => {
                *chosen = title.clone();
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_share_rolls_every_sure_line_and_scales_tier_gear() {
        let grade = &crate::estate_defense_defs::estate_defense_grades()[1];
        let mut rng = rand::rngs::mock::StepRng::new(0, 0);
        let share = roll_share(grade, &[("iron_sword".into(), 1.0)], &mut rng);
        assert_eq!(share.iter().filter(|i| *i == "healing_potion").count(), 3);
        assert_eq!(share.iter().filter(|i| *i == "coin_pile").count(), 3);
        assert!(share.iter().any(|i| i.starts_with("scroll_of_")));
        assert!(share.contains(&"iron_sword".to_string()));
    }

    #[test]
    fn a_defense_lasts_at_least_until_the_last_wave_is_down() {
        assert!(EARLIEST_SUCCESS > *WAVE_AT.last().unwrap());
        assert!(TIME_LIMIT > EARLIEST_SUCCESS);
        assert_eq!(WAVE_SIZE * 4 + FINAL_WAVE_PER_SIDE * 4 + 1, 37);
    }
}
