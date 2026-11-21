//! Teleport bookmarks: named surface spots a character stood on and can
//! later teleport back to (doc/TELEPORTATION.md).

use serde::{Deserialize, Serialize};

pub const MAX_BOOKMARKS: usize = 32;
pub const MAX_NAME_CHARS: usize = 24;

/// Registry id of the NPC who blesses teleport scrolls.
pub const BLESSER: &str = "maelis";
/// Copper per scroll.
pub const BLESSING_PRICE: i64 = 5_000;
pub const SCROLL: &str = "scroll_of_teleportation";
pub const BLESSED_SCROLL: &str = "blessed_scroll_of_teleportation";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: i64,
    pub name: String,
    pub x: f32,
    pub z: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BookmarkError {
    Name,
    Duplicate,
    Full,
    /// Only the surface can be bookmarked.
    NotSurface,
    /// Nowhere to stand there: water, a wall, a building.
    NoLanding,
    /// Someone else's estate.
    Owned,
    Combat,
    Unknown,
    /// Neither a blessed scroll nor an item of teleport control.
    NoControl,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlessError {
    TooFar,
    Asleep,
    /// Away from the post: at a meal or seated in private.
    OffDuty,
    NoScrolls,
    Trading,
    NotEnoughGold,
    Failed,
}

/// A bookmark name as stored: trimmed, one line, at most `MAX_NAME_CHARS`.
pub fn clean_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars = name.chars().count();
    (chars > 0 && chars <= MAX_NAME_CHARS && !name.chars().any(char::is_control)).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(
            clean_name("  north   gate \n").as_deref(),
            Some("north gate")
        );
        assert_eq!(clean_name("   "), None);
        assert_eq!(
            clean_name(&"가".repeat(MAX_NAME_CHARS)).map(|n| n.chars().count()),
            Some(MAX_NAME_CHARS)
        );
        assert_eq!(clean_name(&"a".repeat(MAX_NAME_CHARS + 1)), None);
    }
}
