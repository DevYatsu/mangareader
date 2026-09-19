//! HTTP server and HTML templates for the MangaRead manga library application.
//!
//! This crate provides the web frontend using Axum and Maud templates.

mod auth;
mod config;
mod routes;
mod scraper_helper;
mod templates;

use axum::{
    routing::{get, post},
    Router,
};
use crate::config::SiteConfig;
use mangaread_core::db;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing_subscriber::EnvFilter;

/// Progress for the automatic sequential sync loop.
pub struct SyncProgress {
    /// Whether the loop is currently running.
    pub running: bool,
    /// How many manga we need to sync this pass.
    pub total: i64,
    /// How many we have synced so far.
    pub current: i64,
}

/// Current health status of a single source adapter.
#[derive(Clone)]
pub struct AdapterHealth {
    /// Adapter / source name (e.g. "demonicscans").
    pub source: String,
    /// Whether the adapter is currently considered healthy.
    pub healthy: bool,
    /// Number of consecutive failures since last success.
    pub consecutive_failures: u32,
    /// Timestamp of the last sync attempt.
    pub last_attempt: String,
    /// Error message from the most recent failure, if any.
    pub last_error: Option<String>,
}

/// Shared application state.
///
/// Wrapped in `Arc` and accessed via Axum's `State` extractor.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    /// Last sync error message (cleared on read).
    pub sync_error: Mutex<Option<String>>,
    /// IDs of manga currently being synced in the background.
    pub syncing_manga_ids: Mutex<Vec<i64>>,
    /// Progress of the automatic sequential sync loop.
    pub auto_sync: Mutex<SyncProgress>,
    /// Per-source adapter health (keyed by source name).
    pub adapter_health: Mutex<HashMap<String, AdapterHealth>>,
    /// Site branding configuration (env-driven).
    pub config: SiteConfig,
}



/// Application entrypoint.
///
/// Initializes tracing, opens the SQLite database (with WAL mode and a busy
/// timeout so background scrapers do not block reads), seeds sample data,
/// builds the Axum router with all routes and static file serving, then
/// binds to `0.0.0.0:3000`.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let conn = db::open_db()?;
    db::init_db(&conn)?;

    let site_config = SiteConfig::from_env();

    let state = Arc::new(AppState {
        db: Mutex::new(conn),
        sync_error: Mutex::new(None),
        syncing_manga_ids: Mutex::new(Vec::new()),
        auto_sync: Mutex::new(SyncProgress { running: false, total: 0, current: 0 }),
        config: site_config,
        adapter_health: Mutex::new(HashMap::from([
            ("demonicscans".into(), AdapterHealth {
                source: "demonicscans".into(),
                healthy: true,
                consecutive_failures: 0,
                last_attempt: "never".into(),
                last_error: None,
            }),
            ("manhuaplus".into(), AdapterHealth {
                source: "manhuaplus".into(),
                healthy: true,
                consecutive_failures: 0,
                last_attempt: "never".into(),
                last_error: None,
            }),
        ])),
    });

    // Start the background sync loop (scan → sequential sync → sleep → repeat).
    tracing::info!("starting background sync loop...");
    crate::scraper_helper::start_sync_loop(state.clone());

    let app = Router::new()
        .route("/", get(crate::routes::home::home))
        .route("/series", get(crate::routes::series::series_list))
        .route("/series/{id}", get(crate::routes::series::series_detail))
        .route("/bookmarks", get(crate::routes::bookmarks::bookmarks_page))
        .route("/source/{source}", get(crate::routes::source::source_list))
        .route("/manga/{id}", get(crate::routes::manga::manga_detail))
        .route(
            "/read/{manga_id}/{chapter_id}",
            get(crate::routes::reader::chapter_reader),
        )
        .route("/api/check-manga/{id}", get(crate::routes::check::check_manga))
        .route("/api/check-chapter/{id}", get(crate::routes::check::check_chapter))
        .route("/api/sync-status", get(crate::routes::api::sync_status))
        .route("/api/sync-manga/{id}", post(crate::routes::api::sync_manga))
        .route("/api/progress-series", get(crate::routes::api::progress_series))
        .route("/health", get(crate::routes::health::health))
        .route("/favicon.ico", get(|| async { axum::response::Redirect::permanent("/static/favicon.svg") }))
        .nest_service(
            "/static",
            ServeDir::new(
                std::env::var("MANGAREAD_STATIC_DIR")
                    .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string()),
            ),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
