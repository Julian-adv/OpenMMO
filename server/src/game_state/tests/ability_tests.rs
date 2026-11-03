use super::*;
use onlinerpg_shared::ability::{
    AbilityId, AbilityRejectReason, InspectionResult, InspectionTarget,
};
use onlinerpg_shared::hunger::SATIATION_START;
use std::time::Duration;

const WARD: AbilityId = AbilityId::GuardianWard;
const RADIANCE: AbilityId = AbilityId::Radiance;
const MARK: AbilityId = AbilityId::BowMark;

#[tokio::test]
async fn abilities_and_save_complete_with_queued_writers() {
    for ability in [WARD, RADIANCE, MARK, AbilityId::Auscultation] {
        let gs = make_test_game_state(&format!("ability_save_locks_{ability:?}"));
        let _rx = if ability == MARK {
            add_mark_player(&gs, "caster", 1).await
        } else {
            add_ward_player(&gs, "caster", 0.0, 1).await
        };
        if ability == AbilityId::Auscultation {
            gs.inventories
                .write()
                .await
                .get_mut(&pid("caster"))
                .unwrap()
                .equipped
                .insert(EquipSlot::Neck, bag_item(4, "stethoscope", 1));
        }
        add_mark_target(&gs, "target", 1.0).await;
        gs.monsters
            .write()
            .await
            .get_mut("target")
            .unwrap()
            .monster_type = "goblin".into();
        let id = pid("caster");
        let chars = gs.player_characters.write().await;
        let cast = futures_util::future::maybe_done(gs.use_targeted_ability(
            &id,
            ability,
            Some("target"),
            None,
            None,
        ));
        let save = futures_util::future::maybe_done(gs.get_player_save_data(&id));
        let ready = futures_util::future::maybe_done(gs.mark_world_ready(&id));
        let unequip = futures_util::future::maybe_done(gs.unequip_item(&id, EquipSlot::OffHand));
        tokio::pin!(cast, save, ready, unequip);
        assert!(futures_util::poll!(cast.as_mut()).is_pending());
        assert!(futures_util::poll!(save.as_mut()).is_pending());
        let _ = futures_util::poll!(ready.as_mut());
        let _ = futures_util::poll!(unequip.as_mut());
        drop(chars);
        let result = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::join!(cast, save, ready, unequip);
        })
        .await;
        assert!(
            result.is_ok(),
            "{ability:?} must not deadlock with saving and equipment changes"
        );
    }
}

async fn add_mark_player(gs: &GameState, name: &str, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, 0.0, character).await;
    let mut inventories = gs.inventories.write().await;
    let inv = inventories.get_mut(&pid(name)).unwrap();
    inv.equipped.clear();
    inv.equipped
        .insert(EquipSlot::MainHand, bag_item(1, "bow", 1));
    inv.bag.push(bag_item(3, "iron_arrow", 100));
    rx
}

async fn add_mark_target(gs: &GameState, id: &str, x: f32) {
    let mut target = make_monster(id, Position { x, y: 0.0, z: 0.0 }, 0);
    target.health = 1000;
    target.max_health = 1000;
    gs.monsters.write().await.insert(id.to_owned(), target);
}

#[tokio::test]
async fn bow_mark_does_not_bypass_attack_reach_ammo_or_line_of_sight() {
    let gs = make_test_game_state("bow_mark_attack_gates");
    let mut rx = add_mark_player(&gs, "caster", 1).await;
    add_mark_target(&gs, "target", 8.5).await;
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .position
        .z = 0.5;
    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .position
        .z = 0.5;
    gs.sync_region_furniture(0, 0, &[table_placement(4.5, 0.5)]);
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.sync_region_furniture(0, 0, &[]);
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
    drain(&mut rx);
    gs.sync_region_furniture(0, 0, &[table_placement(4.5, 0.5)]);
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerAttackRejected {
            reason: AttackRejectReason::OutOfRange,
            ..
        }
    )));
    gs.sync_region_furniture(0, 0, &[]);
    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .position
        .x = 20.0;
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerAttackRejected {
            reason: AttackRejectReason::OutOfRange,
            ..
        }
    )));
    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .position
        .x = 8.5;
    gs.inventories
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .bag
        .clear();
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttackRejected { .. })));
    assert_eq!(gs.monsters.read().await.get("target").unwrap().health, 1000);
    gs.inventories
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .bag
        .push(bag_item(3, "iron_arrow", 100));
    gs.inventories
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .equipped
        .get_mut(&EquipSlot::MainHand)
        .unwrap()
        .enchant = -1000;
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(drain(&mut rx).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerAttacked {
            hit: true,
            damage: 1..,
            ..
        }
    )));
    assert!(gs.monsters.read().await.get("target").unwrap().health < 1000);
    assert_eq!(
        gs.inventories.read().await[&pid("caster")].bag[0].quantity,
        99
    );
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(!drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
    assert_eq!(
        gs.inventories.read().await[&pid("caster")].bag[0].quantity,
        99
    );
}

#[tokio::test]
async fn bow_mark_rejects_dead_loading_and_mounted_casters() {
    let gs = make_test_game_state("bow_mark_caster_gates");
    let mut rx = add_mark_player(&gs, "caster", 1).await;
    add_mark_target(&gs, "target", 5.0).await;
    for (health, ready_at, mounted) in [
        (0, 0, false),
        (10, GameState::now_ms() + 30_000, false),
        (10, 0, true),
    ] {
        {
            let mut players = gs.players.write().await;
            let caster = players.get_mut(&pid("caster")).unwrap();
            caster.health = health;
            caster.ready_at = ready_at;
            caster.mount = mounted.then_some(onlinerpg_shared::mount::MountKind::Horse);
        }
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
            .await;
        rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .mount = None;
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
}

#[tokio::test(start_paused = true)]
async fn bow_mark_is_private_and_only_guarantees_its_casters_selected_target() {
    let gs = make_test_game_state("bow_mark_private");
    let mut caster_rx = add_mark_player(&gs, "caster", 1).await;
    let mut party_rx = add_mark_player(&gs, "party", 2).await;
    party(&gs, "caster", "party").await;
    add_mark_target(&gs, "target", 5.0).await;
    drain(&mut caster_rx);
    drain(&mut party_rx);
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    let state = gs.abilities.read().await;
    assert!(state.marks_target(&pid("caster"), "target"));
    assert!(!state.marks_target(&pid("caster"), "other"));
    assert!(!state.marks_target(&pid("party"), "target"));
    drop(state);
    let updates = drain(&mut caster_rx);
    assert!(updates.iter().any(|m| matches!(m, ServerMessage::BowMarkUpdate { monster_id: Some(id), remaining_ms: 5000 } if id == "target")));
    assert!(updates.iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == MARK && b.remaining_ms == 5000))));
    assert!(!updates
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    assert!(drain(&mut party_rx).is_empty());
    tokio::time::advance(Duration::from_millis(4999)).await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
    tokio::time::advance(Duration::from_millis(1)).await;
    assert!(!gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
    gs.tick_buffs().await;
    assert!(drain(&mut caster_rx).iter().any(|m| matches!(
        m,
        ServerMessage::BowMarkUpdate {
            monster_id: None,
            remaining_ms: 0
        }
    )));
    assert!(drain(&mut party_rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn bow_mark_cooldown_is_atomic_and_survives_reconnect() {
    let gs = make_test_game_state("bow_mark_cooldown");
    let mut rx = add_mark_player(&gs, "caster", 1).await;
    add_mark_target(&gs, "target", 5.0).await;
    let caster = pid("caster");
    tokio::join!(
        gs.use_targeted_ability(&caster, MARK, Some("target"), None, None),
        gs.use_targeted_ability(&caster, MARK, Some("target"), None, None)
    );
    let updates = drain(&mut rx);
    assert_eq!(
        updates
            .iter()
            .filter(|m| matches!(
                m,
                ServerMessage::BowMarkUpdate {
                    monster_id: Some(_),
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(updates.iter().any(|m| matches!(
        m,
        ServerMessage::AbilityRejected {
            reason: AbilityRejectReason::Cooldown,
            ..
        }
    )));
    gs.add_player(make_player("spectator", 0.0, 0.0)).await;
    gs.remove_player(&caster).await;
    assert!(!gs.abilities.read().await.marks_target(&caster, "target"));
    let mut rx = add_mark_player(&gs, "reconnected", 1).await;
    tokio::time::advance(Duration::from_millis(9999)).await;
    gs.use_targeted_ability(&pid("reconnected"), MARK, Some("target"), None, None)
        .await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.use_targeted_ability(&pid("reconnected"), MARK, Some("target"), None, None)
        .await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("reconnected"), "target"));
}

#[tokio::test]
async fn bow_mark_rejects_wrong_equipment_missing_dead_far_or_other_floor_targets_without_cooldown()
{
    let gs = make_test_game_state("bow_mark_validation");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    add_mark_target(&gs, "target", 5.0).await;
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    rejected(&mut rx, AbilityRejectReason::Equipment);
    gs.inventories
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .equipped
        .insert(EquipSlot::MainHand, bag_item(1, "bow", 1));
    for target in [None, Some("missing")] {
        gs.use_targeted_ability(&pid("caster"), MARK, target, None, None)
            .await;
        rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    for (x, floor, health, reason) in [
        (10.01, 0, 10, AbilityRejectReason::OutOfRange),
        (5.0, -1, 10, AbilityRejectReason::Unavailable),
        (5.0, 0, 0, AbilityRejectReason::Unavailable),
    ] {
        {
            let mut monsters = gs.monsters.write().await;
            let target = monsters.get_mut("target").unwrap();
            target.position.x = x;
            target.floor_level = floor;
            target.health = health;
        }
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
            .await;
        rejected(&mut rx, reason);
        assert!(!gs
            .abilities
            .read()
            .await
            .marks_target(&pid("caster"), "target"));
        assert!(matches!(
            gs.ability_cooldown_message(&pid("caster")).await,
            ServerMessage::AbilityCooldowns { cooldowns }
                if cooldowns.iter().any(|timer| timer.ability == MARK && timer.remaining_ms == 0)
        ));
    }
    add_mark_target(&gs, "target", 10.0).await;
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
        .await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
}

#[tokio::test]
async fn bow_mark_clears_when_target_dies_disappears_or_caster_changes_floor() {
    for change in ["dead", "despawn", "floor", "caster_dead"] {
        let gs = make_test_game_state(&format!("bow_mark_clear_{change}"));
        let mut rx = add_mark_player(&gs, "caster", 1).await;
        add_mark_target(&gs, "target", 5.0).await;
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None, None)
            .await;
        drain(&mut rx);
        match change {
            "dead" => {
                gs.monsters.write().await.get_mut("target").unwrap().state = MonsterState::Dead
            }
            "despawn" => {
                gs.monsters.write().await.remove("target");
            }
            "floor" => {
                gs.players
                    .write()
                    .await
                    .get_mut(&pid("caster"))
                    .unwrap()
                    .floor_level = -1
            }
            _ => {
                gs.clear_buffs(&pid("caster")).await;
            }
        }
        gs.tick_buffs().await;
        assert!(!gs
            .abilities
            .read()
            .await
            .marks_target(&pid("caster"), "target"));
        assert!(drain(&mut rx).iter().any(|m| matches!(
            m,
            ServerMessage::BowMarkUpdate {
                monster_id: None,
                ..
            }
        )));
    }
}

#[tokio::test]
async fn radiance_accepts_every_weapon_and_empty_hands_without_a_shield() {
    let gs = make_test_game_state("radiance_equipment");
    for (i, weapon) in [
        None,
        Some("dagger"),
        Some("iron_sword"),
        Some("morningstar"),
        Some("spear"),
        Some("great_sword"),
        Some("bow"),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("light{i}");
        let mut rx = add_ward_player(&gs, &name, 0.0, i as i64 + 1).await;
        {
            let mut inventories = gs.inventories.write().await;
            let inventory = inventories.get_mut(&pid(&name)).unwrap();
            inventory.equipped.clear();
            if let Some(weapon) = weapon {
                inventory
                    .equipped
                    .insert(EquipSlot::MainHand, bag_item(1, weapon, 1));
            }
        }
        let guard = gs.effective_guard(&pid(&name)).await;
        gs.use_ability(&pid(&name), RADIANCE).await;
        assert!(gs.get_all_players().await[&pid(&name)].radiance_on);
        assert_eq!(gs.effective_guard(&pid(&name)).await, guard);
        assert!(drain(&mut rx).iter().any(|message| matches!(message, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|buff| buff.ability == RADIANCE && buff.remaining_ms > 119_000))));
    }
}

#[tokio::test(start_paused = true)]
async fn radiance_toggle_is_atomic_and_has_an_eight_hundred_ms_cooldown_both_ways() {
    let gs = make_test_game_state("radiance_toggle");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    let caster = pid("caster");
    tokio::join!(
        gs.use_ability(&caster, RADIANCE),
        gs.use_ability(&caster, RADIANCE)
    );
    let updates = drain(&mut rx);
    assert_eq!(
        updates
            .iter()
            .filter(
                |m| matches!(m, ServerMessage::AbilityUsed { ability, .. } if *ability == RADIANCE)
            )
            .count(),
        1
    );
    assert!(gs.get_all_players().await[&caster].radiance_on);
    tokio::time::advance(Duration::from_millis(799)).await;
    gs.use_ability(&caster, RADIANCE).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.use_ability(&caster, RADIANCE).await;
    assert!(!gs.get_all_players().await[&caster].radiance_on);
    assert!(!drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    tokio::time::advance(Duration::from_millis(799)).await;
    gs.use_ability(&caster, RADIANCE).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.use_ability(&caster, RADIANCE).await;
    assert!(gs.get_all_players().await[&caster].radiance_on);
    assert!(drain(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == RADIANCE && b.remaining_ms == 120_000))));
}

#[tokio::test(start_paused = true)]
async fn radiance_expires_at_two_minutes_without_clearing_a_later_ward() {
    let gs = make_test_game_state("radiance_expiry");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    tokio::time::advance(Duration::from_secs(80)).await;
    gs.use_ability(&pid("caster"), WARD).await;
    tokio::time::advance(Duration::from_secs(39)).await;
    gs.tick_buffs().await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    drain(&mut rx);
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.tick_buffs().await;
    assert!(!gs.get_all_players().await[&pid("caster")].radiance_on);
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    assert!(drain(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.len() == 1 && buffs[0].ability == WARD)));
    gs.use_ability(&pid("caster"), RADIANCE).await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
}

#[tokio::test(start_paused = true)]
async fn ward_expiry_does_not_remove_radiance() {
    let gs = make_test_game_state("radiance_independent");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.use_ability(&pid("caster"), WARD).await;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    drain(&mut rx);
    tokio::time::advance(Duration::from_secs(60)).await;
    gs.tick_buffs().await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
    assert!(drain(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.len() == 1 && buffs[0].ability == RADIANCE)));
}

#[tokio::test(start_paused = true)]
async fn radiance_replicates_light_and_cast_only_to_nearby_same_floor_players() {
    let gs = make_test_game_state("radiance_visibility");
    add_ward_player(&gs, "caster", 0.0, 1).await;
    let mut nearby = add_ward_player(&gs, "nearby", 2.0, 2).await;
    let mut downstairs = add_ward_player(&gs, "downstairs", 2.0, 3).await;
    let mut far = add_ward_player(&gs, "far", 1000.0, 4).await;
    gs.players
        .write()
        .await
        .get_mut(&pid("downstairs"))
        .unwrap()
        .floor_level = -1;
    gs.reconcile_view(&pid("downstairs")).await;
    drain(&mut nearby);
    drain(&mut downstairs);
    drain(&mut far);
    gs.use_ability(&pid("caster"), RADIANCE).await;
    let events = drain(&mut nearby);
    assert!(events.iter().any(|m| matches!(
        m,
        ServerMessage::PlayerRadianceToggled { enabled: true, .. }
    )));
    assert!(events
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { ability, .. } if *ability == RADIANCE)));
    for rx in [&mut downstairs, &mut far] {
        assert!(!drain(rx).iter().any(|m| matches!(
            m,
            ServerMessage::PlayerRadianceToggled { .. } | ServerMessage::AbilityUsed { .. }
        )));
    }
    assert!(!gs.get_all_players().await[&pid("nearby")].radiance_on);
    add_ward_player(&gs, "late", 1.0, 5).await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    tokio::time::advance(Duration::from_millis(800)).await;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    assert!(drain(&mut nearby).iter().any(|m| matches!(
        m,
        ServerMessage::PlayerRadianceToggled { enabled: false, .. }
    )));
}

#[tokio::test(start_paused = true)]
async fn radiance_rejects_dead_loading_and_mounted_casters_and_clears_on_death_or_logout() {
    let gs = make_test_game_state("radiance_cleanup");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    for state in 0..3 {
        {
            let mut players = gs.players.write().await;
            let player = players.get_mut(&pid("caster")).unwrap();
            player.health = if state == 0 { 0 } else { 10 };
            player.ready_at = if state == 1 {
                GameState::now_ms() + 30_000
            } else {
                0
            };
            player.mount = (state == 2).then_some(onlinerpg_shared::mount::MountKind::Horse);
        }
        gs.use_ability(&pid("caster"), RADIANCE).await;
        rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .mount = None;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .health = 0;
    gs.on_player_died(&pid("caster"), "test").await;
    assert!(!gs.get_all_players().await[&pid("caster")].radiance_on);
    tokio::time::advance(Duration::from_millis(800)).await;
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .health = 10;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    gs.remove_player(&pid("caster")).await;
    let mut next = add_ward_player(&gs, "reconnected", 0.0, 1).await;
    assert!(!gs.get_all_players().await[&pid("reconnected")].radiance_on);
    gs.use_ability(&pid("reconnected"), RADIANCE).await;
    rejected(&mut next, AbilityRejectReason::Cooldown);
}

async fn add_ward_player(gs: &GameState, name: &str, x: f32, character: i64) -> DirectRx {
    gs.add_player(make_player(name, x, 0.0)).await;
    gs.mark_world_ready(&pid(name)).await;
    gs.register_hunger(&pid(name), SATIATION_START).await;
    gs.register_mana(&make_player(name, x, 0.0), 10, None).await;
    let mut attrs = attrs_with_cha(10);
    attrs.guard = 20;
    gs.player_characters
        .write()
        .await
        .insert(pid(name), (character, 0, attrs));
    let mut inv = PlayerInventory::default();
    inv.equipped
        .insert(EquipSlot::MainHand, bag_item(1, "iron_sword", 1));
    let mut shield = bag_item(2, "raven_shield", 1);
    shield.enchant = 9;
    inv.equipped.insert(EquipSlot::OffHand, shield);
    gs.inventories.write().await.insert(pid(name), inv);
    gs.register_direct_channel(&pid(name)).await
}

fn rejected(rx: &mut DirectRx, reason: AbilityRejectReason) {
    assert!(drain(rx).iter().any(
        |m| matches!(m, ServerMessage::AbilityRejected { reason: actual, .. } if *actual == reason)
    ));
}

async fn party(gs: &GameState, leader: &str, member: &str) {
    gs.invite_to_party(&pid(leader), member).await;
    gs.respond_to_party_invite(&pid(member), &pid(leader), true)
        .await;
}

#[tokio::test]
async fn ward_spends_two_mana_and_syncs_and_saves_the_new_state() {
    for before in [15, 3, 2] {
        let gs = make_test_game_state(&format!("ward_mana_{before}"));
        let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
        let id = pid("caster");
        gs.mana.write().await.get_mut(&id).unwrap().mana = before;
        gs.register_hunger(&id, 0).await;
        gs.remove_dirty(&id).await;
        gs.use_targeted_ability(&id, WARD, None, None, None).await;
        assert_eq!(gs.mana.read().await[&id].mana, before - 2);
        assert_eq!(gs.hunger_satiation(&id).await, Some(0));
        assert!(gs.dirty_players.read().await.contains(&id));
        assert_eq!(
            gs.get_player_save_data(&id).await.unwrap().mana,
            Some(before - 2)
        );
        let updates = drain(&mut rx);
        assert!(updates.iter().any(|message| matches!(message,
            ServerMessage::ManaUpdate { mana, max_mana: 15 } if *mana == before - 2)));
        assert!(!updates
            .iter()
            .any(|m| matches!(m, ServerMessage::HungerUpdate { .. })));
        assert!(updates.iter().any(|message| matches!(message,
            ServerMessage::AbilityUsed { ability, .. } if *ability == WARD)));
    }
}

#[tokio::test]
async fn ward_rejects_insufficient_mana_without_buff_cost_or_cooldown() {
    for before in [0, 1] {
        let gs = make_test_game_state(&format!("ward_empty_mana_{before}"));
        let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
        let id = pid("caster");
        gs.mana.write().await.get_mut(&id).unwrap().mana = before;
        gs.remove_dirty(&id).await;
        gs.use_ability(&id, WARD).await;
        rejected(&mut rx, AbilityRejectReason::NotEnoughMana);
        assert_eq!(gs.mana.read().await[&id].mana, before);
        assert!(!gs.dirty_players.read().await.contains(&id));
        assert_eq!(gs.effective_guard(&id).await, 31);
        gs.mana.write().await.get_mut(&id).unwrap().mana = 2;
        gs.use_ability(&id, WARD).await;
        assert_eq!(gs.mana.read().await[&id].mana, 0);
        assert_eq!(gs.effective_guard(&id).await, 33);
    }
}

#[tokio::test]
async fn ward_cost_is_unaffected_by_activity_drain_modifiers() {
    let gs = make_test_game_state("ward_fixed_mana_cost");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    let id = pid("caster");
    gs.inflict_debuff(&id, "food_poisoning", Some(true)).await;
    gs.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .push(bag_item(3, "silver_necklace", 1));
    gs.equip_item(&id, 3).await;
    drain(&mut rx);
    gs.use_ability(&id, WARD).await;
    assert_eq!(gs.mana.read().await[&id].mana, 13);
    assert_eq!(gs.hunger_satiation(&id).await, Some(SATIATION_START));
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::AbilityUsed { ability, .. } if *ability == WARD)));
}

#[tokio::test]
async fn ward_rejects_other_classes_without_buff_or_cooldown() {
    let gs = make_test_game_state("ward_class_requirement");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    let id = pid("caster");
    for class in [
        CharacterClass::Rogue,
        CharacterClass::Barbarian,
        CharacterClass::Ranger,
        CharacterClass::Priest,
    ] {
        gs.players.write().await.get_mut(&id).unwrap().class = class;
        gs.use_targeted_ability(&id, WARD, None, None, None).await;
        let responses = drain(&mut rx);
        assert!(responses.iter().any(|message| matches!(message,
            ServerMessage::AbilityRejected { ability, reason: AbilityRejectReason::Unavailable }
                if *ability == WARD)));
        assert!(!responses.iter().any(|message| matches!(
            message,
            ServerMessage::AbilityUsed { .. } | ServerMessage::BuffUpdate { .. }
        )));
        assert_eq!(gs.effective_guard(&id).await, 31);
        assert_eq!(gs.mana.read().await[&id].mana, 15);
        assert_eq!(gs.hunger_satiation(&id).await, Some(SATIATION_START));
        assert!(responses.iter().any(|message| matches!(message,
            ServerMessage::AbilityCooldowns { cooldowns }
                if cooldowns.iter().all(|timer| timer.remaining_ms == 0))));
    }
    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Knight;
    gs.use_targeted_ability(&id, WARD, None, None, None).await;
    assert_eq!(gs.effective_guard(&id).await, 33);
    assert!(drain(&mut rx).iter().any(|message| matches!(message,
        ServerMessage::AbilityUsed { ability, .. } if *ability == WARD)));
}

#[tokio::test]
async fn ward_checks_weapon_classification_and_off_hand_armor_type() {
    let gs = make_test_game_state("ward_equipment");
    for (i, (weapon, shield, allowed)) in [
        ("iron_sword", "raven_shield", true),
        ("goblin_sword", "wooden_shield", true),
        ("small_sword", "raven_shield", true),
        ("morningstar", "wooden_shield", true),
        ("dagger", "raven_shield", false),
        ("great_sword", "raven_shield", false),
        ("iron_sword", "torch", false),
        ("iron_sword", "missing", false),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("ward{i}");
        let mut rx = add_ward_player(&gs, &name, 0.0, i as i64 + 1).await;
        {
            let mut invs = gs.inventories.write().await;
            let inv = invs.get_mut(&pid(&name)).unwrap();
            inv.equipped
                .insert(EquipSlot::MainHand, bag_item(1, weapon, 1));
            inv.equipped
                .insert(EquipSlot::OffHand, bag_item(2, shield, 1));
        }
        let before = gs.effective_guard(&pid(&name)).await;
        gs.use_ability(&pid(&name), WARD).await;
        if allowed {
            assert_eq!(gs.effective_guard(&pid(&name)).await, before + 2);
            assert!(drain(&mut rx)
                .iter()
                .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
        } else {
            assert_eq!(gs.effective_guard(&pid(&name)).await, before);
            assert_eq!(
                gs.hunger_satiation(&pid(&name)).await,
                Some(SATIATION_START)
            );
            assert_eq!(gs.mana.read().await[&pid(&name)].mana, 15);
            rejected(&mut rx, AbilityRejectReason::Equipment);
        }
    }
}

#[tokio::test]
async fn ward_affects_caster_and_living_party_members_within_twenty_meters_on_same_floor() {
    let gs = make_test_game_state("ward_party_radius");
    let mut receivers = Vec::new();
    for (i, (name, x)) in [
        ("caster", 0.0),
        ("edge", 20.0),
        ("outside", 20.01),
        ("downstairs", 1.0),
        ("dead", 2.0),
        ("stranger", 1.0),
    ]
    .into_iter()
    .enumerate()
    {
        receivers.push(add_ward_player(&gs, name, x, i as i64 + 1).await);
        if i > 0 && i < 5 {
            party(&gs, "caster", name).await;
        }
    }
    {
        let mut players = gs.players.write().await;
        players.get_mut(&pid("edge")).unwrap().class = CharacterClass::Rogue;
        players.get_mut(&pid("downstairs")).unwrap().floor_level = -1;
        players.get_mut(&pid("dead")).unwrap().health = 0;
    }
    gs.reconcile_view(&pid("downstairs")).await;
    for rx in &mut receivers {
        drain(rx);
    }
    gs.use_ability(&pid("caster"), WARD).await;
    for name in ["caster", "edge"] {
        assert_eq!(gs.effective_guard(&pid(name)).await, 33);
    }
    assert_eq!(gs.mana.read().await[&pid("caster")].mana, 13);
    assert_eq!(gs.mana.read().await[&pid("edge")].mana, 15);
    assert!(!drain(&mut receivers[1])
        .iter()
        .any(|message| matches!(message, ServerMessage::ManaUpdate { .. })));
    for name in ["outside", "downstairs", "dead", "stranger"] {
        assert_eq!(gs.effective_guard(&pid(name)).await, 31);
    }
    assert!(!drain(&mut receivers[3])
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    let cast = drain(&mut receivers[0]);
    assert!(cast.iter().any(|m| matches!(m, ServerMessage::AbilityUsed { targets, .. } if targets.len() == 2 && targets.contains(&pid("edge")))));
}

#[tokio::test(start_paused = true)]
async fn ward_expires_at_sixty_seconds_and_restores_reported_guard() {
    let gs = make_test_game_state("ward_expiry");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.use_ability(&pid("caster"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    drain(&mut rx);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
    gs.tick_buffs().await;
    let updates = drain(&mut rx);
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.is_empty())));
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::EffectiveStatsUpdated { guard: 31, .. })));
}

#[tokio::test(start_paused = true)]
async fn ward_cooldown_is_atomic_and_recast_refreshes_without_stacking() {
    let gs = make_test_game_state("ward_refresh");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    let caster = pid("caster");
    tokio::time::advance(Duration::from_secs(16)).await;
    tokio::join!(
        gs.use_ability(&caster, WARD),
        gs.use_ability(&caster, WARD),
        gs.tick_regeneration()
    );
    let casts = drain(&mut rx);
    assert_eq!(gs.mana.read().await[&caster].mana, 13);
    assert_eq!(
        casts
            .iter()
            .filter(|m| matches!(m, ServerMessage::AbilityUsed { .. }))
            .count(),
        1
    );
    tokio::time::advance(Duration::from_secs(44)).await;
    gs.use_ability(&pid("caster"), WARD).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    assert_eq!(gs.mana.read().await[&caster].mana, 13);
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.use_ability(&pid("caster"), WARD).await;
    assert_eq!(gs.mana.read().await[&caster].mana, 11);
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
}

#[tokio::test(start_paused = true)]
async fn ward_from_another_caster_refreshes_and_uses_current_equipment_guard() {
    let gs = make_test_game_state("ward_party_refresh");
    add_ward_player(&gs, "first", 0.0, 1).await;
    add_ward_player(&gs, "second", 1.0, 2).await;
    party(&gs, "first", "second").await;
    gs.use_ability(&pid("first"), WARD).await;
    tokio::time::advance(Duration::from_secs(30)).await;
    gs.use_ability(&pid("second"), WARD).await;
    gs.unequip_item(&pid("first"), EquipSlot::OffHand).await;
    assert_eq!(gs.effective_guard(&pid("first")).await, 22);
    tokio::time::advance(Duration::from_secs(31)).await;
    assert_eq!(gs.effective_guard(&pid("second")).await, 33);
    tokio::time::advance(Duration::from_secs(29)).await;
    assert_eq!(gs.effective_guard(&pid("second")).await, 31);
}

#[tokio::test(start_paused = true)]
async fn ward_bonus_follows_caster_level_and_a_weaker_recast_keeps_it() {
    let gs = make_test_game_state("ward_caster_level");
    add_ward_player(&gs, "veteran", 0.0, 1).await;
    add_ward_player(&gs, "novice", 1.0, 2).await;
    party(&gs, "veteran", "novice").await;
    gs.players
        .write()
        .await
        .get_mut(&pid("veteran"))
        .unwrap()
        .level = 40;
    gs.use_ability(&pid("veteran"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("novice")).await, 35);
    tokio::time::advance(Duration::from_secs(30)).await;
    gs.use_ability(&pid("novice"), WARD).await;
    tokio::time::advance(Duration::from_secs(31)).await;
    assert_eq!(gs.effective_guard(&pid("novice")).await, 35);
    tokio::time::advance(Duration::from_secs(29)).await;
    assert_eq!(gs.effective_guard(&pid("novice")).await, 31);
    gs.use_ability(&pid("novice"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("veteran")).await, 33);
}

#[tokio::test]
async fn dead_or_loading_caster_cannot_apply_ward_or_consume_cooldown() {
    let gs = make_test_game_state("ward_dead_loading");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .health = 0;
    gs.use_ability(&pid("caster"), WARD).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    {
        let mut players = gs.players.write().await;
        let p = players.get_mut(&pid("caster")).unwrap();
        p.health = 10;
        p.ready_at = GameState::now_ms() + 30_000;
    }
    gs.use_ability(&pid("caster"), WARD).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.mark_world_ready(&pid("caster")).await;
    gs.use_ability(&pid("caster"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 33);
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .health = 0;
    gs.on_player_died(&pid("caster"), "test").await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
}

#[tokio::test(start_paused = true)]
async fn reconnect_removes_ward_but_preserves_character_cooldown() {
    let gs = make_test_game_state("ward_reconnect");
    add_ward_player(&gs, "old", 0.0, 1).await;
    gs.use_ability(&pid("old"), WARD).await;
    gs.remove_player(&pid("old")).await;
    let mut rx = add_ward_player(&gs, "new", 0.0, 1).await;
    assert_eq!(gs.effective_guard(&pid("new")).await, 31);
    gs.use_ability(&pid("new"), WARD).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_secs(45)).await;
    gs.use_ability(&pid("new"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("new")).await, 33);
}

async fn add_examiner(gs: &GameState) -> DirectRx {
    let rx = add_ward_player(gs, "examiner", 0.0, 1).await;
    gs.inventories
        .write()
        .await
        .get_mut(&pid("examiner"))
        .unwrap()
        .equipped
        .insert(EquipSlot::Neck, bag_item(4, "stethoscope", 1));
    rx
}

async fn examine(gs: &GameState, monster: Option<&str>, player: Option<PlayerId>) {
    gs.use_targeted_ability(
        &pid("examiner"),
        AbilityId::Auscultation,
        monster,
        player,
        None,
    )
    .await;
}

fn inspection(rx: &mut DirectRx) -> InspectionResult {
    drain(rx)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::InspectionResult { inspection } => Some(inspection),
            _ => None,
        })
        .expect("inspection reply")
}

fn inspection_rejected(rx: &mut DirectRx, reason: AbilityRejectReason) {
    let updates = drain(rx);
    assert!(!updates
        .iter()
        .any(|m| matches!(m, ServerMessage::InspectionResult { .. })));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityRejected { ability: AbilityId::Auscultation, reason: actual }
        if *actual == reason)));
}

#[tokio::test]
async fn auscultation_reports_live_player_stats_and_only_equipped_items_privately() {
    let gs = make_test_game_state("auscultation_player");
    let mut rx = add_examiner(&gs).await;
    let mut target_rx = add_ward_player(&gs, "target", 2.0, 2).await;
    let target_id = pid("target");
    {
        let mut players = gs.players.write().await;
        let target = players.get_mut(&target_id).unwrap();
        target.level = 17;
        target.health = 43;
        target.max_health = 120;
    }
    gs.inventories
        .write()
        .await
        .get_mut(&target_id)
        .unwrap()
        .bag
        .push(bag_item(99, "gold_ring", 1));
    gs.use_ability(&target_id, WARD).await;
    drain(&mut target_rx);
    drain(&mut rx);
    examine(&gs, None, Some(target_id)).await;
    let result = inspection(&mut rx);
    assert_eq!(
        result.target,
        InspectionTarget::Player {
            player_id: target_id
        }
    );
    assert_eq!(
        (result.level, result.health, result.max_health),
        (17, 43, 120)
    );
    assert_eq!(result.guard, gs.effective_guard(&target_id).await);
    assert_eq!(result.equipment.len(), 2);
    assert!(result
        .equipment
        .iter()
        .any(|item| item.slot == EquipSlot::OffHand
            && item.item_def_id == "raven_shield"
            && item.enchant == 9));
    assert!(drain(&mut target_rx).is_empty());
    assert_eq!(
        gs.inventories.read().await[&pid("examiner")].equipped[&EquipSlot::Neck].quantity,
        1
    );
    assert_eq!(gs.players.read().await[&target_id].health, 43);
    let bytes = onlinerpg_shared::serialize_server_msg(&ServerMessage::InspectionResult {
        inspection: result.clone(),
    })
    .unwrap();
    let decoded = onlinerpg_shared::deserialize_server_msg(&bytes).unwrap();
    assert!(
        matches!(decoded, ServerMessage::InspectionResult { inspection } if inspection == result)
    );
}

#[tokio::test]
async fn auscultation_can_examine_signe_without_interrupting_her_performance() {
    let gs = make_test_game_state("auscultation_signe_performance");
    let mut rx = add_examiner(&gs).await;
    let mut signe_rx = add_ward_player(&gs, "Signe", 1.0, 2).await;
    let signe = pid("Signe");
    {
        let mut players = gs.players.write().await;
        let player = players.get_mut(&signe).unwrap();
        player.class = CharacterClass::Bard;
        player.is_official_npc = true;
    }
    gs.inventories
        .write()
        .await
        .get_mut(&signe)
        .unwrap()
        .bag
        .push(bag_item(99, "worn_mandolin", 1));
    gs.start_live_instrument(&signe).await;
    assert!(gs.live_instrument_players.read().await.contains(&signe));
    drain(&mut rx);
    drain(&mut signe_rx);

    examine(&gs, None, Some(signe)).await;

    let result = inspection(&mut rx);
    assert_eq!(result.name, "Signe");
    assert_eq!(result.target, InspectionTarget::Player { player_id: signe });
    assert_eq!(
        gs.players.read().await[&signe].object_type.as_deref(),
        Some(onlinerpg_shared::messages::MUSIC_EMOTE)
    );
    assert!(gs.live_instrument_players.read().await.contains(&signe));
    assert!(drain(&mut signe_rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn auscultation_preserves_seated_and_resting_player_poses() {
    let gs = make_test_game_state("auscultation_target_pose");
    let mut rx = add_examiner(&gs).await;
    let _target_rx = add_ward_player(&gs, "target", 1.0, 2).await;
    let target = pid("target");
    for kind in ["chair", "bed"] {
        let player = gs.players.read().await[&target].clone();
        gs.sync_region_furniture(
            0,
            0,
            &[onlinerpg_shared::furniture::FurniturePlacement {
                id: 39,
                type_id: kind.into(),
                x: player.position.x,
                y: 5.0,
                z: player.position.z,
                rotation_deg: 0.0,
                floor_level: player.floor_level as u8,
            }],
        );
        gs.players
            .write()
            .await
            .get_mut(&target)
            .unwrap()
            .position
            .y = 5.0;
        gs.set_player_interaction(&target, Some(kind.to_owned()), Some(39))
            .await;
        drain(&mut rx);

        examine(&gs, None, Some(target)).await;

        assert_eq!(inspection(&mut rx).name, "target");
        let players = gs.players.read().await;
        assert_eq!(players[&target].object_type.as_deref(), Some(kind));
        assert_eq!(players[&target].object_id, Some(39));
        drop(players);
        tokio::time::advance(Duration::from_millis(800)).await;
    }
}

#[tokio::test]
async fn auscultation_reports_depth_scaled_monster_level_and_current_hp() {
    let gs = make_test_game_state("auscultation_monster");
    let mut rx = add_examiner(&gs).await;
    add_mark_target(&gs, "target", 1.0).await;
    {
        let mut monsters = gs.monsters.write().await;
        let m = monsters.get_mut("target").unwrap();
        m.monster_type = "goblin".into();
        m.level_override = Some(12);
        m.health = 71;
    }
    examine(&gs, Some("target"), None).await;
    let result = inspection(&mut rx);
    assert_eq!(
        result.target,
        InspectionTarget::Monster {
            monster_id: "target".into()
        }
    );
    assert_eq!(
        (result.level, result.health, result.max_health),
        (12, 71, 1000)
    );
    let def = gs.monster_defs.get("goblin").unwrap();
    assert_eq!(result.name, def.name);
    assert_eq!(result.guard, i32::from(def.guard));
    assert_eq!(
        result
            .equipment
            .first()
            .map(|item| item.item_def_id.as_str()),
        def.weapon.as_deref()
    );
}

#[tokio::test]
async fn auscultation_requires_neck_equipment_even_if_carried_or_removed_after_selection() {
    let gs = make_test_game_state("auscultation_equipment");
    let mut rx = add_examiner(&gs).await;
    let _target_rx = add_ward_player(&gs, "target", 1.0, 2).await;
    gs.unequip_item(&pid("examiner"), EquipSlot::Neck).await;
    drain(&mut rx);
    examine(&gs, None, Some(pid("target"))).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Equipment);
    gs.equip_item(&pid("examiner"), 4).await;
    drain(&mut rx);
    examine(&gs, None, Some(pid("target"))).await;
    inspection(&mut rx);
    gs.unequip_item(&pid("examiner"), EquipSlot::Neck).await;
    drain(&mut rx);
    examine(&gs, None, Some(pid("target"))).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Equipment);
}

#[tokio::test]
async fn auscultation_rejects_missing_ambiguous_dead_distant_or_other_floor_players() {
    let gs = make_test_game_state("auscultation_player_gates");
    let mut rx = add_examiner(&gs).await;
    let _target_rx = add_ward_player(&gs, "target", 1.0, 2).await;
    for (monster, player) in [
        (None, None),
        (None, Some(pid("missing"))),
        (None, Some(pid("examiner"))),
        (Some("monster"), Some(pid("target"))),
    ] {
        examine(&gs, monster, player).await;
        inspection_rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    for (x, floor, health, reason) in [
        (2.01, 0, 100, AbilityRejectReason::OutOfRange),
        (1.0, -1, 100, AbilityRejectReason::Unavailable),
        (1.0, 0, 0, AbilityRejectReason::Unavailable),
        (f32::NAN, 0, 100, AbilityRejectReason::Unavailable),
    ] {
        {
            let mut players = gs.players.write().await;
            let p = players.get_mut(&pid("target")).unwrap();
            p.position.x = x;
            p.floor_level = floor;
            p.health = health;
        }
        examine(&gs, None, Some(pid("target"))).await;
        inspection_rejected(&mut rx, reason);
    }
}

#[tokio::test]
async fn auscultation_rejects_invalid_monsters_and_obstructed_targets_without_spending_cooldown() {
    let gs = make_test_game_state("auscultation_monster_gates");
    let mut rx = add_examiner(&gs).await;
    examine(&gs, Some("missing"), None).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Unavailable);
    add_mark_target(&gs, "target", 1.0).await;
    for (x, floor, health, reason) in [
        (2.01, 0, 100, AbilityRejectReason::OutOfRange),
        (1.0, 1, 100, AbilityRejectReason::Unavailable),
        (1.0, 0, 0, AbilityRejectReason::Unavailable),
    ] {
        {
            let mut monsters = gs.monsters.write().await;
            let m = monsters.get_mut("target").unwrap();
            m.monster_type = "goblin".into();
            m.position.x = x;
            m.position.z = 0.5;
            m.floor_level = floor;
            m.health = health;
        }
        gs.players
            .write()
            .await
            .get_mut(&pid("examiner"))
            .unwrap()
            .position
            .z = 0.5;
        examine(&gs, Some("target"), None).await;
        inspection_rejected(&mut rx, reason);
    }
    gs.monsters.write().await.get_mut("target").unwrap().health = 100;
    gs.sync_region_furniture(0, 0, &[table_placement(0.5, 0.5)]);
    examine(&gs, Some("target"), None).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.sync_region_furniture(0, 0, &[]);
    examine(&gs, Some("target"), None).await;
    inspection(&mut rx);
}

#[tokio::test(start_paused = true)]
async fn auscultation_is_class_independent_but_rejects_dead_loading_or_mounted_casters() {
    let gs = make_test_game_state("auscultation_caster_gates");
    let mut rx = add_examiner(&gs).await;
    let _target_rx = add_ward_player(&gs, "target", 1.0, 2).await;
    for (health, mount, ready_at) in [
        (0, None, 0),
        (100, Some(onlinerpg_shared::mount::MountKind::Horse), 0),
        (100, None, u64::MAX),
    ] {
        {
            let mut players = gs.players.write().await;
            let p = players.get_mut(&pid("examiner")).unwrap();
            p.health = health;
            p.mount = mount;
            p.ready_at = ready_at;
        }
        examine(&gs, None, Some(pid("target"))).await;
        inspection_rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    for class in [
        CharacterClass::Knight,
        CharacterClass::Rogue,
        CharacterClass::Merchant,
    ] {
        {
            let mut players = gs.players.write().await;
            let p = players.get_mut(&pid("examiner")).unwrap();
            p.health = 100;
            p.mount = None;
            p.ready_at = 0;
            p.class = class;
        }
        examine(&gs, None, Some(pid("target"))).await;
        inspection(&mut rx);
        tokio::time::advance(Duration::from_millis(800)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn auscultation_serializes_simultaneous_casts_and_releases_cooldown_after_800ms() {
    let gs = make_test_game_state("auscultation_cooldown");
    let mut rx = add_examiner(&gs).await;
    let _target_rx = add_ward_player(&gs, "target", 1.0, 2).await;
    tokio::join!(
        examine(&gs, None, Some(pid("target"))),
        examine(&gs, None, Some(pid("target")))
    );
    let updates = drain(&mut rx);
    assert_eq!(
        updates
            .iter()
            .filter(|m| matches!(m, ServerMessage::InspectionResult { .. }))
            .count(),
        1
    );
    assert!(updates.iter().any(|m| matches!(
        m,
        ServerMessage::AbilityRejected {
            reason: AbilityRejectReason::Cooldown,
            ..
        }
    )));
    tokio::time::advance(Duration::from_millis(799)).await;
    examine(&gs, None, Some(pid("target"))).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    examine(&gs, None, Some(pid("target"))).await;
    inspection(&mut rx);
}

const RAGE: AbilityId = AbilityId::Rage;

async fn add_rage_player(gs: &GameState, name: &str, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, 0.0, character).await;
    gs.players.write().await.get_mut(&pid(name)).unwrap().class = CharacterClass::Barbarian;
    rx
}

#[tokio::test(start_paused = true)]
async fn rage_spends_one_mana_lowers_guard_and_expires_after_ten_seconds() {
    let gs = make_test_game_state("rage_cast");
    let mut rx = add_rage_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.remove_dirty(&id).await;
    gs.use_ability(&id, RAGE).await;
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    assert!(gs.dirty_players.read().await.contains(&id));
    assert_eq!(gs.effective_guard(&id).await, 29);
    let updates = drain(&mut rx);
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityUsed { ability, targets, .. } if *ability == RAGE && targets == &vec![id])));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == RAGE))));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::PlayerRageToggled { player_id, enabled: true } if *player_id == id)));
    assert!(gs.players.read().await[&id].raging);
    tokio::time::advance(Duration::from_millis(9_999)).await;
    assert_eq!(gs.effective_guard(&id).await, 29);
    tokio::time::advance(Duration::from_millis(1)).await;
    assert_eq!(gs.effective_guard(&id).await, 31);
    gs.tick_buffs().await;
    let updates = drain(&mut rx);
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.is_empty())));
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerRageToggled { enabled: false, .. })));
    assert!(!gs.players.read().await[&id].raging);
}

#[tokio::test]
async fn rage_is_visible_to_nearby_players_and_clears_on_weapon_loss() {
    let gs = make_test_game_state("rage_visible");
    add_rage_player(&gs, "caster", 1).await;
    let mut watcher = add_ward_player(&gs, "watcher", 3.0, 2).await;
    let id = pid("caster");
    gs.use_ability(&id, RAGE).await;
    assert!(drain(&mut watcher).iter().any(|m| matches!(m,
        ServerMessage::PlayerRageToggled { player_id, enabled: true } if *player_id == id)));
    gs.unequip_item(&id, EquipSlot::MainHand).await;
    assert!(drain(&mut watcher).iter().any(|m| matches!(m,
        ServerMessage::PlayerRageToggled { player_id, enabled: false } if *player_id == id)));
}

#[tokio::test]
async fn rage_rejects_other_classes_missing_melee_weapon_and_mana_without_cost() {
    let gs = make_test_game_state("rage_requirements");
    let mut rx = add_rage_player(&gs, "caster", 1).await;
    let id = pid("caster");
    let expect_rejected = |rx: &mut DirectRx, reason| {
        let responses = drain(rx);
        assert!(responses.iter().any(|m| matches!(m,
            ServerMessage::AbilityRejected { ability, reason: actual } if *ability == RAGE && *actual == reason)));
        assert!(!responses
            .iter()
            .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    };

    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Knight;
    gs.use_ability(&id, RAGE).await;
    expect_rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Barbarian;

    for weapon in [Some("bow"), Some("torch"), None] {
        let mut inventories = gs.inventories.write().await;
        let inv = inventories.get_mut(&id).unwrap();
        inv.equipped.remove(&EquipSlot::MainHand);
        if let Some(weapon) = weapon {
            inv.equipped
                .insert(EquipSlot::MainHand, bag_item(5, weapon, 1));
        }
        drop(inventories);
        gs.use_ability(&id, RAGE).await;
        expect_rejected(&mut rx, AbilityRejectReason::Equipment);
    }

    gs.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .equipped
        .insert(EquipSlot::MainHand, bag_item(5, "great_sword", 1));
    gs.mana.write().await.get_mut(&id).unwrap().mana = 0;
    gs.use_ability(&id, RAGE).await;
    expect_rejected(&mut rx, AbilityRejectReason::NotEnoughMana);
    assert!(!gs.abilities.read().await.raging(&id));

    gs.mana.write().await.get_mut(&id).unwrap().mana = 1;
    gs.use_ability(&id, RAGE).await;
    assert_eq!(gs.mana.read().await[&id].mana, 0);
    assert!(gs.abilities.read().await.raging(&id));
}

#[tokio::test(start_paused = true)]
async fn rage_has_a_twenty_second_character_cooldown() {
    let gs = make_test_game_state("rage_cooldown");
    let mut rx = add_rage_player(&gs, "caster", 1).await;
    let id = pid("caster");
    tokio::join!(gs.use_ability(&id, RAGE), gs.use_ability(&id, RAGE));
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    tokio::time::advance(Duration::from_secs(19)).await;
    drain(&mut rx);
    gs.use_ability(&id, RAGE).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    gs.remove_player(&id).await;
    let mut rx = add_rage_player(&gs, "caster", 1).await;
    gs.use_ability(&id, RAGE).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.use_ability(&id, RAGE).await;
    assert!(gs.abilities.read().await.raging(&id));
}

#[tokio::test]
async fn each_successful_cast_logs_one_ability_use_line() {
    use tracing::instrument::WithSubscriber;
    let gs = make_test_game_state("ability_use_log");
    add_ward_player(&gs, "knight", 0.0, 1).await;
    add_ward_player(&gs, "squire", 1.0, 2).await;
    party(&gs, "knight", "squire").await;
    let (subscriber, buffer) = crate::test_util::capture_logs();
    async {
        gs.use_ability(&pid("knight"), WARD).await;
        gs.use_ability(&pid("knight"), WARD).await;
    }
    .with_subscriber(subscriber)
    .await;
    let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    let casts: Vec<_> = logs
        .lines()
        .filter(|l| l.contains("ability cast"))
        .collect();
    assert_eq!(casts.len(), 1, "{logs}");
    assert!(
        casts[0].contains(
            "ability_use: ability cast name=knight ability=GuardianWard targets=2 floor=0"
        ),
        "{}",
        casts[0]
    );
}

#[tokio::test]
async fn rage_guard_penalty_comes_off_the_warded_guard() {
    let gs = make_test_game_state("rage_with_ward");
    add_rage_player(&gs, "barbarian", 1).await;
    add_ward_player(&gs, "knight", 1.0, 2).await;
    party(&gs, "knight", "barbarian").await;
    gs.use_ability(&pid("knight"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("barbarian")).await, 33);
    gs.use_ability(&pid("barbarian"), RAGE).await;
    assert_eq!(gs.effective_guard(&pid("barbarian")).await, 31);
}

#[tokio::test]
async fn rage_survives_melee_swaps_but_ends_without_a_melee_weapon() {
    let gs = make_test_game_state("rage_weapon_loss");
    let mut rx = add_rage_player(&gs, "caster", 1).await;
    let id = pid("caster");
    {
        let mut inventories = gs.inventories.write().await;
        let bag = &mut inventories.get_mut(&id).unwrap().bag;
        bag.push(bag_item(5, "spear", 1));
        bag.push(bag_item(6, "bow", 1));
    }
    gs.use_ability(&id, RAGE).await;
    gs.equip_item(&id, 5).await;
    assert!(gs.abilities.read().await.raging(&id));
    drain(&mut rx);
    gs.equip_item(&id, 6).await;
    assert!(!gs.abilities.read().await.raging(&id));
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.is_empty())));

    let gs = make_test_game_state("rage_weapon_unequip");
    add_rage_player(&gs, "caster", 1).await;
    gs.use_ability(&id, RAGE).await;
    gs.unequip_item(&id, EquipSlot::OffHand).await;
    assert!(gs.abilities.read().await.raging(&id));
    gs.unequip_item(&id, EquipSlot::MainHand).await;
    assert!(!gs.abilities.read().await.raging(&id));
}

#[tokio::test]
async fn rage_ends_on_death() {
    let gs = make_test_game_state("rage_death");
    add_rage_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.use_ability(&id, RAGE).await;
    gs.players.write().await.get_mut(&id).unwrap().health = 0;
    gs.on_player_died(&id, "test").await;
    assert!(!gs.abilities.read().await.raging(&id));
    assert_eq!(gs.effective_guard(&id).await, 31);
}

#[tokio::test]
async fn rage_adds_level_scaled_damage_to_melee_hits() {
    // Level 50, DEX 10, 1d4 dagger: a plain hit deals at most 8 even when
    // critical; Rage adds +8, so a raging hit deals at least 9.
    for raging in [false, true] {
        let mut hits = 0;
        for attempt in 0..12 {
            let gs = make_test_game_state(&format!("rage_damage_{raging}_{attempt}"));
            add_rage_player(&gs, "caster", 1).await;
            let id = pid("caster");
            gs.players.write().await.get_mut(&id).unwrap().level = 50;
            gs.inventories
                .write()
                .await
                .get_mut(&id)
                .unwrap()
                .equipped
                .insert(EquipSlot::MainHand, bag_item(1, "dagger", 1));
            add_mark_target(&gs, "target", 1.0).await;
            if raging {
                gs.use_ability(&id, RAGE).await;
                assert!(gs.abilities.read().await.raging(&id));
            }
            gs.player_attack(&id, "target".to_owned(), None).await;
            let damage = 1000 - gs.monsters.read().await["target"].health;
            if damage == 0 {
                continue;
            }
            hits += 1;
            if raging {
                assert!(damage >= 9, "raging hit dealt {damage}");
            } else {
                assert!(damage <= 8, "plain hit dealt {damage}");
            }
        }
        assert!(hits > 0);
    }
}

const ROAR: AbilityId = AbilityId::PrimalRoar;

async fn add_roar_player(gs: &GameState, name: &str, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, 0.0, character).await;
    let mut players = gs.players.write().await;
    let player = players.get_mut(&pid(name)).unwrap();
    player.class = CharacterClass::Caveman;
    player.max_health = 200;
    player.health = 200;
    rx
}

async fn add_roar_target(gs: &GameState, id: &str, x: f32, floor_level: i8, staggers: bool) {
    let target = make_monster(id, Position { x, y: 0.0, z: 0.0 }, floor_level);
    gs.monsters.write().await.insert(id.to_owned(), target);
    gs.insert_brain(id, staggers).await;
}

fn temp_health_updates(rx: &mut DirectRx) -> Vec<u32> {
    drain(rx)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::TempHealthUpdate { amount } => Some(amount),
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn primal_roar_spends_one_mana_and_shields_for_ten_seconds() {
    let gs = make_test_game_state("roar_cast");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.remove_dirty(&id).await;
    gs.use_ability(&id, ROAR).await;
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    assert!(gs.dirty_players.read().await.contains(&id));
    assert_eq!(gs.players.read().await[&id].temp_health, 30);
    let updates = drain(&mut rx);
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityUsed { ability, targets, .. } if *ability == ROAR && targets == &vec![id])));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == ROAR))));
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::TempHealthUpdate { amount: 30 })));
    tokio::time::advance(Duration::from_millis(9_999)).await;
    gs.tick_buffs().await;
    assert_eq!(gs.players.read().await[&id].temp_health, 30);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.tick_buffs().await;
    assert_eq!(temp_health_updates(&mut rx), vec![0]);
    let player = &gs.players.read().await[&id];
    assert_eq!((player.temp_health, player.health), (0, 200));
}

#[tokio::test]
async fn primal_roar_shield_is_at_least_four_and_spent_before_health() {
    let gs = make_test_game_state("roar_minimum");
    add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.players.write().await.get_mut(&id).unwrap().max_health = 14;
    gs.use_ability(&id, ROAR).await;
    assert_eq!(gs.players.read().await[&id].temp_health, 4);

    let mut players = gs.players.write().await;
    let player = players.get_mut(&id).unwrap();
    player.temp_health = 9;
    player.absorb_damage(5);
    assert_eq!((player.temp_health, player.health), (4, 200));
    player.absorb_damage(10);
    assert_eq!((player.temp_health, player.health), (0, 194));
}

#[tokio::test]
async fn primal_roar_shield_takes_monster_blows_before_health() {
    let gs = make_test_game_state("roar_absorb");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    let mut attacker = make_monster(
        "attacker",
        Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        0,
    );
    attacker.level_override = Some(20);
    gs.monsters
        .write()
        .await
        .insert("attacker".into(), attacker);
    gs.use_ability(&id, ROAR).await;
    drain(&mut rx);
    let mut taken = 0;
    for _ in 0..50 {
        gs.monster_attack("attacker", &id).await;
        for m in drain(&mut rx) {
            if let ServerMessage::MonsterAttackedPlayer {
                hit: true, damage, ..
            } = m
            {
                taken += damage;
            }
        }
        let player = &gs.players.read().await[&id];
        assert_eq!(player.temp_health, 30u32.saturating_sub(taken));
        assert_eq!(player.health, 200 - taken.saturating_sub(30));
        if taken > 0 {
            return;
        }
    }
    panic!("no blow landed");
}

#[tokio::test]
async fn primal_roar_ends_when_the_shield_is_spent_or_the_caster_dies() {
    let gs = make_test_game_state("roar_spent");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.use_ability(&id, ROAR).await;
    drain(&mut rx);
    gs.players.write().await.get_mut(&id).unwrap().temp_health = 0;
    gs.send_temp_health(&id).await;
    let updates = drain(&mut rx);
    assert!(updates
        .iter()
        .any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.is_empty())));

    let gs = make_test_game_state("roar_death");
    add_roar_player(&gs, "caster", 1).await;
    gs.use_ability(&id, ROAR).await;
    gs.players.write().await.get_mut(&id).unwrap().health = 0;
    gs.on_player_died(&id, "test").await;
    assert_eq!(gs.players.read().await[&id].temp_health, 0);
}

async fn wait_for_stomp() {
    tokio::time::sleep(Duration::from_millis(
        onlinerpg_shared::ability::PRIMAL_ROAR_STOMP_MS + 10,
    ))
    .await;
}

#[tokio::test(start_paused = true)]
async fn primal_roar_staggers_and_provokes_monsters_within_reach() {
    let gs = make_test_game_state("roar_stagger");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    add_roar_target(&gs, "near", 3.5, 0, true).await;
    add_roar_target(&gs, "far", 4.5, 0, true).await;
    add_roar_target(&gs, "below", 1.0, -1, true).await;
    add_roar_target(&gs, "immune", -2.0, 0, false).await;
    add_roar_target(&gs, "dead", 2.0, 0, true).await;
    gs.monsters.write().await.mark_dead("dead");
    gs.use_ability(&id, ROAR).await;
    assert_eq!(gs.brain_target("near").await, None);
    wait_for_stomp().await;
    assert_eq!(
        gs.brain_state("near").await,
        Some(onlinerpg_shared::monster_ai::AiState::Hit)
    );
    assert_eq!(gs.brain_target("near").await, Some(id));
    assert_eq!(gs.brain_target("immune").await, Some(id));
    assert_ne!(
        gs.brain_state("immune").await,
        Some(onlinerpg_shared::monster_ai::AiState::Hit)
    );
    for untouched in ["far", "below", "dead"] {
        assert_eq!(gs.brain_target(untouched).await, None, "{untouched}");
    }
    assert!(GameState::in_combat(&gs.players.read().await[&id]));
    assert!(drain(&mut rx).iter().any(|m| matches!(m,
        ServerMessage::MonsterStunned { monster_ids, duration_ms }
            if monster_ids == &vec!["near".to_owned()]
                && *duration_ms == onlinerpg_shared::ability::PRIMAL_ROAR_STAGGER_MS)));
}

#[tokio::test(start_paused = true)]
async fn primal_roar_with_nothing_near_leaves_the_caster_out_of_combat() {
    let gs = make_test_game_state("roar_alone");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.use_ability(&id, ROAR).await;
    wait_for_stomp().await;
    assert!(!GameState::in_combat(&gs.players.read().await[&id]));
    assert!(!drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::MonsterStunned { .. })));
}

#[tokio::test]
async fn primal_roar_rejects_other_classes_and_mana_without_cost() {
    let gs = make_test_game_state("roar_requirements");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Barbarian;
    gs.use_ability(&id, ROAR).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Caveman;
    gs.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .equipped
        .clear();
    gs.mana.write().await.get_mut(&id).unwrap().mana = 0;
    gs.use_ability(&id, ROAR).await;
    rejected(&mut rx, AbilityRejectReason::NotEnoughMana);
    assert_eq!(gs.players.read().await[&id].temp_health, 0);
    gs.mana.write().await.get_mut(&id).unwrap().mana = 1;
    gs.use_ability(&id, ROAR).await;
    assert_eq!(gs.players.read().await[&id].temp_health, 30);
}

#[tokio::test(start_paused = true)]
async fn primal_roar_has_a_thirty_second_character_cooldown() {
    let gs = make_test_game_state("roar_cooldown");
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    tokio::join!(gs.use_ability(&id, ROAR), gs.use_ability(&id, ROAR));
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    tokio::time::advance(Duration::from_secs(29)).await;
    drain(&mut rx);
    gs.use_ability(&id, ROAR).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    gs.remove_player(&id).await;
    let mut rx = add_roar_player(&gs, "caster", 1).await;
    gs.use_ability(&id, ROAR).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.use_ability(&id, ROAR).await;
    assert_eq!(gs.players.read().await[&id].temp_health, 30);
}

#[tokio::test(start_paused = true)]
async fn primal_roar_does_not_reach_through_walls() {
    let gs = make_test_game_state("roar_wall");
    add_roar_player(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.players.write().await.get_mut(&id).unwrap().position.z = 0.5;
    add_roar_target(&gs, "behind", 3.5, 0, true).await;
    gs.monsters
        .write()
        .await
        .get_mut("behind")
        .unwrap()
        .position
        .z = 0.5;
    gs.sync_region_furniture(0, 0, &[table_placement(2.0, 0.5)]);
    gs.use_ability(&id, ROAR).await;
    wait_for_stomp().await;
    assert_eq!(gs.brain_target("behind").await, None);
}

#[tokio::test]
async fn primal_roar_holds_the_casters_next_swing_through_the_roar() {
    let gs = make_test_game_state("roar_swing_hold");
    add_roar_player(&gs, "caster", 1).await;
    add_ward_player(&gs, "other", 2.0, 2).await;
    let id = pid("caster");
    assert!(gs.player_attack_ready(&id).await);
    gs.use_ability(&id, ROAR).await;
    assert!(!gs.player_attack_ready(&id).await);
    assert!(gs.player_attack_ready(&pid("other")).await);
    let remaining = gs.player_attack_remaining_ms(&id).await;
    let cast = onlinerpg_shared::ability::PRIMAL_ROAR_CAST_MS;
    assert!(remaining > cast - 100 && remaining <= cast, "{remaining}");
}

const CHARGE: AbilityId = AbilityId::ValhallaCharge;

async fn add_charge_player(gs: &GameState, name: &str, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, 0.0, character).await;
    gs.players.write().await.get_mut(&pid(name)).unwrap().class = CharacterClass::Valkyrie;
    rx
}

async fn add_charge_target(gs: &GameState, id: &str, x: f32) {
    add_mark_target(gs, id, x).await;
    gs.insert_brain(id, true).await;
}

async fn charge(gs: &GameState, name: &str, target: &str) {
    gs.use_targeted_ability(&pid(name), CHARGE, Some(target), None, None)
        .await;
}

async fn wait_for_arrival() {
    tokio::time::sleep(Duration::from_millis(
        onlinerpg_shared::ability::VALHALLA_CHARGE_DASH_MS + 10,
    ))
    .await;
}

fn charge_cooldown_ms(updates: &[ServerMessage]) -> Option<u64> {
    updates.iter().rev().find_map(|m| match m {
        ServerMessage::AbilityCooldowns { cooldowns } => cooldowns
            .iter()
            .find(|c| c.ability == CHARGE)
            .map(|c| c.remaining_ms),
        _ => None,
    })
}

#[tokio::test(start_paused = true)]
async fn valhalla_charge_dashes_into_reach_and_strikes_once() {
    let gs = make_test_game_state("charge_cast");
    let mut rx = add_charge_player(&gs, "caster", 1).await;
    let mut watcher = add_ward_player(&gs, "watcher", 3.0, 2).await;
    add_charge_target(&gs, "target", 7.0).await;
    let id = pid("caster");
    gs.remove_dirty(&id).await;
    charge(&gs, "caster", "target").await;
    assert_eq!(gs.mana.read().await[&id].mana, 14);
    assert!(gs.dirty_players.read().await.contains(&id));
    let position = gs.players.read().await[&id].position;
    assert!((position.x - 5.3).abs() < 1e-3, "{position:?}");
    assert!(position.z.abs() < 1e-3);
    let updates = drain(&mut rx);
    assert_eq!(charge_cooldown_ms(&updates), Some(10_000));
    for updates in [updates, drain(&mut watcher)] {
        assert!(updates.iter().any(|m| matches!(m,
            ServerMessage::PlayerCharged { player_id, monster_id, from, position, duration_ms, .. }
                if *player_id == id && monster_id == "target" && from.x == 0.0
                    && (position.x - 5.3).abs() < 1e-3 && *duration_ms == 400)));
        assert!(!updates
            .iter()
            .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
    }
    assert!(!gs.player_attack_ready(&id).await);
    wait_for_arrival().await;
    let updates = drain(&mut watcher);
    let attacks: Vec<bool> = updates
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerAttacked {
                player_id,
                monster_id,
                hit,
                charge: true,
                ..
            } if *player_id == id && monster_id == "target" => Some(*hit),
            _ => None,
        })
        .collect();
    assert_eq!(attacks.len(), 1);
    let slowed = updates.iter().any(|m| matches!(m,
        ServerMessage::MonsterSlowed { monster_id, duration_ms: 3_000, .. } if monster_id == "target"));
    assert_eq!(slowed, attacks[0]);
    assert!(!gs.player_attack_ready(&id).await);
}

#[tokio::test(start_paused = true)]
async fn valhalla_charge_slows_bosses_and_stagger_immune_monsters_for_half_as_long() {
    let gs = make_test_game_state("charge_boss_slow");
    let mut rx = add_charge_player(&gs, "caster", 1).await;
    for (id, monster_type, duration) in [
        ("plain", "test_monster", 3_000),
        ("boss", "goblin_boss", 1_500),
        ("immune", "terraclash", 1_500),
    ] {
        add_charge_target(&gs, id, 5.0).await;
        gs.monsters.write().await.get_mut(id).unwrap().monster_type = monster_type.into();
        drain(&mut rx);
        gs.slow_monster(id).await;
        assert!(drain(&mut rx).iter().any(|m| matches!(m,
            ServerMessage::MonsterSlowed { monster_id, duration_ms, move_mult }
                if monster_id == id && *duration_ms == duration && *move_mult == 0.6)));
    }
    gs.monsters.write().await.mark_dead("plain");
    gs.slow_monster("plain").await;
    assert!(!drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::MonsterSlowed { .. })));
}

#[tokio::test]
async fn valhalla_charge_rejects_without_spending_mana_or_cooldown() {
    let gs = make_test_game_state("charge_requirements");
    let mut rx = add_charge_player(&gs, "caster", 1).await;
    let id = pid("caster");
    add_charge_target(&gs, "target", 7.0).await;
    add_charge_target(&gs, "close", 3.5).await;
    add_charge_target(&gs, "far", 10.5).await;
    let set_weapon = |item: &'static str| {
        let gs = gs.clone();
        async move {
            gs.inventories
                .write()
                .await
                .get_mut(&pid("caster"))
                .unwrap()
                .equipped
                .insert(EquipSlot::MainHand, bag_item(1, item, 1));
        }
    };

    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Knight;
    charge(&gs, "caster", "target").await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.players.write().await.get_mut(&id).unwrap().class = CharacterClass::Valkyrie;

    for weapon in ["great_sword", "mace", "bow"] {
        set_weapon(weapon).await;
        charge(&gs, "caster", "target").await;
        rejected(&mut rx, AbilityRejectReason::Equipment);
    }
    set_weapon("spear").await;

    charge(&gs, "caster", "close").await;
    rejected(&mut rx, AbilityRejectReason::TooClose);
    charge(&gs, "caster", "far").await;
    rejected(&mut rx, AbilityRejectReason::OutOfRange);
    gs.use_targeted_ability(&id, CHARGE, None, None, None).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);

    gs.players.write().await.get_mut(&id).unwrap().position.z = 0.5;
    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .position
        .z = 0.5;
    gs.sync_region_furniture(0, 0, &[table_placement(3.0, 0.5)]);
    charge(&gs, "caster", "target").await;
    rejected(&mut rx, AbilityRejectReason::Blocked);
    gs.sync_region_furniture(0, 0, &[]);

    gs.last_player_attacks
        .write()
        .await
        .insert(id, GameState::now_ms());
    charge(&gs, "caster", "target").await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    gs.last_player_attacks.write().await.remove(&id);

    assert_eq!(gs.mana.read().await[&id].mana, 15);
    gs.mana.write().await.get_mut(&id).unwrap().mana = 0;
    charge(&gs, "caster", "target").await;
    rejected(&mut rx, AbilityRejectReason::NotEnoughMana);

    assert_eq!(gs.players.read().await[&id].position.x, 0.0);
    assert_eq!(
        charge_cooldown_ms(&[gs.ability_cooldown_message(&id).await]),
        Some(0)
    );
    gs.mana.write().await.get_mut(&id).unwrap().mana = 1;
    charge(&gs, "caster", "target").await;
    assert!(gs.players.read().await[&id].position.x > 5.0);
}

#[tokio::test(start_paused = true)]
async fn valhalla_charge_cut_short_keeps_its_cooldown_and_skips_the_swing() {
    let gs = make_test_game_state("charge_cut_short");
    let mut rx = add_charge_player(&gs, "caster", 1).await;
    let id = pid("caster");
    add_charge_target(&gs, "target", 7.0).await;
    charge(&gs, "caster", "target").await;
    gs.monsters.write().await.mark_dead("target");
    wait_for_arrival().await;
    let updates = drain(&mut rx);
    assert!(!updates
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
    assert!(gs.player_attack_ready(&id).await);
    assert!(charge_cooldown_ms(&[gs.ability_cooldown_message(&id).await]).unwrap() > 9_000);

    add_charge_target(&gs, "second", 12.0).await;
    tokio::time::advance(Duration::from_secs(10)).await;
    charge(&gs, "caster", "second").await;
    let landed = gs.players.read().await[&id].position;
    assert!((landed.x - 10.3).abs() < 1e-3, "{landed:?}");
    gs.request_move_goal(id, 1, 5.0, 0.0, false).await;
    wait_for_arrival().await;
    assert!(!drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerAttacked { .. })));
    assert!(gs.player_attack_ready(&id).await);
}

const PRAYER: AbilityId = AbilityId::HealingPrayer;

async fn add_priest(gs: &GameState, name: &str, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, 0.0, character).await;
    gs.players.write().await.get_mut(&pid(name)).unwrap().class = CharacterClass::Priest;
    rx
}

async fn add_wounded(gs: &GameState, name: &str, x: f32, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, x, character).await;
    let mut players = gs.players.write().await;
    let player = players.get_mut(&pid(name)).unwrap();
    player.health = 10;
    player.max_health = 100;
    gs.interest_lock().refresh_player(player);
    rx
}

async fn pray(gs: &GameState, monster: Option<&str>, player: Option<&str>) {
    gs.use_targeted_ability(&pid("caster"), PRAYER, monster, player.map(pid), None)
        .await;
}

fn prayed(updates: &[ServerMessage]) -> Option<u32> {
    updates.iter().find_map(|m| match m {
        ServerMessage::HealingPrayed { amount, .. } => Some(*amount),
        _ => None,
    })
}

fn health_seen(updates: &[ServerMessage], name: &str) -> Option<(u32, u32)> {
    updates.iter().rev().find_map(|m| match m {
        ServerMessage::PlayerHealthUpdate {
            player_id,
            health,
            max_health,
        } if *player_id == pid(name) => Some((*health, *max_health)),
        _ => None,
    })
}

#[tokio::test(start_paused = true)]
async fn healing_prayer_heals_two_d6_plus_level_and_wisdom_for_six_mana() {
    let gs = make_test_game_state("prayer_heal");
    let mut rx = add_priest(&gs, "caster", 1).await;
    let mut ally = add_wounded(&gs, "ally", 5.0, 2).await;
    let id = pid("caster");
    gs.players.write().await.get_mut(&id).unwrap().level = 30;
    gs.player_characters
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .2
        .wis = 17;
    pray(&gs, None, Some("ally")).await;
    let health = gs.players.read().await[&pid("ally")].health;
    assert!(
        (25..=35).contains(&health),
        "2d6 + 10 + 3 on 10 HP: {health}"
    );
    assert_eq!(gs.mana.read().await[&id].mana, 9);
    assert!(gs.dirty_players.read().await.contains(&pid("ally")));
    let updates = drain(&mut rx);
    assert_eq!(prayed(&updates), Some(health - 10));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityCooldowns { cooldowns }
            if cooldowns.iter().any(|c| c.ability == PRAYER && c.remaining_ms == 8_000))));
    assert_eq!(health_seen(&drain(&mut ally), "ally"), Some((health, 100)));

    tokio::time::advance(Duration::from_millis(7_999)).await;
    pray(&gs, None, Some("ally")).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    assert_eq!(gs.mana.read().await[&id].mana, 9);
    tokio::time::advance(Duration::from_millis(1)).await;
    pray(&gs, None, Some("ally")).await;
    assert_eq!(gs.mana.read().await[&id].mana, 3);
}

#[tokio::test]
async fn healing_prayer_heals_the_caster_by_default_and_discards_overflow() {
    let gs = make_test_game_state("prayer_self");
    let mut rx = add_priest(&gs, "caster", 1).await;
    let id = pid("caster");
    gs.players.write().await.get_mut(&id).unwrap().health = 9;
    gs.use_ability(&id, PRAYER).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    pray(&gs, None, None).await;
    assert_eq!(gs.players.read().await[&id].health, 10);
    assert_eq!(prayed(&drain(&mut rx)), Some(1));
}

#[tokio::test]
async fn healing_prayer_rejects_without_cost() {
    let gs = make_test_game_state("prayer_rejects");
    let mut rx = add_priest(&gs, "caster", 1).await;
    add_wounded(&gs, "ally", 5.0, 2).await;
    let set_ally = |x: f32, floor: i8, health: u32| {
        let gs = gs.clone();
        async move {
            let mut players = gs.players.write().await;
            let ally = players.get_mut(&pid("ally")).unwrap();
            (ally.position.x, ally.floor_level, ally.health) = (x, floor, health);
        }
    };
    for (x, floor, health, reason) in [
        (5.0, 0, 100, AbilityRejectReason::FullHealth),
        (10.01, 0, 10, AbilityRejectReason::OutOfRange),
        (5.0, 1, 10, AbilityRejectReason::Unavailable),
        (5.0, 0, 0, AbilityRejectReason::Unavailable),
    ] {
        set_ally(x, floor, health).await;
        pray(&gs, None, Some("ally")).await;
        rejected(&mut rx, reason);
    }
    set_ally(1.0, 0, 10).await;
    gs.sync_region_furniture(0, 0, &[table_placement(0.5, 0.0)]);
    pray(&gs, None, Some("ally")).await;
    rejected(&mut rx, AbilityRejectReason::Blocked);
    gs.sync_region_furniture(0, 0, &[]);
    pray(&gs, None, None).await;
    rejected(&mut rx, AbilityRejectReason::FullHealth);
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .class = CharacterClass::Knight;
    pray(&gs, None, Some("ally")).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    assert_eq!(gs.mana.read().await[&pid("caster")].mana, 15);
    assert_eq!(gs.players.read().await[&pid("ally")].health, 10);

    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .class = CharacterClass::Priest;
    gs.mana.write().await.get_mut(&pid("caster")).unwrap().mana = 5;
    pray(&gs, None, Some("ally")).await;
    rejected(&mut rx, AbilityRejectReason::NotEnoughMana);
    gs.mana.write().await.get_mut(&pid("caster")).unwrap().mana = 6;
    pray(&gs, None, Some("ally")).await;
    assert!(gs.players.read().await[&pid("ally")].health > 10);
}

#[tokio::test]
async fn healing_prayer_harms_only_undead_monsters_and_can_kill_them() {
    let gs = make_test_game_state("prayer_undead");
    let mut rx = add_priest(&gs, "caster", 1).await;
    add_charge_target(&gs, "target", 6.0).await;
    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .monster_type = "goblin".into();
    pray(&gs, Some("target"), None).await;
    rejected(&mut rx, AbilityRejectReason::NotUndead);
    assert_eq!(gs.mana.read().await[&pid("caster")].mana, 15);

    gs.monsters
        .write()
        .await
        .get_mut("target")
        .unwrap()
        .monster_type = "skeleton".into();
    pray(&gs, Some("target"), Some("caster")).await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    pray(&gs, Some("target"), None).await;
    let health = gs.monsters.read().await["target"].health;
    assert!((988..=998).contains(&health), "{health}");
    assert_eq!(prayed(&drain(&mut rx)), Some(1000 - health));
    assert!(GameState::in_combat(
        &gs.players.read().await[&pid("caster")]
    ));

    add_charge_target(&gs, "weak", 6.0).await;
    {
        let mut monsters = gs.monsters.write().await;
        let weak = monsters.get_mut("weak").unwrap();
        weak.monster_type = "skeleton_weak".into();
        weak.health = 1;
    }
    gs.abilities.write().await.start_cooldown(1, PRAYER, 0);
    pray(&gs, Some("weak"), None).await;
    assert_eq!(gs.monsters.read().await["weak"].state, MonsterState::Dead);
}

#[tokio::test]
async fn healing_prayer_puts_the_caster_in_combat_only_when_the_target_fights() {
    let gs = make_test_game_state("prayer_combat_time");
    add_priest(&gs, "caster", 1).await;
    add_wounded(&gs, "ally", 5.0, 2).await;
    pray(&gs, None, Some("ally")).await;
    assert_eq!(gs.players.read().await[&pid("caster")].last_combat_at, 0);

    gs.abilities.write().await.start_cooldown(1, PRAYER, 0);
    gs.players
        .write()
        .await
        .get_mut(&pid("ally"))
        .unwrap()
        .last_combat_at = GameState::now_ms();
    pray(&gs, None, Some("ally")).await;
    assert!(GameState::in_combat(
        &gs.players.read().await[&pid("caster")]
    ));
}

#[tokio::test]
async fn other_players_hp_reaches_only_themselves_and_stethoscope_wearers() {
    let gs = make_test_game_state("hp_privacy");
    add_priest(&gs, "caster", 1).await;
    let mut ally = add_wounded(&gs, "ally", 5.0, 2).await;
    let mut watcher = add_ward_player(&gs, "watcher", 3.0, 3).await;
    let mut doctor = add_ward_player(&gs, "doctor", 4.0, 4).await;
    let mut gear = gs.inventories.read().await[&pid("doctor")].clone();
    gear.equipped
        .insert(EquipSlot::Neck, bag_item(9, "stethoscope", 1));
    gs.set_player_gear(&pid("doctor"), &gear).await;
    assert_eq!(health_seen(&drain(&mut doctor), "ally"), Some((10, 100)));

    pray(&gs, None, Some("ally")).await;
    let healed = gs.players.read().await[&pid("ally")].health;
    assert_eq!(health_seen(&drain(&mut ally), "ally"), Some((healed, 100)));
    assert_eq!(health_seen(&drain(&mut watcher), "ally"), Some((1, 0)));
    assert_eq!(
        health_seen(&drain(&mut doctor), "ally"),
        Some((healed, 100))
    );

    gs.reset_world_view(&pid("watcher")).await;
    assert!(drain(&mut watcher).iter().any(|m| matches!(m,
        ServerMessage::PlayerAppeared { player }
            if player.id == pid("ally") && player.health == 1 && player.max_health == 0)));

    let position = gs.players.read().await[&pid("ally")].position;
    gs.publish_nearby(
        &position,
        0,
        ServerMessage::MonsterSkillStruck {
            monster_id: "m".into(),
            skill: onlinerpg_shared::monster_ai::MonsterSkillId::EarthRoar,
            victims: vec![
                onlinerpg_shared::messages::SkillVictim {
                    player_id: pid("ally"),
                    damage: 5,
                    current_health: 40,
                },
                onlinerpg_shared::messages::SkillVictim {
                    player_id: pid("watcher"),
                    damage: 5,
                    current_health: 7,
                },
            ],
            burn: false,
        },
        None,
    )
    .await;
    let struck = |updates: Vec<ServerMessage>| {
        updates
            .into_iter()
            .find_map(|m| match m {
                ServerMessage::MonsterSkillStruck { victims, .. } => {
                    Some(victims.iter().map(|v| v.current_health).collect::<Vec<_>>())
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(struck(drain(&mut ally)), vec![40, 1]);
    assert_eq!(struck(drain(&mut watcher)), vec![1, 7]);
    assert_eq!(struck(drain(&mut doctor)), vec![40, 7]);

    gear.equipped.remove(&EquipSlot::Neck);
    gs.set_player_gear(&pid("doctor"), &gear).await;
    assert_eq!(health_seen(&drain(&mut doctor), "ally"), Some((1, 0)));
}

const HYMN: AbilityId = AbilityId::BattleHymn;
const MUSIC: &str = onlinerpg_shared::messages::MUSIC_EMOTE;

async fn add_bard(gs: &GameState, name: &str, x: f32, character: i64) -> DirectRx {
    let rx = add_ward_player(gs, name, x, character).await;
    gs.players.write().await.get_mut(&pid(name)).unwrap().class = CharacterClass::Bard;
    give(gs, &pid(name), bag_item(9, "worn_mandolin", 1)).await;
    rx
}

async fn hymn_speed(gs: &GameState, name: &str) -> f32 {
    gs.hunger_movement_profiles_for(&[pid(name)])
        .await
        .get(&pid(name))
        .map_or(1.0, |profile| profile.0)
}

fn hymn_badge_ms(updates: &[ServerMessage]) -> Option<u64> {
    updates.iter().rev().find_map(|m| match m {
        ServerMessage::BuffUpdate { buffs } => Some(
            buffs
                .iter()
                .find(|b| b.ability == HYMN)
                .map_or(0, |b| b.remaining_ms),
        ),
        _ => None,
    })
}

async fn finish_playing(gs: &GameState) {
    tokio::time::advance(Duration::from_millis(10_000)).await;
    gs.tick_battle_hymns().await;
}

#[tokio::test(start_paused = true)]
async fn battle_hymn_plays_ten_seconds_then_speeds_up_the_party_for_ten_minutes() {
    let gs = make_test_game_state("hymn_cast");
    let mut bard_rx = add_bard(&gs, "bard", 0.0, 1).await;
    let mut near_rx = add_ward_player(&gs, "near", 19.0, 2).await;
    add_ward_player(&gs, "far", 21.0, 3).await;
    add_ward_player(&gs, "stranger", 3.0, 4).await;
    party(&gs, "bard", "near").await;
    party(&gs, "bard", "far").await;
    drain(&mut near_rx);
    let base_interval = gs.player_attack_interval_ms(&pid("near")).await;

    gs.use_ability(&pid("bard"), HYMN).await;
    assert_eq!(
        gs.mana.read().await[&pid("bard")].mana,
        15,
        "paid at the end"
    );
    assert_eq!(
        gs.players.read().await[&pid("bard")].object_type.as_deref(),
        Some(MUSIC)
    );
    let updates = drain(&mut bard_rx);
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::BattleHymnStarted { player_id, tune, .. }
            if *player_id == pid("bard")
                && *tune < onlinerpg_shared::ability::BATTLE_HYMN_TUNES)));
    assert!(!updates
        .iter()
        .any(|m| matches!(m, ServerMessage::PlayerMusicStarted { .. })));
    assert!(drain(&mut near_rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::BattleHymnStarted { .. })));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityCooldowns { cooldowns }
            if cooldowns.iter().any(|c| c.ability == HYMN && c.remaining_ms == 10_000))));
    tokio::time::advance(Duration::from_millis(9_999)).await;
    gs.tick_battle_hymns().await;
    assert_eq!(hymn_speed(&gs, "near").await, 1.0);

    tokio::time::advance(Duration::from_millis(1)).await;
    gs.tick_battle_hymns().await;
    assert_eq!(gs.mana.read().await[&pid("bard")].mana, 10);
    assert!(
        gs.players.read().await[&pid("bard")].object_type.is_none(),
        "the strum ends with the hymn"
    );
    assert_eq!(hymn_speed(&gs, "bard").await, 1.08);
    assert_eq!(hymn_speed(&gs, "near").await, 1.08);
    assert_eq!(hymn_speed(&gs, "far").await, 1.0);
    assert_eq!(hymn_speed(&gs, "stranger").await, 1.0);
    assert_eq!(
        gs.player_attack_interval_ms(&pid("near")).await,
        (base_interval as f32 / 1.08).ceil() as u64
    );
    let updates = drain(&mut near_rx);
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::HungerUpdate { move_mult, attack_mult, .. }
            if *move_mult == 1.08 && *attack_mult == 1.08)));
    assert_eq!(hymn_badge_ms(&updates), Some(600_000));
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::AbilityUsed { ability, targets, .. }
            if *ability == HYMN && targets.len() == 2)));

    gs.players
        .write()
        .await
        .get_mut(&pid("near"))
        .unwrap()
        .position
        .x = 300.0;
    tokio::time::advance(Duration::from_millis(599_999)).await;
    gs.tick_debuffs().await;
    assert_eq!(
        hymn_speed(&gs, "near").await,
        1.08,
        "range only counts once"
    );
    tokio::time::advance(Duration::from_millis(2)).await;
    gs.tick_debuffs().await;
    assert_eq!(hymn_speed(&gs, "near").await, 1.0);
    let updates = drain(&mut near_rx);
    assert!(updates.iter().any(|m| matches!(m,
        ServerMessage::HungerUpdate { move_mult, .. } if *move_mult == 1.0)));
    assert_eq!(hymn_badge_ms(&updates), Some(0));
}

#[tokio::test(start_paused = true)]
async fn stopping_the_tune_interrupts_the_hymn_at_no_cost() {
    let gs = make_test_game_state("hymn_interrupt");
    let mut rx = add_bard(&gs, "bard", 0.0, 1).await;
    let id = pid("bard");
    gs.use_ability(&id, HYMN).await;
    gs.use_ability(&id, HYMN).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    gs.set_player_interaction(&id, None, None).await;
    rejected(&mut rx, AbilityRejectReason::Interrupted);
    assert!(!gs.abilities.read().await.hymns.contains_key(&id));
    finish_playing(&gs).await;
    assert_eq!(gs.mana.read().await[&id].mana, 15);
    assert_eq!(hymn_speed(&gs, "bard").await, 1.0);

    gs.use_ability(&id, HYMN).await;
    assert!(
        gs.abilities.read().await.hymns.contains_key(&id),
        "an interrupted hymn leaves no cooldown"
    );
}

#[tokio::test]
async fn a_landed_blow_interrupts_the_hymn() {
    let gs = make_test_game_state("hymn_hit");
    add_bard(&gs, "bard", 0.0, 1).await;
    let id = pid("bard");
    {
        let mut players = gs.players.write().await;
        let bard = players.get_mut(&id).unwrap();
        (bard.health, bard.max_health) = (500, 500);
    }
    let mut attacker = make_monster(
        "attacker",
        Position {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
        0,
    );
    attacker.level_override = Some(20);
    gs.monsters
        .write()
        .await
        .insert("attacker".into(), attacker);
    gs.use_ability(&id, HYMN).await;
    for _ in 0..50 {
        let before = gs.players.read().await[&id].health;
        gs.monster_attack("attacker", &id).await;
        let playing = gs.abilities.read().await.hymns.contains_key(&id);
        if gs.players.read().await[&id].health < before {
            assert!(!playing);
            assert_eq!(gs.mana.read().await[&id].mana, 15);
            assert!(gs.players.read().await[&id].object_type.is_none());
            return;
        }
        assert!(playing, "a miss leaves the hymn playing");
    }
    panic!("no blow landed");
}

#[tokio::test(start_paused = true)]
async fn battle_hymn_takes_over_a_live_performance() {
    let gs = make_test_game_state("hymn_live");
    let mut rx = add_bard(&gs, "bard", 0.0, 1).await;
    let id = pid("bard");
    gs.start_live_instrument(&id).await;
    drain(&mut rx);
    gs.use_ability(&id, HYMN).await;
    assert!(!gs.live_instrument_players.read().await.contains(&id));
    assert_eq!(
        gs.players.read().await[&id].object_type.as_deref(),
        Some(MUSIC)
    );
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::BattleHymnStarted { .. })));
    finish_playing(&gs).await;
    assert_eq!(hymn_speed(&gs, "bard").await, 1.08);
}

#[tokio::test(start_paused = true)]
async fn battle_hymn_takes_over_a_play_music_tune() {
    let gs = make_test_game_state("hymn_play_music");
    let auth = make_test_auth("hymn_play_music");
    add_bard(&gs, "bard", 0.0, 1).await;
    let id = pid("bard");
    gs.send_chat_message(&id, "/play_music twilight".to_string(), &auth)
        .await;
    assert!(gs.music_performances.read().await.contains_key(&id));
    gs.use_ability(&id, HYMN).await;
    assert!(!gs.music_performances.read().await.contains_key(&id));
    assert!(gs.abilities.read().await.hymns.contains_key(&id));
    gs.send_chat_message(&id, "/play_music twilight".to_string(), &auth)
        .await;
    assert!(
        !gs.abilities.read().await.hymns.contains_key(&id),
        "another tune cuts the hymn short"
    );
}

#[tokio::test(start_paused = true)]
async fn battle_hymn_needs_a_bard_an_instrument_and_mana_at_both_ends() {
    let gs = make_test_game_state("hymn_reject");
    let mut knight = add_ward_player(&gs, "knight", 0.0, 1).await;
    give(&gs, &pid("knight"), bag_item(9, "worn_mandolin", 1)).await;
    gs.use_ability(&pid("knight"), HYMN).await;
    rejected(&mut knight, AbilityRejectReason::Unavailable);

    let mut bard = add_bard(&gs, "bard", 5.0, 2).await;
    let id = pid("bard");
    gs.inventories
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .bag
        .clear();
    gs.use_ability(&id, HYMN).await;
    rejected(&mut bard, AbilityRejectReason::Equipment);
    give(&gs, &id, bag_item(9, "worn_mandolin", 1)).await;
    gs.mana.write().await.get_mut(&id).unwrap().mana = 4;
    gs.use_ability(&id, HYMN).await;
    rejected(&mut bard, AbilityRejectReason::NotEnoughMana);
    assert!(gs.players.read().await[&id].object_type.is_none());

    gs.mana.write().await.get_mut(&id).unwrap().mana = 5;
    gs.use_ability(&id, HYMN).await;
    gs.mana.write().await.get_mut(&id).unwrap().mana = 4;
    finish_playing(&gs).await;
    rejected(&mut bard, AbilityRejectReason::NotEnoughMana);
    assert_eq!(hymn_speed(&gs, "bard").await, 1.0);
    assert_eq!(gs.mana.read().await[&id].mana, 4);
    assert!(gs.players.read().await[&id].object_type.is_none());
}

#[tokio::test(start_paused = true)]
async fn another_hymn_renews_the_buff_without_stacking() {
    let gs = make_test_game_state("hymn_stack");
    add_bard(&gs, "bard", 0.0, 1).await;
    add_bard(&gs, "minstrel", 4.0, 2).await;
    let mut member_rx = add_ward_player(&gs, "member", 2.0, 3).await;
    party(&gs, "bard", "minstrel").await;
    party(&gs, "bard", "member").await;
    gs.use_ability(&pid("bard"), HYMN).await;
    finish_playing(&gs).await;
    tokio::time::advance(Duration::from_millis(300_000)).await;
    drain(&mut member_rx);
    gs.use_ability(&pid("minstrel"), HYMN).await;
    finish_playing(&gs).await;
    assert_eq!(hymn_speed(&gs, "member").await, 1.08);
    assert_eq!(hymn_badge_ms(&drain(&mut member_rx)), Some(600_000));
    assert!(gs.hunger.read().await[&pid("bard")]
        .hymn_active(tokio::time::Instant::now() + Duration::from_millis(599_000)));
}

#[tokio::test(start_paused = true)]
async fn death_drops_the_hymn_buff() {
    let gs = make_test_game_state("hymn_death");
    add_bard(&gs, "bard", 0.0, 1).await;
    gs.use_ability(&pid("bard"), HYMN).await;
    finish_playing(&gs).await;
    assert_eq!(hymn_speed(&gs, "bard").await, 1.08);
    gs.clear_debuffs(&pid("bard")).await;
    assert_eq!(hymn_speed(&gs, "bard").await, 1.0);
}
