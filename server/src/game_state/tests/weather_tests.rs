use super::*;
use crate::game_state::weather::{sectors_tag, WeatherState};

#[tokio::test]
async fn weather_is_silent_until_loaded_then_broadcasts_seed_bias_and_tag() {
    let game_state = make_test_game_state("weather_broadcast");
    let mut rx = game_state.subscribe();

    assert!(game_state.weather_sync_message().is_none());
    assert!(game_state.weather_sectors_json().is_none());
    game_state.broadcast_weather();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    let json = br#"{"version":1,"seed":42,"sectors":[]}"#.to_vec();
    game_state.set_weather(WeatherState::new(
        serde_json::from_slice(&json).unwrap(),
        0.5,
        json.clone(),
    ));
    game_state.broadcast_weather();
    let msg = rx.try_recv().expect("weather broadcast");
    match rmp_serde::from_slice::<ServerMessage>(&msg.bytes).expect("decode") {
        ServerMessage::WeatherSync {
            seed,
            bias,
            sectors_tag: tag,
            rain_override,
            snow_override,
        } => {
            assert_eq!(seed, 42);
            assert_eq!(bias, 0.5);
            assert_eq!(tag, sectors_tag(&json));
            assert_eq!(tag.len(), 16);
            assert_eq!(rain_override, None);
            assert!(!snow_override);
        }
        other => panic!("Expected WeatherSync, got {:?}", other),
    }
    assert_eq!(
        game_state.weather_sectors_json().as_deref(),
        Some(json.as_slice())
    );
}

#[tokio::test]
async fn load_weather_reads_the_baked_seed_and_keeps_the_file_bytes() {
    let game_state = make_test_game_state("weather_load");
    let sectors = onlinerpg_shared::weather::WeatherSectors {
        version: onlinerpg_shared::weather::WEATHER_SECTORS_VERSION,
        seed: 777,
        sectors: vec![],
    };
    let base = game_state.terrain_io.base_dir().clone();
    tokio::fs::create_dir_all(&base).await.unwrap();
    let json = serde_json::to_vec(&sectors).unwrap();
    tokio::fs::write(
        onlinerpg_terrain::coords::weather_sectors_path(&base),
        &json,
    )
    .await
    .unwrap();

    game_state.load_weather(0.75).await;
    assert!(matches!(
        game_state.weather_sync_message(),
        Some(ServerMessage::WeatherSync { seed: 777, bias, sectors_tag, rain_override: None, snow_override: false })
            if bias == 0.75 && sectors_tag == self::sectors_tag(&json)
    ));
    assert_eq!(
        game_state.weather_sectors_json().as_deref(),
        Some(json.as_slice())
    );
}

#[test]
fn sectors_tag_follows_the_content_not_the_file() {
    assert_eq!(sectors_tag(b"a"), sectors_tag(b"a"));
    assert_ne!(sectors_tag(b"a"), sectors_tag(b"b"));
}

#[tokio::test]
async fn weather_commands_broadcast_overrides_and_preserve_the_automatic_schedule() {
    let game_state = make_test_game_state("weather_commands");
    let auth = make_test_auth("weather_commands");
    let admin_id = pid("admin");
    game_state.add_player(make_player("admin", 0.0, 0.0)).await;
    let mut direct = game_state.register_direct_channel(&admin_id).await;
    let mut broadcast = game_state.subscribe();
    let json = br#"{"version":1,"seed":42,"sectors":[]}"#.to_vec();
    game_state.set_weather(WeatherState::new(
        serde_json::from_slice(&json).unwrap(),
        0.5,
        json.clone(),
    ));

    for (command, expected, expected_snow) in [
        ("/weather rain", Some(1.0), false),
        ("  /weather rain 0.4  ", Some(0.4), false),
        ("/weather snow", Some(1.0), true),
        ("/weather snow 0.3", Some(0.3), true),
        ("/weather clear", Some(0.0), false),
        ("/weather snow 0.5", Some(0.5), true),
        ("/weather rain 0", Some(0.0), false),
        ("/weather snow 0", Some(0.0), false),
        ("/weather snow 0.5", Some(0.5), true),
        ("/weather auto", None, false),
    ] {
        game_state
            .send_chat_message(&admin_id, command.into(), &auth)
            .await;
        let payload = broadcast.try_recv().expect("immediate weather broadcast");
        assert!(
            matches!(
                rmp_serde::from_slice::<ServerMessage>(&payload.bytes).unwrap(),
                ServerMessage::WeatherSync { seed: 42, bias: 0.5, sectors_tag, rain_override, snow_override }
                    if sectors_tag == self::sectors_tag(&json)
                        && rain_override == expected
                        && snow_override == expected_snow
            ),
            "{command}"
        );
        assert!(matches!(broadcast.try_recv(), Err(TryRecvError::Empty)));
        assert!(matches!(drain(&mut direct).as_slice(),
            [ServerMessage::SystemMessage { message, .. }] if message.starts_with("Weather:")
        ));
        let sync = game_state.weather_sync_message().unwrap();
        assert_eq!(
            onlinerpg_shared::serialize_server_msg(&sync).unwrap(),
            payload.bytes
        );
        assert_eq!(
            game_state.weather_sectors_json().as_deref(),
            Some(json.as_slice())
        );
    }
}

#[tokio::test]
async fn invalid_weather_commands_do_not_change_or_broadcast_weather() {
    let game_state = make_test_game_state("weather_invalid");
    let auth = make_test_auth("weather_invalid");
    let admin_id = pid("admin");
    game_state.add_player(make_player("admin", 0.0, 0.0)).await;
    let mut direct = game_state.register_direct_channel(&admin_id).await;
    let json = br#"{"version":1,"seed":42,"sectors":[]}"#.to_vec();
    game_state.set_weather(WeatherState::new(
        serde_json::from_slice(&json).unwrap(),
        0.5,
        json,
    ));
    game_state.weather_command(&admin_id, "rain 0.4").unwrap();
    let before = game_state.weather.read().unwrap().clone();
    let mut broadcast = game_state.subscribe();

    for command in [
        "/weather",
        "/weather hail",
        "/weather snow nope",
        "/weather snow 1.1",
        "/weather snow 0.5 extra",
        "/weather rain nope",
        "/weather rain NaN",
        "/weather rain inf",
        "/weather rain -0.1",
        "/weather rain 1.1",
        "/weather rain 0.5 extra",
        "/weather clear 1",
        "/weather auto extra",
    ] {
        game_state
            .send_chat_message(&admin_id, command.into(), &auth)
            .await;
        assert_eq!(*game_state.weather.read().unwrap(), before, "{command}");
        assert!(
            matches!(broadcast.try_recv(), Err(TryRecvError::Empty)),
            "{command}"
        );
        assert!(
            matches!(drain(&mut direct).as_slice(),
                [ServerMessage::SystemMessage { message, .. }] if message.starts_with("Weather:")
            ),
            "{command}"
        );
    }
}

#[test]
fn weather_override_requires_loaded_weather_data() {
    let game_state = make_test_game_state("weather_unloaded");
    assert_eq!(
        game_state
            .weather_command(&pid("admin"), "rain")
            .unwrap_err(),
        "Weather: weather data is not loaded."
    );
    assert!(game_state.weather_sync_message().is_none());
}
