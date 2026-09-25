use super::*;
use onlinerpg_shared::estate_storage::EstateChest;
use onlinerpg_shared::interest::{InterestChange, WorldEvent};

fn chest(x: f32, z: f32) -> EstateChest {
    EstateChest {
        id: 1,
        estate_id: 1,
        owner_id: 1,
        item_def_id: "storage_chest".into(),
        position: p(x, 0.0, z),
        rotation_deg: 0.0,
        floor_level: 0,
        overdue: false,
        revision: 0,
        text: None,
    }
}

fn state(id: u64) -> SharedState {
    let (mut state, _rx) = test_state();
    let mut player = test_player(5.5, 10.5);
    player.id = id.into();
    state.self_player_id = Some(player.id);
    state.self_player = Some(player);
    state
}

fn event(chest: &EstateChest, revision: u64, change: InterestChange) -> WorldEvent {
    let visible = matches!(change, InterestChange::Enter | InterestChange::Update);
    WorldEvent {
        subject: format!("chest:{}", chest.id),
        revision,
        change,
        messages: vec![ServerMessage::EstateChestVisibility {
            added: if visible { vec![chest.clone()] } else { vec![] },
            removed: if visible { vec![] } else { vec![chest.id] },
        }],
    }
}

fn world_update(state: &mut SharedState, epoch: &str, reset: bool, events: Vec<WorldEvent>) {
    state.push_event(ServerMessage::WorldUpdate {
        world_epoch: epoch.into(),
        generation: state.world_view.generation + u64::from(reset),
        sequence: if reset {
            1
        } else {
            state.world_view.sequence + 1
        },
        position: state.self_player.as_ref().unwrap().position,
        floor_level: 0,
        reset,
        ready: true,
        events,
    });
}

fn occupied(state: &SharedState, x: f32, z: f32, floor: u8) -> bool {
    !state.world_cache.read().unwrap().is_walkable(x, z, floor)
}

#[test]
fn estate_chest_visibility_routes_around_the_reported_storage_cluster() {
    let mut state = state(1);
    let origin = p(-1373.8975, 0.6809866, 4477.1035);
    let goal = p(-1382.1764, origin.y, 4473.573);
    state.self_player.as_mut().unwrap().position = origin;
    let chests: Vec<_> = [
        (-1374.5, 0.7125015, 4477.5),
        (-1374.5, 0.6499939, 4476.0),
        (-1373.0, 0.625, 4475.5),
        (-1373.0, 0.7000122, 4477.0),
    ]
    .into_iter()
    .enumerate()
    .map(|(id, (x, y, z))| EstateChest {
        id: id as i64,
        position: p(x, y, z),
        rotation_deg: 90.0,
        ..chest(x, z)
    })
    .collect();
    assert_eq!(state.find_path_to(goal.x, goal.z, 0).waypoints.len(), 1);
    assert_eq!(
        state.push_event(ServerMessage::EstateChestVisibility {
            added: chests.clone(),
            removed: vec![],
        }),
        EventUrgency::Noise
    );

    let route = state.find_path_to(goal.x, goal.z, 0);
    assert!(route.found);
    assert!(route.waypoints.len() > 1);
    assert!(
        !state
            .find_path_to(chests[0].position.x, chests[0].position.z, 0)
            .found
    );
    let world = state.world_cache.read().unwrap();
    assert!(pathfinding::is_movement_blocked(
        world.passability_cache(),
        origin.x,
        origin.z,
        goal.x,
        goal.z,
        0,
        None
    ));
    let (mut x, mut z) = (origin.x, origin.z);
    for waypoint in &route.waypoints {
        assert!(!pathfinding::is_movement_blocked(
            world.passability_cache(),
            x,
            z,
            waypoint.x,
            waypoint.z,
            waypoint.floor,
            None
        ));
        (x, z) = (waypoint.x, waypoint.z);
    }
    assert_eq!((x, z), (goal.x, goal.z));
    let revision = world.collision_revision;
    drop(world);

    state.push_event(ServerMessage::EstateChestVisibility {
        added: vec![],
        removed: chests.iter().map(|chest| chest.id).collect(),
    });
    assert!(state.world_cache.read().unwrap().collision_revision > revision);
    assert_eq!(state.find_path_to(goal.x, goal.z, 0).waypoints.len(), 1);
}

#[test]
fn estate_furniture_updates_rotation_position_height_and_floor() {
    let mut state = state(1);
    let mut bed = EstateChest {
        item_def_id: "furniture_bed".into(),
        ..chest(10.5, 10.5)
    };
    world_update(
        &mut state,
        "epoch",
        true,
        vec![event(&bed, 1, InterestChange::Enter)],
    );
    assert!(state.world_view.synchronized);
    assert!(occupied(&state, 10.5, 8.5, 0));
    bed.rotation_deg = 90.0;
    world_update(
        &mut state,
        "epoch",
        false,
        vec![event(&bed, 2, InterestChange::Update)],
    );
    assert!(!occupied(&state, 10.5, 8.5, 0));
    assert!(occupied(&state, 10.5, 10.5, 0));

    bed.position = p(43.5, 3.0, 10.5);
    bed.floor_level = 1;
    world_update(
        &mut state,
        "epoch",
        false,
        vec![event(&bed, 3, InterestChange::Update)],
    );
    assert!(!occupied(&state, 10.5, 10.5, 0));
    assert!(!occupied(&state, 43.5, 10.5, 0));
    assert!(occupied(&state, 43.5, 10.5, 1));
    let world = state.world_cache.read().unwrap();
    for (y, blocked) in [(3.05, true), (6.0, false)] {
        assert_eq!(
            pathfinding::is_movement_blocked(
                world.passability_cache(),
                38.5,
                10.5,
                48.5,
                10.5,
                1,
                Some(y)
            ),
            blocked
        );
    }
}

#[test]
fn estate_chest_views_share_updates_and_reject_stale_resurrection() {
    let mut a = state(1);
    let mut b = state(2);
    b.world_cache = a.world_cache.clone();
    let old = chest(10.5, 10.5);
    let moved = chest(20.5, 10.5);
    world_update(
        &mut a,
        "epoch",
        true,
        vec![event(&old, 1, InterestChange::Enter)],
    );
    world_update(
        &mut b,
        "epoch",
        true,
        vec![event(&moved, 2, InterestChange::Enter)],
    );
    let revision = a.world_cache.read().unwrap().collision_revision;
    world_update(
        &mut a,
        "epoch",
        false,
        vec![event(&old, 1, InterestChange::Update)],
    );
    assert!(a.world_view.synchronized);
    assert!(!occupied(&a, 10.5, 10.5, 0));
    assert!(occupied(&a, 20.5, 10.5, 0));
    world_update(
        &mut a,
        "epoch",
        false,
        vec![event(&moved, 2, InterestChange::Update)],
    );
    assert_eq!(a.world_cache.read().unwrap().collision_revision, revision);
    world_update(
        &mut a,
        "epoch",
        false,
        vec![event(&moved, 2, InterestChange::Leave)],
    );
    assert!(occupied(&b, 20.5, 10.5, 0));
    assert_eq!(a.world_cache.read().unwrap().collision_revision, revision);

    world_update(
        &mut a,
        "epoch",
        true,
        vec![event(&old, 1, InterestChange::Delete)],
    );
    assert!(occupied(&b, 20.5, 10.5, 0));
    world_update(
        &mut a,
        "epoch",
        false,
        vec![event(&moved, 2, InterestChange::Enter)],
    );
    world_update(
        &mut b,
        "epoch",
        false,
        vec![event(&moved, 3, InterestChange::Delete)],
    );
    assert!(!occupied(&a, 20.5, 10.5, 0));
    world_update(
        &mut a,
        "epoch",
        false,
        vec![event(&moved, 3, InterestChange::Update)],
    );
    assert!(!occupied(&a, 20.5, 10.5, 0));
    assert!(!a.world_view.synchronized);
    assert!(a
        .pending_commands
        .iter()
        .any(|cmd| matches!(cmd, ClientMessage::ResyncWorld)));
}

#[test]
fn estate_chest_snapshot_reset_disconnect_and_epoch_change_clear_collision() {
    let mut a = state(1);
    let mut b = state(2);
    b.world_cache = a.world_cache.clone();
    let chest = chest(10.5, 10.5);
    for state in [&mut a, &mut b] {
        world_update(
            state,
            "old",
            true,
            vec![event(&chest, 4, InterestChange::Enter)],
        );
    }
    world_update(&mut a, "old", true, vec![]);
    assert!(occupied(&b, 10.5, 10.5, 0));
    drop(b);
    assert!(!occupied(&a, 10.5, 10.5, 0));

    world_update(
        &mut a,
        "old",
        false,
        vec![event(&chest, 3, InterestChange::Enter)],
    );
    assert!(!occupied(&a, 10.5, 10.5, 0));
    assert!(!a.world_view.synchronized);
    world_update(
        &mut a,
        "old",
        true,
        vec![event(&chest, 4, InterestChange::Enter)],
    );
    assert!(occupied(&a, 10.5, 10.5, 0));

    world_update(&mut a, "new", true, vec![]);
    assert!(!occupied(&a, 10.5, 10.5, 0));
    world_update(
        &mut a,
        "new",
        false,
        vec![event(&chest, 1, InterestChange::Enter)],
    );
    assert!(occupied(&a, 10.5, 10.5, 0));
    a.push_event(ServerMessage::JoinSuccess {
        player: test_player(0.0, 0.0),
        is_admin: false,
    });
    assert!(!occupied(&a, 10.5, 10.5, 0));
}

#[test]
fn estate_chest_metadata_updates_do_not_invalidate_routes() {
    let mut state = state(1);
    let mut chest = chest(10.5, 10.5);
    world_update(
        &mut state,
        "epoch",
        true,
        vec![event(&chest, 1, InterestChange::Enter)],
    );
    let revision = state.world_cache.read().unwrap().collision_revision;
    chest.overdue = true;
    chest.revision += 1;
    chest.text = Some("storage".into());
    world_update(
        &mut state,
        "epoch",
        false,
        vec![event(&chest, 2, InterestChange::Update)],
    );
    assert_eq!(
        state.world_cache.read().unwrap().collision_revision,
        revision
    );
    assert!(occupied(&state, 10.5, 10.5, 0));
}
