//! `/admin/email` — SMTP settings, the unverified sign-up switch, and a
//! test send.
//!
//! The stored SMTP URL usually embeds a password, so it is never echoed back
//! into the page: a blank field keeps the saved value.

use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use mangaread_core::domain::Role;
use maud::{Markup, html};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{AuthUser, require_role},
    email::{ALLOW_UNVERIFIED_KEY, MailSettings},
    routes::admin::page,
};

/// What the form shows. Never contains the SMTP URL itself.
struct View {
    url_saved: bool,
    url_from_env: bool,
    from: String,
    allow_unverified: bool,
}

fn load_view(conn: &rusqlite::Connection) -> View {
    let get = |k: &str| {
        mangaread_core::db::get_setting(conn, k)
            .ok()
            .flatten()
            .unwrap_or_default()
    };
    let url_saved = !get("smtp_url").trim().is_empty();
    View {
        url_saved,
        url_from_env: !url_saved && std::env::var("SMTP_URL").is_ok_and(|v| !v.trim().is_empty()),
        from: get("smtp_from"),
        allow_unverified: get(ALLOW_UNVERIFIED_KEY) == "1",
    }
}

fn content(view: &View, notice: Markup, setup: bool) -> Markup {
    let configured = view.url_saved || view.url_from_env;
    html! {
        section {
            h1 class="title is-2" { "Email" }
            @if setup {
                div class="notification is-info is-light" {
                    strong { "Your admin account is ready." }
                    " Next, set up email so readers can sign up and reset their passwords. "
                    "You can do this later from the admin dashboard."
                }
            }
            (notice)
            div class="box" {
                form method="post" action="/admin/email" {
                    div class="field" {
                        label class="label" for="smtp_url" { "SMTP URL" }
                        div class="control" {
                            input class="input" id="smtp_url" type="password" name="smtp_url" autocomplete="off"
                                placeholder=(if configured { "Saved — leave blank to keep it" } else { "smtps://user:password@smtp.example.com" });
                        }
                        p class="help" {
                            "Use " code { "smtps://user:pass@host" } " (port 465) or "
                            code { "smtp://user:pass@host:587?tls=required" } ". "
                            @if view.url_from_env { "Currently set by the SMTP_URL environment variable; a saved value overrides it." }
                        }
                    }
                    @if view.url_saved {
                        div class="field" {
                            label class="checkbox" {
                                input type="checkbox" name="clear_url" value="1";
                                " Remove the saved SMTP URL"
                            }
                        }
                    }
                    div class="field" {
                        label class="label" for="smtp_from" { "From address" }
                        div class="control" {
                            input class="input" id="smtp_from" type="text" name="smtp_from" value=(view.from)
                                placeholder="MangaRead <noreply@example.com>";
                        }
                    }
                    div class="field" {
                        label class="checkbox" {
                            input type="checkbox" name="allow_unverified" value="1" checked[view.allow_unverified];
                            " Allow sign-up without email verification when SMTP is not set up"
                        }
                        p class="help" {
                            "Off by default. Turning this on lets anyone create accounts with any address, "
                            "which makes spam accounts easy."
                        }
                    }
                    div class="field" {
                        button class="button is-primary" type="submit" { "Save" }
                    }
                }
            }
            @if configured {
                div class="box" {
                    h2 class="title is-5" { "Test the connection" }
                    p class="content" { "Sends a short message to your own account's address." }
                    form method="post" action="/admin/email/test" {
                        button class="button" type="submit" { "Send test email" }
                    }
                }
            }
            a class="button is-light" href="/admin" { "Back to admin" }
        }
    }
}

fn notice(kind: &str, text: &str) -> Markup {
    html! { div class=(format!("notification is-light {kind}")) { (text) } }
}

async fn render(
    state: &AppState,
    user: &AuthUser,
    status: StatusCode,
    note: Markup,
    setup: bool,
) -> Response {
    let (cfg, view) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), load_view(&db))
    };
    (
        status,
        page(&cfg, "Email", content(&view, note, setup), Some(user)),
    )
        .into_response()
}

/// GET `/admin/email`.
pub async fn email_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let note = match q.get("ok").map(String::as_str) {
        Some("saved") => notice("is-success", "Email settings saved."),
        _ => html! {},
    };
    render(&state, &user, StatusCode::OK, note, q.contains_key("setup")).await
}

#[derive(Deserialize)]
pub struct EmailForm {
    #[serde(default)]
    pub smtp_url: String,
    #[serde(default)]
    pub clear_url: Option<String>,
    #[serde(default)]
    pub smtp_from: String,
    #[serde(default)]
    pub allow_unverified: Option<String>,
}

/// POST `/admin/email` — validate everything, then write.
pub async fn email_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<EmailForm>,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let url = form.smtp_url.trim();
    if !url.is_empty() && !(url.starts_with("smtp://") || url.starts_with("smtps://")) {
        let note = notice(
            "is-danger",
            "The SMTP URL must start with smtp:// or smtps://.",
        );
        return render(&state, &user, StatusCode::BAD_REQUEST, note, false).await;
    }
    let from = form.smtp_from.trim();
    if !from.is_empty() && from.parse::<lettre::message::Mailbox>().is_err() {
        let note = notice(
            "is-danger",
            "The from address is not valid. Example: MangaRead <noreply@example.com>",
        );
        return render(&state, &user, StatusCode::BAD_REQUEST, note, false).await;
    }
    let saved = {
        let db = state.db.lock().await;
        let set = |k: &str, v: &str| mangaread_core::db::set_setting(&db, k, v, user.id);
        let unverified = if form.allow_unverified.is_some() {
            "1"
        } else {
            "0"
        };
        let mut result = set("smtp_from", from).and_then(|_| set(ALLOW_UNVERIFIED_KEY, unverified));
        if form.clear_url.is_some() {
            result = result.and_then(|_| set("smtp_url", ""));
        } else if !url.is_empty() {
            result = result.and_then(|_| set("smtp_url", url));
        }
        result.is_ok()
    };
    if !saved {
        let note = notice("is-danger", "Could not save the settings.");
        return render(
            &state,
            &user,
            StatusCode::INTERNAL_SERVER_ERROR,
            note,
            false,
        )
        .await;
    }
    Redirect::to("/admin/email?ok=saved").into_response()
}

/// POST `/admin/email/test` — send to the admin's own address and show the
/// SMTP error inline (escaped) when it fails.
pub async fn email_test(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (settings, to, site_title) = {
        let db = state.db.lock().await;
        let to = mangaread_core::db::find_user_by_id(&db, user.id)
            .map(|u| u.email)
            .unwrap_or_default();
        (
            MailSettings::load(&db),
            to,
            crate::config::site_config(&db).site_title,
        )
    };
    let Some(settings) = settings else {
        let note = notice("is-danger", "Save an SMTP URL first.");
        return render(&state, &user, StatusCode::BAD_REQUEST, note, false).await;
    };
    let body = format!("Email from {site_title} works. Readers can now sign up by email.");
    let note = match state
        .mailer
        .send(&settings, &to, &format!("{site_title} test email"), &body)
        .await
    {
        Ok(()) => notice("is-success", &format!("Test email sent to {to}.")),
        Err(e) => notice("is-danger", &format!("Sending failed: {e}")),
    };
    render(&state, &user, StatusCode::OK, note, false).await
}

#[cfg(test)]
mod tests {
    use crate::test_support::app_with_admin;
    use axum::http::StatusCode;

    #[tokio::test]
    async fn members_are_refused() {
        let (app, _) = app_with_admin().await;
        let (_, member) = app.member("m@example.com").await;
        assert_eq!(
            app.get("/admin/email", Some(&member)).await.status,
            StatusCode::FORBIDDEN
        );
        let res = app
            .post_form(
                "/admin/email",
                &[("smtp_url", "smtp://evil")],
                Some(&member),
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn saved_url_is_never_echoed_and_blank_keeps_it() {
        let (app, admin) = app_with_admin().await;
        let secret = "smtps://user:hunter22@smtp.example.com";
        let res = app
            .post_form(
                "/admin/email",
                &[
                    ("smtp_url", secret),
                    ("smtp_from", "Lib <noreply@example.com>"),
                ],
                Some(&admin),
            )
            .await;
        assert_eq!(res.location.as_deref(), Some("/admin/email?ok=saved"));
        let page = app.get("/admin/email", Some(&admin)).await;
        assert!(!page.body.contains("hunter22"));
        // A blank URL on the next save keeps the stored one.
        app.post_form(
            "/admin/email",
            &[("smtp_url", ""), ("smtp_from", "")],
            Some(&admin),
        )
        .await;
        let db = app.state.db.lock().await;
        let stored = mangaread_core::db::get_setting(&db, "smtp_url").unwrap();
        assert_eq!(stored.as_deref(), Some(secret));
    }

    #[tokio::test]
    async fn invalid_input_is_rejected_without_writing() {
        let (app, admin) = app_with_admin().await;
        let res = app
            .post_form("/admin/email", &[("smtp_url", "http://nope")], Some(&admin))
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST);
        let res = app
            .post_form(
                "/admin/email",
                &[("smtp_from", "not an address")],
                Some(&admin),
            )
            .await;
        assert_eq!(res.status, StatusCode::BAD_REQUEST);
        let db = app.state.db.lock().await;
        assert_eq!(
            mangaread_core::db::get_setting(&db, "smtp_url").unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn unverified_toggle_opens_direct_signup_and_test_send_reaches_admin() {
        let (app, admin) = app_with_admin().await;
        app.post_form("/admin/email", &[("allow_unverified", "1")], Some(&admin))
            .await;
        if std::env::var("SMTP_URL").is_err() {
            let reg = app.get("/register", None).await;
            assert!(reg.body.contains("name=\"password\""));
        }
        app.post_form(
            "/admin/email",
            &[("smtp_url", "smtp://localhost:2525")],
            Some(&admin),
        )
        .await;
        let res = app.post_form("/admin/email/test", &[], Some(&admin)).await;
        assert!(res.body.contains("Test email sent to owner@example.com"));
        assert_eq!(app.mailer.count_to("owner@example.com"), 1);
    }

    #[tokio::test]
    async fn admin_reset_link_works_through_the_hashed_store() {
        let (app, admin) = app_with_admin().await;
        let (id, _) = app.member("m@example.com").await;
        let res = app
            .post_form(&format!("/admin/users/{id}/reset-link"), &[], Some(&admin))
            .await;
        let location = res.location.unwrap();
        let link = form_urlencoded::parse(location.split_once('?').unwrap().1.as_bytes())
            .find(|(k, _)| k == "reset_link")
            .unwrap()
            .1
            .into_owned();
        let path = &link[link.find("/reset/").unwrap()..];
        let done = app
            .post_form(path, &[("password", "fresh-password")], None)
            .await;
        assert_eq!(done.location.as_deref(), Some("/login"));
        let again = app
            .post_form(path, &[("password", "fresh-password")], None)
            .await;
        assert_eq!(again.status, StatusCode::BAD_REQUEST, "single use");
    }
}
