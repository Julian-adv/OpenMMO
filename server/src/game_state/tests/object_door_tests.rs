use super::*;
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::pathfinding::is_movement_blocked;

const HALL_ID: u32 = 121;

/// Entrance faces +z: the doorway spans x 98.1..101.9 at z 108.5.
async fn hall(test: &str) -> GameState {
    let game = make_test_game_state(test);
    let (rx, rz) = passability::region_of(100.0, 100.0);
    game.sync_region_furniture(
        rx,
        rz,
        &[FurniturePlacement {
            id: HALL_ID,
            type_id: "union_hall".into(),
            x: 100.0,
            y: 0.0,
            z: 100.0,
            rotation_deg: 0.0,
            floor_level: 0,
            door_open: false,
        }],
    );
    game
}

async fn toggle(game: &GameState, name: &str, object_id: u32) -> Option<bool> {
    let (rx, rz) = passability::region_of(100.0, 100.0);
    let door = DoorId::Object { rx, rz, object_id };
    game.toggle_door(&pid(name), &door.to_string()).await
}

fn through_portal_blocked(game: &GameState) -> bool {
    let cache = game.passability_read();
    is_movement_blocked(&cache, 100.0, 107.5, 100.0, 109.5, 0, Some(0.0))
        || is_movement_blocked(&cache, 100.0, 109.5, 100.0, 107.5, 0, Some(0.0))
}

#[tokio::test]
async fn object_door_opens_and_closes_the_portal() {
    let game = hall("object_door_toggle").await;
    game.add_player(make_player("usher", 100.0, 110.0)).await;
    assert!(through_portal_blocked(&game));

    assert_eq!(toggle(&game, "usher", HALL_ID).await, Some(true));
    assert!(!through_portal_blocked(&game));

    assert_eq!(toggle(&game, "usher", HALL_ID).await, Some(false));
    assert!(through_portal_blocked(&game));
}

#[tokio::test]
async fn object_door_refuses_a_player_out_of_reach() {
    let game = hall("object_door_reach").await;
    game.add_player(make_player("far", 100.0, 112.0)).await;
    game.add_player(make_player("aside", 105.0, 109.0)).await;
    assert_eq!(toggle(&game, "far", HALL_ID).await, None);
    assert_eq!(toggle(&game, "aside", HALL_ID).await, None);
    assert_eq!(toggle(&game, "far", HALL_ID + 1).await, None);
    assert!(through_portal_blocked(&game));
}

#[tokio::test]
async fn object_door_stays_open_while_someone_stands_in_it() {
    let game = hall("object_door_occupied").await;
    game.add_player(make_player("usher", 100.0, 110.0)).await;
    assert_eq!(toggle(&game, "usher", HALL_ID).await, Some(true));
    game.add_player(make_player("guest", 99.0, 108.5)).await;
    assert_eq!(toggle(&game, "usher", HALL_ID).await, Some(true));
    assert!(!through_portal_blocked(&game));
}

#[tokio::test]
async fn editor_save_keeps_an_open_door_and_drops_a_removed_one() {
    let game = hall("object_door_editor").await;
    game.add_player(make_player("usher", 100.0, 110.0)).await;
    toggle(&game, "usher", HALL_ID).await;
    let (rx, rz) = passability::region_of(100.0, 100.0);
    let placement = FurniturePlacement {
        id: HALL_ID,
        type_id: "union_hall".into(),
        x: 100.0,
        y: 0.0,
        z: 100.0,
        rotation_deg: 0.0,
        floor_level: 0,
        door_open: false,
    };
    game.sync_region_furniture(rx, rz, std::slice::from_ref(&placement));
    assert!(!through_portal_blocked(&game));

    game.sync_region_furniture(rx, rz, &[]);
    game.sync_region_furniture(rx, rz, &[placement]);
    assert!(through_portal_blocked(&game));
}
