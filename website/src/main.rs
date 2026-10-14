//! HTTP server and HTML templates for the MangaRead manga library application.
//!
//! This crate provides the web frontend using Axum and Maud templates.

mod auth;
mod config;
mod email;
mod oauth;
mod rate_limit;
mod routes;
mod scraper_helper;
mod templates;
#[cfg(test)]
mod test_support;

use axum::{
    Router,
    routing::{get, post},
};
use mangaread_core::db;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::compression::CompressionLayer;
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
    /// When the last full pass finished (UTC, SQLite format).
    pub last_finished: Option<String>,
}

/// Shared application state.
///
/// Wrapped in `Arc` and accessed via Axum's `State` extractor.
///
/// Site branding is resolved per request from the DB-backed settings table
/// (see [`crate::config::site_config`]); no config is cached here so admin
/// edits take effect immediately.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
    /// Last sync error message (cleared on read).
    pub sync_error: Mutex<Option<String>>,
    /// IDs of manga currently being synced in the background.
    pub syncing_manga_ids: Mutex<Vec<i64>>,
    /// Progress of the automatic sequential sync loop.
    pub auto_sync: Mutex<SyncProgress>,
    /// Wakes the sync loop early ("Run full sync now").
    pub sync_wake: tokio::sync::Notify,
    /// Starts a background scrape job; returns whether it started. Routes go
    /// through this instead of calling the scraper directly, so tests can
    /// record jobs instead of launching Chromium.
    pub spawn_job: Arc<dyn Fn(crate::scraper_helper::Job) -> bool + Send + Sync>,
    /// One-time token required by `/setup` while the instance has no users.
    /// `None` once setup is complete (or was never needed).
    pub setup_token: Mutex<Option<String>>,
    /// Outbound mail (SMTP in production, in-memory in tests).
    pub mailer: Arc<dyn crate::email::Mailer>,
    /// Sliding-window limits for mail-sending endpoints.
    pub rate_limiter: crate::rate_limit::RateLimiter,
}

impl AppState {
    /// Fresh state over an initialized connection, with empty sync bookkeeping.
    pub fn new(conn: rusqlite::Connection) -> Self {
        AppState {
            db: Mutex::new(conn),
            sync_error: Mutex::new(None),
            syncing_manga_ids: Mutex::new(Vec::new()),
            auto_sync: Mutex::new(SyncProgress {
                running: false,
                total: 0,
                current: 0,
                last_finished: None,
            }),
            sync_wake: tokio::sync::Notify::new(),
            spawn_job: Arc::new(crate::scraper_helper::spawn_job),
            setup_token: Mutex::new(None),
            mailer: Arc::new(crate::email::SmtpMailer),
            rate_limiter: crate::rate_limit::RateLimiter::default(),
        }
    }
}

/// Build the full router. Kept separate from `main` so tests can drive every
/// route in-process without binding a port or starting the sync loop.
pub fn app(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(crate::routes::home::home))
        .route("/series", get(crate::routes::series::series_list))
        .route("/series/{id}", get(crate::routes::series::series_detail))
        .route("/bookmarks", get(crate::routes::bookmarks::bookmarks_page))
        .route(
            "/login",
            get(crate::routes::auth_pages::login_page)
                .post(crate::routes::auth_pages::login_submit),
        )
        .route(
            "/register",
            get(crate::routes::signup::register_page).post(crate::routes::signup::register_submit),
        )
        .route(
            "/register/{token}",
            get(crate::routes::signup::complete_page).post(crate::routes::signup::complete_submit),
        )
        .route("/logout", post(crate::routes::auth_pages::logout))
        .route(
            "/forgot",
            get(crate::routes::password_reset::forgot_page)
                .post(crate::routes::password_reset::forgot_submit),
        )
        .route(
            "/reset/{token}",
            get(crate::routes::password_reset::reset_page)
                .post(crate::routes::password_reset::reset_submit),
        )
        .route("/auth/{provider}", get(crate::routes::oauth::oauth_start))
        .route(
            "/auth/{provider}/callback",
            get(crate::routes::oauth::oauth_callback),
        )
        .route(
            "/setup",
            get(crate::routes::setup::setup_page).post(crate::routes::setup::setup_submit),
        )
        .route("/settings", get(crate::routes::settings::settings_page))
        .route(
            "/settings/profile",
            post(crate::routes::settings::profile_submit),
        )
        .route(
            "/settings/password",
            post(crate::routes::settings::password_submit),
        )
        .route("/admin", get(crate::routes::admin::dashboard))
        .route(
            "/admin/branding",
            get(crate::routes::admin::branding_page).post(crate::routes::admin::branding_submit),
        )
        .route(
            "/admin/email",
            get(crate::routes::admin_email::email_page)
                .post(crate::routes::admin_email::email_submit),
        )
        .route(
            "/admin/email/test",
            post(crate::routes::admin_email::email_test),
        )
        .route(
            "/admin/series",
            get(crate::routes::admin_series::series_list),
        )
        .route(
            "/admin/series/{id}",
            get(crate::routes::admin_series::series_edit),
        )
        .route(
            "/admin/series/{id}/rename",
            post(crate::routes::admin_series::series_rename),
        )
        .route(
            "/admin/series/{id}/cover",
            post(crate::routes::admin_series::series_cover),
        )
        .route(
            "/admin/series/{id}/hide",
            post(crate::routes::admin_series::series_hide),
        )
        .route(
            "/admin/series/{id}/merge",
            post(crate::routes::admin_series::series_merge),
        )
        .route(
            "/admin/manga/{id}/split",
            post(crate::routes::admin_series::manga_split),
        )
        .route(
            "/admin/sources",
            get(crate::routes::admin_sync::sources_page),
        )
        .route(
            "/admin/sources/{name}/toggle",
            post(crate::routes::admin_sync::source_toggle),
        )
        .route(
            "/admin/sources/{name}/scan",
            post(crate::routes::admin_sync::source_scan),
        )
        .route("/admin/sync", get(crate::routes::admin_sync::sync_page))
        .route(
            "/admin/sync/run",
            post(crate::routes::admin_sync::sync_run_now),
        )
        .route(
            "/admin/sync/retry/{id}",
            post(crate::routes::admin_sync::sync_retry),
        )
        .route("/admin/users", get(crate::routes::admin::users_page))
        .route(
            "/admin/users/{id}/role",
            post(crate::routes::admin::user_set_role),
        )
        .route(
            "/admin/users/{id}/ban",
            post(crate::routes::admin::user_set_banned),
        )
        .route(
            "/admin/users/{id}/reset-link",
            post(crate::routes::admin::user_reset_link),
        )
        .route("/terms", get(crate::routes::legal::terms))
        .route("/privacy", get(crate::routes::legal::privacy))
        .route("/source/{source}", get(crate::routes::source::source_list))
        .route("/manga/{id}", get(crate::routes::manga::manga_detail))
        .route(
            "/read/{manga_id}/{chapter_id}",
            get(crate::routes::reader::chapter_reader),
        )
        .route(
            "/api/check-manga/{id}",
            get(crate::routes::check::check_manga),
        )
        .route(
            "/api/check-chapter/{id}",
            get(crate::routes::check::check_chapter),
        )
        .route("/api/sync-status", get(crate::routes::api::sync_status))
        .route("/api/sync-manga/{id}", post(crate::routes::api::sync_manga))
        .route(
            "/api/progress-series",
            get(crate::routes::api::progress_series),
        )
        .route(
            "/api/bookmarks",
            get(crate::routes::sync::get_bookmarks).post(crate::routes::sync::set_bookmark),
        )
        .route(
            "/api/progress",
            get(crate::routes::sync::get_progress).post(crate::routes::sync::post_progress),
        )
        .route("/api/history", post(crate::routes::sync::post_history))
        .route("/api/migrate", post(crate::routes::sync::post_migrate))
        .route(
            "/api/reader-prefs",
            get(crate::routes::reader_prefs::get_prefs)
                .post(crate::routes::reader_prefs::post_prefs),
        )
        .route(
            "/api/notes",
            get(crate::routes::notes::list_notes).post(crate::routes::notes::create_note),
        )
        .route("/health", get(crate::routes::health::health))
        .route(
            "/favicon.ico",
            get(|| async { axum::response::Redirect::permanent("/static/favicon.svg") }),
        )
        .nest_service(
            "/static",
            ServeDir::new(
                std::env::var("MANGAREAD_STATIC_DIR")
                    .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string()),
            ),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::routes::setup::setup_gate,
        ))
        .layer(CompressionLayer::new())
        .with_state(state)
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
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let conn = db::open_db()?;
    db::init_db(&conn)?;

    let boot = crate::config::SiteConfig::from_env();
    tracing::info!(site_title = %boot.site_title, accent = %boot.accent, "boot defaults loaded");

    let state = Arc::new(AppState::new(conn));
    crate::routes::setup::arm_if_needed(&state).await;

    // Start the background sync loop (scan → sequential sync → sleep → repeat).
    tracing::info!("starting background sync loop...");
    crate::scraper_helper::start_sync_loop(state.clone());

    let app = app(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://0.0.0.0:3000");
    // Connect info feeds `ClientIp`, which keys the sign-up rate limits.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await?;

    Ok(())
}
