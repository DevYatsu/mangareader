use axum::extract::{Path, State};
use axum::response::Redirect;
use std::sync::Arc;

use crate::AppState;

/// Redirect /manga/{id} to its series page if linked, or show a simple page.
pub async fn manga_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Result<Redirect, axum::response::Html<String>> {
    let db = state.db.lock().await;
    match mangaread_core::db::get_manga_by_id(&db, id) {
        Ok(m) => {
            if let Some(sid) = m.series_id {
                Ok(Redirect::to(&format!("/series/{}", sid)))
            } else {
                Ok(Redirect::to("/series"))
            }
        }
        Err(_) => {
            let cfg = crate::config::site_config(&db);
            Err(crate::templates::layout::page(
                &cfg,
                "Not Found",
                crate::templates::layout::Nav::Series,
                None,
                crate::templates::layout::error_content("Not Found", "Manga not found."),
            ))
        }
    }
}
