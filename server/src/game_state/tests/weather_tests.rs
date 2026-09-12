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
    game_state.set_weather(WeatherState::new(42, 0.5, json.clone()));
    game_state.broadcast_weather();
    let msg = rx.try_recv().expect("weather broadcast");
    match rmp_serde::from_slice::<ServerMessage>(&msg.bytes).expect("decode") {
        ServerMessage::WeatherSync {
            seed,
            bias,
            sectors_tag: tag,
        } => {
            assert_eq!(seed, 42);
            assert_eq!(bias, 0.5);
            assert_eq!(tag, sectors_tag(&json));
            assert_eq!(tag.len(), 16);
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
        Some(ServerMessage::WeatherSync { seed: 777, bias, sectors_tag })
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
