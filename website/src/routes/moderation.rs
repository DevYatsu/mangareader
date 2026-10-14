//! `/mod/reports`: the comment moderation queue (moderators and admins).
//!
//! Reported comments are grouped, most-reported first. A moderator hides
//! the comment, dismisses the reports, or mutes the author from commenting.
//! Every action is written to `mod_log`.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use mangaread_core::domain::{ReportGroup, Role};
use maud::{Markup, html};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser};

fn guard(user: &AuthUser) -> Option<Response> {
    (!user.role.at_least(&Role::Moderator)).then(|| StatusCode::FORBIDDEN.into_response())
}

/// Fixed notice copy for `?ok=` / `?err=` codes; unknown codes render nothing.
fn notice(q: &HashMap<String, String>) -> Markup {
    let (class, text) = match (
        q.get("ok").map(String::as_str),
        q.get("err").map(String::as_str),
    ) {
        (Some("hidden"), _) => ("is-success", "Comment hidden and its reports closed."),
        (Some("dismissed"), _) => ("is-success", "Reports dismissed; the comment stays up."),
        (Some("muted"), _) => ("is-success", "Author muted from commenting."),
        (Some("unmuted"), _) => ("is-success", "Mute lifted."),
        (_, Some("rank")) => ("is-danger", "Only admins can mute moderators or admins."),
        (_, Some("self")) => ("is-danger", "You can't mute yourself."),
        _ => return html! {},
    };
    html! { div class=(format!("notification is-light {class}")) { (text) } }
}

/// One queue entry plus the chapter link, resolved while the DB is locked.
struct Entry {
    group: ReportGroup,
    chapter_href: Option<String>,
}

fn content(entries: &[Entry], note: Markup) -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Reported comments" }
            (note)
            @if entries.is_empty() {
                div class="box" { p { "Nothing to review. Reported comments show up here." } }
            }
            @for e in entries {
                @let c = &e.group.comment;
                div class="box" {
                    div class="level is-mobile mb-2" {
                        div class="level-left" {
                            div {
                                p {
                                    strong { (c.author) }
                                    " on " (e.group.series_title) ", ch. " (c.chapter_number)
                                }
                                p class="is-size-7 m-text-muted" {
                                    (e.group.report_count) " report(s) since " (e.group.first_reported_at) " UTC"
                                    @if c.hidden_at.is_some() { " · already hidden" }
                                }
                            }
                        }
                        @if let Some(href) = &e.chapter_href {
                            div class="level-right" { a class="is-size-7" href=(href) { "Open chapter" } }
                        }
                    }
                    blockquote class="m-comment-quote" { (c.body) }
                    @if !e.group.reasons.is_empty() {
                        div class="tags mt-2" {
                            @for r in &e.group.reasons { span class="tag is-light" { (r) } }
                        }
                    }
                    div class="buttons are-small mt-3" {
                        form method="post" action=(format!("/mod/comments/{}/hide", c.id)) {
                            button class="button is-small is-danger is-light" type="submit" { "Hide comment" }
                        }
                        form method="post" action=(format!("/mod/comments/{}/dismiss", c.id)) {
                            button class="button is-small is-light" type="submit" { "Dismiss reports" }
                        }
                        form class="field has-addons" method="post" action=(format!("/mod/users/{}/mute", c.user_id)) {
                            div class="control" {
                                div class="select is-small" {
                                    select name="duration" aria-label={"Mute " (c.author) " for"} {
                                        option value="1d" { "Mute 1 day" }
                                        option value="7d" { "Mute 7 days" }
                                        option value="forever" { "Mute until lifted" }
                                        option value="off" { "Lift mute" }
                                    }
                                }
                            }
                            div class="control" {
                                button class="button is-small" type="submit" { "Apply" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// GET `/mod/reports`.
pub async fn reports_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = guard(&user) {
        return r;
    }
    let (cfg, entries) = {
        let db = state.db.lock().await;
        let entries: Vec<Entry> = mangaread_core::db::list_open_reports(&db, 50)
            .unwrap_or_default()
            .into_iter()
            .map(|group| {
                let chapter_href = mangaread_core::db::find_chapter_in_series(
                    &db,
                    group.comment.series_id,
                    group.comment.chapter_number,
                )
                .ok()
                .flatten()
                .map(|(m, c)| format!("/read/{m}/{c}#chapter-comments"));
                Entry {
                    group,
                    chapter_href,
                }
            })
            .collect();
        (crate::config::site_config(&db), entries)
    };
    crate::templates::layout::page(
        &cfg,
        "Moderation",
        crate::templates::layout::Nav::Moderation,
        Some(&user),
        content(&entries, notice(&q)),
    )
    .into_response()
}

fn back(query: &str) -> Response {
    Redirect::to(&format!("/mod/reports?{query}")).into_response()
}

/// POST `/mod/comments/{id}/hide` — hide and close its reports.
pub async fn hide(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    if let Some(r) = guard(&user) {
        return r;
    }
    let db = state.db.lock().await;
    match mangaread_core::db::set_comment_hidden(&db, id, user.id, true) {
        Ok(()) => {
            let _ = mangaread_core::db::insert_mod_log(
                &db,
                user.id,
                "comment_hide",
                None,
                &format!("#{id}"),
            );
            back("ok=hidden")
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// POST `/mod/comments/{id}/dismiss` — close reports, keep the comment.
pub async fn dismiss(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    if let Some(r) = guard(&user) {
        return r;
    }
    let db = state.db.lock().await;
    match mangaread_core::db::resolve_reports(&db, id, user.id) {
        Ok(_) => {
            let _ = mangaread_core::db::insert_mod_log(
                &db,
                user.id,
                "reports_dismiss",
                None,
                &format!("#{id}"),
            );
            back("ok=dismissed")
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
pub struct MuteForm {
    pub duration: String,
}

/// POST `/mod/users/{id}/mute` — `duration` is `1d`, `7d`, `forever` or `off`.
///
/// Moderators may mute only members; muting a moderator or admin takes an
/// admin, so moderators cannot silence each other or their admins.
pub async fn mute(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(target): Path<i64>,
    Form(form): Form<MuteForm>,
) -> Response {
    if let Some(r) = guard(&user) {
        return r;
    }
    if target == user.id {
        return back("err=self");
    }
    let until = match form.duration.as_str() {
        "1d" => Some(crate::auth::session_expires_at(24 * 3600)),
        "7d" => Some(crate::auth::session_expires_at(7 * 24 * 3600)),
        // Far-future sentinel: compares greater than any real timestamp.
        "forever" => Some("9999-12-31 23:59:59".to_string()),
        "off" => None,
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let db = state.db.lock().await;
    let Ok(target_user) = mangaread_core::db::find_user_by_id(&db, target) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if target_user.role != Role::Member && user.role != Role::Admin {
        return back("err=rank");
    }
    if mangaread_core::db::set_user_muted(&db, target, until.as_deref()).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let action = if until.is_some() { "mute" } else { "unmute" };
    let _ = mangaread_core::db::insert_mod_log(&db, user.id, action, Some(target), &form.duration);
    back(if until.is_some() {
        "ok=muted"
    } else {
        "ok=unmuted"
    })
}

#[cfg(test)]
mod tests {
    use crate::test_support::{TestApp, app_with_admin};
    use axum::http::StatusCode;
    use mangaread_core::domain::{NewComment, Role};

    /// A moderator, a member author with one reported comment.
    async fn setup() -> (TestApp, String, String, i64, i64) {
        let (app, _) = app_with_admin().await;
        let (author_id, author) = app.member("author@example.com").await;
        let (mod_id, modr) = app.member("mod@example.com").await;
        let (reporter_id, _) = app.member("reporter@example.com").await;
        let cid = {
            let db = app.state.db.lock().await;
            mangaread_core::db::set_user_role(&db, mod_id, &Role::Moderator).unwrap();
            let sid = mangaread_core::db::insert_series(&db, "Series Q").unwrap();
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id) VALUES ('a','q','Q','u',?1)",
                [sid],
            )
            .unwrap();
            db.execute(
                "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (last_insert_rowid(),'3',3,'u')",
                [],
            )
            .unwrap();
            let cid = mangaread_core::db::create_comment(
                &db,
                &NewComment {
                    series_id: sid,
                    chapter_number: 3.0,
                    user_id: author_id,
                    parent_id: None,
                    body: "<i>spam</i> link".into(),
                    spoiler: false,
                },
            )
            .unwrap();
            mangaread_core::db::report_comment(&db, cid, reporter_id, "Spam").unwrap();
            cid
        };
        (app, modr, author, cid, author_id)
    }

    #[tokio::test]
    async fn members_cannot_see_the_queue() {
        let (app, _, author, cid, _) = setup().await;
        assert_eq!(
            app.get("/mod/reports", Some(&author)).await.status,
            StatusCode::FORBIDDEN
        );
        let res = app
            .post_form(&format!("/mod/comments/{cid}/hide"), &[], Some(&author))
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn queue_lists_escapes_and_hide_clears_it() {
        let (app, modr, _, cid, _) = setup().await;
        let page = app.get("/mod/reports", Some(&modr)).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("Series Q"));
        assert!(page.body.contains("&lt;i&gt;spam"), "bodies are escaped");
        assert!(page.body.contains(">Spam<"));
        let res = app
            .post_form(&format!("/mod/comments/{cid}/hide"), &[], Some(&modr))
            .await;
        assert_eq!(res.location.as_deref(), Some("/mod/reports?ok=hidden"));
        assert!(
            app.get("/mod/reports", Some(&modr))
                .await
                .body
                .contains("Nothing to review")
        );
    }

    #[tokio::test]
    async fn mute_blocks_posting_and_respects_rank() {
        let (app, modr, author, _, author_id) = setup().await;
        let res = app
            .post_form(
                &format!("/mod/users/{author_id}/mute"),
                &[("duration", "1d")],
                Some(&modr),
            )
            .await;
        assert_eq!(res.location.as_deref(), Some("/mod/reports?ok=muted"));
        let post = app
            .post_json(
                "/api/comments",
                serde_json::json!({"series_id": 1, "chapter_number": 3, "body": "hi"}),
                Some(&author),
            )
            .await;
        assert_eq!(post.status, StatusCode::FORBIDDEN);
        assert!(post.body.contains("paused your commenting"));
        let page = app.get("/mod/reports", Some(&modr)).await;
        assert!(page.body.contains("Open chapter"), "links to the chapter");
        app.post_form(
            &format!("/mod/users/{author_id}/mute"),
            &[("duration", "off")],
            Some(&modr),
        )
        .await;
        let post = app
            .post_json(
                "/api/comments",
                serde_json::json!({"series_id": 1, "chapter_number": 3, "body": "back"}),
                Some(&author),
            )
            .await;
        assert_eq!(post.status, StatusCode::CREATED);
        // Moderators cannot mute the admin (user 1).
        let res = app
            .post_form("/mod/users/1/mute", &[("duration", "1d")], Some(&modr))
            .await;
        assert_eq!(res.location.as_deref(), Some("/mod/reports?err=rank"));
    }
}
