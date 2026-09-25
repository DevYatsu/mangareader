//! Per-user server sync APIs: bookmarks, progress, history, migrate.
//!
//! Every endpoint requires `AuthUser` (missing auth → `401` JSON via the
//! extractor). `updated_at` timestamps use `%Y-%m-%d %H:%M:%S` UTC (same
//! format as the session helpers in `crate::auth`).

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mangaread_core::{db, domain::Progress};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser};

/// Current UTC time formatted `%Y-%m-%d %H:%M:%S` (matches SQLite datetime).
fn now_utc_string() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

fn ok() -> Response {
    Json(serde_json::json!({"ok": true})).into_response()
}

fn db_error(e: impl std::fmt::Display) -> Response {
    tracing::warn!("sync api db error: {e}");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"ok": false, "error": "database error"})),
    )
        .into_response()
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({"ok": false, "error": msg})),
    )
        .into_response()
}

/// Statuses the JS client actually sends (`website/static/app.js`
/// `setChapterStatus`, `auth.js` `pushProgress` default).
fn valid_status(s: &str) -> bool {
    matches!(s, "unread" | "started" | "finished")
}

fn validate_progress(page: i64, status: &str) -> Option<Response> {
    if page < 1 {
        return Some(bad_request("page must be >= 1"));
    }
    if !valid_status(status) {
        return Some(bad_request("invalid status"));
    }
    None
}

/// GET /api/bookmarks → `{"series":[ids]}`.
pub async fn get_bookmarks(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    let series = {
        let conn = state.db.lock().await;
        db::list_bookmarks(&conn, user.id)
    };
    match series {
        Ok(series) => Json(serde_json::json!({"series": series})).into_response(),
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize)]
pub struct SetBookmark {
    pub series_id: i64,
    pub bookmarked: bool,
}

/// POST /api/bookmarks `{"series_id":1,"bookmarked":true}` → `{"ok":true}`.
pub async fn set_bookmark(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<SetBookmark>,
) -> Response {
    let res = {
        let conn = state.db.lock().await;
        db::set_bookmark(&conn, user.id, body.series_id, body.bookmarked)
    };
    match res {
        Ok(()) => ok(),
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize)]
pub struct ProgressQuery {
    pub series_id: i64,
}

#[derive(Serialize)]
struct ChapterProgress {
    page: i64,
    status: String,
    updated_at: String,
}

/// GET /api/progress?series_id=1 → `{"chapters":{"<id>":{...}}}`.
pub async fn get_progress(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ProgressQuery>,
) -> Response {
    let rows = {
        let conn = state.db.lock().await;
        db::list_progress_by_series(&conn, user.id, q.series_id)
    };
    match rows {
        Ok(rows) => {
            let chapters: HashMap<String, ChapterProgress> = rows
                .into_iter()
                .map(|p| {
                    (
                        p.chapter_id.to_string(),
                        ChapterProgress {
                            page: p.page,
                            status: p.status,
                            updated_at: p.updated_at,
                        },
                    )
                })
                .collect();
            Json(serde_json::json!({"chapters": chapters})).into_response()
        }
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize)]
pub struct PostProgress {
    pub chapter_id: i64,
    pub page: i64,
    pub status: String,
}

/// POST /api/progress → server stamps `updated_at` = now UTC.
pub async fn post_progress(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<PostProgress>,
) -> Response {
    if let Some(resp) = validate_progress(body.page, &body.status) {
        return resp;
    }
    let stamped = now_utc_string();
    let res = {
        let conn = state.db.lock().await;
        db::upsert_progress(
            &conn,
            &Progress {
                user_id: user.id,
                chapter_id: body.chapter_id,
                page: body.page,
                status: body.status,
                updated_at: stamped,
            },
        )
    };
    match res {
        Ok(()) => ok(),
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize)]
pub struct PostHistory {
    pub chapter_id: i64,
}

/// POST /api/history `{"chapter_id":12}` → `{"ok":true}`.
pub async fn post_history(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<PostHistory>,
) -> Response {
    let at = now_utc_string();
    let res = {
        let conn = state.db.lock().await;
        db::add_history(&conn, user.id, body.chapter_id, &at)
    };
    match res {
        Ok(()) => ok(),
        Err(e) => db_error(e),
    }
}

#[derive(Deserialize)]
pub struct MigrateProgress {
    pub chapter_id: i64,
    pub page: i64,
    pub status: String,
    pub updated_at: String,
}

#[derive(Deserialize)]
pub struct MigrateRequest {
    #[serde(default)]
    pub bookmarks: Vec<i64>,
    #[serde(default)]
    pub progress: Vec<MigrateProgress>,
}

/// POST /api/migrate: union bookmarks (insert-or-ignore), upsert progress
/// with the same last-write-wins rule as live writes.
///
/// All-or-nothing: the whole body runs in a single SQLite transaction, so a
/// mid-batch failure (e.g. a bookmark FK on an unknown series) rolls back
/// every row instead of leaving partial state.
pub async fn post_migrate(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<MigrateRequest>,
) -> Response {
    let n_bookmarks = body.bookmarks.len();
    let n_progress = body.progress.len();
    for p in &body.progress {
        if let Some(resp) = validate_progress(p.page, &p.status) {
            return resp;
        }
    }
    // One guard for the whole migrate (transaction requires it). Only fast
    // local SQL runs under the guard — no hashing or slow work.
    let mut conn = state.db.lock().await;
    let tx = match conn.transaction() {
        Ok(tx) => tx,
        Err(e) => return db_error(e),
    };
    let applied: Result<(), mangaread_core::error::Error> = (|| {
        for series_id in &body.bookmarks {
            db::set_bookmark(&tx, user.id, *series_id, true)?;
        }
        for p in &body.progress {
            db::upsert_progress(
                &tx,
                &Progress {
                    user_id: user.id,
                    chapter_id: p.chapter_id,
                    page: p.page,
                    status: p.status.clone(),
                    updated_at: p.updated_at.clone(),
                },
            )?;
        }
        Ok(())
    })();
    // `tx` drops uncommitted on this path, rolling back partial rows.
    if let Err(e) = applied {
        return db_error(e);
    }
    match tx.commit() {
        Ok(()) => {
            Json(serde_json::json!({"ok": true, "bookmarks": n_bookmarks, "progress": n_progress}))
                .into_response()
        }
        Err(e) => db_error(e),
    }
}

#[cfg(test)]
mod tests {
    use super::{valid_status, validate_progress};

    #[test]
    fn progress_validation_rejects_bad_status_and_page() {
        assert!(valid_status("started"));
        assert!(valid_status("finished"));
        assert!(valid_status("unread"));
        assert!(!valid_status("bogus"));
        assert!(validate_progress(1, "started").is_none());
        assert!(validate_progress(0, "started").is_some());
        assert!(validate_progress(-1, "finished").is_some());
        assert!(validate_progress(1, "bogus").is_some());
    }
}
