//! Title definitions from data-src/titles.csv (doc/TITLES.md).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Deserialize)]
pub struct TitleDef {
    pub id: String,
    pub name: String,
    pub source: String,
    #[serde(rename = "bossId", default)]
    pub boss_id: Option<String>,
    #[serde(rename = "itemId", default)]
    pub item_id: Option<String>,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub order: u32,
    /// A title this one outranks: earning it auto-replaces that one when shown.
    #[serde(default)]
    pub supersedes: Option<String>,
}

static DEFS: LazyLock<HashMap<String, TitleDef>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../data/titles.json"))
        .expect("Failed to parse titles.json")
});

pub fn title_def(id: &str) -> Option<&'static TitleDef> {
    DEFS.get(id)
}

/// The kill titles for `boss_type`: (shared, solo), either may be absent.
pub fn boss_kill_titles(boss_type: &str) -> (Option<&'static TitleDef>, Option<&'static TitleDef>) {
    let mut shared = None;
    let mut solo = None;
    for def in DEFS.values() {
        if def.source == "boss_kill" && def.boss_id.as_deref() == Some(boss_type) {
            if def.solo {
                solo = Some(def);
            } else {
                shared = Some(def);
            }
        }
    }
    (shared, solo)
}

/// The raid titles for `boss_type`: (raiders', final blow's), either may be
/// absent.
pub fn field_raid_titles(
    boss_type: &str,
) -> (Option<&'static TitleDef>, Option<&'static TitleDef>) {
    let find = |source: &str| {
        DEFS.values()
            .find(|def| def.source == source && def.boss_id.as_deref() == Some(boss_type))
    };
    (find("field_raid"), find("field_raid_last_hit"))
}

/// The title for catching `item_id`, if any.
pub fn fishing_catch_title(item_id: &str) -> Option<&'static TitleDef> {
    DEFS.values()
        .find(|def| def.source == "fishing" && def.item_id.as_deref() == Some(item_id))
}

/// Order earned title ids as the definitions list them.
pub fn sort_ids(ids: &mut [String]) {
    ids.sort_by_key(|id| (title_def(id).map_or(u32::MAX, |d| d.order), id.clone()));
}

/// Boot-time check: every boss and raid title names a real monster, every
/// fishing title a real item.
pub fn validate(
    monster_defs: &crate::monster_defs::MonsterDefs,
    item_defs: &crate::item_defs::ItemDefs,
) {
    for def in DEFS.values() {
        if let Some(over) = &def.supersedes {
            assert!(
                DEFS.contains_key(over),
                "title '{}' supersedes unknown title '{}'",
                def.id,
                over
            );
        }
        match def.source.as_str() {
            "boss_kill" | "field_raid" | "field_raid_last_hit" => {
                let boss = def
                    .boss_id
                    .as_deref()
                    .unwrap_or_else(|| panic!("title '{}' has no bossId", def.id));
                assert!(
                    monster_defs.get(boss).is_some(),
                    "title '{}' names unknown boss '{}'",
                    def.id,
                    boss
                );
            }
            "fishing" => {
                let item = def
                    .item_id
                    .as_deref()
                    .unwrap_or_else(|| panic!("title '{}' has no itemId", def.id));
                assert!(
                    item_defs.get(item).is_some(),
                    "title '{}' names unknown item '{}'",
                    def.id,
                    item
                );
            }
            "estate_defense" => assert!(
                def.boss_id.as_deref().is_some_and(|grade| {
                    crate::estate_defense_defs::estate_defense_grades()
                        .iter()
                        .any(|g| g.id == grade)
                }),
                "title '{}' names no estate defense grade",
                def.id
            ),
            "disgrace" => {}
            other => panic!("title '{}' has unknown source '{}'", def.id, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_boss_has_a_shared_and_a_solo_title() {
        for boss in onlinerpg_shared::dungeon::entrances()
            .iter()
            .map(|entrance| entrance.boss.as_str())
        {
            let (shared, solo) = boss_kill_titles(boss);
            assert!(shared.is_some_and(|d| !d.solo), "{boss} shared");
            assert!(solo.is_some_and(|d| d.solo), "{boss} solo");
            assert_eq!(
                solo.unwrap().supersedes.as_deref(),
                Some(shared.unwrap().id.as_str())
            );
        }
        assert!(boss_kill_titles("goblin").0.is_none());
    }

    #[test]
    fn every_field_boss_has_a_raid_and_a_final_blow_title() {
        for boss in onlinerpg_shared::field_boss::field_boss_defs() {
            let (raid, final_blow) = field_raid_titles(&boss.monster);
            let raid = raid.unwrap_or_else(|| panic!("{} raid", boss.monster));
            let final_blow = final_blow.unwrap_or_else(|| panic!("{} final blow", boss.monster));
            assert_eq!(final_blow.supersedes.as_deref(), Some(raid.id.as_str()));
        }
    }

    #[test]
    fn the_golden_sturgeon_has_a_catch_title() {
        let def = fishing_catch_title("golden_sturgeon").expect("sturgeon title");
        assert!(!def.solo);
        assert!(fishing_catch_title("raw_minnow").is_none());
    }

    #[test]
    fn titles_validate_against_the_monster_and_item_tables() {
        validate(
            &crate::monster_defs::MonsterDefs::load(),
            crate::item_defs::item_defs(),
        );
    }
}
