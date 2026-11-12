use super::*;
use crate::game_state::dungeon_layout::night_variant;
use onlinerpg_shared::dungeon::{
    dungeon_cache_key, dungeon_passability, generate_dungeon_for, generate_dungeon_variant,
    interior_doors, layout_hash, layout_is_traversable,
};
use onlinerpg_shared::ServerMessage;

const NIGHTLY: &str = "old_crypt";
const FIXED: &str = "orc_warrens";

#[test]
fn the_nights_variant_is_a_function_of_the_epoch() {
    let def = onlinerpg_shared::dungeon::entrance(NIGHTLY).unwrap();
    let fixed = generate_dungeon_for(NIGHTLY);
    let mut seen = std::collections::HashSet::new();
    for epoch in 0..40 {
        let (variant, layouts) = night_variant(def, epoch);
        assert_eq!(variant, night_variant(def, epoch).0, "night {epoch}");
        assert_ne!(variant, 0, "night {epoch}");
        assert_eq!(layouts.len(), fixed.len(), "night {epoch}");
        assert!(
            layout_is_traversable(&def.position(), &layouts),
            "night {epoch}"
        );
        seen.insert(layout_hash(&layouts));
    }
    assert_eq!(seen.len(), 40);
}

#[tokio::test]
async fn sunset_redraws_only_the_nightly_dungeon() {
    let game = make_test_game_state("nightly_layout_redraw");
    game.debug_set_time(12, 0);
    game.tick_dungeon_reset().await;
    game.init_passability(&crate::terrain::io::TerrainIO::new(
        "nonexistent_terrain_dir".into(),
    ))
    .await
    .unwrap();
    let door_subjects = |layouts: &[onlinerpg_shared::dungeon::FloorLayout]| {
        layouts
            .iter()
            .flat_map(|l| {
                interior_doors(l)
                    .into_iter()
                    .map(move |d| format!("dungeon:{NIGHTLY}:{}:{}", l.depth, d.door_id))
            })
            .collect::<std::collections::HashSet<_>>()
    };
    let old_doors = door_subjects(&generate_dungeon_for(NIGHTLY));
    assert!(old_doors
        .iter()
        .all(|key| game.interest_lock().has_subject(key)));
    for id in [NIGHTLY, FIXED] {
        game.ensure_dungeon_runtime(id).await;
    }
    let entrance = game.dungeon_defs.get(NIGHTLY).unwrap().position();
    let arrival = cell_center(
        &entrance,
        1,
        generate_dungeon_for(NIGHTLY)[0].up_shaft.exit_cell(),
    );
    let mut player = make_player("Delver", arrival.x, arrival.z);
    player.position = arrival;
    player.floor_level = -1;
    let id = player.id;
    game.add_player(player).await;
    game.handle_player_floor_change(&id, 0, -1, &arrival, &arrival)
        .await;
    {
        let mut dungeons = game.dungeons.write().await;
        dungeons.get_mut(FIXED).unwrap().floors.insert(
            1,
            super::dungeon::FloorRuntime {
                slots: vec![super::dungeon::SpawnSlot {
                    alive_monster_id: None,
                    respawn_at_ms: super::dungeon::BOSS_RESPAWN_NEVER,
                    is_boss: true,
                }],
                players: HashMap::new(),
            },
        );
        let rt = dungeons.get_mut(NIGHTLY).unwrap();
        rt.broken_props.entry(1).or_default().insert(0);
        rt.open_doors.entry(0).or_default().insert(ENTRANCE_DOOR_ID);
        rt.open_doors.entry(1).or_default().insert(7);
    }

    game.debug_set_time(23, 0);
    game.tick_dungeon_reset().await;

    let variant = game.dungeon_variant(NIGHTLY);
    assert_ne!(variant, 0);
    assert_eq!(game.dungeon_variant(FIXED), 0);
    assert_eq!(game.players.read().await[&id].floor_level, 0);
    {
        let dungeons = game.dungeons.read().await;
        let rt = &dungeons[NIGHTLY];
        assert_eq!(
            layout_hash(&rt.layouts),
            layout_hash(&generate_dungeon_variant(NIGHTLY, variant))
        );
        assert_ne!(
            layout_hash(&rt.layouts),
            layout_hash(&generate_dungeon_for(NIGHTLY))
        );
        assert!(rt.floors.is_empty() && rt.broken_props.is_empty());
        assert_eq!(rt.open_doors.len(), 1);
        assert!(rt.open_doors[&0].contains(&ENTRANCE_DOOR_ID));
        assert_eq!(
            layout_hash(&dungeons[FIXED].layouts),
            layout_hash(&generate_dungeon_for(FIXED))
        );
        assert_eq!(dungeons[FIXED].floors[&1].slots[0].respawn_at_ms, 0);
    }
    assert!(matches!(
        game.dungeon_layout_variants_message(),
        ServerMessage::DungeonLayoutVariants { variants }
            if variants == vec![(NIGHTLY.to_string(), variant)]
    ));

    let layouts = generate_dungeon_variant(NIGHTLY, variant);
    let cells = |rp: &onlinerpg_shared::pathfinding::RuntimePassability| {
        rp.floors
            .iter()
            .map(|f| f.cells.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        cells(
            game.passability_read()
                .get(&dungeon_cache_key(NIGHTLY))
                .unwrap()
        ),
        cells(&dungeon_passability(&entrance, &layouts))
    );
    let new_doors = door_subjects(&layouts);
    assert!(old_doors.difference(&new_doors).count() > 0);
    {
        let interest = game.interest_lock();
        assert!(new_doors.iter().all(|key| interest.has_subject(key)));
        assert!(old_doors
            .difference(&new_doors)
            .all(|key| !interest.has_subject(key)));
        assert!(interest.has_subject(&format!("dungeon:{NIGHTLY}:0:0")));
    }

    // The new maze is live: floor 1 spawns from its slots.
    let arrival = cell_center(&entrance, 1, layouts[0].up_shaft.exit_cell());
    game.handle_player_floor_change(&id, 0, -1, &arrival, &arrival)
        .await;
    assert_eq!(
        game.dungeons.read().await[NIGHTLY].floors[&1].slots.len(),
        layouts[0].spawns.len()
    );
}
