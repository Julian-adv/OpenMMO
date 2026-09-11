use super::*;
use crate::metrics::ConcurrentCounts;
use crate::types::ClientKind;

#[tokio::test]
async fn unique_activity_records_short_visits_and_excludes_official_npcs() {
    let game = make_test_game_state("unique_activity");
    let auth = make_test_auth("unique_activity");
    let mut ids = Vec::new();
    for (account, npc) in [("browser", false), ("official_npc", true)] {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let session = game.register_account_session(account, tx, &auth).await;
        let mut player = make_player(account, 0.0, 0.0);
        player.is_official_npc = npc;
        let id = player.id;
        game.attach_player_to_account_session(account, session, id)
            .await;
        game.add_player(player).await;
        game.begin_account_activity(id, account, &auth).await;
        ids.push((account, session));
    }
    assert_eq!(game.account_activity_snapshot().await.len(), 1);
    for (account, session) in ids {
        game.end_account_session(account, session, &auth).await;
    }
    assert!(game.account_activity_snapshot().await.is_empty());
    let now = crate::auth::unix_now();
    let midnight = crate::metrics::kst_day_start(now) + crate::metrics::DAY_SECONDS;
    auth.aggregate_daily_unique_accounts(midnight).unwrap();
    let history = auth.unique_account_history(midnight, 1).unwrap();
    assert_eq!(history.samples.last().unwrap().accounts, 1);
}

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
