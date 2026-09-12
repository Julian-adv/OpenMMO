//! Regional weather (doc/WEATHER_SYSTEM.md). The server only hands clients
//! the seed the rain sectors were baked with; rain itself is evaluated from
//! `onlinerpg_shared::weather` on both sides.

use bytes::Bytes;
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::weather::WeatherSectors;
use sha2::Digest;
use tracing::{info, warn};

use super::GameState;

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherState {
    pub seed: u64,
    /// Multiplier on every zone's rain chance; 1.0 is the baked schedule.
    pub bias: f32,
    /// The sector file as loaded at boot; served to clients so every client
    /// evaluates the same list the seed was broadcast with.
    pub sectors_json: Bytes,
    /// Content hash of `sectors_json`; clients re-fetch only when it changes.
    pub sectors_tag: String,
}

impl WeatherState {
    pub fn new(seed: u64, bias: f32, sectors_json: Vec<u8>) -> Self {
        let sectors_tag = sectors_tag(&sectors_json);
        Self {
            seed,
            bias,
            sectors_json: Bytes::from(sectors_json),
            sectors_tag,
        }
    }
}

pub fn sectors_tag(json: &[u8]) -> String {
    let digest = sha2::Sha256::digest(json);
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

impl GameState {
    /// Weather stays off until a bake has produced `weather-sectors.json`.
    pub async fn load_weather(&self, bias: f32) {
        let bias = if bias.is_finite() && bias >= 0.0 {
            bias
        } else {
            warn!("weather: invalid bias {bias}; using 1.0");
            1.0
        };
        let json = match self.terrain_io.read_weather_sectors_bytes().await {
            Ok(Some(json)) => json,
            Ok(None) => {
                warn!("weather: no weather-sectors.json in the terrain dir; weather disabled");
                return;
            }
            Err(err) => {
                warn!("weather: failed to read weather-sectors.json; weather disabled: {err}");
                return;
            }
        };
        match serde_json::from_slice::<WeatherSectors>(&json) {
            Ok(sectors) => {
                let state = WeatherState::new(sectors.seed, bias, json);
                info!(
                    "weather: {} rain sectors, seed {}, bias {bias}, tag {}",
                    sectors.sectors.len(),
                    sectors.seed,
                    state.sectors_tag
                );
                self.set_weather(state);
            }
            Err(err) => {
                warn!("weather: invalid weather-sectors.json; weather disabled: {err}")
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
            .as_ref()
            .map(|w| ServerMessage::WeatherSync {
                seed: w.seed,
                bias: w.bias,
                sectors_tag: w.sectors_tag.clone(),
            })
    }

    pub fn weather_sectors_json(&self) -> Option<Bytes> {
        self.weather
            .read()
            .expect("weather lock poisoned")
            .as_ref()
            .map(|w| w.sectors_json.clone())
    }

    pub fn broadcast_weather(&self) {
        if let Some(msg) = self.weather_sync_message() {
            self.broadcast(msg);
        }
    }
}
