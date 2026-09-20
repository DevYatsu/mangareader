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
    mail,
    oauth::base_url,
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

fn forgot_form() -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Forgot password" }
            form method="post" action="/forgot" class="auth-form" {
                label { "Email" input type="email" name="email" required=""; }
                button type="submit" { "Send reset link" }
            }
            p { a href="/login" { "Back to login" } }
        }
    }
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

/// GET `/forgot`.
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
    page(&cfg, &theme, "Forgot password", forgot_form(), user.as_ref())
}

#[derive(Deserialize)]
pub struct ForgotForm {
    pub email: String,
}

/// POST `/forgot` — identical success text whether the email exists or not.
pub async fn forgot_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Form(form): Form<ForgotForm>,
) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, user.as_ref()),
        )
    };
    let done = || {
        page(
            &cfg,
            &theme,
            "Forgot password",
            html! { section class="auth-section" { p { "If an account exists for that email, a reset link is on its way." } } },
            user.as_ref(),
        )
    };
    let email = form.email.trim().to_string();
    let found = {
        let db = state.db.lock().await;
        mangaread_core::db::find_user_by_email(&db, &email)
            .ok()
            .map(|u| (u.id, u.email))
    };
    let Some((user_id, to)) = found else {
        return done();
    };
    let token = crate::auth::new_token();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::create_password_reset(&db, user_id, &token, &reset_expires_at())
            .is_err()
        {
            return done();
        }
    }
    if mail::smtp_configured() {
        let link = format!("{}/reset/{token}", base_url());
        // Best-effort: the success text renders either way.
        let _ = mail::send_reset_email(to, link).await;
    }
    done()
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
