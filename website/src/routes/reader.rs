use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn chapter_reader(
    State(state): State<Arc<AppState>>,
    Path((manga_id, chapter_id)): Path<(i64, i64)>,
) -> Html<String> {
    // Check if pages exist — quick peek, release lock
    let (manga, chapter, needs_sync) = {
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
        (manga, chapter, pages.is_empty())
    };

    let syncing = needs_sync;
    if needs_sync {
        crate::scraper_helper::sync_chapter_background(chapter_id);
    }

    // Re-fetch with fresh data
    let db = state.db.lock().await;
    let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();

    let back_url = if let Some(sid) = manga.series_id {
        format!("/series/{}", sid)
    } else {
        format!("/manga/{}", manga_id)
    };

    // Fetch all chapters for prev/next nav
    let all_chapters =
        mangaread_core::db::list_chapters_by_manga(&db, manga_id).unwrap_or_default();
    let current_idx = all_chapters.iter().position(|c| c.id == chapter.id);
    let prev_chapter = current_idx.and_then(|i| i.checked_sub(1)).and_then(|i| all_chapters.get(i));
    let next_chapter = current_idx.and_then(|i| all_chapters.get(i + 1));
    let total_pages = pages.len();

    let reload_script = if syncing {
        Some(format!(
            r#"<script>
(function(){{
let a=0;
const iv=setInterval(()=>{{
fetch('/api/check-chapter/{}').then(r=>r.json()).then(d=>{{if(d.pages>0){{clearInterval(iv);location.reload()}}}}).catch(()=>{{}});
if(++a>=20)clearInterval(iv)
}},1500)
}})()
</script>"#,
            chapter_id
        ))
    } else {
        None
    };

    let content = html! {
        @if let Some(script) = reload_script {
            (maud::PreEscaped(script))
        }

        // Tracking element (invisible)
        div id="reader-track" style="display:none"
            data-series-id=(manga.series_id.unwrap_or(0))
            data-chapter-id=(chapter.id)
            data-manga-id=(manga_id)
            data-total-pages=(total_pages) {}
        div id="resume-target" style="display:none" data-page=(0) {}

        // ── Top Navigation Bar (hidden by default, shows on scroll-up) ──
        div id="reader-topbar" class="reader-topbar" {
            div class="reader-topbar-inner" {
                a class="reader-back-btn" href=(back_url) {
                    svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                        path d="M19 12H5m7-7-7 7 7 7" {}
                    }
                }
                div class="reader-topbar-info" {
                    span class="reader-topbar-title" { (manga.title) }
                    span class="reader-topbar-chapter" { "Ch. " (chapter.number) }
                }
                span id="reader-page-indicator" class="reader-page-indicator" {
                    "1 / " (total_pages)
                }
            }
        }

        // ── Loading state ──
        @if syncing {
            div class="loading-state reader-loading" {
                div class="spinner" {}
                p { "Fetching pages from the source…" }
                p class="loading-hint" { "This page will reload automatically." }
            }
        } @else {
            // Click zones for page navigation
            div id="reader-click-left" class="reader-click-left" title="Previous page" {}
            div id="reader-click-right" class="reader-click-right" title="Next page" {}

            // Page images — full width, stacked
            div id="reader-images" class="reader-images" {
                @for (i, page) in pages.iter().enumerate() {
                    @let load = if i == 0 { "eager" } else { "lazy" };
                    div class="reader-page" data-page-idx=(i + 1) {
                        img src=(page.image_url) alt={"Page "(i + 1)} loading=(load) decoding="async";
                    }
                }
            }

            // ── Bottom Navigation (after all pages) ──
            div class="reader-bottom-nav" {
                div class="reader-bottom-nav-inner" {
                    @if let Some(prev) = prev_chapter {
                        a href={"/read/"(manga_id)"/"(prev.id)} class="reader-nav-btn" {
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M19 12H5m7-7-7 7 7 7" {}
                            }
                            "Ch. " (prev.number)
                        }
                    } @else {
                        span class="reader-nav-btn disabled" {
                            "First Chapter"
                        }
                    }
                    a href=(back_url) class="reader-nav-btn secondary" { "Series Page" }
                    @if let Some(next) = next_chapter {
                        a href={"/read/"(manga_id)"/"(next.id)} class="reader-nav-btn" {
                            "Ch. " (next.number)
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M5 12h14m-7-7 7 7-7 7" {}
                            }
                        }
                    } @else {
                        span class="reader-nav-btn disabled" {
                            "Last Chapter"
                        }
                    }
                }
            }

            // Subtle hint that fades out
            div id="reader-hint" class="reader-hint" { "Tap sides to navigate pages" }
        }

        script src="/static/app.js" {}
    };

    Html(crate::templates::layout::reader_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
    ).0)
}
