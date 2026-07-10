//! Admin dashboard — source health, sync statistics, and diagnostics.
//!
//! # Routes
//! - `GET /admin` — overview of adapter health and sync loop status

use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

/// GET `/admin` — render the admin dashboard.
///
/// Shows per-source adapter health, sync loop progress, and
/// basic database statistics.
pub async fn admin(State(state): State<Arc<AppState>>) -> Html<String> {
    let db = state.db.lock().await;

    let series_count: i64 = db
        .query_row("SELECT COUNT(*) FROM series", [], |row| row.get(0))
        .unwrap_or(0);
    let manga_count: i64 = db
        .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
        .unwrap_or(0);
    let chapter_count: i64 = db
        .query_row("SELECT COUNT(*) FROM chapters", [], |row| row.get(0))
        .unwrap_or(0);

    let health = state.adapter_health.lock().await;
    let sync_p = state.auto_sync.lock().await;
    let syncing_ids = state.syncing_manga_ids.lock().await;

    let content = html! {
        nav class="breadcrumb" {
            a href="/" { "Home" }
            span class="breadcrumb-sep" { "/" }
            span class="breadcrumb-current" { "Admin" }
        }

        h1 class="page-title" { "Admin" }

        // ── Sync Loop ──
        h2 class="section-title" { "Sync Loop" }
        div class="admin-stats" {
            div class="admin-stat" {
                span class="admin-stat-label" { "Status" }
                span class="admin-stat-value" {
                    @if sync_p.running { "Running" } @else { "Idle" }
                }
            }
            div class="admin-stat" {
                span class="admin-stat-label" { "Progress" }
                span class="admin-stat-value" {
                    @if sync_p.total > 0 {
                        (sync_p.current) " / " (sync_p.total)
                    } @else {
                        "—"
                    }
                }
            }
            div class="admin-stat" {
                span class="admin-stat-label" { "Active syncs" }
                span class="admin-stat-value" { (syncing_ids.len()) }
            }
        }

        // ── Source Health ──
        h2 class="section-title" { "Source Adapters" }
        @if health.is_empty() {
            p class="empty-state" { "No adapters registered." }
        } @else {
            table class="admin-table" {
                thead {
                    tr {
                        th { "Source" }
                        th { "Status" }
                        th { "Failures" }
                        th { "Last error" }
                    }
                }
                tbody {
                    @for (_key, h) in health.iter() {
                        tr {
                            td { (h.source) }
                            td {
                                @if h.healthy {
                                    span class="badge badge-ok" { "Healthy" }
                                } @else {
                                    span class="badge badge-err" { "Unhealthy" }
                                }
                            }
                            td { (h.consecutive_failures) }
                            td {
                                @if let Some(ref err) = h.last_error {
                                    code class="admin-error" { (err) }
                                } @else {
                                    "—"
                                }
                            }
                        }
                    }
                }
            }
        }

        // ── Database ──
        h2 class="section-title" { "Database" }
        div class="admin-stats" {
            div class="admin-stat" {
                span class="admin-stat-label" { "Series" }
                span class="admin-stat-value" { (series_count) }
            }
            div class="admin-stat" {
                span class="admin-stat-label" { "Manga entries" }
                span class="admin-stat-value" { (manga_count) }
            }
            div class="admin-stat" {
                span class="admin-stat-label" { "Chapters" }
                span class="admin-stat-value" { (chapter_count) }
            }
        }

        style {
            r#"
.admin-stats { display: flex; gap: 1.5rem; flex-wrap: wrap; margin-bottom: 1.5rem; }
.admin-stat { background: var(--bg-card); padding: 1rem 1.25rem; border-radius: var(--radius-md); border: 1px solid var(--border); min-width: 140px; }
.admin-stat-label { display: block; font-size: 0.75rem; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.3rem; }
.admin-stat-value { font-family: var(--font-display); font-size: 1.25rem; font-weight: 600; color: var(--text-primary); }
.admin-table { width: 100%; border-collapse: collapse; margin-bottom: 1.5rem; }
.admin-table th, .admin-table td { text-align: left; padding: 0.6rem 0.75rem; border-bottom: 1px solid var(--border); font-size: 0.9rem; }
.admin-table th { color: var(--text-muted); font-weight: 600; font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.05em; }
.admin-table tbody tr:hover { background: var(--bg-hover); }
.admin-error { font-size: 0.8rem; color: #fca5a5; word-break: break-word; max-width: 400px; display: inline-block; }
.badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 999px; font-size: 0.75rem; font-weight: 600; }
.badge-ok { background: rgba(34,197,94,0.15); color: #86efac; }
.badge-err { background: rgba(239,68,68,0.15); color: #fca5a5; }
"#
        }
    };

    Html(crate::templates::layout::base_layout("Admin", content, None).0)
}
