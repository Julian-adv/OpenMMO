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
    "families",
    "combat-audit-targets",
    "network",
    "asset-traffic",
    "hardware",
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

async fn serve(access: AuthContext) -> String {
    let auth = Arc::new(
        AuthService::new(crate::test_util::unique_temp_dir("metrics_access").join("game.db"))
            .unwrap(),
    );
    serve_with(access, auth).await
}

async fn serve_with(access: AuthContext, auth: Arc<AuthService>) -> String {
    let router = metrics_router(
        Arc::new(make_test_game_state("metrics_access")),
        auth,
        Arc::new(access),
        crate::test_util::unique_temp_dir("metrics_access_tales").join("ledger.txt"),
        crate::traffic::TrafficMetrics::new(crate::traffic::Config {
            path: crate::test_util::unique_temp_dir("metrics_access_traffic").join("metrics.db"),
            interface: None,
            access_log: None,
            interval_seconds: 60,
        }),
        crate::hardware::HardwareMetrics::new(
            crate::test_util::unique_temp_dir("metrics_access_hardware").join("metrics.db"),
        ),
    );
    format!("{}/api/metrics", crate::test_util::serve(router).await)
}

fn admin_access(admin_emails: &[&str]) -> (AuthContext, EncodingKey) {
    let (verifier, key) = crate::google_auth::test_verifier();
    let access = AuthContext {
        google: Some(verifier),
        npc_token: "test-npc-secret".into(),
        admin_emails: admin_emails.iter().map(|email| email.to_string()).collect(),
    };
    (access, key)
}

async fn status_with_cookie(url: &str, cookie: &str) -> StatusCode {
    reqwest::Client::new()
        .get(url)
        .header("cookie", cookie)
        .send()
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn a_google_sign_in_starts_a_session_cookie_that_opens_every_endpoint_until_sign_out() {
    let (access, key) = admin_access(&["admin@example.com"]);
    let url = serve(access).await;
    let client = reqwest::Client::new();
    let session_url = format!("{url}/session");

    let response = client
        .post(&session_url)
        .bearer_auth(signed_token(&key, &claims()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let set_cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .to_owned();
    for attribute in [
        "Max-Age=2592000",
        "Path=/api/metrics",
        "HttpOnly",
        "Secure",
        "SameSite=Strict",
    ] {
        assert!(set_cookie.contains(attribute), "{set_cookie}");
    }
    let cookie = set_cookie.split(';').next().unwrap().to_owned();
    assert!(cookie.starts_with("dashboard_session="));

    for endpoint in ENDPOINTS {
        let response = client
            .get(format!("{url}/{endpoint}"))
            .header("cookie", format!("theme=dark; {cookie}"))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success(), "{endpoint}");
        assert_eq!(response.headers()["cache-control"], "no-store");
    }

    let renewal = client
        .post(&session_url)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(renewal.status(), StatusCode::UNAUTHORIZED);
    assert!(renewal.headers().get("set-cookie").is_none());

    let expired_bearer = client
        .get(&session_url)
        .header("cookie", &cookie)
        .bearer_auth("malformed-token")
        .send()
        .await
        .unwrap();
    assert_eq!(expired_bearer.status(), StatusCode::UNAUTHORIZED);

    for rejected in [
        "dashboard_session=test-npc-secret".to_string(),
        format!("dashboard_session={}", "0".repeat(64)),
        "dashboard_session=".to_string(),
        format!("other_{cookie}"),
    ] {
        assert_eq!(
            status_with_cookie(&session_url, &rejected).await,
            StatusCode::UNAUTHORIZED,
            "{rejected}"
        );
    }

    let signed_out = client
        .delete(&session_url)
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(signed_out.status(), StatusCode::NO_CONTENT);
    assert!(signed_out.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .starts_with("dashboard_session=; Max-Age=0;"));
    assert_eq!(
        status_with_cookie(&session_url, &cookie).await,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn a_session_outlives_a_restart_but_not_its_expiry_or_its_admin_rights() {
    let path = crate::test_util::unique_temp_dir("metrics_access_sessions").join("game.db");
    let auth = Arc::new(AuthService::new(path.clone()).unwrap());
    let now = unix_now();
    let live = auth
        .create_dashboard_session("Admin@example.com", now)
        .unwrap();
    let removed = auth
        .create_dashboard_session("former@example.com", now)
        .unwrap();
    let expired = auth
        .create_dashboard_session(
            "admin@example.com",
            now - crate::auth::DASHBOARD_SESSION_SECONDS,
        )
        .unwrap();
    let stored: String = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT group_concat(hex(token_hash)) FROM dashboard_sessions",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!stored.to_lowercase().contains(&live));
    assert_eq!(stored.split(',').count(), 3);
    drop(auth);

    let (access, _) = admin_access(&["admin@example.com"]);
    let restarted = Arc::new(AuthService::new(path.clone()).unwrap());
    let url = format!("{}/session", serve_with(access, restarted).await);
    for (token, expected) in [
        (live, StatusCode::NO_CONTENT),
        (expired, StatusCode::UNAUTHORIZED),
        (removed, StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            status_with_cookie(&url, &format!("dashboard_session={token}")).await,
            expected
        );
    }
}

#[tokio::test]
async fn every_metrics_endpoint_requires_a_verified_google_admin() {
    let (verifier, key) = crate::google_auth::test_verifier();
    let url = serve(AuthContext {
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
}

#[tokio::test]
async fn missing_google_configuration_or_empty_admin_list_denies_access() {
    for google_enabled in [false, true] {
        let (verifier, key) = crate::google_auth::test_verifier();
        let url = serve(AuthContext {
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
    let url = format!("{}/game", crate::test_util::serve(router).await);
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
}
