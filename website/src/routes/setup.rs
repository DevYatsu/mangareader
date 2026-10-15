//! First-run setup: the instance owner claims the admin account.
//!
//! On boot with zero users, [`arm_if_needed`] mints a one-time token and logs
//! it. Until setup completes, [`setup_gate`] sends every request to `/setup`,
//! so no visitor can register or log in before the owner. This replaces the
//! old "first registered user becomes admin" rule, which let any visitor of a
//! fresh public instance win the race (bootstrap-secret pattern, like
//! Jenkins' `initialAdminPassword`).

use axum::{
    Form,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Json, Redirect, Response},
};
use mangaread_core::domain::{NewUser, Role};
use maud::html;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{
        REMEMBER_SESSION_MAX_AGE, hash_password, new_token, session_expires_at, set_cookie_value,
    },
};

/// Mint and log a setup token when the instance has no users yet.
/// Returns the token so callers (tests) can complete setup.
pub async fn arm_if_needed(state: &AppState) -> Option<String> {
    let empty = {
        let db = state.db.lock().await;
        mangaread_core::db::count_users(&db).unwrap_or(1) == 0
    };
    if !empty {
        return None;
    }
    let token = new_token();
    tracing::warn!(
        "no accounts yet: open {}/setup and enter setup token {token}",
        crate::oauth::base_url()
    );
    *state.setup_token.lock().await = Some(token.clone());
    Some(token)
}

/// Paths reachable while setup is pending.
fn allowed_during_setup(path: &str) -> bool {
    path == "/setup" || path == "/health" || path == "/favicon.ico" || path.starts_with("/static/")
}

/// Middleware: while a setup token is pending, pages redirect to `/setup`
/// and API calls get `503`.
pub async fn setup_gate(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let pending = state.setup_token.lock().await.is_some();
    let path = req.uri().path();
    if !pending || allowed_during_setup(path) {
        return next.run(req).await;
    }
    if path.starts_with("/api/") {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"ok": false, "error": "setup pending"})),
        )
            .into_response();
    }
    Redirect::to("/setup").into_response()
}

/// Compare secrets without an early exit, so response timing does not leak
/// how many leading characters of a guess were right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn setup_form(error: Option<&str>) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Set up your library" }
                    p class="content m-text-muted" {
                        "Create the admin account. The setup token is printed in the server log "
                        "(" code { "docker compose logs mangaread" } ")."
                    }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    form method="post" action="/setup" {
                        div class="field" {
                            label class="label" { "Setup token" }
                            div class="control" {
                                input class="input" type="text" name="token" autocomplete="off" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Name" }
                            div class="control" {
                                input class="input" type="text" name="name" maxlength="32" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Email" }
                            div class="control" {
                                input class="input" type="email" name="email" required="";
                            }
                        }
                        div class="field" {
                            label class="label" { "Password (min 8 chars)" }
                            div class="control" {
                                input class="input" type="password" name="password" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Create admin account" }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn render(state: &AppState, status: StatusCode, error: Option<&str>) -> Response {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    (
        status,
        crate::templates::layout::page(
            &cfg,
            "Setup",
            crate::templates::layout::Nav::None,
            None,
            setup_form(error),
        ),
    )
        .into_response()
}

/// GET `/setup` — the admin-claim form; redirects home once setup is done.
pub async fn setup_page(State(state): State<Arc<AppState>>) -> Response {
    if state.setup_token.lock().await.is_none() {
        return Redirect::to("/").into_response();
    }
    render(&state, StatusCode::OK, None).await
}

#[derive(Deserialize)]
pub struct SetupForm {
    pub token: String,
    pub name: String,
    pub email: String,
    pub password: String,
}

/// POST `/setup` — verify the token, create the first admin, log them in.
pub async fn setup_submit(
    State(state): State<Arc<AppState>>,
    Form(form): Form<SetupForm>,
) -> Response {
    let token_ok = match state.setup_token.lock().await.as_deref() {
        None => return Redirect::to("/login").into_response(),
        Some(expected) => constant_time_eq(form.token.trim().as_bytes(), expected.as_bytes()),
    };
    if !token_ok {
        return render(
            &state,
            StatusCode::FORBIDDEN,
            Some("That setup token is not valid."),
        )
        .await;
    }
    let name = form.name.trim().to_string();
    let email = form.email.trim().to_string();
    let invalid = if name.is_empty() || name.chars().count() > 32 {
        Some("Name must be 1-32 characters.")
    } else if !(email.contains('@') && email.contains('.')) {
        Some("Enter a valid email address.")
    } else if form.password.len() < 8 {
        Some("Password must be at least 8 characters.")
    } else {
        None
    };
    if let Some(msg) = invalid {
        return render(&state, StatusCode::BAD_REQUEST, Some(msg)).await;
    }
    // bcrypt runs outside the DB lock so it does not serialize other requests.
    let Ok(password_hash) = hash_password(&form.password) else {
        return render(
            &state,
            StatusCode::INTERNAL_SERVER_ERROR,
            Some("Could not create the account."),
        )
        .await;
    };
    let session = new_token();
    {
        let db = state.db.lock().await;
        let created = mangaread_core::db::create_first_admin(
            &db,
            &NewUser {
                name,
                email: email.clone(),
                password_hash,
                role: Role::Admin,
            },
        );
        let id = match created {
            Ok(Some(id)) => id,
            // Another submission won the race; setup is over either way.
            Ok(None) => {
                *state.setup_token.lock().await = None;
                return Redirect::to("/login").into_response();
            }
            Err(_) => {
                drop(db);
                return render(
                    &state,
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Some("Could not create the account."),
                )
                .await;
            }
        };
        if mangaread_core::db::insert_session(
            &db,
            id,
            &session,
            &session_expires_at(REMEMBER_SESSION_MAX_AGE),
            "web",
        )
        .is_err()
        {
            *state.setup_token.lock().await = None;
            return Redirect::to("/login").into_response();
        }
    }
    *state.setup_token.lock().await = None;
    tracing::info!("setup complete: admin account created for {email}");
    // Email is the next step: without it, sign-up stays closed.
    crate::routes::auth_pages::redirect_with_cookie(
        "/admin/email?setup=1",
        set_cookie_value(&session, REMEMBER_SESSION_MAX_AGE),
    )
}

#[cfg(test)]
mod tests {
    use crate::test_support::{app_with_admin, test_app};
    use axum::http::StatusCode;

    #[tokio::test]
    async fn fresh_instance_sends_every_page_to_setup() {
        let app = test_app().await;
        for path in ["/", "/series", "/register", "/login", "/admin"] {
            let res = app.get(path, None).await;
            assert_eq!(res.status, StatusCode::SEE_OTHER, "{path}");
            assert_eq!(res.location.as_deref(), Some("/setup"), "{path}");
        }
        assert_eq!(app.get("/health", None).await.status, StatusCode::OK);
        assert_eq!(app.get("/setup", None).await.status, StatusCode::OK);
        let api = app.get("/api/sync-status", None).await;
        assert_eq!(api.status, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn wrong_token_is_refused_and_creates_nobody() {
        let app = test_app().await;
        let res = app
            .post_form(
                "/setup",
                &[
                    ("token", "guess"),
                    ("name", "Mallory"),
                    ("email", "m@example.com"),
                    ("password", "password1"),
                ],
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
        assert!(res.session.is_none());
        let db = app.state.db.lock().await;
        assert_eq!(mangaread_core::db::count_users(&db).unwrap(), 0);
    }

    #[tokio::test]
    async fn correct_token_creates_admin_and_closes_setup() {
        let (app, cookie) = app_with_admin().await;
        {
            let db = app.state.db.lock().await;
            let owner = mangaread_core::db::find_user_by_email(&db, "owner@example.com").unwrap();
            assert_eq!(owner.role, mangaread_core::domain::Role::Admin);
        }
        assert_eq!(
            app.get("/admin", Some(&cookie)).await.status,
            StatusCode::OK
        );
        assert_eq!(app.get("/", None).await.status, StatusCode::OK);
        // Setup cannot be replayed once complete.
        let again = app.get("/setup", None).await;
        assert_eq!(again.location.as_deref(), Some("/"));
    }

    #[tokio::test]
    async fn self_registration_after_setup_creates_a_member() {
        let (app, _) = app_with_admin().await;
        // Direct mode keeps this test independent of the email flow.
        app.set(crate::email::ALLOW_UNVERIFIED_KEY, "1").await;
        let res = app
            .post_form(
                "/register",
                &[
                    ("name", "Reader"),
                    ("email", "reader@example.com"),
                    ("password", "password1"),
                ],
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::SEE_OTHER);
        let db = app.state.db.lock().await;
        let u = mangaread_core::db::find_user_by_email(&db, "reader@example.com").unwrap();
        assert_eq!(u.role, mangaread_core::domain::Role::Member);
    }

    #[tokio::test]
    async fn existing_instance_is_never_gated() {
        let app = test_app().await;
        app.member("first@example.com").await;
        // Simulate a reboot: arming is a no-op once a user exists.
        *app.state.setup_token.lock().await = None;
        assert!(super::arm_if_needed(&app.state).await.is_none());
        assert_eq!(app.get("/", None).await.status, StatusCode::OK);
    }

    #[test]
    fn constant_time_eq_matches_only_identical_input() {
        assert!(super::constant_time_eq(b"abc", b"abc"));
        assert!(!super::constant_time_eq(b"abc", b"abd"));
        assert!(!super::constant_time_eq(b"abc", b"ab"));
    }
}
