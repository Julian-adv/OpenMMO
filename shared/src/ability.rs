use serde::{Deserialize, Serialize};

pub const GUARDIAN_WARD_RADIUS: f32 = 20.0;
pub const GUARDIAN_WARD_DURATION_MS: u64 = 60_000;
pub const GUARDIAN_WARD_COOLDOWN_MS: u64 = 45_000;
pub const RADIANCE_DURATION_MS: u64 = 120_000;
pub const RADIANCE_COOLDOWN_MS: u64 = 800;
pub const BOW_MARK_DURATION_MS: u64 = 5_000;
pub const BOW_MARK_COOLDOWN_MS: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityId {
    GuardianWard,
    Radiance,
    BowMark,
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
}
