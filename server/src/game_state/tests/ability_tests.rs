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
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
        .await;
    rejected(&mut rx, AbilityRejectReason::Unavailable);
    gs.sync_region_furniture(0, 0, &[]);
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
        .await;
    assert!(gs
        .abilities
        .read()
        .await
        .marks_target(&pid("caster"), "target"));
    messages(&mut rx);
    gs.sync_region_furniture(0, 0, &[table_placement(4.5, 0.5)]);
    gs.player_attack(&pid("caster"), "target".to_owned(), None)
        .await;
    assert!(messages(&mut rx).iter().any(|m| matches!(
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
    assert!(messages(&mut rx).iter().any(|m| matches!(
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
    assert!(messages(&mut rx)
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
    assert!(messages(&mut rx).iter().any(|m| matches!(
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
    assert!(!messages(&mut rx)
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
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
            .await;
        rejected(&mut rx, AbilityRejectReason::Unavailable);
    }
    gs.players
        .write()
        .await
        .get_mut(&pid("caster"))
        .unwrap()
        .mount = None;
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
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
    messages(&mut caster_rx);
    messages(&mut party_rx);
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
        .await;
    let state = gs.abilities.read().await;
    assert!(state.marks_target(&pid("caster"), "target"));
    assert!(!state.marks_target(&pid("caster"), "other"));
    assert!(!state.marks_target(&pid("party"), "target"));
    drop(state);
    let updates = messages(&mut caster_rx);
    assert!(updates.iter().any(|m| matches!(m, ServerMessage::BowMarkUpdate { monster_id: Some(id), remaining_ms: 5000 } if id == "target")));
    assert!(updates.iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == MARK && b.remaining_ms == 5000))));
    assert!(!updates
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    assert!(messages(&mut party_rx).is_empty());
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
    assert!(messages(&mut caster_rx).iter().any(|m| matches!(
        m,
        ServerMessage::BowMarkUpdate {
            monster_id: None,
            remaining_ms: 0
        }
    )));
    assert!(messages(&mut party_rx).is_empty());
}

#[tokio::test(start_paused = true)]
async fn bow_mark_cooldown_is_atomic_and_survives_reconnect() {
    let gs = make_test_game_state("bow_mark_cooldown");
    let mut rx = add_mark_player(&gs, "caster", 1).await;
    add_mark_target(&gs, "target", 5.0).await;
    let caster = pid("caster");
    tokio::join!(
        gs.use_targeted_ability(&caster, MARK, Some("target"), None),
        gs.use_targeted_ability(&caster, MARK, Some("target"), None)
    );
    let updates = messages(&mut rx);
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
    gs.use_targeted_ability(&pid("reconnected"), MARK, Some("target"), None)
        .await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.use_targeted_ability(&pid("reconnected"), MARK, Some("target"), None)
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
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
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
        gs.use_targeted_ability(&pid("caster"), MARK, target, None)
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
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
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
    gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
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
        gs.use_targeted_ability(&pid("caster"), MARK, Some("target"), None)
            .await;
        messages(&mut rx);
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
        assert!(messages(&mut rx).iter().any(|m| matches!(
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
        assert!(messages(&mut rx).iter().any(|message| matches!(message, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|buff| buff.ability == RADIANCE && buff.remaining_ms > 119_000))));
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
    let updates = messages(&mut rx);
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
    assert!(!messages(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    tokio::time::advance(Duration::from_millis(799)).await;
    gs.use_ability(&caster, RADIANCE).await;
    rejected(&mut rx, AbilityRejectReason::Cooldown);
    tokio::time::advance(Duration::from_millis(1)).await;
    gs.use_ability(&caster, RADIANCE).await;
    assert!(gs.get_all_players().await[&caster].radiance_on);
    assert!(messages(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.iter().any(|b| b.ability == RADIANCE && b.remaining_ms == 120_000))));
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
    messages(&mut rx);
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.tick_buffs().await;
    assert!(!gs.get_all_players().await[&pid("caster")].radiance_on);
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
    assert!(messages(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.len() == 1 && buffs[0].ability == WARD)));
    gs.use_ability(&pid("caster"), RADIANCE).await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
}

#[tokio::test(start_paused = true)]
async fn ward_expiry_does_not_remove_radiance() {
    let gs = make_test_game_state("radiance_independent");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.use_ability(&pid("caster"), WARD).await;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    messages(&mut rx);
    tokio::time::advance(Duration::from_secs(60)).await;
    gs.tick_buffs().await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
    assert!(messages(&mut rx).iter().any(|m| matches!(m, ServerMessage::BuffUpdate { buffs } if buffs.len() == 1 && buffs[0].ability == RADIANCE)));
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
    messages(&mut nearby);
    messages(&mut downstairs);
    messages(&mut far);
    gs.use_ability(&pid("caster"), RADIANCE).await;
    let events = messages(&mut nearby);
    assert!(events.iter().any(|m| matches!(
        m,
        ServerMessage::PlayerRadianceToggled { enabled: true, .. }
    )));
    assert!(events
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { ability, .. } if *ability == RADIANCE)));
    for rx in [&mut downstairs, &mut far] {
        assert!(!messages(rx).iter().any(|m| matches!(
            m,
            ServerMessage::PlayerRadianceToggled { .. } | ServerMessage::AbilityUsed { .. }
        )));
    }
    assert!(!gs.get_all_players().await[&pid("nearby")].radiance_on);
    add_ward_player(&gs, "late", 1.0, 5).await;
    assert!(gs.get_all_players().await[&pid("caster")].radiance_on);
    tokio::time::advance(Duration::from_millis(800)).await;
    gs.use_ability(&pid("caster"), RADIANCE).await;
    assert!(messages(&mut nearby).iter().any(|m| matches!(
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

fn messages(rx: &mut DirectRx) -> Vec<ServerMessage> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

fn rejected(rx: &mut DirectRx, reason: AbilityRejectReason) {
    assert!(messages(rx).iter().any(
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
        gs.use_targeted_ability(&id, WARD, None, None).await;
        assert_eq!(gs.mana.read().await[&id].mana, before - 2);
        assert_eq!(gs.hunger_satiation(&id).await, Some(0));
        assert!(gs.dirty_players.read().await.contains(&id));
        assert_eq!(
            gs.get_player_save_data(&id).await.unwrap().mana,
            Some(before - 2)
        );
        let updates = messages(&mut rx);
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
        assert_eq!(gs.effective_guard(&id).await, 34);
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
    messages(&mut rx);
    gs.use_ability(&id, WARD).await;
    assert_eq!(gs.mana.read().await[&id].mana, 13);
    assert_eq!(gs.hunger_satiation(&id).await, Some(SATIATION_START));
    assert!(messages(&mut rx).iter().any(|message| matches!(message,
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
        gs.use_targeted_ability(&id, WARD, None, None).await;
        let responses = messages(&mut rx);
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
    gs.use_targeted_ability(&id, WARD, None, None).await;
    assert_eq!(gs.effective_guard(&id).await, 34);
    assert!(messages(&mut rx).iter().any(|message| matches!(message,
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
            assert_eq!(gs.effective_guard(&pid(&name)).await, before + before / 10);
            assert!(messages(&mut rx)
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
        messages(rx);
    }
    gs.use_ability(&pid("caster"), WARD).await;
    for name in ["caster", "edge"] {
        assert_eq!(gs.effective_guard(&pid(name)).await, 34);
    }
    assert_eq!(gs.mana.read().await[&pid("caster")].mana, 13);
    assert_eq!(gs.mana.read().await[&pid("edge")].mana, 15);
    assert!(!messages(&mut receivers[1])
        .iter()
        .any(|message| matches!(message, ServerMessage::ManaUpdate { .. })));
    for name in ["outside", "downstairs", "dead", "stranger"] {
        assert_eq!(gs.effective_guard(&pid(name)).await, 31);
    }
    assert!(!messages(&mut receivers[3])
        .iter()
        .any(|m| matches!(m, ServerMessage::AbilityUsed { .. })));
    let cast = messages(&mut receivers[0]);
    assert!(cast.iter().any(|m| matches!(m, ServerMessage::AbilityUsed { targets, .. } if targets.len() == 2 && targets.contains(&pid("edge")))));
}

#[tokio::test(start_paused = true)]
async fn ward_expires_at_sixty_seconds_and_restores_reported_guard() {
    let gs = make_test_game_state("ward_expiry");
    let mut rx = add_ward_player(&gs, "caster", 0.0, 1).await;
    gs.use_ability(&pid("caster"), WARD).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
    messages(&mut rx);
    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 31);
    gs.tick_buffs().await;
    let updates = messages(&mut rx);
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
    let casts = messages(&mut rx);
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
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
    tokio::time::advance(Duration::from_secs(59)).await;
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
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
    assert_eq!(gs.effective_guard(&pid("second")).await, 34);
    tokio::time::advance(Duration::from_secs(29)).await;
    assert_eq!(gs.effective_guard(&pid("second")).await, 31);
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
    assert_eq!(gs.effective_guard(&pid("caster")).await, 34);
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
    assert_eq!(gs.effective_guard(&pid("new")).await, 34);
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
    gs.use_targeted_ability(&pid("examiner"), AbilityId::Auscultation, monster, player)
        .await;
}

fn inspection(rx: &mut DirectRx) -> InspectionResult {
    messages(rx)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::InspectionResult { inspection } => Some(inspection),
            _ => None,
        })
        .expect("inspection reply")
}

fn inspection_rejected(rx: &mut DirectRx, reason: AbilityRejectReason) {
    let updates = messages(rx);
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
    messages(&mut target_rx);
    messages(&mut rx);
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
    assert!(messages(&mut target_rx).is_empty());
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
    messages(&mut rx);
    messages(&mut signe_rx);

    examine(&gs, None, Some(signe)).await;

    let result = inspection(&mut rx);
    assert_eq!(result.name, "Signe");
    assert_eq!(result.target, InspectionTarget::Player { player_id: signe });
    assert_eq!(
        gs.players.read().await[&signe].object_type.as_deref(),
        Some(onlinerpg_shared::messages::MUSIC_EMOTE)
    );
    assert!(gs.live_instrument_players.read().await.contains(&signe));
    assert!(messages(&mut signe_rx).is_empty());
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
        messages(&mut rx);

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
    messages(&mut rx);
    examine(&gs, None, Some(pid("target"))).await;
    inspection_rejected(&mut rx, AbilityRejectReason::Equipment);
    gs.equip_item(&pid("examiner"), 4).await;
    messages(&mut rx);
    examine(&gs, None, Some(pid("target"))).await;
    inspection(&mut rx);
    gs.unequip_item(&pid("examiner"), EquipSlot::Neck).await;
    messages(&mut rx);
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
    let updates = messages(&mut rx);
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
