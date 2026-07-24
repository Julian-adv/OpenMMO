//! The CSV-derived data files store item-id lists as semicolon-separated
//! strings; parse them once at load so request handlers never re-split.
//! Point `#[serde(deserialize_with = "crate::semicolon_list::deserialize")]`
//! at the field (add `default` when the column may be absent).

use serde::{Deserialize, Deserializer};

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    Ok(raw
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect())
}
