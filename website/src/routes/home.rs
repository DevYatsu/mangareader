use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

/// GET `/` — render the landing page.
///
/// Queries series count, chapter count, unique sources, and the 30 most
/// recently updated chapters, then renders them inside the `base_layout`.
pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let db = state.db.lock().await;

    let series_count = mangaread_core::db::count_manga_with_chapters(&db)
        .unwrap_or(0);
    let chapter_count: i64 = db
        .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
        .unwrap_or(0);
    let sources_concat: String = db
        .query_row(
            "SELECT IFNULL(GROUP_CONCAT(DISTINCT source, ', '), '') FROM manga",
            [],
            |row| row.get(0),
        )
        .unwrap_or_default();
    let source_count = if sources_concat.is_empty() {
        0
    } else {
        sources_concat.split(", ").count()
    };

    let recent = mangaread_core::db::list_recent_chapters(&db, 30).unwrap_or_default();

    let content = html! {
        // ── Hero Section ──
        section class="hero-section" {
            div class="hero-content" {
                h1 class="hero-title" { "MangaRead" }
                p class="hero-tagline" { "Your personal manga library. Read, track, and discover." }
                div class="hero-stats" {
                    div class="hero-stat" {
                        span class="hero-stat-value" { (series_count) }
                        span class="hero-stat-label" { "Series" }
                    }
                    div class="hero-stat-divider" {}
                    div class="hero-stat" {
                        span class="hero-stat-value" { (chapter_count) }
                        span class="hero-stat-label" { "Chapters" }
                    }
                    div class="hero-stat-divider" {}
                    div class="hero-stat" {
                        span class="hero-stat-value" { (source_count) }
                        span class="hero-stat-label" { "Sources" }
                    }
                }
                a href="/series" class="browse-btn" {
                    svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                        path d="M5 12h14M12 5l7 7-7 7" {}
                    }
                    "Browse All Series"
                }
            }
        }

        // ── Continue Reading ──
        div id="continue-reading-container" class="continue-section" {}

        // ── Recent Chapters ──
        h2 class="section-title" { "Recently Updated" }
        @if recent.is_empty() {
            @for _ in 0..5 {
                div class="skeleton skeleton-row" {}
            }
        } @else {
            ul class="chapter-list" {
                @for ch in &recent {
                    li {
                        a href={"/read/"(ch.manga_id)"/"(ch.chapter_id)} {
                            @if let Some(ref cover) = ch.cover_url {
                                img class="chapter-list-thumb" src=(cover) alt="" loading="lazy";
                            }
                            div class="chapter-list-body" {
                                span class="chapter-list-title" { (ch.series_title) }
                                span class="chapter-num" { "Ch. " (ch.chapter_number) }
                                @if let Some(ref t) = ch.chapter_title {
                                    span class="chapter-title" { (t) }
                                }
                            }
                            span class="chapter-list-date" { (ch.created_at) }
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
    };

    Html(crate::templates::layout::base_layout("Home", content).0)
}
