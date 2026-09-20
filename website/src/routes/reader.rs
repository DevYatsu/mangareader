use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

/// GET `/read/{manga_id}/{chapter_id}` — render the chapter reader page.
///
/// Loads the manga, chapter, and page list from the database. If no pages
/// exist yet (unsynced chapter), triggers a background sync and displays a
/// loading state with polling-based auto-reload. Provides prev/next chapter
/// navigation and uses the reader-specific layout.
pub async fn chapter_reader(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path((manga_id, chapter_id)): Path<(i64, i64)>,
) -> Html<String> {
    // Check if pages exist — quick peek, release lock
    let (manga, chapter, needs_sync) = {
        let db = state.db.lock().await;
        let manga = match mangaread_core::db::get_manga_by_id(&db, manga_id) {
            Ok(m) => m,
            Err(_) => {
                return Html(crate::templates::layout::error_layout(
                    "Not Found",
                    "This manga doesn't exist or may have been removed.",
                ).0);
            }
        };
        let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
            Ok(c) => c,
            Err(_) => {
                return Html(crate::templates::layout::error_layout(
                    "Not Found",
                    "This chapter doesn't exist or may have been removed.",
                ).0);
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
            div class="loading-state" {
                div class="skeleton skeleton-page" {}
                div class="skeleton skeleton-page" style="aspect-ratio:3/5" {}
                div class="skeleton skeleton-page" style="aspect-ratio:3/4" {}
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
    };

    let css = state.config.css_overrides();
    Html(crate::templates::layout::reader_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
        &css,
        &state.config.site_title,
        user.as_ref(),
    ).0)
}
