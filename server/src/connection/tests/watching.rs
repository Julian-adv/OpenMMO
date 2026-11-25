use super::*;
use crate::game::character_attributes::roll_character_attributes;

#[cfg(debug_assertions)]
#[tokio::test]
async fn a_watcher_follows_the_session_without_taking_it_over() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "watch_entry",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("watch_entry"));
    let auth_ctx = test_auth_ctx();
    let account = auth.login_google("watch-entry").unwrap();
    let character = auth
        .create_character(
            &account,
            "Watched",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap();
    let send = async |state: &mut ConnectionState, msg: ClientMessage| {
        let encoded = onlinerpg_shared::serialize_client_msg(&msg).unwrap();
        handle_client_message(&encoded, &game, &auth, &auth_ctx, state)
            .await
            .unwrap()
    };
    let token = format!("dev:{account}:{}", dev_login_secret(&auth_ctx.npc_token));
    let watcher = async || {
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
            &mut state,
        );
        let request = ClientMessage::WatchAccount {
            google_id_token: token.clone(),
        };
        let responses = send(&mut state, request).await;
        (state, responses)
    };

    let (mut early, responses) = watcher().await;
    assert!(
        matches!(responses.as_slice(), [ServerMessage::WatchWaiting]),
        "{responses:?}"
    );

    let mut owner = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "mcp"),
        &mut owner,
    );
    finish_auth(&game, &auth, &mut owner, account.clone(), false).await;
    send(
        &mut owner,
        ClientMessage::EnterGame {
            character_id: character.id,
        },
    )
    .await;
    let id = owner.player_id.unwrap();
    let teed = early.watcher.as_mut().unwrap().rx.try_recv().unwrap();
    assert!(matches!(
        onlinerpg_shared::deserialize_server_msg(&teed).unwrap(),
        ServerMessage::WatchTarget { character: c } if c.id == character.id
    ));

    let (mut late, responses) = watcher().await;
    assert!(matches!(
        responses.first(),
        Some(ServerMessage::WatchTarget { .. })
    ));
    assert!(responses
        .iter()
        .any(|msg| matches!(msg, ServerMessage::JoinSuccess { player, is_admin: false } if player.id == id)));
    assert!(
        game.is_current_account_session(&account, owner.account_session_id.unwrap())
            .await
    );
    assert_eq!((late.player_id, late.account_session_id), (None, None));

    for refused in [
        ClientMessage::ChatMessage {
            message: "/god".into(),
        },
        ClientMessage::CreateCharacter {
            character_name: "Intruder".into(),
            character_class: CharacterClass::Knight,
            gender: crate::types::Gender::Male,
            appearance: Default::default(),
        },
        ClientMessage::EnterGame {
            character_id: character.id,
        },
    ] {
        assert!(send(&mut late, refused).await.is_empty());
    }
    assert_eq!(
        auth.list_characters_with_equipment(&account).unwrap().len(),
        1
    );
    assert_eq!(owner.player_id, Some(id));

    owner.owner_watch.as_ref().unwrap().left(id);
    let rx = &mut late.watcher.as_mut().unwrap().rx;
    let waiting = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|bytes| onlinerpg_shared::deserialize_server_msg(&bytes).ok())
        .any(|msg| matches!(msg, ServerMessage::WatchWaiting));
    assert!(waiting);
}

#[tokio::test]
async fn a_login_waits_to_take_over_a_playing_session() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "watch_deferred",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("watch_deferred"));
    let auth_ctx = test_auth_ctx();
    let account = auth.login_google("watch-deferred").unwrap();
    let character = auth
        .create_character(
            &account,
            "Played",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap();
    let send = async |state: &mut ConnectionState, msg: ClientMessage| {
        let encoded = onlinerpg_shared::serialize_client_msg(&msg).unwrap();
        handle_client_message(&encoded, &game, &auth, &auth_ctx, state)
            .await
            .unwrap()
    };
    let login = async || {
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
            &mut state,
        );
        let responses = finish_auth(&game, &auth, &mut state, account.clone(), false).await;
        (state, responses)
    };
    let enter = ClientMessage::EnterGame {
        character_id: character.id,
    };

    let (mut owner, _) = login().await;
    send(&mut owner, enter).await;
    let owner_session = owner.account_session_id.unwrap();

    let (mut lobby, responses) = login().await;
    assert!(matches!(
        responses.as_slice(),
        [ServerMessage::AuthSuccess { playing: Some(name), .. }] if name == "Played"
    ));
    assert!(
        game.is_current_account_session(&account, owner_session)
            .await
    );

    let (mut watcher, _) = login().await;
    let responses = send(&mut watcher, ClientMessage::WatchPlaying).await;
    assert!(responses
        .iter()
        .any(|msg| matches!(msg, ServerMessage::JoinSuccess { .. })));
    assert!(watcher.watcher.is_some());
    assert!(
        game.is_current_account_session(&account, owner_session)
            .await
    );

    send(
        &mut lobby,
        ClientMessage::EnterGame {
            character_id: character.id,
        },
    )
    .await;
    assert!(
        !game
            .is_current_account_session(&account, owner_session)
            .await
    );
    assert!(lobby.player_id.is_some());
}
