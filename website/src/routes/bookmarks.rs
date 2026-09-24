use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

pub async fn bookmarks_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!(
                "Syncing series\u{2026} {} of {} ({}%)",
                p.current, p.total, pct
            ))
        } else {
            None
        }
    };

    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
    let css = cfg.theme_overrides();
    let content_markup = html! {
        div class="level" {
            div class="level-left" {
                h1 class="title is-2" { "Bookmarks" }
            }
        }
        div id="bookmarks-container" class="columns is-mobile is-multiline" {}
        (content::empty_state_with_id(
            "No bookmarked series yet.",
            false,
            "bookmarks-empty",
            true,
        ))
    };

    Html(
        crate::templates::layout::base_layout(
            "Bookmarks",
            content_markup,
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user.as_ref(),
            crate::templates::layout::Nav::Bookmarks,
        )
        .0,
    )
}
