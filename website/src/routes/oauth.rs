//! OAuth login entry + callback for google/github/discord.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{AppendHeaders, Html, IntoResponse, Response},
};
use mangaread_core::domain::{NewUser, Role};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth::{lifetime_for_remember, new_token, session_expires_at, set_cookie_value, AuthUser},
    oauth::{self, Provider},
    AppState,
};

const STATE_COOKIE: &str = "oauth_state";

/// State cookie header value. Mirrors `auth.rs`' `MANGAREAD_COOKIE_SECURE`
/// decision so the state and session cookies share one convention.
fn state_cookie_value(value: &str, max_age_secs: i64) -> String {
    let secure = std::env::var("MANGAREAD_COOKIE_SECURE").as_deref() == Ok("1");
    state_cookie_with_secure(value, max_age_secs, secure)
}

/// Pure cookie builder; `secure` is passed in so the logic is testable without
/// mutating process environment variables.
fn state_cookie_with_secure(value: &str, max_age_secs: i64, secure: bool) -> String {
    let mut cookie = format!(
        "{STATE_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

fn error_page(cfg: &crate::config::SiteConfig, theme: &str, msg: &str, user: Option<&AuthUser>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(
            crate::templates::layout::base_layout(
                "Login",
                maud::html! { section class="auth-section" { p class="auth-error" { (msg) } p { a href="/login" { "Back to login" } } } },
                None,
                &cfg.css_overrides(),
                &cfg.site_title,
                user,
                theme,
            ).0,
        ),
    ).into_response()
}

/// GET `/auth/{provider}` — set a state cookie, redirect to the provider.
/// No state needed (pure redirect); `user` is accepted and ignored so logged-in
/// users can link-state identically — the callback links by email when matched.
pub async fn oauth_start(
    Path(provider_key): Path<String>,
    user: Option<AuthUser>,
) -> Response {
    let _ = user;
    let Some(p) = Provider::from_key(&provider_key) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((client_id, _)) = oauth::credentials(p) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let state_token = new_token();
    let url = oauth::authorize_url(&p, &oauth::base_url(), &state_token, &client_id);
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, url),
            (header::SET_COOKIE, state_cookie_value(&state_token, 600)),
        ],
        "",
    ).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery { code: Option<String>, state: Option<String>, error: Option<String> }

fn state_from_headers(headers: &HeaderMap) -> Option<String> {
    for raw in headers.get_all(axum::http::header::COOKIE).iter() {
        let Ok(s) = raw.to_str() else { continue };
        for part in s.split(';') {
            let part = part.trim();
            let Some((name, value)) = part.split_once('=') else { continue };
            if name.trim() == STATE_COOKIE { return Some(value.trim().to_string()); }
        }
    }
    None
}

/// GET `/auth/{provider}/callback` — verify state, exchange code, find-or-create, session, redirect home.
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Path(provider_key): Path<String>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
    user: Option<AuthUser>,
) -> Response {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    let fail = |msg: &str| error_page(&cfg, &theme, msg, user.as_ref());
    let Some(p) = Provider::from_key(&provider_key) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((client_id, client_secret)) = oauth::credentials(p) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if q.error.is_some() { return fail("Login with this provider was cancelled."); }
    let (Some(code), Some(returned_state)) = (q.code.as_deref(), q.state.as_deref()) else {
        return fail("Login with this provider failed.");
    };
    match state_from_headers(&headers) {
        Some(cookie_state) if cookie_state == returned_state => {}
        _ => return fail("Login with this provider failed."),
    }
    // Network calls run without the db guard held.
    let profile = match oauth::exchange_code(p, &client_id, &client_secret, code, &oauth::base_url()).await {
        Ok(token) => match oauth::fetch_profile(p, &token).await {
            Ok(profile) => profile,
            Err(_) => return fail("Login with this provider failed."),
        },
        Err(_) => return fail("Login with this provider failed."),
    };
    let email = match (profile.email.clone(), profile.email_verified) {
        (Some(e), true) => e,
        _ => return fail(&format!("could not verify an email address with {}", p.key())),
    };
    let marker = format!("OAUTH:{}:{}", p.key(), profile.provider_user_id);
    let name: String = profile.display_name.chars().take(32).collect();
    let name = if name.trim().is_empty() { "reader".to_string() } else { name };
    let db = state.db.lock().await;
    // 1. already linked → login
    if let Ok(u) = mangaread_core::db::find_user_by_oauth(&db, p.key(), &profile.provider_user_id) {
        return open_session(&db, u.id, &format!("oauth:{}", p.key()));
    }
    // 2. matching verified email → link then login
    if let Ok(u) = mangaread_core::db::find_user_by_email(&db, &email) {
        if mangaread_core::db::link_oauth_account(&db, u.id, p.key(), &profile.provider_user_id).is_err() {
            return fail("Login with this provider failed.");
        }
        return open_session(&db, u.id, &format!("oauth:{}", p.key()));
    }
    // 3. new account (bootstrap admin iff first user)
    let role = match mangaread_core::db::count_users(&db) {
        Ok(0) => Role::Admin,
        Ok(_) => Role::Member,
        Err(_) => return fail("Login with this provider failed."),
    };
    let id = match mangaread_core::db::create_user(&db, &NewUser { name, email, password_hash: marker, role }) {
        Ok(id) => id,
        Err(_) => return fail("Login with this provider failed."),
    };
    if mangaread_core::db::link_oauth_account(&db, id, p.key(), &profile.provider_user_id).is_err() {
        return fail("Login with this provider failed.");
    }
    open_session(&db, id, &format!("oauth:{}", p.key()))
}

/// Insert a remembered (30-day) session and redirect home with the cookie.
/// The state cookie is cleared alongside. Both cookies are emitted through
/// `AppendHeaders` because a raw `[(K, V); N]` array uses `HeaderMap::insert`,
/// which replaces prior values of the same header key — the second `SET_COOKIE`
/// would drop the session cookie.
/// `&MutexGuard<Connection>` coerces to `&Connection` via Deref at the call.
fn open_session(db: &tokio::sync::MutexGuard<'_, rusqlite::Connection>, user_id: i64, device: &str) -> Response {
    let max_age = lifetime_for_remember(true);
    let token = new_token();
    if mangaread_core::db::insert_session(db, user_id, &token, &session_expires_at(max_age), device).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    open_session_response(set_cookie_value(&token, max_age))
}

/// Build the 303-home response carrying the session cookie and the cleared
/// state cookie. Split out from [`open_session`] so the two-header behavior is
/// unit-testable without a live provider or database.
fn open_session_response(session_cookie: String) -> Response {
    (
        StatusCode::SEE_OTHER,
        AppendHeaders([
            (header::SET_COOKIE, session_cookie),
            (header::SET_COOKIE, state_cookie_value("", 0)),
            (header::LOCATION, "/".to_string()),
        ]),
        "",
    ).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: the response must carry BOTH `set-cookie` values, not one
    /// that overwrote the other.
    #[test]
    fn session_response_keeps_both_cookies() {
        let response = open_session_response(set_cookie_value("tok-1", 2592000));
        let cookies: Vec<&str> = response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(cookies.len(), 2, "expected two set-cookie headers: {cookies:?}");
        assert!(cookies.iter().any(|c| c.contains("mangaread_session=tok-1")));
        assert!(cookies.iter().any(|c| c.contains("oauth_state=;") && c.contains("Max-Age=0")));
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/");
    }

    #[test]
    fn state_cookie_secure_flag() {
        let insecure = state_cookie_with_secure("s", 600, false);
        assert!(insecure.contains("oauth_state=s"));
        assert!(insecure.contains("Max-Age=600"));
        assert!(!insecure.contains("Secure"));

        let secure = state_cookie_with_secure("s", 600, true);
        assert!(secure.contains("oauth_state=s"));
        assert!(secure.contains("Max-Age=600"));
        assert!(secure.ends_with("; Secure"));
    }

    #[test]
    fn clear_state_cookie_is_expired() {
        let cookie = state_cookie_value("", 0);
        assert!(cookie.contains("oauth_state=;"));
        assert!(cookie.contains("Max-Age=0"));
    }
}
