//! Looping emotes kept on one beat for every viewer, and `JoinDance`, which
//! stands a player beside a dancer on that beat.

use super::localization::localized;
use super::passability::too_steep;
use super::player_interaction::clear_segment;
use super::GameState;
use crate::rate_limit::RateLimiter;
use crate::types::{Player, PlayerId, Position, ServerMessage};
use onlinerpg_shared::messages::{
    JOINABLE_DANCES, JOIN_DANCE_MAX_RISE_M, JOIN_DANCE_RANGE_M, LOOPING_EMOTES, MAX_DANCE_GROUP,
};
use onlinerpg_shared::pathfinding::PassabilityCache;
use onlinerpg_shared::{shortest_world_delta_x, wrap_world_x};
use std::collections::hash_map::{Entry, HashMap};
use std::time::Instant;

/// Side-by-side gap between dancers, in metres.
const DANCE_SPACING_M: f32 = 1.1;
/// The second row stands this far behind the dancer joined.
const DANCE_ROW_GAP_M: f32 = 1.4;
/// A spot this close to someone already standing there is taken.
const DANCE_SPOT_CLEARANCE_M: f32 = 0.7;
/// Largest ground difference between the dancer and a spot.
const DANCE_SPOT_MAX_RISE_M: f32 = 0.75;
/// Anyone standing within this of the dancer may block a spot.
const DANCE_SPOT_REACH_M: f32 = 4.0 * DANCE_SPACING_M + DANCE_SPOT_CLEARANCE_M;
/// Ground is checked for steepness this often along the joiner's way.
const DANCE_SLOPE_STEP_M: f32 = 0.5;
/// Join requests a player may make per second, and in one burst.
const DANCE_JOINS_PER_SEC: f32 = 1.0;
const DANCE_JOIN_BURST: f32 = 2.0;

/// Why a join was refused: the localization code and its English text.
type Refusal = (&'static str, &'static str);
const REFUSE_GONE: Refusal = ("server.danceGone", "There's no dance nearby to join.");
const REFUSE_FULL: Refusal = ("server.danceFull", "That dance is full.");
const REFUSE_NO_ROOM: Refusal = (
    "server.danceNoRoom",
    "There's no room to dance beside them.",
);
const REFUSE_ALREADY: Refusal = ("server.danceAlready", "You're already dancing with them.");
const REFUSE_COMBAT: Refusal = ("server.danceInCombat", "You can't join a dance in combat.");
const REFUSE_MOUNTED: Refusal = ("server.danceMounted", "Dismount to join the dance.");
const REFUSE_NOT_NOW: Refusal = ("server.danceNotNow", "You can't join a dance right now.");
const REFUSE_DUNGEON: Refusal = (
    "server.danceDungeon",
    "You can't join a dance in a dungeon.",
);
const REFUSE_MOVED: Refusal = ("server.danceStandStill", "Stand still to join the dance.");
const JOIN_DANCE_USAGE: Refusal = ("server.joinDanceUsage", "Usage: /join_dance <player>");

pub(super) fn is_looping_emote(kind: &str) -> bool {
    LOOPING_EMOTES.contains(&kind)
}

/// Each dancer's beat, and how many dancers share each one.
#[derive(Default)]
pub(super) struct DanceBeats {
    anchors: HashMap<PlayerId, Instant>,
    groups: HashMap<Instant, usize>,
}

impl DanceBeats {
    pub(super) fn get(&self, player_id: &PlayerId) -> Option<Instant> {
        self.anchors.get(player_id).copied()
    }

    pub(super) fn contains(&self, player_id: &PlayerId) -> bool {
        self.anchors.contains_key(player_id)
    }

    pub(super) fn group_size(&self, anchor: Instant) -> usize {
        self.groups.get(&anchor).copied().unwrap_or(0)
    }

    /// Returns the player's previous beat.
    pub(super) fn insert(&mut self, player_id: PlayerId, anchor: Instant) -> Option<Instant> {
        let before = self.anchors.insert(player_id, anchor);
        if let Some(before) = before {
            self.leave(before);
        }
        *self.groups.entry(anchor).or_default() += 1;
        before
    }

    pub(super) fn remove(&mut self, player_id: &PlayerId) -> Option<Instant> {
        let before = self.anchors.remove(player_id)?;
        self.leave(before);
        Some(before)
    }

    fn leave(&mut self, anchor: Instant) {
        if let Entry::Occupied(mut group) = self.groups.entry(anchor) {
            *group.get_mut() -= 1;
            if *group.get() == 0 {
                group.remove();
            }
        }
    }
}

/// Where a `JoinDance` moves the player. Holds only while the joiner and the
/// dancer are where and as the spot was checked against.
pub(super) struct Stand {
    pub(super) spot: Position,
    pub(super) rotation: f32,
    joiner_at: Position,
    floor_level: i8,
    target: PlayerId,
    target_at: Position,
    dance: String,
}

/// Spots beside a dancer at `origin` facing `rotation` (radians), in the
/// order they are offered: shoulder to shoulder outwards, then a row behind,
/// each pair's side nearer `joiner` first. Forward is `(sin, cos)`.
fn dance_spots(origin: Position, rotation: f32, joiner: Position) -> Vec<Position> {
    let (right_x, right_z) = (rotation.cos(), -rotation.sin());
    let (back_x, back_z) = (-rotation.sin(), -rotation.cos());
    let at = |across: f32, back: f32| {
        let side = across * DANCE_SPACING_M;
        Position {
            x: wrap_world_x(origin.x + right_x * side + back_x * back),
            y: origin.y,
            z: origin.z + right_z * side + back_z * back,
        }
    };
    let beside = (1..=4).map(|step| (step as f32, 0.0));
    let behind = [0.0, 1.0, 2.0].map(|across| (across, DANCE_ROW_GAP_M));
    let mut spots = Vec::with_capacity(13);
    for (across, back) in beside.chain(behind) {
        let mut pair = [at(across, back), at(-across, back)];
        if pair[1].dist_xz_sq(&joiner) < pair[0].dist_xz_sq(&joiner) {
            pair.swap(0, 1);
        }
        spots.extend_from_slice(&pair[..if across == 0.0 { 1 } else { 2 }]);
    }
    spots
}

impl GameState {
    /// Start `player_id`'s beat at `anchor` and send viewers the position in
    /// the loop. Called after the looping interaction is set.
    pub(super) async fn start_dance_beat(&self, player_id: &PlayerId, anchor: Instant) {
        self.dance_beats.write().await.insert(*player_id, anchor);
        self.publish_subject_change(dance_sync(player_id, anchor));
    }

    pub(super) async fn end_dance_beat(&self, player_id: &PlayerId) {
        if self.dance_beats.read().await.contains(player_id) {
            self.dance_beats.write().await.remove(player_id);
        }
    }

    /// Tell the player their own beat again, after a refused pose change
    /// handed them back the dance.
    pub(super) async fn resend_dance_beat(&self, player_id: &PlayerId) {
        let anchor = self.dance_beats.read().await.get(player_id);
        if let Some(anchor) = anchor {
            self.send_direct_message(player_id, dance_sync(player_id, anchor))
                .await;
        }
    }

    /// `/join_dance <name>`.
    pub(super) async fn join_dance_named(&self, player_id: &PlayerId, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            return self.refuse_join_dance(player_id, JOIN_DANCE_USAGE).await;
        }
        match self.player_id_by_name(name).await {
            Some(target_id) => self.join_dance(player_id, &target_id).await,
            None => self.refuse_join_dance(player_id, REFUSE_GONE).await,
        }
    }

    /// Stand `player_id` in a free spot beside `target_id`, dancing their
    /// dance on their beat. Sharing the anchor is what groups the dancers.
    /// Requests over the player's rate limit are dropped.
    pub async fn join_dance(&self, player_id: &PlayerId, target_id: &PlayerId) {
        let allowed = self
            .dance_join_limits
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(*player_id)
            .or_insert_with(|| RateLimiter::new(DANCE_JOINS_PER_SEC, DANCE_JOIN_BURST))
            .allow();
        if !allowed {
            return;
        }
        if let Err(refusal) = self.try_join_dance(player_id, target_id).await {
            self.refuse_join_dance(player_id, refusal).await;
        }
    }

    /// Whether `joiner`, now under their movement lock, may still be moved
    /// to `stand`: both are where they were, and the join still passes.
    pub(super) async fn stand_holds(&self, stand: &Stand, joiner: &Player) -> bool {
        joiner.floor_level == stand.floor_level
            && joiner.position.dist_xz_sq(&stand.joiner_at) < 0.01
            && self
                .players
                .read()
                .await
                .get(&stand.target)
                .is_some_and(|target| {
                    target.position.dist_xz_sq(&stand.target_at) < 0.01
                        && joinable_dance(joiner, target).is_ok_and(|dance| dance == stand.dance)
                })
    }

    async fn try_join_dance(
        &self,
        player_id: &PlayerId,
        target_id: &PlayerId,
    ) -> Result<(), Refusal> {
        let (joiner, target) = {
            let players = self.players.read().await;
            match (players.get(player_id), players.get(target_id)) {
                (Some(joiner), Some(target)) => (joiner.clone(), target.clone()),
                _ => return Err(REFUSE_GONE),
            }
        };
        let dance = joinable_dance(&joiner, &target)?;
        group_anchor(&*self.dance_beats.read().await, player_id, target_id)?;
        let nearby_ids = self
            .players_within_position(
                &target.position,
                target.floor_level,
                DANCE_SPOT_REACH_M,
                Some(player_id),
            )
            .await;
        let nearby: Vec<Position> = {
            let players = self.players.read().await;
            nearby_ids
                .iter()
                .filter_map(|(id, _)| players.get(id).map(|p| p.position))
                .collect()
        };
        let from = self.interaction_exit(&joiner).await;
        let spot = self
            .free_dance_spot(&target, from, &nearby)
            .await
            .ok_or(REFUSE_NO_ROOM)?;
        let (anchor, before) = {
            let mut beats = self.dance_beats.write().await;
            let anchor = group_anchor(&beats, player_id, target_id)?;
            (anchor, beats.insert(*player_id, anchor))
        };
        let stand = Stand {
            spot,
            rotation: target.rotation,
            joiner_at: joiner.position,
            floor_level: joiner.floor_level,
            target: target.id,
            target_at: target.position,
            dance: dance.clone(),
        };
        if self
            .apply_player_interaction(player_id, Some(dance), None, Some(&stand))
            .await
        {
            // Death or another pose may have taken the claim back meanwhile.
            if self.dance_beats.read().await.get(player_id) == Some(anchor) {
                self.publish_subject_change(dance_sync(player_id, anchor));
            }
            return Ok(());
        }
        hand_back(
            &mut *self.dance_beats.write().await,
            player_id,
            anchor,
            before,
        );
        let now = self.players.read().await.get(player_id).cloned();
        Err(failed_join_refusal(&joiner, now.as_ref()))
    }

    /// The first of [`dance_spots`] clear of anyone in `taken`, level with
    /// the dancer, and reachable without crossing a wall from both the dancer
    /// and the joiner at `from`, nor a slope the joiner could not walk up.
    async fn free_dance_spot(
        &self,
        target: &Player,
        from: Position,
        taken: &[Position],
    ) -> Option<Position> {
        let floor = onlinerpg_shared::dungeon::passability_floor_for_level(target.floor_level);
        let cache = self.passability.snapshot();
        let clearance = DANCE_SPOT_CLEARANCE_M * DANCE_SPOT_CLEARANCE_M;
        let base_y = self
            .surface_ground_y(floor, &target.position, target.position.y, None)
            .await;
        let start = Position {
            y: self.surface_ground_y(floor, &from, from.y, None).await,
            ..from
        };
        for mut spot in dance_spots(target.position, target.rotation, from) {
            if taken.iter().any(|p| p.dist_xz_sq(&spot) < clearance) {
                continue;
            }
            spot.y = self
                .surface_ground_y(floor, &spot, target.position.y, None)
                .await;
            if (spot.y - base_y).abs() > DANCE_SPOT_MAX_RISE_M
                || (spot.y - start.y).abs() > JOIN_DANCE_MAX_RISE_M
                || !self.can_step_to(&cache, floor, target.position, spot)
                || !clear_segment(&cache, from, spot, floor)
                || !self.walkable_slope(&cache, floor, start, spot).await
            {
                continue;
            }
            return Some(spot);
        }
        None
    }

    /// Whether the ground from `from` to `to` passes the movement slope rule
    /// at every step, as walking it would.
    async fn walkable_slope(
        &self,
        cache: &PassabilityCache,
        floor: u8,
        from: Position,
        to: Position,
    ) -> bool {
        let dx = shortest_world_delta_x(from.x, to.x);
        let dz = to.z - from.z;
        let steps = (dx.hypot(dz) / DANCE_SLOPE_STEP_M).ceil().max(1.0) as usize;
        let mut here = from;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let mut next = Position {
                x: wrap_world_x(from.x + dx * t),
                y: here.y,
                z: from.z + dz * t,
            };
            next.y = self.surface_ground_y(floor, &next, here.y, None).await;
            if too_steep(cache, &self.bridge_decks, floor, &here, &next) {
                return false;
            }
            here = next;
        }
        true
    }

    async fn refuse_join_dance(&self, player_id: &PlayerId, (code, text): Refusal) {
        self.send_system_message(player_id, localized(code, text))
            .await;
    }
}

/// Why `joiner` cannot join any dance as they are now.
fn joiner_refusal(joiner: &Player) -> Option<Refusal> {
    if joiner.health == 0 || joiner.petrified {
        Some(REFUSE_NOT_NOW)
    } else if joiner.is_mounted() {
        Some(REFUSE_MOUNTED)
    } else if GameState::in_combat(joiner) {
        Some(REFUSE_COMBAT)
    } else {
        None
    }
}

/// Why a join that passed its checks still did not happen, given the joiner
/// as checked and as they are now.
fn failed_join_refusal(checked: &Player, now: Option<&Player>) -> Refusal {
    let Some(now) = now else {
        return REFUSE_GONE;
    };
    if let Some(refusal) = joiner_refusal(now) {
        refusal
    } else if now.position.dist_xz_sq(&checked.position) >= 0.01 {
        REFUSE_MOVED
    } else {
        REFUSE_GONE
    }
}

/// The dance `joiner` may join `target` in: a joinable dance by a visible,
/// living, unpetrified dancer on the joiner's floor, within reach, above
/// ground.
fn joinable_dance(joiner: &Player, target: &Player) -> Result<String, Refusal> {
    if joiner.floor_level < 0 {
        return Err(REFUSE_DUNGEON);
    }
    if let Some(refusal) = joiner_refusal(joiner) {
        return Err(refusal);
    }
    let dance = target
        .object_type
        .as_deref()
        .filter(|dance| JOINABLE_DANCES.contains(dance))
        .ok_or(REFUSE_GONE)?;
    let within_reach = joiner.floor_level == target.floor_level
        && joiner.position.dist_xz_sq(&target.position) <= JOIN_DANCE_RANGE_M.powi(2)
        && (joiner.position.y - target.position.y).abs() <= JOIN_DANCE_MAX_RISE_M;
    if joiner.id == target.id
        || target.health == 0
        || target.petrified
        || target.invisible
        || !within_reach
    {
        return Err(REFUSE_GONE);
    }
    Ok(dance.to_string())
}

fn dance_sync(player_id: &PlayerId, anchor: Instant) -> ServerMessage {
    ServerMessage::PlayerDanceSync {
        player_id: *player_id,
        elapsed_secs: anchor.elapsed().as_secs_f64(),
    }
}

/// Undo a join's claim on `anchor`, restoring the beat `before` it if its
/// group still has room, unless something has replaced the claim since.
fn hand_back(
    beats: &mut DanceBeats,
    player_id: &PlayerId,
    anchor: Instant,
    before: Option<Instant>,
) {
    if beats.get(player_id) != Some(anchor) {
        return;
    }
    match before {
        Some(before) if beats.group_size(before) < MAX_DANCE_GROUP => {
            beats.insert(*player_id, before)
        }
        _ => beats.remove(player_id),
    };
}

/// The beat of `target_id`'s group, if `player_id` may still join it.
fn group_anchor(
    beats: &DanceBeats,
    player_id: &PlayerId,
    target_id: &PlayerId,
) -> Result<Instant, Refusal> {
    let anchor = beats.get(target_id).ok_or(REFUSE_GONE)?;
    if beats.get(player_id) == Some(anchor) {
        return Err(REFUSE_ALREADY);
    }
    if beats.group_size(anchor) >= MAX_DANCE_GROUP {
        return Err(REFUSE_FULL);
    }
    Ok(anchor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: f32, z: f32) -> Position {
        Position { x, y: 0.0, z }
    }

    #[test]
    fn spots_start_at_the_shoulders_and_alternate_sides() {
        let spots = dance_spots(at(0.0, 0.0), 0.0, at(0.0, 3.0));
        // Facing +Z, right is +X.
        assert!((spots[0].x - DANCE_SPACING_M).abs() < 1e-4 && spots[0].z.abs() < 1e-4);
        assert!((spots[1].x + DANCE_SPACING_M).abs() < 1e-4 && spots[1].z.abs() < 1e-4);
        assert!((spots[2].x - 2.0 * DANCE_SPACING_M).abs() < 1e-4);
        assert!((spots[3].x + 2.0 * DANCE_SPACING_M).abs() < 1e-4);
    }

    #[test]
    fn the_shoulder_nearer_the_joiner_comes_first() {
        let spots = dance_spots(at(0.0, 0.0), 0.0, at(-3.0, 1.0));
        assert!((spots[0].x + DANCE_SPACING_M).abs() < 1e-4);
        assert!((spots[1].x - DANCE_SPACING_M).abs() < 1e-4);
        assert!(spots[9].x < 0.0 && spots[10].x > 0.0, "{:?}", &spots[9..11]);
    }

    #[test]
    fn the_second_row_stands_behind_the_dancer() {
        let spots = dance_spots(at(0.0, 0.0), 0.0, at(0.0, 3.0));
        assert_eq!(spots.len(), 13);
        let first_behind = spots[8];
        assert!(first_behind.x.abs() < 1e-4);
        assert!((first_behind.z + DANCE_ROW_GAP_M).abs() < 1e-4);
    }

    #[test]
    fn spots_turn_with_the_dancer() {
        let quarter = std::f32::consts::FRAC_PI_2;
        let first = dance_spots(at(0.0, 0.0), quarter, at(3.0, 0.0))[0];
        // Facing +X, right is -Z.
        assert!(first.x.abs() < 1e-4);
        assert!((first.z + DANCE_SPACING_M).abs() < 1e-4);
    }

    #[test]
    fn a_failed_join_hands_back_the_beat_it_replaced() {
        let joiner = PlayerId::from(1);
        let (old, new) = (
            Instant::now(),
            Instant::now() + std::time::Duration::from_secs(1),
        );
        let mut beats = DanceBeats::default();
        beats.insert(joiner, new);
        hand_back(&mut beats, &joiner, new, Some(old));
        assert_eq!(beats.get(&joiner), Some(old));
        assert_eq!((beats.group_size(old), beats.group_size(new)), (1, 0));

        beats.insert(joiner, new);
        hand_back(&mut beats, &joiner, new, None);
        assert_eq!(beats.get(&joiner), None);

        let restarted = new + std::time::Duration::from_secs(1);
        beats.insert(joiner, restarted);
        hand_back(&mut beats, &joiner, new, Some(old));
        assert_eq!(beats.get(&joiner), Some(restarted));
    }

    #[test]
    fn a_failed_join_never_overfills_the_group_it_left() {
        let (old, new) = (
            Instant::now(),
            Instant::now() + std::time::Duration::from_secs(1),
        );
        let mut beats = DanceBeats::default();
        for id in 2..=MAX_DANCE_GROUP as u64 + 1 {
            beats.insert(PlayerId::from(id), old);
        }
        let joiner = PlayerId::from(1);
        beats.insert(joiner, new);
        hand_back(&mut beats, &joiner, new, Some(old));
        assert_eq!(beats.get(&joiner), None);
        assert_eq!(beats.group_size(old), MAX_DANCE_GROUP);
    }

    #[test]
    fn a_join_lost_to_a_step_asks_the_joiner_to_stand_still() {
        use super::super::tests::make_player;
        let checked = make_player("joiner", 2.0, 2.0);
        let mut walked = checked.clone();
        walked.position.x += 0.5;
        assert_eq!(failed_join_refusal(&checked, Some(&walked)), REFUSE_MOVED);
        assert_eq!(failed_join_refusal(&checked, Some(&checked)), REFUSE_GONE);
        walked.health = 0;
        assert_eq!(failed_join_refusal(&checked, Some(&walked)), REFUSE_NOT_NOW);
        assert_eq!(failed_join_refusal(&checked, None), REFUSE_GONE);
    }

    #[test]
    fn group_sizes_follow_every_change_of_beat() {
        let (a, b) = (
            Instant::now(),
            Instant::now() + std::time::Duration::from_secs(1),
        );
        let mut beats = DanceBeats::default();
        for id in 1..=3 {
            beats.insert(PlayerId::from(id), a);
        }
        beats.insert(PlayerId::from(1), a);
        beats.insert(PlayerId::from(2), b);
        beats.remove(&PlayerId::from(3));
        beats.remove(&PlayerId::from(9));
        assert_eq!((beats.group_size(a), beats.group_size(b)), (1, 1));
        beats.remove(&PlayerId::from(1));
        assert_eq!(beats.group_size(a), 0);
        assert!(!beats.groups.contains_key(&a));
    }

    #[test]
    fn only_looping_emotes_keep_a_beat() {
        assert!(is_looping_emote("twist"));
        assert!(is_looping_emote("stand_pose2"));
        assert!(!is_looping_emote("clap"));
        assert!(!is_looping_emote("sit"));
    }

    #[tokio::test]
    async fn a_stand_fails_once_the_dancer_cannot_be_joined() {
        use super::super::tests::{make_player, make_test_game_state};
        let game = make_test_game_state("dance_stand_holds");
        let target = Player {
            object_type: Some("twist".into()),
            ..make_player("target", 0.0, 0.0)
        };
        let joiner = make_player("joiner", 2.0, 2.0);
        game.add_player(target.clone()).await;
        let stand = Stand {
            spot: at(1.1, 0.0),
            rotation: 0.0,
            joiner_at: joiner.position,
            floor_level: 0,
            target: target.id,
            target_at: target.position,
            dance: "twist".into(),
        };
        assert!(game.stand_holds(&stand, &joiner).await);
        let changes: [fn(&mut Player); 5] = [
            |p| p.health = 0,
            |p| p.petrified = true,
            |p| p.invisible = true,
            |p| p.object_type = Some("macarena".into()),
            |p| p.position.x += 1.0,
        ];
        for (i, change) in changes.into_iter().enumerate() {
            let mut changed = target.clone();
            change(&mut changed);
            game.players.write().await.insert(target.id, changed);
            assert!(!game.stand_holds(&stand, &joiner).await, "change {i}");
        }
    }
}
