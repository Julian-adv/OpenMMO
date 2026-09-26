use super::*;

fn targetable_state() -> (SharedState, mpsc::Receiver<ClientMessage>) {
    let (mut s, rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);

    let mut karl = test_player(3.0, 0.0);
    karl.id = PlayerId::from(2);
    karl.name = "Karl".to_string();
    s.nearby_players.insert(karl.id, karl);

    let mut near = monster("m21");
    near.monster_type = "goblin".to_string();
    near.position = p(5.0, 0.0, 0.0);
    let mut far = monster("m27");
    far.monster_type = "goblin".to_string();
    far.position = p(12.0, 0.0, 0.0);
    let mut other = monster("m29");
    other.monster_type = "slime".to_string();
    other.position = p(4.0, 0.0, 0.0);
    for m in [near, far, other] {
        s.nearby_monsters.insert(m.id.clone(), m);
    }

    s.remember_ground_item(ground_item(6043, "torch", 2.0, 0.0, 0));
    // Named after a species on purpose: "goblin" must still resolve to the
    // monsters, not to this sword.
    s.remember_ground_item(ground_item(6044, "goblin_sword", 2.5, 0.0, 0));
    s.remember_ground_item(ground_item(6045, "iron_sword", 3.0, 0.0, 0));
    (s, rx)
}

/// The shape of the string picks the namespace before any name lookup
/// runs, so an id and a name never compete.
#[test]
fn a_move_target_resolves_by_shape_then_by_name() {
    let (s, _rx) = targetable_state();

    assert_eq!(
        s.resolve_move_target("m21"),
        Ok(MoveTarget::Monster {
            id: "m21".to_string()
        })
    );
    assert_eq!(
        s.resolve_move_target("6043"),
        Ok(MoveTarget::GroundItem {
            instance_id: 6043,
            name: "torch".to_string()
        })
    );
    assert_eq!(
        s.resolve_move_target("karl"),
        Ok(MoveTarget::Character {
            id: PlayerId::from(2),
            name: "Karl".to_string()
        })
    );
    assert_eq!(
        s.resolve_move_target("torch"),
        Ok(MoveTarget::GroundItem {
            instance_id: 6043,
            name: "torch".to_string()
        })
    );
}

/// A bare number reaches the players rung: arrival events teach numeric
/// character ids, and a prop id must never shadow a person.
#[test]
fn a_numeric_target_resolves_to_a_character() {
    let (s, _rx) = targetable_state();
    assert_eq!(
        s.resolve_move_target("2"),
        Ok(MoveTarget::Character {
            id: PlayerId::from(2),
            name: "Karl".to_string()
        })
    );
}

/// Move-by-name matches the way pickup matches: the display name from
/// items.json and loose spellings work, not just the exact def id.
#[test]
fn a_ground_item_display_name_resolves_like_pickup() {
    let (s, _rx) = targetable_state();
    assert_eq!(
        s.resolve_move_target("Iron Sword"),
        Ok(MoveTarget::GroundItem {
            instance_id: 6045,
            name: "iron_sword".to_string()
        })
    );
}

/// A species is not an id, and saying so with the matching ids is what
/// lets the LLM fix the target on its next turn.
#[test]
fn a_monster_species_is_refused_with_the_ids_that_would_work() {
    let (s, _rx) = targetable_state();

    let Err(MoveTargetError::SpeciesNotId {
        species,
        candidates,
    }) = s.resolve_move_target("goblin")
    else {
        panic!("expected a species rejection");
    };
    assert_eq!(species, "goblin");
    // Nearest first, and only the goblins.
    assert_eq!(
        candidates
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["m21", "m27"]
    );
}

/// An id that no longer names anything in sight is a dead target, not an
/// unknown word — the LLM copied it from a stale world state.
#[test]
fn a_vanished_monster_id_says_it_is_gone() {
    let (s, _rx) = targetable_state();
    assert_eq!(
        s.resolve_move_target("m24"),
        Err(MoveTargetError::MonsterGone {
            id: "m24".to_string()
        })
    );
}

/// Nothing matched, so the reply carries what would have.
#[test]
fn an_unknown_target_lists_what_is_addressable() {
    let (s, _rx) = targetable_state();
    let Err(MoveTargetError::Unknown { addressable, .. }) = s.resolve_move_target("the tavern")
    else {
        panic!("expected an unknown target");
    };
    assert!(addressable.contains(&"Karl".to_string()), "{addressable:?}");
    assert!(addressable.contains(&"m21".to_string()), "{addressable:?}");
}

/// Dungeon names come from the registry and survive whatever casing and
/// spacing the LLM read them back with.
#[test]
fn a_dungeon_name_resolves_however_it_is_written() {
    let (s, _rx) = test_state();
    s.world_cache.write().unwrap().register_dungeons();

    for asked in ["Old Crypt", "old crypt", "old_crypt", "OldCrypt"] {
        assert_eq!(
            s.resolve_move_target(asked),
            Ok(MoveTarget::Dungeon {
                id: "old_crypt".to_string(),
                name: "Old Crypt".to_string()
            }),
            "{asked}"
        );
    }
}

fn tip_hat(owner: PlayerId) -> onlinerpg_shared::tip_hat::TipHat {
    onlinerpg_shared::tip_hat::TipHat {
        id: 900,
        owner,
        owner_name: "Me".to_string(),
        accepts_song_requests: false,
        position: p(0.0, 0.0, 2.0),
        rotation: 0.0,
        floor_level: 0,
    }
}

/// A schedule departure folds our stall and picks our hat up by using the
/// hat item; someone else's hat is not ours to touch.
#[tokio::test]
async fn packing_up_before_leaving_folds_our_stall_and_tip_hat() {
    let (mut s, mut rx) = test_state();
    let me = test_player(0.0, 0.0);
    s.self_player_id = Some(me.id);
    s.self_player = Some(me);
    s.self_bag = vec![onlinerpg_shared::inventory::ItemInstance {
        locked: false,
        instance_id: 41,
        item_def_id: "tip_hat".to_string(),
        quantity: 1,
        enchant: 0,
        cape_color: None,
        cape_texture: None,
    }];

    s.tip_hats.insert(900, tip_hat(PlayerId::from(2)));
    s.pack_up_placeables("test").await;
    assert!(rx.try_recv().is_err(), "another busker's hat is left alone");

    s.tip_hats.insert(900, tip_hat(PlayerId::from(1)));
    s.stalls.insert(
        77,
        onlinerpg_shared::stall::Stall {
            id: 77,
            owner: PlayerId::from(1),
            position: p(0.0, 0.0, 1.0),
            rotation: 0.0,
            floor_level: 0,
            owner_name: "Karl".to_string(),
            sign: String::new(),
        },
    );
    assert!(s.own_tip_hat().is_some());
    s.pack_up_placeables("test").await;
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::ChatMessage { message }) if message == "/pack_stall"
    ));
    assert!(matches!(
        rx.try_recv(),
        Ok(ClientMessage::UseItem { instance_id: 41 })
    ));
}

/// Our gate must mirror the server's exactly, boundary included.
#[tokio::test]
async fn a_step_sprints_only_while_the_server_would_allow_it() {
    use onlinerpg_shared::hunger::{hunger_state, NORMAL_MIN};

    let sprinting_at = |satiation: Option<u32>, always: bool, asked: Option<bool>| async move {
        let (mut s, mut rx) = test_state();
        s.self_player = Some(test_player(0.0, 0.0));
        s.always_sprint = always;
        s.self_hunger = satiation.map(|sat| (sat, hunger_state(sat)));
        synchronize_view(&mut s);
        s.request_move(1.0, 0.0, false, asked).await.unwrap();
        match rx.try_recv() {
            Ok(ClientMessage::PlayerMoveGoal { sprinting, .. }) => sprinting,
            other => panic!("expected a PlayerMove, got {other:?}"),
        }
    };

    // Unset takes the agent's default; an override outranks it both ways.
    assert!(sprinting_at(Some(NORMAL_MIN + 1), true, None).await);
    assert!(!sprinting_at(Some(NORMAL_MIN + 1), false, None).await);
    assert!(!sprinting_at(Some(NORMAL_MIN + 1), true, Some(false)).await);
    assert!(sprinting_at(Some(NORMAL_MIN + 1), false, Some(true)).await);
    // The server's gate is strict (`> NORMAL_MIN`), so ours is too.
    assert!(!sprinting_at(Some(NORMAL_MIN), true, Some(true)).await);
    assert!(!sprinting_at(Some(0), true, Some(true)).await);
    // Nothing known yet: let the request stand and the server judge it.
    assert!(sprinting_at(None, true, None).await);
}

#[tokio::test]
async fn movement_waits_for_current_server_progress_and_never_adopts_a_stale_goal() {
    use onlinerpg_shared::messages::MoveStatus;
    let (mut s, mut rx) = targetable_state();
    let initial = s.self_player.as_ref().unwrap().position;
    let first = s.request_move(3.0, 0.0, false, Some(false)).await.unwrap();
    assert!(
        matches!(rx.recv().await.unwrap(), ClientMessage::PlayerMoveGoal { request_id, .. } if request_id == first)
    );
    assert_eq!(s.self_player.as_ref().unwrap().position, initial);
    let second = s.request_move(6.0, 0.0, false, Some(false)).await.unwrap();
    let update = |request_id, x, floor_level| ServerMessage::PlayerMoveProgress {
        request_id,
        position: p(x, 3.1, 0.0),
        rotation: 1.0,
        floor_level,
        server_time_ms: 1,
        next_waypoint: 0,
        speed: 3.0,
        status: MoveStatus::Moving,
    };
    s.push_event(update(first, 3.0, 0));
    assert_eq!(s.self_player.as_ref().unwrap().position, initial);
    s.push_event(update(second, 2.0, 1));
    assert_eq!(s.self_player.as_ref().unwrap().position.x, 2.0);
    assert_eq!(s.self_floor_level, 1);
    assert_eq!(s.move_status, Some(MoveStatus::Moving));
}

#[tokio::test]
async fn furniture_positions_change_only_on_server_approval_and_exit() {
    let (mut s, _rx) = targetable_state();
    let before = s.self_player.as_ref().unwrap().position;
    s.send_command(ClientMessage::InteractObject {
        object_type: "chair".into(),
        object_id: 7,
    })
    .await
    .unwrap();
    assert_eq!(s.self_player.as_ref().unwrap().position, before);
    let id = s.self_player_id.unwrap();
    let seat = p(3.0, 8.1, 4.0);
    s.push_event(ServerMessage::PlayerInteractionChanged {
        player_id: id,
        object_type: Some("chair".into()),
        object_id: Some(7),
        position: seat,
        rotation: 1.2,
        floor_level: 1,
    });
    assert_eq!(s.self_player.as_ref().unwrap().position, seat);
    assert_eq!(s.self_player.as_ref().unwrap().rotation, 1.2);
    assert_eq!(s.self_floor_level, 1);
    s.send_command(ClientMessage::StopInteraction)
        .await
        .unwrap();
    assert_eq!(s.self_player.as_ref().unwrap().position, seat);
    let exit = p(4.0, 8.1, 4.0);
    s.push_event(ServerMessage::PlayerInteractionChanged {
        player_id: id,
        object_type: None,
        object_id: None,
        position: exit,
        rotation: 1.2,
        floor_level: 1,
    });
    assert_eq!(s.self_player.as_ref().unwrap().position, exit);
    assert!(s.self_player.as_ref().unwrap().object_id.is_none());
}
