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
        let threshold = xp_for_level(next);
        if xp < threshold {
            break;
        }
        if threshold == u64::MAX {
            level = next;
            break;
        }
        level = next;
    }
    level
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeathPenaltyResult {
    pub old_level: u32,
    pub new_level: u32,
    pub old_xp: u64,
    pub new_xp: u64,
    pub xp_penalty: u64,
    pub leveled_down: bool,
}

/// Apply death penalty to cumulative XP.
/// - XP loss: 15% of current level band (minimum 1)
/// - If below current level start XP, lose exactly 1 level (unless level 1)
/// - On level down, guarantee at least 30% progress in lower level band
pub fn apply_death_penalty(old_xp: u64) -> DeathPenaltyResult {
    let old_level = level_from_xp(old_xp);
    let level_start_xp = xp_for_level(old_level);
    let next_level_xp = xp_for_level(old_level.saturating_add(1));
    let level_band = next_level_xp.saturating_sub(level_start_xp);
    let xp_penalty = (level_band.saturating_mul(15) / 100).max(1);

    let mut new_xp = old_xp.saturating_sub(xp_penalty);
    let mut new_level = old_level;
    let mut leveled_down = false;

    if old_level > 1 && new_xp < level_start_xp {
        leveled_down = true;
        new_level = old_level - 1;

        let lower_start_xp = xp_for_level(new_level);
        let lower_next_xp = xp_for_level(new_level.saturating_add(1));
        let lower_band = lower_next_xp.saturating_sub(lower_start_xp);
        let recovery_floor = lower_start_xp.saturating_add(lower_band.saturating_mul(30) / 100);
        new_xp = new_xp.max(recovery_floor);
    }

    DeathPenaltyResult {
        old_level,
        new_level,
        old_xp,
        new_xp,
        xp_penalty,
        leveled_down,
    }
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

    #[test]
    fn death_penalty_without_level_down() {
        // Level 2 band = 20, penalty = 3.
        let result = apply_death_penalty(30);
        assert_eq!(result.old_level, 2);
        assert_eq!(result.new_level, 2);
        assert_eq!(result.xp_penalty, 3);
        assert_eq!(result.new_xp, 27);
        assert!(!result.leveled_down);
    }

    #[test]
    fn death_penalty_with_single_level_down_and_recovery_floor() {
        // Old XP 20 (Lv2 start). Penalty 3 => 17, level-down to Lv1.
        // Lv1 band = 20, recovery floor = 6. max(17, 6) = 17.
        let result = apply_death_penalty(20);
        assert_eq!(result.old_level, 2);
        assert_eq!(result.new_level, 1);
        assert_eq!(result.new_xp, 17);
        assert!(result.leveled_down);
    }

    #[test]
    fn death_penalty_never_levels_down_from_level_one() {
        let result = apply_death_penalty(1);
        assert_eq!(result.old_level, 1);
        assert_eq!(result.new_level, 1);
        assert_eq!(result.new_xp, 0);
        assert!(!result.leveled_down);
    }
}
