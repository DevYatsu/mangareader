//! `/admin/sources` and `/admin/sync`: source health, enable/disable,
//! on-demand scans, sync history, retries, waking the sync loop, and the
//! image cache cap.
//!
//! Everything shown here comes from the persisted `sync_runs` table, so it
//! survives restarts. Admin-only.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
};
use mangaread_core::db::{SourceHealth, SyncRun};
use mangaread_core::domain::Role;
use maud::{Markup, html};
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    AppState,
    auth::{AuthUser, require_role},
    routes::admin::page,
    scraper_helper::{Job, SOURCES_DISABLED_KEY, disabled_sources, source_list},
};

/// Failed runs in a row before a source is shown as failing rather than
/// flaky. One timeout is noise; three in a row is a broken adapter or a
/// site change.
const FAILING_STREAK: i64 = 3;

/// Fixed notice copy for `?ok=` / `?err=` codes. Unknown codes render
/// nothing, so the query string cannot inject text.
fn notice(q: &HashMap<String, String>) -> Markup {
    let (class, text) = match (
        q.get("ok").map(String::as_str),
        q.get("err").map(String::as_str),
    ) {
        (Some("scan"), _) => (
            "is-success",
            "Scan started. It appears in the history when it finishes.",
        ),
        (Some("retry"), _) => ("is-success", "Retry started."),
        (Some("run"), _) => ("is-success", "Full sync started."),
        (Some("enabled"), _) => ("is-success", "Source enabled."),
        (Some("cache"), _) => ("is-success", "Image cache limit saved."),
        (Some("disabled"), _) => (
            "is-success",
            "Source disabled. It will not be scanned or synced.",
        ),
        (_, Some("busy")) => (
            "is-warning",
            "That job is already running, or 3 jobs are running. Try again shortly.",
        ),
        (_, Some("cache_cap")) => (
            "is-danger",
            "The image cache limit must be a whole number from 100 to 1048576 MiB.",
        ),
        (_, Some("disabled_source")) => ("is-danger", "That source is disabled. Enable it first."),
        (_, Some("not_retryable")) => (
            "is-danger",
            "Only failed manga and chapter runs can be retried; use Scan now for scans.",
        ),
        _ => return html! {},
    };
    html! { div class=(format!("notification is-light {class}")) { (text) } }
}

fn status_tag(enabled: bool, h: &SourceHealth) -> Markup {
    let (class, label) = if !enabled {
        ("is-light", "Disabled".to_string())
    } else if h.failure_streak >= FAILING_STREAK {
        (
            "is-danger",
            format!("Failing ({} in a row)", h.failure_streak),
        )
    } else if h.failure_streak > 0 {
        (
            "is-warning",
            format!("{} recent failure(s)", h.failure_streak),
        )
    } else if h.last_success.is_some() {
        ("is-success", "Healthy".to_string())
    } else {
        ("is-light", "Never run".to_string())
    };
    html! { span class=(format!("tag {class}")) { (label) } }
}

/// Long WebDriver errors span many lines; show the first, the rest on demand.
fn error_cell(error: Option<&str>) -> Markup {
    match error {
        None => html! { span class="m-text-muted" { "—" } },
        Some(e) => {
            let first = e.lines().next().unwrap_or(e);
            html! {
                details {
                    summary { (first.chars().take(120).collect::<String>()) }
                    pre class="is-size-7" { (e) }
                }
            }
        }
    }
}

fn opt(v: Option<&str>) -> &str {
    v.unwrap_or("—")
}

struct SourceRow {
    name: String,
    base_url: String,
    enabled: bool,
    health: SourceHealth,
}

/// Image cache usage, both in MiB.
struct CacheUsage {
    used_mb: u64,
    cap_mb: u64,
}

/// Smallest and largest accepted cap (MiB). Below 100 MiB a single long
/// chapter can evict itself; the top is 1 TiB.
const CACHE_CAP_RANGE: std::ops::RangeInclusive<u64> = 100..=1_048_576;

fn cache_box(c: &CacheUsage) -> Markup {
    html! {
        div class="box" {
            h2 class="title is-5" { "Image cache" }
            p class="m-text-muted" {
                "Reader pages are fetched once and kept on disk. "
                (c.used_mb) " MiB used of " (c.cap_mb) " MiB; the least recently read pages go first."
            }
            form class="mt-3" method="post" action="/admin/image-cache" {
                div class="field has-addons" {
                    div class="control" {
                        input class="input" type="number" name="max_mb" required
                            min=(CACHE_CAP_RANGE.start()) max=(CACHE_CAP_RANGE.end())
                            value=(c.cap_mb) aria-label="Image cache limit in MiB";
                    }
                    div class="control" { span class="button is-static" { "MiB" } }
                    div class="control" { button class="button is-primary" type="submit" { "Save" } }
                }
            }
        }
    }
}

fn sources_content(rows: &[SourceRow], cache: &CacheUsage, note: Markup) -> Markup {
    html! {
        section {
            h1 class="title is-2" { "Sources" }
            (note)
            div class="box" {
                div class="table-container" {
                    table class="table is-fullwidth is-hoverable" {
                        thead {
                            tr {
                                th { "Source" }
                                th { "Status" }
                                th { "Manga" }
                                th { "Last success" }
                                th { "Last error" }
                                th { "Actions" }
                            }
                        }
                        tbody {
                            @for r in rows {
                                tr {
                                    td {
                                        strong { (r.name) }
                                        br;
                                        a class="is-size-7" href=(r.base_url) rel="noreferrer" target="_blank" { (r.base_url) }
                                    }
                                    td { (status_tag(r.enabled, &r.health)) }
                                    td { (r.health.manga_count) }
                                    td { (opt(r.health.last_success.as_deref())) }
                                    td { (error_cell(r.health.last_error.as_deref())) }
                                    td {
                                        div class="buttons are-small" {
                                            @if r.enabled {
                                                form method="post" action=(format!("/admin/sources/{}/scan", r.name)) {
                                                    button class="button is-small is-primary is-light" type="submit" { "Scan now" }
                                                }
                                            }
                                            form method="post" action=(format!("/admin/sources/{}/toggle", r.name)) {
                                                button class="button is-small is-light" type="submit" {
                                                    @if r.enabled { "Disable" } @else { "Enable" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            p class="content m-text-muted" {
                "A source is marked failing after " (FAILING_STREAK) " failed runs in a row. "
                a href="/admin/sync?failed=1" { "See failed runs" } "."
            }
            (cache_box(cache))
            a class="button is-light" href="/admin" { "Back to admin" }
        }
    }
}

fn admin_guard(user: &AuthUser) -> Option<Response> {
    require_role(user, Role::Admin)
        .err()
        .map(|_| StatusCode::FORBIDDEN.into_response())
}

/// GET `/admin/sources`.
pub async fn sources_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let (cfg, rows, cap_mb) = {
        let db = state.db.lock().await;
        let disabled = disabled_sources(&db);
        let rows: Vec<SourceRow> = source_list()
            .into_iter()
            .map(|(name, base_url)| SourceRow {
                enabled: !disabled.contains(&name),
                health: mangaread_core::db::source_health(&db, &name).unwrap_or_default(),
                name,
                base_url,
            })
            .collect();
        let cap_mb = crate::routes::images::cache_cap_mb(&db);
        (crate::config::site_config(&db), rows, cap_mb)
    };
    let cache = CacheUsage {
        used_mb: state.images.used_bytes() / (1024 * 1024),
        cap_mb,
    };
    page(
        &cfg,
        "Sources",
        sources_content(&rows, &cache, notice(&q)),
        Some(&user),
    )
    .into_response()
}

fn known_source(name: &str) -> bool {
    source_list().iter().any(|(n, _)| n == name)
}

/// POST `/admin/sources/{name}/toggle` — enable or disable a source.
pub async fn source_toggle(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(name): Path<String>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    if !known_source(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let db = state.db.lock().await;
    let mut disabled = disabled_sources(&db);
    let now_disabled = if let Some(i) = disabled.iter().position(|s| s == &name) {
        disabled.remove(i);
        false
    } else {
        disabled.push(name.clone());
        true
    };
    if mangaread_core::db::set_setting(&db, SOURCES_DISABLED_KEY, &disabled.join(","), user.id)
        .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let action = if now_disabled { "disabled" } else { "enabled" };
    let _ =
        mangaread_core::db::insert_mod_log(&db, user.id, &format!("source_{action}"), None, &name);
    Redirect::to(&format!("/admin/sources?ok={action}")).into_response()
}

/// POST `/admin/sources/{name}/scan` — scan one source now.
pub async fn source_scan(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(name): Path<String>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    if !known_source(&name) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let disabled = {
        let db = state.db.lock().await;
        disabled_sources(&db).contains(&name)
    };
    if disabled {
        return Redirect::to("/admin/sources?err=disabled_source").into_response();
    }
    if (state.spawn_job)(Job::Scan(name)) {
        Redirect::to("/admin/sources?ok=scan").into_response()
    } else {
        Redirect::to("/admin/sources?err=busy").into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct CacheForm {
    max_mb: String,
}

/// POST `/admin/image-cache` — set the image cache cap. The next cache write
/// over the new cap evicts down to it.
pub async fn image_cache_cap(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Form(form): Form<CacheForm>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let Some(mb) = form
        .max_mb
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|mb| CACHE_CAP_RANGE.contains(mb))
    else {
        return Redirect::to("/admin/sources?err=cache_cap").into_response();
    };
    let db = state.db.lock().await;
    if mangaread_core::db::set_setting(
        &db,
        crate::image_proxy::CACHE_MAX_MB_KEY,
        &mb.to_string(),
        user.id,
    )
    .is_err()
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let _ =
        mangaread_core::db::insert_mod_log(&db, user.id, "image_cache_cap", None, &mb.to_string());
    Redirect::to("/admin/sources?ok=cache").into_response()
}

// ── /admin/sync ──

fn run_target(run: &SyncRun) -> Markup {
    match (run.kind.as_str(), run.target_id) {
        ("manga", Some(id)) => html! { a href=(format!("/manga/{id}")) { "manga #" (id) } },
        ("chapter", Some(id)) => html! { "chapter #" (id) },
        _ => html! { span class="m-text-muted" { "all listings" } },
    }
}

fn run_status(run: &SyncRun) -> Markup {
    let class = match run.status.as_str() {
        "ok" => "is-success",
        "failed" => "is-danger",
        _ => "is-info",
    };
    html! { span class=(format!("tag {class} is-light")) { (run.status) } }
}

fn sync_content(
    progress: (bool, i64, i64, Option<String>),
    runs: &[SyncRun],
    failed_only: bool,
    note: Markup,
) -> Markup {
    let (running, current, total, last_finished) = progress;
    html! {
        section {
            h1 class="title is-2" { "Sync" }
            (note)
            div class="box" {
                div class="level is-mobile" {
                    div class="level-left" {
                        div {
                            @if running {
                                p { strong { "Sync running" } }
                                p class="m-text-muted" {
                                    @if total > 0 { (current) " of " (total) " manga synced" }
                                    @else { "Scanning sources…" }
                                }
                            } @else {
                                p { strong { "Idle" } }
                                p class="m-text-muted" {
                                    "Last full pass: " (last_finished.as_deref().unwrap_or("not since this server started"))
                                    ". Runs every 6 hours."
                                }
                            }
                        }
                    }
                    div class="level-right" {
                        form method="post" action="/admin/sync/run" {
                            button class="button is-primary" type="submit" disabled[running] { "Run full sync now" }
                        }
                    }
                }
            }
            div class="tabs" {
                ul {
                    li class=[(!failed_only).then_some("is-active")] { a href="/admin/sync" { "All runs" } }
                    li class=[failed_only.then_some("is-active")] { a href="/admin/sync?failed=1" { "Failed" } }
                }
            }
            @if runs.is_empty() {
                p class="m-text-muted" {
                    @if failed_only { "No failed runs in the last 30 days." } @else { "No sync runs yet." }
                }
            } @else {
                div class="box" {
                    div class="table-container" {
                        table class="table is-fullwidth is-hoverable is-narrow" {
                            thead {
                                tr {
                                    th { "Started (UTC)" }
                                    th { "Source" }
                                    th { "Job" }
                                    th { "Status" }
                                    th { "Items" }
                                    th { "Error" }
                                    th {}
                                }
                            }
                            tbody {
                                @for run in runs {
                                    tr {
                                        td class="is-size-7" { (run.started_at) }
                                        td { (run.source) }
                                        td { (run.kind) " · " (run_target(run)) }
                                        td { (run_status(run)) }
                                        td { @if let Some(n) = run.items { (n) } }
                                        td { (error_cell(run.error.as_deref())) }
                                        td {
                                            @if run.status == "failed" && run.kind != "scan" {
                                                form method="post" action=(format!("/admin/sync/retry/{}", run.id)) {
                                                    button class="button is-small is-light" type="submit" { "Retry" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            a class="button is-light" href="/admin" { "Back to admin" }
        }
    }
}

/// GET `/admin/sync` — loop status and the last 100 runs (`?failed=1` for failures).
pub async fn sync_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let failed_only = q.get("failed").is_some_and(|v| v == "1");
    let progress = {
        let p = state.auto_sync.lock().await;
        (p.running, p.current, p.total, p.last_finished.clone())
    };
    let (cfg, runs) = {
        let db = state.db.lock().await;
        (
            crate::config::site_config(&db),
            mangaread_core::db::list_sync_runs(&db, failed_only, 100).unwrap_or_default(),
        )
    };
    page(
        &cfg,
        "Sync",
        sync_content(progress, &runs, failed_only, notice(&q)),
        Some(&user),
    )
    .into_response()
}

/// POST `/admin/sync/run` — wake the loop for a full pass now.
pub async fn sync_run_now(State(state): State<Arc<AppState>>, user: AuthUser) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    // `notify_one` stores a permit when the loop is mid-pass, so the press
    // is not lost: the next sleep returns immediately.
    state.sync_wake.notify_one();
    Redirect::to("/admin/sync?ok=run").into_response()
}

/// POST `/admin/sync/retry/{id}` — rerun a failed manga or chapter run.
pub async fn sync_retry(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(id): Path<i64>,
) -> Response {
    if let Some(r) = admin_guard(&user) {
        return r;
    }
    let run = {
        let db = state.db.lock().await;
        mangaread_core::db::get_sync_run(&db, id)
    };
    let Ok(run) = run else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let job = match (run.status.as_str(), run.kind.as_str(), run.target_id) {
        ("failed", "manga", Some(t)) => Job::Manga(t),
        ("failed", "chapter", Some(t)) => Job::Chapter(t),
        _ => return Redirect::to("/admin/sync?err=not_retryable").into_response(),
    };
    if (state.spawn_job)(job) {
        Redirect::to("/admin/sync?ok=retry").into_response()
    } else {
        Redirect::to("/admin/sync?err=busy").into_response()
    }
}

#[cfg(test)]
mod tests {
    use crate::scraper_helper::Job;
    use crate::test_support::app_with_admin;
    use axum::http::StatusCode;
    use mangaread_core::db::SyncKind;

    #[tokio::test]
    async fn members_are_refused_everywhere() {
        let (app, _) = app_with_admin().await;
        let (_, member) = app.member("m@example.com").await;
        for path in ["/admin/sources", "/admin/sync"] {
            assert_eq!(
                app.get(path, Some(&member)).await.status,
                StatusCode::FORBIDDEN
            );
        }
        for path in [
            "/admin/sources/demonicscans/scan",
            "/admin/sources/demonicscans/toggle",
            "/admin/sync/run",
            "/admin/sync/retry/1",
        ] {
            let res = app.post_form(path, &[], Some(&member)).await;
            assert_eq!(res.status, StatusCode::FORBIDDEN, "{path}");
        }
        assert!(app.jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sources_page_shows_health_from_recorded_runs() {
        let (app, admin) = app_with_admin().await;
        {
            let db = app.state.db.lock().await;
            for _ in 0..3 {
                let r = mangaread_core::db::start_sync_run(&db, "manhuaplus", SyncKind::Scan, None)
                    .unwrap();
                mangaread_core::db::finish_sync_run(&db, r, Err("<b>timeout</b>")).unwrap();
            }
        }
        let page = app.get("/admin/sources", Some(&admin)).await;
        assert_eq!(page.status, StatusCode::OK);
        assert!(page.body.contains("demonicscans"));
        assert!(page.body.contains("Never run"));
        assert!(page.body.contains("Failing (3 in a row)"));
        assert!(page.body.contains("&lt;b&gt;timeout"), "errors are escaped");
    }

    #[tokio::test]
    async fn image_cache_cap_is_validated_and_shown() {
        let (app, admin) = app_with_admin().await;
        let page = app.get("/admin/sources", Some(&admin)).await;
        assert!(page.body.contains("of 5120 MiB"), "default cap");
        for bad in ["0", "99", "abc", "-5", "1048577"] {
            let res = app
                .post_form("/admin/image-cache", &[("max_mb", bad)], Some(&admin))
                .await;
            assert_eq!(
                res.location.as_deref(),
                Some("/admin/sources?err=cache_cap"),
                "{bad}"
            );
        }
        let res = app
            .post_form("/admin/image-cache", &[("max_mb", "2048")], Some(&admin))
            .await;
        assert_eq!(res.location.as_deref(), Some("/admin/sources?ok=cache"));
        let page = app.get("/admin/sources", Some(&admin)).await;
        assert!(page.body.contains("of 2048 MiB"));

        let (_, member) = app.member("m@example.com").await;
        let res = app
            .post_form("/admin/image-cache", &[("max_mb", "500")], Some(&member))
            .await;
        assert_eq!(res.status, StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn toggle_disables_and_blocks_scans_then_reenables() {
        let (app, admin) = app_with_admin().await;
        let res = app
            .post_form("/admin/sources/manhuaplus/toggle", &[], Some(&admin))
            .await;
        assert_eq!(res.location.as_deref(), Some("/admin/sources?ok=disabled"));
        assert!(
            app.get("/admin/sources", Some(&admin))
                .await
                .body
                .contains("Disabled")
        );
        let scan = app
            .post_form("/admin/sources/manhuaplus/scan", &[], Some(&admin))
            .await;
        assert_eq!(
            scan.location.as_deref(),
            Some("/admin/sources?err=disabled_source")
        );
        assert!(app.jobs.lock().unwrap().is_empty());
        app.post_form("/admin/sources/manhuaplus/toggle", &[], Some(&admin))
            .await;
        let scan = app
            .post_form("/admin/sources/manhuaplus/scan", &[], Some(&admin))
            .await;
        assert_eq!(scan.location.as_deref(), Some("/admin/sources?ok=scan"));
        assert_eq!(*app.jobs.lock().unwrap(), [Job::Scan("manhuaplus".into())]);
        let unknown = app
            .post_form("/admin/sources/nope/scan", &[], Some(&admin))
            .await;
        assert_eq!(unknown.status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn retry_reruns_failed_manga_but_not_scans_or_successes() {
        let (app, admin) = app_with_admin().await;
        let (failed_manga, failed_scan, ok_manga) = {
            let db = app.state.db.lock().await;
            let a =
                mangaread_core::db::start_sync_run(&db, "s", SyncKind::Manga, Some(42)).unwrap();
            mangaread_core::db::finish_sync_run(&db, a, Err("x")).unwrap();
            let b = mangaread_core::db::start_sync_run(&db, "s", SyncKind::Scan, None).unwrap();
            mangaread_core::db::finish_sync_run(&db, b, Err("x")).unwrap();
            let c = mangaread_core::db::start_sync_run(&db, "s", SyncKind::Manga, Some(7)).unwrap();
            mangaread_core::db::finish_sync_run(&db, c, Ok(5)).unwrap();
            (a, b, c)
        };
        let page = app.get("/admin/sync?failed=1", Some(&admin)).await;
        assert!(
            page.body
                .contains(&format!("/admin/sync/retry/{failed_manga}"))
        );
        assert!(
            !page
                .body
                .contains(&format!("/admin/sync/retry/{failed_scan}"))
        );
        let res = app
            .post_form(
                &format!("/admin/sync/retry/{failed_manga}"),
                &[],
                Some(&admin),
            )
            .await;
        assert_eq!(res.location.as_deref(), Some("/admin/sync?ok=retry"));
        for id in [failed_scan, ok_manga] {
            let res = app
                .post_form(&format!("/admin/sync/retry/{id}"), &[], Some(&admin))
                .await;
            assert_eq!(
                res.location.as_deref(),
                Some("/admin/sync?err=not_retryable")
            );
        }
        assert_eq!(*app.jobs.lock().unwrap(), [Job::Manga(42)]);
    }

    #[tokio::test]
    async fn run_now_wakes_the_loop() {
        let (app, admin) = app_with_admin().await;
        let res = app.post_form("/admin/sync/run", &[], Some(&admin)).await;
        assert_eq!(res.location.as_deref(), Some("/admin/sync?ok=run"));
        let woke = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            app.state.sync_wake.notified(),
        )
        .await;
        assert!(woke.is_ok(), "a permit was stored for the loop");
    }
}
