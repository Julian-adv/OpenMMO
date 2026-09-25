use crate::CharacterClass;

pub const MANA_REGEN_INTERVAL_MS: u64 = 16_000;
pub const MANA_REGEN_DELAY_MS: u64 = 10_000;

pub fn mana_die(class: &CharacterClass) -> u8 {
    match class {
        CharacterClass::Wizard | CharacterClass::Priest | CharacterClass::Healer => 8,
        CharacterClass::Ranger | CharacterClass::Monk | CharacterClass::Bard => 6,
        _ => 4,
    }
}

pub fn max_mana(class: &CharacterClass, wis: u8, level: u32) -> u32 {
    let mean_quarters = match mana_die(class) {
        8 => 21,
        6 => 16,
        _ => 11,
    };
    let wis_mod = (i32::from(wis) - 10) / 2;
    let growth_eighths = (mean_quarters + 4 * wis_mod).max(8) as u64;
    let starting = 10 + u64::from(wis / 2);
    (starting + u64::from(level.saturating_sub(1)) * growth_eighths / 8).min(u64::from(u32::MAX))
        as u32
}

pub fn mana_regen_amount(wis: u8, level: u32) -> u32 {
    let base = 1 + i64::from(level / 5) + (i64::from(wis) - 10) / 2;
    ((3 * base + 2) / 5).max(1) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn growth_matches_design_and_keeps_fractional_gains() {
        for (class, expected) in [
            (CharacterClass::Wizard, 194),
            (CharacterClass::Priest, 194),
            (CharacterClass::Healer, 194),
            (CharacterClass::Ranger, 164),
            (CharacterClass::Monk, 164),
            (CharacterClass::Bard, 164),
            (CharacterClass::Knight, 133),
            (CharacterClass::Rogue, 133),
        ] {
            assert_eq!(max_mana(&class, 14, 1), 17);
            assert_eq!(max_mana(&class, 14, 50), expected);
        }
        assert_eq!(
            (1..=3)
                .map(|level| max_mana(&CharacterClass::Wizard, 14, level))
                .collect::<Vec<_>>(),
            [17, 20, 24]
        );
        assert_eq!(max_mana(&CharacterClass::Knight, 3, 50), 60);
        assert_eq!(max_mana(&CharacterClass::Wizard, 16, 5), 34);
        assert_eq!(max_mana(&CharacterClass::Knight, 9, 2), 15);
        assert_eq!(max_mana(&CharacterClass::Wizard, 255, u32::MAX), u32::MAX);
    }

    #[test]
    fn regeneration_matches_design() {
        for (wis, first, last) in [(10, 1, 7), (14, 2, 8), (18, 3, 9)] {
            assert_eq!(mana_regen_amount(wis, 1), first);
            assert_eq!(mana_regen_amount(wis, 50), last);
        }
        assert_eq!(mana_regen_amount(3, 1), 1);
        assert_eq!(mana_regen_amount(16, 5), 3);
    }
}
