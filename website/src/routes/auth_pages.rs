//! Login and logout handlers, plus the auth page shell shared with
//! `routes::signup` and `routes::password_reset`.
//!
//! Login failures all render the same generic message so registered emails
//! cannot be enumerated.

use axum::{
    Form,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Response},
};
use maud::html;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{
        AuthUser, lifetime_for_remember, new_token, session_expires_at, set_cookie_value,
        token_from_headers, verify_password,
    },
    oauth::Provider,
};

/// Human-readable provider name for the login-page buttons.
fn provider_label(p: &Provider) -> &'static str {
    match p {
        Provider::Google => "Google",
        Provider::Github => "GitHub",
        Provider::Discord => "Discord",
    }
}

/// "Continue with X" links for the configured providers; empty when none.
pub(crate) fn oauth_buttons(providers: &[Provider]) -> maud::Markup {
    html! {
        @if !providers.is_empty() {
            div class="buttons mt-5" {
                @for p in providers {
                    a class="button is-fullwidth" href=(format!("/auth/{}", p.key())) { "Continue with " (provider_label(p)) }
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    /// Checkbox sends `remember=on` only when checked; `Option` keeps the form
    /// parseable when the box is unchecked.
    pub remember: Option<String>,
}

pub(crate) fn page(
    cfg: &crate::config::SiteConfig,
    title: &str,
    content: maud::Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    crate::templates::layout::page(
        cfg,
        title,
        crate::templates::layout::Nav::None,
        user,
        content,
    )
}

fn login_form(error: Option<&str>, providers: &[Provider]) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Login" }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    form method="post" action="/login" {
                        div class="field" {
                            label class="label" { "Email" }
                            div class="control" {
                                input class="input" type="email" name="email" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Password" }
                            div class="control" {
                                input class="input" type="password" name="password" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                label class="checkbox" {
                                    input type="checkbox" name="remember" value="on";
                                    " Remember me (30 days)"
                                }
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Login" }
                            }
                        }
                    }
                    (oauth_buttons(providers))
                    div class="is-size-7 m-text-muted mt-5" {
                        p { a href="/register" { "Create an account" } }
                        p { a href="/forgot" { "Forgot password?" } }
                    }
                }
            }
        }
    }
}

pub(crate) fn redirect_with_cookie(location: &str, cookie: String) -> Response {
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, location.to_string()),
            (header::SET_COOKIE, cookie),
        ],
        "",
    )
        .into_response()
}

/// GET `/login` — render the login form.
pub async fn login_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    page(
        &cfg,
        "Login",
        login_form(None, &crate::oauth::configured_providers()),
        user.as_ref(),
    )
}

/// POST `/login` — verify credentials, open a session, redirect home.
pub async fn login_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Form(form): Form<LoginForm>,
) -> Response {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    let render_401 = |user: Option<&AuthUser>| {
        (
            StatusCode::UNAUTHORIZED,
            page(
                &cfg,
                "Login",
                login_form(
                    Some("Invalid email or password"),
                    &crate::oauth::configured_providers(),
                ),
                user,
            ),
        )
            .into_response()
    };

    // Lock only for the lookup; bcrypt verification runs without the guard so
    // concurrent requests are not serialized behind hashing.
    let found = {
        let db = state.db.lock().await;
        match mangaread_core::db::find_user_by_email(&db, form.email.trim()) {
            Ok(u) => (u.id, u.password_hash, u.banned_at.is_some()),
            // Identical message for unknown email — no user enumeration.
            Err(_) => return render_401(user.as_ref()),
        }
    };
    if !verify_password(&form.password, &found.1) {
        return render_401(user.as_ref());
    }
    // Revealed only after the password checks out, so the ban status of an
    // account is not visible to someone guessing at it.
    if found.2 {
        return (
            StatusCode::FORBIDDEN,
            page(
                &cfg,
                "Login",
                login_form(
                    Some("This account is suspended."),
                    &crate::oauth::configured_providers(),
                ),
                user.as_ref(),
            ),
        )
            .into_response();
    }
    let token = new_token();
    let max_age = lifetime_for_remember(form.remember.as_deref() == Some("on"));
    {
        let db = state.db.lock().await;
        if mangaread_core::db::insert_session(
            &db,
            found.0,
            &token,
            &session_expires_at(max_age),
            "web",
        )
        .is_err()
        {
            return render_401(user.as_ref());
        }
    }
    redirect_with_cookie("/", set_cookie_value(&token, max_age))
}

/// POST `/logout` — close the session, expire the cookie, redirect home.
pub async fn logout(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if let Some(token) = token_from_headers(&headers) {
        let db = state.db.lock().await;
        // Best-effort: logout always redirects home even if deletion fails.
        let _ = mangaread_core::db::delete_session(&db, &token);
    }
    redirect_with_cookie(
        "/",
        format!(
            "{}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
            crate::auth::SESSION_COOKIE
        ),
    )
}
