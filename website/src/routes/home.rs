use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let db = state.db.lock().await;
    let sources = mangaread_core::db::list_manga_sources(&db).unwrap_or_default();

    let content = html! {
        h2 { "Sources" }
        @if sources.is_empty() {
            p { "No manga sources found. Run the scraper first!" }
        } @else {
            div class="source-list" {
                ul {
                    @for s in &sources {
                        li { a href={"/source/"(s)} { (s) } }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout("Home", content).0)
}
