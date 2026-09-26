//! Regional weather sync and admin overrides (doc/WEATHER_SYSTEM.md).

use bytes::Bytes;
use onlinerpg_shared::housing::HouseData;
use onlinerpg_shared::messages::ServerMessage;
use onlinerpg_shared::weather::{
    cells_at, Cell, Precip, Sector, WeatherSectors, WEATHER_SECTORS_VERSION,
};
use onlinerpg_shared::PlayerId;
use sha2::Digest;
use std::collections::HashMap;
use std::ops::RangeInclusive;
use tracing::{info, warn};

use super::{chat::parse_bounded, GameState};

#[derive(Debug, Clone, PartialEq)]
pub struct WeatherState {
    pub seed: u64,
    /// Multiplier on every zone's rain chance; 1.0 is the baked schedule.
    pub bias: f32,
    pub rain_override: Option<f32>,
    /// The forced `rain_override` falls as snow.
    pub snow_override: bool,
    /// Sector bytes loaded at boot and served to clients.
    pub sectors_json: Bytes,
    /// Content hash of `sectors_json`; clients re-fetch only when it changes.
    pub sectors_tag: String,
    sectors: Vec<Sector>,
}

impl WeatherState {
    pub fn new(data: WeatherSectors, bias: f32, sectors_json: Vec<u8>) -> Self {
        let sectors_tag = sectors_tag(&sectors_json);
        Self {
            seed: data.seed,
            bias,
            rain_override: None,
            snow_override: false,
            sectors_json: Bytes::from(sectors_json),
            sectors_tag,
            sectors: data.sectors,
        }
    }
}

pub(super) struct RainShelter {
    x: RangeInclusive<f32>,
    z: RangeInclusive<f32>,
}

impl RainShelter {
    pub(super) fn contains(&self, x: f32, z: f32) -> bool {
        self.x.contains(&x) && self.z.contains(&z)
    }
}

pub(super) type RainShelterIndex = HashMap<String, Vec<RainShelter>>;

pub fn sectors_tag(json: &[u8]) -> String {
    let digest = sha2::Sha256::digest(json);
    digest.iter().take(8).map(|b| format!("{b:02x}")).collect()
}

impl GameState {
    pub(super) fn sync_rain_shelters(&self, house: &HouseData) {
        let shelters = house
            .rooms
            .iter()
            .filter(|room| room.floor_level == 0)
            .map(|room| {
                let x = house.origin.x + room.local_x as f32;
                let z = house.origin.z + room.local_z as f32;
                RainShelter {
                    x: x..=x + room.size_x as f32,
                    z: z..=z + room.size_z as f32,
                }
            })
            .collect();
        self.rain_shelters
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(house.id.clone(), shelters);
    }

    /// The override as (intensity, falls as snow), or the live cells.
    pub(super) fn current_rain_cells(&self) -> (Option<Precip>, Vec<Cell>) {
        let weather = self.weather.read().expect("weather lock poisoned");
        let Some(weather) = weather.as_ref() else {
            return (Some(Precip::default()), Vec::new());
        };
        if let Some(amount) = weather.rain_override {
            let forced = if weather.snow_override {
                Precip {
                    rain: 0.0,
                    snow: amount,
                }
            } else {
                Precip {
                    rain: amount,
                    snow: 0.0,
                }
            };
            return (Some(forced), Vec::new());
        }
        let minutes = self.current_total_game_seconds() as f64 / 60.0;
        (
            None,
            cells_at(&weather.sectors, weather.seed, weather.bias as f64, minutes),
        )
    }

    /// Weather stays off until a bake has produced `weather-sectors.json`.
    pub async fn load_weather(&self, bias: f32) {
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
            Ok(sectors) if sectors.version != WEATHER_SECTORS_VERSION => {
                warn!(
                    "weather: weather-sectors.json is version {} (expected {WEATHER_SECTORS_VERSION}); weather disabled",
                    sectors.version
                )
            }
            Ok(sectors) => {
                let state = WeatherState::new(sectors, bias, json);
                info!(
                    "weather: {} rain sectors, seed {}, bias {bias}, tag {}",
                    state.sectors.len(),
                    state.seed,
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
                rain_override: w.rain_override,
                snow_override: w.snow_override,
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

    pub(super) fn weather_command(
        &self,
        admin_id: &PlayerId,
        args: &str,
    ) -> Result<String, String> {
        let mut args = args.split_whitespace();
        let (rain_override, snow_override) = match (args.next(), args.next(), args.next()) {
            (Some(kind @ ("rain" | "snow")), raw, None) => {
                let amount = parse_bounded(raw.unwrap_or("1"), 0.0..=1.0, "Weather: intensity")?;
                (Some(amount), kind == "snow" && amount > 0.0)
            }
            (Some("clear"), None, None) => (Some(0.0), false),
            (Some("auto"), None, None) => (None, false),
            _ => {
                return Err(
                    "Weather: /weather rain [0..1], /weather snow [0..1], /weather clear, /weather auto"
                        .into(),
                )
            }
        };
        {
            let mut weather = self.weather.write().expect("weather lock poisoned");
            let weather = weather
                .as_mut()
                .ok_or("Weather: weather data is not loaded.")?;
            weather.rain_override = rain_override;
            weather.snow_override = snow_override;
        }
        info!(admin = ?admin_id, ?rain_override, snow_override, "admin weather override");
        self.broadcast_weather();
        let kind = if snow_override { "snow" } else { "rain" };
        Ok(match rain_override {
            None => "Weather: automatic regional weather restored server-wide.".into(),
            Some(0.0) => "Weather: clear skies server-wide. /weather auto restores automatic weather.".into(),
            Some(amount) => format!("Weather: {kind} intensity {amount} server-wide. /weather auto restores automatic weather."),
        })
    }
}
