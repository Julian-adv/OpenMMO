use super::*;
use crate::game_state::chat::AdminCommand;

fn followed(messages: &[ServerMessage]) -> Option<Option<PlayerId>> {
    messages.iter().rev().find_map(|m| match m {
        ServerMessage::AdminFollowChanged { target, .. } => Some(*target),
        _ => None,
    })
}

async fn gap(game: &GameState, a: &str, b: &str) -> f32 {
    let players = game.players.read().await;
    let (a, b) = (&players[&pid(a)].position, &players[&pid(b)].position);
    (a.x - b.x).hypot(a.z - b.z)
}

async fn move_to(game: &GameState, name: &str, x: f32, z: f32, floor: i8) {
    game.teleport_player(&pid(name), Position { x, y: 0.0, z }, 0.0, floor)
        .await;
}

async fn setup(test: &str) -> (GameState, crate::auth::AuthService, DirectRx) {
    let game = make_test_game_state(test);
    let auth = make_test_auth(test);
    game.add_player(make_player("admin", 0.0, 0.0)).await;
    game.add_player(make_player("wanderer", 300.0, 0.0)).await;
    let rx = game.register_direct_channel(&pid("admin")).await;
    (game, auth, rx)
}

#[test]
fn follow_commands_parse() {
    assert_eq!(
        crate::game_state::parse_admin_command("/follow Abuser"),
        Some(AdminCommand::Follow("Abuser"))
    );
    assert_eq!(
        crate::game_state::parse_admin_command("/follow"),
        Some(AdminCommand::Follow(""))
    );
    assert_eq!(
        crate::game_state::parse_admin_command("/unfollow"),
        Some(AdminCommand::Unfollow)
    );
    assert_eq!(crate::game_state::parse_admin_command("/followers"), None);
}

#[tokio::test]
async fn follow_lands_beside_the_target_and_tells_the_client() {
    let (game, auth, mut rx) = setup("admin_follow_start").await;
    game.send_chat_message(&pid("admin"), "/follow WANDERER".into(), &auth)
        .await;
    assert!(gap(&game, "admin", "wanderer").await <= 2.0);
    assert_eq!(
        game.admin_follow_target(&pid("admin")),
        Some(pid("wanderer"))
    );
    let messages = drain(&mut rx);
    assert_eq!(followed(&messages), Some(Some(pid("wanderer"))));

    game.send_chat_message(&pid("admin"), "/unfollow".into(), &auth)
        .await;
    assert_eq!(game.admin_follow_target(&pid("admin")), None);
    assert_eq!(followed(&drain(&mut rx)), Some(None));
}

#[tokio::test]
async fn bare_follow_stops_and_self_follow_is_refused() {
    let (game, auth, mut rx) = setup("admin_follow_bare").await;
    game.send_chat_message(&pid("admin"), "/follow admin".into(), &auth)
        .await;
    assert_eq!(game.admin_follow_target(&pid("admin")), None);
    game.send_chat_message(&pid("admin"), "/follow wanderer".into(), &auth)
        .await;
    game.send_chat_message(&pid("admin"), "/follow".into(), &auth)
        .await;
    assert_eq!(game.admin_follow_target(&pid("admin")), None);
    assert_eq!(followed(&drain(&mut rx)), Some(None));
}

#[tokio::test]
async fn tick_relocates_only_when_walking_cannot_keep_up() {
    let (game, auth, _rx) = setup("admin_follow_tick").await;
    game.send_chat_message(&pid("admin"), "/follow wanderer".into(), &auth)
        .await;

    move_to(&game, "wanderer", 310.0, 0.0, 0).await;
    game.tick_admin_follows().await;
    assert!(
        gap(&game, "admin", "wanderer").await > 5.0,
        "a short walk is the client's to follow"
    );

    move_to(&game, "wanderer", -200.0, 80.0, 0).await;
    game.tick_admin_follows().await;
    assert!(gap(&game, "admin", "wanderer").await <= 2.0);
}

#[tokio::test]
async fn a_floor_change_nearby_waits_before_relocating() {
    let (game, auth, _rx) = setup("admin_follow_floor").await;
    game.send_chat_message(&pid("admin"), "/follow wanderer".into(), &auth)
        .await;
    let (x, z) = {
        let players = game.players.read().await;
        let p = &players[&pid("admin")].position;
        (p.x + 3.0, p.z)
    };
    move_to(&game, "wanderer", x, z, 1).await;
    for _ in 0..2 {
        game.tick_admin_follows().await;
        assert_eq!(game.players.read().await[&pid("admin")].floor_level, 0);
    }
    game.tick_admin_follows().await;
    assert_eq!(game.players.read().await[&pid("admin")].floor_level, 1);
}

#[tokio::test]
async fn target_leaving_ends_the_follow() {
    let (game, auth, mut rx) = setup("admin_follow_leave").await;
    game.send_chat_message(&pid("admin"), "/follow wanderer".into(), &auth)
        .await;
    drain(&mut rx);
    game.remove_player(&pid("wanderer")).await;
    assert_eq!(game.admin_follow_target(&pid("admin")), None);
    let messages = drain(&mut rx);
    assert_eq!(followed(&messages), Some(None));
    assert!(messages.iter().any(|m| matches!(
        m,
        ServerMessage::SystemMessage { message, .. } if message == "Follow: wanderer left the world."
    )));
}

#[tokio::test]
async fn unfollow_without_a_follow_still_clears_the_client() {
    let (game, auth, mut rx) = setup("admin_follow_stale").await;
    game.send_chat_message(&pid("admin"), "/unfollow".into(), &auth)
        .await;
    assert_eq!(followed(&drain(&mut rx)), Some(None));
}
