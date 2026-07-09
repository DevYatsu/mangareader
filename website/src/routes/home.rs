use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::collections::HashMap;
use std::sync::Arc;

use crate::AppState;

pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let db = state.db.lock().await;
    let sources = mangaread_core::db::list_manga_sources(&db).unwrap_or_default();

    // Build source -> manga count map
    let all_manga = mangaread_core::db::list_all_manga(&db).unwrap_or_default();
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in &all_manga {
        *counts.entry(m.source.clone()).or_insert(0) += 1;
    }

    let total_manga = all_manga.len();
    let total_sources = sources.len();

    let content = html! {
        // ── Hero Section ──
        section class="hero-section" {
            div class="hero-content" {
                h1 class="hero-title" { "MangaRead" }
                p class="hero-tagline" { "Your personal manga library. Read, track, and discover." }
                div class="hero-stats" {
                    div class="hero-stat" {
                        span class="hero-stat-value" { (total_manga) }
                        span class="hero-stat-label" { "Manga" }
                    }
                    div class="hero-stat-divider" {}
                    div class="hero-stat" {
                        span class="hero-stat-value" { (total_sources) }
                        span class="hero-stat-label" { "Sources" }
                    }
                }
            }
        }

        // ── Sources Grid ──
        h2 class="section-title" { "Browse by Source" }
        @if sources.is_empty() {
            div class="empty-state" {
                p { "No manga sources found. Run the scraper first!" }
            }
        } @else {
            div class="source-grid" {
                @for s in &sources {
                    @let count = counts.get(s).copied().unwrap_or(0);
                    a class="source-card" href={"/source/"(s)} {
                        div class="source-card-glow" {}
                        div class="source-card-body" {
                            div class="source-card-icon" {
                                span { (s.chars().next().unwrap_or('?').to_uppercase()) }
                            }
                            div class="source-card-info" {
                                h3 { (s) }
                                p {
                                    (count)
                                    @if count == 1 {
                                        " manga"
                                    } @else {
                                        " manga"
                                    }
                                }
                            }
                        }
                        div class="source-card-arrow" {
                            svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M5 12h14M12 5l7 7-7 7" {}
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout("Home", content).0)
}
