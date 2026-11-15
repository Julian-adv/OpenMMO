use super::*;

#[tokio::test]
async fn unauthenticated_character_deletion_is_rejected_and_identified() {
    use tracing::instrument::WithSubscriber;

    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "deletion_unauthenticated",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth(
        "deletion_unauthenticated",
    ));
    let auth_ctx = test_auth_ctx();
    let mut state = ConnectionState::new("203.0.113.7".parse().unwrap());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    for (request, action) in [
        (
            ClientMessage::DeleteCharacter { character_id: 42 },
            "DeleteCharacter",
        ),
        (
            ClientMessage::CancelCharacterDeletion { character_id: 42 },
            "CancelCharacterDeletion",
        ),
    ] {
        let (subscriber, buffer) = crate::test_util::capture_logs();
        let encoded = onlinerpg_shared::serialize_client_msg(&request).unwrap();
        let responses = handle_client_message(&encoded, &game, &auth, &auth_ctx, &mut state)
            .with_subscriber(subscriber)
            .await
            .unwrap();
        assert!(
            matches!(responses.as_slice(), [ServerMessage::CharacterError { message }] if message == "Authenticate first")
        );
        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        for field in [
            "unauthenticated client",
            "ip=203.0.113.7",
            "character_id=42",
            &format!("action=\"{action}\""),
        ] {
            assert!(logs.contains(field), "{logs}");
        }
    }
}

#[test]
fn token_matches_requires_exact_token() {
    assert!(token_matches("secret-token", "secret-token"));
    assert!(!token_matches("secret-token", "secret-tokeN"));
    assert!(!token_matches("secret", "secret-token"));
    assert!(!token_matches("", "secret-token"));
}

/// The gate both login paths share: a banned account must not reach the
/// character list, and lifting the ban must let it back in.
#[tokio::test]
async fn finish_auth_refuses_a_banned_account() {
    let game_state = crate::game_state::tests::make_test_game_state("auth_ban_gate");
    let auth = crate::game_state::tests::make_test_auth("auth_ban_gate");
    let account = auth.login_npc("npc_ban_gate").unwrap();

    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let ok = finish_auth(&game_state, &auth, &mut state, account.clone(), true).await;
    assert!(
        matches!(ok.as_slice(), [ServerMessage::AuthSuccess { .. }]),
        "unbanned account authenticates: {ok:?}"
    );

    auth.ban_account(&account, Some("testing"), None).unwrap();
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let refused = finish_auth(&game_state, &auth, &mut state, account.clone(), true).await;
    match refused.as_slice() {
        [ServerMessage::AuthError { message }] => {
            assert!(
                message.contains("testing"),
                "reason reaches the client: {message}"
            )
        }
        other => panic!("expected an auth error, got {other:?}"),
    }
    assert!(
        state.account_name.is_none(),
        "a refused session must not be left holding the account"
    );

    auth.unban_account(&account).unwrap();
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let ok = finish_auth(&game_state, &auth, &mut state, account, true).await;
    assert!(
        matches!(ok.as_slice(), [ServerMessage::AuthSuccess { .. }]),
        "lifting the ban restores access: {ok:?}"
    );
}

/// The upload credential a player's own REST calls carry: issued with
/// `AuthSuccess`, good while the connection lives, gone when it ends.
#[tokio::test]
async fn finish_auth_issues_an_upload_token_that_dies_with_the_connection() {
    let game_state = crate::game_state::tests::make_test_game_state("auth_upload_token");
    let auth = crate::game_state::tests::make_test_auth("auth_upload_token");
    let account = auth.login_npc("npc_upload_token").unwrap();

    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let ok = finish_auth(&game_state, &auth, &mut state, account, true).await;
    let [ServerMessage::AuthSuccess {
        cape_upload_token, ..
    }] = ok.as_slice()
    else {
        panic!("expected an auth success, got {ok:?}");
    };
    assert_eq!(
        state.cape_upload_token.as_deref(),
        Some(cape_upload_token.as_str()),
        "the connection keeps the token it handed out"
    );

    let png = crate::test_util::test_png(8, [200, 40, 40, 255]);
    let hash = game_state
        .cape_textures()
        .store(cape_upload_token, png.clone().into())
        .await
        .expect("the token uploads");
    assert!(game_state.cape_textures().is_wearable(&hash).await);

    game_state
        .cape_textures()
        .close_session(cape_upload_token)
        .await;
    assert!(
        game_state
            .cape_textures()
            .store(cape_upload_token, png.clone().into())
            .await
            .is_err(),
        "the token dies with the connection"
    );
}

#[cfg(debug_assertions)]
#[test]
fn dev_login_requires_the_npc_token() {
    let npc_token = "0123456789abcdef";
    let secret = dev_login_secret(npc_token);
    assert_eq!(
        parse_dev_login(&format!("dev:jake:{secret}"), npc_token),
        Some("jake")
    );
    assert_eq!(
        parse_dev_login(&format!("dev:jake:{npc_token}"), npc_token),
        None
    );
    assert_eq!(parse_dev_login("dev:jake", npc_token), None);
    assert_eq!(parse_dev_login(&format!("jake:{secret}"), npc_token), None);
}

#[test]
fn operator_only_classes_are_refused_for_players() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    assert!(state
        .require_selectable_class(&CharacterClass::Merchant)
        .is_err());
    assert!(state
        .require_selectable_class(&CharacterClass::Guard)
        .is_err());
    assert!(state
        .require_selectable_class(&CharacterClass::Maid)
        .is_err());
    assert!(state
        .require_selectable_class(&CharacterClass::Ranger)
        .is_ok());

    // Operator NPCs are exactly who those classes exist for.
    state.is_official_npc = true;
    assert!(state
        .require_selectable_class(&CharacterClass::Merchant)
        .is_ok());
}
