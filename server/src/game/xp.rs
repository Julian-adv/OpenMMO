/// Calculate XP awarded for killing a monster.
/// Formula: 1 + level² + guard_bonus
/// guard_bonus: 0 if guard < 8, +5 at 8, +6 at 9, 7 + 2*(guard-10) for guard >= 10
pub fn monster_xp(level: u8, guard: u8) -> u32 {
    let base = 1u32 + (level as u32) * (level as u32);
    let guard_bonus = if guard >= 10 {
        7u32 + 2 * (guard as u32 - 10)
    } else if guard == 9 {
        6
    } else if guard == 8 {
        5
    } else {
        0
    };
    base + guard_bonus
}

/// Minimum cumulative XP required to reach the given level.
/// Level 1: 0, Level n (n>=2): 20 * 2^(n-2)
/// Saturates at u64::MAX for astronomically high levels (~62+).
pub fn xp_for_level(level: u32) -> u64 {
    if level <= 1 {
        return 0;
    }
    let shift = level - 2;
    if shift >= 64 {
        return u64::MAX;
    }
    20u64.saturating_mul(1u64 << shift)
}

/// Determine current level from cumulative XP. No upper bound.
pub fn level_from_xp(xp: u64) -> u32 {
    let mut level = 1u32;
    loop {
        let next = match level.checked_add(1) {
            Some(n) => n,
            None => break,
        };
        if xp < xp_for_level(next) {
            break;
        }
        level = next;
    }
    level
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monster_xp_no_guard_bonus() {
        // level 3, guard 5: 1 + 9 + 0 = 10
        assert_eq!(monster_xp(3, 5), 10);
    }

    #[test]
    fn monster_xp_guard_8() {
        // level 3, guard 8: 1 + 9 + 5 = 15
        assert_eq!(monster_xp(3, 8), 15);
    }

    #[test]
    fn monster_xp_guard_10() {
        // level 5, guard 10: 1 + 25 + 7 = 33
        assert_eq!(monster_xp(5, 10), 33);
    }

    #[test]
    fn monster_xp_guard_13() {
        // level 8, guard 13: 1 + 64 + (7 + 2*3) = 1 + 64 + 13 = 78
        assert_eq!(monster_xp(8, 13), 78);
    }

    #[test]
    fn xp_for_level_thresholds() {
        assert_eq!(xp_for_level(1), 0);
        assert_eq!(xp_for_level(2), 20);
        assert_eq!(xp_for_level(3), 40);
        assert_eq!(xp_for_level(10), 5120);
        assert_eq!(xp_for_level(11), 10240);
    }

    #[test]
    fn level_from_xp_basic() {
        assert_eq!(level_from_xp(0), 1);
        assert_eq!(level_from_xp(19), 1);
        assert_eq!(level_from_xp(20), 2);
        assert_eq!(level_from_xp(39), 2);
        assert_eq!(level_from_xp(40), 3);
        assert_eq!(level_from_xp(5120), 10);
        assert_eq!(level_from_xp(10240), 11);
    }

    #[test]
    fn xp_for_level_no_overflow() {
        // level 61: last level where 20 * 2^(n-2) fits in u64
        assert!(xp_for_level(61) < u64::MAX);
        // level 62+: saturates at u64::MAX
        assert_eq!(xp_for_level(62), u64::MAX);
        assert_eq!(xp_for_level(100), u64::MAX);
        assert_eq!(xp_for_level(u32::MAX), u64::MAX);
    }

    #[test]
    fn level_from_xp_max_does_not_panic() {
        // Should terminate without panic at extreme values
        let _ = level_from_xp(u64::MAX);
        let _ = level_from_xp(u64::MAX - 1);
    }
}
