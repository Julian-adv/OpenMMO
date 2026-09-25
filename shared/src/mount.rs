//! Shared mount rules and tuning; see `doc/MOUNTS.md`.

use serde::{Deserialize, Serialize};

const HORSE_MOVE_MULT: f32 = 3.0;
const HORSE_TURN_RADIUS: f32 = 0.65;
// Faster than wading, slower than an unmounted sprint.
const ROWBOAT_MOVE_MULT: f32 = 1.25;
const ROWBOAT_TURN_RADIUS: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MountKind {
    Horse,
    Rowboat,
}

impl MountKind {
    /// Parse the wire name used by the browser's WASM lookups.
    pub fn from_wire(kind: &str) -> Option<Self> {
        match kind {
            "horse" => Some(Self::Horse),
            "rowboat" => Some(Self::Rowboat),
            _ => None,
        }
    }

    /// Losing this item ends the ride.
    pub fn item_id(self) -> &'static str {
        match self {
            Self::Horse => "horse_reins",
            Self::Rowboat => "rowboat",
        }
    }

    /// Multiplier on walk and sprint speed alike.
    pub fn speed_mult(self) -> f32 {
        match self {
            Self::Horse => HORSE_MOVE_MULT,
            Self::Rowboat => ROWBOAT_MOVE_MULT,
        }
    }

    pub fn turn_radius(self) -> f32 {
        match self {
            Self::Horse => HORSE_TURN_RADIUS,
            Self::Rowboat => ROWBOAT_TURN_RADIUS,
        }
    }

    pub fn dismounts_in_combat(self) -> bool {
        match self {
            Self::Horse => true,
            Self::Rowboat => false,
        }
    }

    /// Floating mounts use water height and prevent soaking.
    pub fn floats(self) -> bool {
        match self {
            Self::Horse => false,
            Self::Rowboat => true,
        }
    }

    pub fn cannot_mount_here_message(self) -> &'static str {
        match self {
            Self::Horse => "Mount on outdoor ground while alive and out of combat.",
            Self::Rowboat => "Launch the boat while standing in water, outdoors and alive.",
        }
    }

    /// Inclusive depth limits in metres; dry land has negative depth.
    pub fn water_depth_band(self) -> (f32, f32) {
        match self {
            Self::Horse => (f32::NEG_INFINITY, 0.6),
            Self::Rowboat => (crate::fishing::MIN_FISHABLE_DEPTH_M, f32::INFINITY),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MountKind;

    #[test]
    fn every_kind_round_trips_through_its_wire_name() {
        for kind in [MountKind::Horse, MountKind::Rowboat] {
            let wire = serde_json::to_value(kind).unwrap();
            let name = wire.as_str().expect("kinds serialize as bare strings");
            assert_eq!(MountKind::from_wire(name), Some(kind), "{name}");
        }
        assert_eq!(MountKind::from_wire("griffin"), None);
    }
}
