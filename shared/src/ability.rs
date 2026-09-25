use serde::{Deserialize, Serialize};

pub const GUARDIAN_WARD_RADIUS: f32 = 20.0;
pub const GUARDIAN_WARD_DURATION_MS: u64 = 60_000;
pub const GUARDIAN_WARD_COOLDOWN_MS: u64 = 45_000;
pub const GUARDIAN_WARD_MANA_COST: u32 = 2;
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
}

impl AbilityId {
    pub fn mana_cost(self) -> u32 {
        match self {
            Self::GuardianWard => GUARDIAN_WARD_MANA_COST,
            _ => 0,
        }
    }
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
