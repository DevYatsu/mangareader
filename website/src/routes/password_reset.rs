//! Forgot-password + reset-token pages.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Form,
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
    theme: &str,
    title: &str,
    content: maud::Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    Html(
        crate::templates::layout::base_layout(
            title,
            content,
            None,
            &cfg.css_overrides(),
            &cfg.site_title,
            user,
            theme,
        )
        .0,
    )
}

fn reset_form() -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Set a new password" }
            form method="post" class="auth-form" {
                label { "New password (min 8 chars)" input type="password" name="password" required=""; }
                button type="submit" { "Set password" }
            }
        }
    }
}

/// GET `/forgot` — no email in this app; direct the user to an administrator.
pub async fn forgot_page(
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
    page(
        &cfg,
        &theme,
        "Forgot password",
        html! {
            section class="auth-section" {
                h1 class="page-title" { "Forgot password" }
                p { "Password resets are handled by an administrator. Please contact the site administrator to request a reset link." }
                p { a href="/login" { "Back to login" } }
            }
        },
        user.as_ref(),
    )
}

/// GET `/reset/{token}` — always renders the form (no validity leak).
pub async fn reset_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(_token): Path<String>,
) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    page(&cfg, &theme, "Set a new password", reset_form(), user.as_ref())
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
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    let bad = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
                &theme,
                "Set a new password",
                html! { section class="auth-section" { p class="auth-error" { (msg) } } ({ reset_form() }) },
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
