use super::*;

fn ambush_tree() -> BehaviorTree {
    let trees =
        load_behavior_trees(include_str!("../../../../data-src/behavior_trees.json")).unwrap();
    trees[AMBUSH_BEHAVIOR].clone()
}

fn statue() -> MonsterBrain {
    let mut brain = make_brain();
    brain.set_ambush(3.0, true);
    brain
}

fn dormant_changes(commands: &[AiCommand]) -> Vec<bool> {
    commands
        .iter()
        .filter_map(|c| match c {
            AiCommand::Dormant { dormant } => Some(*dormant),
            _ => None,
        })
        .collect()
}

fn tick(brain: &mut MonsterBrain, ms: f32, players: &[NearbyPlayer]) -> Vec<AiCommand> {
    let mut rng = SmallRng::seed_from_u64(7);
    brain.tick_with_behavior_tree(ms, players, &[], &ambush_tree(), &DirectPath, &mut rng)
}

#[test]
fn a_statue_stands_still_until_a_player_comes_within_reach() {
    let mut brain = statue();
    for _ in 0..20 {
        assert!(tick(&mut brain, 200.0, &attacker_at(14.0, 10.0)).is_empty());
    }
    assert!(brain.is_dormant());

    let woke = tick(&mut brain, 200.0, &attacker_at(12.5, 10.0));
    assert_eq!(dormant_changes(&woke), vec![false]);
    assert!(!brain.is_dormant());
    assert_eq!(brain.target_player_id(), Some(1.into()));
}

#[test]
fn stone_players_do_not_wake_it() {
    let mut brain = statue();
    let mut players = attacker_at(11.0, 10.0);
    players[0].petrified = true;
    assert!(tick(&mut brain, 200.0, &players).is_empty());
    assert!(brain.is_dormant());
}

#[test]
fn it_holds_while_waking_then_attacks() {
    let mut brain = statue();
    let players = attacker_at(11.0, 10.0);
    tick(&mut brain, 200.0, &players);
    let mut waited = 0.0;
    loop {
        let commands = tick(&mut brain, 200.0, &players);
        waited += 200.0;
        if commands
            .iter()
            .any(|c| matches!(c, AiCommand::Attack { .. }))
        {
            break;
        }
        assert!(waited < 2_000.0, "never attacked");
    }
    assert!(waited >= WAKE_MS);
}

#[test]
fn a_blow_wakes_it_from_any_distance() {
    let mut brain = statue();
    let commands = brain.handle_hit_with_behavior_tree(&1.into(), true, 3);
    assert_eq!(dormant_changes(&commands), vec![false]);
    assert!(!brain.is_dormant());
    assert_eq!(brain.health, 7);
}

#[test]
fn it_sleeps_again_after_resting_at_home() {
    let mut brain = statue();
    tick(&mut brain, 200.0, &attacker_at(11.0, 10.0));
    let mut slept = Vec::new();
    for _ in 0..200 {
        slept.extend(dormant_changes(&tick(&mut brain, 200.0, &[])));
        if brain.is_dormant() {
            break;
        }
    }
    assert_eq!(slept, vec![true]);
    assert!(brain.target_player_id().is_none());
}

#[test]
fn a_plain_brain_never_sleeps() {
    let mut brain = make_brain();
    brain.set_ambush(0.0, true);
    assert!(!brain.is_dormant());
}
