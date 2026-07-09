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
            let content = html! { h2 { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
        Ok(c) => c,
        Err(_) => {
            let content = html! { h2 { "Chapter not found" } };
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
        div class="reader-nav" {
            @if let Some(prev) = prev_chapter {
                a href={"/read/"(manga_id)"/"(prev.id)} { "← Ch. " (prev.number) }
            } @else {
                span { "← First chapter" }
            }
            span class="current" { (manga.title) " — Ch. " (chapter.number) }
            @if let Some(next) = next_chapter {
                a href={"/read/"(manga_id)"/"(next.id)} { "Ch. " (next.number) " →" }
            } @else {
                span { "Last chapter →" }
            }
        }
        div class="reader" {
            @for page in &pages {
                img src=(page.image_url) alt={"Page "(page.index)};
            }
        }
        div class="reader-nav" {
            @if let Some(prev) = prev_chapter {
                a href={"/read/"(manga_id)"/"(prev.id)} { "← Ch. " (prev.number) }
            } @else {
                span { "← First chapter" }
            }
            span { (pages.len()) " pages" }
            @if let Some(next) = next_chapter {
                a href={"/read/"(manga_id)"/"(next.id)} { "Ch. " (next.number) " →" }
            } @else {
                span { "Last chapter →" }
            }
        }
    };

    Html(crate::templates::layout::base_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
    ).0)
}
