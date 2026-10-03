use axum::{
    extract::{ConnectInfo, Request, State},
    http::{
        header::{AUTHORIZATION, CACHE_CONTROL},
        HeaderMap, HeaderValue, Method, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{net::SocketAddr, sync::Arc};
use tracing::warn;

use crate::connection::{token_matches, AuthContext};

/// Public game reads; writes require an NPC token or Google admin.
pub async fn require_admin_for_writes(
    State(auth): State<Arc<AuthContext>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, String)> {
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return Ok(next.run(req).await);
    }

    let token = bearer_token(req.headers()).ok_or_else(|| {
        let ip = req
            .extensions()
            .get::<ConnectInfo<SocketAddr>>()
            .map(|peer| {
                crate::conn_limit::forwarded_client_ip(
                    peer.0,
                    req.headers()
                        .get("x-forwarded-for")
                        .and_then(|v| v.to_str().ok()),
                    req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()),
                )
            });
        warn!(
            ip = ip.map(tracing::field::display),
            method = %req.method(), path = req.uri().path(),
            "REST write rejected: missing bearer token"
        );
        unauthorized()
    })?;

    if token_matches(token, &auth.npc_token) {
        return Ok(next.run(req).await);
    }

    verify_google_admin(&auth, token).await?;
    Ok(next.run(req).await)
}

pub async fn require_google_admin(
    State(auth): State<Arc<AuthContext>>,
    req: Request,
    next: Next,
) -> Response {
    let result = match bearer_token(req.headers()) {
        Some(token) => verify_google_admin(&auth, token).await,
        None => Err(unauthorized()),
    };
    let mut response = match result {
        Ok(()) => next.run(req).await,
        Err(error) => error.into_response(),
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn verify_google_admin(auth: &AuthContext, token: &str) -> Result<(), (StatusCode, String)> {
    let verifier = auth.google.as_ref().ok_or_else(unauthorized)?;
    let claims = verifier.verify(token).await.map_err(|_| unauthorized())?;
    if !auth.is_admin(&claims) {
        return Err((StatusCode::FORBIDDEN, "not an admin".to_string()));
    }
    Ok(())
}

/// Extract the bearer credential for admin or player authentication.
pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
}

fn unauthorized() -> (StatusCode, String) {
    (StatusCode::UNAUTHORIZED, "unauthorized".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::instrument::WithSubscriber;

    #[tokio::test]
    async fn missing_bearer_logs_proxy_ip_method_and_path_without_query_credentials() {
        let auth = Arc::new(AuthContext {
            google: None,
            npc_token: "npc-secret".into(),
            admin_emails: vec![],
        });
        let (subscriber, buffer) = crate::test_util::capture_logs();
        let dispatch = tracing::Dispatch::new(subscriber);
        let app = axum::Router::new()
            .route("/api/v1/execute", axum::routing::post(|| async { "ok" }))
            .layer(axum::middleware::from_fn_with_state(
                auth,
                require_admin_for_writes,
            ))
            .layer(axum::middleware::from_fn(
                move |req: Request, next: Next| {
                    let dispatch = dispatch.clone();
                    async move { next.run(req).with_subscriber(dispatch).await }
                },
            ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .await
            .unwrap();
        });
        let response = reqwest::Client::new()
            .post(format!(
                "http://{address}/api/v1/execute?token=private-token"
            ))
            .header("x-forwarded-for", "192.0.2.1, 203.0.113.7")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let logs = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert!(logs.contains("REST write rejected"), "{logs}");
        assert!(logs.contains("ip=203.0.113.7"), "{logs}");
        assert!(logs.contains("method=POST"), "{logs}");
        assert!(logs.contains("path=\"/api/v1/execute\""), "{logs}");
        assert!(!logs.contains("private-token"), "{logs}");
        server.abort();
    }
}
