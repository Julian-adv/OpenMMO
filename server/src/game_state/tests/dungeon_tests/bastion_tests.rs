use super::*;

#[tokio::test]
async fn sundered_bastion_floors_spawn_their_own_pool() {
    let game_state = make_test_game_state("sundered_bastion_floors");
    let entrance = game_state.dungeon_defs.get("sundered_bastion").unwrap();
    for (depth, allowed) in [
        (1, &["stone_golem", "lizardfolk"][..]),
        (24, &["stone_golem", "cyclop", "skeleton_warrior"][..]),
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
            assert!(allowed.contains(&monster.monster_type.as_str()));
            let def = game_state.monster_defs.get(&monster.monster_type).unwrap();
            assert_eq!(
                monster.level_override,
                Some(onlinerpg_shared::dungeon::monster_level_for_depth(
                    def.level, depth
                ))
            );
            assert!(monster.aggressive);
        }
    }
}

#[tokio::test]
async fn sundered_bastion_spawns_barkan_and_rewards_every_floor_key() {
    let auth = make_test_auth("sundered_bastion_rewards");
    let account = auth.login_npc("npc_sundered_bastion_rewards").unwrap();
    let character = create_test_character(&auth, &account, "Delver");
    let game_state = make_test_game_state("sundered_bastion_rewards");
    let entrance = game_state.dungeon_defs.get("sundered_bastion").unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    let at = {
        let dungeons = game_state.dungeons.read().await;
        let layouts = &dungeons[&entrance.id].layouts;
        assert_eq!(layouts.len(), 25);
        cell_center(&entrance.position(), 25, layouts[24].chest.unwrap())
    };
    let mut player = make_player("Delver", at.x, at.z);
    player.position = at;
    player.floor_level = -25;
    let id = player.id;
    game_state.add_player(player).await;
    game_state
        .register_player_character(&id, character.id, 0, attrs_with_cha(12), 0, None)
        .await;
    game_state
        .handle_player_floor_change(&id, 0, -25, &at, &at)
        .await;

    let boss_id = {
        let index = game_state.dungeon_monsters.read().await;
        let bosses: Vec<_> = index
            .iter()
            .filter(|(_, entry)| entry.entrance_id == entrance.id && entry.is_boss)
            .collect();
        assert_eq!(bosses.len(), 1);
        assert_eq!(bosses[0].1.depth, 25);
        bosses[0].0.clone()
    };
    {
        let monsters = game_state.monsters.read().await;
        let boss = monsters.get(&boss_id).unwrap();
        assert_eq!(boss.monster_type, "barkan");
        assert_eq!(boss.level_override, Some(30));
        assert_eq!(boss.health, 600);
        assert!(boss.aggressive);
    }

    give_bag(&game_state, &id, None).await;
    for depth in onlinerpg_shared::dungeon::locked_depths(25) {
        assert!(
            game_state
                .give_item(&id, &entrance.key_item_id(depth))
                .await
        );
    }
    let mut rx = game_state.register_direct_channel(&id).await;
    game_state
        .open_dungeon_chest(&id, &entrance.id, &auth)
        .await;
    let gold = drain(&mut rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::DungeonChestOpened { gold, .. } => Some(gold),
            _ => None,
        })
        .expect("the floor keys open the chest while the boss is alive");
    assert!((12_500..=37_500).contains(&gold));
    let table: HashMap<_, _> = game_state
        .item_defs
        .chest_roll_table(entrance.chest_tier)
        .into_iter()
        .collect();
    assert_eq!(table["ring_of_protection"], 0.2);
}
