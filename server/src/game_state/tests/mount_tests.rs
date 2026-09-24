use super::*;
use onlinerpg_shared::mount::MountKind;

async fn rider(game: &GameState) -> PlayerId {
    let player = make_player("Rider", 0.0, 0.0);
    let id = player.id;
    game.add_player(player).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "horse_reins", 1)],
            ..Default::default()
        },
    );
    id
}

#[tokio::test]
async fn horse_reins_toggle_without_consumption_and_broadcast() {
    let game = make_test_game_state("horse_toggle");
    let id = rider(&game).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMountChanged {
            mount: Some(MountKind::Horse),
            ..
        }
    )));
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
    assert_eq!(
        game.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        1
    );
}

#[tokio::test]
async fn horse_mount_rejects_defeat_dungeons_combat_and_water() {
    let game = make_test_game_state("horse_guards");
    let id = rider(&game).await;
    for state in 0..4 {
        {
            let mut players = game.players.write().await;
            let p = players.get_mut(&id).unwrap();
            p.health = if state == 0 { 0 } else { 10 };
            p.floor_level = if state == 1 { -1 } else { 0 };
            p.last_combat_at = if state == 2 { GameState::now_ms() } else { 0 };
            p.position.x = if state == 3 { -50.0 } else { 0.0 };
        }
        game.use_item(&id, 1).await;
        assert!(!game.players.read().await[&id].is_mounted());
    }
    assert_eq!(
        game.get_player_inventory(&id).await.unwrap().bag[0].quantity,
        1
    );
}

#[tokio::test]
async fn horse_dismounts_when_combat_or_interaction_starts() {
    let game = make_test_game_state("horse_dismount");
    let id = rider(&game).await;
    game.use_item(&id, 1).await;
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.advance_test_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    game.use_item(&id, 1).await;
    game.players.write().await.get_mut(&id).unwrap().object_type = Some("sit".into());
    game.advance_test_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

#[tokio::test]
async fn horse_cannot_mount_indoors_and_dismounts_on_entry() {
    use onlinerpg_shared::pathfinding::RuntimePassability;

    let game = make_test_game_state("horse_indoors");
    let id = rider(&game).await;
    game.passability_write().insert(
        "house:test".into(),
        RuntimePassability {
            house_origin_x: 10.0,
            house_origin_z: 0.0,
            min_x: 10.0,
            max_x: 14.0,
            min_z: -2.0,
            max_z: 2.0,
            floors: vec![],
            stairwells: vec![],
            yields_to_trapped_mover: false,
            allows_projectiles: false,
            is_ground: true,
        },
    );
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 12.0;
    game.advance_test_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

async fn boater(game: &GameState) -> PlayerId {
    let mut player = make_player("Boater", -50.0, 0.0);
    player.position.x = -50.0;
    let id = player.id;
    game.add_player(player).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "rowboat", 1)],
            ..Default::default()
        },
    );
    id
}

#[tokio::test]
async fn rowboat_launches_on_water_and_not_on_land() {
    let game = make_test_game_state("rowboat_water");
    let id = boater(&game).await;
    let mut rx = game.register_direct_channel(&id).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMountChanged {
            mount: Some(MountKind::Rowboat),
            ..
        }
    )));

    game.use_item(&id, 1).await;
    assert!(!game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 0.0;
    game.use_item(&id, 1).await;
    assert!(
        !game.players.read().await[&id].is_mounted(),
        "dry land floats no boat"
    );
}

/// The horse throws its rider in combat; a boat has nowhere to throw them.
#[tokio::test]
async fn rowboat_stays_under_its_rider_in_combat() {
    let game = make_test_game_state("rowboat_combat");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    game.advance_test_movement(0.2).await;
    assert!(game.players.read().await[&id].is_mounted());
}

/// Rowing into the shallows grounds the hull, the mirror of a horse walking
/// into deep water.
#[tokio::test]
async fn rowboat_grounds_itself_in_the_shallows() {
    let game = make_test_game_state("rowboat_ground");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());
    game.players.write().await.get_mut(&id).unwrap().position.x = 0.0;
    game.advance_test_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

/// The client mirrors this in `floatSurfaceY`; disagree and its prediction
/// fights the server's snap-backs.
#[tokio::test]
async fn a_floating_mount_uses_the_water_surface_a_horse_the_bed() {
    let game = make_test_game_state("rowboat_float");
    let at = Position {
        x: -50.0,
        y: 0.0,
        z: 0.0,
    };
    let (bed, depth) = game.ground_and_depth_at(at.x, at.z).await.unwrap();
    assert!(depth > 0.0, "test water expected at x=-50");

    let afloat = game
        .surface_ground_y(0, &at, at.y, Some(MountKind::Rowboat))
        .await;
    assert!(
        (afloat - (bed + depth)).abs() < 1e-3,
        "boat should sit on the surface, got {afloat} for bed {bed} + depth {depth}"
    );
    assert!(afloat > bed, "the surface is above the bed it covers");

    for mount in [None, Some(MountKind::Horse)] {
        let grounded = game.surface_ground_y(0, &at, at.y, mount).await;
        assert!(
            (grounded - bed).abs() < 1e-3,
            "{mount:?} should stand on the bed, got {grounded}"
        );
    }
}

/// The boat is the item, so losing it ends the ride — combat does not.
#[tokio::test]
async fn losing_the_boat_dismounts() {
    let game = make_test_game_state("rowboat_lost");
    let id = boater(&game).await;
    game.use_item(&id, 1).await;
    assert!(game.players.read().await[&id].is_mounted());

    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .clear();
    game.advance_test_movement(0.2).await;
    assert!(!game.players.read().await[&id].is_mounted());
}

/// A new variant with no CSV row would otherwise only fail on use.
#[test]
fn every_mount_kind_has_an_item_that_boards_it() {
    for kind in [MountKind::Horse, MountKind::Rowboat] {
        let def = crate::item_defs::item_defs()
            .get(kind.item_id())
            .unwrap_or_else(|| panic!("{kind:?} names a missing item {}", kind.item_id()));
        match def.use_effect() {
            Some(crate::item_defs::UseEffect::ToggleMount(mapped)) => {
                assert_eq!(mapped, kind, "{} boards the wrong mount", kind.item_id())
            }
            _ => panic!("{} does not board anything", kind.item_id()),
        }
    }
}

/// Combat gates every mounted-movement handler, which suits a horse that is
/// about to be unseated. A boat keeps its rider, so it must keep steering.
#[tokio::test]
async fn boarding_lifts_the_rider_to_the_surface() {
    let game = make_test_game_state("rowboat_lift");
    let id = boater(&game).await;
    let watcher = make_player("Watcher", -50.0, 3.0);
    let watcher_id = watcher.id;
    game.add_player(watcher).await;
    let mut rx = game.register_direct_channel(&watcher_id).await;

    // A wader arrives standing on the bed, which is where the test player
    // must start too — it is minted at sea level, already at the surface.
    let (bed, depth) = game.ground_and_depth_at(-50.0, 0.0).await.unwrap();
    assert!(depth > 0.0);
    game.players.write().await.get_mut(&id).unwrap().position.y = bed;

    game.use_item(&id, 1).await;
    let afloat = game.players.read().await[&id].position.y;
    assert!(
        (afloat - (bed + depth)).abs() < 1e-3,
        "boarding: {bed} -> {afloat}, surface {}",
        bed + depth
    );
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerMoved { player_id, position, .. }
            if *player_id == id && (position.y - afloat).abs() < 1e-3
    )));

    game.use_item(&id, 1).await;
    let ashore = game.players.read().await[&id].position.y;
    assert!((ashore - bed).abs() < 1e-3, "leaving sets them back down");
}
