use super::*;
use onlinerpg_shared::dungeon::{cell_center, ENTRANCE_DOOR_ID};

mod bastion_tests;
mod chest_tests;
mod den_tests;
mod discovery_tests;
mod door_tests;
mod guild_tests;
mod layout_tests;
mod mine_tests;
mod monster_tests;
mod population_tests;
mod reconnect_tests;
mod respawn_tests;
mod skeleton_tests;

/// Monsters spawned on `depth` of `entrance_id` once a player arrives there.
async fn floor_monsters(
    game_state: &GameState,
    entrance_id: &str,
    depth: u8,
) -> Vec<onlinerpg_shared::entity::Monster> {
    let entrance = game_state.dungeon_defs.get(entrance_id).unwrap();
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
    let monsters = game_state.monsters.read().await;
    ids.iter()
        .map(|id| monsters.get(id).unwrap().clone())
        .collect()
}

/// Every monster on `depth` of `entrance_id` is one of `allowed`, aggressive
/// and levelled for its depth.
async fn assert_floor_pool(game_state: &GameState, entrance_id: &str, depth: u8, allowed: &[&str]) {
    let monsters = floor_monsters(game_state, entrance_id, depth).await;
    assert!(!monsters.is_empty(), "{entrance_id} floor {depth}");
    for monster in monsters {
        assert!(
            allowed.contains(&monster.monster_type.as_str()),
            "{entrance_id} floor {depth}: {}",
            monster.monster_type
        );
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

/// Each `(variant, model_of, stats_of)` wears `model_of`'s look, unless it has
/// its own (`None`), and fights with `stats_of`'s numbers.
fn assert_variants(game_state: &GameState, rows: &[(&str, Option<&str>, &str)]) {
    let defs = &game_state.monster_defs;
    for &(variant, model_of, stats_of) in rows {
        let (v, s) = (defs.get(variant).unwrap(), defs.get(stats_of).unwrap());
        if let Some(model_of) = model_of {
            let m = defs.get(model_of).unwrap();
            assert_eq!(v.model, m.model, "{variant}");
            assert_eq!(v.weapon, m.weapon, "{variant}");
            assert_eq!(v.hit_debuff, m.hit_debuff, "{variant}");
        }
        assert_eq!(
            (
                v.level,
                v.guard,
                v.attack_bonus(),
                v.max_health(),
                v.damage_roll()
            ),
            (
                s.level,
                s.guard,
                s.attack_bonus(),
                s.max_health(),
                s.damage_roll()
            ),
            "{variant}"
        );
    }
}

/// The final chest's item chances for `entrance_id`.
fn chest_chances(game_state: &GameState, entrance_id: &str) -> HashMap<String, f32> {
    let entrance = game_state.dungeon_defs.get(entrance_id).unwrap();
    game_state
        .item_defs
        .dungeon_chest_chances(entrance)
        .into_iter()
        .collect()
}

/// Walk a fresh character onto the final floor holding every floor key and
/// open the chest. Returns the boss and the chest's items and gold.
async fn open_final_chest(
    name: &str,
    entrance_id: &str,
) -> (onlinerpg_shared::entity::Monster, Vec<String>, i64) {
    let auth = make_test_auth(name);
    let account = auth.login_npc(&format!("npc_{name}")).unwrap();
    let character = create_test_character(&auth, &account, "Delver");
    let game_state = make_test_game_state(name);
    let entrance = game_state.dungeon_defs.get(entrance_id).unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    let (total, at) = {
        let dungeons = game_state.dungeons.read().await;
        let layouts = &dungeons[&entrance.id].layouts;
        let total = layouts.len() as u8;
        let chest = layouts.last().unwrap().chest.unwrap();
        (total, cell_center(&entrance.position(), total, chest))
    };
    let mut player = make_player("Delver", at.x, at.z);
    player.position = at;
    player.floor_level = -(total as i8);
    let id = player.id;
    game_state.add_player(player).await;
    game_state
        .register_player_character(&id, character.id, 0, attrs_with_cha(12), 0, None)
        .await;
    game_state
        .handle_player_floor_change(&id, 0, -(total as i8), &at, &at)
        .await;

    let boss = {
        let index = game_state.dungeon_monsters.read().await;
        let bosses: Vec<_> = index
            .iter()
            .filter(|(_, entry)| entry.entrance_id == entrance.id && entry.is_boss)
            .collect();
        assert_eq!(bosses.len(), 1);
        assert_eq!(bosses[0].1.depth, total);
        game_state.monsters.read().await[bosses[0].0.as_str()].clone()
    };

    give_bag(&game_state, &id, None).await;
    for depth in onlinerpg_shared::dungeon::locked_depths(total) {
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
    let (items, gold) = drain(&mut rx)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::DungeonChestOpened {
                item_def_ids, gold, ..
            } => Some((item_def_ids, gold)),
            _ => None,
        })
        .expect("the floor keys open the chest while the boss is alive");
    (boss, items, gold)
}

/// Give `player_id` an empty bag, plus one `item_def_id` when given.
async fn give_bag(game_state: &GameState, player_id: &PlayerId, item_def_id: Option<&str>) {
    game_state
        .inventories
        .write()
        .await
        .insert(*player_id, Default::default());
    if let Some(id) = item_def_id {
        assert!(game_state.give_item(player_id, id).await);
    }
}
