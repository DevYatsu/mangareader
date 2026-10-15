//! User settings page with visible profile and password forms.
//!
//! All endpoints require [`AuthUser`]. Successful POSTs redirect with
//! `303` to `/settings`. Validation failures re-render the page with
//! `400` and an inline error message.

use axum::{
    Form,
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use maud::{Markup, html};
use serde::Deserialize;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser};

/// Locales the profile form accepts. The page stays English until
/// Slice 6 i18n; anything else is a `400` re-render.
const LOCALES: &[(&str, &str)] = &[
    ("en", "English"),
    ("fr", "Français"),
    ("es", "Español"),
    ("de", "Deutsch"),
    ("ja", "日本語"),
];

fn is_locale(locale: &str) -> bool {
    LOCALES.iter().any(|(key, _)| *key == locale)
}

#[derive(Deserialize)]
pub struct ProfileForm {
    pub name: String,
    pub locale: String,
}

#[derive(Deserialize)]
pub struct PasswordForm {
    pub current: String,
    pub new: String,
}

/// Current persisted profile fields for prefilling the forms.
struct CurrentProfile {
    name: String,
    locale: String,
}

fn current_profile(conn: &rusqlite::Connection, user_id: i64) -> CurrentProfile {
    let row: Result<(String, String), _> = conn.query_row(
        "SELECT name, locale FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    );
    match row {
        Ok((name, locale)) => CurrentProfile { name, locale },
        Err(_) => CurrentProfile {
            name: String::new(),
            locale: "en".into(),
        },
    }
}

fn page(cfg: &crate::config::SiteConfig, content: Markup, user: Option<&AuthUser>) -> Html<String> {
    crate::templates::layout::page(
        cfg,
        "Settings",
        crate::templates::layout::Nav::None,
        user,
        content,
    )
}

fn settings_content(
    current: &CurrentProfile,
    profile_error: Option<&str>,
    password_error: Option<&str>,
) -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Settings" }
            @if let Some(msg) = profile_error.or(password_error) {
                div class="notification is-danger is-light" { (msg) }
            }

            div class="box" {
                h2 class="title is-4" { "Profile" }
                form method="post" action="/settings/profile" {
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="settings-profile-name-label" for="settings-profile-name" { "Name" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="settings-profile-name" class="input" type="text" name="name" maxlength="32" required="" value=(current.name);
                            }
                        }
                    }
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="settings-profile-locale-label" for="settings-profile-locale" { "Locale" }
                        }
                        div class="field-body" {
                            div class="control" {
                                div class="select is-fullwidth" {
                                    select id="settings-profile-locale" name="locale" {
                                        @for (key, label) in LOCALES {
                                            option value=(key) selected[current.locale == *key] { (label) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div class="field" {
                        div class="control" {
                            button class="button is-primary" type="submit" { "Save profile" }
                        }
                    }
                }
            }

            div class="box" {
                h2 class="title is-4" { "Password" }
                form method="post" action="/settings/password" {
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="settings-current-password-label" for="settings-current-password" { "Current password" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="settings-current-password" class="input" type="password" name="current" required="";
                            }
                        }
                    }
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="settings-new-password-label" for="settings-new-password" { "New password (min 8 chars)" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="settings-new-password" class="input" type="password" name="new" required="";
                            }
                        }
                    }
                    div class="field" {
                        div class="control" {
                            button class="button is-primary" type="submit" { "Change password" }
                        }
                    }
                }
            }
        }
    }
}

/// GET `/settings` — visible profile and password forms.
pub async fn settings_page(State(state): State<Arc<AppState>>, user: AuthUser) -> Html<String> {
    let (cfg, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            current_profile(&db, user.id),
        )
    };
    page(&cfg, settings_content(&current, None, None), Some(&user))
}

fn render_400(
    cfg: &crate::config::SiteConfig,
    current: &CurrentProfile,
    profile_error: Option<&str>,
    password_error: Option<&str>,
    user: Option<&AuthUser>,
) -> Response {
    (
        StatusCode::BAD_REQUEST,
        page(
            cfg,
            settings_content(current, profile_error, password_error),
            user,
        ),
    )
        .into_response()
}

/// POST `/settings/profile` — update name and locale.
pub async fn profile_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<ProfileForm>,
) -> Response {
    let name = form.name.trim().to_string();
    let (cfg, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            current_profile(&db, user.id),
        )
    };
    if name.is_empty() || name.chars().count() > 32 {
        return render_400(
            &cfg,
            &current,
            Some("Name must be 1-32 characters."),
            None,
            Some(&user),
        );
    }
    if !is_locale(&form.locale) {
        return render_400(&cfg, &current, Some("Unknown locale."), None, Some(&user));
    }
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_profile(&db, user.id, &name, &form.locale).is_err() {
            return render_400(
                &cfg,
                &current,
                Some("Could not save profile."),
                None,
                Some(&user),
            );
        }
    }
    Redirect::to("/settings").into_response()
}

/// POST `/settings/password` — verify the current password, then set the new one.
///
/// Wrong current passwords and too-short new passwords render the same
/// message so failures do not reveal which check failed.
pub async fn password_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<PasswordForm>,
) -> Response {
    let (cfg, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            current_profile(&db, user.id),
        )
    };
    let render_failed = |cfg: &crate::config::SiteConfig, current: &CurrentProfile| {
        render_400(
            cfg,
            current,
            None,
            Some("Invalid current password or new password too short."),
            Some(&user),
        )
    };
    // Verify the current password first: lock only for the hash lookup;
    // bcrypt verification runs without the guard so concurrent requests
    // are not serialized behind hashing.
    let stored_hash = {
        let db = state.db.lock().await;
        match db.query_row(
            "SELECT password_hash FROM users WHERE id = ?1",
            rusqlite::params![user.id],
            |row| row.get::<_, String>(0),
        ) {
            Ok(h) => h,
            Err(_) => return render_failed(&cfg, &current),
        }
    };
    if !crate::auth::verify_password(&form.current, &stored_hash) {
        return render_failed(&cfg, &current);
    }
    if form.new.len() < 8 {
        return render_failed(&cfg, &current);
    }
    let new_hash = match crate::auth::hash_password(&form.new) {
        Ok(h) => h,
        Err(_) => return render_failed(&cfg, &current),
    };
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_password(&db, user.id, &new_hash).is_err() {
            return render_failed(&cfg, &current);
        }
    }
    Redirect::to("/settings").into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_markup_uses_bulma_and_keeps_form_contracts() {
        let markup = settings_content(
            &CurrentProfile {
                name: "Reader".into(),
                locale: "en".into(),
            },
            None,
            None,
        )
        .into_string();

        for class_name in [
            "box",
            "field is-horizontal",
            "field-label",
            "field-body",
            "input",
            "select",
            "title is-2",
        ] {
            assert!(
                markup.contains(class_name),
                "missing Bulma class: {class_name}"
            );
        }
        for id in [
            "settings-profile-name",
            "settings-profile-locale",
            "settings-current-password",
            "settings-new-password",
        ] {
            assert!(markup.contains(&format!("id=\"{id}-label\" for=\"{id}\"")));
            assert!(markup.contains(&format!("id=\"{id}\"")));
        }
        assert!(markup.contains("method=\"post\" action=\"/settings/profile\""));
        assert!(markup.contains("method=\"post\" action=\"/settings/password\""));
        assert!(markup.contains("name=\"name\""));
        assert!(markup.contains("name=\"locale\""));
        assert!(markup.contains("name=\"current\""));
        assert!(markup.contains("name=\"new\""));
        assert!(!markup.contains("name=\"theme\""));
        assert!(!markup.contains("/settings/appearance"));
        assert!(!markup.contains(concat!("settings", "-section")));
        assert!(!markup.contains(concat!("settings", "-form")));
        assert!(!markup.contains(concat!("auth", "-error")));
    }
}
