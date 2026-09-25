use super::*;
use crate::pathfinding::{
    PassabilityCache, RuntimeFloorGrid, RuntimePassability, EDGE_E, EDGE_N, EDGE_S, EDGE_W,
};
use std::cell::{Cell, RefCell};

struct ReturnDoor {
    cache: RefCell<PassabilityCache>,
    queries: Cell<usize>,
}

impl ReturnDoor {
    fn new() -> Self {
        let mut cells = vec![0; 80 * 3];
        for x in 0..80 {
            cells[x] |= EDGE_N;
            cells[x + 160] |= EDGE_S;
        }
        for z in 0..3 {
            cells[z * 80] |= EDGE_W;
            cells[z * 80 + 79] |= EDGE_E;
        }
        Self {
            cache: RefCell::new(HashMap::from([(
                "corridor".into(),
                RuntimePassability {
                    house_origin_x: 0.0,
                    house_origin_z: 9.0,
                    min_x: 0.0,
                    max_x: 80.0,
                    min_z: 9.0,
                    max_z: 12.0,
                    floors: vec![RuntimeFloorGrid {
                        floor_level: 4,
                        origin_x: 0,
                        origin_z: 0,
                        width: 80,
                        depth: 3,
                        y_base: -4.0,
                        wall_height: 3.0,
                        cells,
                    }],
                    stairwells: vec![],
                    yields_to_trapped_mover: false,
                    allows_projectiles: false,
                    is_ground: false,
                },
            )])),
            queries: Cell::new(0),
        }
    }

    fn set_open(&self, open: bool) {
        let mut cache = self.cache.borrow_mut();
        let cells = &mut cache.get_mut("corridor").unwrap().floors[0].cells;
        for z in 0..3 {
            if open {
                cells[z * 80 + 59] &= !EDGE_E;
                cells[z * 80 + 60] &= !EDGE_W;
            } else {
                cells[z * 80 + 59] |= EDGE_E;
                cells[z * 80 + 60] |= EDGE_W;
            }
        }
    }
}

impl PathProvider for ReturnDoor {
    fn find_path(&self, sx: f32, sz: f32, sf: u8, gx: f32, gz: f32, gf: u8) -> PathResult {
        self.queries.set(self.queries.get() + 1);
        CachePathProvider {
            cache: &self.cache.borrow(),
        }
        .find_path(sx, sz, sf, gx, gz, gf)
    }

    fn attack_line_blocked(&self, fx: f32, fz: f32, tx: f32, tz: f32, floor: u8) -> bool {
        CachePathProvider {
            cache: &self.cache.borrow(),
        }
        .attack_line_blocked(fx, fz, tx, tz, floor)
    }
}

fn returning_brain() -> MonsterBrain {
    let mut brain = make_brain();
    brain.spawn_position = Position {
        x: 10.5,
        y: -4.0,
        z: 10.5,
    };
    brain.position = Position {
        x: 70.5,
        ..brain.spawn_position
    };
    brain.path_floor = 4;
    brain
}

fn return_combat_tree() -> BehaviorTree {
    BehaviorTree {
        description: None,
        root: BehaviorNode::Selector {
            children: vec![leash_tree().root, chase_attack_tree().root],
        },
    }
}

#[test]
fn closed_return_door_allows_damage_and_retaliation() {
    let door = ReturnDoor::new();
    let mut brain = returning_brain();
    let tree = return_combat_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    brain.tick_with_behavior_tree(16.0, &[], &[], &tree, &door, &mut rng);
    assert_eq!(brain.state(), AiState::Return);

    door.set_open(false);
    let path = brain.query_path(10.5, 10.5, &door);
    assert!(!path.found);
    assert!(
        !path.waypoints.is_empty(),
        "A* returns a partial route to the door"
    );

    brain.tick_with_behavior_tree(600.0, &[], &[], &tree, &door, &mut rng);
    assert_eq!(brain.state(), AiState::Idle);

    let players = attacker_at(brain.position.x + 1.0, brain.position.z);
    brain.handle_hit_with_behavior_tree(&players[0].id, true, 1);
    assert_eq!(brain.health, 9);
    let result = brain.tick_with_behavior_tree(800.0, &players, &[], &tree, &door, &mut rng);
    assert_eq!(brain.state(), AiState::Attack);
    assert!(result.iter().any(|c| matches!(c, AiCommand::Attack { .. })));
}

#[test]
fn return_repaths_after_a_position_correction() {
    let mut brain = returning_brain();
    let tree = leash_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    brain.tick_with_behavior_tree(16.0, &[], &[], &tree, &DirectPath, &mut rng);
    let kept = Position {
        x: 65.0,
        ..brain.position
    };
    brain.apply_authoritative_position(kept);

    brain.tick_with_behavior_tree(200.0, &[], &[], &tree, &DirectPath, &mut rng);
    assert_eq!(brain.state(), AiState::Return);
    assert!(!brain.waypoints.is_empty());
    assert!(brain.position.x < kept.x);
}

#[test]
fn return_repath_reports_a_turn_before_moving() {
    let mut brain = returning_brain();
    brain.spawn_position.z = 30.5;
    let tree = leash_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    brain.tick_with_behavior_tree(16.0, &[], &[], &tree, &DirectPath, &mut rng);
    let turn = brain.position;

    let result = brain.tick_with_behavior_tree(600.0, &[], &[], &tree, &BentPath, &mut rng);
    assert!(brain.position.x < turn.x);
    assert_eq!(brain.position.z, turn.z);
    assert!(result.iter().any(|c| matches!(
        c,
        AiCommand::Move { position, target_position, .. }
            if *position == turn && target_position.z == turn.z
    )));
}

#[test]
fn return_resumes_when_the_door_reopens() {
    let door = ReturnDoor::new();
    let mut brain = returning_brain();
    let tree = leash_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    door.set_open(false);
    brain.tick_with_behavior_tree(16.0, &[], &[], &tree, &door, &mut rng);
    assert_eq!(brain.state(), AiState::Idle);
    let stopped = brain.position;

    door.set_open(true);
    for _ in 0..10 {
        brain.tick_with_behavior_tree(100.0, &[], &[], &tree, &door, &mut rng);
    }
    assert_eq!(brain.state(), AiState::Return);
    assert!(brain.position.x < stopped.x);

    for _ in 0..70 {
        brain.tick_with_behavior_tree(1000.0, &[], &[], &tree, &door, &mut rng);
    }
    assert_eq!(brain.state(), AiState::Idle);
    assert!(brain.position.dist_xz_sq(&brain.spawn_position) <= DEFAULT_RETURN_ARRIVE_DIST.powi(2));
}

#[test]
fn blocked_return_retries_are_throttled() {
    let door = ReturnDoor::new();
    let mut brain = returning_brain();
    let tree = leash_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    door.set_open(false);
    for _ in 0..120 {
        brain.tick_with_behavior_tree(16.0, &[], &[], &tree, &door, &mut rng);
    }
    assert_eq!(brain.state(), AiState::Idle);
    assert!(
        (2..=4).contains(&door.queries.get()),
        "queries: {}",
        door.queries.get()
    );
}

#[test]
fn blocked_return_does_not_attack_through_the_door() {
    let door = ReturnDoor::new();
    let mut brain = returning_brain();
    brain.position.x = 60.5;
    brain.target_player_id = Some(1.into());
    let players = attacker_at(59.5, 10.5);
    let tree = return_combat_tree();
    let mut rng = SmallRng::seed_from_u64(42);
    door.set_open(false);
    for _ in 0..10 {
        let result = brain.tick_with_behavior_tree(200.0, &players, &[], &tree, &door, &mut rng);
        assert!(result
            .iter()
            .all(|c| !matches!(c, AiCommand::Attack { .. })));
        assert!(brain.position.x >= 60.0);
    }
    assert_eq!(brain.state(), AiState::Hold);
}
