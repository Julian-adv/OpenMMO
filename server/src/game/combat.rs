use rand::Rng;

pub struct AttackResult {
    pub hit: bool,
    pub roll: u8,
    pub damage: u32,
}

pub fn roll_attack() -> AttackResult {
    let mut rng = rand::thread_rng();

    // Roll d20: 1-20
    let roll = rng.gen_range(1..=20);
    let hit = roll > 10;
    let mut damage = 0;

    if hit {
        // Roll 1d6 damage: 1-6
        damage = rng.gen_range(1..=6) as u32;
    }

    AttackResult { hit, roll, damage }
}
