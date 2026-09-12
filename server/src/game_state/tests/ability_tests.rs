use super::*;
use onlinerpg_shared::ability::{AbilityId, AbilityRejectReason};
use std::time::Duration;

const WARD: AbilityId = AbilityId::GuardianWard;

async fn add_ward_player(gs: &GameState, name: &str, x: f32, character: i64) -> DirectRx {
    gs.add_player(make_player(name, x, 0.0)).await;
    gs.mark_world_ready(&pid(name)).await;
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
        players.get_mut(&pid("downstairs")).unwrap().floor_level = -1;
        players.get_mut(&pid("dead")).unwrap().health = 0;
    }
    for rx in &mut receivers {
        messages(rx);
    }
    gs.use_ability(&pid("caster"), WARD).await;
    for name in ["caster", "edge"] {
        assert_eq!(gs.effective_guard(&pid(name)).await, 34);
    }
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
    tokio::join!(gs.use_ability(&caster, WARD), gs.use_ability(&caster, WARD));
    let casts = messages(&mut rx);
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
    tokio::time::advance(Duration::from_secs(1)).await;
    gs.use_ability(&pid("caster"), WARD).await;
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
