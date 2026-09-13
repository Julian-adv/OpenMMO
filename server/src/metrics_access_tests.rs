use super::*;
use crate::connection::AuthContext;
use crate::game_state::tests::make_test_game_state;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::{json, Value};

const ENDPOINTS: &[&str] = &[
    "session",
    "concurrent",
    "unique",
    "gold",
    "item-gold-sources",
    "gold-sinks",
    "level-leaderboard",
    "gold-leaderboard",
    "land-leaderboard",
    "weapon-enchant-leaderboard",
    "weapon-enchant-failures",
    "armor-enchant-leaderboard",
    "gold-per-account",
    "heroic-tales",
    "combat-audit-targets",
];

fn claims() -> Value {
    json!({
        "sub": "test-account",
        "email": "Admin@example.com",
        "email_verified": true,
        "aud": "test-client",
        "iss": "https://accounts.google.com",
        "exp": unix_now() + 3600,
    })
}

fn signed_token(key: &EncodingKey, claims: &Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-google-key".into());
    encode(&header, claims, key).unwrap()
}

async fn serve(access: AuthContext) -> (String, tokio::task::JoinHandle<()>) {
    let auth = Arc::new(
        AuthService::new(crate::test_util::unique_temp_dir("metrics_access").join("game.db"))
            .unwrap(),
    );
    let router = metrics_router(
        Arc::new(make_test_game_state("metrics_access")),
        auth,
        Arc::new(access),
        crate::test_util::unique_temp_dir("metrics_access_tales").join("ledger.txt"),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/api/metrics", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    (url, task)
}

#[tokio::test]
async fn every_metrics_endpoint_requires_a_verified_google_admin() {
    let (verifier, key) = crate::google_auth::test_verifier();
    let (url, task) = serve(AuthContext {
        google: Some(verifier),
        npc_token: "test-npc-secret".into(),
        admin_emails: vec!["admin@example.com".into()],
    })
    .await;
    let client = reqwest::Client::new();
    let mut rejected = vec![
        (None, StatusCode::UNAUTHORIZED),
        (Some("test-npc-secret".into()), StatusCode::UNAUTHORIZED),
        (Some("malformed-token".into()), StatusCode::UNAUTHORIZED),
    ];
    for (field, value, status) in [
        ("email", json!("player@example.com"), StatusCode::FORBIDDEN),
        ("email", Value::Null, StatusCode::FORBIDDEN),
        ("email_verified", json!(false), StatusCode::FORBIDDEN),
        ("email_verified", Value::Null, StatusCode::FORBIDDEN),
        ("exp", json!(unix_now() - 3600), StatusCode::UNAUTHORIZED),
        ("aud", json!("another-client"), StatusCode::UNAUTHORIZED),
        (
            "iss",
            json!("https://attacker.example"),
            StatusCode::UNAUTHORIZED,
        ),
    ] {
        let mut payload = claims();
        payload[field] = value;
        rejected.push((Some(signed_token(&key, &payload)), status));
    }
    let mut forged_header = Header::new(Algorithm::HS256);
    forged_header.kid = Some("test-google-key".into());
    rejected.push((
        Some(
            encode(
                &forged_header,
                &claims(),
                &EncodingKey::from_secret(b"forged"),
            )
            .unwrap(),
        ),
        StatusCode::UNAUTHORIZED,
    ));
    let valid = signed_token(&key, &claims());
    let (prefix, signature) = valid.rsplit_once('.').unwrap();
    let replacement = if signature.starts_with('A') { 'B' } else { 'A' };
    rejected.push((
        Some(format!("{prefix}.{replacement}{}", &signature[1..])),
        StatusCode::UNAUTHORIZED,
    ));

    for endpoint in ENDPOINTS {
        let endpoint_url = format!("{url}/{endpoint}");
        for (token, expected) in &rejected {
            let mut request = client.get(&endpoint_url);
            if let Some(token) = token {
                request = request.bearer_auth(token);
            }
            let response = request.send().await.unwrap();
            assert_eq!(response.status(), *expected, "{endpoint}");
            assert_eq!(response.headers()["cache-control"], "no-store");
            assert!(matches!(
                response.text().await.unwrap().as_str(),
                "unauthorized" | "not an admin"
            ));
        }
        let head = client.head(&endpoint_url).send().await.unwrap();
        assert_eq!(head.status(), StatusCode::UNAUTHORIZED, "HEAD {endpoint}");
        let response = client
            .get(&endpoint_url)
            .bearer_auth(&valid)
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if *endpoint == "session" {
                StatusCode::NO_CONTENT
            } else {
                StatusCode::OK
            },
            "{endpoint}"
        );
        assert_eq!(response.headers()["cache-control"], "no-store");
    }
    task.abort();
}

#[tokio::test]
async fn missing_google_configuration_or_empty_admin_list_denies_access() {
    for google_enabled in [false, true] {
        let (verifier, key) = crate::google_auth::test_verifier();
        let (url, task) = serve(AuthContext {
            google: google_enabled.then_some(verifier),
            npc_token: "test-npc-secret".into(),
            admin_emails: vec![],
        })
        .await;
        let response = reqwest::Client::new()
            .get(format!("{url}/session"))
            .bearer_auth(signed_token(&key, &claims()))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            if google_enabled {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::UNAUTHORIZED
            }
        );
        task.abort();
    }
}

#[tokio::test]
async fn game_api_keeps_public_reads_and_existing_write_access() {
    let (verifier, key) = crate::google_auth::test_verifier();
    let access = Arc::new(AuthContext {
        google: Some(verifier),
        npc_token: "test-npc-secret".into(),
        admin_emails: vec!["admin@example.com".into()],
    });
    let router = Router::new()
        .route(
            "/game",
            get(|| async { StatusCode::NO_CONTENT }).post(|| async { StatusCode::NO_CONTENT }),
        )
        .layer(axum::middleware::from_fn_with_state(
            access,
            crate::api_auth::require_admin_for_writes,
        ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/game", listener.local_addr().unwrap());
    let task = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::new();
    assert_eq!(
        client.get(&url).send().await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        client.post(&url).send().await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    for token in ["test-npc-secret".to_owned(), signed_token(&key, &claims())] {
        assert_eq!(
            client
                .post(&url)
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
    }
    let mut player = claims();
    player["email"] = json!("player@example.com");
    assert_eq!(
        client
            .post(&url)
            .bearer_auth(signed_token(&key, &player))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    task.abort();
}
