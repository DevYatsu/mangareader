//! API endpoints for sync management, progress tracking, and health checks.
//!
//! Provides:
//! - `GET /api/sync-status` — current sync state (errors, active manga IDs)
//! - `POST /api/sync-manga/{id}` — trigger a background manga sync
//! - `GET /api/progress-series?ids=1,2,3` — series info for continue-reading

use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::AppState;

/// Response for the sync-status endpoint.
#[derive(Serialize)]
pub struct SyncStatusResponse {
    /// Whether any sync is currently in progress.
    pub syncing: bool,
    /// IDs of manga currently being synced.
    pub manga_ids: Vec<i64>,
    /// The most recent sync error (cleared after reading).
    pub last_error: Option<String>,
}

/// Response for the sync-manga trigger endpoint.
#[derive(Serialize)]
pub struct SyncMangaResponse {
    pub ok: bool,
}

/// A series entry for the continue-reading section.
#[derive(Serialize)]
pub struct ProgressSeriesItem {
    pub id: i64,
    pub title: String,
    pub cover_url: Option<String>,
    pub manga_id: i64,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub page: i64,
    pub total_pages: i64,
}

/// Query parameters for the progress-series endpoint.
#[derive(Deserialize)]
pub struct ProgressQuery {
    /// Comma-separated series IDs.
    pub ids: String,
}

/// `GET /api/sync-status` — return current sync state and drain any stored error.
pub async fn sync_status(State(state): State<Arc<AppState>>) -> Json<SyncStatusResponse> {
    let error = state.sync_error.lock().await.take();
    let manga_ids = state.syncing_manga_ids.lock().await.clone();
    Json(SyncStatusResponse {
        syncing: !manga_ids.is_empty(),
        manga_ids,
        last_error: error,
    })
}

/// `POST /api/sync-manga/{id}` — trigger a full manga sync in the background.
///
/// The server starts syncing and returns immediately. Callers can poll
/// `/api/sync-status` or rely on the auto-reload on the series page.
pub async fn sync_manga(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Json<SyncMangaResponse> {
    // Mark as syncing
    state.syncing_manga_ids.lock().await.push(id);

    let state_clone = state.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("bg sync rt");
        rt.block_on(async move {
            tracing::info!("api sync: manga {} starting...", id);

            let engine = crate::scraper_helper::build_engine();

            let driver = match thirtyfour::WebDriver::managed(
                crate::scraper_helper::build_chrome_caps(),
            )
            .await
            {
                Ok(d) => d,
                Err(e) => {
                    *state_clone.sync_error.lock().await =
                        Some(format!("WebDriver error: {e}"));
                    state_clone.syncing_manga_ids.lock().await.retain(|&x| x != id);
                    return;
                }
            };

            let db = match crate::scraper_helper::open_db() {
                Ok(db) => db,
                Err(e) => {
                    *state_clone.sync_error.lock().await =
                        Some(format!("DB error: {e}"));
                    state_clone.syncing_manga_ids.lock().await.retain(|&x| x != id);
                    let _ = driver.quit().await;
                    return;
                }
            };

            if let Err(e) = engine.sync_manga(&driver, &db, id).await {
                *state_clone.sync_error.lock().await =
                    Some(format!("Sync failed for manga {id}: {e}"));
            }

            if let Err(e) = driver.quit().await {
                tracing::warn!("api sync: driver quit error: {:?}", e);
            }

            state_clone.syncing_manga_ids.lock().await.retain(|&x| x != id);
            tracing::info!("api sync: manga {} complete", id);
        });
    });

    Json(SyncMangaResponse { ok: true })
}

/// `GET /api/progress-series?ids=1,2,3` — fetch series info for continue-reading.
///
/// Given a comma-separated list of series IDs, returns the series title,
/// cover image, and first chapter info for each. The frontend uses this
/// to build the "Continue Reading" row on the home page.
pub async fn progress_series(
    State(state): State<Arc<AppState>>,
    query: Query<ProgressQuery>,
) -> Json<Vec<ProgressSeriesItem>> {
    let ids: Vec<i64> = query
        .ids
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    if ids.is_empty() {
        return Json(vec![]);
    }

    let db = state.db.lock().await;
    let mut results = Vec::new();

    for series_id in ids {
        let Ok(series) = mangaread_core::db::get_series_by_id(&db, series_id) else {
            continue;
        };
        let manga_entries =
            mangaread_core::db::list_manga_by_series(&db, series_id).unwrap_or_default();
        let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
            .or_else(|| manga_entries.first());

        let Some(m) = primary else {
            continue;
        };

        let chapters =
            mangaread_core::db::list_chapters_by_manga(&db, m.id).unwrap_or_default();
        // Return the first chapter as the resume point
        if let Some(ch) = chapters.first() {
            let pages =
                mangaread_core::db::list_pages_by_chapter(&db, ch.id).unwrap_or_default();
            results.push(ProgressSeriesItem {
                id: series_id,
                title: series.title,
                cover_url: m.cover_url.clone(),
                manga_id: m.id,
                chapter_id: ch.id,
                chapter_number: ch.number,
                page: 1,
                total_pages: pages.len() as i64,
            });
        }
    }

    Json(results)
}
