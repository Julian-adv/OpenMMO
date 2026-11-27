use super::*;
use onlinerpg_shared::estate_storage::EstateChest;
use onlinerpg_shared::respawn::{RespawnPlace, ShelterAccess, AID_COT};
use onlinerpg_terrain::land::plot_addr;

/// An estate owner with a claimed plot at (1, 1) and an aid cot at `cot`.
async fn shelter_owner(
    game: &GameState,
    auth: &crate::auth::AuthService,
    tag: &str,
    name: &str,
    cot: Position,
) -> (EstateChest, DirectRx) {
    let account = auth.login_google(&format!("shelter-{tag}")).unwrap();
    let bag = vec![bag_item(1, "land_deed", 1), bag_item(2, AID_COT, 1)];
    estate_owner(game, auth, &account, name, pos3(1.0, 0.0, 1.0), bag).await;
    let rx = game.register_direct_channel(&pid(name)).await;
    claim_plot_at(game, auth, name, 1.0, 1.0).await;
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&pid(name)).unwrap();
        player.position = cot;
        player.floor_level = 0;
    }
    game.place_estate_chest(&pid(name), 2, cot, 90.0, 0, auth)
        .await;
    let cot = auth
        .load_estate_chests()
        .unwrap()
        .into_iter()
        .find(|chest| chest.item_def_id == AID_COT)
        .expect("aid cot placed");
    (cot, rx)
}

async fn stranger(game: &GameState, auth: &crate::auth::AuthService, name: &str) -> DirectRx {
    let account = auth
        .login_google(&format!("shelter-stranger-{name}"))
        .unwrap();
    estate_owner(game, auth, &account, name, pos3(1.0, 0.0, 1.0), vec![]).await;
    game.register_direct_channel(&pid(name)).await
}

async fn fall(game: &GameState, name: &str, x: f32, z: f32) {
    let mut players = game.players.write().await;
    let player = players.get_mut(&pid(name)).unwrap();
    player.position = pos3(x, 5.0, z);
    player.floor_level = 0;
    player.health = 0;
}

fn options(rx: &mut DirectRx) -> (bool, bool, Option<i64>) {
    drain(rx)
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ServerMessage::RespawnOptions {
                estate,
                family_seat,
                nearby,
            } => Some((estate, family_seat, nearby.map(|n| n.estate_id))),
            _ => None,
        })
        .expect("respawn options sent")
}

#[tokio::test]
async fn owners_wake_on_their_aid_cot_from_anywhere() {
    let game = make_test_game_state("shelter_own");
    let auth = make_test_auth("shelter_own");
    let (cot, mut rx) = shelter_owner(&game, &auth, "own", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    fall(&game, "Keeper", -1440.0, 4750.0).await;
    drain(&mut rx);
    game.send_respawn_options(&pid("Keeper"), &auth).await;
    assert_eq!(options(&mut rx), (true, false, None));
    game.respawn_player_at(&pid("Keeper"), RespawnPlace::Estate, &auth)
        .await;
    let player = game.players.read().await[&pid("Keeper")].clone();
    assert_eq!(player.health, player.max_health);
    assert_eq!(player.position, cot.position);
    assert_eq!(player.object_type.as_deref(), Some(AID_COT));
    assert_eq!(player.object_id, Some(cot.id as u32));
}

#[tokio::test]
async fn public_shelters_take_in_strangers_who_fell_nearer_than_the_inn() {
    let game = make_test_game_state("shelter_public");
    let auth = make_test_auth("shelter_public");
    let (cot, _) = shelter_owner(&game, &auth, "public", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    let mut rx = stranger(&game, &auth, "Wanderer").await;
    fall(&game, "Wanderer", 60.0, 40.0).await;

    game.send_respawn_options(&pid("Wanderer"), &auth).await;
    assert_eq!(options(&mut rx), (false, false, None));
    game.respawn_player_at(
        &pid("Wanderer"),
        RespawnPlace::Nearby {
            estate_id: cot.estate_id,
        },
        &auth,
    )
    .await;
    let position = game.players.read().await[&pid("Wanderer")].position;
    assert_ne!(plot_addr(position.x, position.z), plot_addr(1.0, 1.0));

    game.set_shelter_access(&pid("Keeper"), cot.id, ShelterAccess::Public, &auth)
        .await;
    fall(&game, "Wanderer", 60.0, 40.0).await;
    game.send_respawn_options(&pid("Wanderer"), &auth).await;
    assert_eq!(options(&mut rx), (false, false, Some(cot.estate_id)));
    game.respawn_player_at(
        &pid("Wanderer"),
        RespawnPlace::Nearby {
            estate_id: cot.estate_id,
        },
        &auth,
    )
    .await;
    let player = game.players.read().await[&pid("Wanderer")].clone();
    assert_eq!(player.position, cot.position);
    assert_eq!(player.object_id, Some(cot.id as u32));

    fall(&game, "Wanderer", -1440.0, 4750.0).await;
    game.send_respawn_options(&pid("Wanderer"), &auth).await;
    assert_eq!(options(&mut rx), (false, false, None));
}

#[tokio::test]
async fn a_taken_cot_leaves_the_next_sleeper_standing_on_the_estate() {
    let game = make_test_game_state("shelter_taken");
    let auth = make_test_auth("shelter_taken");
    let (cot, _) = shelter_owner(&game, &auth, "taken", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    game.set_shelter_access(&pid("Keeper"), cot.id, ShelterAccess::Public, &auth)
        .await;
    stranger(&game, &auth, "First").await;
    stranger(&game, &auth, "Second").await;
    let nearby = RespawnPlace::Nearby {
        estate_id: cot.estate_id,
    };
    fall(&game, "First", 60.0, 40.0).await;
    game.respawn_player_at(&pid("First"), nearby, &auth).await;
    fall(&game, "Second", 60.0, 40.0).await;
    game.respawn_player_at(&pid("Second"), nearby, &auth).await;
    let players = game.players.read().await;
    assert_eq!(players[&pid("First")].object_id, Some(cot.id as u32));
    let second = &players[&pid("Second")];
    assert_eq!(second.health, second.max_health);
    assert_eq!(second.object_id, None);
    assert_eq!(
        plot_addr(second.position.x, second.position.z),
        plot_addr(1.0, 1.0)
    );
}

#[tokio::test]
async fn only_the_owner_opens_the_shelter_and_every_cot_follows() {
    let game = make_test_game_state("shelter_access");
    let auth = make_test_auth("shelter_access");
    let (cot, _) = shelter_owner(&game, &auth, "access", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    assert_eq!(cot.shelter, Some(ShelterAccess::Estate));
    stranger(&game, &auth, "Meddler").await;
    game.set_shelter_access(&pid("Meddler"), cot.id, ShelterAccess::Public, &auth)
        .await;
    assert_eq!(
        game.estate_chests.read().await.get(cot.id).unwrap().shelter,
        Some(ShelterAccess::Estate)
    );
    game.set_shelter_access(&pid("Keeper"), cot.id, ShelterAccess::Public, &auth)
        .await;
    assert_eq!(
        game.estate_chests.read().await.get(cot.id).unwrap().shelter,
        Some(ShelterAccess::Public)
    );
    assert_eq!(auth.public_shelter_markers().unwrap().len(), 1);
    let reloaded = auth.load_estate_chests().unwrap();
    assert_eq!(
        reloaded.iter().find(|c| c.id == cot.id).unwrap().shelter,
        Some(ShelterAccess::Public)
    );
}

#[tokio::test]
async fn overdue_shelters_stay_open_to_the_owner_only() {
    let game = make_test_game_state("shelter_overdue");
    let (auth, path) = make_test_auth_with_path("shelter_overdue");
    let (cot, mut owner_rx) =
        shelter_owner(&game, &auth, "overdue", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    game.set_shelter_access(&pid("Keeper"), cot.id, ShelterAccess::Public, &auth)
        .await;
    rusqlite::Connection::open(path)
        .unwrap()
        .execute("UPDATE land_estates SET missed=1", [])
        .unwrap();
    let mut rx = stranger(&game, &auth, "Wanderer").await;
    fall(&game, "Wanderer", 60.0, 40.0).await;
    game.send_respawn_options(&pid("Wanderer"), &auth).await;
    assert_eq!(options(&mut rx), (false, false, None));
    assert!(auth.public_shelter_markers().unwrap().is_empty());
    fall(&game, "Keeper", -1440.0, 4750.0).await;
    drain(&mut owner_rx);
    game.send_respawn_options(&pid("Keeper"), &auth).await;
    assert_eq!(options(&mut owner_rx), (true, false, None));
}

#[tokio::test]
async fn waking_on_the_cot_is_a_lasting_bed_rest() {
    let game = make_test_game_state("shelter_bed_rest");
    let auth = make_test_auth("shelter_bed_rest");
    let (cot, _) = shelter_owner(&game, &auth, "rest", "Keeper", pos3(6.5, 5.0, 6.5)).await;
    fall(&game, "Keeper", -1440.0, 4750.0).await;
    game.respawn_player_at(&pid("Keeper"), RespawnPlace::Estate, &auth)
        .await;
    let player = game.players.read().await[&pid("Keeper")].clone();
    assert!(game.valid_bed_pose(&player).await);
    game.end_invalid_bed_rests().await;
    assert_eq!(
        game.players.read().await[&pid("Keeper")].object_id,
        Some(cot.id as u32)
    );
}
