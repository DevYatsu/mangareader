use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

/// GET `/source/{source}` — list all manga from a given source.
///
/// Renders a grid of manga cards for entries matching the specified source
/// name, with chapter counts and links to series or manga detail pages.
pub async fn source_list(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
    user: Option<AuthUser>,
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
    let manga_list: Vec<_> = mangaread_core::db::list_manga_by_source_with_count(&db, &source)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.chapter_count > 0)
        .collect();

    let total_manga = manga_list.len();

    let content = html! {
        // ── Breadcrumb ──
        nav class="breadcrumb" {
            a href="/" { "Home" }
            span class="breadcrumb-sep" { "/" }
            span class="breadcrumb-current" { (source) }
        }

        // ── Source Header ──
        div class="source-header" {
            h1 class="source-title" {
                (source)
            }
            span class="source-meta-badge" {
                (total_manga)
                @if total_manga == 1 {
                    " manga"
                } @else {
                    " manga"
                }
            }
        }

        @if manga_list.is_empty() {
            div class="empty-state" {
                p { "No manga found for this source." }
            }
        } @else {
            div class="manga-grid" {
                @for item in &manga_list {
                    @let m = &item.manga;
                    @let count = item.chapter_count;
                    @let series_url = if let Some(sid) = m.series_id {
                        format!("/series/{}", sid)
                    } else {
                        format!("/manga/{}", m.id)
                    };
                    div class="manga-card" {
                        a href=(series_url) {
                            div class="cover-wrapper" {
                                @if let Some(ref cover) = m.cover_url {
                                    img src=(cover) alt=(m.title) loading="lazy";
                                } @else {
                                    div class="cover-placeholder" {
                                        div class="cover-initial" {
                                            span { (m.title.chars().next().unwrap_or('?').to_uppercase()) }
                                        }
                                    }
                                }
                                @if count > 0 {
                                    div class="chapter-badge" {
                                        (count)
                                        @if count == 1 {
                                            " Ch."
                                        } @else {
                                            " Chs."
                                        }
                                    }
                                }
                                div class="card-overlay" {}
                            }
                            div class="info" {
                                h3 { (m.title) }
                            }
                        }
                    }
                }
            }
        }
    };

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    let theme = crate::config::resolve_theme(&db, user.as_ref());
    Html(crate::templates::layout::base_layout(&source, content, banner.as_deref(), &css, &cfg.site_title, user.as_ref(), &theme).0)
}
