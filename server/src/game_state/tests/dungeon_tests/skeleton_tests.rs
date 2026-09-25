use super::*;

#[tokio::test]
async fn skeleton_crypt_early_floors_spawn_weaker_skeletons() {
    let game_state = make_test_game_state("skeleton_crypt_early_floors");
    let entrance = game_state.dungeon_defs.get("skeleton_crypt").unwrap();
    let weak = game_state.monster_defs.get("skeleton_weak").unwrap();
    let regular = game_state.monster_defs.get("skeleton").unwrap();
    assert_eq!(weak.model, regular.model);
    assert_eq!(weak.guard, 14);
    assert_eq!(weak.damage_roll(), "3d6");
    assert_eq!(regular.guard, 20);
    assert_eq!(regular.damage_roll(), "6d8");
    for (depth, expected_type, expected_level, expected_health) in [
        (1, "skeleton_weak", 8, 40),
        (5, "skeleton_weak", 8, 40),
        (6, "skeleton", 17, 77),
        (10, "skeleton", 19, 86),
    ] {
        let mut player = make_player(&format!("delver{depth}"), entrance.x, entrance.z);
        player.floor_level = -(depth as i8);
        player.position.y = onlinerpg_shared::dungeon::floor_world_y(entrance.y, depth);
        let id = player.id;
        let at = player.position;
        game_state.add_player(player).await;
        game_state
            .handle_player_floor_change(&id, 0, -(depth as i8), &at, &at)
            .await;
        let ids: Vec<_> = game_state
            .dungeon_monsters
            .read()
            .await
            .iter()
            .filter(|(_, entry)| entry.entrance_id == entrance.id && entry.depth == depth)
            .map(|(id, _)| id.clone())
            .collect();
        assert!(!ids.is_empty());
        let monsters = game_state.monsters.read().await;
        for id in ids {
            let monster = monsters.get(&id).unwrap();
            assert_eq!(monster.monster_type, expected_type);
            assert_eq!(monster.level_override, Some(expected_level));
            assert_eq!(monster.health, expected_health);
            assert_eq!(monster.max_health, expected_health);
            assert!(monster.aggressive);
        }
    }
}

#[tokio::test]
async fn skeleton_crypt_spawns_its_boss_and_rewards_the_twentieth_floor_key() {
    let auth = make_test_auth("skeleton_crypt_rewards");
    let account = auth.login_npc("npc_skeleton_crypt_rewards").unwrap();
    let character = create_test_character(&auth, &account, "Delver");
    let game_state = make_test_game_state("skeleton_crypt_rewards");
    let entrance = game_state.dungeon_defs.get("skeleton_crypt").unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    let at = {
        let dungeons = game_state.dungeons.read().await;
        let layouts = &dungeons[&entrance.id].layouts;
        assert_eq!(layouts.len(), 20);
        cell_center(&entrance.position(), 20, layouts[19].chest.unwrap())
    };
    let mut player = make_player("Delver", at.x, at.z);
    player.position = at;
    player.floor_level = -20;
    let id = player.id;
    game_state.add_player(player).await;
    game_state
        .register_player_character(&id, character.id, 0, attrs_with_cha(12), 0, None)
        .await;
    game_state
        .handle_player_floor_change(&id, 0, -20, &at, &at)
        .await;

    let boss_id = {
        let index = game_state.dungeon_monsters.read().await;
        let bosses: Vec<_> = index
            .iter()
            .filter(|(_, entry)| entry.entrance_id == entrance.id && entry.is_boss)
            .collect();
        assert_eq!(bosses.len(), 1);
        assert_eq!(bosses[0].1.depth, 20);
        bosses[0].0.clone()
    };
    {
        let monsters = game_state.monsters.read().await;
        let boss = monsters.get(&boss_id).unwrap();
        assert_eq!(boss.monster_type, "skeleton_knight");
        assert_eq!(boss.level_override, Some(25));
        assert_eq!(boss.health, 350);
        assert_eq!(boss.max_health, 350);
        assert!(boss.aggressive);
    }

    give_bag(&game_state, &id, Some("skeleton_key_15")).await;
    let mut rx = game_state.register_direct_channel(&id).await;
    game_state
        .open_dungeon_chest(&id, &entrance.id, &auth)
        .await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ServerMessage::InteractionRejected { .. })
    ));
    assert_eq!(game_state.get_player_gold(&id).await, 0);

    assert!(game_state.give_item(&id, "skeleton_key_20").await);
    game_state
        .open_dungeon_chest(&id, &entrance.id, &auth)
        .await;
    let (items, gold) = drain(&mut rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::DungeonChestOpened {
                item_def_ids, gold, ..
            } => Some((item_def_ids, gold)),
            _ => None,
        })
        .expect("final key opens the chest while the boss is alive");
    for signature in ["breastplate", "great_sword"] {
        assert_eq!(
            items.iter().filter(|id| id.as_str() == signature).count(),
            1
        );
    }
    assert!((10_000..=30_000).contains(&gold));
    assert_eq!(game_state.get_player_gold(&id).await, gold);
    let inventories = game_state.inventories.read().await;
    assert!(inventories[&id]
        .bag
        .iter()
        .all(|item| !item.item_def_id.starts_with("skeleton_key_")));
    let table: HashMap<_, _> = game_state
        .item_defs
        .chest_roll_table(entrance.chest_tier)
        .into_iter()
        .collect();
    assert_eq!(table["plate_helmet"], 0.3);
    assert_eq!(table["plate_gauntlets"], 0.3);
    assert_eq!(table["plate_boots"], 0.1);
    assert_eq!(table["plate_greaves"], 0.1);
    assert!(!table.contains_key("ring_of_protection"));
    assert!(!table.contains_key("rune_blade"));
}
