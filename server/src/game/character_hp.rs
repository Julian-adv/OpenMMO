use std::fmt::{Display, Formatter};

pub const DEFAULT_CHARACTER_RACE: &str = "human";
pub const DEFAULT_CHARACTER_CLASS: &str = "knight";

const RACE_HP_BONUSES: &[(&str, i32)] = &[
    ("human", 2),
    ("elf", 1),
    ("dwarf", 4),
    ("gnome", 1),
    ("orc", 1),
];

const CLASS_BASE_HP: &[(&str, u32)] = &[
    ("knight", 14),
    ("barbarian", 14),
    ("caveman", 14),
    ("valkyrie", 14),
    ("ranger", 13),
    ("samurai", 13),
    ("monk", 12),
    ("priest", 12),
    ("archaeologist", 11),
    ("healer", 11),
    ("rogue", 10),
    ("wizard", 10),
    ("tourist", 8),
];

#[derive(Debug)]
pub enum CharacterHpTableError {
    InvalidInput,
    UnknownRace(String),
    UnknownClass(String),
    OutOfRange,
}

impl Display for CharacterHpTableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterHpTableError::InvalidInput => write!(f, "Race and class are required"),
            CharacterHpTableError::UnknownRace(race) => write!(f, "Unknown race '{race}'"),
            CharacterHpTableError::UnknownClass(class_name) => {
                write!(f, "Unknown class '{class_name}'")
            }
            CharacterHpTableError::OutOfRange => write!(f, "Computed max HP is out of range"),
        }
    }
}

impl std::error::Error for CharacterHpTableError {}

pub fn level_one_max_hp(race: &str, class_name: &str) -> Result<u32, CharacterHpTableError> {
    let normalized_race = race.trim().to_lowercase();
    let normalized_class_name = class_name.trim().to_lowercase();
    if normalized_race.is_empty() || normalized_class_name.is_empty() {
        return Err(CharacterHpTableError::InvalidInput);
    }

    let race_bonus = RACE_HP_BONUSES
        .iter()
        .find_map(|(name, bonus)| (*name == normalized_race).then_some(*bonus))
        .ok_or_else(|| CharacterHpTableError::UnknownRace(normalized_race.clone()))?;

    let class_base_hp = CLASS_BASE_HP
        .iter()
        .find_map(|(name, base)| (*name == normalized_class_name).then_some(*base))
        .ok_or_else(|| CharacterHpTableError::UnknownClass(normalized_class_name.clone()))?;

    let max_hp = i64::from(class_base_hp) + i64::from(race_bonus);
    if !(1..=i64::from(u32::MAX)).contains(&max_hp) {
        return Err(CharacterHpTableError::OutOfRange);
    }

    Ok(max_hp as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_knight_level_one_max_hp_is_sixteen() {
        let max_hp = level_one_max_hp("human", "knight").unwrap();
        assert_eq!(max_hp, 16);
    }

    #[test]
    fn lookup_is_case_insensitive_and_trimmed() {
        let max_hp = level_one_max_hp(" Human ", " KNIGHT ").unwrap();
        assert_eq!(max_hp, 16);
    }
}
