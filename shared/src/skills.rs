//! Permanently learned character skills.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SkillId {
    #[serde(rename = "fishing")]
    Fishing,
}

impl SkillId {
    pub fn as_str(&self) -> &'static str {
        match self {
            SkillId::Fishing => "fishing",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            SkillId::Fishing => "Fishing",
        }
    }
}

impl std::str::FromStr for SkillId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "fishing" => Ok(SkillId::Fishing),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Skills {
    pub learned: BTreeSet<SkillId>,
}

impl Skills {
    pub fn has(&self, skill: SkillId) -> bool {
        self.learned.contains(&skill)
    }

    pub fn learn(&mut self, skill: SkillId) -> bool {
        self.learned.insert(skill)
    }
}
