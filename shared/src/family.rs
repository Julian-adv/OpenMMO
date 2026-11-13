//! Families (doc/FAMILY_SYSTEM.md): shared limits, rank permissions and the
//! roster a member's client is sent.

use serde::{Deserialize, Serialize};

use crate::character::CharacterClass;

pub const FAMILY_MAX_MEMBERS: usize = 30;
/// The land registrar's fee stamp, spent on family filings.
pub const FAMILY_STAMP_ITEM: &str = "revenue_stamp";
/// The priest who presides over family mergers (`data-src/npcs.csv`).
pub const MERGER_OFFICIANT: &str = "Maelis";
pub const FAMILY_FOUND_STAMPS: u32 = 10;
pub const FAMILY_RENAME_STAMPS: u32 = 10;
pub const FAMILY_DISSOLVE_STAMPS: u32 = 1;
pub const FAMILY_RENAME_INTERVAL_SECS: i64 = 30 * 24 * 60 * 60;
/// How long a dissolved or renamed-away family name stays unusable.
pub const FAMILY_NAME_RETIRE_SECS: i64 = 30 * 24 * 60 * 60;
pub const FAMILY_REJOIN_COOLDOWN_SECS: i64 = 24 * 60 * 60;
/// A head unseen for this long can be succeeded by the next in line.
pub const FAMILY_HEAD_INACTIVE_SECS: i64 = 30 * 24 * 60 * 60;
pub const FAMILY_RANK_LABEL_MAX_CHARS: usize = 12;
pub const FAMILY_INVITE_TTL: std::time::Duration = std::time::Duration::from_secs(120);
/// Estate furniture that belongs to the family the estate is attached to.
pub const FAMILY_CHEST_ITEM: &str = "family_chest";
pub const FAMILY_BOARD_ITEM: &str = "family_board";
pub const FAMILY_CHEST_LIMIT: i64 = 4;
/// Storage log entries a member is shown, and kept per family.
pub const FAMILY_STORAGE_LOG_SHOWN: usize = 200;
pub const FAMILY_STORAGE_LOG_KEPT: i64 = 2000;

pub fn is_family_furniture(item_def_id: &str) -> bool {
    item_def_id == FAMILY_CHEST_ITEM
        || item_def_id == FAMILY_BOARD_ITEM
        || shows_family_emblem(item_def_id)
}

pub const FAMILY_RANK_HEAD: u8 = 0;
pub const FAMILY_RANK_ELDER: u8 = 1;
pub const FAMILY_RANK_MEMBER: u8 = 2;
pub const FAMILY_RANK_GUEST: u8 = 3;
pub const FAMILY_RANK_COUNT: u8 = 4;

/// Permission bits a head grants per rank. The head's own rank always holds
/// every bit.
pub mod permission {
    pub const INVITE: u32 = 1 << 0;
    pub const KICK: u32 = 1 << 1;
    pub const BUILD_HOUSE: u32 = 1 << 2;
    pub const DEMOLISH_HOUSE: u32 = 1 << 3;
    pub const PLACE_FURNITURE: u32 = 1 << 4;
    pub const LANDSCAPE: u32 = 1 << 5;
    pub const STORAGE_DEPOSIT: u32 = 1 << 6;
    pub const STORAGE_WITHDRAW: u32 = 1 << 7;
    pub const BOARD_WRITE: u32 = 1 << 8;
    pub const BOARD_MODERATE: u32 = 1 << 9;
    pub const TREASURY_DEPOSIT: u32 = 1 << 10;
    pub const EDIT_EMBLEM: u32 = 1 << 11;
    pub const EDIT_RANKS: u32 = 1 << 12;
    pub const ALL: u32 = (1 << 13) - 1;
}

pub fn default_rank_permissions(rank: u8) -> u32 {
    use permission::*;
    match rank {
        FAMILY_RANK_HEAD => ALL,
        FAMILY_RANK_ELDER => {
            INVITE
                | KICK
                | BUILD_HOUSE
                | DEMOLISH_HOUSE
                | PLACE_FURNITURE
                | LANDSCAPE
                | STORAGE_DEPOSIT
                | STORAGE_WITHDRAW
                | BOARD_WRITE
                | BOARD_MODERATE
                | TREASURY_DEPOSIT
        }
        FAMILY_RANK_MEMBER => {
            PLACE_FURNITURE | STORAGE_DEPOSIT | STORAGE_WITHDRAW | BOARD_WRITE | TREASURY_DEPOSIT
        }
        FAMILY_RANK_GUEST => STORAGE_DEPOSIT | TREASURY_DEPOSIT,
        _ => 0,
    }
}

pub fn default_rank_label(rank: u8) -> &'static str {
    match rank {
        FAMILY_RANK_HEAD => "Head",
        FAMILY_RANK_ELDER => "Elder",
        FAMILY_RANK_MEMBER => "Family",
        _ => "Guest",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyRank {
    pub label: String,
    pub permissions: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyMember {
    pub character_id: i64,
    pub name: String,
    pub level: u32,
    pub class: CharacterClass,
    pub rank: u8,
    pub online: bool,
}

/// An estate attached to the family. `mine` marks the recipient's own
/// account's estate, the only one they may detach.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyEstate {
    pub estate_id: i64,
    pub owner_name: String,
    pub plots: u32,
    pub missed: u32,
    pub mine: bool,
}

pub const FAMILY_BANNER_ITEM: &str = "family_banner";
pub const FAMILY_PENNANT_ITEM: &str = "family_pennant";

/// Furniture that shows the family emblem.
pub fn shows_family_emblem(item_def_id: &str) -> bool {
    item_def_id == FAMILY_BANNER_ITEM || item_def_id == FAMILY_PENNANT_ITEM
}

/// A family emblem as furniture shows it: a cape texture hash and a colour.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Emblem {
    pub texture: Option<String>,
    pub color: Option<String>,
}

impl Emblem {
    pub fn is_set(&self) -> bool {
        self.texture.is_some() || self.color.is_some()
    }
}

/// Which of the two families a merge term keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeSide {
    Proposer,
    Target,
}

/// What survives a merge. The family whose name is kept absorbs the other
/// and keeps its ranks; the other head becomes an elder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FamilyMergeTerms {
    pub keep_name: MergeSide,
    pub keep_emblem: MergeSide,
    pub keep_head: MergeSide,
}

/// A merge offer as its target head sees it, with the outcome spelled out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyMergeProposal {
    pub proposer_id: crate::entity::PlayerId,
    pub proposer_name: String,
    pub proposer_family: String,
    pub target_name: String,
    pub target_family: String,
    pub terms: FamilyMergeTerms,
    /// The emblem `terms.keep_emblem` leaves the merged family.
    pub emblem: Emblem,
    /// Members of the merged family.
    pub members: u32,
}

pub const FAMILY_MERGE_TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// One family chest movement; `quantity` is positive for a deposit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyStorageEntry {
    pub character_name: String,
    pub item_def_id: String,
    pub quantity: i64,
    /// Unix seconds.
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FamilyInfo {
    pub name: String,
    /// Indexed by rank, `FAMILY_RANK_COUNT` entries.
    pub ranks: Vec<FamilyRank>,
    /// Succession order: by rank, then by time joined.
    pub members: Vec<FamilyMember>,
    pub estates: Vec<FamilyEstate>,
    pub my_rank: u8,
    /// The recipient's account owns an estate this family does not hold yet.
    pub can_attach_estate: bool,
    /// The head has been away long enough for the recipient to succeed them.
    pub can_claim_head: bool,
    /// Unix seconds from which the head may rename the family; 0 = now.
    pub rename_at: i64,
    /// Cape texture hash and `#rrggbb` colour members' capes show.
    pub emblem_texture: Option<String>,
    pub emblem_color: Option<String>,
    /// The recipient's cape shows the emblem rather than their own print.
    pub my_cape_emblem: bool,
    /// Where a Scroll of Family Return lands.
    pub seat_estate_id: Option<i64>,
}
