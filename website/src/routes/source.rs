use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn source_list(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
) -> Html<String> {
    let db = state.db.lock().await;
    let manga_list =
        mangaread_core::db::list_manga_by_source(&db, &source).unwrap_or_default();

    let content = html! {
        h2 class="page-title" {
            "Manga from "
            span class="accent" { (source) }
        }
        @if manga_list.is_empty() {
            div class="empty-state" {
                p { "No manga found for this source." }
            }
        } @else {
            div class="manga-grid" {
                @for m in &manga_list {
                    div class="manga-card" {
                        a href={"/manga/"(m.id)} {
                            div class="cover-wrapper" {
                                @if let Some(ref cover) = m.cover_url {
                                    img src=(cover) alt=(m.title);
                                } @else {
                                    div class="cover-placeholder" { "No cover" }
                                }
                            }
                            div class="info" {
                                h3 { (m.title) }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&source, content).0)
}
