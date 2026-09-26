// ---- Wet (doc/DEBUFF.md) ---------------------------------------------------
// `SplitWorldTiles` + `SeaOnlyWater` put the ocean at negative x (bed −5 m,
// surface 0) and dry land at positive x, so a step's x decides whether it
// soaks. Paused-time tests drive `soak_movers` by hand.

use super::*;
use crate::game_state::ambient_spawn::MoveStep;
use onlinerpg_shared::hunger::SATIATION_START;
use tokio::time::{advance, Duration};

use crate::game_state::COLD_DEBUFF_ID;
use crate::game_state::WET_DEBUFF_ID as WET;

/// One round per water-check bucket, so the mover is sampled whatever their
/// id hashes to.
const BUCKETS: usize = 5;

async fn make_wader(game_state: &GameState, name: &str) -> (PlayerId, DirectRx) {
    let id = pid(name);
    game_state.add_player(make_player(name, 100.0, 50.0)).await;
    game_state
        .register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(SATIATION_START))
        .await;
    let rx = game_state.register_direct_channel(&id).await;
    (id, rx)
}

fn step_to(player_id: PlayerId, x: f32, floor_level: i8) -> MoveStep {
    let to = Position { x, y: 0.0, z: 50.0 };
    MoveStep {
        player_id,
        from: to,
        to,
        floor_level,
        is_official_npc: false,
        mount: None,
    }
}

async fn soak(game_state: &GameState, steps: &[MoveStep]) {
    for _ in 0..BUCKETS {
        game_state.soak_movers(steps).await;
    }
}

async fn debuff_remaining(game_state: &GameState, id: &PlayerId, debuff: &str) -> Option<Duration> {
    let now = tokio::time::Instant::now();
    let hunger = game_state.hunger.read().await;
    hunger
        .get(id)?
        .debuffs
        .iter()
        .find(|d| d.def.id == debuff)
        .map(|d| d.until.saturating_duration_since(now))
}

async fn wet_remaining(game_state: &GameState, id: &PlayerId) -> Option<Duration> {
    debuff_remaining(game_state, id, WET).await
}

async fn move_mult(game_state: &GameState, id: &PlayerId) -> f32 {
    game_state
        .hunger_movement_profiles_for(&[*id])
        .await
        .get(id)
        .map_or(1.0, |(m, _)| *m)
}

mod rain {
    use super::*;
    use crate::game_state::weather::WeatherState;
    use crate::housing::test_fixtures::{house_at, room_at};
    use onlinerpg_shared::weather::{
        cells_at, rain_at, Sector, WeatherSectors, WEATHER_SECTORS_VERSION,
    };

    fn set_rain(game: &GameState, intensity: f32) {
        set_override(game, intensity, false);
    }

    fn set_snow(game: &GameState, intensity: f32) {
        set_override(game, intensity, true);
    }

    fn set_override(game: &GameState, intensity: f32, snow: bool) {
        let json = br#"{"version":1,"seed":42,"sectors":[]}"#.to_vec();
        let mut weather = WeatherState::new(serde_json::from_slice(&json).unwrap(), 1.0, json);
        weather.rain_override = Some(intensity);
        weather.snow_override = snow;
        game.set_weather(weather);
    }

    async fn cold_remaining(game: &GameState, id: &PlayerId) -> Option<Duration> {
        debuff_remaining(game, id, COLD_DEBUFF_ID).await
    }

    #[tokio::test(start_paused = true)]
    async fn snow_chills_after_twenty_game_minutes_and_never_soaks() {
        let game = make_test_game_state("cold_snow_threshold");
        let (id, mut rx) = make_wader(&game, "snow_walker").await;
        set_snow(&game, 1.0);
        rain_for(&game, 149).await;
        assert_eq!(cold_remaining(&game, &id).await, None);
        assert!(drain(&mut rx).is_empty());

        rain_for(&game, 1).await;
        assert_eq!(
            cold_remaining(&game, &id).await,
            Some(Duration::from_secs(450))
        );
        assert_eq!(wet_remaining(&game, &id).await, None);
        assert!(!broadcast_wet_flag(&game, &id).await);
        assert!((move_mult(&game, &id).await - 0.9).abs() < 1e-6);
        assert!(drain(&mut rx)
            .iter()
            .any(|msg| matches!(msg, ServerMessage::DebuffUpdate { .. })));

        rain_for(&game, 150).await;
        assert_eq!(
            cold_remaining(&game, &id).await,
            Some(Duration::from_secs(300))
        );
        rain_for(&game, 1).await;
        assert_eq!(
            cold_remaining(&game, &id).await,
            Some(Duration::from_secs(450))
        );
        assert_eq!(wet_remaining(&game, &id).await, None);
    }

    #[tokio::test(start_paused = true)]
    async fn rain_soaks_without_chilling() {
        let game = make_test_game_state("cold_rain_only_wet");
        let (id, _rx) = make_wader(&game, "rain_only").await;
        set_rain(&game, 1.0);
        rain_for(&game, 300).await;
        assert!(wet_remaining(&game, &id).await.is_some());
        assert_eq!(cold_remaining(&game, &id).await, None);
        assert_eq!(game.hunger.read().await[&id].cold_exposure_secs, 0.0);
    }

    #[tokio::test(start_paused = true)]
    async fn shelter_and_death_reset_snow_exposure() {
        let game = make_test_game_state("cold_snow_shelter");
        let (id, _rx) = make_wader(&game, "snow_shelter").await;
        set_snow(&game, 1.0);
        rain_for(&game, 149).await;

        let house = house_at(100.0, 49.0, vec![room_at(0, 0)]);
        game.passability_add_house(&house).await;
        rain_for(&game, 150).await;
        assert_eq!(cold_remaining(&game, &id).await, None);

        game.passability_remove_house(&house.id).await;
        rain_for(&game, 149).await;
        game.clear_debuffs(&id).await;
        rain_for(&game, 1).await;
        assert_eq!(cold_remaining(&game, &id).await, None);
        rain_for(&game, 149).await;
        assert!(cold_remaining(&game, &id).await.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn a_campfire_warms_off_cold_and_wet_together() {
        let game = make_test_game_state("cold_campfire");
        let (id, mut rx) = make_wader(&game, "shivering").await;
        game.inflict_debuff(&id, WET, Some(true)).await;
        game.inflict_debuff(&id, COLD_DEBUFF_ID, Some(true)).await;
        light_fire_at(&game, 100.0, 0).await;
        drain(&mut rx);

        game.tick_campfire_drying(Duration::from_secs(1)).await;
        assert_eq!(
            cold_remaining(&game, &id).await,
            Some(Duration::from_secs(441))
        );
        assert_eq!(
            wet_remaining(&game, &id).await,
            Some(Duration::from_secs(441))
        );
        assert!(matches!(
            drain(&mut rx).as_slice(),
            [ServerMessage::DebuffUpdate { debuffs }] if debuffs.len() == 2
        ));

        for _ in 0..49 {
            game.tick_campfire_drying(Duration::from_secs(1)).await;
        }
        game.tick_debuffs().await;
        assert_eq!(cold_remaining(&game, &id).await, None);
        assert_eq!(wet_remaining(&game, &id).await, None);
        assert_eq!(move_mult(&game, &id).await, 1.0);
    }

    #[tokio::test(start_paused = true)]
    async fn a_campfire_warms_a_player_who_is_only_cold() {
        let game = make_test_game_state("cold_campfire_only");
        let (id, _rx) = make_wader(&game, "only_cold").await;
        game.inflict_debuff(&id, COLD_DEBUFF_ID, Some(true)).await;
        light_fire_at(&game, 100.0, 0).await;
        game.tick_campfire_drying(Duration::from_secs(1)).await;
        assert_eq!(
            cold_remaining(&game, &id).await,
            Some(Duration::from_secs(441))
        );
    }

    async fn rain_for(game: &GameState, seconds: u64) {
        for _ in 0..seconds {
            advance(Duration::from_secs(1)).await;
            game.tick_rain_soaking(Duration::from_secs(1)).await;
            game.tick_campfire_drying(Duration::from_secs(1)).await;
            game.tick_debuffs().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn standing_in_rain_soaks_after_ten_game_minutes_scaled_by_intensity() {
        for (intensity, seconds) in [(1.0, 75), (0.5, 150)] {
            let game = make_test_game_state("wet_rain_threshold");
            let (id, mut rx) = make_wader(&game, "rain_walker").await;
            set_rain(&game, intensity);
            rain_for(&game, seconds - 1).await;
            assert_eq!(wet_remaining(&game, &id).await, None);
            assert!(drain(&mut rx).is_empty());

            rain_for(&game, 1).await;
            assert_eq!(
                wet_remaining(&game, &id).await,
                Some(Duration::from_secs(450))
            );
            assert!((move_mult(&game, &id).await - 0.83).abs() < 1e-6);
            assert_eq!(game.armor_weight_mult(&id).await, 1.5);
            assert!(broadcast_wet_flag(&game, &id).await);
            assert!(drain(&mut rx)
                .iter()
                .any(|msg| matches!(msg, ServerMessage::DebuffUpdate { .. })));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn shelter_resets_exposure_but_gaps_between_rooms_still_get_rain() {
        let game = make_test_game_state("wet_rain_shelter");
        let (id, _rx) = make_wader(&game, "sheltering").await;
        set_rain(&game, 1.0);
        rain_for(&game, 74).await;

        let house = house_at(100.0, 49.0, vec![room_at(0, 0), room_at(0, 6)]);
        game.passability_add_house(&house).await;
        rain_for(&game, 75).await;
        assert_eq!(wet_remaining(&game, &id).await, None);

        game.players.write().await.get_mut(&id).unwrap().position.z = 54.0;
        rain_for(&game, 74).await;
        assert_eq!(wet_remaining(&game, &id).await, None);
        rain_for(&game, 1).await;
        assert!(wet_remaining(&game, &id).await.is_some());

        game.players.write().await.get_mut(&id).unwrap().position.z = 50.0;
        rain_for(&game, 1).await;
        assert_eq!(
            wet_remaining(&game, &id).await,
            Some(Duration::from_secs(449))
        );
        rain_for(&game, 449).await;
        assert_eq!(wet_remaining(&game, &id).await, None);
        assert!(!broadcast_wet_flag(&game, &id).await);
    }

    #[tokio::test(start_paused = true)]
    async fn replacing_and_removing_a_house_updates_rain_shelter() {
        let game = make_test_game_state("wet_rain_shelter_changes");
        let (id, _rx) = make_wader(&game, "builder").await;
        set_rain(&game, 1.0);
        let mut house = house_at(100.0, 49.0, vec![room_at(0, 0)]);
        game.passability_add_house(&house).await;
        rain_for(&game, 75).await;
        assert_eq!(wet_remaining(&game, &id).await, None);

        house.origin.x = 200.0;
        game.passability_add_house(&house).await;
        rain_for(&game, 75).await;
        assert!(wet_remaining(&game, &id).await.is_some());
        game.clear_debuffs(&id).await;
        game.players.write().await.get_mut(&id).unwrap().position.x = 200.0;
        rain_for(&game, 75).await;
        assert_eq!(wet_remaining(&game, &id).await, None);

        game.passability_remove_house(&house.id).await;
        rain_for(&game, 75).await;
        assert!(wet_remaining(&game, &id).await.is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn dry_weather_and_ineligible_players_reset_partial_exposure() {
        let game = make_test_game_state("wet_rain_exemptions");
        let (id, _rx) = make_wader(&game, "resting").await;
        let mut npc = make_player("rain_npc", 100.0, 50.0);
        npc.is_official_npc = true;
        game.add_player(npc).await;

        for reason in [
            "clear",
            "trace_rain",
            "upper_floor",
            "dungeon",
            "dead",
            "loading",
            "death_cleanup",
        ] {
            set_rain(&game, 1.0);
            rain_for(&game, 74).await;
            match reason {
                "clear" => set_rain(&game, 0.0),
                "trace_rain" => set_rain(&game, 0.02),
                "upper_floor" => game.players.write().await.get_mut(&id).unwrap().floor_level = 1,
                "dungeon" => game.players.write().await.get_mut(&id).unwrap().floor_level = -1,
                "dead" => game.players.write().await.get_mut(&id).unwrap().health = 0,
                "loading" => game.players.write().await.get_mut(&id).unwrap().ready_at = u64::MAX,
                "death_cleanup" => game.clear_debuffs(&id).await,
                _ => unreachable!(),
            }
            if reason != "death_cleanup" {
                rain_for(&game, 75).await;
            }
            assert_eq!(wet_remaining(&game, &id).await, None, "{reason}");
            assert!(!broadcast_wet_flag(&game, &pid("rain_npc")).await);
            *game.players.write().await.get_mut(&id).unwrap() = make_player("resting", 100.0, 50.0);
            set_rain(&game, 1.0);
            rain_for(&game, 1).await;
            assert_eq!(wet_remaining(&game, &id).await, None, "{reason}");
            game.clear_debuffs(&id).await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn rain_refreshes_wet_without_spamming_updates_and_clear_weather_allows_drying() {
        let game = make_test_game_state("wet_rain_refresh");
        let (id, mut rx) = make_wader(&game, "soaked").await;
        set_rain(&game, 1.0);
        rain_for(&game, 75).await;
        drain(&mut rx);
        rain_for(&game, 150).await;
        assert_eq!(
            wet_remaining(&game, &id).await,
            Some(Duration::from_secs(300))
        );
        assert!(drain(&mut rx).is_empty());
        rain_for(&game, 1).await;
        assert_eq!(
            wet_remaining(&game, &id).await,
            Some(Duration::from_secs(450))
        );
        assert!(matches!(
            drain(&mut rx).as_slice(),
            [ServerMessage::DebuffUpdate { .. }]
        ));

        set_rain(&game, 0.0);
        rain_for(&game, 1).await;
        assert_eq!(
            wet_remaining(&game, &id).await,
            Some(Duration::from_secs(449))
        );
        light_fire_at(&game, 100.0, 0).await;
        rain_for(&game, 45).await;
        assert_eq!(wet_remaining(&game, &id).await, None);
        assert!(!broadcast_wet_flag(&game, &id).await);
    }

    #[tokio::test(start_paused = true)]
    async fn automatic_rain_only_soaks_players_in_the_active_region() {
        let game = make_test_game_state("wet_rain_regional");
        let (near, _near_rx) = make_wader(&game, "near_rain").await;
        let (far, _far_rx) = make_wader(&game, "far_from_rain").await;
        game.players.write().await.get_mut(&far).unwrap().position.z = 10_000.0;
        let sectors = WeatherSectors {
            version: WEATHER_SECTORS_VERSION,
            seed: 42,
            sectors: vec![Sector {
                zone: 1,
                spots: vec![[100.0, 50.0]],
                ..Default::default()
            }],
        };
        let minute = (0..1440)
            .find(|minute| {
                rain_at(
                    &cells_at(&sectors.sectors, 42, 1.0, *minute as f64),
                    100.0,
                    50.0,
                ) > 0.99
            })
            .expect("full rain during the first winter day");
        game.debug_set_datetime(&GameState::total_game_seconds_to_datetime(minute * 60));
        let json = serde_json::to_vec(&sectors).unwrap();
        game.set_weather(WeatherState::new(sectors, 1.0, json));
        rain_for(&game, 80).await;
        assert!(wet_remaining(&game, &near).await.is_some());
        assert_eq!(wet_remaining(&game, &far).await, None);

        game.weather_command(&near, "clear").unwrap();
        rain_for(&game, 450).await;
        assert_eq!(wet_remaining(&game, &near).await, None);
        game.weather_command(&near, "auto").unwrap();
        rain_for(&game, 80).await;
        assert!(wet_remaining(&game, &near).await.is_some());
        assert_eq!(wet_remaining(&game, &far).await, None);
    }
}

#[tokio::test(start_paused = true)]
async fn a_step_into_the_sea_soaks_and_slows() {
    let game_state = make_test_game_state("wet_sea_step");
    let (id, _rx) = make_wader(&game_state, "wader").await;

    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(450))
    );
    // Walk 3.0 → 2.49, sprint 4.5 → 3.735: between walking and running.
    assert!((move_mult(&game_state, &id).await - 0.83).abs() < 1e-6);
}

/// The deck is the server's own index and the mover's server-side Y, so a
/// client claiming a lofty Y beside the bridge gains nothing, and a wader
/// passing under the span still soaks.
#[tokio::test(start_paused = true)]
async fn a_bridge_deck_over_the_sea_stays_dry_beside_and_under_it_soaks() {
    let game_state = make_test_game_state("wet_bridge_deck");
    let (id, _rx) = make_wader(&game_state, "walker").await;
    game_state.sync_region_furniture(-1, 0, &[stone_bridge(-100.0, 3.0, 50.0)]);

    let mut on_deck = step_to(id, -92.0, 0);
    on_deck.to.y = 4.0;
    soak(&game_state, &[on_deck]).await;
    assert_eq!(wet_remaining(&game_state, &id).await, None);

    let mut under = step_to(id, -100.0, 0);
    under.to.y = -5.0;
    soak(&game_state, &[under]).await;
    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(450))
    );

    let (id2, _rx2) = make_wader(&game_state, "flier").await;
    let mut beside = step_to(id2, -112.0, 0);
    beside.to.y = 9.0;
    soak(&game_state, &[beside]).await;
    assert_eq!(
        wet_remaining(&game_state, &id2).await,
        Some(Duration::from_secs(450))
    );
}

#[tokio::test(start_paused = true)]
async fn steps_on_dry_land_never_soak() {
    let game_state = make_test_game_state("wet_dry_step");
    let (id, _rx) = make_wader(&game_state, "walker").await;

    soak(&game_state, &[step_to(id, 100.0, 0)]).await;

    assert_eq!(wet_remaining(&game_state, &id).await, None);
    assert_eq!(move_mult(&game_state, &id).await, 1.0);
}

#[tokio::test(start_paused = true)]
async fn water_above_a_surface_floor_is_never_sampled() {
    let game_state = make_test_game_state("wet_upper_floor");
    let (id, _rx) = make_wader(&game_state, "upstairs").await;

    soak(&game_state, &[step_to(id, -100.0, 1)]).await;

    assert_eq!(wet_remaining(&game_state, &id).await, None);
}

#[tokio::test(start_paused = true)]
async fn a_player_without_a_hunger_entry_is_exempt() {
    let game_state = make_test_game_state("wet_official_exempt");
    let id = pid("official");
    game_state
        .add_player(make_player("official", 100.0, 50.0))
        .await;

    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    assert_eq!(wet_remaining(&game_state, &id).await, None);
}

#[tokio::test(start_paused = true)]
async fn wading_refreshes_only_once_the_soaking_is_wearing_off() {
    let game_state = make_test_game_state("wet_refresh_gate");
    let (id, _rx) = make_wader(&game_state, "swimmer").await;
    let in_water = [step_to(id, -100.0, 0)];

    soak(&game_state, &in_water).await;
    advance(Duration::from_secs(100)).await;

    // Still 350 s left: wading costs no terrain sample and no refresh.
    soak(&game_state, &in_water).await;
    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(350))
    );

    advance(Duration::from_secs(100)).await;
    soak(&game_state, &in_water).await;
    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(450))
    );
}

#[tokio::test(start_paused = true)]
async fn leaving_the_water_dries_off_after_a_game_hour() {
    let game_state = make_test_game_state("wet_dries_off");
    let (id, _rx) = make_wader(&game_state, "beachcomber").await;

    soak(&game_state, &[step_to(id, -100.0, 0)]).await;
    advance(Duration::from_secs(451)).await;
    game_state.tick_debuffs().await;

    assert_eq!(wet_remaining(&game_state, &id).await, None);
    assert_eq!(move_mult(&game_state, &id).await, 1.0);
}

/// A lit fire at the wader's feet, on their floor.
async fn light_fire_at(game_state: &GameState, x: f32, floor_level: i8) {
    game_state
        .spawn_campfire(
            Position { x, y: 0.0, z: 50.0 },
            floor_level,
            onlinerpg_shared::hunger::CAMPFIRE_DURATION_MS,
        )
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_campfire_pulls_the_soaking_down_ten_times_faster() {
    let game_state = make_test_game_state("wet_campfire_dries");
    let (id, _rx) = make_wader(&game_state, "camper").await;
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;
    light_fire_at(&game_state, 100.0, 0).await;

    // One sweep second by the fire burns ten off the soaking.
    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;
    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(441))
    );

    for _ in 0..49 {
        game_state
            .tick_campfire_drying(Duration::from_secs(1))
            .await;
    }
    game_state.tick_debuffs().await;
    assert_eq!(wet_remaining(&game_state, &id).await, None);
    assert_eq!(move_mult(&game_state, &id).await, 1.0);
}

#[tokio::test(start_paused = true)]
async fn a_fire_out_of_reach_or_on_another_floor_dries_nobody() {
    let game_state = make_test_game_state("wet_campfire_out_of_reach");
    let (id, _rx) = make_wader(&game_state, "loner").await;
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    // 10 m away, and a second fire underfoot but on the storey above.
    light_fire_at(&game_state, 110.0, 0).await;
    light_fire_at(&game_state, 100.0, 1).await;
    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;

    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(450))
    );
}

#[tokio::test(start_paused = true)]
async fn drying_leaves_a_dry_player_alone() {
    let game_state = make_test_game_state("wet_campfire_dry_player");
    let (id, _rx) = make_wader(&game_state, "dry_camper").await;
    light_fire_at(&game_state, 100.0, 0).await;

    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;

    assert_eq!(wet_remaining(&game_state, &id).await, None);
}

/// An official NPC's `/light_campfire` fire is the same entry as a player's,
/// so it dries the people standing around it (doc/HUNGER.md).
#[tokio::test(start_paused = true)]
async fn an_npc_lit_campfire_dries_too() {
    let game_state = make_test_game_state("wet_npc_campfire");
    let auth = make_test_auth("wet_npc_campfire");
    let (id, _rx) = make_wader(&game_state, "guest").await;
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    let npc_id = pid("npc_signe");
    let mut npc = make_player("npc_signe", 100.0, 50.0);
    npc.is_official_npc = true;
    game_state.add_player(npc).await;
    game_state
        .send_chat_message(&npc_id, "/light_campfire".to_string(), &auth)
        .await;
    assert_eq!(game_state.campfires.read().await.len(), 1);

    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;

    assert_eq!(
        wet_remaining(&game_state, &id).await,
        Some(Duration::from_secs(441))
    );
}

async fn broadcast_wet_flag(game_state: &GameState, id: &PlayerId) -> bool {
    game_state.players.read().await[id].wet
}

#[tokio::test(start_paused = true)]
async fn the_soaking_rides_the_broadcast_player_for_nearby_clients() {
    let game_state = make_test_game_state("wet_broadcast_flag");
    let (id, _rx) = make_wader(&game_state, "splasher").await;
    assert!(!broadcast_wet_flag(&game_state, &id).await);

    soak(&game_state, &[step_to(id, -100.0, 0)]).await;
    assert!(broadcast_wet_flag(&game_state, &id).await);

    advance(Duration::from_secs(451)).await;
    game_state.tick_debuffs().await;
    assert!(!broadcast_wet_flag(&game_state, &id).await);
}

#[tokio::test(start_paused = true)]
async fn death_drops_the_broadcast_soaking_too() {
    let game_state = make_test_game_state("wet_broadcast_death");
    let (id, _rx) = make_wader(&game_state, "drowner").await;
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    game_state.clear_debuffs(&id).await;

    assert!(!broadcast_wet_flag(&game_state, &id).await);
    assert_eq!(wet_remaining(&game_state, &id).await, None);
}

async fn carry(game_state: &GameState, id: &PlayerId, inventory: PlayerInventory) {
    game_state.inventories.write().await.insert(*id, inventory);
}

async fn carried_weight(game_state: &GameState, id: &PlayerId) -> f32 {
    let armor_mult = game_state.armor_weight_mult(id).await;
    let inventories = game_state.inventories.read().await;
    game_state.calc_total_weight(&inventories[id], armor_mult)
}

#[tokio::test(start_paused = true)]
async fn soaked_armour_drags_but_the_rest_of_the_pack_does_not() {
    let game_state = make_test_game_state("wet_armour_weight");
    let (id, _rx) = make_wader(&game_state, "porter").await;
    // chain_mail (30) + wooden_shield (6) armour, plus a non-armour torch (1).
    carry(
        &game_state,
        &id,
        PlayerInventory {
            active_ammo: None,
            bag: vec![bag_item(801, "chain_mail", 1), bag_item(802, "torch", 1)],
            equipped: std::collections::HashMap::from([(
                EquipSlot::OffHand,
                bag_item(803, "wooden_shield", 1),
            )]),
        },
    )
    .await;

    let dry = carried_weight(&game_state, &id).await;
    assert!((dry - 37.0).abs() < 1e-3, "expected 30+6+1, got {dry}");

    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    // Only the 36 of armour scales; the torch is untouched. Derived from the
    // constant so retuning it stays a one-line change.
    let mult = crate::debuff_defs::debuff_def(WET)
        .unwrap()
        .armor_weight_mult;
    let expected = 36.0 * mult + 1.0;
    let soaked = carried_weight(&game_state, &id).await;
    assert!(
        (soaked - expected).abs() < 1e-3,
        "expected {expected}, got {soaked}"
    );
}

#[tokio::test(start_paused = true)]
async fn an_unarmoured_player_carries_the_same_soaked_or_dry() {
    let game_state = make_test_game_state("wet_no_armour_weight");
    let (id, _rx) = make_wader(&game_state, "streaker").await;
    carry(
        &game_state,
        &id,
        PlayerInventory {
            active_ammo: None,
            bag: vec![bag_item(811, "torch", 4)],
            equipped: std::collections::HashMap::new(),
        },
    )
    .await;

    let dry = carried_weight(&game_state, &id).await;
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;

    assert_eq!(carried_weight(&game_state, &id).await, dry);
}

/// Scale measurement for the wet path, sized against the 5,000 concurrent-user
/// target. Not an assertion — run with --nocapture to read the timings.
#[tokio::test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
async fn wet_path_cost_at_scale() {
    const USERS: usize = 5_000;
    let game_state = make_test_game_state("wet_scale");

    // Cache-hit sampling cost: the fixture serves tiles from memory, so this
    // is the steady-state path, not first-touch tile IO.
    for i in 0..64 {
        let _ = game_state.water_depth_at(-100.0 + i as f32, 50.0).await;
    }
    let start = std::time::Instant::now();
    for i in 0..USERS {
        let x = -100.0 + (i % 64) as f32;
        let z = 50.0 + ((i / 64) % 64) as f32;
        let _ = game_state.water_depth_at(x, z).await;
    }
    let elapsed = start.elapsed();
    println!(
        "water_depth_at x{USERS}: {elapsed:?} ({:.2} us/sample)",
        elapsed.as_secs_f64() * 1e6 / USERS as f64
    );

    let mut ids = Vec::with_capacity(USERS);
    for i in 0..USERS {
        let name = format!("wet_bot{i}");
        let id = pid(&name);
        game_state.add_player(make_player(&name, 100.0, 50.0)).await;
        game_state
            .register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(700))
            .await;
        ids.push(id);
    }
    let steps: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| step_to(*id, 100.0 + (i % 64) as f32, 0))
        .collect();

    // One movement tick with everybody walking: a fifth of them fall in this
    // tick's water-check bucket.
    let start = std::time::Instant::now();
    game_state.soak_movers(&steps).await;
    println!(
        "soak_movers, {USERS} movers, none wet: {:?}",
        start.elapsed()
    );

    let start = std::time::Instant::now();
    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;
    println!("tick_campfire_drying, nobody wet: {:?}", start.elapsed());

    for id in &ids {
        game_state.inflict_debuff(id, WET, Some(true)).await;
    }
    // The refresh gate should now skip the sampling for all of them.
    let start = std::time::Instant::now();
    game_state.soak_movers(&steps).await;
    println!(
        "soak_movers, {USERS} movers, all already wet: {:?}",
        start.elapsed()
    );

    for i in 0..200 {
        light_fire_at(&game_state, 300.0 + i as f32, 0).await;
    }
    let start = std::time::Instant::now();
    game_state
        .tick_campfire_drying(Duration::from_secs(1))
        .await;
    println!(
        "tick_campfire_drying, {USERS} wet x 200 fires: {:?}",
        start.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn a_boat_rider_does_not_get_wet() {
    let game_state = make_test_game_state("wet_rowboat");
    let (id, _rx) = make_wader(&game_state, "rower").await;

    let mut afloat = step_to(id, -100.0, 0);
    afloat.mount = Some(onlinerpg_shared::mount::MountKind::Rowboat);
    soak(&game_state, &[afloat]).await;
    assert_eq!(wet_remaining(&game_state, &id).await, None);

    // The same crossing on foot still soaks, so the exemption is the boat.
    soak(&game_state, &[step_to(id, -100.0, 0)]).await;
    assert!(wet_remaining(&game_state, &id).await.is_some());
}
