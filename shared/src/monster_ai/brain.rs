//! [`MonsterBrain`] — the per-monster behavior tree instance: its state, the
//! main tick entry point, and damage/death event handlers. Behavior tree
//! evaluation lives in [`super::behavior`]; movement and state transitions in
//! [`super::movement`].

use std::borrow::Cow;

use super::tree::BehaviorStatus;
use super::{
    cell_of, AiCommand, AiState, BehaviorTree, ChaseAim, NearbyMonster, NearbyPlayer, PathProvider,
    DEFAULT_ATTACK_RANGE, DEFAULT_CHASE_RANGE, DEFAULT_HIT_STAGGER_MS, DEFAULT_MAX_MOVE_DIST,
    DEFAULT_MIN_MOVE_DIST, NETWORK_SYNC_INTERVAL_MS,
};
use crate::pathfinding::PathWaypoint;
use crate::{MonsterState, PlayerId, Position};
use rand::seq::SliceRandom;
use rand::Rng;

/// The skill under way and where it aims.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cast {
    pub(super) skill: &'static super::MonsterSkill,
    pub(super) rotation: f32,
    pub(super) spots: Vec<Position>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MonsterBrain {
    pub monster_id: String,
    pub behavior: String,
    pub position: Position,
    pub rotation: f32,
    pub health: u32,
    pub max_health: u32,
    pub(super) state: AiState,
    pub(super) state_timer_ms: f32,
    pub(super) target_player_id: Option<PlayerId>,
    pub(super) walk_speed: f32,
    pub(super) run_speed: f32,
    pub(super) attack_range: f32,
    pub(super) chase_range: f32,
    pub(super) attack_cooldown_ms: f32,
    pub(super) move_speed: f32,
    pub(super) target_position: Option<Position>,
    pub(super) waypoints: Vec<PathWaypoint>,
    pub(super) current_waypoint_idx: usize,
    pub(super) path_elapsed_ms: f32,
    pub(super) return_retry_left_ms: f32,
    pub(super) chase_retry_left_ms: f32,
    pub(super) chase_failures: u8,
    pub(super) last_known_target_pos: Option<Position>,
    pub(super) spawn_position: Position,
    /// Passability floor for path queries. 0 = overworld/house ground;
    /// dungeon monsters use their depth's passability floor index.
    pub path_floor: u8,
    /// Time accumulated toward the next throttled network position sync while
    /// continuously moving. See [`Self::should_sync_move`].
    pub(super) sync_elapsed_ms: f32,
    /// Movement state at the last emitted sync, so entering a new one syncs at
    /// once instead of waiting out the interval.
    pub(super) last_synced_state: AiState,
    /// A path bend the next sync must not be allowed to cut across.
    pub(super) pending_bend_sync: bool,
    /// Time left before the next swing, kept apart from `state_timer_ms` so
    /// that leaving and re-entering Attack cannot re-arm the cooldown. Zero is
    /// ready: the first swing on contact does not wait one out.
    pub(super) attack_cooldown_left_ms: f32,
    /// How long this type's swing animation runs. The attack holds the state
    /// this long so a swing it started finishes; 0 releases as soon as the
    /// target steps out.
    pub(super) swing_commit_ms: f32,
    /// Time left in the swing currently being delivered.
    pub(super) swing_left_ms: f32,
    pub(super) skills: Option<&'static super::skill::SkillSet>,
    /// Per entry of `skills`.
    pub(super) skill_cooldowns_ms: Vec<f32>,
    /// Last-stand casts left, once it begins.
    pub(super) last_stand: Option<u8>,
    /// The cast whose strike is still to land.
    pub(super) cast: Option<Cast>,
    /// Time left in a cast; the brain holds its pose and facing until it ends.
    pub(super) cast_left_ms: f32,
    /// The free cell near the target the chase is heading to; kept while it
    /// stays free so re-slotting doesn't oscillate.
    pub(super) chase_goal_cell: Option<(i32, i32)>,
    /// Cells occupied by standing nearby monsters, rebuilt each tick.
    pub(super) occupied_cells: Vec<(i32, i32)>,
    /// A standing monster with a smaller id shares our cell this tick — we
    /// are the one who moves aside. Rebuilt each tick.
    pub(super) cell_yield: bool,
    /// Walking off a shared cell to a slot of our own after a yield. Attack
    /// entry stays refused until arrival, or the yielder would stop at the
    /// first cell boundary, still overlapped.
    pub(super) reslotting: bool,
    /// Detour goal; repaths keep avoiding standers until arrival.
    pub(super) detour_goal: Option<(f32, f32)>,
    /// A solid body fixes the facing and moves every reach to its surface.
    pub body: Option<crate::monster_body::BodyFootprint>,
    /// Plain swings past melee reach land this far out; 0 for none.
    pub ranged_range: f32,
    /// How long a player must stay in sight before it gives chase; 0 reacts
    /// at once. Being hit always turns it at once.
    pub reaction_ms: f32,
    /// The player it has spotted and the time left before it reacts.
    pub(super) noticing: Option<(PlayerId, f32)>,
    /// Off for raid bosses: a crowd's blows would never let one act.
    pub staggers: bool,
    /// Time left in the Hit stagger; a later blow never shortens it.
    pub(super) stagger_left_ms: f32,
    /// Steps run at `slow_mult` while this lasts.
    pub(super) slow_left_ms: f32,
    pub(super) slow_mult: f32,
    pub(super) aggro: Option<Box<super::aggro::Aggro>>,
    /// Time left standing by players who are all stone (`SkillSet::shatter`).
    pub(super) stone_wait_left_ms: Option<f32>,
}

impl MonsterBrain {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        monster_id: String,
        monster_type: &str,
        behavior: String,
        position: Position,
        health: u32,
        max_health: u32,
        walk_speed: f32,
        run_speed: f32,
        attack_range: f32,
        chase_range: f32,
        attack_cooldown_ms: f32,
    ) -> Self {
        let swing_commit_ms = super::attack_clip_ms(monster_type);
        let skills = super::skill::skills_for(monster_type);
        Self {
            monster_id,
            behavior,
            rotation: 0.0,
            health,
            max_health,
            state: AiState::Idle,
            state_timer_ms: 0.0,
            target_player_id: None,
            walk_speed,
            run_speed,
            attack_range: if attack_range > 0.0 {
                attack_range
            } else {
                DEFAULT_ATTACK_RANGE
            },
            chase_range: if chase_range > 0.0 {
                chase_range
            } else {
                DEFAULT_CHASE_RANGE
            },
            attack_cooldown_ms,
            move_speed: walk_speed,
            target_position: None,
            waypoints: Vec::new(),
            current_waypoint_idx: 0,
            path_elapsed_ms: 0.0,
            return_retry_left_ms: 0.0,
            chase_retry_left_ms: 0.0,
            chase_failures: 0,
            last_known_target_pos: None,
            spawn_position: position,
            position,
            path_floor: 0,
            sync_elapsed_ms: 0.0,
            last_synced_state: AiState::Idle,
            pending_bend_sync: false,
            attack_cooldown_left_ms: 0.0,
            swing_commit_ms,
            swing_left_ms: 0.0,
            skills,
            skill_cooldowns_ms: skills.map_or(Vec::new(), |set| {
                set.skills.iter().map(|(s, _)| s.first_cast_ms).collect()
            }),
            last_stand: None,
            cast: None,
            cast_left_ms: 0.0,
            chase_goal_cell: None,
            occupied_cells: Vec::new(),
            cell_yield: false,
            reslotting: false,
            detour_goal: None,
            body: None,
            ranged_range: 0.0,
            reaction_ms: 0.0,
            noticing: None,
            staggers: true,
            stagger_left_ms: 0.0,
            slow_left_ms: 0.0,
            slow_mult: 1.0,
            aggro: super::aggro_for(monster_type)
                .map(|rules| Box::new(super::aggro::Aggro::new(rules))),
            stone_wait_left_ms: None,
        }
    }

    /// The path just changed direction here — force the next tick to sync.
    ///
    /// The server only sees the straight line between two reported positions,
    /// and a bend swallowed inside one sync interval makes that line a chord off
    /// the walkable path, which its collision sweep rightly refuses. Reporting
    /// bends keeps every segment inside one smoothed leg, which
    /// `pathfinding::is_line_passable` cleared with `y: None` and a
    /// `PLAYER_RADIUS` sweep — stricter than the server's. Keep that inequality:
    /// it is what makes an accepted path un-refusable.
    pub(super) fn mark_path_bend(&mut self) {
        self.pending_bend_sync = true;
    }

    /// A sync the step must not precede: a new movement state or a path bend,
    /// where a post-step pose would put the server's chord across the corner.
    pub(super) fn sync_due_before_step(&self) -> bool {
        self.pending_bend_sync || self.state != self.last_synced_state
    }

    /// Gate for the per-tick position emits of continuously-moving states
    /// (chase/return/flee): a state change, a bend, or `NETWORK_SYNC_INTERVAL_MS`
    /// elapsed. Remote clients interpolate toward the command's
    /// `target_position` between syncs — see [`Self::current_leg_target`].
    pub(super) fn should_sync_move(&self) -> bool {
        self.sync_due_before_step() || self.sync_elapsed_ms >= NETWORK_SYNC_INTERVAL_MS
    }

    /// Snap to the position the server settled on and drop the current path, so
    /// the next tick re-plans from there. Otherwise the brain keeps walking from
    /// a position the server refused, and every later move is swept from the one
    /// it kept — the same refusal forever.
    pub fn apply_authoritative_position(&mut self, position: Position) {
        if self.state == AiState::Dead {
            return;
        }
        self.position = position;
        self.waypoints.clear();
        self.current_waypoint_idx = 0;
        self.pending_bend_sync = false;
        self.retry_chase();
    }

    pub fn retry_chase(&mut self) {
        self.clear_chase_backoff();
        self.last_known_target_pos = None;
    }

    pub(super) fn clear_chase_backoff(&mut self) {
        self.chase_failures = 0;
        self.chase_retry_left_ms = 0.0;
    }

    pub fn pause_chase(&mut self) -> Vec<AiCommand> {
        self.target_player_id = None;
        self.waypoints.clear();
        self.current_waypoint_idx = 0;
        self.chase_goal_cell = None;
        self.detour_goal = None;
        self.target_position = None;
        let mut commands = Vec::new();
        if !matches!(self.state, AiState::Idle | AiState::Hit | AiState::Dead) {
            self.enter_hold(&mut commands);
        }
        commands
    }

    pub fn resume_chase(&mut self, target: PlayerId) {
        self.target_player_id = Some(target);
        self.retry_chase();
    }

    pub fn state(&self) -> AiState {
        self.state
    }

    pub fn network_state(&self) -> MonsterState {
        self.state.to_monster_state()
    }

    pub fn is_dead(&self) -> bool {
        self.state == AiState::Dead
    }

    // =========================================================================
    // Main tick
    // =========================================================================

    pub(super) fn advance_timers(&mut self, delta_ms: f32) {
        self.state_timer_ms += delta_ms;
        self.path_elapsed_ms += delta_ms;
        self.return_retry_left_ms = (self.return_retry_left_ms - delta_ms).max(0.0);
        self.chase_retry_left_ms = (self.chase_retry_left_ms - delta_ms).max(0.0);
        self.sync_elapsed_ms += delta_ms;
        self.attack_cooldown_left_ms = (self.attack_cooldown_left_ms - delta_ms).max(0.0);
        self.swing_left_ms = (self.swing_left_ms - delta_ms).max(0.0);
        for cooldown in &mut self.skill_cooldowns_ms {
            *cooldown = (*cooldown - delta_ms).max(0.0);
        }
        self.cast_left_ms = (self.cast_left_ms - delta_ms).max(0.0);
        self.stagger_left_ms = (self.stagger_left_ms - delta_ms).max(0.0);
        self.slow_left_ms = (self.slow_left_ms - delta_ms).max(0.0);
        if let Some((_, left)) = &mut self.noticing {
            *left = (*left - delta_ms).max(0.0);
        }
        if let Some(aggro) = &mut self.aggro {
            aggro.advance(delta_ms);
        }
        if let Some(left) = &mut self.stone_wait_left_ms {
            *left = (*left - delta_ms).max(0.0);
        }
    }

    /// Players this monster may go for. Stone is out of reach, unless every
    /// living player around is stone and the monster shatters it: then it
    /// stands by them, holding its swing until the wait runs out.
    fn targetable<'a>(&mut self, players: &'a [NearbyPlayer]) -> Cow<'a, [NearbyPlayer]> {
        if !players.iter().any(|p| p.petrified) {
            self.stone_wait_left_ms = None;
            return Cow::Borrowed(players);
        }
        let all_stone = !players.iter().any(|p| !p.petrified && p.health > 0);
        if let Some(shatter) = self
            .skills
            .and_then(|set| set.shatter)
            .filter(|_| all_stone)
        {
            let wait = *self.stone_wait_left_ms.get_or_insert(shatter.wait_ms);
            self.attack_cooldown_left_ms = self.attack_cooldown_left_ms.max(wait);
            return Cow::Borrowed(players);
        }
        self.stone_wait_left_ms = None;
        let awake: Vec<_> = players.iter().filter(|p| !p.petrified).cloned().collect();
        if self
            .target_player_id
            .is_some_and(|id| !awake.iter().any(|p| p.id == id))
            && players.iter().any(|p| Some(p.id) == self.target_player_id)
        {
            self.target_player_id = None;
            self.retry_chase();
        }
        Cow::Owned(awake)
    }

    pub fn is_casting(&self) -> bool {
        self.cast_left_ms > 0.0
    }

    fn last_stand_rule(&self) -> Option<super::skill::LastStand> {
        let rule = self.skills?.last_stand?;
        (self.health as f32 <= self.max_health as f32 * rule.health_ratio).then_some(rule)
    }

    /// The last stand opens at once, without waiting out the swing cooldown.
    pub(super) fn last_stand_due(&self) -> bool {
        self.last_stand.is_none() && self.last_stand_rule().is_some()
    }

    /// Start a ready skill in place of a plain swing at `target`.
    pub(super) fn try_cast(
        &mut self,
        target: &Position,
        nearby_players: &[NearbyPlayer],
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
        commands: &mut Vec<AiCommand>,
    ) -> bool {
        let Some(set) = self.skills.filter(|_| self.stone_wait_left_ms.is_none()) else {
            return false;
        };
        let mut swing_wait_ms = self.attack_cooldown_ms;
        let skill = if let Some(rule) = self.last_stand_rule() {
            let left = self.last_stand.get_or_insert(rule.casts);
            if *left == 0 {
                return false;
            }
            *left -= 1;
            if *left > 0 {
                swing_wait_ms = rule.every_ms - rule.skill.cast_ms;
            }
            rule.skill
        } else {
            let ready: Vec<usize> = (0..set.skills.len())
                .filter(|&i| self.skill_cooldowns_ms[i] <= 0.0)
                .collect();
            let Ok(&i) = ready.choose_weighted(rng, |&i| set.skills[i].1) else {
                return false;
            };
            let skill = set.skills[i].0;
            self.skill_cooldowns_ms[i] = skill.cooldown_ms;
            // No other skill until this one ends and the gap passes.
            for cooldown in &mut self.skill_cooldowns_ms {
                *cooldown = cooldown.max(skill.cast_ms + set.gap_ms);
            }
            skill
        };
        self.cast_left_ms = skill.cast_ms;
        self.attack_cooldown_left_ms = skill.cast_ms + swing_wait_ms;
        let rotation = self.aim_toward(self.position.bearing_xz_to(target));
        let floor = self.path_floor;
        let spots = skill.lay_spots(
            self.body,
            self.position,
            self.rotation,
            rotation,
            nearby_players,
            |x, z| path_provider.cell_passable(x, z, floor),
            rng,
        );
        commands.push(self.make_move_cmd());
        commands.push(AiCommand::CastSkill {
            skill: skill.id,
            rotation,
            spots: spots.clone(),
        });
        self.cast = Some(Cast {
            skill,
            rotation,
            spots,
        });
        true
    }

    fn tick_cast(&mut self, commands: &mut Vec<AiCommand>) {
        let left = self.cast_left_ms;
        let Some(cast) = self
            .cast
            .take_if(|c| c.skill.cast_ms - left >= c.skill.strike_ms)
        else {
            return;
        };
        commands.push(AiCommand::SkillStrike {
            skill: cast.skill.id,
            position: self.position,
            rotation: cast.rotation,
            spots: cast.spots,
        });
    }

    /// A skill's facing toward `bearing`: a body that cannot turn still aims
    /// this far either side of its own.
    fn aim_toward(&self, bearing: Option<f32>) -> f32 {
        const AIM_LIMIT: f32 = 60.0 * std::f32::consts::PI / 180.0;
        match (self.body, bearing) {
            (Some(_), Some(bearing)) => {
                let off = crate::mount_movement::angle_delta(self.rotation, bearing);
                self.rotation + off.clamp(-AIM_LIMIT, AIM_LIMIT)
            }
            _ => self.rotation,
        }
    }

    /// Where this monster's reach to `target` is measured from.
    pub fn strike_origin(&self, target: &Position) -> Position {
        crate::monster_body::reach_point(self.body, self.position, self.rotation, target)
    }

    /// Turn to `bearing`; a solid body keeps the facing it spawned with.
    pub(super) fn set_facing(&mut self, bearing: Option<f32>) {
        if let (None, Some(bearing)) = (self.body, bearing) {
            self.rotation = bearing;
        }
    }

    pub fn tick_with_behavior_tree(
        &mut self,
        delta_ms: f32,
        nearby_players: &[NearbyPlayer],
        nearby_monsters: &[NearbyMonster],
        behavior_tree: &BehaviorTree,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) -> Vec<AiCommand> {
        if self.state == AiState::Dead || self.health == 0 {
            return vec![];
        }
        self.occupied_cells.clear();
        self.cell_yield = false;
        let my_cell = cell_of(self.position.x, self.position.z);
        for m in nearby_monsters {
            if !m.state.is_stationary()
                || m.path_floor != self.path_floor
                || m.id == self.monster_id
            {
                continue;
            }
            let cell = cell_of(m.position.x, m.position.z);
            self.occupied_cells.push(cell);
            // Sharing a cell with a smaller-id stander: we are the one who
            // yields (see bt_attack_target / the door-wait spread).
            if cell == my_cell && m.id.as_str() < self.monster_id.as_str() {
                self.cell_yield = true;
            }
        }
        if !self.state.is_engaged() {
            self.reslotting = false;
            self.detour_goal = None;
        }

        let casting = self.is_casting();
        self.advance_timers(delta_ms);
        let mut commands = Vec::new();

        if casting {
            self.tick_cast(&mut commands);
            if self.is_casting() {
                return commands;
            }
        }

        if self.state == AiState::Hit {
            if self.stagger_left_ms > 0.0 {
                return commands;
            }
            self.state = AiState::Idle;
            self.state_timer_ms = 0.0;
        }

        let targets = self.targetable(nearby_players);
        let nearby_players = &*targets;
        self.update_aggro_target(nearby_players, path_provider, rng);

        let was_noticing = self.noticing.is_some();
        if matches!(self.state, AiState::Walk | AiState::Run) {
            self.tick_patrol(delta_ms, &mut commands, path_provider, rng);
        } else {
            let status = self.eval_behavior_node(
                &behavior_tree.root,
                delta_ms,
                nearby_players,
                &mut commands,
                path_provider,
                rng,
            );

            if status == BehaviorStatus::Failure && self.state != AiState::Idle {
                self.transition_to_idle(&mut commands);
            }
            // Show the turn toward a player it has just spotted.
            if !was_noticing && self.noticing.is_some() && commands.is_empty() {
                commands.push(self.make_move_cmd());
            }
        }

        commands
    }

    // =========================================================================
    // Event handlers
    // =========================================================================

    /// Apply incoming damage and acquire `attacker_id` as the target (an aggro
    /// brain only tallies it and keeps its pick). Returns
    /// `false` if the brain is already dead or the hit was lethal (in which
    /// case no further reaction should be produced).
    fn apply_hit(&mut self, attacker_id: &PlayerId, hit: bool, damage: u32) -> bool {
        if self.state == AiState::Dead {
            return false;
        }

        let damage = if hit { damage } else { 0 };
        self.health = self.health.saturating_sub(damage);
        match &mut self.aggro {
            Some(aggro) => {
                aggro.record(*attacker_id, damage);
                self.target_player_id.get_or_insert(*attacker_id);
            }
            None => self.target_player_id = Some(*attacker_id),
        }
        self.move_speed = self.run_speed;

        if self.health == 0 {
            self.state = AiState::Dead;
            return false;
        }

        true
    }

    pub fn handle_hit_with_behavior_tree(
        &mut self,
        attacker_id: &PlayerId,
        hit: bool,
        damage: u32,
    ) -> Vec<AiCommand> {
        self.react(
            attacker_id,
            hit,
            damage,
            hit.then_some(DEFAULT_HIT_STAGGER_MS),
        )
    }

    /// Primal Roar: provoke like a missed swing and stagger for `stagger_ms`.
    /// The swing cooldown waits out the added stagger.
    pub fn handle_roar(&mut self, attacker_id: &PlayerId, stagger_ms: f32) -> Vec<AiCommand> {
        let before = self.stagger_left_ms_now();
        let commands = self.react(attacker_id, false, 0, Some(stagger_ms));
        self.attack_cooldown_left_ms += self.stagger_left_ms_now() - before;
        commands
    }

    fn react(
        &mut self,
        attacker_id: &PlayerId,
        hit: bool,
        damage: u32,
        stagger_ms: Option<f32>,
    ) -> Vec<AiCommand> {
        let previous_target = self.target_player_id;
        if !self.apply_hit(attacker_id, hit, damage) {
            return vec![];
        }

        if previous_target != self.target_player_id {
            self.retry_chase();
        }

        if self.is_casting() {
            return vec![];
        }

        if let Some(ms) = stagger_ms.filter(|_| self.staggers) {
            self.stagger_left_ms = self.stagger_left_ms_now().max(ms);
            self.state = AiState::Hit;
            self.state_timer_ms = 0.0;
            vec![self.make_move_cmd()]
        } else if self.state.is_engaged() || self.state == AiState::Hit {
            // Already engaged: idling here stopped a charging monster and
            // slid it back to the last tick's pose on every missed swing.
            // A miss never cuts a stagger short either.
            vec![]
        } else {
            // A miss (and the server's out-of-range provoke event) still
            // acquires the attacker. Cancel any in-progress wander so the
            // next AI tick evaluates the combat branches immediately instead
            // of finishing the old patrol path first.
            let mut commands = Vec::new();
            self.transition_to_idle(&mut commands);
            commands
        }
    }

    /// Valhalla Charge. A new slow replaces the old one.
    pub fn handle_slow(&mut self, mult: f32, ms: f32) {
        if self.state == AiState::Dead {
            return;
        }
        self.slow_mult = mult;
        self.slow_left_ms = ms;
    }

    pub(super) fn step_speed(&self) -> f32 {
        if self.slow_left_ms > 0.0 {
            self.move_speed * self.slow_mult
        } else {
            self.move_speed
        }
    }

    fn stagger_left_ms_now(&self) -> f32 {
        if self.state == AiState::Hit {
            self.stagger_left_ms
        } else {
            0.0
        }
    }

    pub fn target_player_id(&self) -> Option<PlayerId> {
        self.target_player_id
    }

    /// Engaged with a player, including a chase paused at a door.
    pub fn in_combat(&self) -> bool {
        self.target_player_id.is_some() || self.state.is_engaged()
    }

    pub fn handle_death(&mut self) {
        self.state = AiState::Dead;
        self.health = 0;
    }

    // =========================================================================

    fn tick_patrol(
        &mut self,
        delta_ms: f32,
        commands: &mut Vec<AiCommand>,
        path_provider: &dyn PathProvider,
        rng: &mut impl Rng,
    ) {
        if self.target_position.is_none() {
            self.transition_to_idle(commands);
            return;
        }

        let reached = self.follow_path(delta_ms);
        if reached {
            if rng.gen::<f32>() < 0.5 {
                self.transition_to_idle(commands);
            } else {
                self.transition_to_move(
                    commands,
                    DEFAULT_MIN_MOVE_DIST,
                    DEFAULT_MAX_MOVE_DIST,
                    path_provider,
                    rng,
                );
            }
        } else if self.pending_bend_sync {
            // A wander leg is otherwise reported only at its ends, so the
            // server would see one chord across every corner of it. Bends alone
            // are enough — no interval sync, since a straight run between two
            // bends is a sub-segment of a leg the path check already cleared.
            commands.push(self.make_move_cmd());
        }
    }

    /// The one `Move` literal, and the one place a sync is recorded: a later
    /// state change syncs at once even when it returns to the state synced
    /// before this one, and the bend this pose reports is consumed.
    pub(super) fn move_cmd_to(&mut self, target_position: Position) -> AiCommand {
        self.sync_elapsed_ms = 0.0;
        self.last_synced_state = self.state;
        self.pending_bend_sync = false;
        // Derived here, not threaded in: every chase leg carries its live aim.
        // Standing-cell legs end near the ring too, so the sync correction
        // absorbs the cell-vs-ring difference.
        let chasing = if self.state == AiState::Chase {
            self.target_player_id.map(|player_id| ChaseAim {
                player_id,
                stop_range: self.engage_stop_range(),
            })
        } else {
            None
        };
        AiCommand::Move {
            position: self.position,
            rotation: self.rotation,
            state: self.state.to_monster_state(),
            target_position,
            chasing,
        }
    }

    pub(super) fn make_move_cmd(&mut self) -> AiCommand {
        let aim = self.current_leg_target();
        self.move_cmd_to(aim)
    }
}
