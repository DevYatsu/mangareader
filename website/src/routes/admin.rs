//! Admin dashboard + branding (site identity, legal text, homepage flags).
//!
//! Every endpoint requires [`AuthUser`] with the `Admin` role. Logged-out
//! requests hit the `AuthUser` extractor rejection (login redirect for
//! browsers); non-admin users get `403` via [`require_role`].
//!
//! POST validation checks every field before the transactional settings write,
//! so invalid input cannot partially rebrand the site.

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use mangaread_core::domain::Role;
use maud::{Markup, html};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{AuthUser, require_role},
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

/// Parsed branding submission.
#[derive(Default)]
struct BrandingForm {
    site_title: String,
    site_tagline: String,
    site_accent: String,
    site_bg: String,
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
    form.seo_description = one.remove("seo_description").unwrap_or_default();
    form.legal_terms = one.remove("legal_terms").unwrap_or_default();
    form.legal_privacy = one.remove("legal_privacy").unwrap_or_default();
    form
}

fn stored(map: &HashMap<String, String>, key: &str) -> String {
    map.get(key).cloned().unwrap_or_default()
}

fn is_displayable_reset_link(link: &str, base: &str) -> bool {
    let Ok(base_url) = reqwest::Url::parse(base) else {
        return false;
    };
    let Ok(link_url) = reqwest::Url::parse(link) else {
        return false;
    };
    let (Some(base_host), Some(link_host)) = (base_url.host_str(), link_url.host_str()) else {
        return false;
    };
    if !matches!(base_url.scheme(), "http" | "https")
        || base_url.scheme() != link_url.scheme()
        || base_host != link_host
        || base_url.port_or_known_default() != link_url.port_or_known_default()
        || !base_url.username().is_empty()
        || base_url.password().is_some()
        || !link_url.username().is_empty()
        || link_url.password().is_some()
        || link_url.query().is_some()
        || link_url.fragment().is_some()
    {
        return false;
    }

    let base_path = base_url.path().trim_end_matches('/');
    let Some(reset_path) = link_url.path().strip_prefix(base_path) else {
        return false;
    };
    let Some(token) = reset_path.strip_prefix("/reset/") else {
        return false;
    };
    !token.is_empty() && !token.contains('/')
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

fn persist_branding(
    conn: &mut rusqlite::Connection,
    pairs: &[(&str, String)],
    updated_by: i64,
) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    for (key, value) in pairs {
        tx.execute(
            "INSERT INTO site_settings (key, value, updated_by, updated_at) VALUES (?1, ?2, ?3, datetime('now')) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_by = excluded.updated_by, updated_at = datetime('now')",
            rusqlite::params![key, value, updated_by],
        )?;
    }
    tx.commit()
}

fn page(
    cfg: &crate::config::SiteConfig,
    title: &str,
    content: Markup,
    user: Option<&AuthUser>,
) -> Html<String> {
    crate::templates::layout::page(
        cfg,
        title,
        crate::templates::layout::Nav::Admin,
        user,
        content,
    )
}

fn dashboard_content() -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Admin" }
            div class="columns is-multiline" {
                div class="column is-half-mobile is-one-third-tablet" {
                    a class="card" href="/admin/branding" {
                        div class="card-content" {
                            h2 class="title is-4" { "Branding" }
                            p { "Site title, tagline, colors, SEO, legal text, and homepage sections." }
                        }
                        div class="card-footer" {
                            span class="card-footer-item" { "Open branding" }
                        }
                    }
                }
                div class="column is-half-mobile is-one-third-tablet" {
                    a class="card" href="/admin/users" {
                        div class="card-content" {
                            h2 class="title is-4" { "Users" }
                            p { "Review accounts and generate one-time password reset links." }
                        }
                        div class="card-footer" {
                            span class="card-footer-item" { "Open users" }
                        }
                    }
                }
                div class="column is-half-mobile is-one-third-tablet" {
                    div class="card" {
                        div class="card-content" {
                            h2 class="title is-4" { "Sources" }
                            p class="m-text-muted" { "Source management is not available yet." }
                        }
                        div class="card-footer" {
                            span class="card-footer-item" {
                                button class="button is-static" type="button" disabled="" { "Unavailable" }
                            }
                        }
                    }
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
        section {
            h1 class="title is-2" { "Branding" }
            a class="button is-light" href="/admin" { "Back to admin" }
            @if let Some(msg) = error {
                div class="notification is-danger is-light" { (msg) }
            }
            div class="box" {
                form id="admin-branding-form" method="post" action="/admin/branding" {
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="admin-branding-site-title-label" for="admin-branding-site-title" { "Site title (1-64 chars)" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="admin-branding-site-title" class="input" type="text" name="site_title" maxlength="64" required="" value=(stored(map, "site_title"));
                            }
                        }
                    }
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="admin-branding-tagline-label" for="admin-branding-tagline" { "Tagline (up to 140 chars)" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="admin-branding-tagline" class="input" type="text" name="site_tagline" maxlength="140" value=(stored(map, "site_tagline"));
                            }
                        }
                    }
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="admin-branding-accent-label" for="admin-branding-accent" { "Accent color" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="admin-branding-accent" class="input" type="color" name="site_accent" value=(stored(map, "site_accent"));
                            }
                        }
                    }
                    div class="field is-horizontal" {
                        div class="field-label" {
                            label class="label" id="admin-branding-background-label" for="admin-branding-background" { "Background color" }
                        }
                        div class="field-body" {
                            div class="control" {
                                input id="admin-branding-background" class="input" type="color" name="site_bg" value=(stored(map, "site_bg"));
                            }
                        }
                    }
                    div class="field" {
                        label class="label" id="admin-branding-seo-description-label" for="admin-branding-seo-description" { "SEO description (up to 300 chars)" }
                        div class="control" {
                            textarea id="admin-branding-seo-description" class="textarea" name="seo_description" maxlength="300" { (stored(map, "seo_description")) }
                        }
                    }
                    div class="field" {
                        label class="label" id="admin-branding-terms-label" for="admin-branding-terms" { "Terms of Service (plain text)" }
                        div class="control" {
                            textarea id="admin-branding-terms" class="textarea" name="legal_terms" rows="6" { (stored(map, "legal_terms")) }
                        }
                    }
                    div class="field" {
                        label class="label" id="admin-branding-privacy-label" for="admin-branding-privacy" { "Privacy Policy (plain text)" }
                        div class="control" {
                            textarea id="admin-branding-privacy" class="textarea" name="legal_privacy" rows="6" { (stored(map, "legal_privacy")) }
                        }
                    }
                    div class="field" {
                        fieldset {
                            legend class="label" { "Homepage sections" }
                            @for (key, label) in HOME_SECTIONS {
                                p class="control" {
                                    label class="checkbox" id=(format!("admin-branding-home-{key}-label")) for=(format!("admin-branding-home-{key}")) {
                                        input id=(format!("admin-branding-home-{key}")) type="checkbox" name="home_sections" value=(key) checked[checked.iter().any(|s| s == key)];
                                        (label)
                                    }
                                }
                            }
                        }
                    }
                    div class="field" {
                        div class="control" {
                            button class="button is-primary" type="submit" { "Save branding" }
                        }
                    }
                }
            }
            script src="/static/admin.js" {}
        }
    }
}

/// GET `/admin` — dashboard with links to the existing account surfaces.
pub async fn dashboard(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    page(&cfg, "Admin", dashboard_content(), Some(&user)).into_response()
}

/// GET `/admin/branding` — branding form prefilled from stored settings.
pub async fn branding_page(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (cfg, settings) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            mangaread_core::db::all_settings(&db).unwrap_or_default(),
        )
    };
    let checked = normalize_sections(&[stored(&settings, "home_sections")]);
    page(
        &cfg,
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
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    // Re-render prefill keeps the submitted values so no input is lost.
    let submitted: HashMap<String, String> = HashMap::from([
        ("site_title".into(), form.site_title.clone()),
        ("site_tagline".into(), form.site_tagline.clone()),
        ("site_accent".into(), form.site_accent.clone()),
        ("site_bg".into(), form.site_bg.clone()),
        ("seo_description".into(), form.seo_description.clone()),
        ("legal_terms".into(), form.legal_terms.clone()),
        ("legal_privacy".into(), form.legal_privacy.clone()),
        (
            "home_sections".into(),
            normalize_sections(&form.home_sections).join(","),
        ),
    ]);
    let checked = normalize_sections(&form.home_sections);
    let render_400 = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            page(
                &cfg,
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
        let mut db = state.db.lock().await;
        let pairs = [
            ("site_title", title),
            ("site_tagline", form.site_tagline.clone()),
            ("site_accent", form.site_accent.trim().to_string()),
            ("site_bg", form.site_bg.trim().to_string()),
            ("seo_description", form.seo_description.clone()),
            ("legal_terms", form.legal_terms.clone()),
            ("legal_privacy", form.legal_privacy.clone()),
            ("home_sections", checked.join(",")),
        ];
        if persist_branding(&mut db, &pairs, user.id).is_err() {
            return render_400("Could not save branding.");
        }
    }
    Redirect::to("/admin/branding").into_response()
}

fn users_content(users: &[(mangaread_core::domain::User, String)], notice: Markup) -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Users" }
            (notice)
            div class="box" {
                div class="table-container" {
                    table class="table is-fullwidth is-hoverable" {
                        thead {
                            tr {
                                th { "ID" }
                                th { "Name" }
                                th { "Email" }
                                th { "Role" }
                                th { "OAuth" }
                                th { "Created" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            @for (u, prov) in users {
                                tr {
                                    td { (u.id) }
                                    td { (u.name) }
                                    td { (u.email) }
                                    td { (u.role.as_str()) }
                                    td { (prov) }
                                    td { (u.created_at) }
                                    td {
                                        form method="post" action=(format!("/admin/users/{}/reset-link", u.id)) {
                                            button class="button is-small is-light" type="submit" { "Reset link" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            a class="button is-light" href="/admin" { "Back to admin" }
        }
    }
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
    let (cfg, users) = {
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
            // The display label is independent of whether the attacker-controlled
            // link may be rendered as an anchor.
            let is_safe = is_displayable_reset_link(link, &crate::oauth::base_url());
            if is_safe {
                html! { div class="notification is-success is-light" { "Reset link for user " (id) ": " a href=(link) { (link) } } }
            } else {
                html! { div class="notification is-danger is-light" { "Reset link for user " (id) ": " code { (link) } } }
            }
        }
        _ => html! {},
    };
    page(&cfg, "Users", users_content(&users, notice), Some(&user)).into_response()
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

#[cfg(test)]
mod tests {
    use super::*;
    use mangaread_core::domain::{Role, User};

    #[test]
    fn branding_markup_uses_bulma_without_visible_theme_controls() {
        let map = HashMap::from([
            ("site_title".into(), "MangaRead".into()),
            ("site_tagline".into(), "Read onward".into()),
            ("site_accent".into(), "#f59e0b".into()),
            ("site_bg".into(), "#131210".into()),
        ]);
        let markup = branding_content(&map, &[], Some("Invalid branding.")).into_string();

        for class_name in [
            "box",
            "field is-horizontal",
            "field-label",
            "field-body",
            "input",
            "textarea",
            "notification is-danger is-light",
            "title is-2",
        ] {
            assert!(
                markup.contains(class_name),
                "missing Bulma class: {class_name}"
            );
        }
        for id in [
            "admin-branding-site-title",
            "admin-branding-tagline",
            "admin-branding-accent",
            "admin-branding-background",
            "admin-branding-seo-description",
            "admin-branding-terms",
            "admin-branding-privacy",
            "admin-branding-home-latest",
            "admin-branding-home-popular",
            "admin-branding-home-continue",
        ] {
            assert!(markup.contains(&format!("id=\"{id}-label\" for=\"{id}\"")));
            assert!(markup.contains(&format!("id=\"{id}\"")));
        }
        assert!(markup.contains("id=\"admin-branding-form\""));
        assert!(markup.contains("method=\"post\" action=\"/admin/branding\""));
        assert!(markup.contains("name=\"site_title\""));
        assert!(markup.contains("name=\"site_tagline\""));
        assert!(markup.contains("name=\"site_accent\""));
        assert!(markup.contains("name=\"site_bg\""));
        assert!(markup.contains("name=\"seo_description\""));
        assert!(markup.contains("name=\"legal_terms\""));
        assert!(markup.contains("name=\"legal_privacy\""));
        assert!(markup.contains("name=\"home_sections\""));
        assert!(!markup.contains("name=\"site_theme\""));
        assert!(!markup.contains(concat!("theme", "-picker")));
        assert!(!markup.contains("Current theme"));
        assert!(!markup.contains(concat!("admin", "-form")));
        assert!(!markup.contains(concat!("admin", "-section")));
    }

    #[test]
    fn repeated_home_sections_and_csv_values_normalize() {
        let repeated = parse_branding(
            "site_title=Read&home_sections=latest&home_sections=continue&home_sections=popular",
        );
        assert_eq!(
            normalize_sections(&repeated.home_sections),
            ["latest", "continue", "popular"]
        );

        let csv = parse_branding("home_sections=latest%2Ccontinue");
        assert_eq!(
            normalize_sections(&csv.home_sections),
            ["latest", "continue"]
        );
    }

    #[test]
    fn reset_link_accepts_configured_origin() {
        assert!(is_displayable_reset_link(
            "http://localhost:3000/reset/token123",
            "http://localhost:3000"
        ));
    }

    #[test]
    fn reset_link_rejects_external_https_host() {
        assert!(!is_displayable_reset_link(
            "https://evil.example/reset/token123",
            "http://localhost:3000"
        ));
    }

    #[test]
    fn reset_link_rejects_javascript_and_malformed_urls() {
        assert!(!is_displayable_reset_link(
            "javascript:alert(1)",
            "http://localhost:3000"
        ));
        assert!(!is_displayable_reset_link(
            "not a url",
            "http://localhost:3000"
        ));
    }

    #[test]
    fn reset_link_rejects_wrong_path() {
        assert!(!is_displayable_reset_link(
            "http://localhost:3000/change-password/token123",
            "http://localhost:3000"
        ));
    }

    #[test]
    fn reset_link_rejects_wrong_port() {
        assert!(!is_displayable_reset_link(
            "http://localhost:3001/reset/token123",
            "http://localhost:3000"
        ));
    }

    #[test]
    fn users_table_uses_bulma_and_keeps_reset_form_contract() {
        let user = User {
            id: 7,
            name: "Reader".into(),
            email: "reader@example.com".into(),
            password_hash: "hash".into(),
            role: Role::Member,
            locale: "en".into(),
            created_at: "2026-09-24 12:00:00".into(),
        };
        let markup = users_content(&[(user, "google".into())], html! {}).into_string();
        assert!(markup.contains("class=\"table-container\""));
        assert!(markup.contains("class=\"table is-fullwidth is-hoverable\""));
        assert!(markup.contains("action=\"/admin/users/7/reset-link\""));
        assert!(markup.contains("method=\"post\""));
        assert!(markup.contains(">Reset link<"));
    }

    #[test]
    fn branding_settings_write_rolls_back_on_database_failure() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        mangaread_core::db::init_db(&conn).unwrap();
        mangaread_core::db::set_setting(&conn, "site_title", "Original", 1).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_privacy BEFORE INSERT ON site_settings
             WHEN NEW.key = 'legal_privacy' BEGIN
                 SELECT RAISE(ABORT, 'test failure');
             END;",
        )
        .unwrap();

        let pairs = [
            ("site_title", "Changed".to_string()),
            ("legal_privacy", "Should not persist".to_string()),
        ];
        assert!(persist_branding(&mut conn, &pairs, 1).is_err());
        assert_eq!(
            mangaread_core::db::get_setting(&conn, "site_title").unwrap(),
            Some("Original".into())
        );
    }
}
