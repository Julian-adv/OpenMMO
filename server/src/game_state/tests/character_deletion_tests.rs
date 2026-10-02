use super::*;
use std::time::Duration;

#[tokio::test]
async fn character_deletion_waits_24_hours_survives_restart_and_can_be_cancelled() {
    let game = make_test_game_state("character_deletion_delay");
    let (auth, path) = make_test_auth_with_path("character_deletion_delay");
    let account = auth.login_google("delete-delay").unwrap();
    let character = create_test_character(&auth, &account, "Waiting");
    let stranger = auth.login_google("delete-stranger").unwrap();
    let before = crate::auth::unix_now();
    assert!(game
        .change_character_deletion(&auth, &stranger, character.id, false)
        .await
        .is_err());
    let due = game
        .change_character_deletion(&auth, &account, character.id, false)
        .await
        .unwrap()
        .unwrap();
    assert!((before + 86400..=crate::auth::unix_now() + 86400).contains(&due));
    assert_eq!(
        game.change_character_deletion(&auth, &account, character.id, false)
            .await
            .unwrap(),
        Some(due)
    );
    game.tick_character_deletions(&auth).await;
    assert!(!game
        .delete_character_if_inactive(&auth, &account, character.id)
        .await
        .unwrap());
    let reopened = crate::auth::AuthService::new(path).unwrap();
    assert_eq!(
        reopened.list_characters_with_equipment(&account).unwrap()[0]
            .record
            .deletion_due_at,
        Some(due)
    );
    assert_eq!(
        game.change_character_deletion(&reopened, &account, character.id, true)
            .await
            .unwrap(),
        None
    );
    game.tick_character_deletions(&reopened).await;
    assert!(reopened
        .get_character_for_account(&account, character.id)
        .unwrap()
        .deletion_due_at
        .is_none());
    reopened.make_character_deletion_due(character.id);
    assert!(game
        .change_character_deletion(&reopened, &account, character.id, true)
        .await
        .is_err());
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    game.register_account_session(&account, tx, &reopened).await;
    game.tick_character_deletions(&reopened).await;
    assert!(matches!(
        reopened.get_character_for_account(&account, character.id),
        Err(crate::auth::AuthError::CharacterNotFound)
    ));
    assert!(
        matches!(rx.try_recv(), Ok(KickNotice { keep_open: true, message: ServerMessage::CharacterDeleted { character_id }, .. }) if character_id == character.id)
    );
}

#[tokio::test]
async fn active_character_cannot_be_scheduled_for_deletion() {
    let game = make_test_game_state("character_deletion_active");
    let auth = make_test_auth("character_deletion_active");
    let account = auth.login_google("delete-active").unwrap();
    let character = create_test_character(&auth, &account, "Active");
    game.register_player_character(&pid("Active"), character.id, 0, attrs_with_cha(12), 0, None)
        .await;
    assert!(game
        .change_character_deletion(&auth, &account, character.id, false)
        .await
        .is_err());
    assert!(auth
        .get_character_for_account(&account, character.id)
        .unwrap()
        .deletion_due_at
        .is_none());
}

async fn furnished_estate(
    label: &str,
) -> (
    GameState,
    crate::auth::AuthService,
    std::path::PathBuf,
    String,
    i64,
    String,
    i64,
) {
    let game = make_test_game_state(label);
    let (auth, path) = make_test_auth_with_path(label);
    let account = auth.login_google(label).unwrap();
    let character = estate_owner(
        &game,
        &auth,
        &account,
        "Builder",
        pos3(5.0, 5.0, 5.0),
        vec![bag_item(1, "land_deed", 1)],
    )
    .await;
    claim_plot_at(&game, &auth, "Builder", 5.0, 5.0).await;
    game.inventories
        .write()
        .await
        .get_mut(&pid("Builder"))
        .unwrap()
        .bag = vec![
        bag_item(2, "scroll_of_small_house", 1),
        bag_item(3, "storage_chest", 1),
        bag_item(4, onlinerpg_shared::landscaping::TOOLBOX_ITEM, 1),
        bag_item(5, "wooden_fence", 1),
    ];
    game.place_house(&pid("Builder"), 2, pos3(5.0, 5.0, 5.0), 0, &auth)
        .await;
    let house = game.housing_io.read_all_houses().await.unwrap().remove(0);
    game.players
        .write()
        .await
        .get_mut(&pid("Builder"))
        .unwrap()
        .position = pos3(20.5, 5.05, 19.5);
    game.place_estate_chest(&pid("Builder"), 3, pos3(20.5, 5.0, 20.5), 0.0, 0, &auth)
        .await;
    let chest = auth.load_estate_chests().unwrap().remove(0);
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute("INSERT INTO estate_chest_items(chest_id,item_def_id,quantity,enchant) VALUES (?1,'apple',3,0)", [chest.id]).unwrap();
    game.edit_fence(
        &pid("Builder"),
        onlinerpg_shared::fence::FenceEdge {
            x: 21,
            z: 21,
            axis: onlinerpg_shared::fence::FenceAxis::X,
        },
        true,
        &auth,
        false,
    )
    .await;
    assert_eq!(auth.load_fences().unwrap().len(), 1);
    (game, auth, path, account, character, house.id, chest.id)
}

#[tokio::test]
async fn sixth_missed_tax_removes_estate_buildings_storage_and_runtime_state() {
    let (game, auth, path, account, character, house, chest) =
        furnished_estate("foreclosure_cleanup").await;
    let conn = rusqlite::Connection::open(&path).unwrap();
    let month: i64 = conn
        .query_row("SELECT month FROM land_tax_periods", [], |row| row.get(0))
        .unwrap();
    conn.execute("UPDATE land_estates SET free_months=0", [])
        .unwrap();
    auth.collect_land_taxes(month + 5, &[character]).unwrap();
    assert_eq!(auth.land_account(character).unwrap().missed, 5);
    assert!(auth.foreclosed_estates().unwrap().is_empty());
    assert_eq!(game.housing_io.read_all_houses().await.unwrap().len(), 1);
    auth.collect_land_taxes(month + 6, &[character]).unwrap();
    assert_eq!(auth.land_account(character).unwrap().missed, 6);
    let save = game.get_player_save_data(&pid("Builder")).await.unwrap();
    assert!(auth
        .transfer_land_gold(save, &[], 1, true)
        .unwrap()
        .is_err());
    let restarted = crate::auth::AuthService::new(path).unwrap();
    game.tick_land_taxes(&restarted).await;
    assert!(auth.homestead_plots(character).unwrap().is_empty());
    assert!(game.housing_io.read_all_houses().await.unwrap().is_empty());
    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert!(game.estate_chests.read().await.get(chest).is_none());
    assert!(!game.passability_read().contains_key(&house));
    assert!(auth.load_fences().unwrap().is_empty());
    assert!(game
        .fences
        .read()
        .await
        .nearby(&pos3(21.0, 5.0, 21.0))
        .is_empty());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM estate_chest_items", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(auth.get_character_for_account(&account, character).is_ok());
    assert!(auth.foreclosed_estates().unwrap().is_empty());
}

#[tokio::test]
async fn last_character_keeps_estate_while_pending_and_removes_it_when_deleted() {
    let (game, auth, path, account, character, house, chest) =
        furnished_estate("last_character_cleanup").await;
    game.unregister_player_character(&pid("Builder")).await;
    game.change_character_deletion(&auth, &account, character, false)
        .await
        .unwrap();
    game.tick_character_deletions(&auth).await;
    assert_eq!(auth.homestead_plots(character).unwrap().len(), 1);
    assert_eq!(game.housing_io.read_all_houses().await.unwrap().len(), 1);
    assert_eq!(auth.load_estate_chests().unwrap().len(), 1);
    auth.make_character_deletion_due(character);
    game.tick_character_deletions(&auth).await;
    assert!(auth
        .list_characters_with_equipment(&account)
        .unwrap()
        .is_empty());
    assert!(game.housing_io.read_all_houses().await.unwrap().is_empty());
    assert!(auth.load_estate_chests().unwrap().is_empty());
    assert!(game.estate_chests.read().await.get(chest).is_none());
    assert!(!game.passability_read().contains_key(&house));
    let conn = rusqlite::Connection::open(path).unwrap();
    for table in [
        "land_estates",
        "land_plots",
        "estate_chest_items",
        "land_fences",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0,
            "{table}"
        );
    }
}

#[tokio::test]
async fn foreclosure_retries_failed_database_cleanup_in_the_same_month() {
    let (game, auth, path, _, character, _, chest) = furnished_estate("foreclosure_retry").await;
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute("UPDATE land_estates SET missed=6", [])
        .unwrap();
    conn.execute_batch("CREATE TRIGGER reject_foreclosure BEFORE DELETE ON land_estates BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
    game.tick_land_taxes(&auth).await;
    assert_eq!(auth.homestead_plots(character).unwrap().len(), 1);
    assert_eq!(auth.load_estate_chests().unwrap().len(), 1);
    assert!(game.estate_chests.read().await.get(chest).is_some());
    conn.execute_batch("DROP TRIGGER reject_foreclosure;")
        .unwrap();
    game.tick_land_taxes(&auth).await;
    assert!(auth.homestead_plots(character).unwrap().is_empty());
    assert!(game.estate_chests.read().await.get(chest).is_none());
    assert!(auth.load_estate_chests().unwrap().is_empty());
}

#[tokio::test]
async fn foreclosure_preserves_nearby_estates_and_evacuates_house_occupants() {
    let (game, auth, path, _, character, _, _) = furnished_estate("foreclosure_neighbor").await;
    let mut house = game.housing_io.read_all_houses().await.unwrap().remove(0);
    let room = house
        .rooms
        .iter()
        .find(|room| {
            room.floor_level == 0 && room.room_type == onlinerpg_shared::housing::RoomType::Normal
        })
        .unwrap();
    game.players
        .write()
        .await
        .get_mut(&pid("Builder"))
        .unwrap()
        .position = Position {
        x: house.origin.x + room.local_x as f32 + 0.5,
        y: house.origin.y,
        z: house.origin.z + room.local_z as f32 + 0.5,
    };
    let mut rx = game.register_direct_channel(&pid("Builder")).await;
    let neighbor_account = auth.login_google("foreclosure-neighbor").unwrap();
    let neighbor = estate_owner(
        &game,
        &auth,
        &neighbor_account,
        "Neighbor",
        pos3(69.0, 5.0, 5.0),
        vec![bag_item(1, "land_deed", 1), bag_item(2, "storage_chest", 1)],
    )
    .await;
    claim_plot_at(&game, &auth, "Neighbor", 69.0, 5.0).await;
    game.place_estate_chest(&pid("Neighbor"), 2, pos3(70.5, 5.0, 5.5), 0.0, 0, &auth)
        .await;
    house.id = "neighbor-house".to_string();
    house.owner_id = neighbor.to_string();
    house.origin.x += 64.0;
    game.housing_io.write_house(&house).await.unwrap();
    game.passability_add_house(&house).await;
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "UPDATE land_estates SET missed=6 WHERE owner_id=?1",
        [character],
    )
    .unwrap();
    tokio::time::timeout(Duration::from_secs(10), game.tick_land_taxes(&auth))
        .await
        .unwrap();
    let messages = drain(&mut rx);
    assert!(messages
        .iter()
        .any(|message| matches!(message, ServerMessage::HouseRemoved { .. })));
    assert!(messages.iter().any(|message| matches!(message, ServerMessage::EstateChestVisibility { removed, .. } if !removed.is_empty())));
    assert_eq!(auth.homestead_plots(neighbor).unwrap().len(), 1);
    assert_eq!(
        game.housing_io.read_all_houses().await.unwrap()[0].id,
        house.id
    );
    assert_eq!(auth.load_estate_chests().unwrap()[0].owner_id, neighbor);
    assert!(game.passability_read().contains_key(&house.id));
    let player = game
        .players
        .read()
        .await
        .get(&pid("Builder"))
        .unwrap()
        .clone();
    assert!(
        player.position.dist_xz_sq(
            &crate::world_config::world_config()
                .spawn_position
                .position()
        ) < 0.01
    );
    assert_eq!(player.floor_level, 0);
}
