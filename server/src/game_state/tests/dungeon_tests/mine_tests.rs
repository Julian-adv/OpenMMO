use super::*;

/// Each variant wears a stand-in model and fights like its Sundered Bastion
/// counterpart on the same floors.
#[tokio::test]
async fn blackvein_mine_variants_match_their_bastion_counterparts() {
    let game_state = make_test_game_state("blackvein_mine_variants");
    assert_variants(
        &game_state,
        &[
            ("mine_remnant", "hobgoblin", "stone_golem"),
            ("mine_crystal", "orc", "cyclop"),
            ("mine_twisted", "troll", "skeleton"),
            ("mine_veinfused", "stone_golem", "skeleton_warrior"),
            ("mine_predator", "scp939", "skeleton_warrior"),
            ("mine_foreman", "cyclop", "shadow_master"),
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

/// A push from behind slides a rail cart four cells on, a player on the rail
/// stops it short, and sunset rolls it back.
#[tokio::test]
async fn rail_carts_slide_from_a_push_until_sunset() {
    use onlinerpg_shared::dungeon::{cart_rail_run, PropKind, CART_PUSH_CELLS};
    let game_state = make_test_game_state("blackvein_mine_carts");
    let entrance = game_state.dungeon_defs.get("blackvein_mine").unwrap();
    game_state.ensure_dungeon_runtime(&entrance.id).await;
    let (depth, prop_id, cart, run) = {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .layouts
            .iter()
            .find_map(|l| {
                l.props.iter().enumerate().find_map(|(i, p)| {
                    let run = cart_rail_run(l, p)?;
                    let room =
                        (if p.along_x() { p.x } else { p.z }) + 2 + 2 * CART_PUSH_CELLS <= run.end;
                    (p.kind == PropKind::MineRailCart && room)
                        .then_some((l.depth, i as u32, *p, run))
                })
            })
            .expect("a cart with room to roll")
    };
    let at = |offset: i32| {
        let s = cart.slid(offset);
        cell_center(&entrance.position(), depth, (s.x, s.z))
    };
    let mut pusher = make_player("Pusher", 0.0, 0.0);
    pusher.position = at(-1);
    pusher.floor_level = -(depth as i8);
    let pusher_id = pusher.id;
    game_state.add_player(pusher).await;
    let offset = || async {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id]
            .cart_offsets
            .get(&depth)
            .and_then(|m| m.get(&prop_id).copied())
            .unwrap_or(0)
    };

    game_state
        .push_dungeon_cart(&pusher_id, &entrance.id, depth, prop_id)
        .await;
    assert_eq!(offset().await, CART_PUSH_CELLS, "{run:?}");

    let mut blocker = make_player("Blocker", 0.0, 0.0);
    blocker.position = at(CART_PUSH_CELLS + 3);
    blocker.floor_level = -(depth as i8);
    game_state.add_player(blocker).await;
    game_state
        .players
        .write()
        .await
        .get_mut(&pusher_id)
        .unwrap()
        .position = at(CART_PUSH_CELLS - 1);
    game_state
        .push_dungeon_cart(&pusher_id, &entrance.id, depth, prop_id)
        .await;
    assert_eq!(offset().await, CART_PUSH_CELLS + 1);

    game_state.restore_mine_props().await;
    assert_eq!(offset().await, 0);
}
