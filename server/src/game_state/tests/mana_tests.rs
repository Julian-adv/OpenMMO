use super::*;
use onlinerpg_shared::mana::MANA_REGEN_INTERVAL_MS;
use tokio::time::{advance, Duration};

async fn add_mana_player(gs: &GameState, name: &str, saved: Option<u32>) -> (PlayerId, DirectRx) {
    let player = make_player(name, 100.0, 50.0);
    let id = player.id;
    gs.register_mana(&player, 10, saved).await;
    gs.add_player(player).await;
    gs.register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(0))
        .await;
    (id, gs.register_direct_channel(&id).await)
}

#[tokio::test(start_paused = true)]
async fn mana_regenerates_with_full_hp_and_empty_hunger_only_for_its_owner() {
    let gs = make_test_game_state("mana_independent_regen");
    let (id, mut owner) = add_mana_player(&gs, "caster", Some(0)).await;
    let (_, mut other) = add_mana_player(&gs, "other", None).await;
    drain(&mut owner);
    drain(&mut other);
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 0);
    advance(Duration::from_millis(MANA_REGEN_INTERVAL_MS)).await;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 1);
    assert_eq!(gs.players.read().await[&id].health, 10);
    assert_eq!(gs.hunger_satiation(&id).await, Some(0));
    assert!(drain(&mut owner).into_iter().any(|message| matches!(
        message,
        ServerMessage::ManaUpdate {
            mana: 1,
            max_mana: 15
        }
    )));
    assert!(!drain(&mut other)
        .into_iter()
        .any(|message| matches!(message, ServerMessage::ManaUpdate { .. })));
    assert_eq!(gs.get_player_save_data(&id).await.unwrap().mana, Some(1));
}

#[tokio::test(start_paused = true)]
async fn mana_regen_checks_death_loading_combat_and_last_spend_without_banking_ticks() {
    let gs = make_test_game_state("mana_regen_gates");
    let (id, _rx) = add_mana_player(&gs, "caster", Some(10)).await;
    advance(Duration::from_secs(100)).await;
    gs.players.write().await.get_mut(&id).unwrap().health = 0;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 10);
    assert!(gs.revive_in_place(&id, 100).await);
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 10);
    advance(Duration::from_secs(10)).await;
    gs.players.write().await.get_mut(&id).unwrap().ready_at = GameState::now_ms() + 30_000;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 10);
    gs.mark_world_ready(&id).await;
    gs.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = GameState::now_ms();
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 10);
    gs.players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .last_combat_at = 0;
    gs.mana
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .spend(2)
        .unwrap();
    advance(Duration::from_secs(9)).await;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 8);
    advance(Duration::from_secs(1)).await;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 9);
}

#[tokio::test(start_paused = true)]
async fn mana_growth_and_wis_changes_preserve_current_amount_and_clamp_decreases() {
    let gs = make_test_game_state("mana_growth");
    let (id, _rx) = add_mana_player(&gs, "caster", Some(7)).await;
    {
        let mut players = gs.players.write().await;
        players.get_mut(&id).unwrap().class = CharacterClass::Wizard;
        players.get_mut(&id).unwrap().level = 50;
    }
    gs.player_characters
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .2
        .wis = 14;
    gs.refresh_player_mana(&id).await;
    assert_eq!(gs.mana.read().await[&id].max_mana, 194);
    assert_eq!(gs.mana.read().await[&id].mana, 7);
    gs.mana.write().await.get_mut(&id).unwrap().mana = 194;
    gs.players.write().await.get_mut(&id).unwrap().level = 1;
    gs.refresh_player_mana(&id).await;
    assert_eq!(gs.mana.read().await[&id].max_mana, 17);
    assert_eq!(gs.mana.read().await[&id].mana, 17);
    advance(Duration::from_millis(MANA_REGEN_INTERVAL_MS)).await;
    gs.tick_regeneration().await;
    assert_eq!(gs.mana.read().await[&id].mana, 17);
}

#[tokio::test(start_paused = true)]
async fn mana_initializes_once_and_saved_zero_survives_reconnect_and_server_restart() {
    let auth = make_test_auth("mana_persistence");
    let account = auth.login_npc("npc_mana_persistence").unwrap();
    let record = create_test_character(&auth, &account, "Manaknig");
    assert_eq!(record.mana, None);
    for amount in [7, 0] {
        let gs = make_test_game_state(&format!("mana_persist_{amount}"));
        let (id, _rx) = add_mana_player(&gs, "caster", None).await;
        gs.player_characters.write().await.get_mut(&id).unwrap().0 = record.id;
        assert_eq!(gs.mana.read().await[&id].mana, 15);
        gs.mana
            .write()
            .await
            .get_mut(&id)
            .unwrap()
            .spend(15 - amount)
            .unwrap();
        gs.mark_dirty(&id).await;
        gs.flush_dirty_saves(&auth).await;
        let saved = auth.get_character_for_account(&account, record.id).unwrap();
        assert_eq!(saved.mana, Some(amount));
        gs.unregister_player_character(&id).await;
        assert!(!gs.mana.read().await.contains_key(&id));
        advance(Duration::from_secs(3600)).await;
        let next = make_test_game_state(&format!("mana_restart_{amount}"));
        let (new_id, _rx) = add_mana_player(&next, "reconnected", saved.mana).await;
        next.tick_regeneration().await;
        assert_eq!(next.mana.read().await[&new_id].mana, amount);
    }
}

#[tokio::test]
async fn mana_exempts_official_npcs_and_clamps_saved_values() {
    let gs = make_test_game_state("mana_npc");
    let mut player = make_player("npc", 0.0, 0.0);
    player.is_official_npc = true;
    assert!(gs.register_mana(&player, 10, None).await.is_none());
    assert!(!gs.mana.read().await.contains_key(&player.id));
    player.is_official_npc = false;
    assert!(matches!(
        gs.register_mana(&player, 10, Some(999)).await,
        Some(ServerMessage::ManaUpdate {
            mana: 15,
            max_mana: 15
        })
    ));
}
