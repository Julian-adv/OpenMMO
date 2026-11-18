use crate::api_auth::{bearer_token, forbidden, unauthorized, verify_google_admin};
use crate::auth::{unix_now, AuthService, DASHBOARD_SESSION_SECONDS};
use crate::connection::AuthContext;
use crate::game_state::auth_db;
use axum::{
    extract::{Request, State},
    http::{
        header::{CACHE_CONTROL, COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware::Next,
    response::{IntoResponse, Response},
    Extension,
};
use std::sync::Arc;

const COOKIE_NAME: &str = "dashboard_session";
const COOKIE_ATTRIBUTES: &str = "Path=/api/metrics; HttpOnly; Secure; SameSite=Strict";

#[derive(Clone)]
pub struct DashboardAccess {
    pub context: Arc<AuthContext>,
    pub auth: Arc<AuthService>,
}

/// How the request proved it comes from an admin.
#[derive(Clone)]
pub enum DashboardAdmin {
    Google(String),
    Session,
}

fn session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE_NAME)?.strip_prefix('='))
        .map(str::to_owned)
}

async fn session_admin(
    access: &DashboardAccess,
    token: String,
) -> Result<DashboardAdmin, (StatusCode, String)> {
    let auth = access.auth.clone();
    let email = auth_db(move || auth.dashboard_session_email(&token, unix_now()))
        .await
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "unavailable".to_string()))?
        .ok_or_else(unauthorized)?;
    if !access.context.is_admin_email(&email) {
        return Err(forbidden());
    }
    Ok(DashboardAdmin::Session)
}

/// A Google ID token in Authorization decides alone; the session cookie counts only without one.
pub async fn require_dashboard_admin(
    State(access): State<DashboardAccess>,
    mut req: Request,
    next: Next,
) -> Response {
    let admin = match bearer_token(req.headers()) {
        Some(token) => verify_google_admin(&access.context, token)
            .await
            .map(DashboardAdmin::Google),
        None => match session_cookie(req.headers()) {
            Some(token) => session_admin(&access, token).await,
            None => Err(unauthorized()),
        },
    };
    let mut response = match admin {
        Ok(admin) => {
            req.extensions_mut().insert(admin);
            next.run(req).await
        }
        Err(error) => error.into_response(),
    };
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

fn set_cookie(value: &str, max_age: i64) -> [(axum::http::HeaderName, String); 1] {
    [(
        SET_COOKIE,
        format!("{COOKIE_NAME}={value}; Max-Age={max_age}; {COOKIE_ATTRIBUTES}"),
    )]
}

/// Only a fresh Google sign-in starts a session; a session cannot renew itself.
pub async fn start_session(
    State(access): State<DashboardAccess>,
    Extension(admin): Extension<DashboardAdmin>,
) -> Response {
    let DashboardAdmin::Google(email) = admin else {
        return unauthorized().into_response();
    };
    let auth = access.auth.clone();
    match auth_db(move || auth.create_dashboard_session(&email, unix_now())).await {
        Ok(token) => (
            StatusCode::NO_CONTENT,
            set_cookie(&token, DASHBOARD_SESSION_SECONDS),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub async fn end_session(State(access): State<DashboardAccess>, headers: HeaderMap) -> Response {
    if let Some(token) = session_cookie(&headers) {
        let auth = access.auth.clone();
        if auth_db(move || auth.delete_dashboard_session(&token))
            .await
            .is_err()
        {
            return StatusCode::SERVICE_UNAVAILABLE.into_response();
        }
    }
    (StatusCode::NO_CONTENT, set_cookie("", 0)).into_response()
}
