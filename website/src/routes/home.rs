use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

/// Convert a SQLite datetime string (e.g. "2026-07-09 12:00:00") into a
/// human-friendly relative time like "5h ago", "2d ago".
fn relative_time(dt: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    // Parse "YYYY-MM-DD HH:MM:SS"
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

    // Simple day-counting (not leap-year perfect, but good enough for relative time)
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
///
/// Queries series count, chapter count, unique sources, and the 30 most
/// recently updated chapters, then renders them inside the `base_layout`.
pub async fn home(State(state): State<Arc<AppState>>, user: Option<AuthUser>) -> Html<String> {
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

    let series_count = mangaread_core::db::count_manga_with_chapters(&db)
        .unwrap_or(0);
    let chapter_count: i64 = db
        .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
        .unwrap_or(0);
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
            div class="empty-state" {
                span class="empty-state-icon" {}
                p { "No chapters yet. The sync loop is fetching series in the background." }
            }
        } @else {
            ul class="recent-list" {
                @for ch in &recent {
                    li class="recent-item" {
                        a href={"/read/"(ch.manga_id)"/"(ch.chapter_id)} {
                            @if let Some(ref cover) = ch.cover_url {
                                img class="recent-thumb" src=(cover) alt="" loading="lazy";
                            }
                            div class="recent-body" {
                                span class="recent-title" { (ch.series_title) }
                                div class="recent-chapter" {
                                    span class="recent-chapter-num" { "Ch. " (ch.chapter_number) }
                                    @if let Some(ref t) = ch.chapter_title {
                                        span class="recent-chapter-title" { (t) }
                                    }
                                }
                            }
                            span class="recent-time" { (relative_time(&ch.created_at)) }
                            span class="recent-arrow" {
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

    let cfg = crate::config::site_config(&db);
    let css = cfg.css_overrides();
    let theme = crate::config::resolve_theme(&db, user.as_ref());
    Html(crate::templates::layout::base_layout("Home", content, banner.as_deref(), &css, &cfg.site_title, user.as_ref(), &theme).0)
}
