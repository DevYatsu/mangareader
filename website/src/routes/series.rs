use axum::extract::{Path, Query, State};
use axum::response::Html;
use maud::html;
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

/// GET `/series/{id}` — render the series detail page.
///
/// Loads the series, its manga entries (one per source), and merges chapters
/// from all sources into a unified list. Triggers a background sync if the
/// series data is stale (>30 minutes old). Returns a "not found" page when
/// the series does not exist.
pub async fn series_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!("Syncing series\u{2026} {} of {} ({}%)", p.current, p.total, pct))
        } else {
            None
        }
    };

    let db = state.db.lock().await;

    // Load series
    let s = match mangaread_core::db::get_series_by_id(&db, id) {
        Ok(s) => s,
        Err(_) => {
            return Html(crate::templates::layout::error_layout(
                "Not Found",
                "This series doesn't exist or may have been removed.",
            ).0);
        }
    };

    // Load manga entries for this series
    let manga_entries = mangaread_core::db::list_manga_by_series(&db, id).unwrap_or_default();
    if manga_entries.is_empty() {
        let content = html! {
            nav class="breadcrumb" {
                a href="/" { "Home" }
                span class="breadcrumb-sep" { "/" }
                span class="breadcrumb-current" { (s.title) }
            }
            h2 class="page-title" { "Series not found" }
        };
        return Html(crate::templates::layout::base_layout(&s.title, content, banner.as_deref()).0);
    }

    // Use the new service layer for domain logic
    let fresh = mangaread_core::service::series::is_series_fresh(&manga_entries, &db, 30);
    if !fresh {
        let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
            .unwrap_or(&manga_entries[0]);
        tracing::info!("series {} is stale, background sync triggered for manga {}", id, primary.id);
        crate::scraper_helper::sync_manga_background(primary.id);
    }

    let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
        .unwrap_or(&manga_entries[0]);
    let chapter_count = mangaread_core::db::list_chapters_by_manga(&db, primary.id)
        .map(|c| c.len())
        .unwrap_or(0);

    let pref_src = mangaread_core::service::series::preferred_source(&manga_entries, &db)
        .unwrap_or("").to_string();
    let merged = mangaread_core::service::series::merge_chapters(&manga_entries, &pref_src, &db);

    let display_chapters: Vec<(&str, i64, i64, f64)> = merged
        .iter()
        .map(|c| (c.source.as_str(), c.manga_id, c.chapter_id, c.number))
        .collect();

    let first_chapter = mangaread_core::service::series::first_chapter(&manga_entries, &db);

    let cover_url = primary.cover_url.clone();

    // Collect unique sources for filter UI
    let all_sources: Vec<&str> = manga_entries.iter().map(|m| m.source.as_str()).collect();

    // Render page count for each chapter (for resume display)
    // We'll embed it as a data attribute

    let content = html! {
        // ── Breadcrumb ──
        nav class="breadcrumb" {
            a href="/" { "Home" }
            span class="breadcrumb-sep" { "/" }
            span class="breadcrumb-current" { (s.title) }
        }

        // ── Hero Detail ──
        div class="manga-hero" {
            div class="manga-hero-cover" {
                @if let Some(ref cover) = cover_url {
                    img src=(cover) alt=(s.title);
                } @else {
                    div class="manga-hero-placeholder" {
                        div class="manga-hero-initial" {
                            span { (s.title.chars().next().unwrap_or('?').to_uppercase()) }
                        }
                    }
                }
            }
            div class="manga-hero-info" {
                h1 { (s.title) }
                @if let Some(ref desc) = primary.description {
                    p class="manga-hero-desc" { (desc) }
                }
                div class="manga-hero-meta" {
                    span class="source-badge" { (pref_src) }
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
                @if let Some(ref fc) = first_chapter {
                    div class="manga-hero-actions" {
                        a href={"/read/"(fc.manga_id)"/"(fc.chapter_id)} class="start-btn" {
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                polygon points="5 3 19 12 5 21 5 3" {}
                            }
                            "Start Reading"
                        }
                        button id="rescan-btn" class="rescan-btn" data-manga-id=(primary.id) {
                            svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M23 4v6h-6M1 20v-6h6" {}
                                path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" {}
                            }
                            " Refresh"
                        }
                    }
                }
            }
        }

        // Auto-reload if we just kicked off a sync
        @if !fresh {
            script { (maud::PreEscaped(format!(
                "setTimeout(function(){{location.reload()}},15000)"
            ))) }
        }

        // ── Chapter Section ──
        div class="chapter-section" {
            h2 class="section-title" {
                "Chapters"
                @if chapter_count > 0 {
                    span class="section-count" { (chapter_count) }
                }
            }

            // Source filter toggles
            @if all_sources.len() > 1 {
                div id="source-toggles" class="source-toggles" {}
                button id="source-reset" class="source-reset-btn" { "Reset filters" }
            }

            @if !fresh && display_chapters.is_empty() {
                div class="loading-state" {
                    div class="spinner" {}
                    p { "Fetching chapters from the source…" }
                    p class="loading-hint" { "This page will reload automatically." }
                }
            } @else if display_chapters.is_empty() {
                div class="empty-state" {
                    p { "No chapters scraped yet for any source." }
                }
            } @else {
                ul id="chapter-list" class="chapter-list"
                    data-series-id=(s.id)
                    data-sources=(maud::PreEscaped(serde_json::to_string(&all_sources).unwrap_or_else(|_| "[]".to_string()))) {
                    @for &(src, manga_id, ch_id, num) in &display_chapters {
                        li data-source=(src) data-chapter-id=(ch_id) {
                            a href={"/read/"(manga_id)"/"(ch_id)} {
                                span class="ch-status-icon" {}
                                span class="chapter-num" { "Ch. " (num) }
                                span class="chapter-source" { (src) }
                                span class="ch-resume" style="display:none" {}
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

        // Client-side features
        script src="/static/app.js" {}
    };

    Html(crate::templates::layout::base_layout(&s.title, content, banner.as_deref()).0)
}

/// GET `/series` — render the series listing page.
///
/// Accepts an optional `?q=` query parameter to filter by title search.
/// Renders a grid of manga cards with cover images and chapter counts.
pub async fn series_list(
    State(state): State<Arc<AppState>>,
    query: Query<HashMap<String, String>>,
) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!("Syncing series\u{2026} {} of {} ({}%)", p.current, p.total, pct))
        } else {
            None
        }
    };

    let search = query.get("q").map(|s| s.trim()).unwrap_or("");
    let db = state.db.lock().await;

    let all_series: Vec<_> = {
        let list = if search.is_empty() {
            mangaread_core::db::list_all_series(&db).unwrap_or_default()
        } else {
            mangaread_core::db::search_series(&db, search).unwrap_or_default()
        };
        // Only show series that have at least one chapter
        list.into_iter().filter(|s| s.total_chapters > 0).collect()
    };

    let content = html! {
        // ── Search Header ──
        div class="search-card" {
            h1 class="page-title" { "Series" }
            form class="search-form" action="/series" method="get" {
                input type="search" name="q" placeholder="Search series…"
                    value=(search);
                button type="submit" {
                    svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                        path d="M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16m10 2-4.35-4.35" {}
                    }
                }
            }
        }

        @if all_series.is_empty() {
            @if search.is_empty() {
                div class="empty-state" {
                    span class="empty-state-icon" {}
                    p { "No series synced yet. The background sync loop will fetch them." }
                }
            } @else {
                div class="empty-state" {
                    p { "No series match \"" (search) "\"." }
                }
            }
        } @else {
            div class="manga-grid" {
                @for swm in &all_series {
                    @let s = &swm.series;
                    @let count = swm.total_chapters;
                    // Pick the manga entry with most chapters for the cover (consistent with detail page)
                    @let cover = swm.manga_entries.iter()
                        .max_by_key(|m| mangaread_core::db::list_chapters_by_manga(&db, m.id).map(|c| c.len()).unwrap_or(0))
                        .or_else(|| swm.manga_entries.first())
                        .and_then(|m| m.cover_url.as_ref());
                    div class="manga-card" {
                        a href={"/series/"(s.id)} {
                            div class="cover-wrapper" {
                                @if let Some(c) = cover {
                                    img src=(c) alt=(s.title) loading="lazy";
                                } @else {
                                    div class="cover-placeholder" {
                                        div class="cover-initial" {
                                            span { (s.title.chars().next().unwrap_or('?').to_uppercase()) }
                                        }
                                    }
                                }
                                @if count > 0 {
                                    div class="chapter-badge" { (count) " Chs." }
                                }
                                div class="card-overlay" {}
                            }
                            div class="info" {
                                h3 { (s.title) }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout("Series", content, banner.as_deref()).0)
}
