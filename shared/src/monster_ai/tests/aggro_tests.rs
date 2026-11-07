use super::*;
use rand::rngs::mock::StepRng;

fn always_distract() -> StepRng {
    StepRng::new(0, 0)
}

fn never_distract() -> StepRng {
    StepRng::new(u64::MAX, 0)
}

fn terraclash() -> MonsterBrain {
    let mut brain = MonsterBrain::new(
        "terraclash_1".into(),
        "terraclash",
        "brave".into(),
        Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        80_000,
        80_000,
        0.0,
        0.0,
        3.0,
        40.0,
        3000.0,
    );
    brain.ranged_range = 11.0;
    // Only plain swings pick targets; keep the skills out of these fights.
    brain.skills = None;
    brain
}

fn player(id: u64, z: f32, drank_recently: bool) -> NearbyPlayer {
    NearbyPlayer {
        id: id.into(),
        position: Position { x: 0.0, y: 0.0, z },
        health: 100,
        drank_recently,
        petrified: false,
    }
}

/// Ticks until the next plain swing and returns its target and whether it
/// was a ground strike.
fn next_swing(
    brain: &mut MonsterBrain,
    players: &[NearbyPlayer],
    rng: &mut StepRng,
) -> Option<(PlayerId, bool)> {
    for _ in 0..40 {
        let commands =
            brain.tick_with_behavior_tree(200.0, players, &[], &attack_tree(), &DirectPath, rng);
        if let Some(swing) = commands.iter().find_map(|c| match c {
            AiCommand::Attack {
                target_player_id,
                reach,
            } => Some((*target_player_id, matches!(reach, SwingReach::Ground(_)))),
            _ => None,
        }) {
            return Some(swing);
        }
    }
    None
}

#[test]
fn the_top_damage_dealer_in_reach_draws_the_swings() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 6.0, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 10);
    brain.handle_hit_with_behavior_tree(&2.into(), true, 50);
    assert_eq!(
        brain.target_player_id(),
        Some(1.into()),
        "the first blow aggros"
    );

    let mut rng = never_distract();
    assert_eq!(
        next_swing(&mut brain, &players, &mut rng),
        Some((2.into(), true))
    );
    brain.handle_hit_with_behavior_tree(&1.into(), true, 5);
    assert_eq!(
        next_swing(&mut brain, &players, &mut rng),
        Some((2.into(), true)),
        "a lesser hit does not pull the boss off its top threat"
    );
}

#[test]
fn it_waits_to_be_provoked() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false)];
    assert_eq!(
        next_swing(&mut brain, &players, &mut never_distract()),
        None
    );
}

#[test]
fn a_distraction_strikes_a_potion_drinker_in_reach() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 5.0, true)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 100);
    brain.handle_hit_with_behavior_tree(&2.into(), true, 1);
    assert_eq!(
        next_swing(&mut brain, &players, &mut always_distract()),
        Some((2.into(), true))
    );
}

#[test]
fn a_distraction_reaches_a_far_idler_once_then_returns_to_the_top_threat() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 15.0, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 100);
    assert_eq!(
        next_swing(&mut brain, &players, &mut always_distract()),
        Some((2.into(), true)),
        "never swung at the boss, so idle"
    );
    assert_eq!(
        next_swing(&mut brain, &players, &mut never_distract()),
        Some((1.into(), false))
    );
}

#[test]
fn a_distraction_strikes_an_idler_standing_in_reach() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 2.5, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 100);
    assert_eq!(
        next_swing(&mut brain, &players, &mut always_distract()),
        Some((2.into(), false))
    );
}

#[test]
fn a_far_player_who_keeps_swinging_is_not_idle() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 15.0, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 100);
    brain.handle_hit_with_behavior_tree(&2.into(), false, 0);
    assert_eq!(
        next_swing(&mut brain, &players, &mut always_distract()),
        Some((1.into(), false))
    );
}

#[test]
fn an_unreachable_target_gives_way_to_one_in_reach() {
    let mut brain = terraclash();
    let players = [player(1, 30.0, false), player(2, 2.0, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), false, 0);
    assert_eq!(
        next_swing(&mut brain, &players, &mut never_distract()),
        Some((2.into(), false))
    );
}

#[test]
fn no_one_in_reach_means_no_swing_and_no_reroll() {
    let mut brain = terraclash();
    let players = [player(1, 30.0, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), false, 0);
    assert_eq!(
        next_swing(&mut brain, &players, &mut always_distract()),
        None
    );
    assert_eq!(brain.target_player_id(), None);
    assert!(!brain.aggro.as_ref().unwrap().roll_due);
}

#[test]
fn an_old_burst_of_damage_fades_behind_steady_hits() {
    let mut brain = terraclash();
    let players = [player(1, 2.0, false), player(2, 2.5, false)];
    brain.handle_hit_with_behavior_tree(&1.into(), true, 1000);
    let mut rng = never_distract();
    assert_eq!(
        next_swing(&mut brain, &players, &mut rng),
        Some((1.into(), false))
    );
    // 900 in all from player 2, short of the 1000 burst unless it fades.
    for _ in 0..30 {
        brain.handle_hit_with_behavior_tree(&2.into(), true, 30);
        for _ in 0..15 {
            brain.tick_with_behavior_tree(
                200.0,
                &players,
                &[],
                &attack_tree(),
                &DirectPath,
                &mut rng,
            );
        }
    }
    assert_eq!(
        next_swing(&mut brain, &players, &mut rng),
        Some((2.into(), false))
    );
}
