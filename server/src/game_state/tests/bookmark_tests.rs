use super::*;
use onlinerpg_shared::bookmark::{
    BlessError, Bookmark, BookmarkError, BLESSED_SCROLL, BLESSING_PRICE, SCROLL,
};
use onlinerpg_shared::pathfinding::{RuntimeFloorGrid, RuntimePassability};

const CAMP: (f32, f32) = (800.0, 800.0);

async fn setup(tag: &str, gold: i64) -> (GameState, crate::auth::AuthService, i64, DirectRx) {
    let game = make_flat_world_game_state(tag);
    let auth = make_test_auth(tag);
    let account = auth.login_google("Marker").unwrap();
    let character = create_test_character(&auth, &account, "Marker");
    game.add_player(make_player("Marker", CAMP.0, CAMP.1)).await;
    game.register_player_character(
        &pid("Marker"),
        character.id,
        0,
        attrs_with_cha(12),
        gold,
        None,
    )
    .await;
    let rx = game.register_direct_channel(&pid("Marker")).await;
    (game, auth, character.id, rx)
}

fn bookmarks(rx: &mut DirectRx) -> Vec<Bookmark> {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::Bookmarks { bookmarks } => Some(bookmarks),
            _ => None,
        })
        .expect("bookmark list")
}

fn refusal(rx: &mut DirectRx) -> BookmarkError {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::BookmarkRefused { error } => Some(error),
            _ => None,
        })
        .expect("bookmark refusal")
}

#[tokio::test]
async fn a_bookmark_is_saved_where_the_character_stands_on_the_surface() {
    let (game, auth, _, mut rx) = setup("bookmark_save", 0).await;
    let id = pid("Marker");

    game.save_bookmark(&id, "  Camp ".into(), &auth).await;
    let saved = bookmarks(&mut rx);
    assert_eq!(saved.len(), 1);
    assert_eq!(
        (saved[0].name.as_str(), saved[0].x, saved[0].z),
        ("Camp", CAMP.0, CAMP.1)
    );

    game.save_bookmark(&id, "camp".into(), &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::Duplicate);
    game.save_bookmark(&id, " ".into(), &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::Name);
    game.players.write().await.get_mut(&id).unwrap().floor_level = -1;
    game.save_bookmark(&id, "Cellar".into(), &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::NotSurface);
    game.players.write().await.get_mut(&id).unwrap().floor_level = 0;

    game.rename_bookmark(&id, saved[0].id, "Base".into(), &auth)
        .await;
    assert_eq!(bookmarks(&mut rx)[0].name, "Base");
    game.delete_bookmark(&id, saved[0].id, &auth).await;
    assert!(bookmarks(&mut rx).is_empty());
    game.delete_bookmark(&id, saved[0].id, &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::Unknown);
}

#[tokio::test]
async fn only_a_blessed_scroll_read_out_of_combat_reaches_the_bookmark() {
    let (game, auth, _, mut rx) = setup("bookmark_teleport", 0).await;
    let id = pid("Marker");
    game.save_bookmark(&id, "Camp".into(), &auth).await;
    let camp = bookmarks(&mut rx).remove(0);
    let away = Position {
        x: 3_000.0,
        y: 5.0,
        z: 800.0,
    };
    game.teleport_player(&id, away, 0.0, 0).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(100, SCROLL, 1), bag_item(101, BLESSED_SCROLL, 2)],
            ..Default::default()
        },
    );

    game.teleport_to_bookmark(&id, 100, camp.id, &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::NoControl);
    put_in_combat(&game, &id).await;
    game.teleport_to_bookmark(&id, 101, camp.id, &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::Combat);
    game.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    game.teleport_to_bookmark(&id, 101, camp.id + 1, &auth)
        .await;
    assert_eq!(refusal(&mut rx), BookmarkError::Unknown);
    assert_eq!(game.players.read().await[&id].position.x, away.x);
    assert_eq!(
        (
            bag_count(&game, &id, SCROLL).await,
            bag_count(&game, &id, BLESSED_SCROLL).await
        ),
        (1, 2)
    );

    game.teleport_to_bookmark(&id, 101, camp.id, &auth).await;
    let position = game.players.read().await[&id].position;
    assert_eq!((position.x, position.z), CAMP);
    assert_eq!(bag_count(&game, &id, BLESSED_SCROLL).await, 1);
}

#[tokio::test]
async fn the_blesser_trades_plain_scrolls_and_gold_for_blessed_scrolls() {
    let gold = BLESSING_PRICE * 2 + 1;
    let (game, auth, character_id, mut rx) = setup("bookmark_bless", gold).await;
    let id = pid("Marker");
    let mut maelis = make_player("Maelis", CAMP.0 + 1.0, CAMP.1);
    maelis.is_official_npc = true;
    game.add_player(maelis).await;
    game.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(100, SCROLL, 3)],
            ..Default::default()
        },
    );
    let result = |rx: &mut DirectRx| {
        drain(rx)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::BlessResult { blessed, error } => Some((blessed, error)),
                _ => None,
            })
            .expect("bless result")
    };

    game.bless_scrolls(&id, &pid("Maelis"), 3, &auth).await;
    assert_eq!(result(&mut rx), (0, Some(BlessError::NotEnoughGold)));
    assert_eq!(
        (
            bag_count(&game, &id, SCROLL).await,
            bag_count(&game, &id, BLESSED_SCROLL).await
        ),
        (3, 0)
    );

    game.bless_scrolls(&id, &pid("Maelis"), 2, &auth).await;
    assert_eq!(result(&mut rx), (2, None));
    assert_eq!(
        (
            bag_count(&game, &id, SCROLL).await,
            bag_count(&game, &id, BLESSED_SCROLL).await
        ),
        (1, 2)
    );
    assert_eq!(game.get_player_gold(&id).await, 1);
    let saved = auth.load_inventory(character_id).unwrap();
    assert!(saved
        .iter()
        .any(|row| row.item_def_id == BLESSED_SCROLL && row.quantity == 2));

    game.adjust_gold(&id, BLESSING_PRICE * 2).await;
    game.bless_scrolls(&id, &pid("Maelis"), 2, &auth).await;
    assert_eq!(result(&mut rx), (0, Some(BlessError::NoScrolls)));
    game.bless_scrolls(&id, &id, 1, &auth).await;
    assert_eq!(result(&mut rx), (0, Some(BlessError::TooFar)));

    game.offer_blessing(&pid("Maelis"), &id).await;
    assert!(drain(&mut rx).into_iter().any(|message| matches!(
        message,
        ServerMessage::BlessingOffered { npc_player_id } if npc_player_id == pid("Maelis")
    )));
    game.offer_blessing(&id, &id).await;
    assert!(drain(&mut rx).is_empty());

    game.set_npc_schedule(
        "Maelis",
        vec![onlinerpg_shared::schedule::ScheduleEntry {
            at: "0:00".into(),
            action: Some("chair".into()),
            ..Default::default()
        }],
    );
    game.bless_scrolls(&id, &pid("Maelis"), 1, &auth).await;
    assert_eq!(result(&mut rx), (0, Some(BlessError::OffDuty)));
    game.offer_blessing(&pid("Maelis"), &id).await;
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn a_spot_under_a_furniture_hull_can_be_bookmarked_but_a_room_cannot() {
    let (game, auth, _, mut rx) = setup("bookmark_house", 0).await;
    let id = pid("Marker");
    let grid = |origin_x: f32, is_ground| RuntimePassability {
        house_origin_x: origin_x,
        house_origin_z: CAMP.1 - 2.0,
        min_x: origin_x,
        max_x: origin_x + 4.0,
        min_z: CAMP.1 - 2.0,
        max_z: CAMP.1 + 2.0,
        floors: vec![RuntimeFloorGrid {
            floor_level: 0,
            origin_x: 0,
            origin_z: 0,
            width: 4,
            depth: 4,
            y_base: 5.0,
            wall_height: 3.0,
            cells: vec![0; 16],
        }],
        stairwells: vec![],
        yields_to_trapped_mover: false,
        allows_projectiles: false,
        is_ground,
    };
    {
        let mut cache = game.passability_write();
        cache.insert("plaza".into(), grid(CAMP.0 - 2.0, false));
        cache.insert("house".into(), grid(CAMP.0 + 4.0, true));
    }

    game.save_bookmark(&id, "Plaza".into(), &auth).await;
    assert_eq!(bookmarks(&mut rx).len(), 1);
    game.players.write().await.get_mut(&id).unwrap().position.x = CAMP.0 + 6.0;
    game.save_bookmark(&id, "Parlour".into(), &auth).await;
    assert_eq!(refusal(&mut rx), BookmarkError::NoLanding);
}
