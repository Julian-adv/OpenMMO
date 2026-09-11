use super::*;
use crate::metrics::ConcurrentCounts;
use crate::types::ClientKind;

#[tokio::test]
async fn concurrent_counts_only_accounts_in_game_and_excludes_official_npcs() {
    let game = make_test_game_state("concurrent_accounts");
    let auth = make_test_auth("concurrent_accounts");

    assert_eq!(
        game.concurrent_account_counts().await,
        ConcurrentCounts::default()
    );
    for (account, enter, npc, kind) in [
        ("browser", true, false, ClientKind::Web),
        ("external_agent", true, false, ClientKind::Cli),
        ("character_select", false, false, ClientKind::Web),
        ("official_npc", true, true, ClientKind::Cli),
        ("custom_client", true, false, ClientKind::Other),
        ("unknown_client", true, false, ClientKind::Unknown),
    ] {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let session = game.register_account_session(account, tx, &auth).await;
        if enter {
            let mut player = make_player(account, 0.0, 0.0);
            player.is_official_npc = npc;
            player.client_kind = kind;
            game.attach_player_to_account_session(account, session, player.id)
                .await;
            game.add_player(player).await;
        }
    }
    game.add_player(make_player("unattached", 0.0, 0.0)).await;
    let counts = game.concurrent_account_counts().await;
    assert_eq!(
        counts,
        ConcurrentCounts {
            web_accounts: 1,
            agent_accounts: 1,
            other_accounts: 2
        }
    );
    assert_eq!(counts.total(), 4);

    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    let session = game.register_account_session("browser", tx, &auth).await;
    assert_eq!(
        game.concurrent_account_counts().await,
        ConcurrentCounts {
            web_accounts: 0,
            agent_accounts: 1,
            other_accounts: 2
        }
    );
    let mut player = make_player("replacement", 0.0, 0.0);
    player.client_kind = ClientKind::Cli;
    game.attach_player_to_account_session("browser", session, player.id)
        .await;
    game.add_player(player).await;
    assert_eq!(
        game.concurrent_account_counts().await,
        ConcurrentCounts {
            web_accounts: 0,
            agent_accounts: 2,
            other_accounts: 2
        }
    );
    game.end_account_session("browser", session, &auth).await;
    assert_eq!(
        game.concurrent_account_counts().await,
        ConcurrentCounts {
            web_accounts: 0,
            agent_accounts: 1,
            other_accounts: 2
        }
    );
}
