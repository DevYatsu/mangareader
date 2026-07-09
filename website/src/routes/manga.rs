use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn manga_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Html<String> {
    let db = state.db.lock().await;

    let manga = match mangaread_core::db::get_manga_by_id(&db, id) {
        Ok(m) => m,
        Err(_) => {
            let content = html! { h2 { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapters = mangaread_core::db::list_chapters_by_manga(&db, manga.id).unwrap_or_default();

    let content = html! {
        div class="manga-detail" {
            @if let Some(ref cover) = manga.cover_url {
                img src=(cover) alt=(manga.title);
            }
            div class="meta" {
                h2 { (manga.title) }
                @if let Some(ref author) = manga.author {
                    p class="author" { "by " (author) }
                }
                @if let Some(ref desc) = manga.description {
                    p class="description" { (desc) }
                }
                p { "Source: " (manga.source) }
            }
        }
        h3 { "Chapters" }
        @if chapters.is_empty() {
            p { "No chapters scraped yet." }
        } @else {
            ul class="chapter-list" {
                @for ch in &chapters {
                    li {
                        a href={"/read/"(manga.id)"/"(ch.id)} {
                            span class="chapter-num" { "Ch. " (ch.number) }
                            @if let Some(ref t) = ch.title {
                                span { " — " (t) }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&manga.title, content).0)
}
