// ---- Petrified (doc/DEBUFF.md `petrifies`) ----------------------------------

use super::*;
use onlinerpg_shared::hunger::SATIATION_START;
use tokio::time::{advance, Duration};

async fn stone(game: &GameState, id: &PlayerId) -> (bool, bool) {
    let players = game.players.read().await;
    let player = &players[id];
    (player.petrified, player.is_damageable(GameState::now_ms()))
}

fn toggled(msgs: &[ServerMessage]) -> Vec<bool> {
    msgs.iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerPetrifiedToggled { petrified, .. } => Some(*petrified),
            _ => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn petrify_holds_twenty_seconds_and_tells_the_owner() {
    let game = make_test_game_state("petrify_expiry");
    let (id, mut rx) = add_hungry_player(&game, "statue", SATIATION_START).await;
    game.mark_world_ready(&id).await;
    drain(&mut rx);

    assert!(game.inflict_debuff(&id, "petrified", None).await);
    assert_eq!(stone(&game, &id).await, (true, false));
    assert_eq!(toggled(&drain(&mut rx)), [true]);

    advance(Duration::from_secs(19)).await;
    game.tick_debuffs().await;
    assert!(stone(&game, &id).await.0);

    advance(Duration::from_secs(1)).await;
    game.tick_debuffs().await;
    assert_eq!(stone(&game, &id).await, (false, true));
    assert_eq!(toggled(&drain(&mut rx)), [false]);
}

#[tokio::test(start_paused = true)]
async fn death_breaks_the_stone() {
    let game = make_test_game_state("petrify_death");
    let (id, mut rx) = add_hungry_player(&game, "crumbled", SATIATION_START).await;
    game.inflict_debuff(&id, "petrified", None).await;
    drain(&mut rx);
    game.clear_debuffs(&id).await;
    assert!(!stone(&game, &id).await.0);
    assert_eq!(toggled(&drain(&mut rx)), [false]);
}

#[tokio::test(start_paused = true)]
async fn the_foremans_swing_breaks_the_stone_hard() {
    let game = make_test_game_state("petrify_shatter");
    let (id, mut rx) = add_hungry_player(&game, "shattered", SATIATION_START).await;
    game.mark_world_ready(&id).await;
    let mut foreman = make_monster(
        "foreman",
        Position {
            x: 100.0,
            y: 0.0,
            z: 48.5,
        },
        0,
    );
    foreman.monster_type = "mine_foreman".into();
    game.monsters
        .write()
        .await
        .insert("foreman".into(), foreman);
    game.inflict_debuff(&id, "petrified", None).await;
    drain(&mut rx);

    game.monster_attack("foreman", &id).await;
    let msgs = drain(&mut rx);
    assert_eq!(toggled(&msgs), [false]);
    assert!(!stone(&game, &id).await.0);
    assert!(msgs.iter().any(|m| matches!(
        m,
        ServerMessage::MonsterAttackedPlayer { hit: true, damage, .. } if *damage >= 4
    )));
}

#[tokio::test(start_paused = true)]
async fn other_monsters_cannot_hit_stone() {
    let game = make_test_game_state("petrify_plain");
    let (id, mut rx) = add_hungry_player(&game, "statue2", SATIATION_START).await;
    game.mark_world_ready(&id).await;
    let monster = make_monster(
        "goblin",
        Position {
            x: 100.0,
            y: 0.0,
            z: 49.0,
        },
        0,
    );
    game.monsters.write().await.insert("goblin".into(), monster);
    game.inflict_debuff(&id, "petrified", None).await;
    drain(&mut rx);

    game.monster_attack("goblin", &id).await;
    assert!(stone(&game, &id).await.0);
    assert!(toggled(&drain(&mut rx)).is_empty());
}
