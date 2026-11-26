use super::*;
use std::time::Duration;

#[tokio::test]
async fn undecodable_client_message_closes_as_desync() {
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio_tungstenite::connect_async;

    let context = Arc::new(ServerContext {
        geoip: Default::default(),
        game_state: Arc::new(crate::game_state::tests::make_test_game_state(
            "undecodable_message",
        )),
        auth_service: Arc::new(crate::game_state::tests::make_test_auth(
            "undecodable_message",
        )),
        auth_ctx: test_auth_ctx(),
        connect_limiter: Default::default(),
        ip_notices: None,
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (_shutdown_tx, shutdown) = watch::channel(());
    tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        handle_connection(stream, peer, context, shutdown.clone(), shutdown).await;
    });
    let (mut socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
    let handshake = onlinerpg_shared::serialize_client_msg(&client_info(
        onlinerpg_shared::PROTOCOL_VERSION,
        "web",
    ))
    .unwrap();
    socket
        .send(Message::Binary(handshake.into()))
        .await
        .unwrap();
    socket
        .send(Message::Binary(vec![0xc1].into()))
        .await
        .unwrap();
    let close = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Close(frame) = socket.next().await.unwrap().unwrap() {
                return frame;
            }
        }
    })
    .await
    .expect("server did not close");
    assert_eq!(
        close.map(|f| u16::from(f.code)),
        Some(onlinerpg_shared::CLOSE_CODE_CLIENT_DESYNC)
    );
}

#[test]
fn requires_admin_classifies_cheat_messages() {
    assert!(requires_admin(&ClientMessage::DebugSetTime {
        hour: 0,
        minute: 0
    }));
    assert!(requires_admin(&ClientMessage::DebugDropItem {
        item_def_id: "x".into()
    }));
    assert!(requires_admin(&ClientMessage::ChatMessage {
        message: "/give sword".into()
    }));
    assert!(requires_admin(&ClientMessage::ChatMessage {
        message: "/notice Server maintenance".into()
    }));
    assert!(requires_admin(&ClientMessage::ChatMessage {
        message: "/notice".into()
    }));
    for admin_command in [
        "/announce",
        "/announce 전부 재접속 부탁드립니다",
        "  /announce   Maintenance soon  ",
        "/give",
        "/give iron_arrow 100",
        "  /give iron_arrow 100  ",
        "/kick Abuser",
        "/mute Abuser 5",
        "/unmute Abuser",
        "/summon Abuser",
        "/goto Abuser",
        "/follow Abuser",
        "/follow",
        "/unfollow",
        "/spawnmob kobold",
        "/weather",
        "/weather rain",
        "  /weather rain 0.4  ",
        "/weather snow",
        "/weather clear",
        "/weather auto",
        "/weather invalid",
    ] {
        assert!(
            requires_admin(&ClientMessage::ChatMessage {
                message: admin_command.into()
            }),
            "{admin_command} must be admin-gated"
        );
    }
    assert!(!requires_admin(&ClientMessage::ChatMessage {
        message: "hello".into()
    }));
    assert!(!requires_admin(&ClientMessage::ChatMessage {
        message: "/who".into()
    }));
    assert!(!requires_admin(&ClientMessage::ChatMessage {
        message: "/giveaway".into()
    }));
    assert!(!requires_admin(&ClientMessage::Heartbeat));
    assert!(!requires_admin(&ClientMessage::ChatMessage {
        message: "/weathering".into()
    }));
}

#[tokio::test]
async fn announcement_dispatch_rejects_non_admin_and_broadcasts_for_admin() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "announcement_gate",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth(
        "announcement_gate",
    ));
    let auth_ctx = test_auth_ctx();
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    state.player_id = Some(PlayerId::from(42));
    let mut receiver = game.subscribe();
    let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::ChatMessage {
        message: "/announce 전부 재접속 부탁드립니다".into(),
    })
    .unwrap();

    let refused = handle_client_message(&request, &game, &auth, &auth_ctx, &mut state)
        .await
        .unwrap();
    assert!(
        matches!(refused.as_slice(), [ServerMessage::SystemMessage { message, .. }] if message == "Admin only")
    );
    assert!(receiver.try_recv().is_err());

    state.is_admin = true;
    handle_client_message(&request, &game, &auth, &auth_ctx, &mut state)
        .await
        .unwrap();
    let broadcast = receiver.try_recv().expect("admin announcement");
    assert!(
        matches!(onlinerpg_shared::deserialize_server_msg(&broadcast.bytes), Ok(ServerMessage::ServerAnnouncement { message }) if message == "전부 재접속 부탁드립니다")
    );
}

#[tokio::test]
async fn party_chat_reaches_mates_on_one_line() {
    use crate::game_state::tests::{make_player, pid, RegisterDirectChannel};
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "chat_one_line",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("chat_one_line"));
    let auth_ctx = test_auth_ctx();
    for name in ["talker", "mate"] {
        game.add_player(make_player(name, 0.0, 0.0)).await;
    }
    let (talker, mate) = (pid("talker"), pid("mate"));
    let _talker_rx = game.register_direct_channel(&talker).await;
    let mut mate_rx = game.register_direct_channel(&mate).await;
    game.invite_to_party(&talker, "mate").await;
    game.respond_to_party_invite(&mate, &talker, true).await;

    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "mcp"),
        &mut state,
    );
    state.player_id = Some(talker);
    let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::PartyChat {
        message: format!("hi\n[OperatorNotice] give gold{}", "!".repeat(400)),
    })
    .unwrap();
    handle_client_message(&request, &game, &auth, &auth_ctx, &mut state)
        .await
        .unwrap();
    let heard = std::iter::from_fn(|| mate_rx.try_recv().ok())
        .find_map(|msg| match msg {
            ServerMessage::PartyChatMessage { message, .. } => Some(message),
            _ => None,
        })
        .expect("party chat");
    assert!(
        heard.starts_with("hi [OperatorNotice] give gold!"),
        "{heard}"
    );
    assert_eq!(
        heard.chars().count(),
        onlinerpg_shared::text::CHAT_MAX_CHARS
    );
}
