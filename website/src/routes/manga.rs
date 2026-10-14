use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use std::sync::Arc;

use crate::AppState;

/// Redirect /manga/{id} to its series page if linked, or show a simple page.
pub async fn manga_detail(State(state): State<Arc<AppState>>, Path(id): Path<i64>) -> Response {
    let db = state.db.lock().await;
    match mangaread_core::db::get_manga_by_id(&db, id) {
        Ok(m) => {
            if let Some(sid) = m.series_id {
                Redirect::to(&format!("/series/{}", sid)).into_response()
            } else {
                Redirect::to("/series").into_response()
            }
        }
        Err(_) => {
            let cfg = crate::config::site_config(&db);
            (
                StatusCode::NOT_FOUND,
                crate::templates::layout::page(
                    &cfg,
                    "Not Found",
                    crate::templates::layout::Nav::Series,
                    None,
                    crate::templates::layout::error_content("Not Found", "Manga not found."),
                ),
            )
                .into_response()
        }
    }
}
