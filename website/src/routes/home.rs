use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};
use mangaread_core::domain::ContinueItem;
use maud::Markup;

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

/// A chapter list row: cover, series, chapter, age.
fn chapter_list(chapters: &[mangaread_core::domain::RecentChapter], list_id: &str) -> Markup {
    html! {
        ul id=(list_id) class="m-chapter-feed" {
            @for ch in chapters {
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
                                @if let Some(ref t) = ch.chapter_title { " · " (t) }
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

fn continue_cards(items: &[ContinueItem]) -> Markup {
    let cards: Vec<Markup> = items
        .iter()
        .map(|i| {
            let badge = if i.is_next {
                format!("Next: Ch. {}", i.chapter_number)
            } else if i.page > 1 {
                format!("Ch. {} · p. {}", i.chapter_number, i.page)
            } else {
                format!("Ch. {}", i.chapter_number)
            };
            content::manga_card(
                &format!("/read/{}/{}", i.manga_id, i.chapter_id),
                &i.series_title,
                i.cover_url.as_deref(),
                Some(&badge),
            )
        })
        .collect();
    content::manga_grid(&cards)
}

/// GET `/` — what a reader came back for, in that order: continue reading,
/// new chapters in their library, then everything new.
pub async fn home(State(state): State<Arc<AppState>>, user: Option<AuthUser>) -> Html<String> {
    let db = state.db.lock().await;
    let recent = mangaread_core::db::list_recent_chapters(&db, 30).unwrap_or_default();
    let (continuing, library_new) = match &user {
        Some(u) => (
            mangaread_core::db::continue_reading(&db, u.id, 12).unwrap_or_default(),
            mangaread_core::db::library_updates(&db, u.id, 10).unwrap_or_default(),
        ),
        None => (Vec::new(), Vec::new()),
    };
    let tagline = mangaread_core::db::get_setting(&db, "site_tagline")
        .ok()
        .flatten()
        .filter(|t| !t.trim().is_empty());

    let enabled: Vec<String> = mangaread_core::db::get_setting(&db, "home_sections")
        .ok()
        .flatten()
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let show = |key: &str| enabled.is_empty() || enabled.iter().any(|s| s == key);
    let cfg = crate::config::site_config(&db);

    let content = html! {
        @if user.is_none() {
            section class="m-home-intro" {
                h1 class="title is-2" { (cfg.site_title) }
                p class="subtitle is-5 m-text-muted" {
                    (tagline.as_deref().unwrap_or("Read, track, and discover manga."))
                }
                div class="buttons" {
                    a href="/series" class="button is-primary" { "Browse series" }
                    a href="/register" class="button is-light" { "Create an account" }
                }
            }
        }

        @if show("continue") {
            @if user.is_some() {
                @if !continuing.is_empty() {
                    section class="m-home-section" {
                        h2 class="title is-4" { "Continue reading" }
                        (continue_cards(&continuing))
                    }
                }
                @if !library_new.is_empty() {
                    section class="m-home-section" {
                        div class="level is-mobile" {
                            div class="level-left" { h2 class="title is-4" { "New in your library" } }
                            div class="level-right" { a href="/library" { "Library" } }
                        }
                        (chapter_list(&library_new, "library-chapters"))
                    }
                }
            } @else {
                // Visitors: filled from this device's progress by app.js.
                section id="continue-reading-container" {}
            }
        }

        @if show("latest") {
            section class="m-home-section" {
                div class="level is-mobile" {
                    div class="level-left" { h2 class="title is-4" { "Latest updates" } }
                    div class="level-right" { a href="/series" { "All series" } }
                }
                @if recent.is_empty() {
                    (content::empty_state(
                        "No chapters yet. New chapters appear here as the library fills up.",
                        true,
                    ))
                } @else {
                    (chapter_list(&recent, "recent-chapters"))
                }
            }
        }
    };

    crate::templates::layout::page(
        &cfg,
        "Home",
        crate::templates::layout::Nav::Home,
        user.as_ref(),
        content,
    )
}

#[cfg(test)]
mod route_tests {
    use crate::test_support::app_with_admin;

    #[tokio::test]
    async fn members_see_continue_and_library_sections_visitors_see_intro() {
        let (app, _) = app_with_admin().await;
        let (uid, member) = app.member("m@example.com").await;
        {
            let db = app.state.db.lock().await;
            let sid = mangaread_core::db::insert_series(&db, "Kept Series").unwrap();
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id) VALUES ('a','k','K','u',?1)",
                [sid],
            )
            .unwrap();
            db.execute(
                "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (last_insert_rowid(),'1',1,'u')",
                [],
            )
            .unwrap();
            let ch = db.last_insert_rowid();
            mangaread_core::db::set_shelf(&db, uid, sid, mangaread_core::domain::Shelf::Reading)
                .unwrap();
            db.execute(
                "INSERT INTO progress (user_id, chapter_id, page, status) VALUES (?1, ?2, 4, 'started')",
                [uid, ch],
            )
            .unwrap();
            mangaread_core::db::set_setting(&db, "site_title", "Shelf Club", 1).unwrap();
        }
        let home = app.get("/", Some(&member)).await.body;
        assert!(home.contains("Continue reading"));
        assert!(home.contains("Ch. 1 · p. 4"));
        assert!(home.contains("New in your library"));
        assert!(!home.contains("continue-reading-container"));
        let visitor = app.get("/", None).await.body;
        assert!(
            visitor.contains("Shelf Club"),
            "site title, not a hardcoded name"
        );
        assert!(visitor.contains("continue-reading-container"));
        assert!(visitor.contains("Latest updates"));
        assert!(!visitor.contains("New in your library"));
    }
}
