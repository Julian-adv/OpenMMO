use super::*;

#[test]
fn monster_state_and_sightings_use_display_names() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    for (id, monster_type, name) in [
        ("m21", "scp939", "Stalker"),
        ("m22", "unknown_monster", "unknown_monster"),
    ] {
        let mut m = monster(id);
        m.monster_type = monster_type.into();
        m.aggressive = true;
        s.nearby_monsters.insert(id.into(), m);
        let world = s.format_world_state();
        assert!(
            world.contains(&format!("Monster: {name} [{id}]")),
            "{world}"
        );
    }
    s.check_sightings();
    assert!(s
        .agent_events
        .iter()
        .any(|line| line.starts_with("[Sighted] Stalker [m21]")));
    assert!(!s.format_world_state().contains("scp939"));
    assert!(s.agent_events.iter().all(|line| !line.contains("scp939")));
    assert_eq!(s.nearby_monsters["m21"].monster_type, "scp939");
}

#[test]
fn fence_visibility_is_shared_without_one_npc_removing_anothers_collision() {
    use onlinerpg_shared::fence::{Fence, FenceAxis, FenceEdge};
    use onlinerpg_shared::pathfinding::is_movement_blocked;
    let mut world = WorldCache::new();
    let edge = FenceEdge {
        x: 2,
        z: 1,
        axis: FenceAxis::Z,
    };
    let fence = Fence {
        edge,
        y: 0.0,
        owner_id: 1,
    };
    world.update_fences(1.into(), std::slice::from_ref(&fence), &[]);
    world.update_fences(2.into(), &[fence], &[]);
    world.update_fences(1.into(), &[], &[edge]);
    assert!(is_movement_blocked(
        world.passability_cache(),
        1.5,
        1.5,
        2.5,
        1.5,
        0,
        Some(0.05)
    ));
    world.remove_fence_view(2.into());
    assert!(!is_movement_blocked(
        world.passability_cache(),
        1.5,
        1.5,
        2.5,
        1.5,
        0,
        Some(0.05)
    ));
}

/// Table service can select an open standing spot beside furniture.
#[test]
fn walkable_near_steps_off_sealed_furniture() {
    let (s, _rx) = test_state();
    let bed = onlinerpg_shared::furniture::FurniturePlacement {
        id: 0,
        type_id: "bed".to_string(),
        x: 10.5,
        y: 0.0,
        z: 10.5,
        rotation_deg: 0.0,
        floor_level: 0,
    };
    s.world_cache
        .write()
        .unwrap()
        .sync_furniture(0, 0, vec![bed]);

    let (x, z) = s.walkable_near(10.5, 10.5, 0);
    assert_ne!((x, z), (10.5, 10.5), "the on-bed goal must be nudged");
    assert!((x - 10.5).abs() <= 1.5 && (z - 10.5).abs() <= 1.5);
    let world = s.world_cache.read().unwrap();
    assert!(!pathfinding::is_cell_sealed(
        world.passability_cache(),
        x,
        z,
        0,
        None
    ));
    drop(world);

    assert_eq!(s.walkable_near(0.5, 0.5, 0), (0.5, 0.5));
}

/// Our own performances land in the recent-song list, oldest first and
/// capped; the world state shows the list only to an agent that busks,
/// and never counts someone else's tune as ours.
#[test]
fn recent_songs_render_for_the_busker_only() {
    let (mut s, _rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    s.plays_music = true;

    s.push_event(ServerMessage::PlayerMusicStarted {
        player_id: PlayerId::from(2),
        track: "Someone Else's Tune".to_string(),
        elapsed_secs: 0.0,
    });
    for i in 0..10 {
        s.push_event(ServerMessage::PlayerMusicStarted {
            player_id: PlayerId::from(1),
            track: format!("Song {i}"),
            elapsed_secs: 0.0,
        });
    }

    let world = s.format_world_state();
    assert!(
        world.contains("Songs you played recently, oldest first: Song 2,"),
        "capped at MAX_RECENT_SONGS, oldest dropped: {world}"
    );
    assert!(world.contains("Song 9"), "{world}");
    assert!(!world.contains("Someone Else's Tune"), "{world}");

    s.plays_music = false;
    assert!(!s.format_world_state().contains("Songs you played recently"));
}

/// The world state lists reachable ground items closest first, and
/// leaves out other floors and anything out of sight.
#[test]
fn world_state_lists_nearby_ground_items() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    for item in [
        ground_item(1, "small_sword", 5.0, 0.0, 0),
        ground_item(2, "wooden_shield", 2.0, 0.0, 0),
        ground_item(3, "coin_pile", 1.0, 0.0, 0),
        ground_item(4, "iron_sword", 0.0, EVENT_DELIVERY_RADIUS + 5.0, 0),
        ground_item(5, "healing_potion", 3.0, 0.0, 1),
    ] {
        s.remember_ground_item(item);
    }

    let lines: Vec<String> = s
        .format_world_state()
        .lines()
        .filter(|l| l.starts_with("Item on ground:"))
        .map(str::to_string)
        .collect();

    assert_eq!(
        lines,
        vec![
            "Item on ground: coin_pile (1.0m away) [id 3]",
            "Item on ground: wooden_shield (2.0m away) [id 2]",
            "Item on ground: small_sword (5.0m away) [id 1]",
            "Item on ground: iron_sword (37.0m away) [id 4]",
        ]
    );
}

/// Another player's claimed loot is marked as theirs until the server says
/// the claim ended; our own claim is no obstacle.
#[test]
fn claimed_loot_is_marked_until_the_claim_ends() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.self_player_id = Some(PlayerId::from(1));
    for (id, def, x, owner) in [(7, "crypt_key_5", 1.0, 2), (8, "small_sword", 2.0, 1)] {
        let mut item = ground_item(id, def, x, 0.0, 0);
        item.claimed_by = Some(PlayerId::from(owner));
        s.remember_ground_item(item);
    }

    let world = s.format_world_state();
    assert!(
        world.contains("[id 7] — Someone's loot for now, leave it"),
        "{world}"
    );
    assert!(
        world
            .lines()
            .any(|l| l.ends_with("small_sword (2.0m away) [id 8]")),
        "{world}"
    );
    assert_eq!(s.loot_claimant(s.ground_item(8).unwrap()), None);

    s.push_event(ServerMessage::GroundItemClaimEnded { instance_id: 7 });
    assert_eq!(s.loot_claimant(s.ground_item(7).unwrap()), None);
    assert!(s
        .format_world_state()
        .contains("[id 7] — DUNGEON KEY, pick it up first"));
}

/// An announced item is loot the agent may go for right away — the
/// server does any withholding.
#[test]
fn an_announced_drop_is_actionable_at_once() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));

    s.push_event(ServerMessage::GroundItemSpawned {
        item: ground_item(1, "goblin_sword", 2.0, 0.0, 0),
    });
    s.push_event(ServerMessage::GroundItemAppeared {
        item: ground_item(2, "small_sword", 3.0, 0.0, 0),
    });

    let ids: Vec<u64> = s
        .ground_items_in_sight()
        .iter()
        .map(|(_, i)| i.instance_id)
        .collect();
    assert_eq!(ids, vec![1, 2]);
    assert!(s.ground_item(1).is_some());
    assert!(s.format_world_state().contains("goblin_sword"));
}

/// A field strewn with drops is summarised, not listed line by line.
#[test]
fn world_state_caps_the_ground_item_list() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    for id in 1..=(MAX_LISTED_GROUND_ITEMS as u64 + 3) {
        let item = ground_item(id, "small_sword", id as f32 * 0.5, 0.0, 0);
        s.remember_ground_item(item);
    }

    let world = s.format_world_state();
    let listed = world
        .lines()
        .filter(|l| l.starts_with("Item on ground:"))
        .count();

    assert_eq!(listed, MAX_LISTED_GROUND_ITEMS);
    assert!(world.contains("(and 3 more items further away)"), "{world}");
}

/// A `DoorToggled` must land on both faces of the door: the passability
/// edge A* walks and the `HouseData` wall the door hunt reads. With only
/// the edge updated, `closed_doors_on_our_floor` kept re-listing a door
/// that was already open and the agent toggled it shut again.
#[test]
fn door_toggle_keeps_house_walls_in_step_with_the_edges() {
    use onlinerpg_shared::housing::{
        PassabilityGrid, RoomType, WallConfig, WallDirection, WallVariant,
    };

    let wall = |variant| WallConfig {
        variant,
        texture: 0,
        is_open: false,
    };
    let room = RoomData {
        size_x: 1,
        size_z: 1,
        wall_north: vec![wall(WallVariant::WithDoor)],
        wall_south: vec![wall(WallVariant::Solid)],
        wall_east: vec![wall(WallVariant::Solid)],
        wall_west: vec![wall(WallVariant::Solid)],
        ..room(0, 0, RoomType::default())
    };
    let mut house = house("h", p(10.0, 0.0, 10.0), vec![room]);
    house.passability = vec![PassabilityGrid {
        floor_level: 0,
        origin_x: 0,
        origin_z: 0,
        width: 1,
        depth: 1,
        // All four edges walled (N=1, E=2, S=4, W=8), door shut.
        cells: vec![1 | 2 | 4 | 8],
    }];

    let mut world = WorldCache::new();
    world.add_house(house);

    let door_blocked = |world: &WorldCache| {
        pathfinding::is_movement_blocked(world.passability_cache(), 10.5, 10.5, 10.5, 9.5, 0, None)
    };
    assert!(door_blocked(&world), "the north door starts shut");

    world.update_door("h", 0, WallDirection::North, 0, true);
    assert!(
        world.houses()["h"].rooms[0].wall_north[0].is_open,
        "HouseData must track the open"
    );
    assert!(!door_blocked(&world), "the edge must open with the door");

    world.update_door("h", 0, WallDirection::North, 0, false);
    assert!(!world.houses()["h"].rooms[0].wall_north[0].is_open);
    assert!(door_blocked(&world), "the edge must seal again");
}

/// Splat tiles that paint one road cell and one river cell near origin,
/// so the summary test can assert distance and bearing against known
/// world coordinates.
struct PaintedSplat;

#[async_trait::async_trait]
impl crate::splat::SplatTiles for PaintedSplat {
    async fn read_splat(&self, tx: i32, tz: i32) -> std::io::Result<Vec<u8>> {
        let mut data = vec![0u8; onlinerpg_terrain::defaults::SPLATMAP_SIZE];
        if (tx, tz) == (0, 0) {
            let mut paint = |wx: f32, wz: f32, pal: u8| {
                let cx = (wx + 32.0).floor() as usize;
                let cz = (wz + 32.0).floor() as usize;
                data[(cz * 64 + cx) * 4] = pal << 4;
            };
            paint(6.0, 0.0, crate::splat::PAL_ROAD);
            paint(-6.0, -6.0, crate::splat::PAL_RIVER_BED);
        }
        Ok(data)
    }
}

#[tokio::test]
async fn terrain_summary_names_nearest_features_with_bearing() {
    let (mut s, _rx) = test_state();
    s.splat_sampler = Arc::new(crate::splat::SplatSampler::new(PaintedSplat));
    s.self_player = Some(test_player(0.0, 0.0));
    let line = s
        .terrain_summary_job()
        .expect("on the surface")
        .render()
        .await;

    assert!(line.starts_with("Terrain within 32m: "), "{line}");
    // Road cell (6, 0): 6m due east. River cell (-6, -6): 8m northwest.
    assert!(line.contains("road 6m east"), "{line}");
    assert!(line.contains("water 8m northwest"), "{line}");
    assert!(!line.contains("open ground"), "{line}");
    assert_eq!(
        line.trim_end().lines().count(),
        1,
        "one line, no map rows: {line}"
    );
}

#[test]
fn terrain_summary_is_absent_underground() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(0.0, 0.0));
    s.self_floor_level = -1;
    assert!(s.terrain_summary_job().is_none());
}

/// Agents can discover every dungeon entrance from above ground.
#[test]
fn world_state_names_the_dungeon_entrances() {
    let (mut s, _rx) = test_state();
    s.self_player = Some(test_player(-1450.0, 4720.0));
    s.world_cache.write().unwrap().register_dungeons();

    let lines: Vec<String> = s
        .format_world_state()
        .lines()
        .filter(|l| l.starts_with("Dungeon:"))
        .map(str::to_string)
        .collect();

    // Include distant entrances, ordered nearest first.
    assert_eq!(lines.len(), 8, "{lines:?}");
    assert!(lines[0].contains("Old Crypt"), "{lines:?}");
    assert!(lines[1].contains("Orc Warrens"), "{lines:?}");
    assert!(lines[2].contains("Ogre Stronghold"), "{lines:?}");
    assert!(lines[3].contains("Skeleton Crypt"), "{lines:?}");
    assert!(lines[4].contains("Sundered Bastion"), "{lines:?}");
    assert!(lines[5].contains("Thieves' Den"), "{lines:?}");
    assert!(lines[6].contains("Shadow Guild"), "{lines:?}");
    assert!(lines[7].contains("Blackvein Mine"), "{lines:?}");
}

/// The prompt names the storey only while we stand inside a room's
/// footprint on our own floor; outdoors and underground say nothing.
#[test]
fn world_state_names_the_storey_when_indoors() {
    use onlinerpg_shared::housing::RoomType;

    let (mut s, _rx) = test_state();
    s.world_cache.write().unwrap().add_house(house(
        "inn",
        p(100.0, 0.0, 100.0),
        vec![
            room(0, 0, RoomType::default()),
            room(0, 1, RoomType::default()),
            room(4, 0, RoomType::Stairwell),
        ],
    ));

    s.self_player = Some(test_player(102.0, 102.0));
    assert!(s
        .format_world_state()
        .contains("You are indoors (ground floor)"));

    s.self_floor_level = 1;
    assert!(s
        .format_world_state()
        .contains("You are indoors (2nd floor)"));

    s.self_player = Some(test_player(106.0, 102.0));
    assert!(
        s.format_world_state()
            .contains("You are indoors (2nd floor)"),
        "a stairwell holds the floor above it"
    );

    s.self_floor_level = 0;
    s.self_player = Some(test_player(120.0, 120.0));
    assert!(!s.format_world_state().contains("indoors"));

    s.self_player = Some(test_player(102.0, 102.0));
    s.self_floor_level = -1;
    assert!(!s.format_world_state().contains("indoors"));
}

/// A coordinate move walks on the storey the LLM names, else the floor we
/// stand on — and is refused up front when no such spot is on that floor.
#[test]
fn a_coordinate_off_the_floor_it_would_walk_on_is_refused_before_any_walk() {
    use onlinerpg_shared::housing::RoomType;
    use onlinerpg_shared::Position;

    let (mut s, _rx) = test_state();
    let inn = house(
        "inn",
        Position {
            x: -1448.0,
            y: 0.0,
            z: 4753.0,
        },
        vec![room(0, 1, RoomType::Normal)],
    );
    s.world_cache.write().unwrap().add_house(inn);
    let mut me = test_player(-1446.7, 4754.9);
    me.floor_level = 1;
    s.self_player = Some(me);
    s.self_floor_level = 1;

    let outside = (-1450.0, 4720.0);
    let refused = s
        .resolve_goal_floor(outside.0, outside.1, None)
        .unwrap_err();
    assert!(refused.contains("not on the 2nd floor"), "{refused}");
    assert_eq!(s.resolve_goal_floor(outside.0, outside.1, Some(0)), Ok(0));
    assert_eq!(
        s.resolve_goal_floor(-1445.5, 4755.5, None),
        Ok(1),
        "same room"
    );
    let refused = s
        .resolve_goal_floor(outside.0, outside.1, Some(1))
        .unwrap_err();
    assert!(refused.contains("no 2nd floor at that spot"), "{refused}");
    assert!(s.resolve_goal_floor(-1445.5, 4755.5, Some(-1)).is_err());

    s.self_floor_level = 0;
    assert_eq!(s.resolve_goal_floor(outside.0, outside.1, None), Ok(0));
}

fn hidden_bob() -> Player {
    let mut bob = test_player(2.0, 0.0);
    bob.id = PlayerId::from(2);
    bob.name = "Bob".into();
    bob.health = 1;
    bob.max_health = 0;
    bob
}

#[test]
fn other_players_hp_shows_only_when_disclosed() {
    let (mut s, _rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    let bob = hidden_bob();
    s.push_event(ServerMessage::PlayerAppeared {
        player: bob.clone(),
    });
    let world = s.format_world_state();
    assert!(world.contains("You: Me Lv.1 Rogue HP 10/10"), "{world}");
    assert!(
        world.contains("Player: Bob Lv.1 alive (HP hidden) at"),
        "{world}"
    );
    assert!(!world.contains("HP 1/0"), "{world}");
    let stethoscope = |health, max_health| ServerMessage::PlayerHealthUpdate {
        player_id: bob.id,
        health,
        max_health,
    };
    s.push_event(stethoscope(7, 20));
    assert!(s
        .format_world_state()
        .contains("Player: Bob Lv.1 HP 7/20 at"));
    s.push_event(stethoscope(1, 0));
    assert!(s
        .format_world_state()
        .contains("Player: Bob Lv.1 alive (HP hidden) at"));
    s.push_event(ServerMessage::PlayerDead { player_id: bob.id });
    assert!(s.format_world_state().contains("Player: Bob Lv.1 dead at"));
    assert_eq!(s.self_player.as_ref().unwrap().health, 10);
}

#[test]
fn party_vitals_keep_member_hp_exact() {
    use onlinerpg_shared::messages::{PartyMember, PartyMemberVitals};
    let (mut s, _rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me.clone());
    let bob = hidden_bob();
    s.push_event(ServerMessage::PlayerAppeared {
        player: bob.clone(),
    });
    let member = |player: &Player, hp| PartyMember {
        id: player.id,
        name: player.name.clone(),
        hp,
        max_hp: 20,
        class: player.class.clone(),
        family: None,
    };
    s.push_event(ServerMessage::PartyState {
        leader_id: me.id,
        members: vec![member(&me, 10), member(&bob, 20)],
        loot_shared: false,
    });
    let vitals = ServerMessage::PartyVitals {
        members: vec![PartyMemberVitals {
            id: bob.id,
            hp: 6,
            max_hp: 20,
        }],
    };
    s.push_event(vitals);
    let world = s.format_world_state();
    assert!(
        world.contains("Your party: Me (leader) HP 10/20, Bob HP 6/20"),
        "{world}"
    );
    assert!(world.contains("Player: Bob Lv.1 HP 6/20 at"), "{world}");
}

#[test]
fn self_defense_targets_the_nearest_monster_that_struck_within_reach() {
    let (mut s, _rx) = test_state();
    let mut me = test_player(0.0, 0.0);
    me.name = "Ysolde".into();
    me.is_official_npc = true;
    s.self_player_id = Some(me.id);
    s.self_player = Some(me.clone());
    for (id, x) in [("m1", 1.5), ("m2", 1.0), ("m3", 6.0), ("m4", 1.2)] {
        let mut m = monster(id);
        m.position = p(x, 0.0, 0.0);
        s.nearby_monsters.insert(id.into(), m);
    }
    for id in ["m1", "m3", "m4"] {
        s.push_event(ServerMessage::MonsterAttackedPlayer {
            monster_id: id.into(),
            player_id: me.id,
            hit: true,
            roll: 15,
            damage: 1,
            current_health: me.health,
            ranged: false,
        });
    }
    assert_eq!(s.self_defense_target().as_deref(), Some("m4"));
    s.nearby_monsters.get_mut("m4").unwrap().health = 0;
    assert_eq!(s.self_defense_target().as_deref(), Some("m1"));

    s.self_player.as_mut().unwrap().name = "Rica".into();
    assert_eq!(s.self_defense_target(), None);
}
