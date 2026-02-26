use rand::Rng;
use std::fmt::{Display, Formatter};

pub const DEFAULT_CHARACTER_RACE: &str = "human";

const RACE_HP_BONUSES: &[(&str, i32)] = &[
    ("human", 2),
    ("elf", 1),
    ("dwarf", 4),
    ("gnome", 1),
    ("orc", 1),
];

const CLASS_HIT_DICE: &[(&str, u8)] = &[
    ("warrior", 10),
    ("knight", 10),
    ("barbarian", 10),
    ("caveman", 10),
    ("valkyrie", 10),
    ("ranger", 8),
    ("samurai", 8),
    ("monk", 8),
    ("priest", 8),
    ("archaeologist", 6),
    ("healer", 6),
    ("thief", 8),
    ("rogue", 6),
    ("wizard", 6),
    ("tourist", 4),
];

const MIN_ATTRIBUTE: u8 = 3;
const MAX_ATTRIBUTE: u8 = 18;

#[derive(Debug)]
pub enum CharacterHpTableError {
    InvalidInput,
    InvalidCon(u8),
    UnknownRace(String),
    UnknownClass(String),
    OutOfRange,
}

impl Display for CharacterHpTableError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CharacterHpTableError::InvalidInput => write!(f, "Race and class are required"),
            CharacterHpTableError::InvalidCon(con) => {
                write!(f, "Constitution value '{con}' is out of range")
            }
            CharacterHpTableError::UnknownRace(race) => write!(f, "Unknown race '{race}'"),
            CharacterHpTableError::UnknownClass(class_name) => {
                write!(f, "Unknown class '{class_name}'")
            }
            CharacterHpTableError::OutOfRange => write!(f, "Computed max HP is out of range"),
        }
    }
}

impl std::error::Error for CharacterHpTableError {}

pub fn level_one_max_hp(
    race: &str,
    class_name: &str,
    con: u8,
) -> Result<u32, CharacterHpTableError> {
    let normalized_race = race.trim().to_lowercase();
    let normalized_class_name = class_name.trim().to_lowercase();
    if normalized_race.is_empty() || normalized_class_name.is_empty() {
        return Err(CharacterHpTableError::InvalidInput);
    }
    if !(MIN_ATTRIBUTE..=MAX_ATTRIBUTE).contains(&con) {
        return Err(CharacterHpTableError::InvalidCon(con));
    }

    let race_bonus = RACE_HP_BONUSES
        .iter()
        .find_map(|(name, bonus)| (*name == normalized_race).then_some(*bonus))
        .ok_or_else(|| CharacterHpTableError::UnknownRace(normalized_race.clone()))?;

    let class_hit_die = CLASS_HIT_DICE
        .iter()
        .find_map(|(name, sides)| (*name == normalized_class_name).then_some(*sides))
        .ok_or_else(|| CharacterHpTableError::UnknownClass(normalized_class_name.clone()))?;

    let con_mod = con_modifier(con);
    let max_hp = i64::from(class_hit_die) + i64::from(con_mod) + i64::from(race_bonus);
    if !(1..=i64::from(u32::MAX)).contains(&max_hp) {
        return Err(CharacterHpTableError::OutOfRange);
    }

    Ok(max_hp as u32)
}

pub fn level_up_max_hp(
    current_max_hp: u32,
    class_name: &str,
    con: u8,
) -> Result<u32, CharacterHpTableError> {
    let hp_gain = i64::from(roll_level_hp_delta(class_name, con)?);
    let new_max_hp = i64::from(current_max_hp) + hp_gain;

    if !(1..=i64::from(u32::MAX)).contains(&new_max_hp) {
        return Err(CharacterHpTableError::OutOfRange);
    }

    Ok(new_max_hp as u32)
}

/// Roll per-level HP delta with the same distribution used by level-up:
/// max(dHD roll, HD/2) + CON modifier.
pub fn roll_level_hp_delta(class_name: &str, con: u8) -> Result<i32, CharacterHpTableError> {
    let normalized_class_name = class_name.trim().to_lowercase();
    if normalized_class_name.is_empty() {
        return Err(CharacterHpTableError::InvalidInput);
    }
    if !(MIN_ATTRIBUTE..=MAX_ATTRIBUTE).contains(&con) {
        return Err(CharacterHpTableError::InvalidCon(con));
    }

    let class_hit_die = CLASS_HIT_DICE
        .iter()
        .find_map(|(name, sides)| (*name == normalized_class_name).then_some(*sides))
        .ok_or_else(|| CharacterHpTableError::UnknownClass(normalized_class_name.clone()))?;

    let con_mod = con_modifier(con);
    let mut rng = rand::thread_rng();
    let roll = rng.gen_range(1..=class_hit_die);
    let min_roll = class_hit_die / 2;
    let adjusted_roll = roll.max(min_roll);
    Ok(i32::from(adjusted_roll) + i32::from(con_mod))
}

fn con_modifier(con: u8) -> i16 {
    (i16::from(con) - 10) / 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_knight_level_one_max_hp_includes_con_mod() {
        let max_hp = level_one_max_hp("human", "knight", 14).unwrap();
        assert_eq!(max_hp, 14);
    }

    #[test]
    fn lookup_is_case_insensitive_and_trimmed() {
        let max_hp = level_one_max_hp(" Human ", " KNIGHT ", 14).unwrap();
        assert_eq!(max_hp, 14);
    }
}
