use axum::{
    extract::{Request, State},
    http::{
        header::{AUTHORIZATION, CACHE_CONTROL},
        HeaderMap, HeaderValue, Method, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;
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
        warn!("REST write rejected: missing bearer token");
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
