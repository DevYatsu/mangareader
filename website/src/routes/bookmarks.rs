use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn bookmarks_page(State(state): State<Arc<AppState>>) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!("Syncing series\u{2026} {} of {} ({}%)", p.current, p.total, pct))
        } else {
            None
        }
    };

    let css = state.config.css_overrides();
    let content = html! {
        div class="bookmarks-header" {
            h1 class="page-title" { "Bookmarks" }
        }
        div id="bookmarks-container" class="manga-grid" {}
        div id="bookmarks-empty" class="empty-state" style="display:none" {
            p { "No bookmarked series yet." }
        }
    };

    Html(crate::templates::layout::base_layout("Bookmarks", content, banner.as_deref(), &css, &state.config.site_title).0)
}
