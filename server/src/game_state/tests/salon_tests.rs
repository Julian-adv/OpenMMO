use super::*;
use onlinerpg_shared::salon::{SalonError, FACE_PRICE, HAIR_COLOR_PRICE, HAIR_PRICE};
use onlinerpg_shared::{CharacterAppearance, FaceStyle, HairStyle};

async fn setup(tag: &str, gold: i64) -> (GameState, crate::auth::AuthService, String, DirectRx) {
    let game = make_flat_world_game_state(tag);
    let auth = make_test_auth(tag);
    let account = auth.login_google("Client").unwrap();
    let character = create_test_character(&auth, &account, "Client");
    game.add_player(make_player("Client", 0.0, 0.0)).await;
    game.register_player_character(
        &pid("Client"),
        character.id,
        0,
        attrs_with_cha(12),
        gold,
        None,
    )
    .await;
    let mut odette = make_player("Odette", 1.0, 0.0);
    odette.is_official_npc = true;
    game.add_player(odette).await;
    let rx = game.register_direct_channel(&pid("Client")).await;
    (game, auth, account, rx)
}

fn result(rx: &mut DirectRx) -> (i64, Option<SalonError>) {
    drain(rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::AppearanceChangeResult { price, error } => Some((price, error)),
            _ => None,
        })
        .expect("salon result")
}

fn restyled() -> CharacterAppearance {
    CharacterAppearance {
        face: FaceStyle::Rugged,
        hair: HairStyle::WavyBone,
        ..Default::default()
    }
}

#[tokio::test]
async fn a_paid_restyle_is_saved_and_seen_by_those_nearby_and_those_who_arrive_later() {
    let (game, auth, account, mut rx) = setup("salon_restyle", 30_000).await;
    let client = pid("Client");
    game.add_player(make_player("Watcher", 2.0, 0.0)).await;
    let mut watcher = game.register_direct_channel(&pid("Watcher")).await;
    drain(&mut watcher);

    game.change_appearance(&client, &pid("Odette"), restyled(), &auth)
        .await;
    assert_eq!(result(&mut rx), (FACE_PRICE + HAIR_PRICE, None));
    assert_eq!(game.get_player_gold(&client).await, 15_000);
    assert_eq!(game.get_all_players().await[&client].appearance, restyled());
    assert!(drain(&mut watcher).iter().any(|message| matches!(message,
        ServerMessage::PlayerAppearanceChanged { player_id, appearance }
        if *player_id == client && *appearance == restyled()
    )));
    let character_id = game.character_id_of(&client).await.unwrap();
    let saved = auth
        .get_character_for_account(&account, character_id)
        .unwrap();
    assert_eq!((saved.appearance, saved.gold), (restyled(), 15_000));
    let late = join_snapshot(&game, make_player("Late", 3.0, 0.0)).await;
    assert!(late.iter().any(|message| matches!(message,
        ServerMessage::PlayerAppeared { player }
        if player.id == client && player.appearance == restyled()
    )));
    assert_gold_consumption(&game, GoldSink::Salon, 1, 15_000).await;

    let recoloured = CharacterAppearance {
        hair_color_original: true,
        ..restyled()
    };
    game.change_appearance(&client, &pid("Odette"), recoloured, &auth)
        .await;
    assert_eq!(result(&mut rx), (HAIR_COLOR_PRICE, None));
    assert_eq!(game.get_player_gold(&client).await, 13_000);
}

#[tokio::test]
async fn nothing_is_charged_when_the_restyle_is_refused() {
    let (game, auth, _, mut rx) = setup("salon_refusals", 14_999).await;
    let client = pid("Client");
    let odette = pid("Odette");
    let ask = |appearance| game.change_appearance(&client, &odette, appearance, &auth);

    ask(CharacterAppearance::default()).await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::Unchanged)));
    // A woman's face on a man is fitted back to his own, which leaves nothing to change.
    ask(CharacterAppearance {
        face: FaceStyle::Elegant,
        ..Default::default()
    })
    .await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::Unchanged)));
    ask(restyled()).await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::NotEnoughGold)));

    game.add_player(make_player("Stranger", 1.0, 1.0)).await;
    game.change_appearance(&client, &pid("Stranger"), restyled(), &auth)
        .await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::TooFar)));
    game.players
        .write()
        .await
        .get_mut(&client)
        .unwrap()
        .position
        .x = 40.0;
    ask(restyled()).await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::TooFar)));
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&client).unwrap();
        player.position.x = 0.0;
        player.class = CharacterClass::Merchant;
    }
    ask(restyled()).await;
    assert_eq!(result(&mut rx), (0, Some(SalonError::Unsupported)));

    assert_eq!(game.get_player_gold(&client).await, 14_999);
    assert_eq!(
        game.get_all_players().await[&client].appearance,
        CharacterAppearance::default()
    );
}
