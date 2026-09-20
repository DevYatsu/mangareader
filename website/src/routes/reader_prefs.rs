//! Reader 2.0 preferences API.
//!
//! One JSON blob per user, validated by [`mangaread_core::ReaderPrefs::normalized`].
//! Anonymous callers get 401: preferences are an identity-level concern and
//! the design contract's server blob is only authoritative when logged in.

use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

use crate::auth::AuthUser;
use crate::AppState;

/// GET `/api/reader-prefs` — the current user's preferences.
pub async fn get_prefs(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    let prefs = {
        let db = state.db.lock().await;
        match mangaread_core::db::get_reader_prefs(&db, user.id) {
            Ok(p) => p,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false}))).into_response(),
        }
    };
    Json(json!({ "prefs": prefs })).into_response()
}

/// POST `/api/reader-prefs` — replace the current user's preferences.
///
/// The body is normalized before storage and the stored value is echoed back,
/// so the client never has to guess what was accepted.
pub async fn post_prefs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<mangaread_core::ReaderPrefs>,
) -> Response {
    let prefs = body.normalized();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::set_reader_prefs(&db, user.id, &prefs).is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false}))).into_response();
        }
    }
    Json(json!({ "ok": true, "prefs": prefs })).into_response()
}

#[cfg(test)]
mod tests {
    use mangaread_core::ReaderPrefs;

    /// The API response shape is `{"prefs": {...}}` and the inner object
    /// round-trips through the same serde contract the DB stores.
    #[test]
    fn prefs_blob_serializes_under_prefs_key() {
        use serde_json::json;
        let p = ReaderPrefs::default();
        let body = json!({ "prefs": p });
        assert_eq!(body["prefs"]["layout"], "strip");
        assert_eq!(body["prefs"]["maxW"], 700); // camelCase, per the design contract
        assert!(body["prefs"]["clickZones"].is_array());
        assert!(body["prefs"]["keybinds"]["page.next"].is_array());
    }

    /// A partial body must deserialize and normalize rather than 400.
    #[test]
    fn partial_body_normalizes() {
        let p: ReaderPrefs = serde_json::from_value(serde_json::json!({ "maxW": 5000 })).unwrap();
        let n = p.normalized();
        assert_eq!(n.max_w, 1400);
        assert_eq!(n.layout, "strip"); // unspecified field -> default
    }
}
