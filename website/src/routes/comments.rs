//! Chapter comments: the JSON API behind the reader's comment section.
//!
//! Anyone can read. Logged-in users post, reply, edit and delete their own
//! comments, and report others'. Moderators and admins hide and restore.
//!
//! CSRF: every write takes a JSON body. Browsers cannot send
//! `Content-Type: application/json` cross-site without a CORS preflight,
//! which this server never grants, and the session cookie is `SameSite=Lax`.

use axum::{
    extract::{Json, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use mangaread_core::domain::{Comment, NewComment, Role};
use mangaread_core::error::Error;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Duration;

use crate::{AppState, auth::AuthUser};

pub const MAX_COMMENT_CHARS: usize = 2000;
const MAX_REASON_CHARS: usize = 200;
const PAGE_SIZE: i64 = 20;
/// One comment per 15 s and 50 per day per user (spec §4 Slice C.8).
const BURST_WINDOW: Duration = Duration::from_secs(15);
const DAY: Duration = Duration::from_secs(24 * 3600);
const PER_DAY: usize = 50;

/// Who is looking, for visibility rules.
pub struct Viewer {
    pub user_id: Option<i64>,
    pub can_moderate: bool,
}

impl Viewer {
    fn from(user: Option<&AuthUser>) -> Self {
        Viewer {
            user_id: user.map(|u| u.id),
            can_moderate: user.is_some_and(|u| u.role.at_least(&Role::Moderator)),
        }
    }
}

/// What `viewer` may see of `c`; `None` drops it from the response.
///
/// - Deleted by the author: nobody sees the body. It stays as a
///   "[deleted]" placeholder only while it still has visible replies.
/// - Hidden by a moderator: moderators see it, tagged. Everyone else gets a
///   "[removed]" placeholder when it has visible replies, else nothing.
pub fn view(c: &Comment, has_visible_replies: bool, viewer: &Viewer) -> Option<Value> {
    let placeholder = |kind: &str| json!({ "id": c.id, "placeholder": kind, "parent_id": c.parent_id, "created_at": c.created_at });
    if c.deleted_at.is_some() {
        return has_visible_replies.then(|| placeholder("deleted"));
    }
    if c.hidden_at.is_some() && !viewer.can_moderate {
        return has_visible_replies.then(|| placeholder("removed"));
    }
    Some(json!({
        "id": c.id,
        "parent_id": c.parent_id,
        "author": c.author,
        "body": c.body,
        "spoiler": c.spoiler,
        "created_at": c.created_at,
        "edited": c.edited_at.is_some(),
        "hidden": c.hidden_at.is_some(),
        "mine": viewer.user_id == Some(c.user_id),
    }))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(json!({ "ok": false, "error": code, "message": message })),
    )
        .into_response()
}

fn db_error() -> Response {
    err(
        StatusCode::INTERNAL_SERVER_ERROR,
        "server",
        "Something went wrong. Please try again.",
    )
}

/// Trimmed body, or the reason it is unacceptable.
fn clean_body(body: &str) -> Result<String, Response> {
    let t = body.trim();
    if t.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "empty",
            "Write something first.",
        ));
    }
    if t.chars().count() > MAX_COMMENT_CHARS {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "too_long",
            "Comments are limited to 2000 characters.",
        ));
    }
    Ok(t.to_string())
}

/// The series must exist and be visible to this viewer.
fn series_visible(conn: &rusqlite::Connection, series_id: i64, viewer: &Viewer) -> bool {
    mangaread_core::db::get_series_by_id(conn, series_id)
        .is_ok_and(|s| !s.hidden || viewer.can_moderate)
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub series: i64,
    pub chapter: f64,
    pub before: Option<i64>,
}

/// GET `/api/comments?series=&chapter=&before=` — one page of a discussion.
pub async fn list(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Query(q): Query<ListQuery>,
) -> Response {
    let viewer = Viewer::from(user.as_ref());
    let db = state.db.lock().await;
    if !series_visible(&db, q.series, &viewer) {
        return err(StatusCode::NOT_FOUND, "not_found", "No such chapter.");
    }
    let Ok((tops, replies)) =
        mangaread_core::db::list_comment_page(&db, q.series, q.chapter, q.before, PAGE_SIZE)
    else {
        return db_error();
    };
    let count = mangaread_core::db::count_visible_comments(&db, q.series, q.chapter).unwrap_or(0);
    let muted_until = user.as_ref().and_then(|u| {
        mangaread_core::db::muted_until(&db, u.id, &crate::auth::now_utc_string())
            .ok()
            .flatten()
    });
    drop(db);

    let next_before = (tops.len() as i64 == PAGE_SIZE)
        .then(|| tops.last().map(|c| c.id))
        .flatten();
    let threads: Vec<Value> = tops
        .iter()
        .filter_map(|top| {
            let visible_replies: Vec<Value> = replies
                .iter()
                .filter(|r| r.parent_id == Some(top.id))
                .filter_map(|r| view(r, false, &viewer))
                .collect();
            let mut t = view(top, !visible_replies.is_empty(), &viewer)?;
            t["replies"] = Value::Array(visible_replies);
            Some(t)
        })
        .collect();
    Json(json!({
        "ok": true,
        "comments": threads,
        "next_before": next_before,
        "count": count,
        "viewer": {
            "logged_in": viewer.user_id.is_some(),
            "can_moderate": viewer.can_moderate,
            "muted_until": muted_until,
        },
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct NewCommentBody {
    pub series_id: i64,
    pub chapter_number: f64,
    pub body: String,
    #[serde(default)]
    pub spoiler: bool,
    pub parent_id: Option<i64>,
}

/// POST `/api/comments` — post a comment or a reply.
pub async fn create(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(form): Json<NewCommentBody>,
) -> Response {
    let body = match clean_body(&form.body) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let viewer = Viewer::from(Some(&user));
    let db = state.db.lock().await;
    if !series_visible(&db, form.series_id, &viewer)
        || !matches!(
            mangaread_core::db::find_chapter_in_series(&db, form.series_id, form.chapter_number),
            Ok(Some(_))
        )
    {
        return err(StatusCode::NOT_FOUND, "not_found", "No such chapter.");
    }
    if let Ok(Some(until)) =
        mangaread_core::db::muted_until(&db, user.id, &crate::auth::now_utc_string())
    {
        return err(
            StatusCode::FORBIDDEN,
            "muted",
            &format!("A moderator paused your commenting until {until} UTC."),
        );
    }
    // Checked after validation so typos and refused posts do not use quota.
    if !state
        .rate_limiter
        .allow(&format!("comment_burst:{}", user.id), 1, BURST_WINDOW)
    {
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            "slow_down",
            "Please wait a few seconds between comments.",
        );
    }
    if !state
        .rate_limiter
        .allow(&format!("comment_day:{}", user.id), PER_DAY, DAY)
    {
        return err(
            StatusCode::TOO_MANY_REQUESTS,
            "daily_limit",
            "You've reached today's comment limit.",
        );
    }
    let new = NewComment {
        series_id: form.series_id,
        chapter_number: form.chapter_number,
        user_id: user.id,
        parent_id: form.parent_id,
        body,
        spoiler: form.spoiler,
    };
    let id = match mangaread_core::db::create_comment(&db, &new) {
        Ok(id) => id,
        Err(Error::NotFound(_) | Error::Conflict(_)) => {
            return err(
                StatusCode::BAD_REQUEST,
                "bad_parent",
                "That comment can no longer be replied to.",
            );
        }
        Err(_) => return db_error(),
    };
    match mangaread_core::db::get_comment(&db, id) {
        Ok(c) => {
            let mut v = view(&c, false, &viewer).unwrap_or(Value::Null);
            v["replies"] = json!([]);
            (
                StatusCode::CREATED,
                Json(json!({ "ok": true, "comment": v })),
            )
                .into_response()
        }
        Err(_) => db_error(),
    }
}

#[derive(Deserialize)]
pub struct EditBody {
    pub body: String,
}

/// POST `/api/comments/{id}/edit` — authors edit their own live comments.
pub async fn edit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(form): Json<EditBody>,
) -> Response {
    let body = match clean_body(&form.body) {
        Ok(b) => b,
        Err(r) => return r,
    };
    let db = state.db.lock().await;
    match mangaread_core::db::edit_comment(&db, id, user.id, &body) {
        Ok(()) => Json(json!({ "ok": true, "body": body })).into_response(),
        Err(Error::NotFound(_)) => err(
            StatusCode::NOT_FOUND,
            "not_found",
            "That comment can't be edited.",
        ),
        Err(_) => db_error(),
    }
}

/// POST `/api/comments/{id}/delete` — authors delete their own comments.
pub async fn delete(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    let db = state.db.lock().await;
    match mangaread_core::db::delete_comment(&db, id, user.id) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(Error::NotFound(_)) => err(
            StatusCode::NOT_FOUND,
            "not_found",
            "That comment can't be deleted.",
        ),
        Err(_) => db_error(),
    }
}

#[derive(Deserialize)]
pub struct ReportBody {
    #[serde(default)]
    pub reason: String,
}

/// POST `/api/comments/{id}/report` — flag a comment for moderators.
pub async fn report(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Json(form): Json<ReportBody>,
) -> Response {
    let reason: String = form.reason.trim().chars().take(MAX_REASON_CHARS).collect();
    let db = state.db.lock().await;
    match mangaread_core::db::report_comment(&db, id, user.id, &reason) {
        Ok(()) => Json(json!({ "ok": true })).into_response(),
        Err(Error::NotFound(_)) => err(
            StatusCode::NOT_FOUND,
            "not_found",
            "That comment no longer exists.",
        ),
        Err(Error::Conflict(_)) => err(
            StatusCode::BAD_REQUEST,
            "own_comment",
            "You can't report your own comment.",
        ),
        Err(_) => db_error(),
    }
}

/// POST `/api/comments/{id}/hide` and `/restore` — moderators only.
async fn set_hidden(state: &AppState, user: &AuthUser, id: i64, hidden: bool) -> Response {
    if !user.role.at_least(&Role::Moderator) {
        return err(StatusCode::FORBIDDEN, "forbidden", "Moderators only.");
    }
    let db = state.db.lock().await;
    match mangaread_core::db::set_comment_hidden(&db, id, user.id, hidden) {
        Ok(()) => {
            let action = if hidden {
                "comment_hide"
            } else {
                "comment_restore"
            };
            let _ =
                mangaread_core::db::insert_mod_log(&db, user.id, action, None, &format!("#{id}"));
            Json(json!({ "ok": true })).into_response()
        }
        Err(Error::NotFound(_)) => err(
            StatusCode::NOT_FOUND,
            "not_found",
            "That comment no longer exists.",
        ),
        Err(_) => db_error(),
    }
}

pub async fn hide(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    set_hidden(&state, &user, id, true).await
}

pub async fn restore(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    set_hidden(&state, &user, id, false).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{TestApp, app_with_admin};

    fn sample(deleted: bool, hidden: bool) -> Comment {
        Comment {
            id: 1,
            series_id: 1,
            chapter_number: 1.0,
            user_id: 5,
            author: "a".into(),
            parent_id: None,
            body: "secret body".into(),
            spoiler: false,
            created_at: "t".into(),
            edited_at: None,
            hidden_at: hidden.then(|| "t".into()),
            deleted_at: deleted.then(|| "t".into()),
        }
    }

    #[test]
    fn visibility_rules() {
        let reader = Viewer {
            user_id: Some(9),
            can_moderate: false,
        };
        let modr = Viewer {
            user_id: Some(2),
            can_moderate: true,
        };
        assert!(view(&sample(true, false), false, &modr).is_none());
        let deleted = view(&sample(true, false), true, &reader).unwrap();
        assert_eq!(deleted["placeholder"], "deleted");
        assert!(deleted.get("body").is_none());
        assert!(view(&sample(false, true), false, &reader).is_none());
        assert_eq!(
            view(&sample(false, true), true, &reader).unwrap()["placeholder"],
            "removed"
        );
        let as_mod = view(&sample(false, true), false, &modr).unwrap();
        assert_eq!(as_mod["hidden"], true);
        assert_eq!(as_mod["body"], "secret body");
        let mine = view(
            &sample(false, false),
            false,
            &Viewer {
                user_id: Some(5),
                can_moderate: false,
            },
        )
        .unwrap();
        assert_eq!(mine["mine"], true);
    }

    /// Admin app with one series that has chapter 1 from one source.
    async fn with_chapter() -> (TestApp, String, i64) {
        let (app, admin) = app_with_admin().await;
        let sid = {
            let db = app.state.db.lock().await;
            let sid = mangaread_core::db::insert_series(&db, "S").unwrap();
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id) VALUES ('a','s','S','u',?1)",
                [sid],
            )
            .unwrap();
            db.execute(
                "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (1,'1',1,'u')",
                [],
            )
            .unwrap();
            sid
        };
        (app, admin, sid)
    }

    #[tokio::test]
    async fn anonymous_reads_but_cannot_post() {
        let (app, _, sid) = with_chapter().await;
        let list = app
            .get(&format!("/api/comments?series={sid}&chapter=1"), None)
            .await;
        assert_eq!(list.status, StatusCode::OK);
        assert!(list.body.contains("\"logged_in\":false"));
        let post = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "hi"}),
                None,
            )
            .await;
        assert_eq!(post.status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn post_reply_edit_delete_flow() {
        let (app, _, sid) = with_chapter().await;
        let (_, alice) = app.member("alice@example.com").await;
        let (_, bob) = app.member("bob@example.com").await;
        let res = app
            .post_json("/api/comments", json!({"series_id": sid, "chapter_number": 1, "body": "  First!  ", "spoiler": true}), Some(&alice))
            .await;
        assert_eq!(res.status, StatusCode::CREATED);
        let created: Value = serde_json::from_str(&res.body).unwrap();
        let top = created["comment"]["id"].as_i64().unwrap();
        assert_eq!(created["comment"]["body"], "First!");
        assert_eq!(created["comment"]["spoiler"], true);

        let reply = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "reply", "parent_id": top}),
                Some(&bob),
            )
            .await;
        assert_eq!(reply.status, StatusCode::CREATED);

        let hijack = app
            .post_json(
                &format!("/api/comments/{top}/edit"),
                json!({"body": "x"}),
                Some(&bob),
            )
            .await;
        assert_eq!(hijack.status, StatusCode::NOT_FOUND);
        let ok = app
            .post_json(
                &format!("/api/comments/{top}/edit"),
                json!({"body": "First (edited)"}),
                Some(&alice),
            )
            .await;
        assert_eq!(ok.status, StatusCode::OK);
        app.post_json(
            &format!("/api/comments/{top}/delete"),
            json!({}),
            Some(&alice),
        )
        .await;

        let list: Value = serde_json::from_str(
            &app.get(&format!("/api/comments?series={sid}&chapter=1"), None)
                .await
                .body,
        )
        .unwrap();
        let thread = &list["comments"][0];
        assert_eq!(
            thread["placeholder"], "deleted",
            "kept because it has a reply"
        );
        assert_eq!(thread["replies"][0]["body"], "reply");
        assert_eq!(list["count"], 1);
    }

    #[tokio::test]
    async fn validation_rate_limit_and_unknown_chapters() {
        let (app, _, sid) = with_chapter().await;
        let (_, alice) = app.member("alice@example.com").await;
        let empty = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "   "}),
                Some(&alice),
            )
            .await;
        assert_eq!(empty.status, StatusCode::BAD_REQUEST);
        let long = "x".repeat(MAX_COMMENT_CHARS + 1);
        let too_long = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": long}),
                Some(&alice),
            )
            .await;
        assert_eq!(too_long.status, StatusCode::BAD_REQUEST);
        let ghost = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 99, "body": "hi"}),
                Some(&alice),
            )
            .await;
        assert_eq!(ghost.status, StatusCode::NOT_FOUND);
        let first = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "one"}),
                Some(&alice),
            )
            .await;
        assert_eq!(first.status, StatusCode::CREATED);
        let second = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "two"}),
                Some(&alice),
            )
            .await;
        assert_eq!(second.status, StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn moderators_hide_and_readers_stop_seeing_it() {
        let (app, _, sid) = with_chapter().await;
        let (_, alice) = app.member("alice@example.com").await;
        let (mod_id, modr) = app.member("mod@example.com").await;
        {
            let db = app.state.db.lock().await;
            mangaread_core::db::set_user_role(&db, mod_id, &Role::Moderator).unwrap();
        }
        let res = app
            .post_json(
                "/api/comments",
                json!({"series_id": sid, "chapter_number": 1, "body": "rude"}),
                Some(&alice),
            )
            .await;
        let id = serde_json::from_str::<Value>(&res.body).unwrap()["comment"]["id"]
            .as_i64()
            .unwrap();
        let denied = app
            .post_json(&format!("/api/comments/{id}/hide"), json!({}), Some(&alice))
            .await;
        assert_eq!(denied.status, StatusCode::FORBIDDEN);
        let hid = app
            .post_json(&format!("/api/comments/{id}/hide"), json!({}), Some(&modr))
            .await;
        assert_eq!(hid.status, StatusCode::OK);
        let anon = app
            .get(&format!("/api/comments?series={sid}&chapter=1"), None)
            .await
            .body;
        assert!(!anon.contains("rude"));
        let as_mod = app
            .get(
                &format!("/api/comments?series={sid}&chapter=1"),
                Some(&modr),
            )
            .await
            .body;
        assert!(as_mod.contains("rude") && as_mod.contains("\"hidden\":true"));
    }
}
