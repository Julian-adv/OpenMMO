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

pub fn ability_modifier(score: u8) -> i32 {
    (i32::from(score) - 10).div_euclid(2)
}

pub fn roll_attack(hit_threshold: u8, damage_roll: &str, damage_bonus: i32) -> AttackResult {
    let mut rng = rand::thread_rng();

    let roll = rng.gen_range(1..=20);
    let hit = roll > hit_threshold;
    let mut damage = 0;

    if hit {
        let (count, sides) = parse_damage_roll(damage_roll);
        let mut total: i64 = i64::from(damage_bonus);
        for _ in 0..count {
            total += i64::from(rng.gen_range(1..=sides));
        }
        // Hit always deals at least 1, even if bonus drives the roll non-positive.
        damage = total.max(1) as u32;
    }

    AttackResult { hit, roll, damage }
}
