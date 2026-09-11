use super::*;

#[tokio::test]
async fn concurrent_counts_only_accounts_in_game_and_excludes_official_npcs() {
    let game = make_test_game_state("concurrent_accounts");
    let auth = make_test_auth("concurrent_accounts");

    for (account, enter, npc) in [
        ("browser", true, false),
        ("external_agent", true, false),
        ("character_select", false, false),
        ("official_npc", true, true),
    ] {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let session = game.register_account_session(account, tx, &auth).await;
        if enter {
            let mut player = make_player(account, 0.0, 0.0);
            player.is_official_npc = npc;
            game.attach_player_to_account_session(account, session, player.id)
                .await;
            game.add_player(player).await;
        }
    }
    game.add_player(make_player("unattached", 0.0, 0.0)).await;
    assert_eq!(game.concurrent_account_count().await, 2);

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let session = game.register_account_session("browser", tx, &auth).await;
    assert_eq!(game.concurrent_account_count().await, 1);
    let player = make_player("replacement", 0.0, 0.0);
    game.attach_player_to_account_session("browser", session, player.id)
        .await;
    game.add_player(player).await;
    assert_eq!(game.concurrent_account_count().await, 2);
    game.end_account_session("browser", session, &auth).await;
    assert_eq!(game.concurrent_account_count().await, 1);
}
