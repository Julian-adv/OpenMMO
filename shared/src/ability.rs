use serde::{Deserialize, Serialize};

pub const GUARDIAN_WARD_RADIUS: f32 = 20.0;
pub const GUARDIAN_WARD_DURATION_MS: u64 = 60_000;
pub const GUARDIAN_WARD_COOLDOWN_MS: u64 = 45_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AbilityId {
    GuardianWard,
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
