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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let conn = rusqlite::Connection::open("mangaread.db")?;
    mangaread_core::db::init_db(&conn)?;

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
