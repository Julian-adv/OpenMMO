use super::{auth_db, metrics_response, unix_now, MetricsState};
use axum::{extract::State, response::Response};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct CombatAuditTarget {
    character_id: i64,
    name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CombatAuditTargets {
    until: i64,
    entries: Vec<CombatAuditTarget>,
}

pub(super) async fn targets(State(state): State<MetricsState>) -> Response {
    let until = unix_now();
    let character_ids = state.game.combat_audit_character_ids();
    metrics_response(
        auth_db(move || {
            let mut names = state.auth.character_names(&character_ids)?;
            Ok(CombatAuditTargets {
                until,
                entries: character_ids
                    .into_iter()
                    .map(|character_id| CombatAuditTarget {
                        character_id,
                        name: names.remove(&character_id),
                    })
                    .collect(),
            })
        })
        .await,
        "Combat audit targets",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{auth::AuthService, game_state::tests::make_test_game_state};
    use axum::http::StatusCode;
    use std::sync::Arc;

    #[tokio::test]
    async fn combat_audit_targets_show_applied_ids_with_current_names_even_when_offline() {
        let dir = crate::test_util::unique_temp_dir("combat_audit_targets_api");
        let path = dir.join("game.db");
        let auth = Arc::new(AuthService::new(path.clone()).unwrap());
        let game = Arc::new(make_test_game_state("combat_audit_targets_api"));
        let router = crate::metrics::metrics_routes(Arc::clone(&game), auth);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/api/metrics/combat-audit-targets",
            listener.local_addr().unwrap()
        );
        let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();

        let data: CombatAuditTargets = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert!(data.entries.is_empty());

        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (player_name) VALUES ('audit-test');
             INSERT INTO characters (id, account_name, character_name) VALUES
             (1, 'audit-test', '용사A'), (2, 'audit-test', '용사B'), (3, 'audit-test', 'Other');",
        )
        .unwrap();
        std::fs::write(dir.join("combat-audit.txt"), "999\n2\n1\n2\n").unwrap();
        game.tick_combat_audit(dir.clone(), 30, false).await;
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let data: CombatAuditTargets = response.json().await.unwrap();
        assert!(data.until > 0 && data.until <= unix_now());
        assert_eq!(
            data.entries,
            vec![
                CombatAuditTarget {
                    character_id: 1,
                    name: Some("용사A".into())
                },
                CombatAuditTarget {
                    character_id: 2,
                    name: Some("용사B".into())
                },
                CombatAuditTarget {
                    character_id: 999,
                    name: None
                },
            ]
        );

        std::fs::write(dir.join("combat-audit.txt"), "3\n").unwrap();
        conn.execute_batch(
            "UPDATE characters SET character_name = '새 이름' WHERE id = 1;
             DELETE FROM characters WHERE id = 2;",
        )
        .unwrap();
        let data: CombatAuditTargets = client.get(&url).send().await.unwrap().json().await.unwrap();
        assert_eq!(
            data.entries,
            vec![
                CombatAuditTarget {
                    character_id: 1,
                    name: Some("새 이름".into())
                },
                CombatAuditTarget {
                    character_id: 2,
                    name: None
                },
                CombatAuditTarget {
                    character_id: 999,
                    name: None
                },
            ]
        );

        conn.execute_batch("ALTER TABLE characters RENAME TO unavailable_characters;")
            .unwrap();
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()["cache-control"], "no-store");
        task.abort();
        drop(conn);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
