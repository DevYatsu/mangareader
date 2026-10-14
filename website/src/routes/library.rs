//! `/library`: a member's series, sorted onto shelves.
//!
//! Members get a server-rendered library that follows them across devices.
//! Visitors keep the browser-only list (filled by `app.js` from
//! `localStorage`); it merges into the account on sign-up.

use axum::{
    Form,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use mangaread_core::domain::Shelf;
use maud::{Markup, html};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{AppState, auth::AuthUser, templates::content};

/// GET `/library?shelf=` — shelves for members, the local list for visitors.
pub async fn library_page(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Query(q): Query<HashMap<String, String>>,
) -> Html<String> {
    let db = state.db.lock().await;
    let cfg = crate::config::site_config(&db);
    let content = match &user {
        None => visitor_list(),
        Some(u) => {
            let shelf = q
                .get("shelf")
                .and_then(|s| Shelf::parse(s))
                .unwrap_or(Shelf::Reading);
            let counts = mangaread_core::db::library_counts(&db, u.id).unwrap_or_default();
            let cards = mangaread_core::db::list_shelf(&db, u.id, shelf).unwrap_or_default();
            member_library(shelf, &counts, &cards)
        }
    };
    crate::templates::layout::page(
        &cfg,
        "Library",
        crate::templates::layout::Nav::Library,
        user.as_ref(),
        content,
    )
}

fn member_library(
    active: Shelf,
    counts: &[(Shelf, i64)],
    cards: &[mangaread_core::domain::SeriesCard],
) -> Markup {
    let total: i64 = counts.iter().map(|(_, n)| n).sum();
    let tiles: Vec<Markup> = cards
        .iter()
        .map(|c| {
            content::manga_card(
                &format!("/series/{}", c.id),
                &c.title,
                c.cover_url.as_deref(),
                None,
            )
        })
        .collect();
    html! {
        div class="level" {
            div class="level-left" { h1 class="title is-2" { "Library" } }
        }
        div class="tabs" {
            ul {
                @for (shelf, n) in counts {
                    li class=[(*shelf == active).then_some("is-active")] {
                        a href=(format!("/library?shelf={}", shelf.as_str()))
                            aria-current=[(*shelf == active).then_some("page")] {
                            (shelf.label()) " " span class="tag is-light ml-1" { (n) }
                        }
                    }
                }
            }
        }
        @if total == 0 {
            (content::empty_state("Your library is empty. Open a series and choose a shelf to add it.", true))
            div class="has-text-centered" { a class="button is-primary" href="/series" { "Browse series" } }
        } @else if tiles.is_empty() {
            (content::empty_state(&format!("Nothing on \"{}\" yet.", active.label()), false))
        } @else {
            (content::manga_grid(&tiles))
        }
    }
}

/// The pre-account list, filled client-side by `app.js`.
fn visitor_list() -> Markup {
    html! {
        div class="level" {
            div class="level-left" { h1 class="title is-2" { "Library" } }
        }
        div class="notification is-light" {
            "Saved on this device only. "
            a href="/register" { "Create an account" } " to sort series onto shelves and sync across devices."
        }
        div id="bookmarks-container" class="columns is-mobile is-multiline" {}
        (content::empty_state_with_id("Nothing saved yet.", false, "bookmarks-empty", true))
    }
}

/// The member shelf picker on the series page.
pub fn shelf_picker(series_id: i64, current: Option<Shelf>) -> Markup {
    html! {
        form class="field has-addons m-shelf-picker" method="post" action=(format!("/library/{series_id}")) {
            input type="hidden" name="next" value=(format!("/series/{series_id}"));
            div class="control" {
                div class="select is-small" {
                    select name="shelf" aria-label="Library shelf" {
                        @if current.is_none() {
                            option value="" selected disabled { "Add to library…" }
                        }
                        @for s in Shelf::ALL {
                            option value=(s.as_str()) selected[current == Some(s)] { (s.label()) }
                        }
                        @if current.is_some() {
                            option value="remove" { "Remove from library" }
                        }
                    }
                }
            }
            div class="control" {
                button class="button is-small is-primary" type="submit" {
                    @if current.is_some() { "Move" } @else { "Add" }
                }
            }
        }
    }
}

#[derive(Deserialize)]
pub struct ShelfForm {
    #[serde(default)]
    pub shelf: String,
    #[serde(default)]
    pub next: String,
}

/// Only same-site paths: `//evil.example` and `https://…` would make this an
/// open redirect.
fn safe_next(next: &str) -> &str {
    if next.starts_with('/') && !next.starts_with("//") && !next.contains('\\') {
        next
    } else {
        "/library"
    }
}

/// POST `/library/{series_id}` — `shelf` is a shelf name or `remove`.
pub async fn set_shelf(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Path(series_id): Path<i64>,
    Form(form): Form<ShelfForm>,
) -> Response {
    let db = state.db.lock().await;
    let visible = mangaread_core::db::get_series_by_id(&db, series_id).is_ok_and(|s| !s.hidden);
    if !visible {
        return StatusCode::NOT_FOUND.into_response();
    }
    let result = if form.shelf == "remove" {
        mangaread_core::db::set_bookmark(&db, user.id, series_id, false)
    } else {
        match Shelf::parse(&form.shelf) {
            Some(shelf) => mangaread_core::db::set_shelf(&db, user.id, series_id, shelf),
            None => return Redirect::to(safe_next(&form.next)).into_response(),
        }
    };
    match result {
        Ok(()) => Redirect::to(safe_next(&form.next)).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::app_with_admin;
    use axum::http::StatusCode;

    #[test]
    fn next_must_stay_on_site() {
        assert_eq!(super::safe_next("/series/3"), "/series/3");
        for evil in ["//evil.example", "https://evil.example", "/\\evil", ""] {
            assert_eq!(super::safe_next(evil), "/library", "{evil}");
        }
    }

    #[tokio::test]
    async fn members_shelve_series_and_visitors_get_the_local_list() {
        let (app, _) = app_with_admin().await;
        let (_, member) = app.member("m@example.com").await;
        let sid = {
            let db = app.state.db.lock().await;
            let sid = mangaread_core::db::insert_series(&db, "Shelved One").unwrap();
            db.execute(
                "INSERT INTO manga (source, source_id, title, detail_url, series_id) VALUES ('a','x','X','u',?1)",
                [sid],
            )
            .unwrap();
            db.execute(
                "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (last_insert_rowid(),'1',1,'u')",
                [],
            )
            .unwrap();
            sid
        };
        let anon = app.get("/library", None).await;
        assert!(anon.body.contains("bookmarks-container"));
        assert!(anon.body.contains("Saved on this device only"));

        let picker = app.get(&format!("/series/{sid}"), Some(&member)).await.body;
        assert!(picker.contains("Add to library"));

        let res = app
            .post_form(
                &format!("/library/{sid}"),
                &[("shelf", "plan"), ("next", &format!("/series/{sid}"))],
                Some(&member),
            )
            .await;
        assert_eq!(res.location, Some(format!("/series/{sid}")));
        let plan = app.get("/library?shelf=plan", Some(&member)).await.body;
        assert!(plan.contains("Shelved One"));
        let reading = app.get("/library", Some(&member)).await.body;
        assert!(!reading.contains("Shelved One"), "default tab is Reading");

        app.post_form(
            &format!("/library/{sid}"),
            &[("shelf", "remove"), ("next", "//evil")],
            Some(&member),
        )
        .await;
        let after = app.get("/library?shelf=plan", Some(&member)).await.body;
        assert!(after.contains("Your library is empty"));
        assert_eq!(
            app.get("/bookmarks", None).await.location.as_deref(),
            Some("/library")
        );
        let missing = app
            .post_form("/library/999", &[("shelf", "plan")], Some(&member))
            .await;
        assert_eq!(missing.status, StatusCode::NOT_FOUND);
    }
}
