//! Behavior input/output types: the internal [`AiState`], the [`NearbyPlayer`]
//! projection fed into each tick, and [`AiCommand`] outputs.

use super::MonsterSkillId;
use crate::{MonsterState, PlayerId, Position};
use serde::{Deserialize, Serialize};

/// Internal behavior state (superset of network [`MonsterState`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AiState {
    #[default]
    Idle,
    Walk,
    Run,
    Chase,
    Attack,
    Hit,
    Dead,
    Flee,
    Return,
    /// Chase on hold: queued behind a stander, or waiting on an unreachable
    /// target (doc/MONSTER_SEPARATION.md). Reports as Idle.
    Hold,
}

impl AiState {
    pub fn to_monster_state(self) -> MonsterState {
        match self {
            AiState::Idle => MonsterState::Idle,
            AiState::Walk => MonsterState::Walk,
            AiState::Run => MonsterState::Run,
            AiState::Chase => MonsterState::Run,
            AiState::Attack => MonsterState::Attack,
            AiState::Hit => MonsterState::Hit,
            AiState::Dead => MonsterState::Dead,
            AiState::Flee => MonsterState::Run,
            AiState::Return => MonsterState::Walk,
            AiState::Hold => MonsterState::Idle,
        }
    }

    /// Chasing, queued behind a stander, or swinging at a target.
    pub fn is_engaged(self) -> bool {
        matches!(self, AiState::Chase | AiState::Hold | AiState::Attack)
    }

    /// Walks a path between ticks.
    pub fn is_on_the_move(self) -> bool {
        matches!(
            self,
            AiState::Chase | AiState::Walk | AiState::Run | AiState::Flee | AiState::Return
        )
    }
}

/// How a plain swing reaches its target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SwingReach {
    Melee,
    /// Past melee reach, the ground under the target is struck from where the
    /// monster stands, out to this far.
    Ground(f32),
}

/// Minimal player projection for behavior input.
#[derive(Debug, Clone)]
pub struct NearbyPlayer {
    pub id: PlayerId,
    pub position: Position,
    pub health: u32,
    /// Drank a potion in the last few seconds.
    pub drank_recently: bool,
    /// Turned to stone: out of reach unless the monster shatters it
    /// (`SkillSet::shatter`).
    pub petrified: bool,
}

/// Other monsters' last-synced poses, for cell-occupancy separation
/// (doc/MONSTER_SEPARATION.md). Caller filters out dead monsters.
#[derive(Debug, Clone)]
pub struct NearbyMonster {
    pub id: String,
    pub position: Position,
    /// Last-synced network state. Only stationary states
    /// ([`MonsterState::is_stationary`]) occupy cells — a ~500ms-stale
    /// position is wrong for a mover.
    pub state: MonsterState,
    pub path_floor: u8,
}

/// The chased player and the radius the chase stops and swings at, so a
/// viewer's live aim lands where the server's attack transition will.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ChaseAim {
    pub player_id: PlayerId,
    pub stop_range: f32,
}

/// Behavior output applied by the server.
#[derive(Debug, Clone)]
pub enum AiCommand {
    Move {
        position: Position,
        rotation: f32,
        state: MonsterState,
        /// Where a remote view walks the model until the next sync — a point on
        /// the mover's own path, not its destination. Aiming a viewer's straight
        /// line at the destination walks the model through the walls the path
        /// goes around. See `MonsterBrain::current_leg_target`.
        target_position: Position,
        /// Set on chase legs; a viewer can aim the walk at the chased
        /// player's live local position instead of the sync-old point above,
        /// stopping at the carried radius.
        chasing: Option<ChaseAim>,
    },
    Attack {
        target_player_id: PlayerId,
        reach: SwingReach,
    },
    /// A skill wound up facing `rotation`, marking `spots`; viewers play it
    /// from now.
    CastSkill {
        skill: MonsterSkillId,
        rotation: f32,
        spots: Vec<Position>,
    },
    /// The cast's damage moment, from where it was cast.
    SkillStrike {
        skill: MonsterSkillId,
        position: Position,
        rotation: f32,
        spots: Vec<Position>,
    },
}
