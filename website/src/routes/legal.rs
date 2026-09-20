//! Public legal pages (`/terms`, `/privacy`).
//!
//! Both render the admin-edited plain text in the standard `base_layout`
//! shell (the `error_layout`-style document shell with header and footer).
//! Maud escapes the text, so stored markup never renders as HTML.

use axum::{extract::State, response::Html};
use maud::html;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

fn legal_content(heading: &str, body: &str) -> maud::Markup {
    html! {
        section class="legal-section" {
            h1 class="page-title" { (heading) }
            p class="legal-body" { (body) }
        }
    }
}

async fn legal_page(
    state: &Arc<AppState>,
    user: Option<&AuthUser>,
    title: &str,
    heading: &str,
    key: &str,
    fallback: &str,
) -> Html<String> {
    let (cfg, body) = {
        let db = state.db.lock().await;
        let text = mangaread_core::db::get_setting(&db, key)
            .ok()
            .flatten()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string());
        (crate::config::site_config(&db), text)
    };
    let banner: Option<String> = None;
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            title,
            legal_content(heading, &body),
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user,
            crate::templates::layout::Nav::None,
        )
        .0,
    )
}

/// GET `/terms` — Terms of Service from the `legal_terms` setting.
pub async fn terms(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    legal_page(
        &state,
        user.as_ref(),
        "Terms",
        "Terms of Service",
        "legal_terms",
        "Terms of Service — coming soon.",
    )
    .await
}

/// GET `/privacy` — Privacy Policy from the `legal_privacy` setting.
pub async fn privacy(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    legal_page(
        &state,
        user.as_ref(),
        "Privacy",
        "Privacy Policy",
        "legal_privacy",
        "Privacy Policy — coming soon.",
    )
    .await
}
