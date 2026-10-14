use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use maud::{Markup, html};
use std::sync::Arc;

use crate::{AppState, auth::AuthUser};

/// HTML-escape a value before it is placed inside a double-quoted attribute.
///
/// `body_prefs_attrs` returns raw attribute text, and the reader layout splices
/// that text with [`maud::PreEscaped`]. Values come from normalized prefs, but
/// escaping here keeps the function safe for any caller.
pub fn attr_escape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Body attributes for the reader, derived from stored preferences.
///
/// Rendered server-side so the first paint already has the right layout —
/// applying prefs in JS would flash the default layout on every navigation.
///
/// `fit` is omitted when empty (`""` = width mode) and `frame` is omitted when
/// off, so the attributes are absent rather than empty. `bg == "custom"`
/// additionally emits the `--r-bg-custom` custom property, which is the only
/// way the user-chosen color can reach CSS.
pub fn body_prefs_attrs(prefs: &mangaread_core::ReaderPrefs) -> String {
    let mut a = String::new();
    a.push_str(&format!(
        " data-r-layout=\"{}\"",
        attr_escape(&prefs.layout)
    ));
    a.push_str(&format!(
        " data-r-preset=\"{}\"",
        preset_for_layout(&prefs.layout)
    ));
    a.push_str(&format!(
        " data-r-direction=\"{}\"",
        attr_escape(&prefs.direction)
    ));
    // Fit does not apply to `strip` (controller ruling); omitting the attribute
    // keeps first paint consistent with what `reader.js` applies on init.
    if !prefs.fit.is_empty() && prefs.layout != "strip" {
        a.push_str(&format!(" data-r-fit=\"{}\"", attr_escape(&prefs.fit)));
    }
    a.push_str(&format!(" data-r-bg=\"{}\"", attr_escape(&prefs.bg)));
    if prefs.frame {
        a.push_str(" data-r-frame=\"on\"");
    }
    if prefs.bg == "custom" {
        a.push_str(&format!(
            " style=\"--r-bg-custom:{}\"",
            attr_escape(&prefs.bg_custom)
        ));
    }
    a
}

/// Serialize prefs for the `#reader-prefs` JSON island.
///
/// `<` is escaped to `\u003c` so a keybind string containing `</script>`
/// cannot terminate the enclosing script element.
pub fn prefs_json_for_script(prefs: &mangaread_core::ReaderPrefs) -> String {
    prefs.to_json().replace('<', "\\u003c")
}

/// Serialize the first page URLs of each upcoming chapter for the Next-up
/// prefetch warm-up (contract §5.5).
///
/// Shape: `{"<chapter_id>": ["<url>", ...]}`, capped at 3 URLs per chapter.
/// `<` is escaped to `\u003c` for the same script-breakout reason as
/// [`prefs_json_for_script`].
pub fn nextup_pages_json_for_script(pairs: &[(i64, Vec<String>)]) -> String {
    let map: std::collections::HashMap<String, &Vec<String>> = pairs
        .iter()
        .map(|(id, urls)| (id.to_string(), urls))
        .collect();
    serde_json::to_string(&map)
        .unwrap_or_else(|_| "{}".into())
        .replace('<', "\\u003c")
}

/// Map the four existing layout values to the small curated preset surface.
///
/// `vertical` remains supported by the navigator, but the public surface maps
/// it to the single-page preset because it has the same page-at-a-time job.
pub fn preset_for_layout(layout: &str) -> &'static str {
    match layout {
        "double" => "spread",
        "single" | "vertical" => "page",
        _ => "flow",
    }
}

fn chapter_matches_manga(chapter: &mangaread_core::Chapter, manga_id: i64) -> bool {
    chapter.manga_id == manga_id
}

/// A compact segmented control for the two high-value secondary choices.
fn seg(pref: &str, current: &str, options: &[(&str, &str)]) -> Markup {
    let class = if pref == "bg" {
        "buttons has-addons reader-seg reader-tone-seg"
    } else {
        "buttons has-addons reader-seg"
    };
    html! {
        div class=(class) role="group"
            aria-label=(if pref == "direction" { "Reading order" } else { "Page tone" }) {
            @for (value, label) in options {
                button class="button is-small reader-seg-btn" type="button"
                    data-pref=(pref) data-value=(value)
                    disabled=[if pref == "bg" && *value == "custom" { Some("disabled") } else { None }]
                    aria-disabled=(if pref == "bg" && *value == "custom" { "true" } else { "false" })
                    title=[if pref == "bg" && *value == "custom" { Some("Custom tone is set for this series") } else { None }]
                    aria-pressed=(if *value == current { "true" } else { "false" }) {
                    (label)
                }
            }
        }
    }
}

fn preset_button(
    id: &str,
    label: &str,
    description: &str,
    visual_class: &str,
    lines: usize,
    selected: bool,
) -> Markup {
    let visual_class = format!("reader-preset-visual {}", visual_class);
    html! {
        button class="button is-small reader-preset-btn" type="button" data-r-preset=(id)
            aria-pressed=(if selected { "true" } else { "false" }) {
            span class=(visual_class) aria-hidden="true" {
                @for _ in 0..lines {
                    span {}
                }
            }
            span class="reader-preset-copy" {
                strong { (label) }
                small { (description) }
            }
        }
    }
}

/// GET `/read/{manga_id}/{chapter_id}` — render the chapter reader page.
///
/// Loads the manga, chapter, and page list from the database. If no pages
/// exist yet (unsynced chapter), triggers a background sync and displays a
/// loading state with polling-based auto-reload. Provides prev/next chapter
/// navigation and uses the reader-specific layout.
///
/// Emits the Reader 2.0 markup contract (design contract §3.2): a
/// [`mangaread_core::ReaderPrefs`] body-attribute layer, real tap-zone buttons,
/// per-page state wrappers, the settings sheet, notes, and the JSON prefs
/// island consumed by `reader.js`.
pub async fn chapter_reader(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path((manga_id, chapter_id)): Path<(i64, i64)>,
) -> Response {
    // Check if pages exist — quick peek, release lock
    let (manga, chapter, needs_sync) = {
        let db = state.db.lock().await;
        let manga = match mangaread_core::db::get_manga_by_id(&db, manga_id) {
            Ok(m) => m,
            Err(_) => {
                let cfg = crate::config::site_config(&db);
                return (
                    StatusCode::NOT_FOUND,
                    crate::templates::layout::page(
                        &cfg,
                        "Not Found",
                        crate::templates::layout::Nav::None,
                        user.as_ref(),
                        crate::templates::layout::error_content(
                            "Not Found",
                            "This manga doesn't exist or may have been removed.",
                        ),
                    ),
                )
                    .into_response();
            }
        };
        let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
            Ok(c) => c,
            Err(_) => {
                let cfg = crate::config::site_config(&db);
                return (
                    StatusCode::NOT_FOUND,
                    crate::templates::layout::page(
                        &cfg,
                        "Not Found",
                        crate::templates::layout::Nav::None,
                        user.as_ref(),
                        crate::templates::layout::error_content(
                            "Not Found",
                            "This chapter doesn't exist or may have been removed.",
                        ),
                    ),
                )
                    .into_response();
            }
        };
        if !chapter_matches_manga(&chapter, manga_id) {
            let cfg = crate::config::site_config(&db);
            return (
                StatusCode::NOT_FOUND,
                crate::templates::layout::page(
                    &cfg,
                    "Not Found",
                    crate::templates::layout::Nav::None,
                    user.as_ref(),
                    crate::templates::layout::error_content(
                        "Not Found",
                        "This chapter doesn't exist or may have been removed.",
                    ),
                ),
            )
                .into_response();
        }
        // A series hidden by an admin is unreadable for everyone else.
        let is_admin = user
            .as_ref()
            .is_some_and(|u| u.role == mangaread_core::domain::Role::Admin);
        let hidden = manga.series_id.is_some_and(|sid| {
            mangaread_core::db::get_series_by_id(&db, sid).is_ok_and(|s| s.hidden)
        });
        if hidden && !is_admin {
            let cfg = crate::config::site_config(&db);
            return (
                StatusCode::NOT_FOUND,
                crate::templates::layout::page(
                    &cfg,
                    "Not Found",
                    crate::templates::layout::Nav::None,
                    user.as_ref(),
                    crate::templates::layout::error_content(
                        "Not Found",
                        "This chapter doesn't exist or may have been removed.",
                    ),
                ),
            )
                .into_response();
        }
        let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();
        (manga, chapter, pages.is_empty())
    };

    let syncing = needs_sync;
    if needs_sync {
        (state.spawn_job)(crate::scraper_helper::Job::Chapter(chapter_id));
    }

    // Re-fetch with fresh data. The guard below is held only for synchronous
    // SQLite reads plus template construction — no `.await`, bcrypt, or network
    // I/O happens while it is held.
    let db = state.db.lock().await;
    let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();

    let back_url = if let Some(sid) = manga.series_id {
        format!("/series/{}", sid)
    } else {
        format!("/manga/{}", manga_id)
    };

    // Fetch all chapters for prev/next nav
    let all_chapters =
        mangaread_core::db::list_chapters_by_manga(&db, manga_id).unwrap_or_default();
    let current_idx = all_chapters.iter().position(|c| c.id == chapter.id);
    let prev_chapter = current_idx
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| all_chapters.get(i));
    let next_chapter = current_idx.and_then(|i| all_chapters.get(i + 1));
    let total_pages = pages.len();

    // Prefs and notes ride the same guard as the chapter list.
    let prefs = match user.as_ref() {
        Some(u) => mangaread_core::db::get_reader_prefs(&db, u.id).unwrap_or_default(),
        None => mangaread_core::ReaderPrefs::default(),
    };
    let notes = match user.as_ref() {
        Some(u) => mangaread_core::db::list_notes(&db, u.id, chapter.id).unwrap_or_default(),
        None => Vec::new(),
    };
    // Comment count for the end-of-chapter button; the list loads lazily.
    let comment_count = manga
        .series_id
        .map(|sid| {
            mangaread_core::db::count_visible_comments(&db, sid, chapter.number).unwrap_or(0)
        })
        .unwrap_or(0);

    let prev_url = prev_chapter.map(|c| format!("/read/{}/{}", manga_id, c.id));
    let next_url = next_chapter.map(|c| format!("/read/{}/{}", manga_id, c.id));

    // Up to 3 upcoming chapters for the Next-up queue.
    let upcoming: Vec<&mangaread_core::Chapter> = current_idx
        .map(|i| all_chapters.iter().skip(i + 1).take(3).collect())
        .unwrap_or_default();

    // First 3 page URLs per upcoming chapter, so the Next-up prefetch button
    // can actually warm them (contract §5.5). The same guard covers it.
    let nextup_pages: Vec<(i64, Vec<String>)> = upcoming
        .iter()
        .map(|c| {
            let urls = mangaread_core::db::list_pages_by_chapter(&db, c.id)
                .unwrap_or_default()
                .into_iter()
                .take(3)
                .map(|p| p.image_url)
                .collect::<Vec<_>>();
            (c.id, urls)
        })
        .collect();

    let reload_script = if syncing {
        Some(format!(
            r#"<script>
(function(){{
let a=0;
const iv=setInterval(()=>{{
fetch('/api/check-chapter/{}').then(r=>r.json()).then(d=>{{if(d.pages>0){{clearInterval(iv);location.reload()}}}}).catch(()=>{{}});
if(++a>=20)clearInterval(iv)
}},1500)
}})()
</script>"#,
            chapter_id
        ))
    } else {
        None
    };

    let selected_preset = preset_for_layout(&prefs.layout);
    let flow_preset = preset_button(
        "flow",
        "Flow",
        "Long, seamless scroll",
        "reader-preset-visual-flow",
        3,
        selected_preset == "flow",
    );
    let page_preset = preset_button(
        "page",
        "Page",
        "One page at a time",
        "reader-preset-visual-page",
        1,
        selected_preset == "page",
    );
    let spread_preset = preset_button(
        "spread",
        "Spread",
        "Book-like pairs",
        "reader-preset-visual-spread",
        2,
        selected_preset == "spread",
    );
    let direction_control = seg(
        "direction",
        &prefs.direction,
        &[("ltr", "Left to right"), ("rtl", "Right to left")],
    );
    // Always render Custom so a device-local per-series `bg=custom` override
    // has a selected, non-editable control before JS resolves preferences.
    let tone_options = vec![
        ("black", "Dark"),
        ("sepia", "Paper"),
        ("white", "Light"),
        ("custom", "Custom"),
    ];
    let tone_control = seg("bg", &prefs.bg, &tone_options);

    let content = html! {
        @if let Some(script) = reload_script {
            (maud::PreEscaped(script))
        }

        // Tracking element (invisible)
        div id="reader-track" style="display:none"
            data-series-id=(manga.series_id.unwrap_or(0))
            data-chapter-id=(chapter.id)
            data-manga-id=(manga_id)
            data-total-pages=(total_pages)
            data-prev-chapter=[prev_url.as_deref()]
            data-next-chapter=[next_url.as_deref()]
            data-auto-next=[if prefs.auto_next { Some("1") } else { None }]
            data-auto-prev=[if prefs.auto_prev { Some("1") } else { None }]
            data-prime-idx="0" {}
        div id="resume-target" style="display:none" data-page=(0) {}

        // ── Top Navigation Bar (hidden by default, shows on scroll-up) ──
        header id="reader-topbar" class="navbar reader-topbar" {
            div class="navbar-inner reader-topbar-inner" {
                a class="button is-ghost is-small reader-back-btn" href=(back_url) aria-label="Back to series" {
                    svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                        path d="M19 12H5m7-7-7 7 7 7" {}
                    }
                }
                div class="reader-topbar-info" {
                    span class="reader-topbar-title" { (manga.title) }
                    span class="reader-topbar-chapter" { "Ch. " (chapter.number) }
                }
                span id="reader-page-indicator" class="reader-page-indicator" aria-live="polite" {
                    "1 / " (total_pages)
                }
                button id="reader-settings-btn" class="button is-ghost is-small reader-icon-btn" type="button"
                    aria-expanded="false" aria-controls="reader-sheet"
                    aria-label="Reader settings" { "⚙" }
                button id="reader-focus-btn" class="button is-ghost is-small reader-icon-btn" type="button"
                    aria-label="Toggle focus mode" aria-pressed="false" data-focus-toggle="true" { "◱" }
            }
        }

        // ── Floating page counter, shown while scrolling (strip) ──
        // Interactive on strip, where it replaces the center tap zone (§4.1):
        // a long-press opens the settings sheet, so it is exposed as a button
        // rather than hidden from assistive tech.
        div id="reader-hud" class="tag reader-hud" role="button" tabindex="0"
            aria-label="Reader menu" {
            "1 / " (total_pages)
        }
        button id="reader-focus-exit" class="button is-primary is-small reader-focus-exit" type="button"
            aria-label="Exit focus mode" aria-pressed="true" data-focus-toggle="true" {
            "Exit focus"
        }

        // ── Resume affordance (revealed by JS when local progress > page 1) ──
        span id="reader-resume-badge" class="reader-resume-badge" data-page="1" hidden {
            "Resume p. 1"
        }

        // ── Loading state ──
        @if syncing {
            // Contract §7 keeps `.loading-state > .skeleton.skeleton-page`;
            // `reader-loading` is the style hook the reader stylesheet keys on.
            div class="loading-state reader-loading" {
                div class="skeleton skeleton-page" {}
                div class="skeleton skeleton-page" style="aspect-ratio:3/5" {}
                div class="skeleton skeleton-page" style="aspect-ratio:3/4" {}
            }
        } @else if total_pages == 0 {
            // ── Empty chapter (contract §7: `.reader-empty`, no id) ──
            section class="reader-empty" role="status" {
                p { "This chapter has no pages yet." }
                a class="button is-small reader-nav-btn" href=(back_url) { "Back to series" }
            }
        } @else {
            // Tap zones for page navigation (buttons: focusable, real semantics)
            div id="reader-zones" data-r-zone-mode="three" {
                button class="reader-zone" type="button" data-zone="prev" aria-label="Previous page" {}
                button class="reader-zone" type="button" data-zone="menu" aria-label="Reader menu" {}
                button class="reader-zone" type="button" data-zone="next" aria-label="Next page" {}
            }

            // Page images — full width, stacked
            main id="reader-images" class="reader-images" {
                @for (i, page) in pages.iter().enumerate() {
                    @let load = if i == 0 { "eager" } else { "lazy" };
                    section class="reader-page" data-page-idx=(i + 1) data-page-state="idle" {
                        div class="reader-page-placeholder" aria-hidden="true" {}
                        img src=(page.image_url) alt={"Page "(i + 1)} loading=(load) decoding="async";
                        div class="reader-page-error" hidden {
                            p { "Could not load this page." }
                            button class="button is-primary is-small reader-retry-btn" type="button" { "Retry" }
                        }
                        @let page_notes: Vec<_> = notes.iter().filter(|n| n.page == (i as i64 + 1)).collect();
                        @if !page_notes.is_empty() {
                            button class="tag is-primary is-light reader-note-pin" type="button"
                                data-note-id=(page_notes[0].id)
                                data-count=(page_notes.len())
                                data-page-idx=(i + 1)
                                aria-label={"Note on page "(i + 1)} { (page_notes.len()) }
                        }
                    }
                }
            }

            // ── Bottom Navigation (after all pages) ──
            div id="reader-bottom-nav" class="reader-bottom-nav" {
                div class="level reader-bottom-nav-inner" {
                    @if let Some(prev) = prev_chapter {
                        a href={"/read/"(manga_id)"/"(prev.id)} class="button is-small reader-nav-btn" {
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M19 12H5m7-7-7 7 7 7" {}
                            }
                            "Ch. " (prev.number)
                        }
                    } @else {
                        span class="button is-small reader-nav-btn disabled" {
                            "First Chapter"
                        }
                    }
                    a href=(back_url) class="button is-small reader-nav-btn secondary" { "Series Page" }
                    @if let Some(next) = next_chapter {
                        a href={"/read/"(manga_id)"/"(next.id)} class="button is-small reader-nav-btn" {
                            "Ch. " (next.number)
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M5 12h14m-7-7 7 7-7 7" {}
                            }
                        }
                    } @else {
                        span class="button is-small reader-nav-btn disabled" {
                            "Last Chapter"
                        }
                    }
                }
            }

            // Subtle hint that fades out (existing element; kept per the
            // contract's "existing elements keep their ids and classes").
            div id="reader-hint" class="reader-hint" {
                @if prefs.layout == "strip" { "Scroll to read" } @else { "Tap sides to navigate pages" }
            }

            // ── End-of-chapter overlay for paged layouts ──
            div id="reader-chapter-nav" class="reader-chapter-nav" hidden {
                div class="level reader-bottom-nav-inner" {
                    @if let Some(prev) = prev_chapter {
                        a href={"/read/"(manga_id)"/"(prev.id)} class="button is-small reader-nav-btn" {
                            "Ch. " (prev.number)
                        }
                    } @else {
                        span class="button is-small reader-nav-btn disabled" { "First Chapter" }
                    }
                    a href=(back_url) class="button is-small reader-nav-btn secondary" { "Series Page" }
                    @if manga.series_id.is_some() {
                        button class="button is-small reader-nav-btn secondary" type="button" data-comments-open {
                            "Comments (" span data-comments-count { (comment_count) } ")"
                        }
                    }
                    @if let Some(next) = next_chapter {
                        a href={"/read/"(manga_id)"/"(next.id)} class="button is-small reader-nav-btn" {
                            "Ch. " (next.number)
                        }
                        button class="button is-small reader-nav-btn" type="button" data-action="next" {
                            "Next up: Ch. " (next.number)
                        }
                    } @else {
                        span class="button is-small reader-nav-btn disabled" { "Last Chapter" }
                    }
                }
            }
        }

        @if next_url.is_some() {
            // ── Next-up queue ──
            // `.reader-nextup-item` is a layout row, not the link: nesting a
            // button inside an anchor is invalid, and §5.5 asks for a
            // prefetch button "per item".
            div id="reader-nextup" class="box reader-nextup" {
                @for c in &upcoming {
                    div class="level reader-nextup-item" data-chapter-id=(c.id) {
                        a href={"/read/"(manga_id)"/"(c.id)} { "Next: Ch. " (c.number) }
                        button class="button is-small is-ghost reader-nextup-prefetch" type="button"
                            data-prefetch-state="idle" aria-label="Warm up chapter" {}
                    }
                }
            }
        }

        // ── Chapter comments ──
        // Inline below the chapter in `strip`; a bottom sheet above the tap
        // zones in paged layouts, opened from the end-of-chapter overlay.
        // `comments.js` fills the list lazily from `/api/comments`.
        @if let Some(series_id) = manga.series_id {
            section id="chapter-comments" class="reader-comments" aria-labelledby="chapter-comments-title"
                data-series-id=(series_id) data-chapter-number=(chapter.number)
                data-logged-in=(if user.is_some() { "1" } else { "0" }) {
                div class="reader-comments-inner" {
                    div class="reader-comments-head" {
                        h2 id="chapter-comments-title" class="title is-5" {
                            "Comments " span class="tag is-light" data-comments-count { (comment_count) }
                        }
                        button class="delete reader-comments-close" type="button" aria-label="Close comments" {}
                    }
                    @if user.is_some() {
                        form class="reader-comment-form" data-comment-form {
                            textarea class="textarea" name="body" rows="3" maxlength=(crate::routes::comments::MAX_COMMENT_CHARS)
                                placeholder="Share your thoughts on this chapter" aria-label="Write a comment" required {}
                            div class="level is-mobile mt-2" {
                                label class="checkbox is-size-7" {
                                    input type="checkbox" name="spoiler";
                                    " Contains spoilers"
                                }
                                button class="button is-primary is-small" type="submit" { "Post" }
                            }
                        }
                    } @else {
                        p class="reader-comments-login" {
                            a href="/login" { "Log in" } " or " a href="/register" { "sign up" } " to join the discussion."
                        }
                    }
                    p class="reader-comments-status is-size-7" role="status" aria-live="polite" {}
                    ol class="reader-comments-list" {}
                    button class="button is-small is-fullwidth reader-comments-more" type="button" hidden { "Load older comments" }
                }
            }
        }

        // ── Auto-next countdown ──
        div id="reader-countdown" class="notification reader-countdown" role="status" aria-live="polite" hidden {
            "Next chapter in "
            span class="reader-countdown-secs" { "3" }
            button class="button is-small is-light" type="button" { "Cancel" }
        }

        // ── Idle dim overlay ──
        div id="reader-dim" class="reader-dim" aria-hidden="true" {}

        // ── Local-only persistence notice ──
        div id="reader-offline" class="notification is-warning reader-offline" role="status" hidden {
            "Saved on this device only."
        }

        // ── Note composer (bottom sheet, keeps the target page visible) ──
        div id="reader-note-composer" class="box reader-note-composer" hidden {
            form id="reader-note-form" class="reader-note-form" {
                textarea id="reader-note-body" class="textarea reader-note-body"
                    required maxlength="2000" aria-label="Note" {}
                input id="reader-note-page" type="hidden" value="1";
                button class="button is-primary reader-note-save" type="submit" { "Save note" }
                button class="button reader-note-cancel" type="button" data-r-close { "Cancel" }
                p class="reader-note-error" role="alert" hidden {}
            }
        }

        // ── Notes list ──
        aside id="reader-notes-panel" class="panel reader-notes-panel" hidden {
            header class="panel-heading" {
                h2 { "Notes — Ch. " (chapter.number) }
                span class="reader-notes-count" { (notes.len()) }
                button class="delete reader-notes-close" type="button" data-r-close aria-label="Close notes" { "✕" }
            }
            @if notes.is_empty() {
                p class="reader-notes-empty" { "No notes in this chapter." }
            } @else {
                ul class="reader-notes-list" {
                    @for n in &notes {
                        li class="reader-note-item" data-note-id=(n.id) data-page-idx=(n.page) {
                            button class="button is-small reader-note-jump" type="button" { "p. " (n.page) }
                            // SQLite stores "YYYY-MM-DD HH:MM:SS"; `datetime`
                            // wants ISO 8601, so the space becomes `T`.
                            time class="reader-note-time" datetime=(n.created_at.replace(' ', "T")) {
                                (n.created_at)
                            }
                            p class="reader-note-text" { (n.body) }
                        }
                    }
                }
            }
        }

        // ── Reading settings: three curated modes, not a settings framework ──
        div id="reader-sheet" class="reader-sheet" hidden {
            div class="reader-sheet-backdrop" data-r-close {}
            div class="box reader-sheet-panel" data-r-sheet="rail" role="dialog" aria-modal="true" aria-labelledby="reader-sheet-title" {
                header class="reader-sheet-header" {
                    div {
                        p { "Make the page feel right" }
                        h2 id="reader-sheet-title" { "Reading style" }
                    }
                    button class="reader-sheet-close" type="button" data-r-close aria-label="Close reading style" { "✕" }
                }

                label class="checkbox reader-series-remember" {
                    input id="reader-remember-series" type="checkbox" {}
                    span {
                        strong { "Keep for this series" }
                        small class="reader-series-remember-note" data-series-title=(manga.title) { (manga.title) }
                    }
                }

                div class="reader-sheet-group" data-group="reading" role="group" aria-label="Reading style" {
                    div class="reader-setting-intro" {
                        h3 id="reader-preset-title" { "Choose a pace" }
                        p { "Flow is recommended. The reader handles the rest." }
                    }

                    div class="reader-preset-grid" {
                        (flow_preset)
                        (page_preset)
                        (spread_preset)
                    }
                    p id="reader-spread-note" class="reader-preset-note" hidden {
                        "Spread needs a screen at least 768px wide."
                    }

                    div class="reader-compact-setting" {
                        span { "Reading order" }
                        (direction_control)
                    }
                    div class="reader-compact-setting" {
                        span { "Page tone" }
                        (tone_control)
                    }
                }

                div class="reader-utility-row" role="group" aria-label="Reader utilities" {
                    button id="reader-focus-toggle" class="button is-small reader-utility-btn" type="button"
                        data-action="ui.focus" data-focus-toggle="true" aria-pressed="false" {
                        "Focus mode"
                    }
                    button class="button is-small reader-utility-btn" type="button" data-action="note.add" {
                        "Add note"
                    }
                    button class="button is-small reader-utility-btn" type="button"
                        data-action="ui.chapterNav" {
                        "Go to"
                    }
                }

                div class="reader-sheet-footer" role="group" aria-label="Reading preference scope" {
                    button class="button is-small reader-restore-btn" type="button" data-action="ui.restoreDefaults" {
                        "Restore recommended settings"
                    }
                    p id="reader-settings-status" class="reader-settings-status" role="status" aria-live="polite" {}
                }

            }
        }

        // ── Prefs blob consumed by reader.js ──
        script type="application/json" id="reader-prefs" {
            (maud::PreEscaped(prefs_json_for_script(&prefs)))
        }

        // ── First page URLs per upcoming chapter, for the Next-up warm-up ──
        script type="application/json" id="reader-nextup-pages" {
            (maud::PreEscaped(nextup_pages_json_for_script(&nextup_pages)))
        }
    };

    let cfg = crate::config::site_config(&db);
    let css = cfg.theme_overrides();
    Html(
        crate::templates::layout::reader_layout(
            &format!("Ch. {} — {}", chapter.number, manga.title),
            content,
            &css,
            &cfg.site_title,
            user.as_ref(),
            &prefs,
        )
        .0,
    )
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mangaread_core::ReaderPrefs;

    #[test]
    fn body_attrs_reflect_prefs() {
        let mut p = ReaderPrefs::default();
        p.layout = "double".into();
        p.direction = "rtl".into();
        p.fit = "height".into();
        p.bg = "white".into();
        p.frame = true;
        let a = body_prefs_attrs(&p);
        assert!(a.contains(r#"data-r-layout="double""#), "{a}");
        assert!(a.contains(r#"data-r-preset="spread""#), "{a}");
        assert!(a.contains(r#"data-r-direction="rtl""#), "{a}");
        assert!(a.contains(r#"data-r-fit="height""#), "{a}");
        assert!(a.contains(r#"data-r-bg="white""#), "{a}");
        assert!(a.contains(r#"data-r-frame="on""#), "{a}");
    }

    #[test]
    fn curated_markup_hooks_remain_explicit() {
        let markup = include_str!("reader.rs");
        for hook in [
            "data-r-preset=(id)",
            "\"flow\"",
            "\"page\"",
            "\"spread\"",
            "data-r-sheet=\"rail\"",
            "(\"custom\", \"Custom\")",
            "reader-focus-toggle",
            "data-action=\"note.add\"",
            "data-action=\"ui.chapterNav\"",
            "navbar reader-topbar",
        ] {
            assert!(markup.contains(hook), "missing curated reader hook: {hook}");
        }
        for dead_editor in [
            ["reader-", "zone-editor"].concat(),
            ["reader-", "keybind-row"].concat(),
            ["reader-", "slider"].concat(),
            ["reader-", "color"].concat(),
        ] {
            assert!(
                !markup.contains(&dead_editor),
                "advanced editor returned: {dead_editor}"
            );
        }
    }

    #[test]
    fn curated_presets_cover_every_existing_layout() {
        assert_eq!(preset_for_layout("strip"), "flow");
        assert_eq!(preset_for_layout("single"), "page");
        assert_eq!(preset_for_layout("vertical"), "page");
        assert_eq!(preset_for_layout("double"), "spread");
        assert_eq!(preset_for_layout("future-layout"), "flow");

        let a = body_prefs_attrs(&ReaderPrefs::default());
        assert!(a.contains(r#"data-r-preset="flow""#), "{a}");
    }

    #[test]
    fn reader_rejects_chapter_from_another_manga() {
        let chapter = mangaread_core::Chapter {
            id: 9,
            manga_id: 4,
            source_id: "chapter-9".into(),
            number: 1.0,
            title: None,
            url: "/chapter/9".into(),
            created_at: "2026-09-25 00:00:00".into(),
        };
        assert!(chapter_matches_manga(&chapter, 4));
        assert!(!chapter_matches_manga(&chapter, 5));
    }

    #[test]
    fn body_attrs_omit_empty_values() {
        let p = ReaderPrefs::default(); // fit == "" and frame == false
        let a = body_prefs_attrs(&p);
        assert!(!a.contains("data-r-fit=\"\""), "{a}");
        assert!(!a.contains("data-r-frame"), "{a}");
    }

    /// Fit does not apply to `strip` (controller ruling), so a stored fit must
    /// not reach the first paint there — `reader.js` would remove it on init
    /// anyway, and emitting it risks a flash of viewport-capped strip pages.
    #[test]
    fn body_attrs_omit_fit_on_strip() {
        let mut p = ReaderPrefs::default(); // layout == "strip"
        p.fit = "height".into();
        let a = body_prefs_attrs(&p);
        assert!(a.contains(r#"data-r-layout="strip""#), "{a}");
        assert!(!a.contains("data-r-fit"), "{a}");
    }

    #[test]
    fn custom_bg_emits_the_custom_property() {
        let mut p = ReaderPrefs::default();
        p.bg = "custom".into();
        p.bg_custom = "#123456".into();
        let a = body_prefs_attrs(&p);
        assert!(a.contains("--r-bg-custom:#123456"), "{a}");
    }

    /// A keybind can contain arbitrary user text; the JSON island must not be
    /// terminable by `</script>`.
    #[test]
    fn prefs_json_escapes_script_close() {
        let mut p = ReaderPrefs::default();
        p.keybinds.insert(
            "page.next".into(),
            vec!["</script><script>alert(1)</script>".into()],
        );
        let json = prefs_json_for_script(&p);
        assert!(!json.contains("</script>"), "{json}");
        assert!(json.contains(r"\u003c/script"), "{json}");
    }

    /// Regression guard: the reader layout must place the prefs attributes
    /// *inside* the `<body>` opening tag (maud 0.27 cannot splice bare
    /// attributes, so the tag is built with `PreEscaped`).
    #[test]
    fn reader_layout_body_carries_prefs_attrs() {
        let p = ReaderPrefs::default();
        let out = crate::templates::layout::reader_layout("t", maud::html! {}, "", "S", None, &p)
            .into_string();
        assert!(out.contains("data-r-layout=\"strip\""), "{out}");
        assert!(out.contains("/static/reader.css"), "{out}");
        assert!(out.contains("/static/reader.js"), "{out}");
        let body_start = out.find("<body").expect("body tag");
        let body_end = out[body_start..].find('>').expect("tag end") + body_start;
        assert!(out[body_start..body_end].contains("data-r-layout"), "{out}");
    }

    /// The Next-up prefetch island maps chapter ids to their first page URLs,
    /// and must not be terminable by `</script>` from page data.
    #[test]
    fn nextup_pages_json_shape_and_escaping() {
        let pairs = vec![
            (12i64, vec!["/a.svg".to_string(), "/b.svg".to_string()]),
            (
                13i64,
                vec!["</script><script>alert(1)</script>".to_string()],
            ),
        ];
        let json = nextup_pages_json_for_script(&pairs);
        assert!(json.contains(r#""12":["/a.svg","/b.svg"]"#), "{json}");
        assert!(!json.contains("</script>"), "{json}");
        assert!(json.contains(r"\u003c/script"), "{json}");
        // Empty input degrades to an empty object, never invalid JSON.
        assert_eq!(nextup_pages_json_for_script(&[]), "{}");
    }

    /// `?` is the one default binding stored as a `KeyboardEvent.key` value
    /// rather than a code (§3.1 lists it alongside `KeyS`).
    #[test]
    fn ui_toggle_defaults_include_question_mark() {
        let binding = mangaread_core::DEFAULT_KEYBINDS
            .iter()
            .find(|(a, _)| *a == "ui.toggle")
            .map(|(_, keys)| *keys)
            .expect("ui.toggle default");
        assert!(binding.contains(&"KeyS"), "{binding:?}");
        assert!(binding.contains(&"?"), "{binding:?}");
    }
}
