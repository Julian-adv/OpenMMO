use super::*;
use crate::game_state::weather::WeatherState;

#[tokio::test]
async fn weather_is_silent_until_loaded_then_broadcasts_seed_and_bias() {
    let game_state = make_test_game_state("weather_broadcast");
    let mut rx = game_state.subscribe();

    assert!(game_state.weather_sync_message().is_none());
    game_state.broadcast_weather();
    assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));

    game_state.set_weather(WeatherState {
        seed: 42,
        bias: 0.5,
    });
    game_state.broadcast_weather();
    let msg = rx.try_recv().expect("weather broadcast");
    match rmp_serde::from_slice::<ServerMessage>(&msg.bytes).expect("decode") {
        ServerMessage::WeatherSync { seed, bias } => {
            assert_eq!(seed, 42);
            assert_eq!(bias, 0.5);
        }
        other => panic!("Expected WeatherSync, got {:?}", other),
    }
}

#[tokio::test]
async fn load_weather_reads_the_baked_seed() {
    let game_state = make_test_game_state("weather_load");
    let sectors = onlinerpg_shared::weather::WeatherSectors {
        version: onlinerpg_shared::weather::WEATHER_SECTORS_VERSION,
        seed: 777,
        sectors: vec![],
    };
    let base = game_state.terrain_io.base_dir().clone();
    tokio::fs::create_dir_all(&base).await.unwrap();
    tokio::fs::write(
        onlinerpg_terrain::coords::weather_sectors_path(&base),
        serde_json::to_vec(&sectors).unwrap(),
    )
    .await
    .unwrap();

    game_state.load_weather().await;
    assert!(matches!(
        game_state.weather_sync_message(),
        Some(ServerMessage::WeatherSync { seed: 777, bias }) if bias == 1.0
    ));
}
