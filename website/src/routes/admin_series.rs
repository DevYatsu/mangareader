//! `/admin/series`: catalog fixes without touching SQLite.
//!
//! Rename (locks the title against syncs), cover override, hide/unhide,
//! merge two series, and split one source entry into its own series.
//! Admin-only; moderators have no catalog access. Every change is written
//! to `mod_log`.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use mangaread_core::domain::{AdminSeriesRow, Manga, Role, Series};
use mangaread_core::error::Error;
use maud::{Markup, html};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{AuthUser, require_role},
    routes::admin::page,
};

const MAX_TITLE_CHARS: usize = 200;

fn admin_guard(user: &AuthUser) -> Option<Response> {
    require_role(user, Role::Admin)
        .err()
        .map(|_| StatusCode::FORBIDDEN.into_response())
}

/// Fixed notice copy for `?ok=` / `?err=` codes; unknown codes render nothing.
fn notice(q: &HashMap<String, String>) -> Markup {
    let (class, text) = match (
        q.get("ok").map(String::as_str),
        q.get("err").map(String::as_str),
    ) {
        (Some("renamed"), _) => (
            "is-success",
            "Renamed. Syncs will no longer change this title.",
        ),
        (Some("cover"), _) => ("is-success", "Cover updated."),
        (Some("hidden"), _) => ("is-success", "Hidden from readers."),
        (Some("shown"), _) => ("is-success", "Visible to readers again."),
        (Some("merged"), _) => (
            "is-success",
            "Merged. The other series was removed and its bookmarks moved here.",
        ),
        (Some("split"), _) => ("is-success", "Split into its own series."),
        (_, Some("bad_title")) => ("is-danger", "Titles must be 1-200 characters."),
        (_, Some("bad_cover")) => (
            "is-danger",
            "The cover must be an http:// or https:// image URL.",
        ),
        (_, Some("bad_target")) => (
            "is-danger",
            "Pick a different, existing series to merge into.",
        ),
        (_, Some("last_entry")) => (
            "is-danger",
            "That is the series' only source entry; there is nothing to split.",
        ),
        _ => return html! {},
    };
    html! { div class=(format!("notification is-light {class}")) { (text) } }
}

fn flags(hidden: bool, locked: bool) -> Markup {
    html! {
        @if hidden { span class="tag is-warning is-light" { "Hidden" } " " }
        @if locked { span class="tag is-light" { "Title locked" } }
    }
}

fn list_content(rows: &[AdminSeriesRow], search: &str, note: Markup) -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Series" }
            (note)
            form class="field has-addons" method="get" action="/admin/series" {
                div class="control is-expanded" {
                    input class="input" type="search" name="q" value=(search) placeholder="Search titles, hidden ones included";
                }
                div class="control" {
                    button class="button is-primary" type="submit" { "Search" }
                }
            }
            @if rows.is_empty() {
                p class="m-text-muted" { "No series match." }
            } @else {
                div class="box" {
                    div class="table-container" {
                        table class="table is-fullwidth is-hoverable" {
                            thead {
                                tr {
                                    th { "ID" }
                                    th { "Title" }
                                    th { "Chapters" }
                                    th { "Sources" }
                                    th {}
                                }
                            }
                            tbody {
                                @for r in rows {
                                    tr {
                                        td { (r.id) }
                                        td { (r.title) " " (flags(r.hidden, r.title_locked)) }
                                        td { (r.chapter_count) }
                                        td { (r.manga_count) }
                                        td { a class="button is-small is-light" href=(format!("/admin/series/{}", r.id)) { "Manage" } }
                                    }
                                }
                            }
                        }
                    }
                }
                p class="is-size-7 m-text-muted" { "Showing up to 100, newest first." }
            }
            a class="button is-light" href="/admin" { "Back to admin" }
        }
    }
}

/// GET `/admin/series?q=` — every series, hidden ones included.
pub async fn series_list(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let search = q.get("q").map(|s| s.trim()).unwrap_or("");
    let (cfg, rows) = {
        let db = state.db.lock().await;
        let rows =
            mangaread_core::db::admin_list_series(&db, (!search.is_empty()).then_some(search), 100)
                .unwrap_or_default();
        (crate::config::site_config(&db), rows)
    };
    page(
        &cfg,
        "Series",
        list_content(&rows, search, notice(&q)),
        Some(&user),
    )
    .into_response()
}

fn edit_content(s: &Series, entries: &[(Manga, usize)], note: Markup) -> Markup {
    let action = |verb: &str| format!("/admin/series/{}/{verb}", s.id);
    html! {
        section {
            p class="is-size-7" { a href="/admin/series" { "← All series" } }
            h1 class="title is-2" { (s.title) }
            p class="mb-4" {
                (flags(s.hidden, s.title_locked))
                " " a href=(format!("/series/{}", s.id)) { "View public page" }
            }
            (note)
            div class="columns is-multiline" {
                div class="column is-half" {
                    div class="box" {
                        h2 class="title is-5" { "Title" }
                        form method="post" action=(action("rename")) {
                            div class="field has-addons" {
                                div class="control is-expanded" {
                                    input class="input" type="text" name="title" value=(s.title) maxlength="200" required="";
                                }
                                div class="control" {
                                    button class="button is-primary" type="submit" { "Rename" }
                                }
                            }
                            p class="help" { "Renaming locks the title so syncs stop changing it." }
                        }
                    }
                }
                div class="column is-half" {
                    div class="box" {
                        h2 class="title is-5" { "Cover" }
                        form method="post" action=(action("cover")) {
                            div class="field has-addons" {
                                div class="control is-expanded" {
                                    input class="input" type="url" name="cover_url" value=(s.cover_override.as_deref().unwrap_or(""))
                                        placeholder="https://… (blank uses the source cover)";
                                }
                                div class="control" {
                                    button class="button" type="submit" { "Save" }
                                }
                            }
                        }
                        @if let Some(c) = &s.cover_override {
                            figure class="image mt-3" style="max-width:120px" { img src=(c) alt="Cover override" loading="lazy"; }
                        }
                    }
                }
                div class="column is-half" {
                    div class="box" {
                        h2 class="title is-5" { "Visibility" }
                        form method="post" action=(action("hide")) {
                            @if s.hidden {
                                input type="hidden" name="hidden" value="0";
                                p class="content" { "Readers cannot find or open this series." }
                                button class="button" type="submit" { "Show to readers" }
                            } @else {
                                input type="hidden" name="hidden" value="1";
                                p class="content" { "Hide it from lists, search, the home page and the reader. Nothing is deleted." }
                                button class="button is-warning is-light" type="submit" { "Hide from readers" }
                            }
                        }
                    }
                }
                div class="column is-half" {
                    div class="box" {
                        h2 class="title is-5" { "Merge" }
                        form method="post" action=(action("merge")) {
                            div class="field has-addons" {
                                div class="control" {
                                    input class="input" type="number" name="into" min="1" placeholder="Series ID" required="";
                                }
                                div class="control" {
                                    button class="button is-danger is-light" type="submit" { "Merge into" }
                                }
                            }
                            p class="help" {
                                "Use when two series are the same title. This series is removed; its sources and "
                                "bookmarks move to the target. Find IDs in the "
                                a href="/admin/series" { "series list" } "."
                            }
                        }
                    }
                }
            }
            div class="box" {
                h2 class="title is-5" { "Source entries" }
                div class="table-container" {
                    table class="table is-fullwidth" {
                        thead {
                            tr {
                                th { "Source" }
                                th { "Title on source" }
                                th { "Chapters" }
                                th { "Last synced" }
                                th {}
                            }
                        }
                        tbody {
                            @for (m, chapters) in entries {
                                tr {
                                    td { (m.source) }
                                    td { a href=(m.detail_url) rel="noreferrer" target="_blank" { (m.title) } }
                                    td { (chapters) }
                                    td class="is-size-7" { (m.chapters_synced_at.as_deref().unwrap_or("never")) }
                                    td {
                                        @if entries.len() > 1 {
                                            form method="post" action=(format!("/admin/manga/{}/split", m.id)) {
                                                button class="button is-small is-light" type="submit" { "Split out" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                p class="help" { "Split out moves one source entry into a new series of its own, for titles that were merged by mistake." }
            }
        }
    }
}

/// GET `/admin/series/{id}`.
pub async fn series_edit(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let (cfg, series, entries) = {
        let db = state.db.lock().await;
        let Ok(series) = mangaread_core::db::get_series_by_id(&db, id) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        let entries: Vec<(Manga, usize)> = mangaread_core::db::list_manga_by_series(&db, id)
            .unwrap_or_default()
            .into_iter()
            .map(|m| {
                let n = mangaread_core::db::list_chapters_by_manga(&db, m.id)
                    .map(|c| c.len())
                    .unwrap_or(0);
                (m, n)
            })
            .collect();
        (crate::config::site_config(&db), series, entries)
    };
    page(
        &cfg,
        &series.title.clone(),
        edit_content(&series, &entries, notice(&q)),
        Some(&user),
    )
    .into_response()
}

fn back(id: i64, query: &str) -> Response {
    Redirect::to(&format!("/admin/series/{id}?{query}")).into_response()
}

/// Map a write result to a redirect: `NotFound` → 404, `Conflict` →
/// `err_code`, other failures → 500, success → `ok_code`.
fn outcome(id: i64, result: Result<(), Error>, ok_code: &str, err_code: &str) -> Response {
    match result {
        Ok(()) => back(id, &format!("ok={ok_code}")),
        Err(Error::NotFound(_)) => StatusCode::NOT_FOUND.into_response(),
        Err(Error::Conflict(_)) => back(id, &format!("err={err_code}")),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
pub struct RenameForm {
    pub title: String,
}

/// POST `/admin/series/{id}/rename`.
pub async fn series_rename(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<RenameForm>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let title = form.title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return back(id, "err=bad_title");
    }
    let db = state.db.lock().await;
    let result = mangaread_core::db::rename_series(&db, id, title);
    if result.is_ok() {
        crate::scraper_helper::refresh_search_index(&db);
        let _ = mangaread_core::db::insert_mod_log(
            &db,
            user.id,
            "series_rename",
            None,
            &format!("#{id} → {title}"),
        );
    }
    outcome(id, result, "renamed", "bad_title")
}

#[derive(Deserialize)]
pub struct CoverForm {
    #[serde(default)]
    pub cover_url: String,
}

/// POST `/admin/series/{id}/cover` — blank clears the override.
pub async fn series_cover(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<CoverForm>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let url = form.cover_url.trim();
    // Only web URLs: a `javascript:` or `data:` value would end up in `src`.
    if !url.is_empty() && !(url.starts_with("https://") || url.starts_with("http://")) {
        return back(id, "err=bad_cover");
    }
    let db = state.db.lock().await;
    let result = mangaread_core::db::set_series_cover(&db, id, (!url.is_empty()).then_some(url));
    if result.is_ok() {
        let _ = mangaread_core::db::insert_mod_log(
            &db,
            user.id,
            "series_cover",
            None,
            &format!("#{id}"),
        );
    }
    outcome(id, result, "cover", "bad_cover")
}

#[derive(Deserialize)]
pub struct HideForm {
    pub hidden: String,
}

/// POST `/admin/series/{id}/hide` — `hidden=1` hides, `hidden=0` shows.
pub async fn series_hide(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<HideForm>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let hide = form.hidden == "1";
    let now = crate::auth::now_utc_string();
    let db = state.db.lock().await;
    let result = mangaread_core::db::set_series_hidden(&db, id, hide.then_some(now.as_str()));
    let action = if hide { "series_hide" } else { "series_show" };
    if result.is_ok() {
        let _ = mangaread_core::db::insert_mod_log(&db, user.id, action, None, &format!("#{id}"));
    }
    outcome(
        id,
        result,
        if hide { "hidden" } else { "shown" },
        "bad_target",
    )
}

#[derive(Deserialize)]
pub struct MergeForm {
    pub into: i64,
}

/// POST `/admin/series/{id}/merge` — merge this series into `into`.
pub async fn series_merge(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
    Form(form): Form<MergeForm>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let db = state.db.lock().await;
    match mangaread_core::db::merge_series(&db, id, form.into) {
        Ok(()) => {
            crate::scraper_helper::refresh_search_index(&db);
            let _ = mangaread_core::db::insert_mod_log(
                &db,
                user.id,
                "series_merge",
                None,
                &format!("#{id} → #{}", form.into),
            );
            back(form.into, "ok=merged")
        }
        // A missing target is the admin's typo, not a missing page.
        Err(Error::NotFound(_) | Error::Conflict(_)) => back(id, "err=bad_target"),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// POST `/admin/manga/{id}/split` — move one source entry into a new series.
pub async fn manga_split(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(manga_id): Path<i64>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let db = state.db.lock().await;
    let Ok(manga) = mangaread_core::db::get_manga_by_id(&db, manga_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let old = manga.series_id.unwrap_or(0);
    match mangaread_core::db::split_manga(&db, manga_id) {
        Ok(new_id) => {
            crate::scraper_helper::refresh_search_index(&db);
            let _ = mangaread_core::db::insert_mod_log(
                &db,
                user.id,
                "series_split",
                None,
                &format!("manga #{manga_id}: #{old} → #{new_id}"),
            );
            back(new_id, "ok=split")
        }
        Err(Error::Conflict(_)) => back(old, "err=last_entry"),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{TestApp, app_with_admin};
    use axum::http::StatusCode;

    /// Seed a series with one source entry per `(source, chapters)`.
    async fn seed(app: &TestApp, title: &str, entries: &[(&str, &[f64])]) -> i64 {
        let db = app.state.db.lock().await;
        let sid = mangaread_core::db::insert_series(&db, title).unwrap();
        for (source, numbers) in entries {
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id)
                 VALUES (?1, ?2, ?2, 'https://example.com', ?3)",
                rusqlite::params![source, title, sid],
            )
            .unwrap();
            let mid = db.last_insert_rowid();
            for n in *numbers {
                db.execute(
                    "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (?1, ?2, ?3, 'u')",
                    rusqlite::params![mid, n.to_string(), n],
                )
                .unwrap();
            }
        }
        sid
    }

    #[tokio::test]
    async fn members_cannot_touch_the_catalog() {
        let (app, _) = app_with_admin().await;
        let (_, member) = app.member("m@example.com").await;
        let sid = seed(&app, "Alpha", &[("a", &[1.0])]).await;
        assert_eq!(
            app.get("/admin/series", Some(&member)).await.status,
            StatusCode::FORBIDDEN
        );
        let res = app
            .post_form(
                &format!("/admin/series/{sid}/hide"),
                &[("hidden", "1")],
                Some(&member),
            )
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
        // Moderators are not admins either.
        {
            let db = app.state.db.lock().await;
            mangaread_core::db::set_user_role(&db, 2, &mangaread_core::domain::Role::Moderator)
                .unwrap();
        }
        assert_eq!(
            app.get("/admin/series", Some(&member)).await.status,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn hidden_series_404_for_readers_but_not_admins() {
        let (app, admin) = app_with_admin().await;
        let sid = seed(&app, "Secret", &[("a", &[1.0])]).await;
        let res = app
            .post_form(
                &format!("/admin/series/{sid}/hide"),
                &[("hidden", "1")],
                Some(&admin),
            )
            .await;
        assert_eq!(res.location, Some(format!("/admin/series/{sid}?ok=hidden")));
        assert_eq!(
            app.get(&format!("/series/{sid}"), None).await.status,
            StatusCode::NOT_FOUND
        );
        assert!(!app.get("/series", None).await.body.contains("Secret"));
        let admin_view = app.get(&format!("/series/{sid}"), Some(&admin)).await;
        assert_eq!(admin_view.status, StatusCode::OK);
        assert!(admin_view.body.contains("Hidden from readers"));
        let reader = app.get("/read/1/1", None).await;
        assert_eq!(
            reader.status,
            StatusCode::NOT_FOUND,
            "chapters of hidden series too"
        );
    }

    #[tokio::test]
    async fn rename_and_cover_validate_and_apply() {
        let (app, admin) = app_with_admin().await;
        let sid = seed(&app, "alpha raw", &[("a", &[1.0])]).await;
        let bad = app
            .post_form(
                &format!("/admin/series/{sid}/cover"),
                &[("cover_url", "javascript:alert(1)")],
                Some(&admin),
            )
            .await;
        assert_eq!(
            bad.location,
            Some(format!("/admin/series/{sid}?err=bad_cover"))
        );
        app.post_form(
            &format!("/admin/series/{sid}/cover"),
            &[("cover_url", "https://img.example/c.jpg")],
            Some(&admin),
        )
        .await;
        app.post_form(
            &format!("/admin/series/{sid}/rename"),
            &[("title", "Alpha")],
            Some(&admin),
        )
        .await;
        let grid = app.get("/series", None).await.body;
        assert!(grid.contains("Alpha"));
        assert!(grid.contains("https://img.example/c.jpg"));
        let empty = app
            .post_form(
                &format!("/admin/series/{sid}/rename"),
                &[("title", "  ")],
                Some(&admin),
            )
            .await;
        assert_eq!(
            empty.location,
            Some(format!("/admin/series/{sid}?err=bad_title"))
        );
        let db = app.state.db.lock().await;
        let log = mangaread_core::db::list_mod_log(&db, 5).unwrap();
        assert!(log.iter().any(|e| e.action == "series_rename"));
    }

    #[tokio::test]
    async fn merge_and_split_round_trip() {
        let (app, admin) = app_with_admin().await;
        let keep = seed(&app, "Solo Leveling", &[("a", &[1.0])]).await;
        let dup = seed(&app, "Only I Level Up", &[("b", &[1.0, 2.0])]).await;
        let res = app
            .post_form(
                &format!("/admin/series/{dup}/merge"),
                &[("into", &keep.to_string())],
                Some(&admin),
            )
            .await;
        assert_eq!(
            res.location,
            Some(format!("/admin/series/{keep}?ok=merged"))
        );
        assert_eq!(
            app.get(&format!("/admin/series/{dup}"), Some(&admin))
                .await
                .status,
            StatusCode::NOT_FOUND
        );
        let self_merge = app
            .post_form(
                &format!("/admin/series/{keep}/merge"),
                &[("into", &keep.to_string())],
                Some(&admin),
            )
            .await;
        assert_eq!(
            self_merge.location,
            Some(format!("/admin/series/{keep}?err=bad_target"))
        );
        let missing = app
            .post_form(
                &format!("/admin/series/{keep}/merge"),
                &[("into", "999")],
                Some(&admin),
            )
            .await;
        assert_eq!(
            missing.location,
            Some(format!("/admin/series/{keep}?err=bad_target"))
        );

        let edit = app
            .get(&format!("/admin/series/{keep}"), Some(&admin))
            .await
            .body;
        assert_eq!(edit.matches("Split out</button>").count(), 2);
        let res = app
            .post_form("/admin/manga/2/split", &[], Some(&admin))
            .await;
        let new_id = res.location.unwrap();
        assert!(new_id.ends_with("?ok=split"));
        let last = app
            .post_form("/admin/manga/1/split", &[], Some(&admin))
            .await;
        assert_eq!(
            last.location,
            Some(format!("/admin/series/{keep}?err=last_entry"))
        );
    }
}
