use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

pub async fn bookmarks_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let cfg = {
        let db = state.db.lock().await;
        crate::config::site_config(&db)
    };
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

    crate::templates::layout::page(
        &cfg,
        "Bookmarks",
        crate::templates::layout::Nav::Bookmarks,
        user.as_ref(),
        content_markup,
    )
}
