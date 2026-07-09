use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn source_list(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
) -> Html<String> {
    let db = state.db.lock().await;
    let manga_list =
        mangaread_core::db::list_manga_by_source_with_count(&db, &source).unwrap_or_default();

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

    Html(crate::templates::layout::base_layout(&source, content).0)
}
