use crate::types::CharacterAttributes;
use onlinerpg_shared::{CharacterClass, Gender};
use rand::Rng;

const TARGET_ATTRIBUTE_TOTAL: i16 = 72;
const MIN_ATTRIBUTE: u8 = 3;
const MAX_ATTRIBUTE: u8 = 18;
const MIN_GUARD: i16 = 1;
const MAX_GUARD: i16 = 20;

pub fn roll_character_attributes(class: &CharacterClass, gender: Gender) -> CharacterAttributes {
    let mut rng = rand::thread_rng();
    let mut values = [0_u8; 6];
    for value in &mut values {
        *value = roll_4d6_drop_lowest(&mut rng);
    }

    apply_class_adjustments(&mut values, class, gender);
    rebalance_attributes_to_target(&mut values, TARGET_ATTRIBUTE_TOTAL);
    let guard = calculate_guard(values[1]);

    CharacterAttributes {
        r#str: values[0],
        dex: values[1],
        con: values[2],
        int: values[3],
        wis: values[4],
        cha: values[5],
        guard,
    }
}

fn apply_class_adjustments(values: &mut [u8; 6], class: &CharacterClass, gender: Gender) {
    let adjustments = class.stat_adjustments(gender);
    for (value, adj) in values.iter_mut().zip(adjustments.iter()) {
        let adjusted = (i16::from(*value) + i16::from(*adj))
            .clamp(i16::from(MIN_ATTRIBUTE), i16::from(MAX_ATTRIBUTE));
        *value = adjusted as u8;
    }
}

fn calculate_guard(dex: u8) -> u8 {
    // D20-style baseline where dexterity shifts defense around 10.
    let dex_mod = (i16::from(dex) - 10) / 2;
    (10 + dex_mod).clamp(MIN_GUARD, MAX_GUARD) as u8
}

fn roll_4d6_drop_lowest(rng: &mut impl Rng) -> u8 {
    let mut dice = [0_u8; 4];
    for die in &mut dice {
        *die = rng.gen_range(1..=6);
    }
    dice.sort_unstable();
    dice[1..].iter().sum()
}

fn rebalance_attributes_to_target(values: &mut [u8; 6], target_total: i16) {
    let mut total = values.iter().map(|&value| i16::from(value)).sum::<i16>();

    while total < target_total {
        let mut min_index: Option<usize> = None;
        for (index, &value) in values.iter().enumerate() {
            if value >= MAX_ATTRIBUTE {
                continue;
            }
            match min_index {
                None => min_index = Some(index),
                Some(current_min_index) if value < values[current_min_index] => {
                    min_index = Some(index)
                }
                _ => {}
            }
        }

        let Some(index) = min_index else {
            break;
        };

        values[index] = values[index].saturating_add(1);
        total += 1;
    }

    while total > target_total {
        let mut max_index: Option<usize> = None;
        for (index, &value) in values.iter().enumerate() {
            if value <= MIN_ATTRIBUTE {
                continue;
            }
            match max_index {
                None => max_index = Some(index),
                Some(current_max_index) if value > values[current_max_index] => {
                    max_index = Some(index)
                }
                _ => {}
            }
        }

        let Some(index) = max_index else {
            break;
        };

        values[index] = values[index].saturating_sub(1);
        total -= 1;
    }
}
