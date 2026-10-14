//! Forgot-password + reset-token pages.
//!
//! Reset links come from three places: `/forgot` (self-service, needs SMTP),
//! `/register` with a known address, and admin-issued links. All of them are
//! hashed `email_tokens` rows with purpose `reset`.

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
    auth::{AuthUser, hash_password, new_token, now_utc_string},
    email::{MailSettings, token_hash},
    rate_limit::ClientIp,
    routes::signup::{MAILS_PER_ADDRESS, MAILS_PER_IP},
};
use mangaread_core::db::TokenPurpose;

const HOUR: std::time::Duration = std::time::Duration::from_secs(3600);

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
    crate::templates::layout::page(
        cfg,
        title,
        crate::templates::layout::Nav::None,
        user,
        content,
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

fn forgot_form(error: Option<&str>) -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Forgot password" }
                    @if let Some(msg) = error {
                        div class="notification is-danger is-light" { (msg) }
                    }
                    p class="content m-text-muted" { "Enter your account's email and we'll send a reset link." }
                    form method="post" action="/forgot" {
                        div class="field" {
                            label class="label" { "Email" }
                            div class="control" {
                                input class="input" type="email" name="email" autocomplete="email" required="";
                            }
                        }
                        div class="field" {
                            div class="control" {
                                button type="submit" class="button is-primary is-fullwidth" { "Send reset link" }
                            }
                        }
                    }
                    div class="is-size-7 m-text-muted mt-5" {
                        p { a href="/login" { "Back to login" } }
                    }
                }
            }
        }
    }
}

fn ask_admin() -> maud::Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { "Forgot password" }
                    p class="content" {
                        "This library has not set up email yet. "
                        "Ask an admin to generate a reset link for your account."
                    }
                    a class="button is-primary is-fullwidth" href="/login" { "Back to login" }
                }
            }
        }
    }
}

/// GET `/forgot` — a reset-by-email form when SMTP is configured, otherwise
/// directions to ask an admin.
pub async fn forgot_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let (cfg, mail) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            MailSettings::load(&db).is_some(),
        )
    };
    let content = if mail { forgot_form(None) } else { ask_admin() };
    page(&cfg, "Forgot password", content, user.as_ref())
}

#[derive(Deserialize)]
pub struct ForgotForm {
    pub email: String,
}

/// POST `/forgot` — mail a reset link when the address has an account.
/// The reply is identical either way, so it reveals nothing about members.
pub async fn forgot_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    ClientIp(ip): ClientIp,
    Form(form): Form<ForgotForm>,
) -> Response {
    let email = form.email.trim().to_string();
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    if !crate::routes::signup::valid_email(&email) {
        return (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
                "Forgot password",
                forgot_form(Some("Enter a valid email address.")),
                user.as_ref(),
            ),
        )
            .into_response();
    }
    if !state
        .rate_limiter
        .allow(&format!("ip:{ip}"), MAILS_PER_IP, HOUR)
    {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            page(
                &cfg,
                "Forgot password",
                forgot_form(Some("Too many attempts. Please try again in an hour.")),
                user.as_ref(),
            ),
        )
            .into_response();
    }
    if state
        .rate_limiter
        .allow(&format!("mail:{email}"), MAILS_PER_ADDRESS, HOUR)
    {
        send_reset_mail(&state, &email).await;
    }
    page(
        &cfg,
        "Check your inbox",
        crate::routes::signup::check_inbox("Check your inbox"),
        user.as_ref(),
    )
    .into_response()
}

/// Mail a reset link if `email` belongs to an account; silent otherwise.
async fn send_reset_mail(state: &AppState, email: &str) {
    let token = new_token();
    let (settings, site_title) = {
        let db = state.db.lock().await;
        let Some(settings) = MailSettings::load(&db) else {
            return;
        };
        if mangaread_core::db::find_user_by_email(&db, email).is_err() {
            return;
        }
        if let Err(e) = mangaread_core::db::create_email_token(
            &db,
            &token_hash(&token),
            email,
            TokenPurpose::Reset,
            &reset_expires_at(),
        ) {
            tracing::error!("forgot: could not store token: {e}");
            return;
        }
        (settings, crate::config::site_config(&db).site_title)
    };
    let body = format!(
        "Set a new password for your {site_title} account (valid for 1 hour):\n{}/reset/{token}\n\n\
         If you didn't ask for this, you can ignore this email.",
        crate::oauth::base_url()
    );
    let subject = format!("Reset your {site_title} password");
    if let Err(e) = state.mailer.send(&settings, email, &subject, &body).await {
        tracing::warn!("forgot: mail to {email} failed: {e}");
    }
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
        let email = mangaread_core::db::consume_email_token(
            &db,
            &token_hash(&token),
            TokenPurpose::Reset,
            &now_utc_string(),
        )
        .ok()
        .flatten();
        match email.and_then(|e| mangaread_core::db::find_user_by_email(&db, &e).ok()) {
            Some(u) => u.id,
            None => return bad("This reset link is invalid or expired."),
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
