//! Login, registration, and logout handlers.
//!
//! The first-ever registered user is bootstrapped as `Admin`; everyone else
//! is a `Member`. Registration failures all render the same generic message
//! so duplicate emails cannot be enumerated.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Form,
};
use maud::html;
use mangaread_core::domain::{NewUser, Role};
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth::{
        hash_password, lifetime_for_remember, new_token, session_expires_at, set_cookie_value,
        token_from_headers, verify_password, AuthUser, REMEMBER_SESSION_MAX_AGE,
    },
    AppState,
};

#[derive(Deserialize)]
pub struct RegisterForm {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    pub email: String,
    pub password: String,
    /// Checkbox sends `remember=on` only when checked; `Option` keeps the form
    /// parseable when the box is unchecked.
    pub remember: Option<String>,
}

fn page(
    cfg: &crate::config::SiteConfig,
    theme: &str,
    title: &str,
    content: maud::Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    let banner: Option<String> = None;
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            title,
            content,
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user,
            theme,
        )
        .0,
    )
}

fn login_form(error: Option<&str>) -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Login" }
            @if let Some(msg) = error {
                p class="auth-error" { (msg) }
            }
            form method="post" action="/login" class="auth-form" {
                label { "Email" input type="email" name="email" required=""; }
                label { "Password" input type="password" name="password" required=""; }
                label { input type="checkbox" name="remember" value="on"; " Remember me (30 days)" }
                button type="submit" { "Login" }
            }
            p { a href="/register" { "Create an account" } }
        }
    }
}

fn register_form(error: Option<&str>) -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Register" }
            @if let Some(msg) = error {
                p class="auth-error" { (msg) }
            }
            form method="post" action="/register" class="auth-form" {
                label { "Name" input type="text" name="name" maxlength="32" required=""; }
                label { "Email" input type="email" name="email" required=""; }
                label { "Password (min 8 chars)" input type="password" name="password" required=""; }
                button type="submit" { "Register" }
            }
            p { a href="/login" { "Already have an account? Login" } }
        }
    }
}

fn redirect_with_cookie(location: &str, cookie: String) -> Response {
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
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    page(&cfg, &theme, "Login", login_form(None), user.as_ref())
}

/// POST `/login` — verify credentials, open a session, redirect home.
pub async fn login_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Form(form): Form<LoginForm>,
) -> Response {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    let render_401 = |user: Option<&AuthUser>| {
        (
            StatusCode::UNAUTHORIZED,
            page(&cfg, &theme, "Login", login_form(Some("Invalid email or password")), user),
        )
            .into_response()
    };

    // Lock only for the lookup; bcrypt verification runs without the guard so
    // concurrent requests are not serialized behind hashing.
    let found = {
        let db = state.db.lock().await;
        match mangaread_core::db::find_user_by_email(&db, form.email.trim()) {
            Ok(u) => (u.id, u.password_hash),
            // Identical message for unknown email — no user enumeration.
            Err(_) => return render_401(user.as_ref()),
        }
    };
    if !verify_password(&form.password, &found.1) {
        return render_401(user.as_ref());
    }
    let token = new_token();
    let max_age = lifetime_for_remember(form.remember.as_deref() == Some("on"));
    {
        let db = state.db.lock().await;
        if mangaread_core::db::insert_session(&db, found.0, &token, &session_expires_at(max_age), "web")
            .is_err()
        {
            return render_401(user.as_ref());
        }
    }
    redirect_with_cookie("/", set_cookie_value(&token, max_age))
}

/// GET `/register` — render the registration form.
pub async fn register_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    page(&cfg, &theme, "Register", register_form(None), user.as_ref())
}

/// POST `/register` — validate, create the user, open a session, redirect home.
pub async fn register_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Form(form): Form<RegisterForm>,
) -> Response {
    let name = form.name.trim().to_string();
    let email = form.email.trim().to_string();

    let invalid: Option<&str> = if name.is_empty() || name.len() > 32 {
        Some("Name must be 1-32 characters.")
    } else if !(email.contains('@') && email.contains('.')) {
        Some("Enter a valid email address.")
    } else if form.password.len() < 8 {
        Some("Password must be at least 8 characters.")
    } else {
        None
    };
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    if let Some(msg) = invalid {
        return (
            StatusCode::BAD_REQUEST,
            page(&cfg, &theme, "Register", register_form(Some(msg)), user.as_ref()),
        )
            .into_response();
    }

    // Every failure below renders the same message — no user enumeration
    // (duplicate email surfaces as a raw rusqlite error otherwise).
    let render_failed = |user: Option<&AuthUser>| {
        (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
                &theme,
                "Register",
                register_form(Some("could not create account")),
                user,
            ),
        )
            .into_response()
    };

    // Lock only for the count; bcrypt hashing runs without the guard so
    // concurrent requests are not serialized behind hashing.
    let role = {
        let db = state.db.lock().await;
        match mangaread_core::db::count_users(&db) {
            Ok(0) => Role::Admin,
            Ok(_) => Role::Member,
            Err(_) => return render_failed(user.as_ref()),
        }
    };
    let is_bootstrap_admin = role == Role::Admin;
    let password_hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return render_failed(user.as_ref()),
    };
    let token = new_token();
    {
        let db = state.db.lock().await;
        let id = match mangaread_core::db::create_user(
            &db,
            &NewUser {
                name: name.clone(),
                email: email.clone(),
                password_hash,
                role,
            },
        ) {
            Ok(id) => id,
            Err(_) => return render_failed(user.as_ref()),
        };
        if is_bootstrap_admin {
            tracing::warn!("first registered user bootstrapped as admin: {email}");
        }
        if mangaread_core::db::insert_session(
            &db,
            id,
            &token,
            &session_expires_at(REMEMBER_SESSION_MAX_AGE),
            "web",
        )
        .is_err()
        {
            return render_failed(user.as_ref());
        }
    }
    redirect_with_cookie("/", set_cookie_value(&token, REMEMBER_SESSION_MAX_AGE))
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
