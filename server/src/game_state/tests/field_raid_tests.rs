// ---- Altar offerings and the field raid -------------------------------------

use super::*;
use onlinerpg_shared::field_boss::{field_boss_defs, AltarDef, FieldBossDef};
use onlinerpg_shared::inventory::EquipSlot;
use onlinerpg_shared::messages::{AltarStatus, FieldRaidOutcome};
use onlinerpg_shared::moon::Moon;
use onlinerpg_shared::pathfinding::is_movement_blocked;
use onlinerpg_shared::world::GameDateTime;

const SUMMONER_CHARACTER: i64 = 11;
const GROUND: f32 = 5.0;

fn terraclash() -> &'static FieldBossDef {
    field_boss_defs()
        .iter()
        .find(|d| d.id == "terraclash")
        .expect("terraclash row")
}

fn altar() -> &'static AltarDef {
    terraclash().altar.as_ref().expect("terraclash altar")
}

fn altar_id() -> u32 {
    terraclash().altar_placement_id()
}

/// Day 14 of the calendar, midnight: Eldor is full and overhead.
fn full_moon() -> GameDateTime {
    GameDateTime {
        year: 217,
        month: 1,
        day: 15,
        hour: 0,
        minute: 0,
    }
}

fn half_moon() -> GameDateTime {
    GameDateTime {
        day: 10,
        hour: 12,
        ..full_moon()
    }
}

async fn setup(when: GameDateTime, amulets: u32) -> (GameState, PlayerId, DirectRx) {
    let game = make_test_game_state("field_raid");
    game.debug_set_datetime(&when);
    game.place_field_bosses(field_boss_defs().iter().map(|b| (b, (GROUND, GROUND))));
    let boss = terraclash();
    let (rx, rz) = super::super::passability::region_of(boss.x, boss.z);
    game.sync_region_furniture(rx, rz, &[]);
    let (id, rx) = join_raid(&game, "summoner", SUMMONER_CHARACTER).await;
    if amulets > 0 {
        game.inventories
            .write()
            .await
            .get_mut(&id)
            .unwrap()
            .bag
            .push(bag_item(41, "earth_amulet", amulets));
    }
    (game, id, rx)
}

/// A raider beside the altar, within reach of it and of the battle zone.
async fn join_raid(game: &GameState, name: &str, character: i64) -> (PlayerId, DirectRx) {
    let mut player = make_player(name, altar().x - 1.5, altar().z);
    player.position.y = GROUND;
    let id = player.id;
    game.add_player(player).await;
    game.register_player_character(&id, character, 0, attrs_with_cha(10), 0, None)
        .await;
    let rx = game.register_direct_channel(&id).await;
    game.inventories
        .write()
        .await
        .insert(id, PlayerInventory::default());
    (id, rx)
}

async fn amulets(game: &GameState, id: &PlayerId) -> u32 {
    bag_count(game, id, "earth_amulet").await
}

fn statuses(rx: &mut DirectRx) -> Vec<AltarStatus> {
    drain(rx)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::AltarStatus { status, .. } => Some(status),
            _ => None,
        })
        .collect()
}

fn helms(inv: &PlayerInventory) -> usize {
    inv.bag
        .iter()
        .filter(|i| i.item_def_id == "helm_of_brilliance")
        .count()
}

/// Offer, then tick until the boss stands.
async fn wake(game: &GameState, id: &PlayerId) -> crate::types::Monster {
    game.offer_at_altar(id, altar_id()).await;
    for _ in 1..super::super::field_raid::WAKE_TICKS {
        game.tick_field_raid().await;
        assert!(bosses(game).await.is_empty(), "still crumbling");
    }
    game.tick_field_raid().await;
    bosses(game).await.pop().expect("terraclash awake")
}

async fn kill(game: &GameState, boss: &crate::types::Monster) {
    if let Some(boss) = game.monsters.write().await.get_mut(&boss.id) {
        boss.state = MonsterState::Dead;
        boss.health = 0;
    }
    game.end_field_raid(FieldRaidOutcome::Defeated, None).await;
}

type Notice = (Option<String>, Option<(String, u32)>);

fn notices(rx: &mut DirectRx) -> Vec<Notice> {
    drain(rx)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::FieldRaidNotice { reward, tokens, .. } => Some((reward, tokens)),
            _ => None,
        })
        .collect()
}

async fn bosses(game: &GameState) -> Vec<crate::types::Monster> {
    game.monsters
        .read()
        .await
        .values()
        .filter(|m| m.monster_type == "terraclash")
        .cloned()
        .collect()
}

#[tokio::test]
async fn offering_consumes_one_amulet_and_opens_one_raid() {
    let (game, id, mut rx) = setup(full_moon(), 2).await;
    game.inspect_altar(&id, altar_id()).await;
    assert!(matches!(statuses(&mut rx)[..], [AltarStatus::Ready { .. }]));

    game.offer_at_altar(&id, altar_id()).await;
    assert_eq!(amulets(&game, &id).await, 1);
    assert!(game.field_raid.lock().await.is_some());

    drain(&mut rx);
    game.offer_at_altar(&id, altar_id()).await;
    assert_eq!(
        amulets(&game, &id).await,
        1,
        "a second offering keeps its amulet"
    );
    assert_eq!(statuses(&mut rx), vec![AltarStatus::RaidActive]);
    assert_eq!(
        raid_events(&game),
        ["summoned"],
        "a refused offering is no summon"
    );
}

#[tokio::test]
async fn offering_outside_a_full_moon_keeps_the_amulet() {
    let (game, id, mut rx) = setup(half_moon(), 1).await;
    game.offer_at_altar(&id, altar_id()).await;
    assert_eq!(amulets(&game, &id).await, 1);
    assert!(game.field_raid.lock().await.is_none());
    assert_eq!(statuses(&mut rx), vec![AltarStatus::NotFullMoon]);
}

#[tokio::test]
async fn a_worn_amulet_must_come_off_first() {
    let (game, id, mut rx) = setup(full_moon(), 0).await;
    game.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .equipped
        .insert(EquipSlot::Neck, bag_item(42, "earth_amulet", 1));
    game.inspect_altar(&id, altar_id()).await;
    game.offer_at_altar(&id, altar_id()).await;
    let expected = AltarStatus::OfferingEquipped {
        offering: "earth_amulet".into(),
    };
    assert_eq!(statuses(&mut rx), vec![expected.clone(), expected]);
    assert!(game.inventories.read().await[&id]
        .equipped
        .contains_key(&EquipSlot::Neck));
}

#[tokio::test]
async fn an_out_of_reach_altar_is_ignored() {
    let (game, id, mut rx) = setup(full_moon(), 1).await;
    game.players.write().await.get_mut(&id).unwrap().position.z = altar().z + 10.0;
    game.offer_at_altar(&id, altar_id()).await;
    assert_eq!(amulets(&game, &id).await, 1);
    assert!(statuses(&mut rx).is_empty());
}

#[tokio::test]
async fn the_boss_wakes_at_the_altar_spawn_and_its_death_ends_the_raid() {
    let (game, id, _rx) = setup(full_moon(), 1).await;
    let (wx, wz) = (terraclash().x, terraclash().z);

    let boss = wake(&game, &id).await;
    assert_eq!(boss.lifecycle, crate::types::MonsterLifecycle::FieldRaid);
    assert!((boss.position.x - wx).abs() < 0.01 && (boss.position.z - wz).abs() < 0.01);

    kill(&game, &boss).await;
    assert!(game.field_raid.lock().await.is_none());
    assert_eq!(raid_events(&game), ["summoned", "defeated"]);
}

/// Summons, then finished raids by outcome.
fn raid_events(game: &GameState) -> Vec<&'static str> {
    let mut events: Vec<_> = game
        .pending_field_raid_events()
        .iter()
        .map(crate::metrics::FieldRaidEvent::storage_key)
        .collect();
    events.extend(
        game.pending_field_raid_records()
            .iter()
            .map(|r| crate::metrics::outcome_key(r.outcome)),
    );
    events
}

#[tokio::test]
async fn the_stand_in_stops_blocking_while_its_boss_is_up() {
    let (game, id, _rx) = setup(full_moon(), 1).await;
    // Walking east into the stand-in, short of the awake body (front 3.2 m).
    let (x, z) = (terraclash().x - 4.5, terraclash().z);
    let blocked = |game: &GameState| {
        is_movement_blocked(&game.passability_read(), x - 2.5, z, x, z, 0, Some(GROUND))
    };
    assert!(blocked(&game));

    let boss = wake(&game, &id).await;
    assert!(!blocked(&game));
    let (rx, rz) = super::super::passability::region_of(x, z);
    game.sync_region_furniture(rx, rz, &[]);
    assert!(!blocked(&game), "a region resync keeps it lifted");

    kill(&game, &boss).await;
    assert!(blocked(&game));
}

#[tokio::test]
async fn the_offering_tells_those_around_the_boss_is_waking() {
    let (game, id, mut rx) = setup(full_moon(), 1).await;
    game.offer_at_altar(&id, altar_id()).await;
    assert!(drain(&mut rx).iter().any(
        |m| matches!(m, ServerMessage::FieldBossWaking { monster, .. } if monster == "terraclash")
    ));
}

#[tokio::test]
async fn a_summoner_who_fought_to_the_end_gets_the_helm_once() {
    let (game, id, mut rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &id).await;
    game.record_field_raid_hit(&id, 40).await;
    drain(&mut rx);
    kill(&game, &boss).await;
    assert_eq!(helms(&game.inventories.read().await[&id]), 1);
    assert_eq!(
        notices(&mut rx),
        vec![(
            Some("helm_of_brilliance".to_string()),
            Some(("terraclash_token".to_string(), 1))
        )]
    );

    game.tick_field_raid().await;
    assert_eq!(helms(&game.inventories.read().await[&id]), 1);
}

#[tokio::test]
async fn a_summoner_who_never_hit_or_lay_dead_gets_no_helm() {
    let (game, id, mut rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &id).await;
    drain(&mut rx);
    kill(&game, &boss).await;
    assert_eq!(helms(&game.inventories.read().await[&id]), 0);
    assert_eq!(notices(&mut rx), vec![(None, None)]);

    let (game, id, _rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &id).await;
    game.record_field_raid_hit(&id, 40).await;
    game.players.write().await.get_mut(&id).unwrap().health = 0;
    kill(&game, &boss).await;
    assert_eq!(helms(&game.inventories.read().await[&id]), 0);
}

#[tokio::test]
async fn an_empty_battle_zone_puts_the_boss_back_to_sleep() {
    let (game, id, _rx) = setup(full_moon(), 1).await;
    wake(&game, &id).await;

    game.teleport_player(&id, pos3(altar().x + 200.0, GROUND, altar().z), 0.0, 0)
        .await;
    game.tick_field_raid().await;
    assert!(game.field_raid.lock().await.is_none());
    assert!(bosses(&game).await.is_empty());
    assert_eq!(raid_events(&game), ["summoned", "wiped"]);
    assert_eq!(
        amulets(&game, &id).await,
        0,
        "a failed raid keeps the offering"
    );
}

/// Full moon on the 15th of `month`, at `hour:minute`.
fn full_moon_at(month: u8, hour: u8, minute: u8) -> GameDateTime {
    GameDateTime {
        month,
        hour,
        minute,
        ..full_moon()
    }
}

/// Offer at midnight on `month`'s full moon and wake the boss.
async fn raid_in(month: u8) -> (GameState, DirectRx) {
    let (game, id, mut rx) = setup(full_moon_at(month, 0, 0), 1).await;
    wake(&game, &id).await;
    drain(&mut rx);
    (game, rx)
}

async fn tick_at(game: &GameState, at: GameDateTime) {
    game.debug_set_datetime(&at);
    game.tick_field_raid().await;
}

/// The raid's last messages: whether the boss was put to sleep, then how
/// the raid ended.
fn ending(rx: &mut DirectRx) -> (bool, Option<FieldRaidOutcome>) {
    let (mut slept, mut outcome) = (false, None);
    for m in drain(rx) {
        match m {
            ServerMessage::FieldBossSleeping { .. } => slept = outcome.is_none(),
            ServerMessage::FieldRaidNotice { outcome: o, .. } => outcome = Some(o),
            _ => {}
        }
    }
    (slept, outcome)
}

#[tokio::test]
async fn the_raid_lasts_through_the_dawn_glow_until_the_sky_turns_blue() {
    use onlinerpg_shared::celestial::{is_full_daylight, sun_elevation};
    let (game, mut rx) = raid_in(4).await;

    let glow = full_moon_at(4, 5, 50);
    assert!(sun_elevation(&glow) > 0.0 && !is_full_daylight(&glow));
    tick_at(&game, glow).await;
    assert!(
        game.field_raid.lock().await.is_some(),
        "the sun is still rising"
    );

    let day = full_moon_at(4, 6, 5);
    assert!(is_full_daylight(&day) && Moon::Eldor.is_up(&day));
    tick_at(&game, day).await;
    assert!(game.field_raid.lock().await.is_none());
    assert!(bosses(&game).await.is_empty());
    assert_eq!(ending(&mut rx), (true, Some(FieldRaidOutcome::Dawn)));
}

#[tokio::test]
async fn a_winter_raid_ends_when_its_moon_sets_before_sunrise() {
    let (game, mut rx) = raid_in(1).await;

    tick_at(&game, full_moon_at(1, 6, 0)).await;
    assert!(game.field_raid.lock().await.is_some());

    let set = full_moon_at(1, 6, 20);
    assert!(!Moon::Eldor.is_up(&set) && GameState::is_night(&set));
    tick_at(&game, set).await;
    assert!(game.field_raid.lock().await.is_none());
    assert_eq!(ending(&mut rx), (true, Some(FieldRaidOutcome::MoonSet)));
}

async fn tokens(game: &GameState, id: &PlayerId) -> u32 {
    bag_count(game, id, "terraclash_token").await
}

#[test]
fn each_damage_tier_reached_adds_a_token() {
    let token = terraclash().token.as_ref().expect("terraclash token");
    assert_eq!(token.tiers, [1, 1000, 3000]);
    let counts = [0, 1, 999, 1000, 2999, 3000, 80_000].map(|d| token.count_for(d));
    assert_eq!(counts, [0, 1, 1, 2, 2, 3, 3]);
}

#[test]
fn the_token_is_a_bound_stack() {
    let game = make_test_game_state("field_raid_token");
    let token = &terraclash().token.as_ref().expect("terraclash token").item;
    let def = game.item_defs.get(token).expect("token item");
    assert!(def.stackable);
    assert!(game.item_defs.untradeable(token));
}

#[tokio::test]
async fn raiders_alive_at_the_kill_earn_tokens_by_damage_dealt() {
    let (game, summoner, mut summoner_rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &summoner).await;
    let (top, _) = join_raid(&game, "top", 12).await;
    let (second, _) = join_raid(&game, "second", 13).await;
    let (third, _) = join_raid(&game, "third", 14).await;
    let (idle, mut idle_rx) = join_raid(&game, "idle", 15).await;
    let (fallen, _) = join_raid(&game, "fallen", 16).await;
    for (id, damage) in [
        (&summoner, 1000),
        (&top, 3500),
        (&second, 3000),
        (&third, 999),
        (&fallen, 5000),
    ] {
        game.record_field_raid_hit(id, damage).await;
    }
    game.players.write().await.get_mut(&fallen).unwrap().health = 0;
    drain(&mut summoner_rx);
    drain(&mut idle_rx);
    kill(&game, &boss).await;

    for (id, expected) in [(&top, 3), (&second, 3), (&third, 1), (&summoner, 2)] {
        assert_eq!(tokens(&game, id).await, expected);
    }
    assert_eq!(tokens(&game, &idle).await, 0);
    assert_eq!(tokens(&game, &fallen).await, 0);
    assert_eq!(
        notices(&mut summoner_rx),
        vec![(
            Some("helm_of_brilliance".to_string()),
            Some(("terraclash_token".to_string(), 2))
        )]
    );
    assert_eq!(notices(&mut idle_rx), vec![(None, None)]);

    game.tick_field_raid().await;
    assert_eq!(tokens(&game, &top).await, 3, "paid once");
}

#[tokio::test]
async fn a_reconnect_keeps_the_raiders_damage() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &summoner).await;
    let (before, _) = join_raid(&game, "before", 12).await;
    let (rival, _) = join_raid(&game, "rival", 13).await;
    game.record_field_raid_hit(&before, 600).await;
    game.record_field_raid_hit(&rival, 40).await;
    game.unregister_player_character(&before).await;
    game.remove_player(&before).await;
    let (after, _) = join_raid(&game, "after", 12).await;
    game.record_field_raid_hit(&after, 500).await;
    kill(&game, &boss).await;

    assert_eq!(tokens(&game, &after).await, 2);
    assert_eq!(tokens(&game, &rival).await, 1);
}

#[tokio::test]
async fn raiders_earn_the_raid_title_and_the_last_hit_the_final_blow_too() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &summoner).await;
    join_raid(&game, "killer", 12).await;
    join_raid(&game, "fallen", 13).await;
    join_raid(&game, "idle", 14).await;
    for name in ["summoner", "killer", "fallen"] {
        game.record_field_raid_hit(&pid(name), 40).await;
    }
    game.players
        .write()
        .await
        .get_mut(&pid("fallen"))
        .unwrap()
        .health = 0;
    game.monsters.write().await.get_mut(&boss.id).unwrap().state = MonsterState::Dead;
    game.tick_field_raid().await;
    assert!(
        game.field_raid.lock().await.is_some(),
        "the tick leaves a dead boss to its killing blow"
    );
    game.settle_field_raid_kill(&pid("killer"), 40, None).await;
    assert!(game.field_raid.lock().await.is_none());

    let raider = || "terraclash_raider".to_string();
    let final_blow = || "terraclash_final_blow".to_string();
    assert_eq!(
        titles_of(&game, "summoner").await,
        (vec![raider()], Some(raider()))
    );
    assert_eq!(
        titles_of(&game, "killer").await,
        (vec![raider(), final_blow()], Some(final_blow()))
    );
    assert_eq!(titles_of(&game, "fallen").await, (vec![], None));
    assert_eq!(titles_of(&game, "idle").await, (vec![], None));
}

#[tokio::test]
async fn the_final_blow_needs_no_place_among_the_raiders() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    wake(&game, &summoner).await;
    let (killer, _) = join_raid(&game, "killer", 12).await;
    game.record_field_raid_hit(&summoner, 40).await;
    game.players.write().await.get_mut(&killer).unwrap().health = 0;
    game.settle_field_raid_kill(&killer, 40, None).await;

    let final_blow = "terraclash_final_blow".to_string();
    assert_eq!(
        titles_of(&game, "killer").await,
        (vec![final_blow.clone()], Some(final_blow))
    );
    assert_eq!(tokens(&game, &killer).await, 0);
}

#[test]
fn the_raid_boss_shrugs_off_hit_stagger() {
    let game = make_test_game_state("field_raid_stagger");
    assert!(game.monster_defs.get("terraclash").unwrap().stagger_immune);
}

fn raider<'a>(
    record: &'a crate::metrics::FieldRaidRecord,
    name: &str,
) -> &'a crate::metrics::FieldRaidRaider {
    record
        .raiders
        .iter()
        .find(|r| r.name == name)
        .expect("raider in the record")
}

#[tokio::test]
async fn a_failed_raid_records_who_was_there_and_what_each_did() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    let (early, _) = join_raid(&game, "early", 12).await;
    let boss = wake(&game, &summoner).await;
    let (late, _) = join_raid(&game, "late", 13).await;
    game.tick_field_raid().await;
    game.record_field_raid_hit(&summoner, 500).await;
    game.record_field_raid_hit(&late, 200).await;
    game.record_field_raid_taken(&early, 300).await;
    game.record_field_raid_death(&early).await;
    for (dx, floor_level) in [(200.0, 0), (-200.0, -3)] {
        let mut players = game.players.write().await;
        let player = players.get_mut(&early).unwrap();
        player.position.x += dx;
        player.floor_level = floor_level;
        drop(players);
        game.record_field_raid_death(&early).await;
    }
    game.players.write().await.get_mut(&early).unwrap().health = 0;
    let (passer, _) = join_raid(&game, "passer", 14).await;
    game.record_field_raid_taken(&passer, 50).await;
    game.players.write().await.get_mut(&passer).unwrap().health = 0;
    game.monsters
        .write()
        .await
        .get_mut(&boss.id)
        .unwrap()
        .health = boss.max_health - 700;
    game.end_field_raid(FieldRaidOutcome::Dawn, None).await;

    let records = game.pending_field_raid_records().clone();
    let [record] = records.as_slice() else {
        panic!("one record per raid: {records:?}");
    };
    assert_eq!(record.boss, "terraclash");
    assert_eq!(record.outcome, FieldRaidOutcome::Dawn);
    assert_eq!(record.summoner_id, Some(SUMMONER_CHARACTER));
    assert_eq!(
        (record.boss_health, record.boss_max_health),
        (boss.max_health - 700, boss.max_health)
    );
    assert!(record.ended_at >= record.summoned_at);
    let names: Vec<_> = record.raiders.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["summoner", "late", "early", "passer"],
        "by damage dealt"
    );
    let facts = |r: &crate::metrics::FieldRaidRaider| {
        (
            r.dealt,
            r.taken,
            r.deaths,
            r.near_at_summon,
            r.in_zone,
            r.in_zone_at_end,
        )
    };
    assert_eq!(
        facts(raider(record, "summoner")),
        (500, 0, 0, true, true, true)
    );
    assert_eq!(
        facts(raider(record, "late")),
        (200, 0, 0, false, true, true)
    );
    assert_eq!(
        facts(raider(record, "early")),
        (0, 300, 1, true, true, false),
        "a death counts only in the battle zone on the surface"
    );
    assert_eq!(
        facts(raider(record, "passer")),
        (0, 50, 0, false, true, false),
        "a blow between ticks still puts the raider in the zone"
    );
}

#[tokio::test]
async fn a_raider_who_never_hit_the_boss_earns_nothing() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &summoner).await;
    let (watcher, _) = join_raid(&game, "watcher", 12).await;
    game.tick_field_raid().await;
    game.record_field_raid_hit(&summoner, 40).await;
    kill(&game, &boss).await;

    assert_eq!(tokens(&game, &watcher).await, 0);
    assert_eq!(titles_of(&game, "watcher").await, (vec![], None));
    let records = game.pending_field_raid_records().clone();
    assert_eq!(records[0].boss_health, 0);
    assert!(raider(&records[0], "watcher").in_zone_at_end);
}

#[tokio::test]
async fn the_raid_boss_s_blows_count_as_damage_taken() {
    let (game, summoner, _rx) = setup(full_moon(), 1).await;
    let boss = wake(&game, &summoner).await;
    {
        let mut players = game.players.write().await;
        let player = players.get_mut(&summoner).unwrap();
        player.position = Position {
            x: boss.position.x + 5.0,
            ..boss.position
        };
        player.health = 10_000;
        player.max_health = 10_000;
        player.ready_at = 0;
    }
    for _ in 0..50 {
        game.monsters
            .write()
            .await
            .get_mut(&boss.id)
            .unwrap()
            .last_attack_at = 0;
        game.monster_attack(&boss.id, &summoner).await;
        if game.players.read().await[&summoner].health < 10_000 {
            break;
        }
    }
    let lost = 10_000 - game.players.read().await[&summoner].health;
    assert!(lost > 0, "terraclash lands a blow");
    game.end_field_raid(FieldRaidOutcome::Dawn, None).await;
    let records = game.pending_field_raid_records().clone();
    assert_eq!(raider(&records[0], "summoner").taken, u64::from(lost));
}
