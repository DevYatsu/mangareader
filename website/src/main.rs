//! HTTP server and HTML templates for the MangaRead manga library application.
//!
//! This crate provides the web frontend using Axum and Maud templates.

mod routes;
mod scraper_helper;
mod templates;

use axum::{
    routing::{get},
    Router,
};
use mangaread_core::db;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing_subscriber;

/// Shared application state holding a SQLite database connection.
///
/// Wrapped in `Arc` and accessed via Axum's `State` extractor.
pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
}

/// Seed the database with sample manga, chapters, and pages if the DB is empty.
///
/// Inserts One Piece as a demo entry with two chapters and eight total pages.
/// Skips seeding if the `manga` table already contains rows.
fn seed_data(conn: &rusqlite::Connection) {
    // Only seed if DB is empty
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
        .unwrap_or(0);
    if count > 0 {
        return;
    }

    tracing::info!("seeding sample data...");

    let manga_id = mangaread_core::db::insert_manga(
        conn,
        &mangaread_core::NewManga {
            source: "demonicscans".into(),
            source_id: "one-piece".into(),
            title: "One Piece".into(),
            author: Some("Eiichiro Oda".into()),
            cover_url: Some("https://placehold.co/300x400/1a1a1a/f59e0b?text=One+Piece".into()),
            description: Some("A pirate adventure manga following Monkey D. Luffy and his crew as they search for the legendary One Piece treasure.".into()),
            detail_url: "https://demonicscans.org/manga/one-piece".into(),
            series_id: None,
        },
    )
    .expect("insert manga");

    let ch1_id = mangaread_core::db::insert_chapter(
        conn,
        &mangaread_core::NewChapter {
            manga_id,
            source_id: "ch-1".into(),
            number: 1.0,
            title: Some("Romance Dawn".into()),
            url: "https://demonicscans.org/manga/one-piece/ch-1".into(),
        },
    )
    .expect("insert chapter");

    for i in 1..=5 {
        mangaread_core::db::insert_page(
            conn,
            &mangaread_core::NewPage {
                chapter_id: ch1_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/333/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    let ch2_id = mangaread_core::db::insert_chapter(
        conn,
        &mangaread_core::NewChapter {
            manga_id,
            source_id: "ch-2".into(),
            number: 2.0,
            title: Some("That Guy".into()),
            url: "https://demonicscans.org/manga/one-piece/ch-2".into(),
        },
    )
    .expect("insert chapter");

    for i in 1..=3 {
        mangaread_core::db::insert_page(
            conn,
            &mangaread_core::NewPage {
                chapter_id: ch2_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/444/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    tracing::info!("seed data inserted: 1 manga, 2 chapters, 8 pages");
}

/// Application entrypoint.
///
/// Initializes tracing, opens the SQLite database (with WAL mode and a busy
/// timeout so background scrapers do not block reads), seeds sample data,
/// builds the Axum router with all routes and static file serving, then
/// binds to `0.0.0.0:3000`.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let conn = rusqlite::Connection::open("mangaread.db")?;
    db::init_db(&conn)?;
    seed_data(&conn);

    // Enable WAL + busy timeout so the background scraper won't block reads
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;

    let state = Arc::new(AppState {
        db: Mutex::new(conn),
    });

    let app = Router::new()
        .route("/", get(crate::routes::home::home))
        .route("/series", get(crate::routes::series::series_list))
        .route("/series/{id}", get(crate::routes::series::series_detail))
        .route("/source/{source}", get(crate::routes::source::source_list))
        .route("/manga/{id}", get(crate::routes::manga::manga_detail))
        .route(
            "/read/{manga_id}/{chapter_id}",
            get(crate::routes::reader::chapter_reader),
        )
        .route("/api/check-manga/{id}", get(crate::routes::check::check_manga))
        .route("/api/check-chapter/{id}", get(crate::routes::check::check_chapter))
        .nest_service("/static", ServeDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/static")))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
