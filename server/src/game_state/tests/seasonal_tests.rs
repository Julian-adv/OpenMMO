use super::super::passability::region_of;
use super::*;
use onlinerpg_shared::seasonal::{seasonal_prop_defs, MonthDay, Season, SeasonSchedule};

const GROUND: f32 = 4.0;

fn day(month: u8, day: u8) -> MonthDay {
    MonthDay::new(month, day).unwrap()
}

/// The world on `today`, with the region the Aldermark winter tree stands in.
fn setup(today: MonthDay) -> (GameState, (i32, i32)) {
    let game = make_test_game_state("seasonal");
    let sites = seasonal_prop_defs()
        .iter()
        .map(|def| (def, def.placement(GROUND)))
        .collect();
    game.place_seasonal_props(sites, today);
    let tree = seasonal_prop_defs()
        .iter()
        .find(|def| def.id == "winter_tree_aldermark")
        .expect("the Aldermark winter tree");
    (game, region_of(tree.x, tree.z))
}

#[tokio::test]
async fn the_winter_tree_stands_and_blocks_only_in_season() {
    let (game, (rx, rz)) = setup(day(12, 24));

    let standing = game.fixed_placements_in(rx, rz);
    assert_eq!(standing.len(), 1);
    assert_eq!(standing[0].type_id, "winter_tree");
    assert_eq!(standing[0].y, GROUND);
    assert!(
        game.sync_region_furniture(rx, rz, &[]),
        "the tree seals its cells"
    );

    assert_eq!(game.set_seasonal_date(day(1, 6)), vec![(rx, rz)]);
    assert!(game.fixed_placements_in(rx, rz).is_empty());
    assert!(!game.sync_region_furniture(rx, rz, &[]));
}

#[tokio::test]
async fn a_day_inside_the_season_changes_no_region() {
    let (game, (rx, rz)) = setup(day(12, 24));

    assert!(game.set_seasonal_date(day(12, 25)).is_empty());
    assert!(game.set_seasonal_date(day(12, 30)).is_empty());
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
}

#[tokio::test]
async fn the_tree_goes_up_when_the_season_opens() {
    let (game, (rx, rz)) = setup(day(11, 23));
    assert!(game.fixed_placements_in(rx, rz).is_empty());

    assert_eq!(game.set_seasonal_date(day(11, 24)), vec![(rx, rz)]);
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
}

fn period(start: MonthDay, end: MonthDay) -> SeasonSchedule {
    SeasonSchedule::Period(Season { start, end })
}

#[tokio::test]
async fn an_operator_run_replaces_the_table_dates() {
    let (game, (rx, rz)) = setup(day(11, 10));
    assert!(game.fixed_placements_in(rx, rz).is_empty());

    let november = period(day(11, 5), day(11, 15));
    assert_eq!(
        game.apply_seasonal_schedule("winter", november),
        vec![(rx, rz)]
    );
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);

    assert_eq!(game.set_seasonal_date(day(12, 24)), vec![(rx, rz)]);
    assert!(game.fixed_placements_in(rx, rz).is_empty());

    assert_eq!(
        game.apply_seasonal_schedule("winter", SeasonSchedule::Default),
        vec![(rx, rz)]
    );
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
}

#[tokio::test]
async fn an_event_switched_off_stays_down_in_season() {
    let (game, (rx, rz)) = setup(day(12, 24));

    assert_eq!(
        game.apply_seasonal_schedule("winter", SeasonSchedule::Off),
        vec![(rx, rz)]
    );
    assert!(game.fixed_placements_in(rx, rz).is_empty());
    assert!(game.set_seasonal_date(day(12, 25)).is_empty());
    assert!(game
        .apply_seasonal_schedule("winter", SeasonSchedule::Off)
        .is_empty());
}

#[tokio::test]
async fn a_set_schedule_is_stored_announced_and_kept_across_a_restart() {
    let (game, (rx, rz)) = setup(day(11, 10));
    let auth = make_test_auth("seasonal_store");
    let admin = pid("admin");
    let mut broadcast_rx = game.subscribe();
    let run = period(day(11, 1), day(1, 5));

    game.set_seasonal_schedule(&admin, "winter", run, &auth)
        .await
        .unwrap();

    let broadcast = broadcast_rx.try_recv().expect("the region is announced");
    assert!(matches!(
        rmp_serde::from_slice::<ServerMessage>(&broadcast.bytes),
        Ok(ServerMessage::ObjectRegionsChanged { regions }) if regions == vec![(rx, rz)]
    ));
    assert_eq!(
        auth.seasonal_schedules().unwrap(),
        vec![("winter".to_string(), run)]
    );
    match game.seasonal_events_message(None) {
        ServerMessage::SeasonalEvents { today, events, .. } => {
            assert_eq!(today, day(11, 10));
            assert_eq!(events.len(), 1);
            assert_eq!((events[0].id.as_str(), events[0].props), ("winter", 1));
            assert_eq!(events[0].schedule, run);
            assert!(events[0].active);
        }
        other => panic!("{other:?}"),
    }

    let (restarted, _) = setup(day(11, 10));
    restarted.load_seasonal_schedules(auth.seasonal_schedules().unwrap());
    assert_eq!(restarted.fixed_placements_in(rx, rz).len(), 1);

    game.set_seasonal_schedule(&admin, "winter", SeasonSchedule::Off, &auth)
        .await
        .unwrap();
    assert_eq!(
        auth.seasonal_schedules().unwrap(),
        vec![("winter".to_string(), SeasonSchedule::Off)]
    );
    game.set_seasonal_schedule(&admin, "winter", SeasonSchedule::Default, &auth)
        .await
        .unwrap();
    assert!(auth.seasonal_schedules().unwrap().is_empty());
}

#[tokio::test]
async fn a_bad_schedule_is_refused_and_changes_nothing() {
    let (game, (rx, rz)) = setup(day(11, 10));
    let auth = make_test_auth("seasonal_refuse");
    let admin = pid("admin");
    let mut broadcast_rx = game.subscribe();
    let not_a_day = SeasonSchedule::Period(Season {
        start: MonthDay { month: 2, day: 30 },
        end: day(3, 1),
    });

    let unknown = game
        .set_seasonal_schedule(&admin, "summer", SeasonSchedule::Off, &auth)
        .await
        .unwrap_err();
    assert!(unknown.contains("winter"), "{unknown}");
    assert!(game
        .set_seasonal_schedule(&admin, "winter", not_a_day, &auth)
        .await
        .is_err());

    assert!(broadcast_rx.try_recv().is_err());
    assert!(auth.seasonal_schedules().unwrap().is_empty());
    assert!(game.fixed_placements_in(rx, rz).is_empty());
}

#[tokio::test]
async fn the_season_command_lists_sets_and_explains_itself() {
    let (game, (rx, rz)) = setup(day(11, 10));
    let auth = make_test_auth("seasonal_command");
    let admin = pid("admin");
    let mut admin_rx = game.register_direct_channel(&admin).await;
    let panel_updates = std::cell::Cell::new(0);
    let mut say = async |text: &str| {
        game.send_chat_message(&admin, text.to_string(), &auth)
            .await;
        loop {
            match admin_rx.try_recv() {
                Ok(ServerMessage::SystemMessage { message, .. }) => return message,
                Ok(ServerMessage::SeasonalEvents { error: None, .. }) => {
                    panel_updates.set(panel_updates.get() + 1)
                }
                other => panic!("{other:?}"),
            }
        }
    };

    assert_eq!(
        say("/season").await,
        "Season (UTC 11-10): winter 11-24..12-30 (default), down"
    );
    assert_eq!(
        say("/season winter 11-01 1/5").await,
        "Season (UTC 11-10): winter 11-01..01-05, up"
    );
    assert_eq!(game.fixed_placements_in(rx, rz).len(), 1);
    assert_eq!(
        say("/season winter off").await,
        "Season (UTC 11-10): winter off, down"
    );
    assert_eq!(
        say("/season winter default").await,
        "Season (UTC 11-10): winter 11-24..12-30 (default), down"
    );
    assert!(say("/season winter 11-31 12-01").await.contains("MM-DD"));
    assert!(say("/season winter").await.starts_with("Season: /season"));
    assert!(say("/season winter on now please")
        .await
        .starts_with("Season: /season"));
    assert_eq!(panel_updates.get(), 3, "one per schedule set");
}
