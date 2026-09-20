//! Admin dashboard + branding (site title, theme, legal, homepage flags).
//!
//! Every endpoint requires [`AuthUser`] with the `Admin` role. Logged-out
//! requests hit the `AuthUser` extractor rejection (login redirect for
//! browsers); non-admin users get `403` via [`require_role`].
//!
//! POST validation checks every field before any `set_setting` write, so an
//! invalid submission writes nothing.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use maud::{html, Markup};
use mangaread_core::domain::Role;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    auth::{require_role, AuthUser},
    AppState,
};

/// Homepage sections the branding form offers as checkboxes (stored CSV).
const HOME_SECTIONS: &[(&str, &str)] = &[
    ("latest", "Latest chapters"),
    ("popular", "Popular series"),
    ("continue", "Continue reading"),
];

/// Strict `#rrggbb` check: leading `#` required, exactly 6 hex digits.
fn is_hex_color(s: &str) -> bool {
    let bytes = s.as_bytes();
    bytes.len() == 7 && bytes[0] == b'#' && bytes[1..].iter().all(|c| c.is_ascii_hexdigit())
}

fn theme_label(key: &str) -> &str {
    crate::config::THEMES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| *label)
        .unwrap_or(key)
}

/// Parsed branding submission.
#[derive(Default)]
struct BrandingForm {
    site_title: String,
    site_tagline: String,
    site_accent: String,
    site_bg: String,
    site_theme: String,
    seo_description: String,
    legal_terms: String,
    legal_privacy: String,
    /// Every `home_sections` value in submission order.
    home_sections: Vec<String>,
}

/// Parse the urlencoded body by hand instead of via `Form<BrandingForm>`:
/// `serde_urlencoded` rejects repeated keys (`duplicate field`), but the
/// homepage-section checkboxes legitimately submit one `home_sections` key
/// per checked box. Single-value fields take the last occurrence; a single
/// CSV value and repeated keys normalize to the same list downstream.
fn parse_branding(body: &str) -> BrandingForm {
    let mut form = BrandingForm::default();
    let mut one = HashMap::<String, String>::new();
    for (key, value) in form_urlencoded::parse(body.as_bytes()) {
        if key == "home_sections" {
            form.home_sections.push(value.into_owned());
        } else {
            one.insert(key.into_owned(), value.into_owned());
        }
    }
    form.site_title = one.remove("site_title").unwrap_or_default();
    form.site_tagline = one.remove("site_tagline").unwrap_or_default();
    form.site_accent = one.remove("site_accent").unwrap_or_default();
    form.site_bg = one.remove("site_bg").unwrap_or_default();
    form.site_theme = one.remove("site_theme").unwrap_or_default();
    form.seo_description = one.remove("seo_description").unwrap_or_default();
    form.legal_terms = one.remove("legal_terms").unwrap_or_default();
    form.legal_privacy = one.remove("legal_privacy").unwrap_or_default();
    form
}

fn stored(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

/// Split checkbox values on commas so both repeated keys and a single CSV
/// value normalize to the same list.
fn normalize_sections(values: &[String]) -> Vec<String> {
    values
        .iter()
        .flat_map(|v| v.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn page(
    cfg: &crate::config::SiteConfig,
    theme: &str,
    title: &str,
    content: Markup,
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

fn dashboard_content(site_theme: &str) -> Markup {
    html! {
        section class="admin-section" {
            h1 class="page-title" { "Admin" }
            p class="admin-current-theme" { "Current theme: " strong { (theme_label(site_theme)) } }
            div class="admin-cards" {
                a class="admin-card" href="/admin/branding" {
                    h2 { "Branding" }
                    p { "Site title, tagline, colors, theme, SEO, legal text, homepage sections." }
                }
                div class="admin-card admin-card-disabled" {
                    h2 { "Users" }
                    p { "User management lands in Slice 5." }
                }
                div class="admin-card admin-card-disabled" {
                    h2 { "Sources" }
                    p { "Source management lands in Slice 5." }
                }
            }
        }
    }
}

fn branding_content(
    map: &HashMap<String, String>,
    checked: &[String],
    error: Option<&str>,
) -> Markup {
    html! {
        section class="admin-section" {
            h1 class="page-title" { "Branding" }
            p { a href="/admin" { "← Back to Admin" } }
            @if let Some(msg) = error {
                p class="auth-error" { (msg) }
            }
            form method="post" action="/admin/branding" class="admin-form" {
                label { "Site title (1-64 chars)" input type="text" name="site_title" maxlength="64" required="" value=(stored(map, "site_title")); }
                label { "Tagline (up to 140 chars)" input type="text" name="site_tagline" maxlength="140" value=(stored(map, "site_tagline")); }
                label { "Accent color" input type="color" name="site_accent" value=(stored(map, "site_accent")); }
                label { "Background color" input type="color" name="site_bg" value=(stored(map, "site_bg")); }
                fieldset class="theme-picker" {
                    legend { "Theme" }
                    @for (key, label) in crate::config::THEMES {
                        label class="theme-option" {
                            input type="radio" name="site_theme" value=(key) checked[stored(map, "site_theme") == *key];
                            (label)
                        }
                    }
                }
                label { "SEO description (up to 300 chars)" textarea name="seo_description" maxlength="300" { (stored(map, "seo_description")) } }
                label { "Terms of Service (plain text)" textarea name="legal_terms" rows="6" { (stored(map, "legal_terms")) } }
                label { "Privacy Policy (plain text)" textarea name="legal_privacy" rows="6" { (stored(map, "legal_privacy")) } }
                fieldset class="sections-picker" {
                    legend { "Homepage sections" }
                    @for (key, label) in HOME_SECTIONS {
                        label class="section-option" {
                            input type="checkbox" name="home_sections" value=(key) checked[checked.iter().any(|s| s == key)];
                            (label)
                        }
                    }
                }
                button type="submit" { "Save branding" }
            }
            script src="/static/admin.js" {}
        }
    }
}

/// GET `/admin` — dashboard with section links + current theme name.
pub async fn dashboard(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (cfg, theme, settings) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            mangaread_core::db::all_settings(&db).unwrap_or_default(),
        )
    };
    let site_theme = stored(&settings, "site_theme");
    let site_theme = if site_theme.is_empty() {
        "default".to_string()
    } else {
        site_theme
    };
    page(&cfg, &theme, "Admin", dashboard_content(&site_theme), Some(&user)).into_response()
}

/// GET `/admin/branding` — branding form prefilled from stored settings.
pub async fn branding_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (cfg, theme, settings) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            mangaread_core::db::all_settings(&db).unwrap_or_default(),
        )
    };
    let checked = normalize_sections(&[stored(&settings, "home_sections")]);
    page(
        &cfg,
        &theme,
        "Branding",
        branding_content(&settings, &checked, None),
        Some(&user),
    )
    .into_response()
}

/// POST `/admin/branding` — validate everything, then persist each key.
pub async fn branding_submit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    body: String,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let form = parse_branding(&body);
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
        )
    };
    // Re-render prefill keeps the submitted values so no input is lost.
    let submitted: HashMap<String, String> = HashMap::from([
        ("site_title".into(), form.site_title.clone()),
        ("site_tagline".into(), form.site_tagline.clone()),
        ("site_accent".into(), form.site_accent.clone()),
        ("site_bg".into(), form.site_bg.clone()),
        ("site_theme".into(), form.site_theme.clone()),
        ("seo_description".into(), form.seo_description.clone()),
        ("legal_terms".into(), form.legal_terms.clone()),
        ("legal_privacy".into(), form.legal_privacy.clone()),
        ("home_sections".into(), normalize_sections(&form.home_sections).join(",")),
    ]);
    let checked = normalize_sections(&form.home_sections);
    let render_400 = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
                &theme,
                "Branding",
                branding_content(&submitted, &checked, Some(msg)),
                Some(&user),
            ),
        )
            .into_response()
    };

    // Validate ALL fields before ANY write — invalid input writes nothing.
    let title = form.site_title.trim().to_string();
    if title.chars().count() < 1 || title.chars().count() > 64 {
        return render_400("Site title must be 1-64 characters.");
    }
    if form.site_tagline.chars().count() > 140 {
        return render_400("Tagline must be at most 140 characters.");
    }
    if !is_hex_color(form.site_accent.trim()) {
        return render_400("Accent must be a #rrggbb hex color.");
    }
    if !is_hex_color(form.site_bg.trim()) {
        return render_400("Background must be a #rrggbb hex color.");
    }
    if !crate::config::is_theme(&form.site_theme) {
        return render_400("Unknown theme.");
    }
    if form.seo_description.chars().count() > 300 {
        return render_400("SEO description must be at most 300 characters.");
    }
    if form.legal_terms.chars().count() > 20000 {
        return render_400("Terms must be at most 20000 characters.");
    }
    if form.legal_privacy.chars().count() > 20000 {
        return render_400("Privacy policy must be at most 20000 characters.");
    }

    {
        let db = state.db.lock().await;
        let pairs = [
            ("site_title", title),
            ("site_tagline", form.site_tagline.clone()),
            ("site_accent", form.site_accent.trim().to_string()),
            ("site_bg", form.site_bg.trim().to_string()),
            ("site_theme", form.site_theme.clone()),
            ("seo_description", form.seo_description.clone()),
            ("legal_terms", form.legal_terms.clone()),
            ("legal_privacy", form.legal_privacy.clone()),
            ("home_sections", checked.join(",")),
        ];
        for (key, value) in pairs {
            if mangaread_core::db::set_setting(&db, key, &value, user.id).is_err() {
                return render_400("Could not save branding.");
            }
        }
    }
    Redirect::to("/admin/branding").into_response()
}

/// GET `/admin/users` — minimal user table with per-row reset-link generator.
/// Full user management (roles, suspend, delete) is Slice 5 territory.
pub async fn users_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Query(q): axum::extract::Query<HashMap<String, String>>,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (cfg, theme, users) = {
        let db = state.db.lock().await;
        let users = mangaread_core::db::list_users(&db).unwrap_or_default();
        let providers: HashMap<i64, String> = users
            .iter()
            .map(|u| {
                let p = mangaread_core::db::user_oauth_providers(&db, u.id)
                    .unwrap_or_default()
                    .join(",");
                (u.id, p)
            })
            .collect();
        (
            crate::config::site_config(&db),
            crate::config::resolve_theme(&db, Some(&user)),
            users
                .into_iter()
                .map(|u| {
                    let prov = providers.get(&u.id).cloned().unwrap_or_default();
                    (u, prov)
                })
                .collect::<Vec<_>>(),
        )
    };
    let notice = match (q.get("reset_link"), q.get("for")) {
        (Some(link), Some(id)) => {
            html! { p class="admin-notice" { "Reset link for user " (id) ": " a href=(link) { (link) } } }
        }
        _ => html! {},
    };
    let content = html! {
        (notice)
        table class="admin-table" {
            tr { th { "ID" } th { "Name" } th { "Email" } th { "Role" } th { "OAuth" } th { "Created" } th { "Actions" } }
            @for (u, prov) in &users {
                tr {
                    td { (u.id) } td { (u.name) } td { (u.email) } td { (u.role.as_str()) }
                    td { (prov) } td { (u.created_at) }
                    td { form method="post" action=(format!("/admin/users/{}/reset-link", u.id)) { button type="submit" { "Reset link" } } }
                }
            }
        }
        p { a href="/admin" { "Back to admin" } }
    };
    page(&cfg, &theme, "Users", content, Some(&user)).into_response()
}

/// POST `/admin/users/{id}/reset-link` — mint a 1h token, redirect back with the link.
pub async fn user_reset_link(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let token = crate::auth::new_token();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::create_password_reset(
            &db,
            id,
            &token,
            &crate::routes::password_reset::reset_expires_at(),
        )
        .is_err()
        {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    let link = format!("{}/reset/{token}", crate::oauth::base_url());
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("reset_link", &link)
        .append_pair("for", &id.to_string())
        .finish();
    (
        StatusCode::SEE_OTHER,
        [(
            axum::http::header::LOCATION,
            format!("/admin/users?{query}"),
        )],
        "",
    )
        .into_response()
}
