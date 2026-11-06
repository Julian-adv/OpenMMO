use super::*;

/// Each variant wears a stand-in model and fights like its Sundered Bastion
/// counterpart on the same floors.
#[tokio::test]
async fn blackvein_mine_variants_match_their_bastion_counterparts() {
    let game_state = make_test_game_state("blackvein_mine_variants");
    assert_variants(
        &game_state,
        &[
            ("mine_remnant", None, "stone_golem"),
            ("mine_crystal", None, "cyclop"),
            ("mine_twisted", None, "skeleton"),
            ("mine_veinfused", None, "skeleton_warrior"),
            ("mine_predator", Some("scp939"), "skeleton_warrior"),
            ("mine_foreman", None, "shadow_master"),
        ],
    );
}

#[tokio::test]
async fn blackvein_mine_floors_spawn_one_kind_per_band() {
    let game_state = make_test_game_state("blackvein_mine_floors");
    for (depth, kind) in [
        (1, "mine_remnant"),
        (8, "mine_crystal"),
        (13, "mine_twisted"),
        (18, "mine_veinfused"),
        (24, "mine_predator"),
    ] {
        assert_floor_pool(&game_state, "blackvein_mine", depth, &[kind]).await;
    }
    let last = floor_monsters(&game_state, "blackvein_mine", 25).await;
    let kinds: Vec<_> = last.iter().map(|m| m.monster_type.as_str()).collect();
    assert_eq!(kinds, ["mine_foreman"]);
}

#[tokio::test]
async fn blackvein_mine_spawns_the_foreman_without_signature_rolls() {
    let (boss, items, gold) = open_final_chest("blackvein_mine_rewards", "blackvein_mine").await;
    assert_eq!(boss.monster_type, "mine_foreman");
    assert_eq!(boss.level_override, Some(30));
    assert_eq!(boss.health, 600);
    assert!(boss.aggressive);
    let signatures = ["rune_blade", "earth_amulet", "shadowfang"];
    assert!(!items.iter().any(|id| signatures.contains(&id.as_str())));
    assert!((12_500..=37_500).contains(&gold));

    let game_state = make_test_game_state("blackvein_mine_table");
    let mine = chest_chances(&game_state, "blackvein_mine");
    assert_eq!(mine["ring_of_protection"], 0.2);
    for id in signatures {
        assert!(!mine.contains_key(id), "{id}");
    }
}

/// A pickaxe in the bag mines an ore heap away, sometimes into ore; sunset
/// refills it.
#[tokio::test]
async fn pickaxe_mines_ore_heaps_until_sunset() {
    let game_state = make_test_game_state("blackvein_mine_mining");
    let entrance = game_state.dungeon_defs.get("blackvein_mine").unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    let (depth, prop_id, at) = {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .layouts
            .iter()
            .find_map(|l| {
                let i = l
                    .props
                    .iter()
                    .position(|p| p.kind == onlinerpg_shared::dungeon::PropKind::MineOreHeap)?;
                let p = l.props[i];
                Some((
                    l.depth,
                    i as u32,
                    cell_center(&entrance.position(), l.depth, (p.x, p.z)),
                ))
            })
            .unwrap()
    };
    let mut player = make_player("Miner", at.x, at.z);
    player.position = at;
    player.floor_level = -(depth as i8);
    let id = player.id;
    game_state.add_player(player).await;
    let mined = || async {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .broken_props
            .get(&depth)
            .is_some_and(|ids| ids.contains(&prop_id))
    };

    give_bag(&game_state, &id, None).await;
    game_state
        .start_mining(&id, &entrance.id, depth, prop_id)
        .await;
    assert!(game_state.mining_sessions.read().await.is_empty());

    assert!(game_state.give_item(&id, "miner_pickaxe").await);
    game_state
        .start_mining(&id, &entrance.id, depth, prop_id)
        .await;
    game_state
        .mining_sessions
        .write()
        .await
        .get_mut(&id)
        .expect("a pickaxe starts the cast")
        .until = std::time::Instant::now();
    game_state.tick_mining().await;
    assert!(mined().await);
    let ore: u32 = game_state.inventories.read().await[&id]
        .bag
        .iter()
        .filter(|i| i.item_def_id == "blackvein_ore")
        .map(|i| i.quantity)
        .sum();
    assert!(ore == 0 || ore == 2 + u32::from(depth) / 8, "{ore}");

    game_state.restore_mine_props().await;
    assert!(!mined().await);
}

/// A pushed rail cart rolls until a body stops it. A player struck at push
/// time takes the hit when the cart arrives unless they stepped aside; a
/// monster takes it at once. Sunset rolls the cart back.
#[tokio::test(start_paused = true)]
async fn rail_carts_roll_into_bodies_until_sunset() {
    use onlinerpg_shared::dungeon::{cart_rail_run, PropKind};
    let game_state = make_test_game_state("blackvein_mine_carts");
    let entrance = game_state.dungeon_defs.get("blackvein_mine").unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    // As at boot, so carts and walls block what monsters see.
    game_state.passability_write().insert(
        onlinerpg_shared::dungeon::dungeon_cache_key(&entrance.id),
        onlinerpg_shared::dungeon::dungeon_passability(
            &entrance.position(),
            &onlinerpg_shared::dungeon::generate_dungeon_for(&entrance.id),
        ),
    );
    let (depth, prop_id, cart) = {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .layouts
            .iter()
            .find_map(|l| {
                l.props.iter().enumerate().find_map(|(i, p)| {
                    let run = cart_rail_run(l, p)?;
                    let along = if p.along_x() { p.x } else { p.z };
                    let clear = (along..along + 9).all(|a| {
                        let cell = run.cell(a);
                        !l.spawns.iter().any(|s| (s.x, s.z) == cell)
                    });
                    (p.kind == PropKind::MineRailCart && along + 9 <= run.end && clear)
                        .then_some((l.depth, i as u32, *p))
                })
            })
            .expect("a cart with room to roll")
    };
    let at = |offset: i32| {
        let s = cart.slid(offset);
        cell_center(&entrance.position(), depth, (s.x, s.z))
    };
    let floor = -(depth as i8);
    let place = |name: &str, offset: i32| {
        let mut p = make_player(name, 0.0, 0.0);
        p.position = at(offset);
        p.floor_level = floor;
        p
    };
    let pusher = place("Pusher", -1);
    let pusher_id = pusher.id;
    game_state.add_player(pusher).await;
    let blocker = place("Blocker", 5);
    let blocker_id = blocker.id;
    let full = blocker.health;
    game_state.add_player(blocker).await;
    let move_to = |id, offset| {
        let game_state = &game_state;
        async move {
            game_state
                .players
                .write()
                .await
                .get_mut(&id)
                .unwrap()
                .position = at(offset);
        }
    };
    let health = |id| {
        let game_state = &game_state;
        async move { game_state.players.read().await[&id].health }
    };
    let offset = || async {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .cart_offsets
            .get(&depth)
            .and_then(|m| m.get(&prop_id).copied())
            .unwrap_or(0)
    };
    let push = || game_state.push_dungeon_cart(&pusher_id, &entrance.id, depth, prop_id, None);
    let roll_time = std::time::Duration::from_secs(10);

    push().await;
    assert_eq!(offset().await, 3, "stops before the player");
    assert_eq!(
        health(blocker_id).await,
        full,
        "not hit before the cart arrives"
    );
    tokio::time::sleep(roll_time).await;
    assert!(health(blocker_id).await < full, "hit on arrival");

    game_state.restore_mine_props().await;
    assert_eq!(offset().await, 0);
    let after_first = health(blocker_id).await;
    push().await;
    move_to(blocker_id, 9).await;
    tokio::time::sleep(roll_time).await;
    assert_eq!(health(blocker_id).await, after_first, "stepped aside");

    game_state.restore_mine_props().await;
    move_to(blocker_id, 20).await;
    let monster = game_state
        .spawn_monster(
            "mine_crystal".to_string(),
            at(8),
            0.0,
            floor,
            crate::types::MonsterLifecycle::DungeonSlot,
            Some(10),
            true,
        )
        .await
        .expect("a monster on the rail");
    game_state.tick_monster_ai_by(200.0).await;
    push().await;
    assert_eq!(offset().await, 6, "stops against the monster");
    let hit = game_state
        .monsters
        .read()
        .await
        .get(&monster.id)
        .cloned()
        .unwrap();
    assert!(
        hit.health < hit.max_health,
        "{}/{}",
        hit.health,
        hit.max_health
    );
    // Struck from beyond its sensing range with the cart blocking its view,
    // it still turns on the pusher.
    let start = hit.position.dist_xz_sq(&at(-1));
    for _ in 0..10 {
        game_state.tick_monster_ai_by(200.0).await;
    }
    let now = game_state.monsters.read().await[&monster.id].position;
    assert!(now.dist_xz_sq(&at(-1)) < start, "it comes for the pusher");
}
