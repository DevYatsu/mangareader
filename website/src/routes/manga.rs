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
    let chapter_count = chapters.len();

    let content = html! {
        // ── Breadcrumb ──
        nav class="breadcrumb" {
            a href="/" { "Home" }
            span class="breadcrumb-sep" { "/" }
            a href={"/source/"(manga.source)} { (manga.source) }
            span class="breadcrumb-sep" { "/" }
            span class="breadcrumb-current" { (manga.title) }
        }

        // ── Hero Detail ──
        div class="manga-hero" {
            div class="manga-hero-cover" {
                @if let Some(ref cover) = manga.cover_url {
                    img src=(cover) alt=(manga.title);
                } @else {
                    div class="manga-hero-placeholder" {
                        div class="manga-hero-initial" {
                            span { (manga.title.chars().next().unwrap_or('?').to_uppercase()) }
                        }
                    }
                }
            }
            div class="manga-hero-info" {
                h1 { (manga.title) }
                @if let Some(ref author) = manga.author {
                    p class="manga-hero-author" {
                        svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                            path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2" {}
                            circle cx="12" cy="7" r="4" {}
                        }
                        (author)
                    }
                }
                @if let Some(ref desc) = manga.description {
                    p class="manga-hero-desc" { (desc) }
                }
                div class="manga-hero-meta" {
                    span class="source-badge" { (manga.source) }
                    @if chapter_count > 0 {
                        span class="meta-stat" {
                            svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" {}
                                path d="M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z" {}
                            }
                            (chapter_count)
                            @if chapter_count == 1 {
                                " Chapter"
                            } @else {
                                " Chapters"
                            }
                        }
                    }
                }
            }
        }

        // ── Chapter Section ──
        div class="chapter-section" {
            h2 class="section-title" {
                "Chapters"
                @if chapter_count > 0 {
                    span class="section-count" { (chapter_count) }
                }
            }
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
                                    span class="chapter-title" { (t) }
                                }
                                span class="chapter-arrow" {
                                    svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                        path d="M5 12h14M12 5l7 7-7 7" {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&manga.title, content).0)
}
