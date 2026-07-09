use axum::extract::{Path, State};
use axum::response::Json;
use std::sync::Arc;
use serde::Serialize;

use crate::AppState;

#[derive(Serialize)]
pub struct MangaCheck {
    pub chapters: usize,
}

#[derive(Serialize)]
pub struct ChapterCheck {
    pub pages: usize,
}

/// Lightweight endpoint: does this manga have chapters yet?
pub async fn check_manga(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Json<MangaCheck> {
    let db = state.db.lock().await;
    let chapters = mangaread_core::db::list_chapters_by_manga(&db, id)
        .unwrap_or_default()
        .len();
    Json(MangaCheck { chapters })
}

/// Lightweight endpoint: does this chapter have pages yet?
pub async fn check_chapter(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Json<ChapterCheck> {
    let db = state.db.lock().await;
    let pages = mangaread_core::db::list_pages_by_chapter(&db, id)
        .unwrap_or_default()
        .len();
    Json(ChapterCheck { pages })
}
