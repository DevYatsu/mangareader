// website/src/auth.rs
use axum::{
    extract::{FromRequestParts, OptionalFromRequestParts},
    http::{HeaderMap, StatusCode, header::COOKIE, request::Parts},
    response::{IntoResponse, Json, Redirect, Response},
};
use mangaread_core::{db, domain::Role, error::Error};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::AppState;

pub const SESSION_COOKIE: &str = "mangaread_session";
pub const SHORT_SESSION_MAX_AGE: i64 = 24 * 3600; // 24 hours (default)
pub const REMEMBER_SESSION_MAX_AGE: i64 = 30 * 24 * 3600; // 30 days (remember-me)

pub fn hash_password(pw: &str) -> Result<String, String> {
    bcrypt::hash(pw, bcrypt::DEFAULT_COST).map_err(|e| e.to_string())
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    bcrypt::verify(pw, hash).unwrap_or(false)
}

pub fn new_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Current UTC time formatted `%Y-%m-%d %H:%M:%S` (SQLite session lookup/expiry format).
pub fn now_utc_string() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

/// Expiry timestamp for a fresh session (`now + max_age_secs`), SQLite format.
pub fn session_expires_at(max_age_secs: i64) -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::seconds(max_age_secs))
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

/// `true` → 30-day session, `false` → 24-hour session.
pub fn lifetime_for_remember(remember: bool) -> i64 {
    if remember {
        REMEMBER_SESSION_MAX_AGE
    } else {
        SHORT_SESSION_MAX_AGE
    }
}

pub fn set_cookie_value(token: &str, max_age_secs: i64) -> String {
    let mut value =
        format!("{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}");
    if std::env::var("MANGAREAD_COOKIE_SECURE").as_deref() == Ok("1") {
        value.push_str("; Secure");
    }
    value
}

pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    for raw in headers.get_all(COOKIE).iter() {
        let Ok(s) = raw.to_str() else { continue };
        for part in s.split(';') {
            let part = part.trim();
            let Some((name, value)) = part.split_once('=') else {
                continue;
            };
            if name.trim() == SESSION_COOKIE {
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

pub struct AuthUser {
    pub id: i64,
    pub name: String,
    pub role: Role,
}

/// Rejection for missing/invalid auth: browsers (explicit `text/html` in
/// `Accept`) get a redirect to `/login`; API clients get `401` JSON.
fn unauthenticated(parts: &Parts) -> Response {
    let wants_html = parts
        .headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/html"));
    if wants_html {
        Redirect::to("/login").into_response()
    } else {
        (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"ok": false, "error": "login required"})),
        )
            .into_response()
    }
}

async fn auth_with_db(
    parts: &mut Parts,
    db_mutex: &Mutex<rusqlite::Connection>,
) -> Result<AuthUser, Response> {
    let token = token_from_headers(&parts.headers);
    let Some(token) = token else {
        return Err(unauthenticated(parts));
    };
    let now = now_utc_string();
    let user = {
        let conn = db_mutex.lock().await;
        db::find_user_by_session(&conn, &token, &now)
    };
    match user {
        Ok(user) => Ok(AuthUser {
            id: user.id,
            name: user.name,
            role: user.role,
        }),
        Err(Error::NotFound(_)) => Err(unauthenticated(parts)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("session lookup failed: {e}"),
        )
            .into_response()),
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        auth_with_db(parts, &state.db).await
    }
}

impl FromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Self, Self::Rejection> {
        auth_with_db(parts, &state.db).await
    }
}

/// Allow `Option<AuthUser>` as an extractor (public pages render nav session
/// state instead of redirecting): missing/invalid session yields `None`.
impl OptionalFromRequestParts<Arc<AppState>> for AuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &Arc<AppState>,
    ) -> Result<Option<Self>, Self::Rejection> {
        Ok(auth_with_db(parts, &state.db).await.ok())
    }
}

pub fn require_role(user: &AuthUser, min: Role) -> Result<(), StatusCode> {
    if user.role.at_least(&min) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn lifetimes_differ() {
        assert_eq!(lifetime_for_remember(false), SHORT_SESSION_MAX_AGE);
        assert_eq!(lifetime_for_remember(true), REMEMBER_SESSION_MAX_AGE);
        assert!(REMEMBER_SESSION_MAX_AGE > SHORT_SESSION_MAX_AGE);
        let cookie = set_cookie_value("t", SHORT_SESSION_MAX_AGE);
        assert!(cookie.contains("Max-Age=86400"));
    }

    #[test]
    fn cookie_parse_exact_name() {
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            HeaderValue::from_str("a=1; mangaread_session=tok-9; b=2").unwrap(),
        );
        assert_eq!(token_from_headers(&headers).as_deref(), Some("tok-9"));
    }
}
