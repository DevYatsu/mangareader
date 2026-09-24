//! Private per-chapter reader notes.
//!
//! A note is anchored to a 1-indexed page, matching `progress.page` and
//! `data-page-idx`. There is no anonymous write path: `notes.user_id` is NOT
//! NULL and the design contract forbids inventing a shadow table.

use axum::extract::{Json, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::AppState;
use crate::auth::AuthUser;

/// Max note length, mirrored in the client's `maxlength` and the design contract.
const MAX_NOTE_LEN: usize = 2000;

#[derive(Deserialize)]
pub struct ListQuery {
    pub chapter_id: i64,
}

#[derive(Deserialize)]
pub struct NewNote {
    pub chapter_id: i64,
    pub page: i64,
    pub body: String,
}

/// Note bodies are trimmed; empty and over-long bodies are rejected.
pub fn validate_body(body: &str) -> Result<String, ()> {
    let t = body.trim();
    if t.is_empty() || t.chars().count() > MAX_NOTE_LEN {
        return Err(());
    }
    Ok(t.to_string())
}

/// Pages are 1-indexed.
pub fn validate_page(page: i64) -> Result<i64, ()> {
    if page < 1 { Err(()) } else { Ok(page) }
}

fn bad(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"ok": false, "error": msg})),
    )
        .into_response()
}

fn note_json(note: &mangaread_core::domain::Note) -> serde_json::Value {
    json!({
        "id": note.id,
        "page": note.page,
        "body": note.body,
        "created_at": note.created_at,
    })
}

/// GET `/api/notes?chapter_id=N` — the caller's notes on a chapter.
pub async fn list_notes(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> Response {
    let notes = {
        let db = state.db.lock().await;
        match mangaread_core::db::list_notes(&db, user.id, q.chapter_id) {
            Ok(n) => n,
            Err(_) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"ok": false})),
                )
                    .into_response();
            }
        }
    };
    let items: Vec<_> = notes.iter().map(note_json).collect();
    Json(json!({ "notes": items })).into_response()
}

/// POST `/api/notes` — create a note on a page.
pub async fn create_note(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<NewNote>,
) -> Response {
    let text = match validate_body(&body.body) {
        Ok(t) => t,
        Err(_) => return bad("Note must be 1-2000 characters."),
    };
    let page = match validate_page(body.page) {
        Ok(p) => p,
        Err(_) => return bad("Invalid page."),
    };
    let created = {
        let db = state.db.lock().await;
        // The chapter FK fails if the chapter does not exist.
        let id = match mangaread_core::db::insert_note(&db, user.id, body.chapter_id, page, &text) {
            Ok(id) => id,
            Err(_) => return bad("Could not save note."),
        };
        // Return the persisted timestamp immediately instead of making the
        // client issue a second request merely to render a just-created note.
        match mangaread_core::db::list_notes(&db, user.id, body.chapter_id)
            .ok()
            .and_then(|notes| notes.into_iter().find(|note| note.id == id))
        {
            Some(note) => note,
            None => return bad("Could not save note."),
        }
    };
    (
        StatusCode::CREATED,
        Json(json!({"ok": true, "note": note_json(&created)})),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_validation() {
        assert!(validate_body("").is_err());
        assert!(validate_body(&"x".repeat(2000)).is_ok());
        assert!(validate_body(&"x".repeat(2001)).is_err());
        assert!(validate_body("  trimmed  ").is_ok());
    }

    #[test]
    fn page_validation() {
        assert!(validate_page(0).is_err());
        assert!(validate_page(1).is_ok());
        assert!(validate_page(9999).is_ok());
    }

    #[test]
    fn note_json_includes_creation_timestamp() {
        let note = mangaread_core::domain::Note {
            id: 7,
            user_id: 2,
            chapter_id: 3,
            page: 1,
            body: "remember this".into(),
            created_at: "2026-09-25 12:34:56".into(),
        };
        let value = note_json(&note);
        assert_eq!(value["id"], 7);
        assert_eq!(value["page"], 1);
        assert_eq!(value["body"], "remember this");
        assert_eq!(value["created_at"], "2026-09-25 12:34:56");
    }
}
