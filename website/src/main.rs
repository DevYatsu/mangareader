mod routes;
mod templates;

use axum::{
    routing::{get},
    Router,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing_subscriber;

pub struct AppState {
    pub db: Mutex<rusqlite::Connection>,
}

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
        &mangaread_core::models::NewManga {
            source: "demonicscans".into(),
            source_id: "one-piece".into(),
            title: "One Piece".into(),
            author: Some("Eiichiro Oda".into()),
            cover_url: Some("https://placehold.co/300x400/1a1a1a/f59e0b?text=One+Piece".into()),
            description: Some("A pirate adventure manga following Monkey D. Luffy and his crew as they search for the legendary One Piece treasure.".into()),
            detail_url: "https://demonicscans.org/manga/one-piece".into(),
        },
    )
    .expect("insert manga");

    let ch1_id = mangaread_core::db::insert_chapter(
        conn,
        &mangaread_core::models::NewChapter {
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
            &mangaread_core::models::NewPage {
                chapter_id: ch1_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/333/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    let ch2_id = mangaread_core::db::insert_chapter(
        conn,
        &mangaread_core::models::NewChapter {
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
            &mangaread_core::models::NewPage {
                chapter_id: ch2_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/444/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    tracing::info!("seed data inserted: 1 manga, 2 chapters, 8 pages");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let conn = rusqlite::Connection::open("mangaread.db")?;
    mangaread_core::db::init_db(&conn)?;
    seed_data(&conn);

    let state = Arc::new(AppState { db: Mutex::new(conn) });

    let app = Router::new()
        .route("/", get(crate::routes::home::home))
        .route("/source/:source", get(crate::routes::source::source_list))
        .route("/manga/:id", get(crate::routes::manga::manga_detail))
        .route(
            "/read/:manga_id/:chapter_id",
            get(crate::routes::reader::chapter_reader),
        )
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
