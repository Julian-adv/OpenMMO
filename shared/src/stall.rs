//! Stalls: the table a trader lays out to sell from. NPC merchants spread one
//! for free with `/lay_stall`; players buy a `peddler_stall` and toggle it.
//! Either way it lives only in server memory, one per owner, and folds up when
//! its owner strays, changes floor or logs out (doc/ECONOMY.md, doc/TRADE.md).

use serde::{Deserialize, Serialize};

/// A laid-out stall visible to nearby players. Wire type (positional array —
/// never reorder fields, append only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stall {
    pub id: u64,
    pub owner: crate::entity::PlayerId,
    pub position: crate::world::Position,
    /// Owner's yaw when laid out, so the long side faces them.
    pub rotation: f32,
    pub floor_level: i8,
    /// Shown under the sign, so a passer-by needs no roster lookup.
    pub owner_name: String,
    /// The owner's sign board. Empty for NPC stalls, and blanked per recipient
    /// when the owner is muted or the recipient has them blocked.
    pub sign: String,
}

/// How far the owner may wander before the stall is packed up automatically.
pub const STALL_LEASH_M: f32 = 10.0;

/// Distinct listings one stall may hold. Carrying capacity already bounds a
/// stall's bulk; this bounds the panel and the whole-state pushes.
pub const STALL_MAX_LISTINGS: usize = 12;

/// Sign length, matching the character-name limit rather than inventing a
/// second one.
pub const STALL_MAX_SIGN_CHARS: usize = 32;

/// Sales tax burned out of the seller's proceeds. Player-to-player trade
/// otherwise bypasses the merchant gold sink entirely (doc/PRICING.md).
pub const STALL_TAX_PERCENT: i64 = 5;

/// Tax on a sale, rounded down; the seller keeps the rest.
pub fn stall_tax(total: i64) -> i64 {
    total / 100 * STALL_TAX_PERCENT + total % 100 * STALL_TAX_PERCENT / 100
}

impl Stall {
    /// Owner out of leash range, or off this floor entirely.
    pub fn strayed_from(&self, position: &crate::world::Position, floor_level: i8) -> bool {
        self.floor_level != floor_level
            || self.position.dist_xz_sq(position) > STALL_LEASH_M * STALL_LEASH_M
    }
}

/// One priced entry on a stall. Keyed by bag instance, so an enchanted piece
/// stays distinguishable from its plain twin. Wire type (append only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StallListing {
    pub instance_id: u64,
    pub item_def_id: String,
    pub quantity: u32,
    pub enchant: i32,
    /// Copper per unit. Unbounded on purpose — pricing is the player's.
    pub unit_price: i64,
}
