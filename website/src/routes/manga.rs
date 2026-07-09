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
            let content = html! { h2 class="page-title" { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapters = mangaread_core::db::list_chapters_by_manga(&db, manga.id).unwrap_or_default();

    let content = html! {
        a class="back-link" href={"/source/"(manga.source)} { "← Back to source" }

        div class="manga-detail" {
            div class="cover" {
                @if let Some(ref cover) = manga.cover_url {
                    img src=(cover) alt=(manga.title);
                }
            }
            div class="meta" {
                h2 { (manga.title) }
                @if let Some(ref author) = manga.author {
                    p class="author" { "by " (author) }
                }
                @if let Some(ref desc) = manga.description {
                    p class="description" { (desc) }
                }
                span class="source-badge" { (manga.source) }
            }
        }

        h3 class="section-title" { "Chapters" }
        @if chapters.is_empty() {
            div class="empty-state" {
                p { "No chapters scraped yet." }
            }
        } @else {
            ul class="chapter-list" {
                @for ch in &chapters {
                    li {
                        a href={"/read/"(manga.id)"/"(ch.id)} {
                            span class="chapter-num" { "Ch. " (ch.number) }
                            @if let Some(ref t) = ch.title {
                                span class="chapter-title" { " — " (t) }
                            }
                            span class="chapter-arrow" { "→" }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&manga.title, content).0)
}
