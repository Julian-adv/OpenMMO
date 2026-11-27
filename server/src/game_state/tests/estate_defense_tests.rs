// ---- War-horn estate defenses ----------------------------------------------

use super::*;
use onlinerpg_shared::world::GameDateTime;

const BANNER: (f32, f32) = (16.0, 16.0);

fn night() -> GameDateTime {
    GameDateTime {
        year: 217,
        month: 1,
        day: 10,
        hour: 0,
        minute: 0,
    }
}

fn noon() -> GameDateTime {
    GameDateTime {
        hour: 12,
        ..night()
    }
}

async fn lord(name: &str) -> (GameState, crate::auth::AuthService, DirectRx) {
    let game = make_flat_world_game_state(name);
    let auth = make_test_auth(name);
    let account = auth.login_google(name).unwrap();
    estate_owner(
        &game,
        &auth,
        &account,
        "Lord",
        pos3(BANNER.0, 5.0, BANNER.1),
        vec![bag_item(1, "land_deed", 1), bag_item(2, "war_horn_1", 2)],
    )
    .await;
    claim_plot_at(&game, &auth, "Lord", BANNER.0, BANNER.1).await;
    assert_eq!(auth.owned_land_plots().unwrap().len(), 1);
    game.debug_set_datetime(&night());
    let rx = game.register_direct_channel(&pid("Lord")).await;
    (game, auth, rx)
}

async fn horns(game: &GameState) -> u32 {
    bag_count(game, &pid("Lord"), "war_horn_1").await
}

async fn raiders(game: &GameState) -> Vec<crate::types::Monster> {
    game.monsters
        .read()
        .await
        .values()
        .filter(|m| m.lifecycle == MonsterLifecycle::EstateDefense && m.state != MonsterState::Dead)
        .cloned()
        .collect()
}

async fn tick(game: &GameState, auth: &crate::auth::AuthService, times: u32) {
    for _ in 0..times {
        game.tick_estate_defenses(auth).await;
    }
}

async fn slay_raiders(game: &GameState, auth: &crate::auth::AuthService) {
    for raider in raiders(game).await {
        game.damage_monster(
            &pid("Lord"),
            "Lord",
            super::super::combat::StruckMonster {
                monster_id: &raider.id,
                monster_type: &raider.monster_type,
                position: raider.position,
                floor_level: 0,
                level_override: raider.level_override,
            },
            raider.health,
            std::time::Duration::ZERO,
            Some(auth),
        )
        .await;
    }
}

fn notices(rx: &mut DirectRx) -> Vec<String> {
    drain(rx)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::SystemMessage {
                localization: Some(l),
                ..
            } => Some(l.code),
            _ => None,
        })
        .collect()
}

#[tokio::test]
async fn a_refused_horn_stays_in_the_bag() {
    let (game, auth, mut rx) = lord("defense_refused").await;
    game.debug_set_datetime(&noon());
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    assert_eq!(horns(&game).await, 2);
    assert!(notices(&mut rx).contains(&"server.estateDefense.notNight".to_string()));

    game.debug_set_datetime(&night());
    game.players
        .write()
        .await
        .get_mut(&pid("Lord"))
        .unwrap()
        .position = pos3(80.0, 5.0, 80.0);
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    assert_eq!(horns(&game).await, 2);
    let codes = notices(&mut rx);
    assert!(
        codes.contains(&"server.estateDefense.notOwnEstate".to_string()),
        "{codes:?}"
    );
    assert!(raiders(&game).await.is_empty());
}

#[tokio::test]
async fn holding_the_banner_through_five_waves_pays_the_defender() {
    let (game, auth, mut rx) = lord("defense_held").await;
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    assert_eq!(horns(&game).await, 1);
    assert!(game.in_estate_defense_circle(BANNER.0 + 5.0, BANNER.1));
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    assert_eq!(horns(&game).await, 1, "one defense per estate");

    tick(&game, &auth, 32).await;
    let first = raiders(&game).await;
    assert_eq!(first.len(), 6);
    for raider in &first {
        let banner = pos3(BANNER.0, 5.0, BANNER.1);
        assert!((raider.position.dist_xz_sq(&banner).sqrt() - 10.0).abs() < 0.5);
        assert!(raider.siege.is_some());
    }
    assert!(game.is_defending(&pid("Lord")));

    let mut killed = 0;
    for _ in 0..400 {
        slay_raiders(&game, &auth).await;
        killed = killed.max(1);
        tick(&game, &auth, 1).await;
        if !game.in_estate_defense_circle(BANNER.0, BANNER.1) {
            break;
        }
    }
    assert!(killed > 0);
    assert!(!game.in_estate_defense_circle(BANNER.0, BANNER.1));
    let codes = notices(&mut rx);
    assert!(
        codes.contains(&"server.estateDefense.held".to_string()),
        "{codes:?}"
    );
    assert!(codes.contains(&"server.estateDefense.waveAll".to_string()));

    tokio::time::sleep(std::time::Duration::from_millis(1900)).await;
    let potions = game
        .ground_items
        .read()
        .await
        .values()
        .filter(|g| {
            g.item.item_def_id == "healing_potion"
                && g.item.claimed_by == Some(pid("Lord"))
                && g.item.position.dist_xz_sq(&pos3(BANNER.0, 0.0, BANNER.1)) < 4.0 * 4.0
        })
        .count();
    assert_eq!(potions, 3);
    assert_eq!(bag_count(&game, &pid("Lord"), "trophy_orc_axes").await, 1);
    let trophy = trophy_id(&game).await;
    let record = auth.estate_trophy(trophy).unwrap().expect("trophy record");
    assert_eq!(record.summoner, "Lord");
    assert_eq!(record.defenders, vec!["Lord".to_string()]);
    assert_eq!(record.defended_on.day, night().day);
    let character = game.character_id_of(&pid("Lord")).await.unwrap();
    assert!(auth
        .load_titles(character)
        .unwrap()
        .0
        .contains(&"estate_defender_1".to_string()));
}

#[tokio::test]
async fn a_raider_on_the_banner_wears_it_down() {
    let (game, auth, mut rx) = lord("defense_breach").await;
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    tick(&game, &auth, 32).await;
    let raider = raiders(&game).await.remove(0);
    game.monsters
        .write()
        .await
        .get_mut(&raider.id)
        .unwrap()
        .position = pos3(BANNER.0 + 1.0, 5.0, BANNER.1);
    game.monsters
        .write()
        .await
        .get_mut(&raider.id)
        .unwrap()
        .state = MonsterState::Attack;
    tick(&game, &auth, 3).await;
    assert_eq!(
        raiders(&game).await.len(),
        6,
        "fighting by the banner is not taking it"
    );
    game.monsters
        .write()
        .await
        .get_mut(&raider.id)
        .unwrap()
        .state = MonsterState::Idle;
    tick(&game, &auth, 3).await;
    assert_eq!(raiders(&game).await.len(), 5);
    assert!(notices(&mut rx).contains(&"server.estateDefense.breach".to_string()));
}

#[tokio::test]
async fn a_wiped_defense_disgraces_the_lord() {
    let (game, auth, mut rx) = lord("defense_wiped").await;
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    tick(&game, &auth, 32).await;
    game.players
        .write()
        .await
        .get_mut(&pid("Lord"))
        .unwrap()
        .health = 0;
    tick(&game, &auth, 10).await;
    assert!(raiders(&game).await.is_empty());
    assert!(!game.in_estate_defense_circle(BANNER.0, BANNER.1));
    let codes = notices(&mut rx);
    assert!(
        codes.contains(&"server.estateDefense.wiped".to_string()),
        "{codes:?}"
    );
    assert!(codes.contains(&"server.estateDefense.disgraced".to_string()));
    assert_eq!(bag_count(&game, &pid("Lord"), "trophy_orc_axes").await, 0);
    let shown = game
        .players
        .read()
        .await
        .get(&pid("Lord"))
        .and_then(|p| p.title.clone());
    assert_eq!(shown.as_deref(), Some("estate_disgrace"));
    let character = game.character_id_of(&pid("Lord")).await.unwrap();
    assert!(auth
        .active_disgraces(crate::auth::unix_now())
        .unwrap()
        .characters
        .contains_key(&character));
}

#[tokio::test]
async fn raiders_march_on_the_banner() {
    let (game, auth, _rx) = lord("defense_march").await;
    game.blow_war_horn(&pid("Lord"), 2, &auth).await;
    tick(&game, &auth, 32).await;
    let banner = pos3(BANNER.0, 5.0, BANNER.1);
    for _ in 0..40 {
        game.tick_monster_ai_by(200.0).await;
    }
    let closest = raiders(&game)
        .await
        .iter()
        .map(|m| m.position.dist_xz_sq(&banner).sqrt())
        .fold(f32::MAX, f32::min);
    assert!(
        closest < 8.0,
        "closest raider {closest:.1} m from the banner"
    );
}

#[tokio::test]
async fn a_practice_defense_needs_no_horn_and_leaves_no_disgrace() {
    let (game, auth, _rx) = lord("defense_practice").await;
    game.debug_set_datetime(&noon());
    game.players
        .write()
        .await
        .get_mut(&pid("Lord"))
        .unwrap()
        .position = pos3(80.0, 5.0, 80.0);
    let reply = game
        .practice_defense_command(&pid("Lord"), "2")
        .await
        .unwrap();
    assert!(reply.contains("defense_2"));
    assert_eq!(horns(&game).await, 2);
    tick(&game, &auth, 32).await;
    assert_eq!(raiders(&game).await.len(), 6);
    game.players
        .write()
        .await
        .get_mut(&pid("Lord"))
        .unwrap()
        .health = 0;
    tick(&game, &auth, 10).await;
    assert!(raiders(&game).await.is_empty());
    let character = game.character_id_of(&pid("Lord")).await.unwrap();
    assert!(!auth
        .active_disgraces(crate::auth::unix_now())
        .unwrap()
        .characters
        .contains_key(&character));
}

async fn trophy_id(game: &GameState) -> u64 {
    game.inventories.read().await[&pid("Lord")]
        .bag
        .iter()
        .find(|item| item.item_def_id == "trophy_orc_axes")
        .map(|item| item.instance_id)
        .expect("trophy in the bag")
}

#[tokio::test]
async fn a_trophy_keeps_its_id_through_a_save_and_the_wall() {
    let (game, auth, _rx) = lord("defense_trophy_id").await;
    game.award_item(&pid("Lord"), "trophy_orc_axes").await;
    let id = trophy_id(&game).await;
    let character = game.character_id_of(&pid("Lord")).await.unwrap();

    game.mark_inventory_dirty(&pid("Lord")).await;
    game.flush_dirty_saves(&auth).await;
    game.load_player_inventory(&pid("Lord"), character, &auth)
        .await;
    assert_eq!(trophy_id(&game).await, id, "reloaded from the database");

    let spot = pos3(BANNER.0 + 2.0, 5.0, BANNER.1);
    game.players
        .write()
        .await
        .get_mut(&pid("Lord"))
        .unwrap()
        .position = spot;
    game.place_estate_chest(&pid("Lord"), id, spot, 0.0, 0, &auth)
        .await;
    let placed = auth
        .load_estate_chests()
        .unwrap()
        .into_iter()
        .find(|chest| chest.item_def_id == "trophy_orc_axes")
        .expect("trophy on the wall");
    assert_eq!(placed.item_id, Some(id));
    assert_eq!(bag_count(&game, &pid("Lord"), "trophy_orc_axes").await, 0);

    game.recover_estate_chest(&pid("Lord"), placed.id, &auth)
        .await;
    assert_eq!(trophy_id(&game).await, id, "taken down again");

    game.drop_items(
        &pid("Lord"),
        vec![onlinerpg_shared::messages::BagLineItem {
            instance_id: id,
            qty: 1,
        }],
    )
    .await;
    assert!(
        game.ground_items.read().await.contains_key(&id),
        "on the ground"
    );
    game.pickup_item(&pid("Lord"), id).await;
    assert_eq!(trophy_id(&game).await, id, "picked up again");
}
