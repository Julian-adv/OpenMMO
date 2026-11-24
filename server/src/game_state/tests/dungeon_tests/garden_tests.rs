use super::*;

const MEDUSA_FLOOR: u8 = 30;

#[tokio::test]
async fn the_garden_levels_from_twenty_eight_to_forty_two_and_ends_at_medusa() {
    let game_state = make_test_game_state("stone_garden_floors");
    for (depth, kinds, level) in [
        (1, &["garden_guardian", "garden_digger"][..], 28),
        (8, &["garden_statue"], 32),
        (13, &["garden_serpent"], 34),
        (18, &["garden_excavator"], 37),
        (29, &["garden_handmaiden", "garden_breaker"], 42),
    ] {
        let monsters = floor_monsters(&game_state, "stone_garden", depth).await;
        assert!(!monsters.is_empty(), "floor {depth}");
        for monster in monsters {
            assert!(
                kinds.contains(&monster.monster_type.as_str()),
                "floor {depth}"
            );
            assert_eq!(monster.level_override, Some(level), "floor {depth}");
        }
    }
    let last = floor_monsters(&game_state, "stone_garden", MEDUSA_FLOOR).await;
    assert_eq!(last.len(), 1);
    assert_eq!(last[0].monster_type, "medusa");
    assert_eq!(last[0].level_override, Some(45));
    assert_eq!(last[0].max_health, 25_000);
}

/// In her hall the gaze reaches whoever stands in the open and spares whoever
/// put a pillar between.
#[tokio::test(start_paused = true)]
async fn a_pillar_of_the_hall_breaks_medusas_gaze() {
    let game_state = make_test_game_state("stone_garden_pillar");
    let medusa = floor_monsters(&game_state, "stone_garden", MEDUSA_FLOOR).await[0]
        .id
        .clone();
    let entrance = game_state.dungeon_defs.get("stone_garden").unwrap().clone();
    let (cx, cz) = {
        let dungeons = game_state.dungeons.read().await;
        let hall = *dungeons[&entrance.id].layouts[MEDUSA_FLOOR as usize - 1]
            .rooms
            .last()
            .unwrap();
        hall.center()
    };
    // As at boot, so the rock blocks what monsters see.
    {
        let dungeons = game_state.dungeons.read().await;
        game_state.passability_write().insert(
            onlinerpg_shared::dungeon::dungeon_cache_key(&entrance.id),
            onlinerpg_shared::dungeon::dungeon_passability(
                &entrance.position(),
                &dungeons[&entrance.id].layouts,
            ),
        );
    }
    let at = |dx: i32, dz: i32| cell_center(&entrance.position(), MEDUSA_FLOOR, (cx + dx, cz + dz));
    // A pillar fills cells 8..=9 east and 3..=4 south of the centre.
    game_state
        .monsters
        .write()
        .await
        .get_mut(&medusa)
        .unwrap()
        .position = at(2, 3);
    for (name, spot) in [("hidden", at(10, 3)), ("open", at(10, 1))] {
        let mut player = make_player(name, spot.x, spot.z);
        player.position = spot;
        player.floor_level = -(MEDUSA_FLOOR as i8);
        player.health = 600;
        player.max_health = 600;
        game_state.add_player(player).await;
        game_state
            .register_player_character(&pid(name), 1, 0, attrs_with_cha(10), 0, Some(100))
            .await;
    }
    let east = std::f32::consts::FRAC_PI_2;
    game_state
        .monster_skill_strike(
            &medusa,
            &onlinerpg_shared::monster_ai::STONE_GAZE,
            at(2, 3),
            east,
            vec![],
        )
        .await;
    let players = game_state.players.read().await;
    assert!(!players[&pid("hidden")].petrified);
    assert!(players[&pid("open")].petrified);
    assert_eq!(
        players[&pid("open")].health,
        600,
        "the gaze itself costs nothing"
    );
}

#[tokio::test]
async fn the_gardens_chest_rolls_the_ring_of_teleport_control() {
    let game_state = make_test_game_state("stone_garden_table");
    let chances = chest_chances(&game_state, "stone_garden");
    assert_eq!(chances["ring_of_teleport_control"], 0.05);
    assert_eq!(chances["ring_of_protection"], 0.2);
    assert!(
        !chest_chances(&game_state, "sundered_bastion").contains_key("ring_of_teleport_control")
    );
    let ring = game_state
        .item_defs
        .get("ring_of_teleport_control")
        .unwrap();
    assert!(ring
        .effects()
        .contains(&crate::item_defs::ItemEffect::TeleportControl));
}

/// Medusa with one player 6m in front of her, wearing `gear` and turned to
/// `rotation`. Returns the game, her id and the player's direct channel.
async fn gazed_at(tag: &str, gear: &[&str], rotation: f32) -> (GameState, String, DirectRx) {
    let game_state = make_flat_world_game_state(tag);
    let mut player = make_player("Perseus", 0.0, 6.0);
    player.health = 600;
    player.max_health = 600;
    player.rotation = rotation;
    let id = player.id;
    game_state.add_player(player).await;
    game_state
        .register_player_character(&id, 1, 0, attrs_with_cha(10), 0, Some(100))
        .await;
    let bag = gear
        .iter()
        .enumerate()
        .map(|(i, item)| bag_item(100 + i as u64, item, 1))
        .collect();
    game_state.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag,
            ..Default::default()
        },
    );
    for i in 0..gear.len() {
        game_state.equip_item(&id, 100 + i as u64).await;
    }
    let medusa = game_state
        .spawn_monster(
            "medusa".into(),
            Position {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .unwrap()
        .id;
    let rx = game_state.register_direct_channel(&id).await;
    (game_state, medusa, rx)
}

async fn gaze(game_state: &GameState, medusa: &str) {
    let origin = Position {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    game_state
        .monster_skill_strike(
            medusa,
            &onlinerpg_shared::monster_ai::STONE_GAZE,
            origin,
            0.0,
            vec![],
        )
        .await;
}

/// Her tail lashes north from (0, 2), over the player at (0, 6).
async fn tail(game_state: &GameState, medusa: &str) {
    let origin = Position {
        x: 0.0,
        y: 0.0,
        z: 2.0,
    };
    game_state
        .monsters
        .write()
        .await
        .get_mut(medusa)
        .unwrap()
        .position = origin;
    game_state
        .monster_skill_strike(
            medusa,
            &onlinerpg_shared::monster_ai::TAIL_SWEEP,
            origin,
            0.0,
            vec![],
        )
        .await;
}

/// South, toward Medusa at the origin from (0, 6).
const FACING_HER: f32 = std::f32::consts::PI;

#[tokio::test(start_paused = true)]
async fn a_mirror_shield_turned_to_medusa_throws_a_third_of_the_gaze_back() {
    let (game_state, medusa, mut rx) =
        gazed_at("gaze_mirror", &["mirror_shield"], FACING_HER).await;
    gaze(&game_state, &medusa).await;
    let reflected = drain(&mut rx)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::MonsterSkillReflected { damage, .. } => Some(damage),
            _ => None,
        })
        .expect("the mirror answers");
    assert!((101..=166).contains(&reflected), "{reflected}");
    assert_eq!(game_state.players.read().await[&pid("Perseus")].health, 600);
    assert_eq!(
        game_state.monsters.read().await[&medusa].health,
        25_000 - reflected
    );
}

#[tokio::test(start_paused = true)]
async fn a_mirror_shield_turned_away_does_nothing() {
    for rotation in [0.0, std::f32::consts::FRAC_PI_2, FACING_HER + 1.0] {
        let (game_state, medusa, mut rx) =
            gazed_at("gaze_mirror_away", &["mirror_shield"], rotation).await;
        gaze(&game_state, &medusa).await;
        assert!(drain(&mut rx)
            .iter()
            .all(|m| !matches!(m, ServerMessage::MonsterSkillReflected { .. })));
        assert!(game_state.is_petrified(&pid("Perseus")).await, "{rotation}");
        assert_eq!(game_state.monsters.read().await[&medusa].health, 25_000);
    }
}

#[tokio::test(start_paused = true)]
async fn the_mirror_never_lands_the_killing_blow() {
    let (game_state, medusa, _rx) =
        gazed_at("gaze_mirror_kill", &["mirror_shield"], FACING_HER).await;
    game_state
        .monsters
        .write()
        .await
        .get_mut(&medusa)
        .unwrap()
        .health = 40;
    gaze(&game_state, &medusa).await;
    let monsters = game_state.monsters.read().await;
    assert_eq!(monsters[&medusa].health, 1);
    assert_ne!(monsters[&medusa].state, MonsterState::Dead);
}

#[tokio::test(start_paused = true)]
async fn a_blindfold_spares_its_wearer_the_gaze_and_costs_ten_to_hit() {
    let (game_state, medusa, _rx) = gazed_at("gaze_blindfold", &["blindfold"], 0.0).await;
    gaze(&game_state, &medusa).await;
    let id = pid("Perseus");
    assert_eq!(game_state.players.read().await[&id].health, 600);
    assert_eq!(game_state.equipped_weapon(&id).await.hit_mod, -10);
    game_state.unequip_item(&id, EquipSlot::Head).await;
    assert_eq!(game_state.equipped_weapon(&id).await.hit_mod, 0);
}

/// A blindfolded archer reaches no further than a sword does until floating
/// eye meat teaches them to see without eyes.
#[tokio::test]
async fn a_blindfold_shortens_a_bow_to_arms_reach_until_telepathy() {
    let game_state = make_flat_world_game_state("blindfold_bow");
    let (id, _rx) = add_hungry_player(&game_state, "Archer", 100).await;
    game_state
        .register_player_skills(&id, Default::default())
        .await;
    let at = game_state.players.read().await[&id].position;
    let mut target = make_monster(
        "eye",
        Position {
            x: at.x + 8.0,
            ..at
        },
        0,
    );
    target.health = 1000;
    target.max_health = 1000;
    game_state
        .monsters
        .write()
        .await
        .insert("eye".into(), target);
    game_state.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![
                bag_item(1, "bow", 1),
                bag_item(2, "blindfold", 1),
                bag_item(3, "iron_arrow", 100),
                bag_item(4, "floating_eye_meat", 1),
            ],
            ..Default::default()
        },
    );
    game_state.equip_item(&id, 1).await;
    let shoot = || game_state.player_attack(&id, "eye".into(), None);
    assert!(shoot().await, "a bow reaches 8m");

    game_state.equip_item(&id, 2).await;
    assert!(game_state.equipped_weapon(&id).await.covers_eyes);
    assert!(!shoot().await, "blind, it reaches no further than a sword");

    game_state.use_item(&id, 4).await;
    assert!(
        game_state
            .has_skill(&id, onlinerpg_shared::skills::SkillId::Telepathy)
            .await
    );
    assert!(shoot().await, "telepathy finds the target again");
    assert_eq!(game_state.equipped_weapon(&id).await.hit_mod, -10);
}

async fn eyes_met_walking_at(tag: &str, x: f32, z: f32) -> usize {
    let game_state = make_flat_world_game_state(tag);
    let id = pid("walker");
    game_state.add_player(make_player("walker", x, z)).await;
    game_state.enable_ambient_spawns();
    let mut eyes = 0;
    for _ in 0..40 {
        pace_player(&game_state, &id, x, z, 10).await;
        let ids: Vec<(String, String)> = game_state
            .monsters
            .read()
            .await
            .values()
            .map(|m| {
                if m.monster_type == "floating_eye" {
                    assert_eq!(m.level_override, None, "an eye stays level 10");
                    assert!(!m.aggressive);
                }
                (m.id.clone(), m.monster_type.clone())
            })
            .collect();
        eyes += ids.iter().filter(|(_, ty)| ty == "floating_eye").count();
        // Clear the field so the nearby cap never ends the walk.
        game_state
            .despawn_monsters(ids.into_iter().map(|(id, _)| id).collect())
            .await;
    }
    eyes
}

#[tokio::test]
async fn floating_eyes_drift_only_around_the_garden() {
    assert!(eyes_met_walking_at("eyes_near", -1836.0, 6950.0).await > 0);
    assert_eq!(eyes_met_walking_at("eyes_far", -1836.0, 7400.0).await, 0);
}

/// Long enough for a blow to land and the eye to blink.
const STARE_LANDS: std::time::Duration = std::time::Duration::from_secs(2);

async fn eye_beside_striker(tag: &str) -> (GameState, PlayerId, DirectRx, crate::types::Monster) {
    let game_state = make_flat_world_game_state(tag);
    let (id, rx) = add_hungry_player(&game_state, "Striker", 100).await;
    let at = game_state.players.read().await[&id].position;
    let eye = game_state
        .spawn_monster(
            "floating_eye".into(),
            Position {
                x: at.x + 1.0,
                ..at
            },
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            false,
        )
        .await
        .unwrap();
    (game_state, id, rx, eye)
}

/// A floating eye struck by hand blinks, then holds whoever looked at it; a
/// blindfold or an arrow avoids the stare.
#[tokio::test(start_paused = true)]
async fn striking_a_floating_eye_by_hand_holds_the_striker_unless_blindfolded() {
    let (game_state, id, mut rx, eye) = eye_beside_striker("eye_stare").await;
    game_state.inventories.write().await.insert(
        id,
        PlayerInventory {
            bag: vec![bag_item(1, "blindfold", 1)],
            ..Default::default()
        },
    );
    let held = || async { game_state.players.read().await[&id].petrified };
    let strike_until_hit = || async {
        for _ in 0..200 {
            if game_state.player_attack(&id, eye.id.clone(), None).await
                && game_state.monsters.read().await[&eye.id].health < 60
            {
                tokio::time::sleep(STARE_LANDS).await;
                return;
            }
            game_state.last_player_attacks.write().await.remove(&id);
        }
    };
    let mut blinked = || {
        std::iter::from_fn(|| rx.try_recv().ok())
            .any(|msg| matches!(msg, ServerMessage::MonsterStared { .. }))
    };
    game_state.equip_item(&id, 1).await;
    strike_until_hit().await;
    assert!(!held().await, "covered eyes meet no stare");
    assert!(!blinked());

    game_state.unequip_item(&id, EquipSlot::Head).await;
    game_state
        .monsters
        .write()
        .await
        .get_mut(&eye.id)
        .unwrap()
        .health = 60;
    strike_until_hit().await;
    assert!(held().await, "open eyes are held");
    assert!(blinked(), "the eye blinks first");

    let health = || async { game_state.players.read().await[&id].health };
    let before = health().await;
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    game_state.tick_debuffs().await;
    assert!(!held().await);
    let hurt = before - health().await;
    assert!((2..=16).contains(&hurt), "the hold hurts 2d8 as it lets go");
}

/// The stare lands while the striker's auto-attack runs: it ends the attack
/// and neither it nor the logout after it may hang on the attack session.
#[tokio::test(start_paused = true)]
async fn an_auto_attack_on_a_floating_eye_ends_when_the_stare_holds_the_striker() {
    let (game_state, id, _rx, eye) = eye_beside_striker("eye_stare_auto").await;
    let strikes = async {
        for request in 0..200 {
            game_state
                .start_player_attack(id, eye.id.clone(), false, request, None)
                .await;
            tokio::time::sleep(STARE_LANDS).await;
            if game_state.is_petrified(&id).await {
                break;
            }
            game_state.stop_player_attack(&id).await;
            game_state.last_player_attacks.write().await.remove(&id);
        }
        assert!(game_state.is_petrified(&id).await);
        assert!(!game_state.auto_attacks.read().await.contains_key(&id));
        game_state.remove_player(&id).await;
    };
    tokio::time::timeout(std::time::Duration::from_secs(60), strikes)
        .await
        .expect("the swing or the logout after it never returned");
}

/// The gaze turns to stone for 30 s; left unbroken, the stone costs half a
/// break as it lets go.
#[tokio::test(start_paused = true)]
async fn the_gaze_turns_to_stone_and_lets_go_for_half_a_break() {
    let (game_state, medusa, _rx) = gazed_at("gaze_stone", &[], 0.0).await;
    let id = pid("Perseus");
    gaze(&game_state, &medusa).await;
    assert!(game_state.is_petrified(&id).await);
    tokio::time::sleep(std::time::Duration::from_secs(29)).await;
    game_state.tick_debuffs().await;
    assert!(game_state.is_petrified(&id).await);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    game_state.tick_debuffs().await;
    assert!(!game_state.is_petrified(&id).await);
    let lost = 600 - game_state.players.read().await[&id].health;
    assert!((155..=250).contains(&lost), "letting go took {lost}");
}

/// Her tail frees a statue and lands 5d40+300 on it; flesh takes the plain
/// lash.
#[tokio::test(start_paused = true)]
async fn the_tail_frees_a_statue_and_lands_the_break_roll() {
    let (game_state, medusa, _rx) = gazed_at("gaze_break", &[], 0.0).await;
    let id = pid("Perseus");
    gaze(&game_state, &medusa).await;
    tail(&game_state, &medusa).await;
    assert!(!game_state.is_petrified(&id).await);
    let lost = 600 - game_state.players.read().await[&id].health;
    assert!((305..=500).contains(&lost), "the break took {lost}");

    game_state
        .players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .health = 600;
    tail(&game_state, &medusa).await;
    let lost = 600 - game_state.players.read().await[&id].health;
    assert!(lost < 305, "flesh took {lost}");
}

/// Whoever the tail breaks to death stays standing as stone until they rise
/// again.
#[tokio::test(start_paused = true)]
async fn a_statue_broken_to_death_stays_standing_as_stone() {
    let (game_state, medusa, mut rx) = gazed_at("gaze_statue", &[], 0.0).await;
    let id = pid("Perseus");
    game_state
        .players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .health = 200;
    gaze(&game_state, &medusa).await;
    tail(&game_state, &medusa).await;
    {
        let players = game_state.players.read().await;
        assert_eq!(players[&id].health, 0);
        assert!(players[&id].petrified);
    }
    let messages = drain(&mut rx);
    let stone = messages.iter().rposition(|m| {
        matches!(
            m,
            ServerMessage::PlayerPetrifiedToggled {
                petrified: true,
                ..
            }
        )
    });
    let dead = messages
        .iter()
        .position(|m| matches!(m, ServerMessage::PlayerDead { .. }));
    assert!(
        stone.unwrap() < dead.unwrap(),
        "stone before the body falls"
    );

    game_state.respawn_player(&id).await;
    let players = game_state.players.read().await;
    assert!(players[&id].health > 0);
    assert!(!players[&id].petrified);
}

/// Swinging at Medusa is facing her: the mirror answers without the client
/// reporting a turn.
#[tokio::test(start_paused = true)]
async fn a_swing_at_medusa_turns_the_mirror_to_her() {
    let (game_state, medusa, mut rx) = gazed_at("gaze_mirror_swing", &["mirror_shield"], 0.0).await;
    let id = pid("Perseus");
    game_state
        .players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .position
        .z = 1.5;
    assert!(game_state.player_attack(&id, medusa.clone(), None).await);
    drain(&mut rx);
    gaze(&game_state, &medusa).await;
    assert!(drain(&mut rx)
        .iter()
        .any(|m| matches!(m, ServerMessage::MonsterSkillReflected { .. })));
}

fn statues_in(messages: Vec<ServerMessage>) -> Vec<onlinerpg_shared::messages::StatueView> {
    messages
        .into_iter()
        .rev()
        .find_map(|m| match m {
            ServerMessage::DungeonStatues { statues, .. } => Some(statues),
            _ => None,
        })
        .expect("the floor's statues")
}

/// A player on `depth` of the Garden, standing `dz` cells south of the stairs.
async fn garden_delver(game_state: &GameState, name: &str, depth: u8, dz: f32) -> PlayerId {
    let entrance = game_state.dungeon_defs.get("stone_garden").unwrap();
    let mut player = make_player(name, entrance.x, entrance.z + dz);
    player.floor_level = -(depth as i8);
    player.position.y = onlinerpg_shared::dungeon::floor_world_y(entrance.y, depth);
    let id = player.id;
    game_state.add_player(player).await;
    id
}

/// The gaze's dead stand as statues where they fell for the night, among the
/// nameless excavators; by another night the maze has moved and they take one
/// of the floor's statue cells.
#[tokio::test(start_paused = true)]
async fn statues_broken_to_death_join_the_floors_statues() {
    let game_state = make_test_game_state("garden_statues");
    let medusa = floor_monsters(&game_state, "stone_garden", MEDUSA_FLOOR).await[0]
        .id
        .clone();
    let watcher = pid("delver30");
    let mut rx = game_state.register_direct_channel(&watcher).await;
    game_state
        .send_floor_statues(&watcher, "stone_garden", MEDUSA_FLOOR)
        .await;
    let before = statues_in(drain(&mut rx));
    assert_eq!(before.len(), 12);
    assert!(before.iter().all(|s| s.name.is_none() && s.look.petrified));

    let entrance = game_state.dungeon_defs.get("stone_garden").unwrap().clone();
    let (cx, cz) = {
        let dungeons = game_state.dungeons.read().await;
        dungeons[&entrance.id].layouts[MEDUSA_FLOOR as usize - 1]
            .rooms
            .last()
            .unwrap()
            .center()
    };
    let at = |dx: i32, dz: i32| cell_center(&entrance.position(), MEDUSA_FLOOR, (cx + dx, cz + dz));
    game_state
        .monsters
        .write()
        .await
        .get_mut(&medusa)
        .unwrap()
        .position = at(2, 1);
    let mut victim = make_player("Victim", 0.0, 0.0);
    victim.position = at(6, 1);
    victim.floor_level = -(MEDUSA_FLOOR as i8);
    victim.health = 200;
    victim.max_health = 200;
    game_state.add_player(victim).await;
    game_state
        .register_player_character(&pid("Victim"), 1, 0, attrs_with_cha(10), 0, Some(100))
        .await;
    let east = std::f32::consts::FRAC_PI_2;
    for skill in [
        &onlinerpg_shared::monster_ai::STONE_GAZE,
        &onlinerpg_shared::monster_ai::TAIL_SWEEP,
    ] {
        game_state
            .monster_skill_strike(&medusa, skill, at(2, 1), east, vec![])
            .await;
    }

    let tonight = statues_in(drain(&mut rx));
    assert_eq!(tonight.len(), 12);
    let stone = tonight
        .iter()
        .find(|s| s.name.as_deref() == Some("Victim"))
        .expect("the victim's statue");
    assert_eq!(stone.look.position, at(6, 1));
    assert!(stone.look.petrified && stone.look.health > 0);
    assert!(stone.year > 0);
    assert_eq!(tonight.iter().filter(|s| s.name.is_none()).count(), 11);

    let mut later = game_state.current_game_datetime();
    later.day += 2;
    game_state.debug_set_datetime(&later);
    game_state
        .send_floor_statues(&watcher, "stone_garden", MEDUSA_FLOOR)
        .await;
    let moved = statues_in(drain(&mut rx));
    assert_eq!(moved.len(), 12);
    let stone = moved
        .iter()
        .find(|s| s.name.as_deref() == Some("Victim"))
        .unwrap();
    assert_ne!(stone.look.position, at(6, 1));
    let distinct: HashSet<_> = moved
        .iter()
        .map(|s| (s.look.position.x.to_bits(), s.look.position.z.to_bits()))
        .collect();
    assert_eq!(distinct.len(), 12, "one statue to a cell");
}

/// A floor keeps its four newest statues; they survive a restart, and one
/// whose character is gone comes back nameless.
#[tokio::test]
async fn a_floor_keeps_its_newest_statues_across_a_restart() {
    let game_state = make_test_game_state("garden_statue_cap");
    let auth = make_test_auth("garden_statue_cap");
    let account = auth.login_google("Mason").unwrap();
    let character = create_test_character(&auth, &account, "Mason");
    game_state.ensure_dungeon_runtime("stone_garden").await;
    for (i, name) in ["Ann", "Bo", "Cy", "Di", "Mason"].into_iter().enumerate() {
        let id = garden_delver(&game_state, name, 1, i as f32).await;
        if name == "Mason" {
            game_state
                .register_player_character(&id, character.id, 0, attrs_with_cha(10), 0, None)
                .await;
        }
        game_state.keep_statue_of(&id).await;
    }
    game_state.flush_statues(&auth).await;
    let rows = auth.statues().unwrap();
    assert_eq!(rows.len(), 4, "the oldest made way");
    let names: Vec<_> = rows.iter().map(|r| r.name.as_deref()).collect();
    assert_eq!(names, [None, None, None, Some("Mason")]);

    let restarted = make_test_game_state("garden_statue_cap_restart");
    restarted.load_statues(&rows);
    restarted.ensure_dungeon_runtime("stone_garden").await;
    let watcher = garden_delver(&restarted, "Watcher", 1, 0.0).await;
    let mut rx = restarted.register_direct_channel(&watcher).await;
    restarted
        .send_floor_statues(&watcher, "stone_garden", 1)
        .await;
    let statues = statues_in(drain(&mut rx));
    assert_eq!(statues.len(), 4);
    assert_eq!(
        statues
            .iter()
            .filter(|s| s.name.as_deref() == Some("Mason"))
            .count(),
        1
    );
    assert!(
        statues.iter().all(|s| s.id > 0),
        "no nameless fill-ins needed"
    );

    // Another dungeon keeps none.
    restarted.ensure_dungeon_runtime("old_crypt").await;
    restarted.send_floor_statues(&watcher, "old_crypt", 1).await;
    assert!(drain(&mut rx).is_empty());
}

async fn spawn_statue(game_state: &GameState, at: Position, dx: f32) -> String {
    game_state
        .spawn_monster(
            "garden_statue".into(),
            Position { x: at.x + dx, ..at },
            0.0,
            0,
            MonsterLifecycle::Ambient,
            None,
            true,
        )
        .await
        .unwrap()
        .id
}

/// A waking statue stands as stone until a player comes within three metres,
/// then wakes on them; one struck from afar wakes too.
#[tokio::test]
async fn a_waking_statue_wakes_when_approached_or_struck() {
    let game_state = make_flat_world_game_state("statue_ambush");
    let (id, mut rx) = add_hungry_player(&game_state, "Passerby", 100).await;
    let at = game_state.players.read().await[&id].position;
    let near = spawn_statue(&game_state, at, 6.0).await;
    let far = spawn_statue(&game_state, at, -10.0).await;
    let dormant = |statue: &str| {
        let statue = statue.to_owned();
        let game_state = game_state.clone();
        async move { game_state.monsters.read().await[&statue].dormant }
    };
    let start = game_state.monsters.read().await[&near].position;
    for _ in 0..10 {
        game_state.tick_monster_ai_by(200.0).await;
    }
    assert!(dormant(&near).await && dormant(&far).await);
    assert_eq!(game_state.monsters.read().await[&near].position, start);

    game_state
        .players
        .write()
        .await
        .get_mut(&id)
        .unwrap()
        .position
        .x = at.x + 4.0;
    game_state.tick_monster_ai_by(200.0).await;
    assert!(!dormant(&near).await);
    assert!(std::iter::from_fn(|| rx.try_recv().ok())
        .any(|msg| matches!(msg, ServerMessage::MonsterDormant { dormant: false, .. })));

    game_state.brain_hit(&far, &id, true, 1).await;
    assert!(!dormant(&far).await);
}
