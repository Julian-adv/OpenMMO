use crate::types::CharacterAttributes;
use rand::Rng;

const TARGET_ATTRIBUTE_TOTAL: i16 = 72;
const MIN_ATTRIBUTE: u8 = 3;
const MAX_ATTRIBUTE: u8 = 18;

pub fn roll_character_attributes() -> CharacterAttributes {
    let mut rng = rand::thread_rng();
    let mut values = [0_u8; 6];
    for value in &mut values {
        *value = roll_4d6_drop_lowest(&mut rng);
    }

    rebalance_attributes_to_target(&mut values, TARGET_ATTRIBUTE_TOTAL);

    CharacterAttributes {
        r#str: values[0],
        dex: values[1],
        con: values[2],
        int: values[3],
        wis: values[4],
        cha: values[5],
    }
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
