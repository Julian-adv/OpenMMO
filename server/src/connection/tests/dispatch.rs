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
    assert!(requires_admin(&ClientMessage::QuerySeasonalEvents));
    assert!(requires_admin(&ClientMessage::SetSeasonalEvent {
        event: "winter".into(),
        schedule: onlinerpg_shared::seasonal::SeasonSchedule::Off,
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
        "/spawnmob kobold",
        "/weather",
        "/weather rain",
        "  /weather rain 0.4  ",
        "/weather snow",
        "/weather clear",
        "/weather auto",
        "/weather invalid",
        "/season",
        "/season winter 12-01 01-10",
        "/season winter off",
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
    assert!(!requires_admin(&ClientMessage::ChatMessage {
        message: "/seasoning".into()
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
