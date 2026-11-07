//! Telegraphed boss skills: the brain casts, holds still, and strikes once.

use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::NearbyPlayer;
use crate::monster_body::{reach_point, BodyFootprint};
use crate::Position;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonsterSkillId {
    RuneSlash,
    EarthRoar,
    FormationBreak,
    Earthquake,
    CrystalEruption,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillArea {
    /// Radius and half-angle about the facing locked at cast start.
    Fan { range: f32, half_angle: f32 },
    /// Everyone within `radius` of the caster's body, walls aside.
    Zone { radius: f32 },
    /// Everyone on a spot laid at cast start (`MonsterSkill::spots`), walls
    /// aside.
    Spots,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SkillDamage {
    /// The plain swing's roll, times `multiplier`.
    Scaled {
        multiplier: u32,
        ignores_guard: bool,
    },
    /// Always lands for exactly this much; only a full health bar outlasts it.
    Fixed(u32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MonsterSkill {
    pub id: MonsterSkillId,
    /// The skill clip's length; the brain stands still and ignores staggers
    /// this long.
    pub cast_ms: f32,
    /// Damage lands once, this far into the cast.
    pub strike_ms: f32,
    pub cooldown_ms: f32,
    /// Cooldown already running when the brain spawns, so the first cast comes
    /// after a few plain swings.
    pub first_cast_ms: f32,
    pub area: SkillArea,
    pub damage: SkillDamage,
    pub spots: Option<SpotRule>,
    pub pools: Option<Pools>,
    /// debuffs.csv id put on everyone the strike lands on.
    pub debuff: Option<&'static str>,
}

/// Where a cast marks the ground, chosen when it starts so the marks are the
/// telegraph.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpotRule {
    pub count: usize,
    pub radius: f32,
    /// Players this far from the body get a mark under their feet.
    pub min_reach: f32,
    pub max_reach: f32,
    /// Bare ground in the same ring makes up the count.
    pub fill: bool,
}

/// Magma left on the cast's spots from the strike, burning whoever stands in
/// it once a tick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pools {
    pub tick_ms: f32,
    pub ticks: u32,
    pub damage: u32,
}

/// Skills a monster picks among, by weight, whenever one is ready.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillSet {
    pub skills: &'static [(&'static MonsterSkill, u32)],
    /// No skill starts within this long of the last cast ending.
    pub gap_ms: f32,
    pub last_stand: Option<LastStand>,
    pub shatter: Option<Shatter>,
}

/// With every player around turned to stone, the monster waits beside them
/// `wait_ms`, then breaks one free with a plain swing that lands
/// `multiplier` times over, guard ignored.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shatter {
    pub wait_ms: f32,
    pub multiplier: u32,
}

/// Below `health_ratio` the set gives way to `casts` of `skill`, one every
/// `every_ms` with no swing between, then plain swings only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LastStand {
    pub health_ratio: f32,
    pub skill: &'static MonsterSkill,
    pub casts: u8,
    pub every_ms: f32,
}

/// Times follow `BARKAN_RUNE_SLASH` and the blade sweep measured into
/// doc/assets/barkan-rune-slash-bake.json: the raised-sword telegraph runs to
/// 2.25s, the blade circles overhead, then sweeps right to left from 2.55s.
pub const RUNE_SLASH: MonsterSkill = MonsterSkill {
    id: MonsterSkillId::RuneSlash,
    cast_ms: 4200.0,
    strike_ms: 2550.0,
    cooldown_ms: 15000.0,
    first_cast_ms: 6000.0,
    area: SkillArea::Fan {
        range: 4.0,
        half_angle: std::f32::consts::FRAC_PI_4,
    },
    // The fan is dodged by stepping out, not by armor.
    damage: SkillDamage::Scaled {
        multiplier: 5,
        ignores_guard: true,
    },
    spots: None,
    pools: None,
    debuff: None,
};

/// Terraclash's roar (doc/TIER5_FIELD_RAID.md 대지의 포효), as far as its
/// ranged swing reaches (`rangedAttackRange`), which is a bow shot. 165 is what the lowest max HP
/// above level 35 survives once. Strikes follow the poses in
/// `tools/blender-scripts/rig_terraclash.py`.
pub const EARTH_ROAR: MonsterSkill = MonsterSkill {
    id: MonsterSkillId::EarthRoar,
    cast_ms: 6200.0,
    strike_ms: 4000.0,
    cooldown_ms: 45_000.0,
    first_cast_ms: 40_000.0,
    area: SkillArea::Zone { radius: 11.0 },
    damage: SkillDamage::Fixed(165),
    spots: None,
    pools: None,
    debuff: None,
};

/// 진형 붕괴: both fists slam the fan in front, and magma wells up under
/// whoever stands back from it.
pub const FORMATION_BREAK: MonsterSkill = MonsterSkill {
    id: MonsterSkillId::FormationBreak,
    cast_ms: 4400.0,
    strike_ms: 2800.0,
    cooldown_ms: 20_000.0,
    first_cast_ms: 12_000.0,
    area: SkillArea::Fan {
        range: 12.0,
        half_angle: 50.0 * std::f32::consts::PI / 180.0,
    },
    damage: SkillDamage::Scaled {
        multiplier: 3,
        ignores_guard: true,
    },
    spots: Some(SpotRule {
        count: 4,
        radius: 2.5,
        min_reach: 4.0,
        max_reach: 20.0,
        fill: false,
    }),
    pools: Some(Pools {
        tick_ms: 1000.0,
        ticks: 10,
        damage: 30,
    }),
    debuff: None,
};

/// 지진: the ground splits under most of the raid; the gaps between the marks
/// are the safe ground.
pub const EARTHQUAKE: MonsterSkill = MonsterSkill {
    id: MonsterSkillId::Earthquake,
    cast_ms: 5000.0,
    strike_ms: 3500.0,
    cooldown_ms: 30_000.0,
    first_cast_ms: 24_000.0,
    area: SkillArea::Spots,
    damage: SkillDamage::Fixed(120),
    spots: Some(SpotRule {
        count: 9,
        radius: 3.5,
        min_reach: 0.0,
        max_reach: 16.0,
        fill: true,
    }),
    pools: None,
    debuff: None,
};

/// The foreman's pick strikes the ground and black-violet crystal bursts up
/// under every player in reach, turning whoever stays on it to stone.
/// Timed to the leaping `slash3` slam baked into mine_foreman.glb
/// (assets/monsters/mine_foreman/rig_spec.json `skill_clips`).
pub const CRYSTAL_ERUPTION: MonsterSkill = MonsterSkill {
    id: MonsterSkillId::CrystalEruption,
    cast_ms: 3367.0,
    strike_ms: 2400.0,
    cooldown_ms: 25_000.0,
    first_cast_ms: 10_000.0,
    area: SkillArea::Spots,
    damage: SkillDamage::Scaled {
        multiplier: 1,
        ignores_guard: true,
    },
    spots: Some(SpotRule {
        count: 6,
        radius: 2.0,
        min_reach: 0.0,
        max_reach: 14.0,
        fill: false,
    }),
    pools: None,
    debuff: Some("petrified"),
};

const FOREMAN_SKILLS: SkillSet = SkillSet {
    skills: &[(&CRYSTAL_ERUPTION, 1)],
    gap_ms: 0.0,
    last_stand: None,
    shatter: Some(Shatter {
        wait_ms: 10_000.0,
        multiplier: 4,
    }),
};

const BARKAN_SKILLS: SkillSet = SkillSet {
    skills: &[(&RUNE_SLASH, 1)],
    gap_ms: 0.0,
    last_stand: None,
    shatter: None,
};

/// The roar comes least often; at 5% health it roars five times in a row
/// (최후의 발악).
pub const TERRACLASH_SKILLS: SkillSet = SkillSet {
    skills: &[(&FORMATION_BREAK, 3), (&EARTHQUAKE, 3), (&EARTH_ROAR, 2)],
    gap_ms: 8000.0,
    last_stand: Some(LastStand {
        health_ratio: 0.05,
        skill: &EARTH_ROAR,
        casts: 5,
        every_ms: 10_000.0,
    }),
    shatter: None,
};

/// Eruption radius of a ranged plain swing (`MonsterGroundStrike`).
pub const GROUND_STRIKE_RADIUS: f32 = 1.5;

pub fn skill_by_id(id: MonsterSkillId) -> &'static MonsterSkill {
    match id {
        MonsterSkillId::RuneSlash => &RUNE_SLASH,
        MonsterSkillId::EarthRoar => &EARTH_ROAR,
        MonsterSkillId::FormationBreak => &FORMATION_BREAK,
        MonsterSkillId::Earthquake => &EARTHQUAKE,
        MonsterSkillId::CrystalEruption => &CRYSTAL_ERUPTION,
    }
}

pub fn skills_for(monster_type: &str) -> Option<&'static SkillSet> {
    match monster_type {
        "barkan" => Some(&BARKAN_SKILLS),
        "terraclash" => Some(&TERRACLASH_SKILLS),
        "mine_foreman" => Some(&FOREMAN_SKILLS),
        _ => None,
    }
}

/// Allowance for a body straddling the fan's sides.
const BODY_RADIUS: f32 = 0.4;
/// Lag allowance on the edge of a spot or a ground strike.
pub const REACH_SLACK: f32 = 0.3;

impl MonsterSkill {
    /// How far from the caster the skill reaches.
    pub fn range(&self) -> f32 {
        match self.area {
            SkillArea::Fan { range, .. } => range,
            SkillArea::Zone { radius } => radius,
            SkillArea::Spots => self.spots.map_or(0.0, |rule| rule.max_reach),
        }
    }

    /// How far beyond the caster's body anything the skill does can land.
    pub fn full_reach(&self) -> f32 {
        let spots = self.spots.map_or(0.0, |rule| rule.max_reach);
        self.range().max(spots) + self.spot_reach()
    }

    /// How far from a spot's centre it strikes, lag included.
    pub fn spot_reach(&self) -> f32 {
        self.spots.map_or(0.0, |rule| rule.radius + REACH_SLACK)
    }

    /// Spots for a cast from `at`: under the players its rule picks, then
    /// bare ground where `passable` allows, none overlapping another.
    #[allow(clippy::too_many_arguments)]
    pub fn lay_spots(
        &self,
        body: Option<BodyFootprint>,
        at: Position,
        facing: f32,
        aim: f32,
        players: &[NearbyPlayer],
        passable: impl Fn(f32, f32) -> bool,
        rng: &mut impl Rng,
    ) -> Vec<Position> {
        let Some(rule) = self.spots else {
            return Vec::new();
        };
        let in_ring = |p: &Position| {
            let d = reach_point(body, at, facing, p).dist_xz_sq(p);
            d >= rule.min_reach.powi(2) && d <= rule.max_reach.powi(2)
        };
        let mut picks: Vec<&NearbyPlayer> = players
            .iter()
            .filter(|p| p.health > 0 && !p.petrified && in_ring(&p.position))
            .filter(|p| !self.fan_contains(&at, aim, &p.position))
            .collect();
        picks.shuffle(rng);
        let mut spots: Vec<Position> = Vec::with_capacity(rule.count);
        let apart = |spots: &[Position], p: &Position| {
            spots.iter().all(|s| s.dist_xz_sq(p) >= rule.radius.powi(2))
        };
        for player in picks {
            if spots.len() == rule.count {
                break;
            }
            if apart(&spots, &player.position) {
                spots.push(player.position);
            }
        }
        let mut tries = rule.count * 6;
        while rule.fill && spots.len() < rule.count && tries > 0 {
            tries -= 1;
            let bearing = rng.gen_range(0.0..std::f32::consts::TAU);
            let (sin, cos) = bearing.sin_cos();
            let far = Position {
                x: at.x + sin * 100.0,
                z: at.z + cos * 100.0,
                ..at
            };
            let edge = reach_point(body, at, facing, &far);
            let out = rng.gen_range(rule.min_reach.max(rule.radius * 0.5)..rule.max_reach);
            let spot = Position {
                x: edge.x + sin * out,
                z: edge.z + cos * out,
                ..at
            }
            .wrapped_x();
            if passable(spot.x, spot.z) && apart(&spots, &spot) {
                spots.push(spot);
            }
        }
        spots
    }

    /// Whether `target` stands on one of `spots`.
    pub fn spots_contain(&self, spots: &[Position], target: &Position) -> bool {
        let reach = self.spot_reach().powi(2);
        spots.iter().any(|s| s.dist_xz_sq(target) <= reach)
    }

    /// Whether `target` stands in the fan cast from `origin` facing `rotation`.
    pub fn fan_contains(
        &self,
        origin: &crate::Position,
        rotation: f32,
        target: &crate::Position,
    ) -> bool {
        let SkillArea::Fan { range, half_angle } = self.area else {
            return false;
        };
        let dist_sq = origin.dist_xz_sq(target);
        if !dist_sq.is_finite() || dist_sq > range.powi(2) {
            return false;
        }
        let Some(bearing) = origin.bearing_xz_to(target) else {
            return true;
        };
        let off = crate::mount_movement::angle_delta(rotation, bearing);
        let edge = (BODY_RADIUS / dist_sq.sqrt()).min(1.0).asin();
        off.abs() <= half_angle + edge
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Position;

    fn at(x: f32, z: f32) -> Position {
        Position { x, y: 0.0, z }
    }

    #[test]
    fn rune_slash_fan_covers_the_front_only() {
        let origin = at(0.0, 0.0);
        assert!(RUNE_SLASH.fan_contains(&origin, 0.0, &at(0.0, 3.0)));
        assert!(RUNE_SLASH.fan_contains(&origin, 0.0, &at(-2.0, 2.2)));
        assert!(!RUNE_SLASH.fan_contains(&origin, 0.0, &at(-2.5, 0.5)));
        assert!(!RUNE_SLASH.fan_contains(&origin, 0.0, &at(0.0, -2.0)));
        assert!(!RUNE_SLASH.fan_contains(&origin, 0.0, &at(0.0, 5.0)));
        let east = std::f32::consts::FRAC_PI_2;
        assert!(RUNE_SLASH.fan_contains(&origin, east, &at(3.0, 0.0)));
        assert!(!RUNE_SLASH.fan_contains(&origin, east, &at(-3.0, 0.0)));
    }
}
