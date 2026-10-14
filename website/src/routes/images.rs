//! `GET /img/{page_id}`: reader page images served through the image proxy
//! (spec Slice R), so pages load even when the source blocks hotlinking.

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use mangaread_core::domain::Role;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, image_proxy};

/// Browsers may keep a page image forever: its URL carries a version of the
/// origin URL, so a changed image gets a new URL.
const CACHE_CONTROL: &str = "public, max-age=31536000, immutable";

pub async fn page_image(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(page_id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let (origin, cap_mb) = {
        let db = state.db.lock().await;
        let origin = mangaread_core::db::get_page_origin(&db, page_id)
            .ok()
            .flatten();
        (origin, cache_cap_mb(&db))
    };
    let is_admin = user.as_ref().is_some_and(|u| u.role == Role::Admin);
    let Some(origin) = origin.filter(|o| !o.hidden || is_admin) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // The ETag is derived from the origin URL, so a revalidation needs
    // neither the disk nor the network.
    let etag = format!("\"{}\"", image_proxy::url_key(&origin.image_url));
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.split(',').any(|t| t.trim() == etag))
    {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }

    match state
        .images
        .get(&origin.image_url, &origin.source, cap_mb * 1024 * 1024)
        .await
    {
        Ok(img) => (
            [
                (header::CONTENT_TYPE, img.content_type.to_string()),
                (header::ETAG, img.etag),
                (header::CACHE_CONTROL, CACHE_CONTROL.to_string()),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            ],
            img.bytes,
        )
            .into_response(),
        Err(e) => {
            tracing::warn!(page_id, source = %origin.source, error = %e, "image proxy failed");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Admin-set cache cap in MiB, falling back to the default.
pub fn cache_cap_mb(db: &rusqlite::Connection) -> u64 {
    mangaread_core::db::get_setting(db, image_proxy::CACHE_MAX_MB_KEY)
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(image_proxy::DEFAULT_CACHE_MAX_MB)
}

#[cfg(test)]
mod tests {
    use crate::test_support::{TestApp, app_with_admin};
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use tower::ServiceExt;

    const ORIGIN: &str = "https://cdn.source.example/ch1/001.jpg";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];

    /// One series → manga → chapter → page, with the page already cached so
    /// no test touches the network. Returns `(series_id, manga_id, chapter_id, page_id)`.
    async fn seed(app: &TestApp) -> (i64, i64, i64, i64) {
        let ids = {
            let db = app.state.db.lock().await;
            let sid = mangaread_core::db::insert_series(&db, "Alpha").unwrap();
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id)
                 VALUES ('src', 'alpha', 'Alpha', 'https://source.example/alpha', ?1)",
                [sid],
            )
            .unwrap();
            let mid = db.last_insert_rowid();
            db.execute(
                "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (?1, '1', 1, 'u')",
                [mid],
            )
            .unwrap();
            let cid = db.last_insert_rowid();
            let pid = mangaread_core::db::insert_page(
                &db,
                &mangaread_core::NewPage {
                    chapter_id: cid,
                    index: 0,
                    image_url: ORIGIN.into(),
                },
            )
            .unwrap();
            (sid, mid, cid, pid)
        };
        let path = app.state.images.cached_path(ORIGIN);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, JPEG).unwrap();
        ids
    }

    async fn raw(
        app: &TestApp,
        path: &str,
        if_none_match: Option<&str>,
    ) -> axum::response::Response {
        let mut req = Request::get(path);
        if let Some(t) = if_none_match {
            req = req.header(header::IF_NONE_MATCH, t);
        }
        app.router
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn reader_links_the_proxy_never_the_origin() {
        let (app, _) = app_with_admin().await;
        let (_, mid, cid, pid) = seed(&app).await;
        let html = app.get(&format!("/read/{mid}/{cid}"), None).await.body;
        assert!(html.contains(&format!("src=\"/img/{pid}?v=")), "{html}");
        assert!(!html.contains("cdn.source.example"));
    }

    #[tokio::test]
    async fn serves_cached_image_with_immutable_validators() {
        let (app, _) = app_with_admin().await;
        let (_, _, _, pid) = seed(&app).await;
        let res = raw(&app, &format!("/img/{pid}?v=x"), None).await;
        assert_eq!(res.status(), StatusCode::OK);
        let h = res.headers();
        assert_eq!(h[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(h[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        assert!(
            h[header::CACHE_CONTROL]
                .to_str()
                .unwrap()
                .contains("immutable")
        );
        let etag = h[header::ETAG].to_str().unwrap().to_string();

        let again = raw(&app, &format!("/img/{pid}"), Some(&etag)).await;
        assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn missing_and_hidden_pages_are_not_found() {
        let (app, admin) = app_with_admin().await;
        let (sid, _, _, pid) = seed(&app).await;
        assert_eq!(
            raw(&app, "/img/9999", None).await.status(),
            StatusCode::NOT_FOUND
        );
        {
            let db = app.state.db.lock().await;
            mangaread_core::db::set_series_hidden(&db, sid, Some("2026-10-15 00:00:00")).unwrap();
        }
        assert_eq!(
            raw(&app, &format!("/img/{pid}"), None).await.status(),
            StatusCode::NOT_FOUND
        );
        // Admins still see hidden series.
        assert_eq!(
            app.get(&format!("/img/{pid}"), Some(&admin)).await.status,
            StatusCode::OK
        );
    }
}
