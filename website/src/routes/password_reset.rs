//! Forgot-password + reset-token pages.

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
};
use maud::html;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{AuthUser, hash_password, now_utc_string},
};

/// Reset-link expiry: `now + 1 hour`, SQLite timestamp format.
pub fn reset_expires_at() -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::seconds(3600))
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

fn page(
    cfg: &crate::config::SiteConfig,
    title: &str,
    content: maud::Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    Html(
        crate::templates::layout::base_layout(
            title,
            content,
            None,
            &cfg.theme_overrides(),
            &cfg.site_title,
            user,
            crate::templates::layout::Nav::None,
        )
        .0,
    )
}

/// Shared markup for the initial render and the error re-render, so the two
/// cannot drift.
fn reset_form(action: &str, error: Option<&str>) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Set a new password" }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    form method="post" action=(action) {
                        div class="field" {
                            label class="label" { "New password (min 8 chars)" }
                            div class="control" {
                                input class="input" type="password" name="password" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Set password" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// GET `/forgot` — no email in this app; direct the user to an administrator.
pub async fn forgot_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    page(
        &cfg,
        "Forgot password",
        html! {
            div class="columns is-centered" {
                div class="column is-5-desktop is-6-tablet" {
                    div class="box" {
                        h1 class="title is-3" { "Forgot password" }
                        p class="content" {
                            "Password resets are issued by an administrator for this instance. "
                            "Ask an admin to generate a reset link for your account."
                        }
                        a class="button is-primary is-fullwidth" href="/login" { "Back to login" }
                    }
                }
            }
        },
        user.as_ref(),
    )
}

/// GET `/reset/{token}` — always renders the form (no validity leak).
pub async fn reset_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(token): Path<String>,
) -> Html<String> {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    page(
        &cfg,
        "Set a new password",
        reset_form(&format!("/reset/{token}"), None),
        user.as_ref(),
    )
}

#[derive(Deserialize)]
pub struct ResetForm {
    pub password: String,
}

/// POST `/reset/{token}` — consume token, set hash, revoke sessions.
pub async fn reset_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(token): Path<String>,
    Form(form): Form<ResetForm>,
) -> Response {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    let bad = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
                "Set a new password",
                reset_form(&format!("/reset/{token}"), Some(msg)),
                user.as_ref(),
            ),
        )
            .into_response()
    };
    if form.password.len() < 8 {
        return bad("This reset link is invalid or expired.");
    }
    // Consume first (marks used), hash outside the guard, then write.
    let user_id = {
        let db = state.db.lock().await;
        match mangaread_core::db::consume_password_reset(&db, &token, &now_utc_string()) {
            Ok(id) => id,
            Err(_) => return bad("This reset link is invalid or expired."),
        }
    };
    let hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return bad("This reset link is invalid or expired."),
    };
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_password(&db, user_id, &hash).is_err() {
            return bad("This reset link is invalid or expired.");
        }
        let _ = mangaread_core::db::delete_sessions_for_user(&db, user_id);
    }
    (
        StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, "/login".to_string())],
        "",
    )
        .into_response()
}
