use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

fn manga_href(series_id: Option<i64>, manga_id: i64) -> String {
    match series_id {
        Some(sid) => format!("/series/{}", sid),
        None => format!("/manga/{}", manga_id),
    }
}

fn chapter_badge(count: i64) -> String {
    if count == 1 {
        format!("{} Ch.", count)
    } else {
        format!("{} Chs.", count)
    }
}

/// GET `/source/{source}` — list all manga from a given source.
pub async fn source_list(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
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
    let manga_list: Vec<_> = mangaread_core::db::list_manga_by_source_with_count(&db, &source)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.chapter_count > 0)
        .collect();
    let total_manga = manga_list.len();
    let current_href = format!("/source/{}", source);
    let cards: Vec<_> = manga_list
        .iter()
        .map(|item| {
            let m = &item.manga;
            let href = manga_href(m.series_id, m.id);
            let badge = chapter_badge(item.chapter_count);
            content::manga_card(&href, &m.title, m.cover_url.as_deref(), Some(&badge))
        })
        .collect();

    let content = html! {
        (content::breadcrumb(&[
            ("/", "Home"),
            ("/series", "Series"),
            (&current_href, &source),
        ]))
        div class="level" {
            div class="level-left" {
                h1 class="title is-2" { (source) }
            }
            div class="level-right" {
                span class="tag m-text-muted" {
                    (total_manga)
                    @if total_manga == 1 { " manga" } @else { " manga" }
                }
            }
        }

        @if manga_list.is_empty() {
            (content::empty_state("No manga found for this source.", false))
        } @else {
            (content::manga_grid(&cards))
        }
    };

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            &source,
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
