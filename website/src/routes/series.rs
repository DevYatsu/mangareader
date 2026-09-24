use axum::extract::{Path, Query, State};
use axum::response::Html;
use maud::html;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

fn chapter_badge(count: i64) -> String {
    if count == 1 {
        format!("{} Ch.", count)
    } else {
        format!("{} Chs.", count)
    }
}

/// GET `/series/{id}` — render the series detail page.
pub async fn series_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
    user: Option<AuthUser>,
) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!(
                "Syncing series\u{2026} {} of {} ({}%)",
                p.current, p.total, pct
            ))
        } else {
            None
        }
    };

    let db = state.db.lock().await;
    let s = match mangaread_core::db::get_series_by_id(&db, id) {
        Ok(s) => s,
        Err(_) => {
            let cfg = crate::config::site_config(&db);
            let css = cfg.css_overrides();
            return Html(
                crate::templates::layout::base_layout(
                    "Not Found",
                    crate::templates::layout::error_content(
                        "Not Found",
                        "This series doesn't exist or may have been removed.",
                    ),
                    None,
                    &css,
                    &cfg.site_title,
                    user.as_ref(),
                    crate::templates::layout::Nav::Series,
                )
                .0,
            );
        }
    };

    let manga_entries = mangaread_core::db::list_manga_by_series(&db, id).unwrap_or_default();
    if manga_entries.is_empty() {
        let current_href = format!("/series/{}", id);
        let content = html! {
            (content::breadcrumb(&[("/", "Home"), ("/series", "Series"), (&current_href, &s.title)]))
            h2 class="title is-3" { "Series not found" }
        };
        let cfg = crate::config::site_config(&db);
        let css = cfg.css_overrides();
        return Html(
            crate::templates::layout::base_layout(
                &s.title,
                content,
                banner.as_deref(),
                &css,
                &cfg.site_title,
                user.as_ref(),
                crate::templates::layout::Nav::Series,
            )
            .0,
        );
    }

    let fresh = mangaread_core::service::series::is_series_fresh(&manga_entries, &db, 30);
    if !fresh {
        let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
            .unwrap_or(&manga_entries[0]);
        tracing::info!(
            "series {} is stale, background sync triggered for manga {}",
            id,
            primary.id
        );
        crate::scraper_helper::sync_manga_background(primary.id);
    }

    let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
        .unwrap_or(&manga_entries[0]);
    let chapter_count = mangaread_core::db::list_chapters_by_manga(&db, primary.id)
        .map(|c| c.len())
        .unwrap_or(0);
    let pref_src = mangaread_core::service::series::preferred_source(&manga_entries, &db)
        .unwrap_or("")
        .to_string();
    let merged = mangaread_core::service::series::merge_chapters(&manga_entries, &pref_src, &db);
    let display_chapters: Vec<(&str, i64, i64, f64)> = merged
        .iter()
        .map(|c| (c.source.as_str(), c.manga_id, c.chapter_id, c.number))
        .collect();
    let first_chapter = mangaread_core::service::series::first_chapter(&manga_entries, &db);
    let cover_url = primary.cover_url.clone();
    let all_sources: Vec<&str> = manga_entries.iter().map(|m| m.source.as_str()).collect();
    let sources_json = serde_json::to_string(&all_sources).unwrap_or_else(|_| "[]".to_string());
    let current_href = format!("/series/{}", id);

    let content = html! {
        (content::breadcrumb(&[("/", "Home"), ("/series", "Series"), (&current_href, &s.title)]))

        div id="series-detail-hero" class="media" {
            div class="media-left" {
                (content::cover(None, &s.title, cover_url.as_deref(), None))
            }
            div class="media-content" {
                h1 class="title" { (s.title) }
                @if let Some(ref desc) = primary.description {
                    p class="subtitle is-6" { (desc) }
                }
                div class="tags" {
                    @if !pref_src.is_empty() {
                        span class="tag is-primary is-light" { (pref_src) }
                    }
                    @if chapter_count > 0 {
                        span class="tag m-text-muted" {
                            (chapter_count)
                            @if chapter_count == 1 { " Chapter" } @else { " Chapters" }
                        }
                    }
                }
                @if let Some(ref fc) = first_chapter {
                    div class="buttons" {
                        a href={"/read/"(fc.manga_id)"/"(fc.chapter_id)} class="button is-primary is-medium" {
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true" {
                                polygon points="5 3 19 12 5 21 5 3" {}
                            }
                            "Start Reading"
                        }
                        button class="button is-small is-ghost" data-series-id=(s.id) data-bookmark-button {
                            svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" {
                                path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" {}
                            }
                            span data-bookmark-label { "Bookmark" }
                        }
                        button id="rescan-btn" class="button is-small is-ghost rescan-btn" data-manga-id=(primary.id) {
                            svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true" {
                                path d="M23 4v6h-6M1 20v-6h6" {}
                                path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" {}
                            }
                            " Refresh"
                        }
                    }
                }
            }
        }

        @if !fresh {
            script { (maud::PreEscaped(format!(
                "setTimeout(function(){{location.reload()}},15000)"
            ))) }
        }

        div {
            div class="level" {
                div class="level-left" {
                    h2 class="title is-3" { "Chapters" }
                }
                @if chapter_count > 0 {
                    div class="level-right" {
                        span class="tag m-text-muted" { (chapter_count) }
                    }
                }
            }

            @if all_sources.len() > 1 {
                div id="source-toggles" class="field is-grouped is-grouped-multiline" {}
                button id="source-reset" class="button is-small is-ghost" type="button" { "Reset filters" }
            }

            @if !fresh && display_chapters.is_empty() {
                div class="has-text-centered" {
                    div class="loader" {}
                    p class="m-text-muted" { "Fetching chapters from the source…" }
                    p class="m-text-muted" { "This page will reload automatically." }
                }
            } @else if display_chapters.is_empty() {
                (content::empty_state("No chapters scraped yet for any source.", false))
            } @else {
                ul id="chapter-list" data-series-id=(s.id) data-sources=(sources_json) {
                    @for &(src, manga_id, ch_id, num) in &display_chapters {
                        li class="m-chapter-row" data-source=(src) data-chapter-id=(ch_id) {
                            a href={"/read/"(manga_id)"/"(ch_id)} {
                                span class="ch-status-icon" aria-hidden="true" {}
                                span class="chapter-num" { "Ch. " (num) }
                                span class="chapter-source tag" { (src) }
                                span class="ch-resume" style="display:none" {}
                                span class="chapter-arrow" aria-hidden="true" {
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

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            &s.title,
            content,
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user.as_ref(),
            crate::templates::layout::Nav::Series,
        )
        .0,
    )
}

/// GET `/series` — render the series listing page.
pub async fn series_list(
    State(state): State<Arc<AppState>>,
    query: Query<HashMap<String, String>>,
    user: Option<AuthUser>,
) -> Html<String> {
    let banner = {
        let p = state.auto_sync.lock().await;
        if p.running && p.total > 0 {
            let pct = p.current * 100 / p.total;
            Some(format!(
                "Syncing series\u{2026} {} of {} ({}%)",
                p.current, p.total, pct
            ))
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
        list.into_iter().filter(|s| s.total_chapters > 0).collect()
    };
    let cards: Vec<_> = all_series
        .iter()
        .map(|swm| {
            let s = &swm.series;
            let cover = swm
                .manga_entries
                .iter()
                .max_by_key(|m| {
                    mangaread_core::db::list_chapters_by_manga(&db, m.id)
                        .map(|c| c.len())
                        .unwrap_or(0)
                })
                .or_else(|| swm.manga_entries.first())
                .and_then(|m| m.cover_url.as_ref());
            let href = format!("/series/{}", s.id);
            let badge = chapter_badge(swm.total_chapters);
            content::manga_card(&href, &s.title, cover.map(String::as_str), Some(&badge))
        })
        .collect();

    let content = html! {
        div class="box" {
            div class="level" {
                div class="level-left" {
                    h1 class="title is-2" { "Series" }
                }
                div class="level-right" {
                    form class="field has-addons" action="/series" method="get" {
                        div class="control" {
                            input class="input" type="search" name="q" placeholder="Search series…" value=(search);
                        }
                        div class="control" {
                            button class="button is-primary" type="submit" { "Search" }
                        }
                    }
                }
            }
        }

        @if all_series.is_empty() {
            @if search.is_empty() {
                (content::empty_state(
                    "No series synced yet. The background sync loop will fetch them.",
                    true,
                ))
            } @else {
                (content::empty_state(&format!("No series match \"{}\".", search), false))
            }
        } @else {
            (content::manga_grid(&cards))
        }
    };

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            "Series",
            content,
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user.as_ref(),
            crate::templates::layout::Nav::Series,
        )
        .0,
    )
}
