//! Where the defeated wake: the inn, or an aid cot on an estate (doc/RESPAWN_POINT.md).

use serde::{Deserialize, Serialize};

/// Estate furniture the defeated can wake on.
pub const AID_COT: &str = "furniture_aid_cot";

/// Who may wake on an estate's aid cots, set by the estate's owner.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShelterAccess {
    /// The estate account's own characters.
    #[default]
    Estate,
    /// Also the members of the family the estate is attached to.
    Family,
    /// Anyone who fell nearer to it than to the inn.
    Public,
}

impl ShelterAccess {
    pub fn code(self) -> i64 {
        match self {
            Self::Estate => 0,
            Self::Family => 1,
            Self::Public => 2,
        }
    }

    pub fn from_code(code: i64) -> Self {
        match code {
            1 => Self::Family,
            2 => Self::Public,
            _ => Self::Estate,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RespawnPlace {
    /// The inn's sick room.
    #[default]
    Town,
    /// The account's own estate.
    Estate,
    /// The family seat.
    FamilySeat,
    /// The public shelter offered in `RespawnOptions::nearby`.
    Nearby { estate_id: i64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NearbyShelter {
    pub estate_id: i64,
    /// The estate's name, else its owner's.
    pub name: String,
    pub distance_m: f32,
}

/// Public shelters for the world map.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShelterMarker {
    pub estate_id: i64,
    pub name: String,
    pub x: f32,
    pub z: f32,
}
