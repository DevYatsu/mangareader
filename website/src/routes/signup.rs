//! Open registration, verified by an emailed link (double opt-in).
//!
//! `/register` asks only for an email address and always answers "check your
//! inbox", whether or not the address already has an account, so the form
//! cannot be used to enumerate members. The emailed link (`/register/{token}`)
//! completes the account. Without SMTP, the admin decides between closed
//! sign-up (default) and unverified direct sign-up; see [`SignupMode`].

use axum::{
    Form,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mangaread_core::db::TokenPurpose;
use mangaread_core::domain::{NewUser, Role};
use maud::{Markup, html};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;

use crate::{
    AppState,
    auth::{
        AuthUser, REMEMBER_SESSION_MAX_AGE, hash_password, new_token, now_utc_string,
        session_expires_at, set_cookie_value,
    },
    email::{MailSettings, SignupMode, signup_mode, token_hash},
    rate_limit::ClientIp,
    routes::auth_pages::{oauth_buttons, page, redirect_with_cookie},
};

/// Sign-up links stay valid for 24 hours.
const SIGNUP_TTL_SECS: i64 = 24 * 3600;
/// Reset links (also sent when a known address signs up again) last 1 hour.
pub const RESET_TTL_SECS: i64 = 3600;
const HOUR: Duration = Duration::from_secs(3600);
/// Mails per address per hour; protects inboxes from being spammed.
pub const MAILS_PER_ADDRESS: usize = 3;
/// Mail-triggering requests per client IP per hour.
pub const MAILS_PER_IP: usize = 10;

/// Shared address check for sign-up and reset forms.
pub fn valid_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !email.chars().any(char::is_whitespace)
}

/// Name and password rules shared by every account-creating form.
fn validate_profile(name: &str, password: &str) -> Option<&'static str> {
    if name.is_empty() || name.chars().count() > 32 {
        Some("Name must be 1-32 characters.")
    } else if password.len() < 8 {
        Some("Password must be at least 8 characters.")
    } else {
        None
    }
}

fn card(title: &str, body: Markup) -> Markup {
    html! {
        div class="columns is-centered" {
            div class="column is-5-desktop is-6-tablet" {
                div class="box" {
                    h1 class="title is-3" { (title) }
                    (body)
                }
            }
        }
    }
}

fn error_note(error: Option<&str>) -> Markup {
    html! {
        @if let Some(msg) = error {
            div class="notification is-danger is-light" { (msg) }
        }
    }
}

fn footer_links() -> Markup {
    html! {
        div class="is-size-7 m-text-muted mt-5" {
            p { a href="/login" { "Already have an account? Login" } }
        }
    }
}

fn email_form(error: Option<&str>) -> Markup {
    let providers = crate::oauth::configured_providers();
    card(
        "Create an account",
        html! {
            (error_note(error))
            p class="content m-text-muted" { "We'll email you a link to finish signing up." }
            form method="post" action="/register" {
                div class="field" {
                    label class="label" { "Email" }
                    div class="control" {
                        input class="input" type="email" name="email" autocomplete="email" required="";
                    }
                }
                div class="field" {
                    div class="control" {
                        button type="submit" class="button is-primary is-fullwidth" { "Send sign-up link" }
                    }
                }
            }
            (oauth_buttons(&providers))
            (footer_links())
        },
    )
}

fn direct_form(error: Option<&str>) -> Markup {
    let providers = crate::oauth::configured_providers();
    card(
        "Create an account",
        html! {
            (error_note(error))
            form method="post" action="/register" {
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
                        button type="submit" class="button is-primary is-fullwidth" { "Register" }
                    }
                }
            }
            (oauth_buttons(&providers))
            (footer_links())
        },
    )
}

fn closed_notice() -> Markup {
    let providers = crate::oauth::configured_providers();
    card(
        "Sign-up is not available yet",
        html! {
            p class="content" {
                "This library has not set up email yet, so new accounts cannot be verified. "
                "Please check back later."
            }
            (oauth_buttons(&providers))
            (footer_links())
        },
    )
}

/// The neutral answer for every accepted email-mode submission.
pub fn check_inbox(title: &str) -> Markup {
    card(
        title,
        html! {
            p class="content" {
                "If that address can receive mail, a link is on its way. "
                "It expires soon, so open it from the same device if you can."
            }
            a class="button is-light is-fullwidth" href="/login" { "Back to login" }
        },
    )
}

fn invalid_link() -> Markup {
    card(
        "This link has expired",
        html! {
            p class="content" { "Sign-up links work once and expire after 24 hours." }
            a class="button is-primary is-fullwidth" href="/register" { "Request a new link" }
        },
    )
}

fn complete_form(email: &str, action: &str, error: Option<&str>) -> Markup {
    card(
        "Finish your account",
        html! {
            (error_note(error))
            p class="content m-text-muted" { "Signing up as " strong { (email) } "." }
            form method="post" action=(action) {
                div class="field" {
                    label class="label" { "Display name" }
                    div class="control" {
                        input class="input" type="text" name="name" maxlength="32" required="";
                    }
                }
                div class="field" {
                    label class="label" { "Password (min 8 chars)" }
                    div class="control" {
                        input class="input" type="password" name="password" autocomplete="new-password" required="";
                    }
                }
                div class="field" {
                    div class="control" {
                        button type="submit" class="button is-primary is-fullwidth" { "Create account" }
                    }
                }
            }
        },
    )
}

async fn render(
    state: &AppState,
    status: StatusCode,
    title: &str,
    content: Markup,
    user: Option<&AuthUser>,
) -> Response {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    (status, page(&cfg, title, content, user)).into_response()
}

/// GET `/register` — the form for the current sign-up mode.
pub async fn register_page(State(state): State<Arc<AppState>>, user: Option<AuthUser>) -> Response {
    let mode = {
        let db = state.db.lock().await;
        signup_mode(&db)
    };
    let content = match mode {
        SignupMode::Email => email_form(None),
        SignupMode::Direct => direct_form(None),
        SignupMode::Closed => closed_notice(),
    };
    render(&state, StatusCode::OK, "Register", content, user.as_ref()).await
}

#[derive(Deserialize)]
pub struct RegisterForm {
    pub email: String,
    /// Direct mode only.
    pub name: Option<String>,
    /// Direct mode only.
    pub password: Option<String>,
}

/// POST `/register` — email mode sends a link; direct mode creates the
/// account immediately; closed mode refuses.
pub async fn register_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    ClientIp(ip): ClientIp,
    Form(form): Form<RegisterForm>,
) -> Response {
    let mode = {
        let db = state.db.lock().await;
        signup_mode(&db)
    };
    let email = form.email.trim().to_string();
    match mode {
        SignupMode::Closed => {
            render(
                &state,
                StatusCode::FORBIDDEN,
                "Register",
                closed_notice(),
                user.as_ref(),
            )
            .await
        }
        SignupMode::Direct => {
            let name = form.name.unwrap_or_default().trim().to_string();
            let password = form.password.unwrap_or_default();
            let invalid = if valid_email(&email) {
                validate_profile(&name, &password)
            } else {
                Some("Enter a valid email address.")
            };
            if let Some(msg) = invalid {
                return render(
                    &state,
                    StatusCode::BAD_REQUEST,
                    "Register",
                    direct_form(Some(msg)),
                    user.as_ref(),
                )
                .await;
            }
            match create_member_session(&state, &name, &email, &password).await {
                Some(cookie) => redirect_with_cookie("/", cookie),
                // Same message for a taken address: no enumeration.
                None => {
                    render(
                        &state,
                        StatusCode::BAD_REQUEST,
                        "Register",
                        direct_form(Some("Could not create the account.")),
                        user.as_ref(),
                    )
                    .await
                }
            }
        }
        SignupMode::Email => {
            if !valid_email(&email) {
                return render(
                    &state,
                    StatusCode::BAD_REQUEST,
                    "Register",
                    email_form(Some("Enter a valid email address.")),
                    user.as_ref(),
                )
                .await;
            }
            if !state
                .rate_limiter
                .allow(&format!("ip:{ip}"), MAILS_PER_IP, HOUR)
            {
                return render(
                    &state,
                    StatusCode::TOO_MANY_REQUESTS,
                    "Register",
                    email_form(Some("Too many attempts. Please try again in an hour.")),
                    user.as_ref(),
                )
                .await;
            }
            // Over the per-address limit, answer exactly as on success: a
            // different reply would reveal which addresses were tried.
            if state
                .rate_limiter
                .allow(&format!("mail:{email}"), MAILS_PER_ADDRESS, HOUR)
            {
                send_signup_or_reset_mail(&state, &email).await;
            }
            render(
                &state,
                StatusCode::OK,
                "Check your inbox",
                check_inbox("Check your inbox"),
                user.as_ref(),
            )
            .await
        }
    }
}

/// Mail a sign-up link to a new address, or a reset link to a known one.
/// Failures are logged, never shown, so the response stays identical.
async fn send_signup_or_reset_mail(state: &AppState, email: &str) {
    let token = new_token();
    let base = crate::oauth::base_url();
    let (settings, subject, body) = {
        let db = state.db.lock().await;
        let Some(settings) = MailSettings::load(&db) else {
            return;
        };
        let site_title = crate::config::site_config(&db).site_title;
        let known = mangaread_core::db::find_user_by_email(&db, email).is_ok();
        let (purpose, ttl) = if known {
            (TokenPurpose::Reset, RESET_TTL_SECS)
        } else {
            (TokenPurpose::Signup, SIGNUP_TTL_SECS)
        };
        if let Err(e) = mangaread_core::db::create_email_token(
            &db,
            &token_hash(&token),
            email,
            purpose,
            &session_expires_at(ttl),
        ) {
            tracing::error!("signup: could not store token: {e}");
            return;
        }
        let (subject, body) = if known {
            (
                format!("Your {site_title} account"),
                format!(
                    "Someone tried to sign up to {site_title} with this address, but it already has an account.\n\n\
                     If you forgot your password, set a new one here (valid for 1 hour):\n{base}/reset/{token}\n\n\
                     If this wasn't you, you can ignore this email."
                ),
            )
        } else {
            (
                format!("Finish signing up to {site_title}"),
                format!(
                    "Open this link to finish creating your {site_title} account (valid for 24 hours):\n\
                     {base}/register/{token}\n\n\
                     If you didn't ask for this, you can ignore this email."
                ),
            )
        };
        (settings, subject, body)
    };
    if let Err(e) = state.mailer.send(&settings, email, &subject, &body).await {
        tracing::warn!("signup: mail to {email} failed: {e}");
    }
}

/// Create a member and a remembered session; `None` when the address is
/// taken or a write fails. Returns the `Set-Cookie` value.
async fn create_member_session(
    state: &AppState,
    name: &str,
    email: &str,
    password: &str,
) -> Option<String> {
    // bcrypt runs outside the DB lock so it does not serialize other requests.
    let password_hash = hash_password(password).ok()?;
    let session = new_token();
    let db = state.db.lock().await;
    let id = mangaread_core::db::create_user(
        &db,
        &NewUser {
            name: name.to_string(),
            email: email.to_string(),
            password_hash,
            role: Role::Member,
        },
    )
    .ok()?;
    mangaread_core::db::insert_session(
        &db,
        id,
        &session,
        &session_expires_at(REMEMBER_SESSION_MAX_AGE),
        "web",
    )
    .ok()?;
    Some(set_cookie_value(&session, REMEMBER_SESSION_MAX_AGE))
}

/// GET `/register/{token}` — the finish form, if the link is still live.
pub async fn complete_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(token): Path<String>,
) -> Response {
    let email = {
        let db = state.db.lock().await;
        mangaread_core::db::peek_email_token(
            &db,
            &token_hash(&token),
            TokenPurpose::Signup,
            &now_utc_string(),
        )
        .ok()
        .flatten()
    };
    match email {
        Some(email) => {
            let action = format!("/register/{token}");
            render(
                &state,
                StatusCode::OK,
                "Finish your account",
                complete_form(&email, &action, None),
                user.as_ref(),
            )
            .await
        }
        None => {
            render(
                &state,
                StatusCode::BAD_REQUEST,
                "Link expired",
                invalid_link(),
                user.as_ref(),
            )
            .await
        }
    }
}

#[derive(Deserialize)]
pub struct CompleteForm {
    pub name: String,
    pub password: String,
}

/// POST `/register/{token}` — consume the link, create the member, log in.
pub async fn complete_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(token): Path<String>,
    Form(form): Form<CompleteForm>,
) -> Response {
    let hash = token_hash(&token);
    let name = form.name.trim().to_string();
    // Validate before consuming, so a typo does not burn the link.
    if let Some(msg) = validate_profile(&name, &form.password) {
        let email = {
            let db = state.db.lock().await;
            mangaread_core::db::peek_email_token(
                &db,
                &hash,
                TokenPurpose::Signup,
                &now_utc_string(),
            )
            .ok()
            .flatten()
        };
        return match email {
            Some(email) => {
                let action = format!("/register/{token}");
                render(
                    &state,
                    StatusCode::BAD_REQUEST,
                    "Finish your account",
                    complete_form(&email, &action, Some(msg)),
                    user.as_ref(),
                )
                .await
            }
            None => {
                render(
                    &state,
                    StatusCode::BAD_REQUEST,
                    "Link expired",
                    invalid_link(),
                    user.as_ref(),
                )
                .await
            }
        };
    }
    let email = {
        let db = state.db.lock().await;
        mangaread_core::db::consume_email_token(&db, &hash, TokenPurpose::Signup, &now_utc_string())
            .ok()
            .flatten()
    };
    let Some(email) = email else {
        return render(
            &state,
            StatusCode::BAD_REQUEST,
            "Link expired",
            invalid_link(),
            user.as_ref(),
        )
        .await;
    };
    match create_member_session(&state, &name, &email, &form.password).await {
        Some(cookie) => redirect_with_cookie("/", cookie),
        // The address was registered meanwhile (e.g. via OAuth): just log in.
        None => axum::response::Redirect::to("/login").into_response(),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{TestApp, app_with_admin};
    use axum::http::StatusCode;

    async fn email_mode() -> TestApp {
        let (app, _) = app_with_admin().await;
        app.set("smtp_url", "smtp://localhost:2525").await;
        app
    }

    async fn role_of(app: &TestApp, email: &str) -> Option<mangaread_core::domain::Role> {
        let db = app.state.db.lock().await;
        mangaread_core::db::find_user_by_email(&db, email)
            .ok()
            .map(|u| u.role)
    }

    #[tokio::test]
    async fn signup_is_closed_without_smtp_by_default() {
        if std::env::var("SMTP_URL").is_ok() {
            return;
        }
        let (app, _) = app_with_admin().await;
        let page = app.get("/register", None).await;
        assert!(page.body.contains("Sign-up is not available yet"));
        let res = app
            .post_form(
                "/register",
                &[
                    ("name", "R"),
                    ("email", "r@example.com"),
                    ("password", "password1"),
                ],
                None,
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
        assert_eq!(role_of(&app, "r@example.com").await, None);
    }

    #[tokio::test]
    async fn email_link_creates_a_member_and_works_once() {
        let app = email_mode().await;
        let res = app
            .post_form("/register", &[("email", "new@example.com")], None)
            .await;
        assert_eq!(res.status, StatusCode::OK);
        assert!(res.body.contains("Check your inbox"));
        assert_eq!(
            role_of(&app, "new@example.com").await,
            None,
            "no account before the link"
        );

        let link = app
            .mailer
            .last_link_to("new@example.com", "register")
            .unwrap();
        let form = app.get(&link, None).await;
        assert_eq!(form.status, StatusCode::OK);
        assert!(form.body.contains("new@example.com"));

        // A validation error does not burn the link.
        let short = app
            .post_form(&link, &[("name", "New"), ("password", "short")], None)
            .await;
        assert_eq!(short.status, StatusCode::BAD_REQUEST);

        let done = app
            .post_form(&link, &[("name", "New"), ("password", "password1")], None)
            .await;
        assert_eq!(done.location.as_deref(), Some("/"));
        assert!(done.session.is_some());
        assert_eq!(
            role_of(&app, "new@example.com").await,
            Some(mangaread_core::domain::Role::Member)
        );

        let replay = app
            .post_form(&link, &[("name", "Again"), ("password", "password1")], None)
            .await;
        assert_eq!(replay.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn known_and_unknown_addresses_get_the_same_reply() {
        let app = email_mode().await;
        app.member("known@example.com").await;
        let known = app
            .post_form("/register", &[("email", "known@example.com")], None)
            .await;
        let unknown = app
            .post_form("/register", &[("email", "unknown@example.com")], None)
            .await;
        assert_eq!(known.status, unknown.status);
        assert_eq!(known.body, unknown.body);
        // The known address is pointed at a reset instead of a second account.
        assert!(
            app.mailer
                .last_link_to("known@example.com", "reset")
                .is_some()
        );
        assert!(
            app.mailer
                .last_link_to("known@example.com", "register")
                .is_none()
        );
    }

    #[tokio::test]
    async fn mails_per_address_are_capped_silently() {
        let app = email_mode().await;
        for _ in 0..5 {
            let res = app
                .post_form("/register", &[("email", "spam@example.com")], None)
                .await;
            assert_eq!(res.status, StatusCode::OK);
        }
        assert_eq!(
            app.mailer.count_to("spam@example.com"),
            super::MAILS_PER_ADDRESS
        );
    }

    #[tokio::test]
    async fn mails_per_ip_are_capped_with_429() {
        let app = email_mode().await;
        let mut last = StatusCode::OK;
        for i in 0..=super::MAILS_PER_IP {
            let email = format!("u{i}@example.com");
            last = app
                .post_form("/register", &[("email", &email)], None)
                .await
                .status;
        }
        assert_eq!(last, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn forgot_password_resets_by_email_and_revokes_sessions() {
        let app = email_mode().await;
        let (_, session) = app.member("m@example.com").await;
        let res = app
            .post_form("/forgot", &[("email", "m@example.com")], None)
            .await;
        assert!(res.body.contains("Check your inbox"));
        let link = app.mailer.last_link_to("m@example.com", "reset").unwrap();
        let done = app
            .post_form(&link, &[("password", "brand-new-pass")], None)
            .await;
        assert_eq!(done.location.as_deref(), Some("/login"));
        assert_eq!(
            app.get("/settings", Some(&session))
                .await
                .location
                .as_deref(),
            Some("/login"),
            "old sessions are revoked"
        );
        let login = app
            .post_form(
                "/login",
                &[("email", "m@example.com"), ("password", "brand-new-pass")],
                None,
            )
            .await;
        assert_eq!(login.status, StatusCode::SEE_OTHER);
        // Unknown addresses get the same page and no mail.
        let unknown = app
            .post_form("/forgot", &[("email", "nobody@example.com")], None)
            .await;
        assert_eq!(unknown.body, res.body);
        assert_eq!(app.mailer.count_to("nobody@example.com"), 0);
    }

    #[test]
    fn email_validation_rejects_obvious_garbage() {
        assert!(super::valid_email("a@b.co"));
        assert!(!super::valid_email("a@b"));
        assert!(!super::valid_email("@b.co"));
        assert!(!super::valid_email("a b@c.co"));
    }
}
