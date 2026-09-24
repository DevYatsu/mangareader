use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

/// Convert a SQLite datetime string (e.g. "2026-07-09 12:00:00") into a
/// human-friendly relative time like "5h ago", "2d ago".
fn relative_time(dt: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let parts: Vec<&str> = dt.split(|c| c == ' ' || c == '-' || c == ':').collect();
    if parts.len() != 6 {
        return dt.to_string();
    }

    let year = parts[0].parse::<i64>().unwrap_or(0);
    let month = parts[1].parse::<i64>().unwrap_or(0);
    let day = parts[2].parse::<i64>().unwrap_or(0);
    let hour = parts[3].parse::<i64>().unwrap_or(0);
    let minute = parts[4].parse::<i64>().unwrap_or(0);
    let second = parts[5].parse::<i64>().unwrap_or(0);

    let days_in_month = [0, 31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut total_days = (year - 1970) * 365 + (year - 1969) / 4;
    for m in 1..month {
        total_days += days_in_month[m as usize];
    }
    if month > 2 && (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)) {
        total_days += 1;
    }
    total_days += day - 1;

    let ts = total_days * 86400 + hour * 3600 + minute * 60 + second;
    let diff = (now - ts).max(0);

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else if diff < 604800 {
        format!("{}d ago", diff / 86400)
    } else if diff < 2592000 {
        format!("{}w ago", diff / 604800)
    } else if diff < 31536000 {
        format!("{}mo ago", diff / 2592000)
    } else {
        format!("{}y ago", diff / 31536000)
    }
}

/// GET `/` — render the landing page.
pub async fn home(State(state): State<Arc<AppState>>, user: Option<AuthUser>) -> Html<String> {
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
    let series_count = mangaread_core::db::count_manga_with_chapters(&db).unwrap_or(0);
    let chapter_count: i64 = db
        .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
        .unwrap_or(0);
    let recent = mangaread_core::db::list_recent_chapters(&db, 30).unwrap_or_default();

    let enabled: Vec<String> = mangaread_core::db::get_setting(&db, "home_sections")
        .ok()
        .flatten()
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let show = |key: &str| enabled.is_empty() || enabled.iter().any(|s| s == key);

    let content = html! {
        section class="hero" {
            div class="hero-body" {
                h1 class="title is-1" { "MangaRead" }
                p class="subtitle is-4 m-text-muted" {
                    "Your personal manga library. Read, track, and discover."
                }
                div class="level is-mobile" {
                    div class="level-item has-text-centered" {
                        div {
                            p class="heading" { (series_count) }
                            p class="title is-7" { "Series" }
                        }
                    }
                    div class="level-item has-text-centered" {
                        div {
                            p class="heading" { (chapter_count) }
                            p class="title is-7" { "Chapters" }
                        }
                    }
                }
                a href="/series" class="button is-primary is-medium" {
                    svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true" {
                        path d="M5 12h14M12 5l7 7-7 7" {}
                    }
                    "Browse All Series"
                }
            }
        }

        @if show("continue") {
            section id="continue-reading-container" {}
        }

        @if show("latest") {
            h2 class="title is-3" { "Recently Updated" }
            @if recent.is_empty() {
                (content::empty_state(
                    "No chapters yet. The sync loop is fetching series in the background.",
                    true,
                ))
            } @else {
                ul id="recent-chapters" {
                    @for ch in &recent {
                        li {
                            a class="media" href={"/read/"(ch.manga_id)"/"(ch.chapter_id)} {
                                @if let Some(ref cover) = ch.cover_url {
                                    span class="media-left" {
                                        div class="image is-64x64" {
                                            img src=(cover) alt="" loading="lazy" {}
                                        }
                                    }
                                }
                                span class="media-content" {
                                    span class="title is-6" { (ch.series_title) }
                                    span class="subtitle is-7" {
                                        "Ch. " (ch.chapter_number)
                                        @if let Some(ref t) = ch.chapter_title { (t) }
                                    }
                                }
                                span class="media-right" {
                                    span class="m-text-muted" { (relative_time(&ch.created_at)) }
                                    span aria-hidden="true" {
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
        }
    };

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    Html(
        crate::templates::layout::base_layout(
            "Home",
            content,
            banner.as_deref(),
            &css,
            &cfg.site_title,
            user.as_ref(),
            crate::templates::layout::Nav::Home,
        )
        .0,
    )
}
