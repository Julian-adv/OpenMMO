use rand::Rng;

pub struct AttackResult {
    pub hit: bool,
    pub roll: u8,
    pub damage: u32,
}

/// Parse dice notation like "1d6", "2d8" into (count, sides)
fn parse_damage_roll(damage_roll: &str) -> (u32, u32) {
    let parts: Vec<&str> = damage_roll.split('d').collect();
    if parts.len() == 2 {
        let count = parts[0].parse().unwrap_or(1);
        let sides = parts[1].parse().unwrap_or(6);
        (count, sides)
    } else {
        (1, 6) // default 1d6
    }
}

/// Roll dice notation like "6d4" and return the summed total (minimum 1).
/// Used for consumable healing where there's no attack roll, just the dice.
pub fn roll_dice(notation: &str) -> u32 {
    let (count, sides) = parse_damage_roll(notation);
    let mut rng = rand::thread_rng();
    let mut total: u32 = 0;
    for _ in 0..count {
        total += rng.gen_range(1..=sides);
    }
    total.max(1)
}

pub fn ability_modifier(score: u8) -> i32 {
    (i32::from(score) - 10).div_euclid(2)
}

pub fn level_attack_bonus(level: u32) -> i32 {
    (level / 2) as i32
}

pub fn monster_max_health_for_level(level: u8) -> u32 {
    // Average of level d8, rounded up: Lv3 -> 14, Lv4 -> 18.
    (u32::from(level).max(1) * 9).div_ceil(2)
}

pub fn monster_damage_roll_for_level(level: u8) -> &'static str {
    match level {
        0..=2 => "1d4",
        3..=4 => "1d6",
        5..=6 => "1d8",
        7..=8 => "2d6",
        9..=12 => "2d8",
        _ => "3d6",
    }
}

pub fn roll_attack(
    attack_bonus: i32,
    target_guard: i32,
    damage_roll: &str,
    damage_bonus: i32,
) -> AttackResult {
    roll_attack_with_extra_damage_roll(attack_bonus, target_guard, damage_roll, None, damage_bonus)
}

pub fn roll_attack_with_extra_damage_roll(
    attack_bonus: i32,
    target_guard: i32,
    damage_roll: &str,
    extra_damage_roll: Option<&str>,
    damage_bonus: i32,
) -> AttackResult {
    let mut rng = rand::thread_rng();

    let roll = rng.gen_range(1..=20);
    let hit = i32::from(roll) + attack_bonus > target_guard;
    let mut damage = 0;

    if hit {
        let mut total: i64 = i64::from(damage_bonus);
        for roll in std::iter::once(damage_roll).chain(extra_damage_roll.into_iter()) {
            let (count, sides) = parse_damage_roll(roll);
            for _ in 0..count {
                total += i64::from(rng.gen_range(1..=sides));
            }
        }
        // Hit always deals at least 1, even if bonus drives the roll non-positive.
        damage = total.max(1) as u32;
    }

    AttackResult { hit, roll, damage }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_damage_roll_is_added_on_hit() {
        let result = roll_attack_with_extra_damage_roll(20, 0, "1d1", Some("2d1"), 0);

        assert!(result.hit);
        assert_eq!(result.damage, 3);
    }

    #[test]
    fn extra_damage_roll_is_ignored_on_miss() {
        let result = roll_attack_with_extra_damage_roll(-20, 20, "1d1", Some("2d1"), 0);

        assert!(!result.hit);
        assert_eq!(result.damage, 0);
    }

    #[test]
    fn level_defaults_scale_monsters() {
        assert_eq!(level_attack_bonus(1), 0);
        assert_eq!(level_attack_bonus(4), 2);
        assert_eq!(monster_max_health_for_level(0), 5);
        assert_eq!(monster_max_health_for_level(3), 14);
        assert_eq!(monster_max_health_for_level(4), 18);
        assert_eq!(monster_damage_roll_for_level(3), "1d6");
        assert_eq!(monster_damage_roll_for_level(7), "2d6");
    }
}
