//! Health-check endpoint for Docker healthchecks and monitoring.
//!
//! Provides a simple `GET /health` that returns `{"status":"ok"}` when the
//! server is running and accepting requests.

use axum::response::Json;
use serde::Serialize;

/// Response payload for the health endpoint.
#[derive(Serialize)]
pub struct HealthResponse {
    /// Always `"ok"` when the server is live.
    pub status: String,
}

/// `GET /health` — lightweight liveness probe.
///
/// Returns a 200 JSON response immediately. No database or external checks
/// are performed — this endpoint only confirms the HTTP server is running.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}
