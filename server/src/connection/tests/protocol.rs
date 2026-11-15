use super::*;

#[test]
fn handshake_accepts_matching_protocol_version() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let responses = handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "cli"),
        &mut state,
    );

    assert!(responses.is_some_and(|r| r.is_empty()));
    assert_eq!(state.client_kind, Some(ClientKind::Cli));
    assert!(!state.must_close);
    // Later messages pass through once the handshake is done.
    assert!(handle_handshake(&ClientMessage::Heartbeat, &mut state).is_none());
}

#[test]
fn handshake_refuses_a_stale_dungeon_layout() {
    // Both shapes a mismatch takes: a build stamped with someone else's
    // generator, and one predating the stamp entirely.
    for version in [
        onlinerpg_shared::stamp_layout_version("test")
            .replace(onlinerpg_shared::LAYOUT_VERSION, "0000000000000000"),
        "0.1.0".to_string(),
    ] {
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        let responses = handle_handshake(
            &ClientMessage::ClientInfo {
                protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
                client_kind: "cli".to_string(),
                client_version: version.clone(),
            },
            &mut state,
        );

        assert!(is_auth_error(&responses), "{version} should be refused");
        assert!(state.must_close);
        assert!(state.client_kind.is_none());
    }
}

#[test]
fn handshake_refuses_other_protocol_versions() {
    for version in [
        onlinerpg_shared::PROTOCOL_VERSION - 1,
        onlinerpg_shared::PROTOCOL_VERSION + 1,
    ] {
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        let responses = handle_handshake(&client_info(version, "cli"), &mut state);

        assert!(is_auth_error(&responses), "v{version} should be refused");
        assert!(state.must_close);
        assert!(state.client_kind.is_none());
    }
}

#[test]
fn handshake_refuses_messages_sent_before_client_info() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    let responses = handle_handshake(
        &ClientMessage::AuthenticateNpc {
            account_name: "npc_x".into(),
            npc_token: "t".into(),
        },
        &mut state,
    );

    assert!(is_auth_error(&responses));
    assert!(state.must_close);
}

#[test]
fn handshake_buckets_unknown_client_kinds() {
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "totally-made-up"),
        &mut state,
    );

    assert_eq!(state.client_kind, Some(ClientKind::Other));
}
