//! Estate defense grades from data-src/estate_defense.csv (doc/ESTATE_DEFENSE.md).

use crate::item_defs::ItemDefs;
use crate::monster_defs::MonsterDefs;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    horn: String,
    #[serde(deserialize_with = "crate::semicolon_list::deserialize")]
    monsters: Vec<String>,
    min_level: u8,
    max_level: u8,
    leader: String,
    leader_level: u8,
    place_bonus: i32,
    chest_tier: u8,
    chest: String,
    title: String,
}

/// One line of a chest bundle: one of `items`, `quantity` times, at `chance`.
#[derive(Debug)]
pub struct ChestLine {
    pub items: Vec<String>,
    pub quantity: u32,
    pub chance: f32,
}

#[derive(Debug)]
pub struct DefenseGrade {
    pub id: String,
    pub horn: String,
    pub monsters: Vec<String>,
    pub min_level: u8,
    pub max_level: u8,
    pub leader: String,
    pub leader_level: u8,
    pub place_bonus: i32,
    pub chest_tier: u8,
    pub chest: Vec<ChestLine>,
    pub title: String,
}

fn chest_line(spec: &str) -> ChestLine {
    let mut parts = spec.split(':');
    let items = parts
        .next()
        .unwrap_or_default()
        .split('/')
        .map(str::to_owned)
        .collect();
    let quantity = parts
        .next()
        .map_or(1, |q| q.parse().expect("chest quantity"));
    let chance = parts
        .next()
        .map_or(1.0, |c| c.parse().expect("chest chance"));
    ChestLine {
        items,
        quantity,
        chance,
    }
}

static GRADES: LazyLock<Vec<DefenseGrade>> = LazyLock::new(|| {
    let rows: HashMap<String, Row> =
        serde_json::from_str(include_str!("../../data/estate_defense.json"))
            .expect("Failed to parse estate_defense.json");
    let mut grades: Vec<DefenseGrade> = rows
        .into_values()
        .map(|row| DefenseGrade {
            id: row.id,
            horn: row.horn,
            monsters: row.monsters,
            min_level: row.min_level,
            max_level: row.max_level,
            leader: row.leader,
            leader_level: row.leader_level,
            place_bonus: row.place_bonus,
            chest_tier: row.chest_tier,
            chest: row.chest.split('|').map(chest_line).collect(),
            title: row.title,
        })
        .collect();
    grades.sort_by(|a, b| a.id.cmp(&b.id));
    grades
});

pub fn estate_defense_grades() -> &'static [DefenseGrade] {
    &GRADES
}

pub fn grade_for_horn(item_def_id: &str) -> Option<&'static DefenseGrade> {
    GRADES.iter().find(|g| g.horn == item_def_id)
}

pub fn validate(monster_defs: &MonsterDefs, item_defs: &ItemDefs) {
    for grade in GRADES.iter() {
        assert!(
            item_defs.get(&grade.horn).is_some(),
            "estate defense '{}' names unknown horn '{}'",
            grade.id,
            grade.horn
        );
        for monster in grade.monsters.iter().chain([&grade.leader]) {
            assert!(
                monster_defs.get(monster).is_some(),
                "estate defense '{}' names unknown monster '{monster}'",
                grade.id
            );
        }
        assert!(grade.min_level <= grade.max_level && grade.max_level < grade.leader_level);
        for line in &grade.chest {
            assert!(line.quantity > 0 && (0.0..=1.0).contains(&line.chance));
            for item in &line.items {
                assert!(
                    item_defs.get(item).is_some(),
                    "estate defense '{}' chest names unknown item '{item}'",
                    grade.id
                );
            }
        }
        assert!(
            crate::title_defs::title_def(&grade.title).is_some(),
            "estate defense '{}' names unknown title '{}'",
            grade.id,
            grade.title
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grade_names_real_monsters_items_and_titles() {
        validate(&MonsterDefs::load(), &ItemDefs::load());
        assert_eq!(estate_defense_grades().len(), 4);
    }
}
