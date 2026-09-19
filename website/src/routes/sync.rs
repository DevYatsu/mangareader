//! Per-user server sync APIs: bookmarks, progress, history, migrate.
//!
//! Every endpoint requires `AuthUser` (missing auth → `401` JSON via the
//! extractor). `updated_at` timestamps use `%Y-%m-%d %H:%M:%S` UTC (same
//! format as the session helpers in `crate::auth`).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use mangaread_core::{db, domain::Progress};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

/// Last-write-wins guard: true = incoming row wins (string `>=` on ISO8601
/// `updated_at`, which orders chronologically in this fixed-width format).
pub fn merge_progress(existing: Option<&str>, incoming: &str) -> bool {
    match existing {
        None => true,
        Some(current) => incoming >= current,
    }
}

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
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"ok": false, "error": e.to_string()})),
    )
        .into_response()
}

/// GET /api/bookmarks → `{"series":[ids]}`.
pub async fn get_bookmarks(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
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
pub async fn post_migrate(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<MigrateRequest>,
) -> Response {
    let n_bookmarks = body.bookmarks.len();
    let n_progress = body.progress.len();
    for series_id in &body.bookmarks {
        let res = {
            let conn = state.db.lock().await;
            db::set_bookmark(&conn, user.id, *series_id, true)
        };
        if let Err(e) = res {
            return db_error(e);
        }
    }
    for p in &body.progress {
        let res = {
            let conn = state.db.lock().await;
            db::upsert_progress(
                &conn,
                &Progress {
                    user_id: user.id,
                    chapter_id: p.chapter_id,
                    page: p.page,
                    status: p.status.clone(),
                    updated_at: p.updated_at.clone(),
                },
            )
        };
        if let Err(e) = res {
            return db_error(e);
        }
    }
    Json(serde_json::json!({"ok": true, "bookmarks": n_bookmarks, "progress": n_progress}))
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::merge_progress;

    #[test]
    fn newer_incoming_wins() {
        assert!(merge_progress(
            Some("2026-01-01 00:00:00"),
            "2026-09-19 00:00:00"
        ));
        assert!(!merge_progress(
            Some("2026-09-19 00:00:00"),
            "2026-01-01 00:00:00"
        ));
        assert!(merge_progress(None, "2026-01-01 00:00:00"));
    }
}
