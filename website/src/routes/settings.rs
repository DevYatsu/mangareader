//! User settings page: profile, appearance, and password forms.
//!
//! All endpoints require [`AuthUser`]. Successful POSTs redirect with
//! `303` to `/settings`. Validation failures re-render the page with
//! `400` and an inline error message.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
    Form,
};
use maud::{html, Markup};
use serde::Deserialize;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

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
    pub theme: String,
}

#[derive(Deserialize)]
pub struct AppearanceForm {
    pub theme: String,
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
    theme: String,
}

fn current_profile(conn: &rusqlite::Connection, user_id: i64) -> CurrentProfile {
    let row: Result<(String, String, String), _> = conn.query_row(
        "SELECT name, locale, theme FROM users WHERE id = ?1",
        rusqlite::params![user_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    );
    match row {
        Ok((name, locale, theme)) => CurrentProfile { name, locale, theme },
        Err(_) => CurrentProfile {
            name: String::new(),
            locale: "en".into(),
            theme: String::new(),
        },
    }
}

fn page(
    cfg: &crate::config::SiteConfig,
    theme: &str,
    content: Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    let banner: Option<String> = None;
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            "Settings",
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

fn settings_content(
    current: &CurrentProfile,
    profile_error: Option<&str>,
    appearance_error: Option<&str>,
    password_error: Option<&str>,
) -> Markup {
    html! {
        section class="settings-section" {
            h1 class="page-title" { "Settings" }
            @if let Some(msg) = profile_error {
                p class="auth-error" { (msg) }
            }
            h2 { "Profile" }
            form method="post" action="/settings/profile" class="settings-form" {
                label { "Name" input type="text" name="name" maxlength="32" required="" value=(current.name); }
                label {
                    "Locale"
                    select name="locale" {
                        @for (key, label) in LOCALES {
                            option value=(key) selected[current.locale == *key] { (label) }
                        }
                    }
                }
                label {
                    "Theme"
                    select name="theme" {
                        @for (key, label) in crate::config::THEMES {
                            option value=(key) selected[current.theme == *key] { (label) }
                        }
                    }
                }
                button type="submit" { "Save profile" }
            }
            @if let Some(msg) = appearance_error {
                p class="auth-error" { (msg) }
            }
            h2 { "Appearance" }
            form method="post" action="/settings/appearance" class="settings-form" {
                label {
                    "Theme"
                    select name="theme" {
                        @for (key, label) in crate::config::THEMES {
                            option value=(key) selected[current.theme == *key] { (label) }
                        }
                    }
                }
                button type="submit" { "Save appearance" }
            }
            @if let Some(msg) = password_error {
                p class="auth-error" { (msg) }
            }
            h2 { "Password" }
            form method="post" action="/settings/password" class="settings-form" {
                label { "Current password" input type="password" name="current" required=""; }
                label { "New password (min 8 chars)" input type="password" name="new" required=""; }
                button type="submit" { "Change password" }
            }
        }
    }
}

/// GET `/settings` — profile + appearance + password forms on one page.
pub async fn settings_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Html<String> {
    let (cfg, theme, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            current_profile(&db, user.id),
        )
    };
    page(&cfg, &theme, settings_content(&current, None, None, None), Some(&user))
}

fn render_400(
    cfg: &crate::config::SiteConfig,
    theme: &str,
    current: &CurrentProfile,
    profile_error: Option<&str>,
    appearance_error: Option<&str>,
    password_error: Option<&str>,
    user: Option<&AuthUser>,
) -> Response {
    (
        StatusCode::BAD_REQUEST,
        page(
            cfg,
            theme,
            settings_content(current, profile_error, appearance_error, password_error),
            user,
        ),
    )
        .into_response()
}

/// POST `/settings/profile` — update name, locale, and theme.
pub async fn profile_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<ProfileForm>,
) -> Response {
    let name = form.name.trim().to_string();
    let (cfg, theme, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            current_profile(&db, user.id),
        )
    };
    if name.is_empty() || name.chars().count() > 32 {
        return render_400(
            &cfg,
            &theme,
            &current,
            Some("Name must be 1-32 characters."),
            None,
            None,
            Some(&user),
        );
    }
    if !is_locale(&form.locale) {
        return render_400(
            &cfg,
            &theme,
            &current,
            Some("Unknown locale."),
            None,
            None,
            Some(&user),
        );
    }
    if !crate::config::is_theme(&form.theme) {
        return render_400(
            &cfg,
            &theme,
            &current,
            Some("Unknown theme."),
            None,
            None,
            Some(&user),
        );
    }
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_profile(&db, user.id, &name, &form.locale, &form.theme)
            .is_err()
        {
            return render_400(
                &cfg,
                &theme,
                &current,
                Some("Could not save profile."),
                None,
                None,
                Some(&user),
            );
        }
    }
    Redirect::to("/settings").into_response()
}

/// POST `/settings/appearance` — update the user's theme only.
pub async fn appearance_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<AppearanceForm>,
) -> Response {
    let (cfg, theme, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            current_profile(&db, user.id),
        )
    };
    if !crate::config::is_theme(&form.theme) {
        return render_400(
            &cfg,
            &theme,
            &current,
            None,
            Some("Unknown theme."),
            None,
            Some(&user),
        );
    }
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_profile(
            &db,
            user.id,
            &current.name,
            &current.locale,
            &form.theme,
        )
        .is_err()
        {
            return render_400(
                &cfg,
                &theme,
                &current,
                None,
                Some("Could not save appearance."),
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
    let (cfg, theme, current) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            current_profile(&db, user.id),
        )
    };
    let render_failed = |cfg: &crate::config::SiteConfig,
                         theme: &str,
                         current: &CurrentProfile| {
        render_400(
            cfg,
            theme,
            current,
            None,
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
            Err(_) => return render_failed(&cfg, &theme, &current),
        }
    };
    if !crate::auth::verify_password(&form.current, &stored_hash) {
        return render_failed(&cfg, &theme, &current);
    }
    if form.new.len() < 8 {
        return render_failed(&cfg, &theme, &current);
    }
    let new_hash = match crate::auth::hash_password(&form.new) {
        Ok(h) => h,
        Err(_) => return render_failed(&cfg, &theme, &current),
    };
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_password(&db, user.id, &new_hash).is_err() {
            return render_failed(&cfg, &theme, &current);
        }
    }
    Redirect::to("/settings").into_response()
}
