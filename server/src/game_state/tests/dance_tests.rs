use super::player_interaction_tests::{chair, room};
use super::*;
use crate::auth::AuthService;
use onlinerpg_shared::messages::{JOIN_DANCE_MAX_RISE_M, JOIN_DANCE_RANGE_M, MAX_DANCE_GROUP};
use onlinerpg_shared::mount::MountKind;

async fn dance(game_state: &GameState, auth: &AuthService, name: &str, emote: &str) {
    game_state
        .send_chat_message(&pid(name), format!("/emote {emote}"), auth)
        .await;
}

fn drain(rx: &mut DirectRx) -> Vec<ServerMessage> {
    std::iter::from_fn(|| rx.try_recv().ok()).collect()
}

async fn object_type(game_state: &GameState, name: &str) -> Option<String> {
    game_state.players.read().await[&pid(name)]
        .object_type
        .clone()
}

async fn anchor(game_state: &GameState, name: &str) -> Option<std::time::Instant> {
    game_state.dance_beats.read().await.get(&pid(name))
}

/// The refusal's localization code. Refusals are chat lines, not
/// `InteractionRejected`, which would end a pose the joiner still holds.
fn rejection(messages: &[ServerMessage]) -> Option<&str> {
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, ServerMessage::InteractionRejected { .. })),
        "{messages:?}"
    );
    messages.iter().find_map(|m| match m {
        ServerMessage::SystemMessage {
            localization: Some(localization),
            ..
        } => Some(localization.code.as_str()),
        _ => None,
    })
}

/// A player standing on `make_flat_world_game_state`'s ground.
async fn add_grounded(game_state: &GameState, name: &str, x: f32, z: f32) {
    let mut player = make_player(name, x, z);
    player.position.y = 5.0;
    game_state.add_player(player).await;
}

async fn position(game_state: &GameState, name: &str) -> Position {
    game_state.players.read().await[&pid(name)].position
}

/// The joiner steps into the first spot at the dancer's shoulder, faces the
/// same way, dances the same dance and shares the dancer's beat.
#[tokio::test]
async fn joining_stands_beside_the_dancer_in_step() {
    let game_state = make_test_game_state("dance_join");
    let auth = make_test_auth("dance_join");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("joiner", 3.0, 2.0)).await;
    let mut joiner_rx = game_state.register_direct_channel(&pid("joiner")).await;
    dance(&game_state, &auth, "leader", "twist").await;
    drain(&mut joiner_rx);

    game_state.join_dance(&pid("joiner"), &pid("leader")).await;

    assert_eq!(
        object_type(&game_state, "joiner").await.as_deref(),
        Some("twist")
    );
    let (leader, joiner) = {
        let players = game_state.players.read().await;
        (
            players[&pid("leader")].clone(),
            players[&pid("joiner")].clone(),
        )
    };
    let gap = leader.position.dist_xz_sq(&joiner.position).sqrt();
    assert!(
        (gap - 1.1).abs() < 0.05,
        "stands one spacing away, got {gap}"
    );
    assert_eq!(joiner.rotation, leader.rotation);
    assert_eq!(
        anchor(&game_state, "joiner").await,
        anchor(&game_state, "leader").await
    );

    let messages = drain(&mut joiner_rx);
    let pose = messages.iter().position(|m| {
        matches!(m, ServerMessage::PlayerInteractionChanged { player_id, object_type, .. }
            if *player_id == pid("joiner") && object_type.as_deref() == Some("twist"))
    });
    let beat = messages.iter().position(|m| {
        matches!(m, ServerMessage::PlayerDanceSync { player_id, elapsed_secs }
            if *player_id == pid("joiner") && *elapsed_secs < 5.0)
    });
    assert!(pose.is_some() && beat.is_some(), "got {messages:?}");
    assert!(
        pose < beat,
        "the pose comes before the beat it is sought to"
    );
}

/// A second joiner takes the other shoulder rather than the taken one.
#[tokio::test]
async fn the_next_joiner_takes_the_other_shoulder() {
    let game_state = make_test_game_state("dance_second");
    let auth = make_test_auth("dance_second");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("first", 0.0, 3.0)).await;
    game_state.add_player(make_player("second", 0.5, 3.0)).await;
    dance(&game_state, &auth, "leader", "macarena").await;

    game_state.join_dance(&pid("first"), &pid("leader")).await;
    game_state.join_dance(&pid("second"), &pid("leader")).await;

    let players = game_state.players.read().await;
    let first = players[&pid("first")].position;
    let second = players[&pid("second")].position;
    assert!(first.x > 1.0 && second.x < -1.0, "{first:?} / {second:?}");
    assert_eq!(
        players[&pid("second")].object_type.as_deref(),
        Some("macarena")
    );
}

/// Too far, not dancing, or only holding a pose: nothing to join.
#[tokio::test]
async fn only_a_nearby_joinable_dance_can_be_joined() {
    let game_state = make_test_game_state("dance_refuse");
    let auth = make_test_auth("dance_refuse");
    let far = JOIN_DANCE_RANGE_M + 1.0;
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("near", 2.0, 2.0)).await;
    game_state.add_player(make_player("far", far, 0.0)).await;
    let mut near_rx = game_state.register_direct_channel(&pid("near")).await;
    let mut far_rx = game_state.register_direct_channel(&pid("far")).await;

    game_state.join_dance(&pid("near"), &pid("leader")).await;
    assert_eq!(rejection(&drain(&mut near_rx)), Some("server.danceGone"));

    dance(&game_state, &auth, "leader", "stand_pose2").await;
    drain(&mut near_rx);
    game_state.join_dance(&pid("near"), &pid("leader")).await;
    assert_eq!(rejection(&drain(&mut near_rx)), Some("server.danceGone"));

    dance(&game_state, &auth, "leader", "chicken").await;
    drain(&mut far_rx);
    game_state.join_dance(&pid("far"), &pid("leader")).await;
    assert_eq!(rejection(&drain(&mut far_rx)), Some("server.danceGone"));

    assert_eq!(object_type(&game_state, "near").await, None);
    assert_eq!(object_type(&game_state, "far").await, None);
}

/// A dance holds `MAX_DANCE_GROUP` dancers, its first one included.
#[tokio::test]
async fn a_full_dance_refuses_another_joiner() {
    let game_state = make_test_game_state("dance_full");
    let auth = make_test_auth("dance_full");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    let joiners: Vec<String> = (0..MAX_DANCE_GROUP).map(|i| format!("joiner{i}")).collect();
    for (i, name) in joiners.iter().enumerate() {
        // Waiting in front of the dancer, clear of every spot beside and behind.
        let x = -2.0 + i as f32 * 0.5;
        game_state.add_player(make_player(name, x, 3.0)).await;
    }
    dance(&game_state, &auth, "leader", "twist").await;

    let (last, rest) = joiners.split_last().unwrap();
    for name in rest {
        game_state.join_dance(&pid(name), &pid("leader")).await;
        assert_eq!(
            object_type(&game_state, name).await.as_deref(),
            Some("twist")
        );
    }
    let mut last_rx = game_state.register_direct_channel(&pid(last)).await;
    game_state.join_dance(&pid(last), &pid("leader")).await;

    assert_eq!(rejection(&drain(&mut last_rx)), Some("server.danceFull"));
    assert_eq!(object_type(&game_state, last).await, None);
}

/// Leaving the dance drops the joiner's beat; the dancer keeps theirs.
#[tokio::test]
async fn stopping_ends_only_that_dancers_beat() {
    let game_state = make_test_game_state("dance_stop");
    let auth = make_test_auth("dance_stop");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("joiner", 2.0, 2.0)).await;
    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("joiner"), &pid("leader")).await;

    game_state
        .set_player_interaction(&pid("joiner"), None, None)
        .await;

    assert_eq!(anchor(&game_state, "joiner").await, None);
    assert!(anchor(&game_state, "leader").await.is_some());
    assert_eq!(
        object_type(&game_state, "leader").await.as_deref(),
        Some("twist")
    );
}

/// A wall between the joiner and the dancer keeps the joiner out, though
/// every spot is clear from the dancer's side.
#[tokio::test]
async fn joining_never_crosses_a_wall() {
    let game_state = make_flat_world_game_state("dance_wall");
    let auth = make_test_auth("dance_wall");
    for (name, x) in [("leader", 101.5), ("outsider", 98.5)] {
        let mut player = make_player(name, x, 50.5);
        player.position.y = 5.0;
        game_state.add_player(player).await;
    }
    let mut cells = vec![0; 24];
    for row in 0..4 {
        cells[row * 6 + 1] = 2;
        cells[row * 6 + 2] = 8;
    }
    game_state
        .passability_write()
        .insert("room".into(), room(0, 5.0, cells));
    let mut outsider_rx = game_state.register_direct_channel(&pid("outsider")).await;
    dance(&game_state, &auth, "leader", "twist").await;
    drain(&mut outsider_rx);

    let before = game_state.players.read().await[&pid("outsider")].position;
    game_state
        .join_dance(&pid("outsider"), &pid("leader"))
        .await;

    assert_eq!(
        rejection(&drain(&mut outsider_rx)),
        Some("server.danceNoRoom")
    );
    assert_eq!(object_type(&game_state, "outsider").await, None);
    assert_eq!(
        game_state.players.read().await[&pid("outsider")].position,
        before
    );

    game_state
        .passability_write()
        .insert("room".into(), room(0, 5.0, vec![0; 24]));
    game_state
        .join_dance(&pid("outsider"), &pid("leader"))
        .await;
    assert_eq!(
        object_type(&game_state, "outsider").await.as_deref(),
        Some("twist")
    );
}

/// `/join_dance <name>` is the typed form agents use.
#[tokio::test]
async fn join_dance_by_name() {
    let game_state = make_test_game_state("dance_named");
    let auth = make_test_auth("dance_named");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("agent", 2.0, 2.0)).await;
    dance(&game_state, &auth, "leader", "chicken").await;

    game_state
        .send_chat_message(&pid("agent"), "/join_dance LEADER".to_string(), &auth)
        .await;

    assert_eq!(
        object_type(&game_state, "agent").await.as_deref(),
        Some("chicken")
    );
    assert_eq!(
        anchor(&game_state, "agent").await,
        anchor(&game_state, "leader").await
    );
}

/// The joiner's own state refuses with its own reason; a dancer out of
/// sight, below or on another floor is no dance to join.
#[tokio::test]
async fn joiners_and_dancers_who_cannot_dance_together_are_refused() {
    type Setup = fn(&mut Player, &mut Player);
    let cases: [(&str, Setup); 9] = [
        ("server.danceNotNow", |joiner, _| joiner.health = 0),
        ("server.danceNotNow", |joiner, _| joiner.petrified = true),
        ("server.danceMounted", |joiner, _| {
            joiner.mount = Some(MountKind::Horse)
        }),
        ("server.danceInCombat", |joiner, _| {
            joiner.last_combat_at = GameState::now_ms()
        }),
        ("server.danceGone", |joiner, _| joiner.floor_level = 1),
        ("server.danceGone", |joiner, _| {
            joiner.position.y = -(JOIN_DANCE_MAX_RISE_M + 0.5)
        }),
        ("server.danceGone", |_, leader| leader.invisible = true),
        ("server.danceGone", |_, leader| leader.petrified = true),
        ("server.danceDungeon", |joiner, leader| {
            joiner.floor_level = -1;
            leader.floor_level = -1;
        }),
    ];
    for (i, (expected, setup)) in cases.into_iter().enumerate() {
        let game_state = make_test_game_state(&format!("dance_refused_{i}"));
        let auth = make_test_auth(&format!("dance_refused_{i}"));
        game_state.add_player(make_player("leader", 0.0, 0.0)).await;
        game_state.add_player(make_player("joiner", 2.0, 2.0)).await;
        dance(&game_state, &auth, "leader", "twist").await;
        {
            let mut players = game_state.players.write().await;
            let mut leader = players[&pid("leader")].clone();
            let joiner = players.get_mut(&pid("joiner")).unwrap();
            setup(joiner, &mut leader);
            players.insert(pid("leader"), leader);
        }
        let before = position(&game_state, "joiner").await;
        let mut rx = game_state.register_direct_channel(&pid("joiner")).await;

        game_state.join_dance(&pid("joiner"), &pid("leader")).await;

        assert_eq!(rejection(&drain(&mut rx)), Some(expected), "case {i}");
        assert_eq!(object_type(&game_state, "joiner").await, None, "case {i}");
        assert_eq!(position(&game_state, "joiner").await, before, "case {i}");
    }
}

/// Anyone already in the group can be joined; the newcomer shares the beat.
#[tokio::test]
async fn any_dancer_in_the_group_can_be_joined() {
    let game_state = make_flat_world_game_state("dance_member");
    let auth = make_test_auth("dance_member");
    add_grounded(&game_state, "leader", 0.0, 0.0).await;
    add_grounded(&game_state, "first", 2.0, 2.0).await;
    add_grounded(&game_state, "second", 4.0, 2.0).await;
    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("first"), &pid("leader")).await;

    game_state.join_dance(&pid("second"), &pid("first")).await;

    assert_eq!(
        object_type(&game_state, "second").await.as_deref(),
        Some("twist")
    );
    assert_eq!(
        anchor(&game_state, "second").await,
        anchor(&game_state, "leader").await
    );
    let gap = position(&game_state, "second")
        .await
        .dist_xz_sq(&position(&game_state, "first").await)
        .sqrt();
    assert!(gap < 1.2, "stands beside the one joined, got {gap}");
}

/// Asking again while in the group says so and changes nothing.
#[tokio::test]
async fn joining_your_own_group_again_is_refused() {
    let game_state = make_flat_world_game_state("dance_again");
    let auth = make_test_auth("dance_again");
    add_grounded(&game_state, "leader", 0.0, 0.0).await;
    add_grounded(&game_state, "joiner", 2.0, 2.0).await;
    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("joiner"), &pid("leader")).await;
    let before = position(&game_state, "joiner").await;
    let mut rx = game_state.register_direct_channel(&pid("joiner")).await;

    game_state.join_dance(&pid("joiner"), &pid("leader")).await;

    assert_eq!(rejection(&drain(&mut rx)), Some("server.danceAlready"));
    assert_eq!(position(&game_state, "joiner").await, before);
}

/// A seated player gets up and steps in; a refused one stays seated.
#[tokio::test]
async fn joining_from_a_seat() {
    let game_state = make_flat_world_game_state("dance_seat");
    let auth = make_test_auth("dance_seat");
    let seat = chair();
    game_state.sync_region_furniture(0, 0, std::slice::from_ref(&seat));
    add_grounded(&game_state, "sitter", 98.5, 50.5).await;
    add_grounded(&game_state, "leader", 100.5, 53.5).await;
    game_state
        .set_player_interaction(&pid("sitter"), Some("chair".into()), Some(seat.id))
        .await;
    let seated = game_state.players.read().await[&pid("sitter")].clone();
    assert_eq!(seated.object_id, Some(seat.id));
    let mut rx = game_state.register_direct_channel(&pid("sitter")).await;

    game_state.join_dance(&pid("sitter"), &pid("leader")).await;
    assert_eq!(rejection(&drain(&mut rx)), Some("server.danceGone"));
    let still = game_state.players.read().await[&pid("sitter")].clone();
    assert_eq!(still.object_id, Some(seat.id));
    assert_eq!(still.position, seated.position);

    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("sitter"), &pid("leader")).await;

    let joined = game_state.players.read().await[&pid("sitter")].clone();
    assert_eq!(joined.object_type.as_deref(), Some("twist"));
    assert_eq!(joined.object_id, None);
    let gap = joined
        .position
        .dist_xz_sq(&position(&game_state, "leader").await)
        .sqrt();
    assert!((gap - 1.1).abs() < 0.05, "stands at a shoulder, got {gap}");
    assert_eq!(
        anchor(&game_state, "sitter").await,
        anchor(&game_state, "leader").await
    );
}

/// Joins arriving together still stop at `MAX_DANCE_GROUP`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn simultaneous_joins_never_overfill_a_dance() {
    let game_state = make_test_game_state("dance_rush");
    let auth = make_test_auth("dance_rush");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    let joiners: Vec<String> = (0..MAX_DANCE_GROUP + 3)
        .map(|i| format!("rusher{i}"))
        .collect();
    for (i, name) in joiners.iter().enumerate() {
        let x = -2.5 + i as f32 * 0.5;
        game_state.add_player(make_player(name, x, 3.0)).await;
    }
    dance(&game_state, &auth, "leader", "twist").await;

    let tasks: Vec<_> = joiners
        .iter()
        .map(|name| {
            let game_state = game_state.clone();
            let id = pid(name);
            tokio::spawn(async move { game_state.join_dance(&id, &pid("leader")).await })
        })
        .collect();
    for task in tasks {
        task.await.unwrap();
    }

    let beat = anchor(&game_state, "leader").await.unwrap();
    let group = game_state.dance_beats.read().await.group_size(beat);
    assert_eq!(group, MAX_DANCE_GROUP);
    let mut dancing = 0;
    for name in &joiners {
        if object_type(&game_state, name).await.is_some() {
            dancing += 1;
        }
    }
    assert_eq!(dancing, MAX_DANCE_GROUP - 1);
}

/// A teleport or a logout ends that dancer's beat.
#[tokio::test]
async fn relocation_and_logout_clear_the_beat() {
    let game_state = make_test_game_state("dance_clear");
    let auth = make_test_auth("dance_clear");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("joiner", 2.0, 2.0)).await;
    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("joiner"), &pid("leader")).await;
    assert!(anchor(&game_state, "joiner").await.is_some());

    game_state.teleport_to_town(&pid("joiner")).await;
    assert_eq!(anchor(&game_state, "joiner").await, None);
    assert_eq!(object_type(&game_state, "joiner").await, None);

    game_state.remove_player(&pid("leader")).await;
    assert_eq!(anchor(&game_state, "leader").await, None);
}

/// Death ends the beat, so a dancer who dies holds no place in the group and
/// can join it again after respawning.
#[tokio::test]
async fn death_and_respawn_clear_the_beat() {
    let game_state = make_test_game_state("dance_death");
    let auth = make_test_auth("dance_death");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    game_state.add_player(make_player("joiner", 2.0, 2.0)).await;
    dance(&game_state, &auth, "leader", "twist").await;
    game_state.join_dance(&pid("joiner"), &pid("leader")).await;
    let beat = anchor(&game_state, "leader").await.unwrap();
    assert_eq!(anchor(&game_state, "joiner").await, Some(beat));

    game_state
        .players
        .write()
        .await
        .get_mut(&pid("joiner"))
        .unwrap()
        .health = 0;
    game_state.on_player_died(&pid("joiner"), "test").await;
    assert_eq!(anchor(&game_state, "joiner").await, None);

    game_state
        .dance_beats
        .write()
        .await
        .insert(pid("joiner"), beat);
    game_state.respawn_player(&pid("joiner")).await;
    assert!(game_state.players.read().await[&pid("joiner")].health > 0);
    assert_eq!(anchor(&game_state, "joiner").await, None);
}

/// Ground 5 m high west of the tile edge at x = -32 and 6.8 m east of it: a
/// sheer ledge lower than `JOIN_DANCE_MAX_RISE_M`.
struct Ledge;

#[async_trait::async_trait]
impl onlinerpg_terrain::height::HeightTiles for Ledge {
    async fn read_heightmap(&self, tx: i32, _tz: i32) -> std::io::Result<Vec<u8>> {
        Ok(uniform_heightmap(if tx < 0 { 5.0 } else { 6.8 }))
    }
}

/// A ledge too steep to walk up keeps the player below it out, though the
/// average slope from them to the spot is gentle.
#[tokio::test]
async fn a_ledge_keeps_a_joiner_below_it_out() {
    let game_state = make_game_state_with("dance_ledge", Ledge, SeaOnlyWater);
    let auth = make_test_auth("dance_ledge");
    for (name, x, z, y) in [
        ("leader", -30.5, 0.0, 6.8),
        ("below", -34.5, 0.0, 5.0),
        ("above", -28.0, 2.0, 6.8),
    ] {
        let mut player = make_player(name, x, z);
        player.position.y = y;
        game_state.add_player(player).await;
    }
    dance(&game_state, &auth, "leader", "twist").await;
    let mut below_rx = game_state.register_direct_channel(&pid("below")).await;

    game_state.join_dance(&pid("below"), &pid("leader")).await;
    assert_eq!(rejection(&drain(&mut below_rx)), Some("server.danceNoRoom"));
    assert_eq!(object_type(&game_state, "below").await, None);

    game_state.join_dance(&pid("above"), &pid("leader")).await;
    assert_eq!(
        object_type(&game_state, "above").await.as_deref(),
        Some("twist")
    );
}

/// The Join button and `/join_dance` share one per-player limit.
#[tokio::test]
async fn join_requests_are_rate_limited_across_button_and_chat() {
    let game_state = make_test_game_state("dance_limit");
    let auth = make_test_auth("dance_limit");
    game_state.add_player(make_player("leader", 0.0, 0.0)).await;
    let far = JOIN_DANCE_RANGE_M + 1.0;
    game_state
        .add_player(make_player("spammer", far, 0.0))
        .await;
    dance(&game_state, &auth, "leader", "twist").await;
    let mut rx = game_state.register_direct_channel(&pid("spammer")).await;

    for by_chat in [false, true, true, false] {
        if by_chat {
            game_state
                .send_chat_message(&pid("spammer"), "/join_dance leader".to_string(), &auth)
                .await;
        } else {
            game_state.join_dance(&pid("spammer"), &pid("leader")).await;
        }
    }

    let refusals = drain(&mut rx)
        .iter()
        .filter(|m| {
            matches!(m, ServerMessage::SystemMessage { localization: Some(l), .. }
                if l.code == "server.danceGone")
        })
        .count();
    assert_eq!(refusals, 2);
}

/// A bare `/join_dance` explains itself instead of reporting no dance.
#[tokio::test]
async fn bare_join_dance_shows_its_usage() {
    let game_state = make_test_game_state("dance_usage");
    let auth = make_test_auth("dance_usage");
    game_state.add_player(make_player("agent", 0.0, 0.0)).await;
    let mut rx = game_state.register_direct_channel(&pid("agent")).await;

    game_state
        .send_chat_message(&pid("agent"), "/join_dance".to_string(), &auth)
        .await;

    assert_eq!(rejection(&drain(&mut rx)), Some("server.joinDanceUsage"));
}

/// A dancer whose pose change is refused gets the dance back on its beat.
#[tokio::test]
async fn a_refused_pose_change_restores_the_beat() {
    let game_state = make_flat_world_game_state("dance_restore");
    let auth = make_test_auth("dance_restore");
    let seat = chair();
    game_state.sync_region_furniture(0, 0, std::slice::from_ref(&seat));
    add_grounded(&game_state, "sitter", 98.5, 50.5).await;
    add_grounded(&game_state, "dancer", 100.5, 52.0).await;
    game_state
        .set_player_interaction(&pid("sitter"), Some("chair".into()), Some(seat.id))
        .await;
    dance(&game_state, &auth, "dancer", "twist").await;
    let beat = anchor(&game_state, "dancer").await;
    let mut rx = game_state.register_direct_channel(&pid("dancer")).await;

    game_state
        .set_player_interaction(&pid("dancer"), Some("chair".into()), Some(seat.id))
        .await;

    let messages = drain(&mut rx);
    let pose = messages.iter().position(|m| {
        matches!(m, ServerMessage::PlayerInteractionChanged { object_type, .. }
            if object_type.as_deref() == Some("twist"))
    });
    let restored = messages.iter().position(|m| {
        matches!(m, ServerMessage::PlayerDanceSync { player_id, .. } if *player_id == pid("dancer"))
    });
    assert!(pose.is_some() && pose < restored, "{messages:?}");
    assert_eq!(anchor(&game_state, "dancer").await, beat);
}

/// Revived where they fell, a dancer gets up out of the dance, and viewers
/// never see the dance again on the way.
#[tokio::test]
async fn a_dancer_revived_in_place_stops_dancing() {
    let game_state = make_test_game_state("dance_revive");
    let auth = make_test_auth("dance_revive");
    game_state.add_player(make_player("dancer", 0.0, 0.0)).await;
    game_state.add_player(make_player("viewer", 3.0, 0.0)).await;
    dance(&game_state, &auth, "dancer", "twist").await;
    game_state
        .players
        .write()
        .await
        .get_mut(&pid("dancer"))
        .unwrap()
        .health = 0;
    game_state.on_player_died(&pid("dancer"), "test").await;
    let mut viewer_rx = game_state.register_direct_channel(&pid("viewer")).await;

    assert!(game_state.revive_in_place(&pid("dancer"), 50).await);

    assert_eq!(object_type(&game_state, "dancer").await, None);
    assert_eq!(anchor(&game_state, "dancer").await, None);
    let respawned = drain(&mut viewer_rx)
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::PlayerRespawned { player } if player.id == pid("dancer") => Some(player),
            _ => None,
        })
        .expect("the viewer sees the revive");
    assert_eq!(respawned.object_type, None);
}

/// Changing dances drops the old beat before the new pose shows, so no
/// joiner can pair the new dance with the old beat.
#[tokio::test]
async fn changing_dances_drops_the_old_beat_first() {
    let game_state = make_test_game_state("dance_switch");
    let auth = make_test_auth("dance_switch");
    game_state.add_player(make_player("dancer", 0.0, 0.0)).await;
    game_state.add_player(make_player("joiner", 2.0, 2.0)).await;
    dance(&game_state, &auth, "dancer", "twist").await;
    let twist = anchor(&game_state, "dancer").await;

    game_state
        .set_player_interaction(&pid("dancer"), Some("macarena".into()), None)
        .await;
    assert_eq!(anchor(&game_state, "dancer").await, None);
    game_state.join_dance(&pid("joiner"), &pid("dancer")).await;
    assert_eq!(anchor(&game_state, "joiner").await, None);

    dance(&game_state, &auth, "dancer", "macarena").await;
    let macarena = anchor(&game_state, "dancer").await;
    assert!(macarena.is_some() && macarena != twist);
}
