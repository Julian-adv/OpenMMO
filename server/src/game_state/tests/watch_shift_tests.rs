use super::*;
use crate::game_state::watch_shift::{
    watch_wage, FULL_WAGE, LOST_GEAR_WAGE, SPEAR_ITEM, TABARD_ITEM,
};
use onlinerpg_shared::furniture::FurniturePlacement;
use onlinerpg_shared::messages::{
    PatrolMuster, WatchDuty, WatchGear, WatchNote, WatchStage, WatchTool,
};
use onlinerpg_shared::schedule::{RoadPatrol, ScheduleEntry, WATCH_GEAR_RACK};

const NOON_MIN: i64 = 12 * 60;
const RACK: u32 = 9;

fn set_minute(game: &GameState, minute: i64) {
    let datetime = GameState::total_game_seconds_to_datetime((NOON_MIN + minute) * 60);
    game.debug_set_datetime(&datetime);
}

async fn enter(game: &GameState, name: &str, character_id: i64, x: f32) -> DirectRx {
    game.add_player(make_player(name, x, 3.0)).await;
    game.register_player_character(&pid(name), character_id, 0, attrs_with_cha(10), 0, None)
        .await;
    game.inventories
        .write()
        .await
        .insert(pid(name), PlayerInventory::default());
    game.register_direct_channel(&pid(name)).await
}

async fn post_guard(game: &GameState, name: &str) -> DirectRx {
    let mut guard = make_player(name, 3.0, 3.0);
    guard.is_official_npc = true;
    game.add_player(guard).await;
    game.register_direct_channel(&pid(name)).await
}

/// `sergeant` leads a watch marching at `at`'s start.
pub(super) fn lead_watch(game: &GameState, sergeant: &str, at: &str) {
    let entry = ScheduleEntry {
        at: at.to_string(),
        patrol: Some(RoadPatrol {
            sweeps: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    game.set_npc_schedule(sergeant, vec![entry]);
}

/// Karl at his post, Sergeant Garrick's day watch and the gear rack.
async fn world(game: &GameState) -> DirectRx {
    let karl_rx = post_guard(game, "Karl").await;
    post_guard(game, "Garrick").await;
    lead_watch(game, "Garrick", "12:30-22:00");
    let rack = FurniturePlacement {
        id: RACK,
        type_id: WATCH_GEAR_RACK.into(),
        x: 4.0,
        y: 0.0,
        z: 3.0,
        rotation_deg: 0.0,
        floor_level: 0,
        open_doors: 0,
    };
    game.sync_region_furniture(0, 0, &[rack]);
    karl_rx
}

async fn setup(tag: &str) -> (GameState, crate::auth::AuthService, DirectRx, DirectRx, i64) {
    let game = make_flat_world_game_state(tag);
    let (auth, _) = make_test_auth_with_path(tag);
    let recruit = create_test_character(&auth, &auth.login_google("Recruit").unwrap(), "Recruit");
    let recruit_rx = enter(&game, "Recruit", recruit.id, 2.0).await;
    let karl_rx = world(&game).await;
    set_minute(&game, 0);
    (game, auth, recruit_rx, karl_rx, recruit.id)
}

fn codes(rx: &mut DirectRx) -> Vec<String> {
    drain(rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::SystemMessage { localization, .. } => localization.map(|l| l.code),
            _ => None,
        })
        .collect()
}

fn captain_notes(rx: &mut DirectRx) -> Vec<WatchNote> {
    drain(rx)
        .into_iter()
        .filter_map(|message| match message {
            ServerMessage::WatchVerdict { note, .. } => Some(note),
            _ => None,
        })
        .collect()
}

async fn stand(game: &GameState, name: &str, x: f32) {
    game.players
        .write()
        .await
        .get_mut(&pid(name))
        .unwrap()
        .position
        .x = x;
}

/// Sergeant Garrick calls the roll at `minute`.
async fn call(
    game: &GameState,
    auth: &crate::auth::AuthService,
    minute: i64,
    muster: PatrolMuster,
) {
    set_minute(game, minute);
    game.patrol_roll_call(&pid("Garrick"), muster, 7, auth)
        .await;
}

/// Ask, read the terms, say yes.
async fn hire_as(game: &GameState, auth: &crate::auth::AuthService, name: &str, rx: &mut DirectRx) {
    game.watch_shift_visit(&pid(name), &pid("Karl"), true, auth)
        .await;
    assert!(drain(rx)
        .iter()
        .any(|message| matches!(message, ServerMessage::WatchOffered { .. })));
    game.watch_shift_accept(&pid(name), &pid("Karl"), "Garrick", auth)
        .await;
    assert_eq!(codes(rx), ["server.watchHired"]);
}

async fn hire(game: &GameState, auth: &crate::auth::AuthService, rx: &mut DirectRx) {
    hire_as(game, auth, "Recruit", rx).await;
}

/// Use the rack the way an agent does; the last notice it gave, if any.
async fn rack(game: &GameState, auth: &crate::auth::AuthService, rx: &mut DirectRx) -> String {
    game.watch_rack_use(&pid("Recruit"), RACK, auth).await;
    codes(rx).pop().unwrap_or_default()
}

async fn hire_and_draw(game: &GameState, auth: &crate::auth::AuthService, rx: &mut DirectRx) {
    hire(game, auth, rx).await;
    rack(game, auth, rx).await;
    assert_eq!(bag_count(game, &pid("Recruit"), SPEAR_ITEM).await, 1);
}

/// Out, stops every ten minutes, then home at `home`.
async fn walk_watch(game: &GameState, auth: &crate::auth::AuthService, home: i64) {
    call(game, auth, 10, PatrolMuster::Out).await;
    for minute in (20..home).step_by(10) {
        call(game, auth, minute, PatrolMuster::Stop).await;
    }
    call(game, auth, home, PatrolMuster::Home).await;
}

async fn report(game: &GameState, auth: &crate::auth::AuthService, rx: &mut DirectRx) -> String {
    game.watch_shift_visit(&pid("Recruit"), &pid("Karl"), true, auth)
        .await;
    codes(rx).pop().unwrap()
}

/// What the recruit wears on the back, and whether it carries a crest.
async fn worn(game: &GameState) -> Option<(String, bool)> {
    let inventories = game.inventories.read().await;
    let back = inventories[&pid("Recruit")]
        .equipped
        .get(&onlinerpg_shared::inventory::EquipSlot::Back);
    back.map(|item| (item.item_def_id.clone(), item.cape_texture.is_some()))
}

#[tokio::test]
async fn the_whole_sequence_from_terms_to_rack_to_pay() {
    let (game, auth, mut recruit_rx, mut karl_rx, _) = setup("watch_clean").await;
    let (recruit, karl) = (pid("Recruit"), pid("Karl"));

    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchRackNotRecruit"
    );

    game.watch_shift_visit(&recruit, &karl, true, &auth).await;
    assert!(drain(&mut recruit_rx).iter().any(|message| matches!(
        message,
        ServerMessage::WatchOffered { sergeant, departs_minute: 750, wage, .. }
            if sergeant == "Garrick" && *wage == FULL_WAGE
    )));
    assert!(matches!(
        captain_notes(&mut karl_rx)[0],
        WatchNote::Offered { .. }
    ));
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 0);

    game.watch_shift_accept(&recruit, &karl, "Garrick", &auth)
        .await;
    let hired = drain(&mut recruit_rx);
    assert!(hired.iter().any(|message| matches!(
        message,
        ServerMessage::WatchDuty {
            duty: Some(WatchDuty {
                stage: WatchStage::Waiting,
                gear: WatchGear::Unissued,
                calls: 0,
                answered: 0,
                shovel: false
            })
        }
    )));
    assert!(matches!(
        captain_notes(&mut karl_rx)[0],
        WatchNote::Hired { .. }
    ));

    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchTakeGear"
    );
    assert_eq!(rack(&game, &auth, &mut recruit_rx).await, "");
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 1);
    assert_eq!(worn(&game).await, Some((TABARD_ITEM.to_string(), true)));
    assert_eq!(rack(&game, &auth, &mut recruit_rx).await, "");
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchWaiting"
    );

    call(&game, &auth, 10, PatrolMuster::Out).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchJoined"]);
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchOut"
    );
    assert_eq!(rack(&game, &auth, &mut recruit_rx).await, "");

    for minute in (30..600).step_by(10) {
        call(&game, &auth, minute, PatrolMuster::Stop).await;
    }
    call(&game, &auth, 600, PatrolMuster::Home).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchDone"]);

    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchReturnFirst"
    );
    assert_eq!(game.get_player_gold(&recruit).await, 0);
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid",
        "the wage comes with the return, without another word to the captain"
    );
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 0);
    assert_eq!(worn(&game).await, None);
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchRackNotRecruit"
    );
    assert_eq!(game.get_player_gold(&recruit).await, FULL_WAGE);
    assert_gold_production(&game, GoldSource::WatchWage, 1, FULL_WAGE).await;

    game.watch_shift_visit(&recruit, &karl, true, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchResting"]);
    assert_eq!(game.get_player_gold(&recruit).await, FULL_WAGE);

    set_minute(&game, 600 + 9 * 60);
    hire(&game, &auth, &mut recruit_rx).await;
}

#[tokio::test]
async fn each_watch_takes_one_recruit() {
    let (game, auth, mut recruit_rx, mut karl_rx, _) = setup("watch_full").await;
    let second = create_test_character(&auth, &auth.login_google("Second").unwrap(), "Second");
    let mut second_rx = enter(&game, "Second", second.id, 2.5).await;
    let third = create_test_character(&auth, &auth.login_google("Third").unwrap(), "Third");
    let mut third_rx = enter(&game, "Third", third.id, 2.5).await;

    hire(&game, &auth, &mut recruit_rx).await;
    game.watch_shift_visit(&pid("Second"), &pid("Karl"), true, &auth)
        .await;
    assert_eq!(codes(&mut second_rx), ["server.watchFull"]);
    assert_eq!(captain_notes(&mut karl_rx).last(), Some(&WatchNote::Full));

    post_guard(&game, "Osric").await;
    lead_watch(&game, "Osric", "00:30-10:00");
    game.watch_shift_visit(&pid("Second"), &pid("Karl"), true, &auth)
        .await;
    assert!(drain(&mut second_rx).iter().any(|message| matches!(
        message,
        ServerMessage::WatchOffered { sergeant, .. } if sergeant == "Osric"
    )));
    game.watch_shift_accept(&pid("Second"), &pid("Karl"), "Garrick", &auth)
        .await;
    assert!(
        codes(&mut second_rx).is_empty(),
        "a yes to a place that went is not a yes to the other watch"
    );
    game.watch_shift_accept(&pid("Second"), &pid("Karl"), "Osric", &auth)
        .await;
    assert_eq!(codes(&mut second_rx), ["server.watchHired"]);

    game.watch_shift_visit(&pid("Third"), &pid("Karl"), false, &auth)
        .await;
    assert_eq!(codes(&mut third_rx), ["server.watchFull"]);
    game.watch_shift_accept(&pid("Third"), &pid("Karl"), "Garrick", &auth)
        .await;
    assert_eq!(codes(&mut third_rx), ["server.watchFull"]);

    walk_watch(&game, &auth, 60).await;
    hire_as(&game, &auth, "Third", &mut third_rx).await;
}

#[tokio::test]
async fn a_recruit_who_never_marches_frees_the_place_unpaid() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_no_show").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    stand(&game, "Recruit", 60.0).await;
    walk_watch(&game, &auth, 60).await;
    stand(&game, "Recruit", 2.0).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchReturnFirst"
    );
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchNoShow"
    );
    assert_eq!(game.get_player_gold(&pid("Recruit")).await, 0);
    hire(&game, &auth, &mut recruit_rx).await;
}

#[test]
fn the_wage_is_the_share_of_calls_answered_halved_once_any_is_missed() {
    assert_eq!(watch_wage(200, 200), FULL_WAGE);
    assert_eq!(watch_wage(199, 200), FULL_WAGE * 199 / 200 / 2);
    assert_eq!(watch_wage(100, 200), FULL_WAGE / 4);
    assert_eq!(watch_wage(0, 200), 0);
}

#[tokio::test]
async fn a_missed_roll_call_halves_the_share_of_the_wage() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_missed").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    stand(&game, "Recruit", 40.0).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    assert_eq!(
        codes(&mut recruit_rx).pop().unwrap(),
        "server.watchCalledBack"
    );
    call(&game, &auth, 30, PatrolMuster::Stop).await;
    assert_eq!(
        codes(&mut recruit_rx),
        ["server.watchMissed"],
        "the sergeant waits once, then both calls count against the recruit"
    );
    stand(&game, "Recruit", 2.0).await;
    call(&game, &auth, 40, PatrolMuster::Home).await;

    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchPaidPartial"
    );
    assert_eq!(
        game.get_player_gold(&pid("Recruit")).await,
        watch_wage(2, 4)
    );
}

#[tokio::test]
async fn a_recruit_back_when_called_loses_nothing_and_one_back_late_is_told_so() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_called_back").await;
    let mut sergeant_rx = game.register_direct_channel(&pid("Garrick")).await;
    // Missing, back in time, back late.
    let roll = |rx: &mut DirectRx| {
        drain(rx)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::PatrolRollCalled {
                    missing,
                    rejoined,
                    late,
                } => Some((missing, rejoined, late)),
                _ => None,
            })
            .expect("the roll call is answered")
    };
    let nobody = (
        Vec::<String>::new(),
        Vec::<String>::new(),
        Vec::<String>::new(),
    );
    let named = || vec!["Recruit".to_string()];
    hire(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    assert_eq!(roll(&mut sergeant_rx), nobody);

    stand(&game, "Recruit", 20.0).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    assert_eq!(
        roll(&mut sergeant_rx),
        nobody,
        "seventeen metres is in reach"
    );

    stand(&game, "Recruit", 40.0).await;
    call(&game, &auth, 30, PatrolMuster::Stop).await;
    assert_eq!(roll(&mut sergeant_rx), (named(), vec![], vec![]));
    stand(&game, "Recruit", 2.0).await;
    call(&game, &auth, 31, PatrolMuster::Recheck).await;
    assert_eq!(roll(&mut sergeant_rx), (vec![], named(), vec![]));
    call(&game, &auth, 32, PatrolMuster::Recheck).await;

    // Away again inside five minutes: no wait, the call counts, and the return is late.
    stand(&game, "Recruit", 40.0).await;
    call(&game, &auth, 34, PatrolMuster::Stop).await;
    assert_eq!(roll(&mut sergeant_rx), nobody);
    stand(&game, "Recruit", 2.0).await;
    call(&game, &auth, 36, PatrolMuster::Stop).await;
    assert_eq!(roll(&mut sergeant_rx), (vec![], vec![], named()));
    call(&game, &auth, 38, PatrolMuster::Stop).await;
    assert_eq!(roll(&mut sergeant_rx), nobody);

    call(&game, &auth, 40, PatrolMuster::Home).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaidPartial"
    );
    assert_eq!(
        game.get_player_gold(&pid("Recruit")).await,
        watch_wage(6, 7)
    );
}

#[tokio::test]
async fn the_captain_presents_the_recruit_once_while_the_watch_reports() {
    let (game, auth, mut recruit_rx, mut karl_rx, _) = setup("watch_intro").await;
    hire(&game, &auth, &mut recruit_rx).await;
    drain(&mut karl_rx);
    game.tick_watch_introductions().await;
    assert!(captain_notes(&mut karl_rx).is_empty(), "half an hour early");

    set_minute(&game, 15);
    game.tick_watch_introductions().await;
    let reporting = WatchNote::Reporting {
        sergeant: "Garrick".into(),
    };
    assert_eq!(captain_notes(&mut karl_rx), [reporting]);
    game.tick_watch_introductions().await;
    assert!(captain_notes(&mut karl_rx).is_empty());
}

#[tokio::test]
async fn a_recruit_who_joins_after_the_watch_left_has_missed_the_calls_before() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_late").await;
    hire(&game, &auth, &mut recruit_rx).await;
    stand(&game, "Recruit", 40.0).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    assert!(codes(&mut recruit_rx).is_empty());
    stand(&game, "Recruit", 2.0).await;
    call(&game, &auth, 30, PatrolMuster::Stop).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchJoinedLate"]);
    call(&game, &auth, 40, PatrolMuster::Home).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaidPartial"
    );
    assert_eq!(
        game.get_player_gold(&pid("Recruit")).await,
        watch_wage(2, 4)
    );
}

#[tokio::test]
async fn a_sergeant_carried_far_away_costs_the_recruit_nothing() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_carried").await;
    hire(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    stand(&game, "Garrick", 200.0).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    stand(&game, "Recruit", 199.0).await;
    call(&game, &auth, 30, PatrolMuster::Home).await;
    stand(&game, "Recruit", 2.0).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid"
    );
}

#[tokio::test]
async fn a_watch_that_goes_quiet_without_a_last_call_is_home() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_quiet").await;
    hire(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    set_minute(&game, 40);
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchOut"
    );
    // The sergeant's next call after a long silence belongs to another watch.
    call(&game, &auth, 70, PatrolMuster::Stop).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchDone"]);
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid"
    );
}

#[tokio::test]
async fn lost_gear_docks_a_clean_watch_to_a_silver() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_lost_gear").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    walk_watch(&game, &auth, 60).await;
    game.inventories
        .write()
        .await
        .get_mut(&pid("Recruit"))
        .unwrap()
        .bag
        .clear();
    drain(&mut recruit_rx);
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchPaidLostGear"
    );
    assert_eq!(game.get_player_gold(&pid("Recruit")).await, LOST_GEAR_WAGE);
}

#[tokio::test]
async fn a_watch_survives_a_restart_without_a_mark_against_the_recruit() {
    let (game, auth, mut recruit_rx, _karl_rx, character_id) = setup("watch_restart").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    // A login re-issues every item's instance id.
    let mut kit = game.inventories.read().await[&pid("Recruit")].clone();
    for item in kit.bag.iter_mut().chain(kit.equipped.values_mut()) {
        item.instance_id += 1000;
    }

    let game = make_flat_world_game_state("watch_restart_again");
    let mut recruit_rx = enter(&game, "Recruit", character_id, 30.0).await;
    game.inventories.write().await.insert(pid("Recruit"), kit);
    let _karl_rx = world(&game).await;
    call(&game, &auth, 20, PatrolMuster::Stop).await;
    stand(&game, "Recruit", 2.0).await;
    call(&game, &auth, 30, PatrolMuster::Stop).await;
    call(&game, &auth, 40, PatrolMuster::Home).await;
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid"
    );
    assert_eq!(bag_count(&game, &pid("Recruit"), SPEAR_ITEM).await, 0);
    assert_eq!(game.get_player_gold(&pid("Recruit")).await, FULL_WAGE);
}

#[tokio::test]
async fn the_captain_deals_only_with_someone_standing_with_him() {
    let (game, auth, mut recruit_rx, mut karl_rx, _) = setup("watch_reach").await;
    let (recruit, karl) = (pid("Recruit"), pid("Karl"));

    drain(&mut recruit_rx);
    drain(&mut karl_rx);
    game.watch_shift_visit(&karl, &recruit, true, &auth).await;
    assert!(drain(&mut recruit_rx).is_empty() && captain_notes(&mut karl_rx).is_empty());

    stand(&game, "Recruit", 60.0).await;
    game.watch_shift_visit(&recruit, &karl, true, &auth).await;
    assert_eq!(captain_notes(&mut karl_rx), [WatchNote::NotStanding]);
    game.watch_shift_visit(&recruit, &karl, false, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchTooFar"]);
    game.watch_shift_accept(&recruit, &karl, "Garrick", &auth)
        .await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchTooFar"]);
    game.watch_rack_use(&recruit, RACK, &auth).await;
    assert!(drain(&mut recruit_rx).is_empty());
}

#[tokio::test]
async fn one_character_of_an_account_is_on_the_roll_at_a_time() {
    let (game, auth, mut recruit_rx, mut karl_rx, _) = setup("watch_account").await;
    let (alt, karl) = (pid("Alt"), pid("Karl"));
    let account = auth.login_google("Recruit").unwrap();
    let alt_id = create_test_character(&auth, &account, "Alt").id;
    let mut alt_rx = enter(&game, "Alt", alt_id, 2.5).await;

    hire(&game, &auth, &mut recruit_rx).await;
    walk_watch(&game, &auth, 60).await;
    drain(&mut karl_rx);

    set_minute(&game, 3 * 24 * 60);
    game.watch_shift_visit(&alt, &karl, false, &auth).await;
    assert_eq!(codes(&mut alt_rx), ["server.watchTaken"]);
    assert!(captain_notes(&mut karl_rx).is_empty());

    drain(&mut recruit_rx);
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid"
    );
    game.watch_shift_visit(&alt, &karl, false, &auth).await;
    assert_eq!(codes(&mut alt_rx), ["server.watchResting"]);
}

#[tokio::test]
async fn a_hire_that_never_marched_lapses_and_the_old_spear_is_taken_back() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_lapse").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    set_minute(&game, 25 * 60);
    hire(&game, &auth, &mut recruit_rx).await;
    assert_eq!(bag_count(&game, &pid("Recruit"), SPEAR_ITEM).await, 0);
    assert_eq!(game.get_player_gold(&pid("Recruit")).await, 0);
}

#[tokio::test]
async fn the_lent_tools_work_only_out_with_the_watch() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_tools").await;
    let recruit = pid("Recruit");
    game.watch_tool_use(&recruit, WatchTool::Spear, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchOffDuty"]);

    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    game.watch_tool_use(&recruit, WatchTool::Spear, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchOffDuty"]);

    call(&game, &auth, 10, PatrolMuster::Out).await;
    drain(&mut recruit_rx);
    game.watch_tool_use(&recruit, WatchTool::Spear, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchNothingToClear"]);
    game.watch_tool_use(&recruit, WatchTool::Shovel, &auth)
        .await;
    assert!(
        codes(&mut recruit_rx).is_empty(),
        "cooling down, and no shovel was lent"
    );
}

#[tokio::test]
async fn the_spear_is_refused_away_from_the_sergeant() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_spear_reach").await;
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    drain(&mut recruit_rx);
    stand(&game, "Recruit", 80.0).await;
    game.watch_tool_use(&pid("Recruit"), WatchTool::Spear, &auth)
        .await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchOffDuty"]);
}

#[tokio::test]
async fn a_recruit_hired_after_the_watch_left_walks_its_next_march_a_day_on() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_next").await;
    call(&game, &auth, 40, PatrolMuster::Out).await;
    set_minute(&game, 60);
    hire(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 70, PatrolMuster::Stop).await;
    call(&game, &auth, 80, PatrolMuster::Home).await;
    assert!(codes(&mut recruit_rx).is_empty());
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchTakeGear"
    );

    // A restarted sergeant calls Out again mid-watch: still not theirs.
    call(&game, &auth, 90, PatrolMuster::Out).await;
    call(&game, &auth, 100, PatrolMuster::Home).await;
    assert!(codes(&mut recruit_rx).is_empty());

    let next_day = 24 * 60;
    call(&game, &auth, next_day + 35, PatrolMuster::Out).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchJoined"]);
    for minute in (next_day + 45..next_day + 590).step_by(10) {
        call(&game, &auth, minute, PatrolMuster::Stop).await;
    }
    call(&game, &auth, next_day + 590, PatrolMuster::Home).await;
    assert_eq!(
        report(&game, &auth, &mut recruit_rx).await,
        "server.watchPaid"
    );
    assert_eq!(game.get_player_gold(&pid("Recruit")).await, FULL_WAGE);
}

#[test]
fn the_lent_spear_is_a_plain_unsellable_untradeable_spear() {
    let defs = crate::item_defs::ItemDefs::load();
    let spear = defs.get(SPEAR_ITEM).expect("watch spear is defined");
    assert!(spear.untradeable && spear.base_price.is_none());
    assert_eq!(spear.dice, defs.get("spear").unwrap().dice);
    let tabard = defs.get(TABARD_ITEM).expect("watch tabard is defined");
    assert!(tabard.untradeable && tabard.base_price.is_none() && tabard.is_cape());
    let shovel = defs.get("watch_shovel").expect("watch shovel is defined");
    assert!(shovel.untradeable && shovel.base_price.is_none());
}

#[tokio::test]
async fn the_gear_rack_blocks_the_way_and_is_still_in_reach() {
    use onlinerpg_shared::pathfinding::is_movement_blocked;
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_rack_solid").await;
    let across =
        |x: f32| is_movement_blocked(&game.passability_read(), x, 2.5, x, 4.5, 0, Some(0.0));
    assert!(across(4.0), "the rack stands in the way");
    assert!(!across(8.0));
    assert_eq!(
        rack(&game, &auth, &mut recruit_rx).await,
        "server.watchRackNotRecruit"
    );
}

fn rack_state(rx: &mut DirectRx) -> Option<(Vec<String>, Vec<String>, bool)> {
    drain(rx)
        .into_iter()
        .rev()
        .find_map(|message| match message {
            ServerMessage::WatchRackState {
                on_rack,
                owed,
                returnable,
                ..
            } => Some((on_rack, owed, returnable)),
            _ => None,
        })
}

#[tokio::test]
async fn gear_is_taken_from_the_rack_and_put_back_piece_by_piece() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_rack_pieces").await;
    let recruit = pid("Recruit");
    let names = |kinds: &[&str]| kinds.iter().map(|k| k.to_string()).collect::<Vec<_>>();
    hire(&game, &auth, &mut recruit_rx).await;

    game.watch_rack_open(&recruit, RACK, &auth).await;
    assert_eq!(
        rack_state(&mut recruit_rx),
        Some((names(&[SPEAR_ITEM, TABARD_ITEM]), vec![], true))
    );

    // Taken to the bag, then put back before the march: a change of mind.
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, true, false, &auth)
        .await;
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, true, false, &auth)
        .await;
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 1);
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, false, false, &auth)
        .await;
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 0);

    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, true, false, &auth)
        .await;
    game.watch_rack_move(&recruit, RACK, TABARD_ITEM, true, true, &auth)
        .await;
    assert_eq!(worn(&game).await, Some((TABARD_ITEM.to_string(), true)));
    assert_eq!(
        rack_state(&mut recruit_rx),
        Some((vec![], names(&[SPEAR_ITEM, TABARD_ITEM]), true))
    );

    call(&game, &auth, 10, PatrolMuster::Out).await;
    drain(&mut recruit_rx);
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, false, false, &auth)
        .await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchRackLater"]);
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 1);

    call(&game, &auth, 20, PatrolMuster::Home).await;
    drain(&mut recruit_rx);
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, false, false, &auth)
        .await;
    assert_eq!(
        rack_state(&mut recruit_rx),
        Some((vec![], names(&[TABARD_ITEM]), true))
    );
    assert_eq!(game.get_player_gold(&recruit).await, 0);
    game.watch_rack_move(&recruit, RACK, TABARD_ITEM, false, false, &auth)
        .await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchPaid"]);
    assert_eq!(game.get_player_gold(&recruit).await, FULL_WAGE);
    assert_eq!(worn(&game).await, None);
}

#[tokio::test]
async fn gear_that_cannot_go_back_is_written_off_when_the_recruit_settles() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_rack_settle").await;
    let recruit = pid("Recruit");
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    walk_watch(&game, &auth, 60).await;
    game.inventories
        .write()
        .await
        .get_mut(&recruit)
        .unwrap()
        .bag
        .clear();
    drain(&mut recruit_rx);
    game.watch_rack_move(&recruit, RACK, SPEAR_ITEM, false, false, &auth)
        .await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchNoTool"]);
    game.watch_rack_settle(&recruit, RACK, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchPaidLostGear"]);
    assert_eq!(game.get_player_gold(&recruit).await, LOST_GEAR_WAGE);
    assert_eq!(worn(&game).await, None, "what was still held is taken back");
}

#[tokio::test]
async fn a_recruit_logging_in_is_told_where_they_stand_before_joining_the_roster() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_greet").await;
    hire(&game, &auth, &mut recruit_rx).await;
    drain(&mut recruit_rx);
    let later = pid("NotYetInTheRoster");
    let mut later_rx = game.register_direct_channel(&later).await;
    game.greet_watch_recruit(&later, "Recruit", &auth).await;
    assert!(matches!(
        &drain(&mut later_rx)[..],
        [ServerMessage::WatchDuty {
            duty: Some(WatchDuty {
                stage: WatchStage::Waiting,
                ..
            })
        }]
    ));
    game.greet_watch_recruit(&later, "Stranger", &auth).await;
    assert!(matches!(
        &drain(&mut later_rx)[..],
        [ServerMessage::WatchDuty { duty: None }]
    ));
}

#[tokio::test]
async fn a_recruit_may_give_up_before_the_watch_is_home_and_frees_the_place() {
    let (game, auth, mut recruit_rx, _karl_rx, _) = setup("watch_quit").await;
    let recruit = pid("Recruit");
    hire_and_draw(&game, &auth, &mut recruit_rx).await;
    call(&game, &auth, 10, PatrolMuster::Out).await;
    drain(&mut recruit_rx);

    game.watch_shift_quit(&recruit, &auth).await;
    assert_eq!(codes(&mut recruit_rx), ["server.watchQuit"]);
    assert_eq!(bag_count(&game, &recruit, SPEAR_ITEM).await, 0);
    assert_eq!(worn(&game).await, None);
    assert_eq!(game.get_player_gold(&recruit).await, 0);
    hire(&game, &auth, &mut recruit_rx).await;

    walk_watch(&game, &auth, 60).await;
    drain(&mut recruit_rx);
    game.watch_shift_quit(&recruit, &auth).await;
    assert!(
        codes(&mut recruit_rx).is_empty(),
        "a watch that is home is settled, not quit"
    );
}
