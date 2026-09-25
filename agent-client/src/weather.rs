use std::sync::Arc;
use std::time::{Duration, Instant};

use onlinerpg_shared::weather::{self, WeatherSectors, WEATHER_SECTORS_VERSION};
use onlinerpg_shared::world::GameDateTime;
use tokio::sync::Mutex;
use tracing::warn;

use crate::state::SharedState;
use crate::terrain_http::http_client;

#[derive(Default)]
pub struct Weather {
    seed: u64,
    bias: f32,
    sectors_tag: Option<String>,
    rain_override: Option<f32>,
    sectors: Option<WeatherSectors>,
    t_min: Option<f64>,
    retry_at: Option<Instant>,
}

impl Weather {
    pub fn sync(&mut self, seed: u64, bias: f32, tag: &str, rain_override: Option<f32>) {
        if self.sectors_tag.as_deref() != Some(tag) {
            self.sectors = None;
            self.retry_at = None;
        }
        self.seed = seed;
        self.bias = bias;
        self.sectors_tag = Some(tag.to_string());
        self.rain_override = rain_override;
    }

    pub fn update_time(&mut self, datetime: &GameDateTime) {
        self.t_min = Some(weather::game_minutes(datetime));
    }

    pub fn rain_at(&self, pos: [f32; 3]) -> f32 {
        if let Some(rain) = self.rain_override {
            return rain;
        }
        let (Some(sectors), Some(t_min)) = (&self.sectors, self.t_min) else {
            return 0.0;
        };
        let cells = weather::cells_at(&sectors.sectors, self.seed, f64::from(self.bias), t_min);
        weather::rain_at(&cells, pos[0], pos[2])
    }

    fn fetch_tag(&mut self) -> Option<String> {
        if self.sectors.is_some() || self.retry_at.is_some_and(|at| Instant::now() < at) {
            return None;
        }
        let tag = self.sectors_tag.clone()?;
        self.retry_at = Some(Instant::now() + Duration::from_secs(30));
        Some(tag)
    }
}

pub async fn refresh_sectors(state: &Arc<Mutex<SharedState>>, api_base_url: &str, label: &str) {
    let Some(tag) = state.lock().await.weather.fetch_tag() else {
        return;
    };
    let result = async {
        http_client()
            .get(format!("{api_base_url}/api/terrain/weather-sectors"))
            .query(&[("v", &tag)])
            .timeout(Duration::from_secs(5))
            .send()
            .await?
            .error_for_status()?
            .json::<WeatherSectors>()
            .await
    }
    .await;
    let sectors = match result {
        Ok(sectors) if sectors.version == WEATHER_SECTORS_VERSION => sectors,
        Ok(sectors) => {
            warn!(
                "[{label}] Unsupported weather sectors version {}",
                sectors.version
            );
            return;
        }
        Err(err) => {
            warn!("[{label}] Failed to fetch weather sectors: {err}");
            return;
        }
    };
    let mut s = state.lock().await;
    if s.weather.sectors_tag.as_deref() == Some(&tag) {
        s.weather.sectors = Some(sectors);
        s.weather.retry_at = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::tests::test_state;
    use onlinerpg_shared::weather::Sector;
    use onlinerpg_shared::worldgen::climate::Climate;

    fn sectors() -> WeatherSectors {
        WeatherSectors {
            version: WEATHER_SECTORS_VERSION,
            seed: 42,
            sectors: vec![Sector {
                zone: Climate::Temperate as u8,
                spots: vec![[-1470.0, 4750.9]],
            }],
        }
    }

    #[test]
    fn regional_rain_uses_the_synced_clock_and_respects_admin_overrides() {
        let mut weather = Weather::default();
        weather.sync(42, 1.0, "a", None);
        weather.sectors = Some(sectors());
        let spot = [-1470.0, 0.0, 4750.9];
        assert_eq!(weather.rain_at(spot), 0.0);
        let datetime = (0..30 * 24 * 60)
            .map(|minute| GameDateTime {
                year: 217,
                month: 1,
                day: (minute / (24 * 60) + 1) as u8,
                hour: (minute / 60 % 24) as u8,
                minute: (minute % 60) as u8,
            })
            .find(|dt| {
                weather.update_time(dt);
                weather.rain_at(spot) > 0.5
            })
            .expect("the fixture should produce regional rain");
        assert_eq!(weather.rain_at([0.0, 0.0, 20_000.0]), 0.0);

        let (mut state, _rx) = test_state();
        state.weather = weather;
        state.push_event(onlinerpg_shared::ServerMessage::GameTimeSync {
            datetime: datetime.clone(),
            is_night: false,
        });
        state.drain_events();
        assert!(state.weather.rain_at(spot) > 0.5);
        state.weather.sync(42, 1.0, "a", Some(0.0));
        assert_eq!(state.weather.rain_at(spot), 0.0);
        state.weather.sync(42, 1.0, "a", Some(0.8));
        assert_eq!(state.weather.rain_at([0.0; 3]), 0.8);
        state.weather.sync(42, 1.0, "a", None);
        assert!(state.weather.rain_at(spot) > 0.5);
        state.weather.sync(42, 0.0, "a", None);
        assert_eq!(state.weather.rain_at(spot), 0.0);
    }

    #[tokio::test]
    async fn sectors_are_fetched_once_per_tag_and_failed_fetches_are_throttled() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let calls = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&calls);
        let app = axum::Router::new().route(
            "/api/terrain/weather-sectors",
            axum::routing::get(move || {
                count.fetch_add(1, Ordering::SeqCst);
                async { axum::Json(sectors()) }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let (s, _rx) = test_state();
        let state = Arc::new(Mutex::new(s));
        state.lock().await.weather.sync(42, 1.0, "a", None);
        refresh_sectors(&state, &url, "test").await;
        assert_eq!(state.lock().await.weather.sectors, Some(sectors()));
        state.lock().await.weather.sync(42, 1.0, "a", Some(1.0));
        refresh_sectors(&state, &url, "test").await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        state.lock().await.weather.sync(42, 1.0, "b", None);
        assert!(state.lock().await.weather.sectors.is_none());
        refresh_sectors(&state, &url, "test").await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        server.abort();
        let _ = server.await;

        state.lock().await.weather.sync(42, 1.0, "c", None);
        refresh_sectors(&state, &url, "test").await;
        let mut s = state.lock().await;
        assert!(s.weather.sectors.is_none());
        assert!(s.weather.fetch_tag().is_none());
        s.weather.retry_at = Some(Instant::now() - Duration::from_secs(1));
        assert_eq!(s.weather.fetch_tag().as_deref(), Some("c"));
    }
}
