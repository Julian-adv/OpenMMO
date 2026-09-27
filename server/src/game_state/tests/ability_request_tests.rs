use super::*;
use crate::connection::{handle_connection, AuthContext, ServerContext};
use futures_util::{SinkExt, StreamExt};
use onlinerpg_shared::ability::AbilityId;
use onlinerpg_shared::ClientMessage;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

#[tokio::test]
async fn double_slash_use_ability_over_websocket_deals_both_strikes() {
    double_slash_request(false).await;
}

#[tokio::test]
async fn double_slash_use_ability_over_websocket_allows_movement_between_strikes() {
    double_slash_request(true).await;
}

async fn receive(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>) -> Vec<ServerMessage> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Message::Binary(bytes) = socket.next().await.unwrap().unwrap() {
                return match onlinerpg_shared::deserialize_server_msg(&bytes).unwrap() {
                    ServerMessage::WorldUpdate { events, .. } => events
                        .into_iter()
                        .flat_map(|event| event.messages)
                        .collect(),
                    message => vec![message],
                };
            }
        }
    })
    .await
    .expect("server message timed out")
}

async fn send(socket: &mut WebSocketStream<MaybeTlsStream<TcpStream>>, message: ClientMessage) {
    socket
        .send(Message::Binary(
            onlinerpg_shared::serialize_client_msg(&message)
                .unwrap()
                .into(),
        ))
        .await
        .unwrap();
}

async fn double_slash_request(interrupt: bool) {
    let label = format!("double_slash_websocket_{interrupt}");
    let game = Arc::new(make_flat_world_game_state(&label));
    let auth = Arc::new(make_test_auth(&label));
    let account = auth.login_npc("npc_dagger_tester").unwrap();
    let character = auth
        .create_character(
            &account,
            "Slasher",
            &attrs_with_cha(12),
            100,
            CharacterClass::Rogue,
            Gender::Male,
        )
        .unwrap();
    let context = Arc::new(ServerContext {
        geoip: Default::default(),
        game_state: game.clone(),
        auth_service: auth,
        auth_ctx: Arc::new(AuthContext {
            google: None,
            npc_token: "test-token".into(),
            admin_emails: vec![],
        }),
        connect_limiter: Default::default(),
    });
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (_shutdown_tx, shutdown) = watch::channel(());
    let server = tokio::spawn(async move {
        let (stream, peer) = listener.accept().await.unwrap();
        handle_connection(stream, peer, context, shutdown.clone(), shutdown).await;
    });
    let (mut socket, _) = connect_async(format!("ws://{address}")).await.unwrap();
    for message in [
        ClientMessage::ClientInfo {
            protocol_version: onlinerpg_shared::PROTOCOL_VERSION,
            client_kind: "web".into(),
            client_version: onlinerpg_shared::stamp_layout_version("test"),
        },
        ClientMessage::AuthenticateNpc {
            account_name: "npc_dagger_tester".into(),
            npc_token: "test-token".into(),
        },
        ClientMessage::EnterGame {
            character_id: character.id,
        },
    ] {
        send(&mut socket, message).await;
    }
    let id = loop {
        if let Some(id) = receive(&mut socket).await.into_iter().find_map(|message| {
            if let ServerMessage::JoinSuccess { player, .. } = message {
                Some(player.id)
            } else {
                None
            }
        }) {
            break id;
        }
    };
    send(&mut socket, ClientMessage::WorldReady).await;
    let origin = Position {
        x: 800.0,
        y: 5.0,
        z: 800.0,
    };
    game.teleport_player(&id, origin, 0.0, 0).await;
    let mut inventory = PlayerInventory::default();
    inventory
        .equipped
        .insert(EquipSlot::MainHand, bag_item(100, "dagger", 1));
    game.inventories.write().await.insert(id, inventory);
    let mut monster = make_monster(
        "target",
        Position {
            x: origin.x + 1.0,
            ..origin
        },
        0,
    );
    monster.health = 500;
    monster.max_health = 500;
    game.monsters
        .write()
        .await
        .insert(monster.id.clone(), monster);
    send(
        &mut socket,
        ClientMessage::UseAbility {
            ability: AbilityId::DaggerDoubleSlash,
            monster_id: Some("target".into()),
            target_player_id: None,
        },
    )
    .await;
    let mut started = false;
    let mut strikes = Vec::new();
    let mut interrupted = false;
    while strikes.len() < 2 && !interrupted {
        for message in receive(&mut socket).await {
            match message {
                ServerMessage::DaggerDoubleSlashStarted { player_id, .. } if player_id == id => {
                    started = true;
                }
                ServerMessage::PlayerAttacked {
                    player_id,
                    dagger_strike: Some(strike),
                    ..
                } if player_id == id => {
                    assert!(started);
                    strikes.push(strike);
                    if interrupt && strike == 1 {
                        send(
                            &mut socket,
                            ClientMessage::PlayerMoveDirection {
                                request_id: 1,
                                rotation: 0.0,
                                forward: 1,
                                turn: 0,
                                sprinting: false,
                            },
                        )
                        .await;
                    }
                }
                ServerMessage::DaggerDoubleSlashSkipped { strike, reason, .. } => {
                    assert!(interrupt);
                    assert_eq!(strike, 2);
                    assert_eq!(reason, "interrupted");
                    interrupted = true;
                }
                ServerMessage::DaggerDoubleSlashRejected { .. }
                | ServerMessage::AbilityRejected { .. } => {
                    panic!("unexpected rejection: {message:?}")
                }
                _ => {}
            }
        }
    }
    assert_eq!(strikes, if interrupt { vec![1] } else { vec![1, 2] });
    assert_eq!(interrupted, interrupt);
    socket.close(None).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}
