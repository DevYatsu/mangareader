use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn chapter_reader(
    State(state): State<Arc<AppState>>,
    Path((manga_id, chapter_id)): Path<(i64, i64)>,
) -> Html<String> {
    let db = state.db.lock().await;

    let manga = match mangaread_core::db::get_manga_by_id(&db, manga_id) {
        Ok(m) => m,
        Err(_) => {
            let content = html! { h2 class="page-title" { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
        Ok(c) => c,
        Err(_) => {
            let content = html! { h2 class="page-title" { "Chapter not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();

    // Fetch all chapters for prev/next nav
    let all_chapters =
        mangaread_core::db::list_chapters_by_manga(&db, manga_id).unwrap_or_default();
    let current_idx = all_chapters.iter().position(|c| c.id == chapter.id);
    let prev_chapter = current_idx.and_then(|i| i.checked_sub(1)).and_then(|i| all_chapters.get(i));
    let next_chapter = current_idx.and_then(|i| all_chapters.get(i + 1));

    let content = html! {
        // Floating navigation overlay (hidden by default, shows on click)
        nav id="reader-nav" class="reader-nav" {
            div class="nav-top" {
                a class="back-link" href={"/manga/"(manga_id)} { "← Back" }
                span class="chapter-title" { (manga.title) " — Ch. " (chapter.number) }
            }
            div class="nav-controls" {
                @if let Some(prev) = prev_chapter {
                    a href={"/read/"(manga_id)"/"(prev.id)} class="nav-btn" {
                        "← Ch. " (prev.number)
                    }
                } @else {
                    span class="nav-btn disabled" { "← First" }
                }
                @if let Some(next) = next_chapter {
                    a href={"/read/"(manga_id)"/"(next.id)} class="nav-btn" {
                        "Ch. " (next.number) " →"
                    }
                } @else {
                    span class="nav-btn disabled" { "Last →" }
                }
            }
        }

        // Page images — full width, no chrome
        div id="reader-images" class="reader-images" {
            @for page in &pages {
                img src=(page.image_url) alt={"Page "(page.index)};
            }
        }

        // Subtle hint that fades out
        div id="reader-hint" class="reader-hint" { "Tap anywhere" }
    };

    Html(crate::templates::layout::reader_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
    ).0)
}
