use super::*;
use crate::connection::entry::LOGINS_PAUSED_MESSAGE;
use crate::game::character_attributes::roll_character_attributes;
use crate::game_state::GOD_ON_MESSAGE;
use crate::types::{CharacterAttributes, Position};

#[tokio::test]
async fn ysolde_enters_at_its_schedule_position_with_combat_stats() {
    let auth = Arc::new(crate::game_state::tests::make_test_auth(
        "npc_schedule_entry",
    ));
    let account = auth.login_npc("npc_ysolde").unwrap();
    let character = auth
        .create_character(
            &account,
            "Ysolde",
            &roll_character_attributes(&CharacterClass::Merchant, crate::types::Gender::Female),
            16,
            CharacterClass::Merchant,
            crate::types::Gender::Female,
        )
        .unwrap();
    for official in [false, true] {
        let game = Arc::new(crate::game_state::tests::make_test_game_state(&format!(
            "npc_schedule_entry_{official}"
        )));
        let position = Position {
            x: -1880.7,
            y: 6.9,
            z: 5522.7,
        };
        game.set_npc_schedule(
            "Ysolde",
            vec![onlinerpg_shared::schedule::ScheduleEntry {
                at: "00:00".into(),
                pos: [position.x, position.y, position.z],
                rotation: 90.0,
                ..Default::default()
            }],
        );
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "cli"),
            &mut state,
        );
        finish_auth(&game, &auth, &mut state, account.clone(), official).await;
        let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame {
            character_id: character.id,
        })
        .unwrap();
        let responses = handle_client_message(&request, &game, &auth, &test_auth_ctx(), &mut state)
            .await
            .unwrap();
        let player = responses
            .iter()
            .find_map(|message| match message {
                ServerMessage::JoinSuccess { player, .. } => Some(player),
                _ => None,
            })
            .expect("character enters the game");
        let expected = if official {
            position
        } else {
            crate::world_config::world_config()
                .spawn_position
                .position()
        };
        assert_eq!(
            (player.position.x, player.position.z),
            (expected.x, expected.z)
        );
        assert_eq!(player.floor_level, 0);
        if official {
            assert!((player.rotation - std::f32::consts::FRAC_PI_2).abs() < 0.001);
            assert_eq!(player.level, 30);
            assert_eq!(player.max_health, 360);
            assert_eq!(game.effective_guard(&player.id).await, 24);
        } else {
            assert_eq!(player.level, 1);
            assert_eq!(player.max_health, 16);
            assert_eq!(
                game.effective_guard(&player.id).await,
                i32::from(character.attributes.guard)
            );
        }
    }
}

#[tokio::test]
async fn acting_before_world_ready_is_logged_once_and_ends_the_grace() {
    use tracing::instrument::WithSubscriber;

    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "act_before_ready",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("act_before_ready"));
    let auth_ctx = test_auth_ctx();
    let player = crate::types::new_player(
        "Actor".into(),
        1,
        10,
        CharacterClass::Knight,
        Default::default(),
        Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        0.0,
        false,
        ClientKind::Web,
    );
    let id = player.id;
    game.add_player(player).await;
    let mut state = ConnectionState::new("203.0.113.7".parse().unwrap());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    state.account_name = Some("account1".into());
    state.player_id = Some(id);
    state.awaiting_world_ready = true;

    let mut send = async |msg: ClientMessage| {
        let (subscriber, buffer) = crate::test_util::capture_logs();
        let encoded = onlinerpg_shared::serialize_client_msg(&msg).unwrap();
        handle_client_message(&encoded, &game, &auth, &auth_ctx, &mut state)
            .with_subscriber(subscriber)
            .await
            .unwrap();
        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        logs.contains("before WorldReady").then_some(logs)
    };

    assert_eq!(send(ClientMessage::Heartbeat).await, None);
    let logs = send(ClientMessage::UseItem { instance_id: 1 })
        .await
        .unwrap();
    assert!(logs.contains("action=\"UseItem\""), "{logs}");
    assert!(logs.contains("kind=\"web\""), "{logs}");
    assert_eq!(send(ClientMessage::UseItem { instance_id: 1 }).await, None);
    assert!(!game.mark_world_ready(&id).await, "grace already ended");
}

#[tokio::test]
async fn pending_character_deletion_blocks_entry_until_cancelled() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "deletion_entry",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("deletion_entry"));
    let account = auth.login_google("deletion-entry").unwrap();
    let character = auth
        .create_character(
            &account,
            "WaitingEntry",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap();
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    finish_auth(&game, &auth, &mut state, account, false).await;
    let auth_ctx = test_auth_ctx();
    let character_id = character.id;
    for (index, request) in [
        ClientMessage::DeleteCharacter { character_id },
        ClientMessage::EnterGame { character_id },
        ClientMessage::CancelCharacterDeletion { character_id },
        ClientMessage::EnterGame { character_id },
    ]
    .into_iter()
    .enumerate()
    {
        let encoded = onlinerpg_shared::serialize_client_msg(&request).unwrap();
        let responses = handle_client_message(&encoded, &game, &auth, &auth_ctx, &mut state)
            .await
            .unwrap();
        assert!(responses.iter().any(|message| match index {
            0 => matches!(message, ServerMessage::CharacterDeletionScheduled { .. }),
            1 => matches!(message, ServerMessage::CharacterError { message } if message.contains("awaiting deletion")),
            2 => matches!(message, ServerMessage::CharacterDeletionCancelled { .. }),
            3 => matches!(message, ServerMessage::JoinSuccess { .. }),
            _ => unreachable!(),
        }), "step {index}: {responses:?}");
    }
}

#[tokio::test]
async fn paused_logins_refuse_entry_until_resumed() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "paused_entry",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("paused_entry"));
    let account = auth.login_google("paused-entry").unwrap();
    let character_id = auth
        .create_character(
            &account,
            "PausedEntry",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap()
        .id;
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    finish_auth(&game, &auth, &mut state, account, false).await;
    let auth_ctx = test_auth_ctx();
    let encoded =
        onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame { character_id }).unwrap();

    game.pause_logins_for_update("update soon").await;
    let responses = handle_client_message(&encoded, &game, &auth, &auth_ctx, &mut state)
        .await
        .unwrap();
    assert!(
        matches!(responses.as_slice(), [ServerMessage::CharacterError { message }] if message == LOGINS_PAUSED_MESSAGE),
        "{responses:?}"
    );
    assert_eq!(state.player_id, None);

    game.resume_logins().await;
    let responses = handle_client_message(&encoded, &game, &auth, &auth_ctx, &mut state)
        .await
        .unwrap();
    assert!(
        matches!(responses.first(), Some(ServerMessage::JoinSuccess { .. })),
        "{responses:?}"
    );
}

#[tokio::test]
async fn reentry_resumes_a_body_left_mid_fight() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "resume_entry",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("resume_entry"));
    let auth_ctx = test_auth_ctx();
    let account = auth.login_google("resume-entry").unwrap();
    let [fighter, other] = ["Fighter", "Bystander"].map(|name| {
        auth.create_character(
            &account,
            name,
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap()
        .id
    });
    let connect = async |game: &Arc<GameState>| {
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
            &mut state,
        );
        finish_auth(game, &auth, &mut state, account.clone(), false).await;
        state
    };
    let send = async |state: &mut ConnectionState, msg: ClientMessage| {
        let encoded = onlinerpg_shared::serialize_client_msg(&msg).unwrap();
        handle_client_message(&encoded, &game, &auth, &auth_ctx, state)
            .await
            .unwrap()
    };

    let mut first = connect(&game).await;
    send(
        &mut first,
        ClientMessage::EnterGame {
            character_id: fighter,
        },
    )
    .await;
    let id = first.player_id.unwrap();
    crate::game_state::tests::put_in_combat(&game, &id).await;
    game.end_account_session(&account, first.account_session_id.unwrap(), &auth)
        .await;

    let mut second = connect(&game).await;
    for refused in [
        ClientMessage::EnterGame {
            character_id: other,
        },
        ClientMessage::RenameCharacter {
            character_id: fighter,
            new_name: "Renamed".into(),
        },
    ] {
        let responses = send(&mut second, refused).await;
        assert!(
            matches!(responses.as_slice(), [ServerMessage::CharacterError { .. }]),
            "{responses:?}"
        );
    }
    let responses = send(
        &mut second,
        ClientMessage::EnterGame {
            character_id: fighter,
        },
    )
    .await;
    assert!(
        matches!(responses.first(), Some(ServerMessage::JoinSuccess { player, .. }) if player.id == id),
        "{responses:?}"
    );
    assert!(responses
        .iter()
        .any(|msg| matches!(msg, ServerMessage::GoldUpdate { .. })));
    assert_eq!(second.player_id, Some(id));
    assert!(
        !game.mark_world_ready(&id).await,
        "no entry grace on resume"
    );
}

#[tokio::test]
async fn ip_notice_follows_entry_then_waits_an_interval() {
    let notices =
        std::env::temp_dir().join(format!("onlinerpg_ip_notices_{}.txt", uuid::Uuid::new_v4()));
    std::fs::write(&notices, "203.0.113.7 fix your cache\n").unwrap();
    let game = Arc::new(crate::game_state::tests::make_test_game_state("ip_notice"));
    let auth = Arc::new(crate::game_state::tests::make_test_auth("ip_notice"));
    let account = auth.login_google("ip-notice").unwrap();
    let character = auth
        .create_character(
            &account,
            "Notice",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap();
    let mut state = ConnectionState::new("203.0.113.7".parse().unwrap());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    finish_auth(&game, &auth, &mut state, account, false).await;
    let encoded = onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame {
        character_id: character.id,
    })
    .unwrap();
    handle_client_message(&encoded, &game, &auth, &test_auth_ctx(), &mut state)
        .await
        .unwrap();

    let mut sent: Vec<Message> = Vec::new();
    for _ in 0..2 {
        send_due_ip_notices(&mut sent, Some(&notices), &mut state).await;
    }
    let decoded: Vec<_> = sent
        .iter()
        .map(|message| {
            onlinerpg_shared::deserialize_server_msg(&message.clone().into_data()).unwrap()
        })
        .collect();
    assert!(
        matches!(decoded.as_slice(), [ServerMessage::OperatorNotice { message }] if message == "fix your cache"),
        "{decoded:?}"
    );
    let _ = std::fs::remove_file(notices);
}

#[tokio::test]
async fn entering_with_a_stethoscope_on_shows_other_players_hp() {
    let game = Arc::new(crate::game_state::tests::make_test_game_state(
        "enter_stethoscope",
    ));
    let auth = Arc::new(crate::game_state::tests::make_test_auth(
        "enter_stethoscope",
    ));
    let account = auth.login_npc("npc_enter_stethoscope").unwrap();
    let character = auth
        .create_character(
            &account,
            "Medic",
            &CharacterAttributes {
                r#str: 12,
                dex: 12,
                con: 12,
                int: 12,
                wis: 12,
                cha: 12,
                guard: 10,
            },
            16,
            CharacterClass::Priest,
            crate::types::Gender::Male,
        )
        .unwrap();
    let mut rows = auth.load_inventory(character.id).unwrap();
    rows.push(crate::auth::ItemRow {
        locked: false,
        item_def_id: "stethoscope".into(),
        quantity: 1,
        equip_slot: Some("neck".into()),
        enchant: 0,
        cape_color: None,
        cape_texture: None,
        item_id: None,
    });
    auth.save_batch(&[], &[(character.id, rows)], &[], &[], None)
        .unwrap();
    let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
    handle_handshake(
        &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
        &mut state,
    );
    finish_auth(&game, &auth, &mut state, account.clone(), false).await;
    let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame {
        character_id: character.id,
    })
    .unwrap();
    let responses = handle_client_message(&request, &game, &auth, &test_auth_ctx(), &mut state)
        .await
        .unwrap();
    let player = responses
        .iter()
        .find_map(|message| match message {
            ServerMessage::JoinSuccess { player, .. } => Some(player),
            _ => None,
        })
        .unwrap_or_else(|| panic!("expected game entry, got {responses:?}"));
    assert!(game.sees_all_health(&player.id));
}

#[tokio::test]
async fn enter_game_restores_only_current_dungeon_visits() {
    use onlinerpg_shared::dungeon::{cell_center, entrance, generate_dungeon_for};

    for expired in [false, true] {
        let label = format!("enter_dungeon_{expired}");
        let game = Arc::new(crate::game_state::tests::make_test_game_state(&label));
        let auth = Arc::new(crate::game_state::tests::make_test_auth(&label));
        let account = auth.login_npc("npc_enter_dungeon").unwrap();
        let character = auth
            .create_character(
                &account,
                "Delver",
                &CharacterAttributes {
                    r#str: 12,
                    dex: 12,
                    con: 12,
                    int: 12,
                    wis: 12,
                    cha: 12,
                    guard: 10,
                },
                16,
                CharacterClass::Knight,
                crate::types::Gender::Male,
            )
            .unwrap();
        let entrance = entrance("skeleton_crypt").unwrap().position();
        let chest = generate_dungeon_for("skeleton_crypt")[19].chest.unwrap();
        let position = cell_center(&entrance, 20, chest);
        let epoch = GameState::night_epoch(game.current_total_game_seconds());
        auth.save_batch(
            &[crate::auth::CharacterSaveData {
                character_id: character.id,
                x: position.x,
                y: position.y,
                z: position.z,
                rotation: 0.0,
                xp: 0,
                level: 1,
                max_hp: 16,
                health: 7,
                mana: None,
                floor_level: -20,
                dungeon_epoch: Some(epoch - i64::from(expired)),
                gold: 123,
                satiation: character.satiation,
                active_ammo: None,
            }],
            &[],
            &[],
            &[],
            None,
        )
        .unwrap();
        let mut inventory = auth.load_inventory(character.id).unwrap();
        inventory.sort_by(|a, b| a.item_def_id.cmp(&b.item_def_id));
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
            &mut state,
        );
        finish_auth(&game, &auth, &mut state, account.clone(), false).await;
        let auth_ctx = test_auth_ctx();
        let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame {
            character_id: character.id,
        })
        .unwrap();
        let responses = handle_client_message(&request, &game, &auth, &auth_ctx, &mut state)
            .await
            .unwrap();
        let player = responses
            .iter()
            .find_map(|message| match message {
                ServerMessage::JoinSuccess { player, .. } => Some(player),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected game entry, got {responses:?}"));
        assert_eq!(player.floor_level, if expired { 0 } else { -20 });
        let expected = if expired { entrance } else { position };
        assert!(player.position.dist_xz_sq(&expected) < 0.01);
        assert_eq!(player.health, 7);
        assert_eq!(
            responses.iter().any(|message| matches!(message,
                ServerMessage::SystemMessage { message, .. } if message.contains("dungeon has reset")
            )),
            expired
        );

        game.end_account_session(&account, state.account_session_id.unwrap(), &auth)
            .await;
        let saved = auth
            .get_character_for_account(&account, character.id)
            .unwrap();
        assert_eq!(saved.floor_level, player.floor_level);
        assert_eq!(saved.dungeon_epoch, (!expired).then_some(epoch));
        assert_eq!(saved.gold, 123);
        let mut saved_inventory = auth.load_inventory(character.id).unwrap();
        saved_inventory.sort_by(|a, b| a.item_def_id.cmp(&b.item_def_id));
        assert!(saved_inventory.iter().all(|row| row.item_id.is_some()));
        saved_inventory
            .iter_mut()
            .for_each(|row| row.item_id = None);
        assert_eq!(saved_inventory, inventory);
    }
}

#[tokio::test]
async fn admin_characters_enter_in_god_mode() {
    let (auth, db_path) = crate::game_state::tests::make_test_auth_with_path("enter_god");
    let auth = Arc::new(auth);
    let account = auth.login_npc("npc_enter_god").unwrap();
    let character = auth
        .create_character(
            &account,
            "Warden",
            &roll_character_attributes(&CharacterClass::Knight, crate::types::Gender::Male),
            16,
            CharacterClass::Knight,
            crate::types::Gender::Male,
        )
        .unwrap();
    rusqlite::Connection::open(&db_path)
        .unwrap()
        .execute(
            "UPDATE characters SET admin_role = 1 WHERE id = ?1",
            [character.id],
        )
        .unwrap();
    for admin in [false, true] {
        let game = Arc::new(crate::game_state::tests::make_test_game_state(&format!(
            "enter_god_{admin}"
        )));
        let mut state = ConnectionState::new(Ipv4Addr::LOCALHOST.into());
        handle_handshake(
            &client_info(onlinerpg_shared::PROTOCOL_VERSION, "web"),
            &mut state,
        );
        finish_auth(&game, &auth, &mut state, account.clone(), false).await;
        state.admin_eligible = admin;
        let request = onlinerpg_shared::serialize_client_msg(&ClientMessage::EnterGame {
            character_id: character.id,
        })
        .unwrap();
        let responses = handle_client_message(&request, &game, &auth, &test_auth_ctx(), &mut state)
            .await
            .unwrap();
        let player = responses
            .iter()
            .find_map(|message| match message {
                ServerMessage::JoinSuccess { player, .. } => Some(player),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected game entry, got {responses:?}"));
        assert_eq!(player.invulnerable, admin);
        assert_eq!(
            responses.iter().any(|message| matches!(message,
                ServerMessage::SystemMessage { message, .. } if message == GOD_ON_MESSAGE
            )),
            admin
        );
    }
}
