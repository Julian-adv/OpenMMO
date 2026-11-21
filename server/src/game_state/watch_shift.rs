//! Day recruits hired by the guard captain to walk a watch (doc/ROAD_PATROL.md).

use std::collections::HashMap;

use onlinerpg_shared::messages::{
    PatrolMuster, WatchDuty, WatchGear, WatchNote, WatchStage, WatchTool,
    WATCH_CALL_BACK_WAIT_SECS, WATCH_ROLL_CALL_M,
};
use onlinerpg_shared::schedule::{ScheduleCondition, GUARD_CAPTAIN, WATCH_GEAR_RACK};
use tracing::{info, warn};

use crate::auth::{AuthService, WatchHire, WatchShiftRow};
use crate::metrics::GoldSource;
use crate::npc_defs::npc_defs;
use crate::types::{PlayerId, Position, ServerMessage};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::inventory::ItemInstance;

use super::inventory::{stack_into_bag, BagInsert};
use super::localization::{format_copper, localized, PlayerMessage};
use super::road_patrol::{is_registry_npc, NpcReach, GUARD_CLASS};
use super::{auth_db, GameState};

/// Real milliseconds between the waits a sergeant gives one recruit.
const CALL_BACK_EVERY_MS: u64 = 300_000;
/// A sergeant further than this from his last call was carried; nobody is marked absent.
const CARRIED_M: f32 = 60.0;
/// Game minutes without a roll call after which a watch is taken to be home.
const QUIET_MIN: f64 = 45.0;
/// Game minutes a watch can last; a sergeant unheard since boot is home by then.
const WATCH_MAX_MIN: f64 = 840.0;
/// Paid for a watch with every roll call answered.
pub(super) const FULL_WAGE: i64 = 20_000;
/// The most paid for a watch whose lent gear did not come back.
pub(super) const LOST_GEAR_WAGE: i64 = 100;
/// Roll calls between saves of a recruit who keeps answering.
const SAVE_EVERY_CALLS: u32 = 10;
/// Game minutes an account rests after being paid.
const REST_MIN: f64 = 480.0;
/// Game minutes after its watch was due to march that an unfinished shift is forgotten.
const HIRE_LAPSES_MIN: f64 = 1440.0;
/// Game minutes the captain holds an unclaimed wage: one turn of Serin.
const WAGE_HELD_MIN: f64 = (onlinerpg_shared::moon::SERIN_PERIOD_DAYS * 1440) as f64;
const TOOL_COOLDOWN_MS: u64 = 3_000;
/// How far from the sergeant's last roll call the lent spear still works.
const SPEAR_REACH_M: f32 = 30.0;
/// Game minutes before its nominal time that a watch may march.
const MARCH_EARLY_MIN: f64 = 30.0;
/// Game minutes before the march that the watch stands reporting to the captain.
const REPORT_MIN: f64 = 18.0;
pub(super) const SPEAR_ITEM: &str = "watch_spear";
pub(super) const SHOVEL_ITEM: &str = "watch_shovel";
pub(super) const TABARD_ITEM: &str = "watch_tabard";
const WATCH_GEAR: [&str; 3] = [SPEAR_ITEM, TABARD_ITEM, SHOVEL_ITEM];
/// The crest printed on the lent tabard.
const WATCH_CREST: &str = "aldermark";

/// The share of the wage for the calls answered, halved once any was missed.
pub(super) fn watch_wage(answered: u32, calls: u32) -> i64 {
    let share = FULL_WAGE * i64::from(answered) / i64::from(calls.max(1));
    if answered < calls {
        share / 2
    } else {
        share
    }
}

fn roll_key(name: &str) -> String {
    name.to_ascii_lowercase()
}

fn minute_of_day(game_min: f64) -> u32 {
    (game_min as i64).rem_euclid(1440) as u32
}

fn clock(game_min: f64) -> String {
    let minute = minute_of_day(game_min);
    format!("{:02}:{:02}", minute / 60, minute % 60)
}

struct WatchShift {
    row: WatchShiftRow,
    tool_at_ms: u64,
    /// The captain has been told to present this recruit to the watch.
    introduced: bool,
    /// When the sergeant last waited for this recruit.
    called_back_ms: u64,
    /// That roll call is still waiting to be counted.
    held: bool,
    /// Out of the sergeant's reach at the last roll call.
    away: bool,
    /// That absence has cost a roll call.
    away_docked: bool,
}

/// The last roll call a sergeant made.
struct RollCall {
    patrol_id: i64,
    at: Position,
    at_min: f64,
    /// Roll calls of this watch so far, this one included.
    calls: u32,
}

/// Recruits by lowercased character name, sergeants by name.
#[derive(Default)]
pub(crate) struct WatchRollState {
    shifts: HashMap<String, WatchShift>,
    sergeants: HashMap<String, RollCall>,
}

pub(crate) type WatchRoll = std::sync::Mutex<WatchRollState>;

impl WatchShift {
    fn new(row: WatchShiftRow) -> Self {
        Self {
            row,
            tool_at_ms: 0,
            introduced: false,
            called_back_ms: 0,
            held: false,
            away: false,
            away_docked: false,
        }
    }

    fn lapsed(&self, now_min: f64) -> bool {
        let kept = if self.row.done {
            WAGE_HELD_MIN
        } else {
            HIRE_LAPSES_MIN
        };
        now_min - self.row.departs_min > kept
    }

    /// Each kind of gear with whether it is lent out.
    fn gear(&self) -> [(&'static str, bool); 3] {
        [
            (SPEAR_ITEM, self.row.spear),
            (TABARD_ITEM, self.row.tabard),
            (SHOVEL_ITEM, self.row.shovel),
        ]
    }

    /// Lent kinds not yet back on the rack.
    fn owed(&self) -> Vec<&'static str> {
        self.gear()
            .into_iter()
            .filter_map(|(kind, out)| out.then_some(kind))
            .collect()
    }

    fn lent(&mut self, kind: &str) -> Option<&mut bool> {
        match kind {
            SPEAR_ITEM => Some(&mut self.row.spear),
            TABARD_ITEM => Some(&mut self.row.tabard),
            SHOVEL_ITEM => Some(&mut self.row.shovel),
            _ => None,
        }
    }

    fn duty(&self) -> WatchDuty {
        let stage = match (self.row.done, self.row.calls) {
            (true, _) => WatchStage::Done,
            (false, 0) => WatchStage::Waiting,
            (false, _) => WatchStage::OnWatch,
        };
        let gear = if self.owed().is_empty() {
            WatchGear::Unissued
        } else {
            WatchGear::Issued
        };
        WatchDuty {
            stage,
            gear,
            calls: self.row.calls,
            answered: self.row.answered,
            shovel: self.row.shovel,
        }
    }
}

impl WatchRollState {
    /// Whether the watch the recruit marched with has come home.
    fn home(&self, shift: &WatchShift, now_min: f64) -> bool {
        if shift.row.done {
            return true;
        }
        shift.row.calls > 0
            && match self.sergeants.get(&shift.row.leader) {
                Some(call) => now_min - call.at_min > QUIET_MIN,
                None => now_min - shift.row.departs_min > WATCH_MAX_MIN,
            }
    }
}

struct OpenWatch {
    sergeant: String,
    departs_min: f64,
}

/// A recruit's shift as the rack sees it.
struct RackView {
    on_rack: Vec<&'static str>,
    owed: Vec<&'static str>,
    /// Pieces may go back: before the march, or once home.
    returnable: bool,
    home: bool,
}

enum Standing {
    Unlisted,
    Lapsed(WatchShift),
    /// Not marched yet.
    Waiting {
        geared: bool,
        sergeant: String,
        departs_min: f64,
    },
    OnWatch,
    /// Home, with the lent gear not yet back on the rack.
    GearOut,
    Home(WatchShift),
}

/// What the captain's visit came to, for the recruit and for the captain.
struct Verdict {
    recruit: Option<PlayerMessage>,
    captain: WatchNote,
    /// Whether the roll changed; an unasked captain hears only of those.
    changed: bool,
}

/// One recruit after a roll call.
struct Answer {
    player_id: Option<PlayerId>,
    mark: Option<Marked>,
    save: Option<WatchShiftRow>,
    duty: WatchDuty,
}

/// What a roll call did to one recruit.
enum Marked {
    Joined,
    JoinedLate,
    CalledBack,
    Missed,
    Home,
}

impl GameState {
    fn watch_home_notice() -> PlayerMessage {
        localized(
            "server.watchDone",
            "The watch is home. Put the gear back on the rack by the watch house door to be paid.",
        )
    }

    async fn watch_roll(&self, auth: &AuthService) -> &WatchRoll {
        self.watch_roll
            .get_or_init(|| async {
                let db = auth.clone();
                let rows = auth_db(move || db.watch_shifts())
                    .await
                    .unwrap_or_else(|error| {
                        warn!(%error, "Failed to load the watch roll");
                        Vec::new()
                    });
                let shifts = rows
                    .into_iter()
                    .map(|row| (roll_key(&row.name), WatchShift::new(row)));
                std::sync::Mutex::new(WatchRollState {
                    shifts: shifts.collect(),
                    sergeants: HashMap::new(),
                })
            })
            .await
    }

    async fn lock_roll(&self, auth: &AuthService) -> std::sync::MutexGuard<'_, WatchRollState> {
        self.watch_roll(auth)
            .await
            .lock()
            .expect("watch roll lock poisoned")
    }

    async fn save_watch_shift(&self, row: WatchShiftRow, auth: &AuthService) {
        let db = auth.clone();
        let character_id = row.character_id;
        if let Err(error) = auth_db(move || db.save_watch_shift(&row)).await {
            warn!(%error, character_id, "Failed to save a watch shift");
        }
    }

    async fn send_watch_duty(&self, player_id: &PlayerId, duty: Option<WatchDuty>) {
        self.send_direct_message(player_id, ServerMessage::WatchDuty { duty })
            .await;
    }

    /// Tell a player entering the game where they stand on the roll; they
    /// are not on the roster yet, so the name is passed in.
    pub async fn greet_watch_recruit(&self, player_id: &PlayerId, name: &str, auth: &AuthService) {
        let duty = {
            let roll = self.lock_roll(auth).await;
            roll.shifts.get(&roll_key(name)).map(WatchShift::duty)
        };
        self.send_watch_duty(player_id, duty).await;
    }

    /// The sergeant calls the roll: recruits beside him are marked present.
    pub async fn patrol_roll_call(
        &self,
        guard_id: &PlayerId,
        muster: PatrolMuster,
        patrol_id: i64,
        auth: &AuthService,
    ) {
        let Ok(at) = self.surface_guard(guard_id).await else {
            return;
        };
        let sergeant = self.player_name_of(guard_id).await;
        let now = self.game_minutes_now();
        let mut gone_home: Vec<(String, WatchShiftRow, WatchDuty)> = Vec::new();
        let (carried, called): (bool, Vec<String>) = {
            let mut roll = self.lock_roll(auth).await;
            let earlier = match (muster, roll.sergeants.get(&sergeant)) {
                (PatrolMuster::Out, _) | (_, None) => 0,
                (_, Some(last)) => last.calls,
            };
            let call = RollCall {
                patrol_id,
                at,
                at_min: now,
                calls: earlier + u32::from(muster != PatrolMuster::Recheck),
            };
            let last = roll.sergeants.insert(sergeant.clone(), call);
            let carried = last
                .as_ref()
                .is_none_or(|last| last.at.dist_xz_sq(&at) > CARRIED_M * CARRIED_M);
            // A watch that went quiet without calling its recruits home is home.
            let quiet = last.is_some_and(|last| now - last.at_min > QUIET_MIN);
            let called = roll
                .shifts
                .iter_mut()
                .filter(|(_, shift)| shift.row.leader == sergeant && !shift.row.done)
                .filter(|(_, shift)| !shift.lapsed(now))
                .filter_map(|(name, shift)| {
                    if shift.row.calls > 0 && quiet {
                        shift.row.done = true;
                        gone_home.push((name.clone(), shift.row.clone(), shift.duty()));
                        return None;
                    }
                    // Hired for a later march of this watch: not theirs yet.
                    let theirs =
                        shift.row.calls > 0 || now >= shift.row.departs_min - MARCH_EARLY_MIN;
                    theirs.then(|| name.clone())
                })
                .collect();
            (carried, called)
        };
        for (name, row, duty) in gone_home {
            self.save_watch_shift(row, auth).await;
            if let Some(id) = self.player_id_by_name(&name).await {
                self.send_system_message(&id, Self::watch_home_notice())
                    .await;
                self.send_watch_duty(&id, Some(duty)).await;
            }
        }
        let online: Vec<(String, Option<PlayerId>)> = {
            let ids = self.player_ids_by_name.read().await;
            called
                .into_iter()
                .map(|name| {
                    let id = ids.get(&name).copied();
                    (name, id)
                })
                .collect()
        };
        let answered: Vec<(String, Option<PlayerId>, bool)> = {
            let players = self.players.read().await;
            online
                .into_iter()
                .map(|(name, id)| {
                    let present = id.and_then(|id| players.get(&id)).is_some_and(|recruit| {
                        recruit.health > 0
                            && recruit.floor_level == 0
                            && recruit.position.dist_xz_sq(&at)
                                <= WATCH_ROLL_CALL_M * WATCH_ROLL_CALL_M
                    });
                    (name, id, present)
                })
                .collect()
        };
        let now_ms = Self::now_ms();
        let mut called_back = Vec::new();
        let (mut rejoined, mut late) = (Vec::new(), Vec::new());
        let marked: Vec<Answer> = {
            let mut roll = self.lock_roll(auth).await;
            let watch_calls = roll.sergeants.get(&sergeant).map_or(1, |call| call.calls);
            answered
                .into_iter()
                .filter_map(|(name, id, present)| {
                    let shift = roll.shifts.get_mut(&name)?;
                    let home = muster == PatrolMuster::Home;
                    let recheck = muster == PatrolMuster::Recheck;
                    let mark = if shift.row.calls == 0 {
                        if recheck {
                            return None;
                        }
                        if present {
                            shift.row.calls = watch_calls;
                            shift.row.answered = 1;
                            Some(if watch_calls == 1 {
                                Marked::Joined
                            } else {
                                Marked::JoinedLate
                            })
                        } else if home {
                            // Never marched: the place frees with nothing earned.
                            shift.row.calls = watch_calls;
                            None
                        } else {
                            return None;
                        }
                    } else {
                        if recheck && !shift.held {
                            return None;
                        }
                        let clean = shift.row.answered == shift.row.calls;
                        let here = present || carried;
                        let count = |row: &mut WatchShiftRow| {
                            row.calls += 1;
                            row.answered += u32::from(here);
                        };
                        // The call the sergeant waited on is counted now.
                        if std::mem::take(&mut shift.held) {
                            count(&mut shift.row);
                        }
                        let waits = !recheck
                            && !here
                            && !home
                            && now_ms.saturating_sub(shift.called_back_ms) >= CALL_BACK_EVERY_MS;
                        if waits {
                            shift.held = true;
                            shift.called_back_ms = now_ms;
                            called_back.push(shift.row.name.clone());
                        } else if !recheck {
                            count(&mut shift.row);
                        }
                        if present {
                            if std::mem::take(&mut shift.away) {
                                let back = if std::mem::take(&mut shift.away_docked) {
                                    &mut late
                                } else {
                                    &mut rejoined
                                };
                                back.push(shift.row.name.clone());
                            }
                        } else if !carried {
                            shift.away = true;
                            shift.away_docked |= shift.row.answered < shift.row.calls && !waits;
                        }
                        if waits {
                            Some(Marked::CalledBack)
                        } else if clean && shift.row.answered < shift.row.calls {
                            Some(Marked::Missed)
                        } else {
                            None
                        }
                    };
                    let row = &mut shift.row;
                    row.done = home;
                    let save = matches!(
                        mark,
                        Some(Marked::Joined | Marked::JoinedLate | Marked::Missed)
                    ) || home
                        || row.calls % SAVE_EVERY_CALLS == 0;
                    let mark = if home { Some(Marked::Home) } else { mark };
                    Some(Answer {
                        player_id: id,
                        mark,
                        save: save.then(|| row.clone()),
                        duty: shift.duty(),
                    })
                })
                .collect()
        };
        self.send_direct_message(
            guard_id,
            ServerMessage::PatrolRollCalled {
                missing: called_back,
                rejoined,
                late,
            },
        )
        .await;
        for answer in marked {
            if let Some(row) = answer.save {
                self.save_watch_shift(row, auth).await;
            }
            let Some(player_id) = answer.player_id else {
                continue;
            };
            let message = answer.mark.map(|mark| match mark {
                Marked::Joined => localized(
                    "server.watchJoined",
                    "You march out with the watch. Stay beside the sergeant at every roll call until the watch is home.",
                ),
                Marked::JoinedLate => localized(
                    "server.watchJoinedLate",
                    "You fall in with the watch late; the roll calls before now are missed.",
                ),
                Marked::CalledBack => localized(
                    "server.watchCalledBack",
                    format!(
                        "The sergeant calls you back. Get within {WATCH_ROLL_CALL_M:.0} paces of him in {WATCH_CALL_BACK_WAIT_SECS} seconds."
                    ),
                )
                .with_param("seconds", WATCH_CALL_BACK_WAIT_SECS.to_string()),
                Marked::Missed => localized(
                    "server.watchMissed",
                    "You missed a roll call. The wage is now halved and counted by the calls you answer.",
                ),
                Marked::Home => Self::watch_home_notice(),
            });
            if let Some(message) = message {
                self.send_system_message(&player_id, message).await;
            }
            self.send_watch_duty(&player_id, Some(answer.duty)).await;
        }
    }

    /// Sergeants by name with the minute of the day their watch marches.
    fn patrol_watches(&self) -> Vec<(String, u32)> {
        let schedules = self
            .npc_schedules
            .read()
            .expect("npc schedules lock poisoned");
        schedules
            .iter()
            .filter_map(|(name, entries)| {
                entries
                    .iter()
                    .find_map(|entry| match (&entry.patrol, &entry.condition) {
                        (Some(patrol), Some(ScheduleCondition::TimeRange { start_minute, .. }))
                            if patrol.sweeps =>
                        {
                            Some((name.clone(), *start_minute))
                        }
                        _ => None,
                    })
            })
            .collect()
    }

    /// The next watch to march that has no recruit yet.
    async fn open_watch(&self, now: f64, auth: &AuthService) -> Option<OpenWatch> {
        let minute = minute_of_day(now);
        let wait = |departs: u32| (departs + 1440 - minute) % 1440;
        let mut watches = self.patrol_watches();
        watches.sort_by_key(|(_, departs)| wait(*departs));
        let roll = self.lock_roll(auth).await;
        watches
            .into_iter()
            .find(|(sergeant, _)| {
                !roll.shifts.values().any(|shift| {
                    shift.row.leader == *sergeant && !roll.home(shift, now) && !shift.lapsed(now)
                })
            })
            .map(|(sergeant, departs)| OpenWatch {
                sergeant,
                departs_min: now.floor() + f64::from(wait(departs)),
            })
    }

    /// Whether the player stands with the captain, telling whoever asked when not.
    async fn before_the_captain(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        spoken: bool,
    ) -> Option<String> {
        let (name, recruitable) = {
            let players = self.players.read().await;
            if spoken
                && !players
                    .get(npc_player_id)
                    .is_some_and(|npc| is_registry_npc(npc, GUARD_CAPTAIN))
            {
                return None;
            }
            match players.get(player_id) {
                Some(player) => (player.name.clone(), !player.is_official_npc),
                None => (String::new(), false),
            }
        };
        let reach = self
            .at_registry_npc(player_id, npc_player_id, GUARD_CAPTAIN)
            .await;
        if recruitable && reach.is_ok() {
            return Some(name);
        }
        if spoken {
            let note = if name.is_empty() {
                WatchNote::VisitorGone
            } else {
                WatchNote::NotStanding
            };
            self.send_watch_note(npc_player_id, &name, note).await;
        } else if let Err(reach) = reach {
            let message = match reach {
                NpcReach::TooFar => localized(
                    "server.watchTooFar",
                    "Stand with the Captain of the Guard to ask about watch work.",
                ),
                NpcReach::Asleep => localized(
                    "server.watchAsleep",
                    "The Captain of the Guard is asleep. Come back when he is at his post.",
                ),
            };
            self.send_system_message(player_id, message).await;
        }
        None
    }

    /// The captain offers a watch or settles the one walked; `spoken` when he asked for it.
    pub async fn watch_shift_visit(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        spoken: bool,
        auth: &AuthService,
    ) {
        let Some(name) = self
            .before_the_captain(player_id, npc_player_id, spoken)
            .await
        else {
            return;
        };
        let verdict = self
            .settle_or_offer(player_id, npc_player_id, &name, auth)
            .await;
        let captain = (spoken || verdict.changed).then_some(*npc_player_id);
        self.deliver(verdict, player_id, &name, captain).await;
    }

    async fn send_watch_note(&self, guard_id: &PlayerId, recruit: &str, note: WatchNote) {
        let recruit = recruit.to_string();
        self.send_direct_message(guard_id, ServerMessage::WatchVerdict { recruit, note })
            .await;
    }

    async fn deliver(
        &self,
        verdict: Verdict,
        player_id: &PlayerId,
        name: &str,
        captain: Option<PlayerId>,
    ) {
        if let Some(recruit) = verdict.recruit {
            self.send_system_message(player_id, recruit).await;
        }
        if let Some(captain) = captain {
            self.send_watch_note(&captain, name, verdict.captain).await;
        }
    }

    /// The player said yes to the captain's terms.
    pub async fn watch_shift_accept(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        sergeant: &str,
        auth: &AuthService,
    ) {
        let Some(name) = self
            .before_the_captain(player_id, npc_player_id, false)
            .await
        else {
            return;
        };
        let now = self.game_minutes_now();
        let listed = self
            .lock_roll(auth)
            .await
            .shifts
            .contains_key(&roll_key(&name));
        let open = match self.open_watch(now, auth).await {
            Some(watch) if !listed && watch.sergeant == sergeant => Some(watch),
            _ => None,
        };
        // Listed already, or the place offered went meanwhile: start over.
        let verdict = match open {
            Some(watch) => {
                self.hire_for_watch(player_id, &name, watch, now, auth)
                    .await
            }
            None => {
                self.settle_or_offer(player_id, npc_player_id, &name, auth)
                    .await
            }
        };
        let captain = verdict.changed.then_some(*npc_player_id);
        self.deliver(verdict, player_id, &name, captain).await;
    }

    async fn settle_or_offer(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        name: &str,
        auth: &AuthService,
    ) -> Verdict {
        let key = roll_key(name);
        let now = self.game_minutes_now();
        let standing = {
            let mut roll = self.lock_roll(auth).await;
            match roll.shifts.remove(&key) {
                None => Standing::Unlisted,
                Some(shift) if shift.lapsed(now) => Standing::Lapsed(shift),
                Some(shift) if roll.home(&shift, now) => {
                    if shift.owed().is_empty() {
                        Standing::Home(shift)
                    } else {
                        roll.shifts.insert(key, shift);
                        Standing::GearOut
                    }
                }
                Some(shift) => {
                    let standing = if shift.row.calls == 0 {
                        Standing::Waiting {
                            geared: shift.row.spear,
                            sergeant: shift.row.leader.clone(),
                            departs_min: shift.row.departs_min,
                        }
                    } else {
                        Standing::OnWatch
                    };
                    roll.shifts.insert(key, shift);
                    standing
                }
            }
        };
        match standing {
            Standing::Unlisted => self.offer_watch(player_id, npc_player_id, now, auth).await,
            Standing::Lapsed(shift) => {
                self.strike_off(player_id, &shift.row, auth).await;
                self.offer_watch(player_id, npc_player_id, now, auth).await
            }
            Standing::Waiting {
                geared,
                sergeant,
                departs_min,
            } => Verdict {
                recruit: Some({
                    let departs = clock(departs_min);
                    if geared {
                        localized(
                            "server.watchWaiting",
                            format!(
                                "You are on the roll and have your gear. Your watch marches at {departs} and reports to the Captain just before; stay beside the sergeant from then on."
                            ),
                        )
                    } else {
                        localized(
                            "server.watchTakeGear",
                            format!(
                                "You are on the roll. Take your gear from the rack by the watch house door before your watch marches at {departs}."
                            ),
                        )
                    }
                    .with_param("departs", departs)
                }),
                captain: WatchNote::Waiting {
                    sergeant,
                    departs_minute: minute_of_day(departs_min),
                    geared,
                },
                changed: false,
            },
            Standing::OnWatch => Verdict {
                recruit: Some(localized(
                    "server.watchOut",
                    "The watch is still out. Get back to the sergeant.",
                )),
                captain: WatchNote::StillOut,
                changed: false,
            },
            Standing::GearOut => Verdict {
                recruit: Some(localized(
                    "server.watchReturnFirst",
                    "Put the lent gear back on the rack by the watch house door first, then report.",
                )),
                captain: WatchNote::GearOut,
                changed: false,
            },
            Standing::Home(shift) => {
                self.pay_for_watch(player_id, name, shift, true, now, auth)
                    .await
            }
        }
    }

    async fn close_watch_shift(
        &self,
        character_id: i64,
        paid_min: Option<f64>,
        auth: &AuthService,
    ) {
        let db = auth.clone();
        if let Err(error) = auth_db(move || db.close_watch(character_id, paid_min)).await {
            warn!(%error, character_id, "Failed to close a watch shift");
        }
    }

    /// Put one unit in the bag if the player can carry it; its instance id.
    async fn lend_item(&self, player_id: &PlayerId, item_def_id: &str) -> Option<u64> {
        // Only a cape takes the watch's crest.
        let crest = self
            .item_defs
            .get(item_def_id)
            .filter(|def| def.is_cape())
            .and_then(|_| self.cape_textures.crest(WATCH_CREST));
        let max_weight = self.max_carry_weight(player_id).await;
        let armor_mult = self.armor_weight_mult(player_id).await;
        let weight = self.item_defs.weight_with(item_def_id, armor_mult);
        let instance_id = self.reserve_instance_ids(1).await;
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let inventory = inventories.get_mut(player_id)?;
            if self.calc_total_weight(inventory, armor_mult) + weight > max_weight {
                return None;
            }
            let mut insert = BagInsert::one(false, item_def_id, 0, instance_id);
            insert.cape_texture = crest.map(|crest| crest.texture.clone());
            insert.cape_color = crest.map(|crest| crest.color.to_string());
            stack_into_bag(&mut inventory.bag, insert);
            inventory.clone()
        };
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        Some(instance_id)
    }

    /// Take every item of these kinds out of the bag and off the body; whether any was there.
    async fn remove_items(&self, player_id: &PlayerId, kinds: &[&str]) -> bool {
        let is_kind = |item: &ItemInstance| kinds.contains(&item.item_def_id.as_str());
        let snapshot = {
            let mut inventories = self.inventories.write().await;
            let Some(inventory) = inventories.get_mut(player_id) else {
                return false;
            };
            let before = inventory.bag.len() + inventory.equipped.len();
            inventory.bag.retain(|item| !is_kind(item));
            inventory.equipped.retain(|_, item| !is_kind(item));
            if before == inventory.bag.len() + inventory.equipped.len() {
                return false;
            }
            inventory.clone()
        };
        self.mark_inventory_dirty(player_id).await;
        self.send_inventory_snapshot(player_id, snapshot).await;
        true
    }

    /// Off the roll unpaid, the lent gear taken back wherever the recruit stands.
    async fn strike_off(&self, player_id: &PlayerId, row: &WatchShiftRow, auth: &AuthService) {
        self.remove_items(player_id, &WATCH_GEAR).await;
        self.close_watch_shift(row.character_id, None, auth).await;
        self.send_watch_duty(player_id, None).await;
    }

    /// Whether the account may be hired now, as a refusal when it may not.
    async fn hire_refusal(
        &self,
        character_id: i64,
        watch: Option<(String, f64)>,
        now: f64,
        auth: &AuthService,
    ) -> Option<Verdict> {
        let db = auth.clone();
        let hired = auth_db(move || {
            db.hire_watch(
                character_id,
                watch,
                now,
                REST_MIN,
                now - HIRE_LAPSES_MIN,
                now - WAGE_HELD_MIN,
            )
        })
        .await;
        match hired {
            Err(error) => {
                warn!(%error, character_id, "Failed to hire for the watch");
                Some(Verdict {
                    recruit: Some(localized(
                        "server.watchUnavailable",
                        "The watch roll is unavailable.",
                    )),
                    captain: WatchNote::RollUnavailable,
                    changed: false,
                })
            }
            Ok(WatchHire::Resting) => Some(Verdict {
                recruit: Some(localized(
                    "server.watchResting",
                    "You have just come off a watch. Rest, and ask again later.",
                )),
                captain: WatchNote::Resting,
                changed: false,
            }),
            Ok(WatchHire::Taken(other)) => Some(Verdict {
                recruit: Some(
                    localized(
                        "server.watchTaken",
                        format!("{other} is already on the roll for your account."),
                    )
                    .with_param("name", other.clone()),
                ),
                captain: WatchNote::Taken { other },
                changed: false,
            }),
            Ok(WatchHire::Hired) => None,
        }
    }

    fn watch_full() -> Verdict {
        Verdict {
            recruit: Some(localized(
                "server.watchFull",
                "Both watches already have their recruit. Ask again another day.",
            )),
            captain: WatchNote::Full,
            changed: false,
        }
    }

    /// Show the player the terms of the next open watch.
    async fn offer_watch(
        &self,
        player_id: &PlayerId,
        npc_player_id: &PlayerId,
        now: f64,
        auth: &AuthService,
    ) -> Verdict {
        let Some(OpenWatch {
            sergeant,
            departs_min,
        }) = self.open_watch(now, auth).await
        else {
            return Self::watch_full();
        };
        let Some(character_id) = self.character_id_of(player_id).await else {
            return Self::watch_full();
        };
        if let Some(refusal) = self.hire_refusal(character_id, None, now, auth).await {
            return refusal;
        }
        self.send_direct_message(
            player_id,
            ServerMessage::WatchOffered {
                npc_player_id: *npc_player_id,
                sergeant: sergeant.clone(),
                departs_minute: minute_of_day(departs_min),
                wage: FULL_WAGE,
            },
        )
        .await;
        Verdict {
            recruit: None,
            captain: WatchNote::Offered { sergeant },
            changed: false,
        }
    }

    async fn hire_for_watch(
        &self,
        player_id: &PlayerId,
        name: &str,
        watch: OpenWatch,
        now: f64,
        auth: &AuthService,
    ) -> Verdict {
        let OpenWatch {
            sergeant,
            departs_min,
        } = watch;
        let Some(character_id) = self.character_id_of(player_id).await else {
            return Self::watch_full();
        };
        let listing = Some((sergeant.clone(), departs_min));
        if let Some(refusal) = self.hire_refusal(character_id, listing, now, auth).await {
            return refusal;
        }
        let row = WatchShiftRow {
            character_id,
            name: name.to_string(),
            departs_min,
            leader: sergeant.clone(),
            ..Default::default()
        };
        let shift = WatchShift::new(row);
        let duty = shift.duty();
        self.lock_roll(auth)
            .await
            .shifts
            .insert(roll_key(name), shift);
        info!(character_id, name, sergeant, "watch recruit hired");
        self.send_watch_duty(player_id, Some(duty)).await;
        self.tell_the_watch(name, &sergeant).await;
        let departs = clock(departs_min);
        Verdict {
            recruit: Some(
                localized(
                    "server.watchHired",
                    format!(
                        "Hired for Sergeant {sergeant}'s watch, marching at {departs}. Take your gear from the rack by the watch house door, then be beside the sergeant when the watch reports to the Captain."
                    ),
                )
                .with_param("sergeant", sergeant.clone())
                .with_param("departs", departs.clone()),
            ),
            captain: WatchNote::Hired {
                sergeant,
                departs_minute: minute_of_day(departs_min),
            },
            changed: true,
        }
    }

    async fn guard_captain_id(&self) -> Option<PlayerId> {
        let name = npc_defs().npc_name_by_id(GUARD_CAPTAIN)?;
        let id = self.player_id_by_name(name).await?;
        let players = self.players.read().await;
        players
            .get(&id)
            .is_some_and(|npc| is_registry_npc(npc, GUARD_CAPTAIN))
            .then_some(id)
    }

    /// While a watch reports before marching, have the captain present its recruit.
    pub async fn tick_watch_introductions(&self) {
        let Some(roll) = self.watch_roll.get() else {
            return;
        };
        let now = self.game_minutes_now();
        let due: Vec<(String, String)> = {
            let mut roll = roll.lock().expect("watch roll lock poisoned");
            roll.shifts
                .values_mut()
                .filter(|shift| shift.row.calls == 0 && !shift.introduced)
                .filter(|shift| (0.0..REPORT_MIN).contains(&(shift.row.departs_min - now)))
                .map(|shift| {
                    shift.introduced = true;
                    (shift.row.name.clone(), shift.row.leader.clone())
                })
                .collect()
        };
        if due.is_empty() {
            return;
        }
        let Some(captain) = self.guard_captain_id().await else {
            return;
        };
        for (name, sergeant) in due {
            self.send_watch_note(&captain, &name, WatchNote::Reporting { sergeant })
                .await;
        }
    }

    /// The recruit gives up a watch that is not home yet; nothing is paid.
    pub async fn watch_shift_quit(&self, player_id: &PlayerId, auth: &AuthService) {
        let key = roll_key(&self.player_name_of(player_id).await);
        let now = self.game_minutes_now();
        let shift = {
            let mut roll = self.lock_roll(auth).await;
            match roll.shifts.get(&key) {
                Some(shift) if !roll.home(shift, now) => roll.shifts.remove(&key),
                _ => None,
            }
        };
        let Some(shift) = shift else {
            return;
        };
        self.strike_off(player_id, &shift.row, auth).await;
        let name = &shift.row.name;
        info!(
            character_id = shift.row.character_id,
            name, "watch recruit quit"
        );
        let message = localized(
            "server.watchQuit",
            "You give up the watch. The lent gear is handed in and there is no pay.",
        );
        self.send_system_message(player_id, message).await;
        if let Some(captain) = self.guard_captain_id().await {
            self.send_watch_note(&captain, name, WatchNote::Quit).await;
        }
    }

    /// The kinds not drawn yet, the shovel only while snow lies at the rack.
    fn on_rack(&self, shift: &WatchShift, rack: &FurniturePlacement) -> Vec<&'static str> {
        shift
            .gear()
            .into_iter()
            .filter(|(kind, out)| {
                !out && (*kind != SHOVEL_ITEM || self.snow_lies_at(rack.x, rack.z))
            })
            .map(|(kind, _)| kind)
            .collect()
    }

    async fn rack_view(
        &self,
        key: &str,
        rack: &FurniturePlacement,
        now: f64,
        auth: &AuthService,
    ) -> Option<RackView> {
        let roll = self.lock_roll(auth).await;
        let shift = roll.shifts.get(key)?;
        let home = roll.home(shift, now) || shift.lapsed(now);
        let on_rack = if home {
            Vec::new()
        } else {
            self.on_rack(shift, rack)
        };
        Some(RackView {
            on_rack,
            owed: shift.owed(),
            returnable: home || shift.row.calls == 0,
            home,
        })
    }

    /// Show the recruit the rack, or pay one who is home with nothing owed.
    /// False when the player is not on the roll.
    async fn show_rack(
        &self,
        player_id: &PlayerId,
        key: &str,
        rack: &FurniturePlacement,
        now: f64,
        auth: &AuthService,
    ) -> bool {
        let Some(view) = self.rack_view(key, rack, now, auth).await else {
            return false;
        };
        if view.home && view.owed.is_empty() {
            self.settle_at_rack(player_id, key, true, now, auth).await;
            return true;
        }
        let names = |kinds: Vec<&str>| kinds.into_iter().map(str::to_string).collect();
        self.send_direct_message(
            player_id,
            ServerMessage::WatchRackState {
                object_id: rack.id,
                on_rack: names(view.on_rack),
                owed: names(view.owed),
                returnable: view.returnable,
            },
        )
        .await;
        true
    }

    async fn rack_in_reach(
        &self,
        player_id: &PlayerId,
        object_id: u32,
    ) -> Option<(FurniturePlacement, String)> {
        let (_, rack) = self
            .reachable_placement(player_id, object_id, |p| p.type_id == WATCH_GEAR_RACK)
            .await?;
        let key = roll_key(&self.player_name_of(player_id).await);
        Some((rack, key))
    }

    /// Mark a kind lent or back, save the shift and tell the recruit.
    async fn set_lent(
        &self,
        player_id: &PlayerId,
        key: &str,
        kind: &str,
        out: bool,
        auth: &AuthService,
    ) {
        let changed = self.lock_roll(auth).await.shifts.get_mut(key).map(|shift| {
            if let Some(lent) = shift.lent(kind) {
                *lent = out;
            }
            (shift.row.clone(), shift.duty())
        });
        if let Some((row, duty)) = changed {
            self.save_watch_shift(row, auth).await;
            self.send_watch_duty(player_id, Some(duty)).await;
        }
    }

    /// Pay a finished watch whose gear is all back, or is written off as lost.
    async fn settle_at_rack(
        &self,
        player_id: &PlayerId,
        key: &str,
        gear_back: bool,
        now: f64,
        auth: &AuthService,
    ) {
        let Some(shift) = self.lock_roll(auth).await.shifts.remove(key) else {
            return;
        };
        let name = shift.row.name.clone();
        if !gear_back {
            self.remove_items(player_id, &WATCH_GEAR).await;
        }
        let verdict = self
            .pay_for_watch(player_id, &name, shift, gear_back, now, auth)
            .await;
        let captain = self.guard_captain_id().await;
        self.deliver(verdict, player_id, &name, captain).await;
    }

    /// Look at the rack. A recruit home with nothing owed is paid here.
    pub async fn watch_rack_open(&self, player_id: &PlayerId, object_id: u32, auth: &AuthService) {
        let Some((rack, key)) = self.rack_in_reach(player_id, object_id).await else {
            return;
        };
        let now = self.game_minutes_now();
        if !self.show_rack(player_id, &key, &rack, now, auth).await {
            let message = localized(
                "server.watchRackNotRecruit",
                "The gear on this rack is for the day recruits of the watch. Ask the Captain of the Guard for work.",
            );
            self.send_system_message(player_id, message).await;
        }
    }

    /// Take one piece off the rack, or put one back.
    pub async fn watch_rack_move(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        kind: &str,
        take: bool,
        equip: bool,
        auth: &AuthService,
    ) {
        let Some((rack, key)) = self.rack_in_reach(player_id, object_id).await else {
            return;
        };
        if take {
            self.rack_take(player_id, &key, &rack, kind, equip, auth)
                .await;
        } else {
            self.rack_return(player_id, &key, &rack, kind, auth).await;
        }
    }

    async fn rack_take(
        &self,
        player_id: &PlayerId,
        key: &str,
        rack: &FurniturePlacement,
        kind: &str,
        equip: bool,
        auth: &AuthService,
    ) {
        let now = self.game_minutes_now();
        let Some(view) = self.rack_view(key, rack, now, auth).await else {
            return;
        };
        let Some(kind) = view.on_rack.into_iter().find(|there| *there == kind) else {
            return;
        };
        // Claim the piece before the bag changes, so two clicks lend one.
        let claimed = self
            .lock_roll(auth)
            .await
            .shifts
            .get_mut(key)
            .and_then(|shift| shift.lent(kind))
            .is_some_and(|out| !std::mem::replace(out, true));
        if !claimed {
            return;
        }
        let lent = self.lend_item(player_id, kind).await;
        self.set_lent(player_id, key, kind, lent.is_some(), auth)
            .await;
        match lent {
            Some(instance_id) if equip => self.equip_item(player_id, instance_id).await,
            Some(_) => {}
            None => {
                let message = localized(
                    "server.watchTooHeavy",
                    "You carry too much to take that. Lighten your bag first.",
                );
                self.send_system_message(player_id, message).await;
            }
        }
        self.show_rack(player_id, key, rack, now, auth).await;
    }

    /// The last piece back after the watch pays the wage.
    async fn rack_return(
        &self,
        player_id: &PlayerId,
        key: &str,
        rack: &FurniturePlacement,
        kind: &str,
        auth: &AuthService,
    ) {
        let now = self.game_minutes_now();
        let Some(view) = self.rack_view(key, rack, now, auth).await else {
            return;
        };
        if !view.owed.contains(&kind) {
            return;
        }
        let refusal = if !view.returnable {
            Some(localized(
                "server.watchRackLater",
                "Keep your gear while the watch is out. Put it back here when the watch is home.",
            ))
        } else if !self.remove_items(player_id, &[kind]).await {
            Some(localized(
                "server.watchNoTool",
                "You no longer have that piece of lent gear.",
            ))
        } else {
            None
        };
        if let Some(message) = refusal {
            self.send_system_message(player_id, message).await;
            return;
        }
        self.set_lent(player_id, key, kind, false, auth).await;
        self.show_rack(player_id, key, rack, now, auth).await;
    }

    /// Settle a finished watch with gear still owed: it is written off as lost.
    pub async fn watch_rack_settle(
        &self,
        player_id: &PlayerId,
        object_id: u32,
        auth: &AuthService,
    ) {
        let Some((rack, key)) = self.rack_in_reach(player_id, object_id).await else {
            return;
        };
        let now = self.game_minutes_now();
        let view = self.rack_view(&key, &rack, now, auth).await;
        if let Some(view) = view.filter(|view| view.home) {
            self.settle_at_rack(player_id, &key, view.owed.is_empty(), now, auth)
                .await;
        }
    }

    /// Draw everything the rack holds for the recruit, or once the watch is
    /// home put everything back and be paid (agents).
    pub async fn watch_rack_use(&self, player_id: &PlayerId, object_id: u32, auth: &AuthService) {
        let Some((rack, key)) = self.rack_in_reach(player_id, object_id).await else {
            return;
        };
        let now = self.game_minutes_now();
        let Some(view) = self.rack_view(&key, &rack, now, auth).await else {
            return self.watch_rack_open(player_id, object_id, auth).await;
        };
        if !view.home {
            for kind in view.on_rack {
                let equip = kind == TABARD_ITEM;
                self.rack_take(player_id, &key, &rack, kind, equip, auth)
                    .await;
            }
            return;
        }
        for kind in view.owed {
            self.rack_return(player_id, &key, &rack, kind, auth).await;
        }
        // Whatever could not go back is lost.
        self.watch_rack_settle(player_id, object_id, auth).await;
    }

    async fn pay_for_watch(
        &self,
        player_id: &PlayerId,
        name: &str,
        shift: WatchShift,
        gear_back: bool,
        now: f64,
        auth: &AuthService,
    ) -> Verdict {
        let character_id = shift.row.character_id;
        let (answered, calls) = (shift.row.answered, shift.row.calls);
        let earned = watch_wage(answered, calls);
        let wage = if gear_back {
            earned
        } else {
            earned.min(LOST_GEAR_WAGE)
        };
        let paid_min = (wage > 0).then_some(now);
        self.close_watch_shift(character_id, paid_min, auth).await;
        self.send_watch_duty(player_id, None).await;
        info!(
            character_id,
            name, wage, answered, calls, gear_back, "watch wage paid"
        );
        if wage == 0 {
            return Verdict {
                recruit: Some(localized(
                    "server.watchNoShow",
                    "You answered no roll call of your watch. There is no pay; you are off the roll.",
                )),
                captain: WatchNote::NoShow,
                changed: true,
            };
        }
        self.award_copper(player_id, wage).await;
        self.record_gold_source(GoldSource::WatchWage, 1, wage)
            .await;
        let amount = format_copper(wage);
        let (code, text) = if !gear_back {
            (
                "server.watchPaidLostGear",
                format!("Some of the lent gear did not come back. You are paid {amount}."),
            )
        } else if answered == calls {
            (
                "server.watchPaid",
                format!("The gear is back on the rack. You are paid {amount} for a full watch."),
            )
        } else {
            (
                "server.watchPaidPartial",
                format!(
                    "The gear is back on the rack. Attendance {answered}/{calls}: you are paid {amount}."
                ),
            )
        };
        Verdict {
            recruit: Some(
                localized(code, text)
                    .with_param("amount", wage.to_string())
                    .with_param("answered", answered.to_string())
                    .with_param("calls", calls.to_string()),
            ),
            captain: WatchNote::Paid {
                wage,
                answered,
                calls,
                gear_back,
            },
            changed: true,
        }
    }

    /// A recruit on watch strikes down what blocks the road, or shovels snow off it.
    pub async fn watch_tool_use(&self, player_id: &PlayerId, tool: WatchTool, auth: &AuthService) {
        let (name, at) = {
            let players = self.players.read().await;
            match players.get(player_id) {
                Some(p) if p.health > 0 && p.floor_level == 0 => (p.name.clone(), p.position),
                _ => return,
            }
        };
        let now_ms = Self::now_ms();
        let on_duty = {
            let mut roll = self.lock_roll(auth).await;
            let state = &mut *roll;
            let sergeants = &state.sergeants;
            match state.shifts.get_mut(&roll_key(&name)) {
                Some(shift) if shift.row.calls > 0 && !shift.row.done => {
                    if now_ms.saturating_sub(shift.tool_at_ms) < TOOL_COOLDOWN_MS {
                        return;
                    }
                    shift.tool_at_ms = now_ms;
                    let lent = match tool {
                        WatchTool::Spear => shift.row.spear.then_some(SPEAR_ITEM),
                        WatchTool::Shovel => shift.row.shovel.then_some(SHOVEL_ITEM),
                    };
                    let call = sergeants.get(&shift.row.leader);
                    // The sergeant's lap id, so one lap counts one offence per owner.
                    let patrol_id =
                        call.map_or(shift.row.departs_min as i64, |call| call.patrol_id);
                    let near = call.is_some_and(|call| {
                        call.at.dist_xz_sq(&at) <= SPEAR_REACH_M * SPEAR_REACH_M
                    });
                    lent.filter(|_| near || tool == WatchTool::Shovel)
                        .map(|kind| (kind, patrol_id))
                }
                _ => None,
            }
        };
        let Some((lent, patrol_id)) = on_duty else {
            self.send_system_message(
                player_id,
                localized(
                    "server.watchOffDuty",
                    "That is for a recruit out with the watch, beside the sergeant.",
                ),
            )
            .await;
            return;
        };
        if !self.holds_item(player_id, lent).await {
            self.send_system_message(
                player_id,
                localized("server.watchNoTool", "You no longer have the lent tool."),
            )
            .await;
            return;
        }
        let message = match tool {
            WatchTool::Spear => {
                let swung = ServerMessage::PlayerSwung {
                    player_id: *player_id,
                };
                self.publish_nearby(&at, 0, swung, None).await;
                let seized = self
                    .sweep_road_at(at, patrol_id, auth)
                    .await
                    .unwrap_or_else(|reason| {
                        warn!(name, reason, "recruit sweep refused");
                        Vec::new()
                    });
                if seized.is_empty() {
                    localized(
                        "server.watchNothingToClear",
                        "Nothing blocks the road here.",
                    )
                } else {
                    info!(
                        name,
                        owners = seized.len(),
                        "watch recruit cleared the road"
                    );
                    localized(
                        "server.watchCleared",
                        "You pull down what blocked the road.",
                    )
                }
            }
            WatchTool::Shovel => {
                if self.shovel_at(player_id, &at).await {
                    return;
                }
                localized("server.watchNoSnow", "No snow lies on the road here.")
            }
        };
        self.send_system_message(player_id, message).await;
    }

    /// Let the patrol guards know who walks with them.
    async fn tell_the_watch(&self, name: &str, sergeant: &str) {
        let guards: Vec<PlayerId> = {
            let players = self.players.read().await;
            players
                .values()
                .filter(|npc| npc.is_official_npc)
                .filter(|npc| {
                    npc_defs()
                        .get_by_npc_name(&npc.name)
                        .is_some_and(|def| def.class == GUARD_CLASS && def.id != GUARD_CAPTAIN)
                })
                .map(|npc| npc.id)
                .collect()
        };
        let verdict = ServerMessage::WatchVerdict {
            recruit: name.to_string(),
            note: WatchNote::TakenOn {
                sergeant: sergeant.to_string(),
            },
        };
        self.send_direct_message_to_players(&guards, verdict).await;
    }
}
