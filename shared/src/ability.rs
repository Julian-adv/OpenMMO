use serde::{Deserialize, Serialize};

pub const GUARDIAN_WARD_RADIUS: f32 = 20.0;
pub const GUARDIAN_WARD_DURATION_MS: u64 = 60_000;
pub const GUARDIAN_WARD_COOLDOWN_MS: u64 = 45_000;
pub const GUARDIAN_WARD_MANA_COST: u32 = 2;
pub const RAGE_DURATION_MS: u64 = 10_000;
pub const RAGE_COOLDOWN_MS: u64 = 20_000;
pub const RAGE_MANA_COST: u32 = 1;
pub const RAGE_GUARD_PENALTY: i32 = 2;
pub const PRIMAL_ROAR_RADIUS: f32 = 4.0;
/// The stomp in the cast clip; the stun lands here.
pub const PRIMAL_ROAR_STOMP_MS: u64 = 1_050;
/// The caster's swings wait out the roar pose.
pub const PRIMAL_ROAR_CAST_MS: u64 = 1_850;
/// 1.5 s plus the 2.17 s roar clip the caster spends not swinging.
pub const PRIMAL_ROAR_STAGGER_MS: u64 = 3_700;
pub const PRIMAL_ROAR_DURATION_MS: u64 = 10_000;
pub const PRIMAL_ROAR_COOLDOWN_MS: u64 = 30_000;
pub const PRIMAL_ROAR_MANA_COST: u32 = 1;
pub const VALHALLA_CHARGE_MIN_RANGE: f32 = 4.0;
pub const VALHALLA_CHARGE_MAX_RANGE: f32 = 10.0;
pub const VALHALLA_CHARGE_DASH_MS: u64 = 400;
pub const VALHALLA_CHARGE_SLOW_MULT: f32 = 0.6;
pub const VALHALLA_CHARGE_SLOW_MS: u64 = 3_000;
/// For bosses and stagger-immune monsters.
pub const VALHALLA_CHARGE_SHORT_SLOW_MS: u64 = 1_500;
pub const VALHALLA_CHARGE_COOLDOWN_MS: u64 = 10_000;
pub const VALHALLA_CHARGE_MANA_COST: u32 = 1;
pub const HEALING_PRAYER_RANGE: f32 = 10.0;
pub const HEALING_PRAYER_COOLDOWN_MS: u64 = 8_000;
pub const HEALING_PRAYER_MANA_COST: u32 = 6;
pub const HEALING_PRAYER_DICE: &str = "2d6";
pub const BATTLE_HYMN_RADIUS: f32 = 20.0;
/// Spent when the playing finishes.
pub const BATTLE_HYMN_MANA_COST: u32 = 5;
/// The bard plays this long, unbroken, before the buff lands.
pub const BATTLE_HYMN_CAST_MS: u64 = 10_000;
pub const BATTLE_HYMN_DURATION_MS: u64 = 600_000;
pub const BATTLE_HYMN_SPEED_MULT: f32 = 1.08;
/// Recorded marches the web client holds (`battle-hymn-<n>.ogg`).
pub const BATTLE_HYMN_TUNES: u8 = 3;
pub const RADIANCE_DURATION_MS: u64 = 120_000;
pub const RADIANCE_COOLDOWN_MS: u64 = 800;
pub const BOW_MARK_DURATION_MS: u64 = 5_000;
pub const BOW_MARK_COOLDOWN_MS: u64 = 10_000;
pub const AUSCULTATION_RANGE: f32 = 2.0;
pub const AUSCULTATION_COOLDOWN_MS: u64 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityId {
    GuardianWard,
    Radiance,
    BowMark,
    DaggerDoubleSlash,
    Auscultation,
    Rage,
    PrimalRoar,
    ValhallaCharge,
    HealingPrayer,
    BattleHymn,
}

impl AbilityId {
    pub fn mana_cost(self) -> u32 {
        match self {
            Self::GuardianWard => GUARDIAN_WARD_MANA_COST,
            Self::Rage => RAGE_MANA_COST,
            Self::PrimalRoar => PRIMAL_ROAR_MANA_COST,
            Self::ValhallaCharge => VALHALLA_CHARGE_MANA_COST,
            Self::HealingPrayer => HEALING_PRAYER_MANA_COST,
            Self::BattleHymn => BATTLE_HYMN_MANA_COST,
            _ => 0,
        }
    }
}

pub fn guardian_ward_guard_bonus(caster_level: u32) -> i32 {
    2 + (caster_level / 20) as i32
}

pub fn rage_damage_bonus(level: u32) -> i32 {
    3 + (level / 10) as i32
}

pub fn primal_roar_temp_health(max_health: u32) -> u32 {
    (max_health * 15 / 100).max(4)
}

/// Added to the `HEALING_PRAYER_DICE` roll; `wis_mod` as in `mana::max_mana`.
pub fn healing_prayer_bonus(level: u32, wis: u8) -> i32 {
    (level / 3) as i32 + (i32::from(wis) - 10) / 2
}

pub fn healing_prayer_amount(roll: u32, level: u32, wis: u8) -> u32 {
    (roll as i32 + healing_prayer_bonus(level, wis)).max(1) as u32
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbilityTimer {
    pub ability: AbilityId,
    pub remaining_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityRejectReason {
    Unavailable,
    Equipment,
    Cooldown,
    OutOfRange,
    NotEnoughMana,
    TooClose,
    Blocked,
    FullHealth,
    NotUndead,
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InspectionTarget {
    Player { player_id: crate::PlayerId },
    Monster { monster_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectedEquipment {
    pub slot: crate::inventory::EquipSlot,
    pub item_def_id: String,
    pub enchant: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InspectionResult {
    pub target: InspectionTarget,
    pub name: String,
    pub level: u32,
    pub health: u32,
    pub max_health: u32,
    pub guard: i32,
    pub equipment: Vec<InspectedEquipment>,
}
