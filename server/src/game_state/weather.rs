//! Regional weather (doc/WEATHER_SYSTEM.md). The server only hands clients
//! the seed the rain sectors were baked with; rain itself is evaluated from
//! `onlinerpg_shared::weather` on both sides.

use onlinerpg_shared::messages::ServerMessage;
use tracing::{info, warn};

use super::GameState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeatherState {
    pub seed: u64,
    /// Multiplier on every zone's rain chance; 1.0 is the baked schedule.
    pub bias: f32,
}

impl GameState {
    /// Weather stays off until a bake has produced `weather-sectors.json`.
    pub async fn load_weather(&self) {
        match self.terrain_io.read_weather_sectors().await {
            Ok(Some(sectors)) => {
                info!(
                    "weather: {} rain sectors, seed {}",
                    sectors.sectors.len(),
                    sectors.seed
                );
                self.set_weather(WeatherState {
                    seed: sectors.seed,
                    bias: 1.0,
                });
            }
            Ok(None) => {
                warn!("weather: no weather-sectors.json in the terrain dir; weather disabled")
            }
            Err(err) => {
                warn!("weather: failed to read weather-sectors.json; weather disabled: {err}")
            }
        }
    }

    pub fn set_weather(&self, state: WeatherState) {
        *self.weather.write().expect("weather lock poisoned") = Some(state);
    }

    pub fn weather_sync_message(&self) -> Option<ServerMessage> {
        self.weather
            .read()
            .expect("weather lock poisoned")
            .map(|w| ServerMessage::WeatherSync {
                seed: w.seed,
                bias: w.bias,
            })
    }

    pub fn broadcast_weather(&self) {
        if let Some(msg) = self.weather_sync_message() {
            self.broadcast(msg);
        }
    }
}
