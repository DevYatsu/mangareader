use axum::extract::{Path, State};
use axum::response::Html;
use maud::{html, Markup};
use std::sync::Arc;

use crate::{auth::AuthUser, AppState};

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
    a.push_str(&format!(" data-r-layout=\"{}\"", attr_escape(&prefs.layout)));
    a.push_str(&format!(" data-r-direction=\"{}\"", attr_escape(&prefs.direction)));
    if !prefs.fit.is_empty() {
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

/// A segmented control: `.reader-seg` with one `.reader-seg-btn` per option.
///
/// Every button carries `data-pref`, the single binding point between CSS, JS,
/// and the prefs blob (design contract §3.2).
fn seg(pref: &str, current: &str, options: &[(&str, &str)]) -> Markup {
    html! {
        div class="reader-seg" {
            @for (value, label) in options {
                button class="reader-seg-btn" type="button"
                    data-pref=(pref) data-value=(value)
                    aria-pressed=(if *value == current { "true" } else { "false" }) {
                    (label)
                }
            }
        }
    }
}

/// A two-state segmented control (`off`/`on`) bound to a boolean prefs key.
///
/// `on_id` optionally labels the `on` half, which is how §6.2's
/// `#reader-notes-toggle` hook is satisfied while keeping the `data-pref`
/// binding uniform.
fn toggle_with_id(
    pref: &str,
    on: bool,
    off_label: &str,
    on_label: &str,
    on_id: Option<&str>,
) -> Markup {
    html! {
        div class="reader-seg" {
            button class="reader-seg-btn" type="button"
                data-pref=(pref) data-value="off"
                aria-pressed=(if on { "false" } else { "true" }) { (off_label) }
            button class="reader-seg-btn" type="button"
                id=[on_id] data-pref=(pref) data-value="on"
                aria-pressed=(if on { "true" } else { "false" }) { (on_label) }
        }
    }
}

fn toggle(pref: &str, on: bool, off_label: &str, on_label: &str) -> Markup {
    toggle_with_id(pref, on, off_label, on_label, None)
}

/// A labelled range input with a live `output.reader-slider-value` sibling.
///
/// The row is a bare `div` + `span`: Task 5's CSS styles `.reader-sheet-group >
/// div` and `> span:first-child` by element, so no row class is invented.
fn slider(
    pref: &str,
    label: &str,
    min: f64,
    max: f64,
    step: f64,
    value: String,
    unit: &str,
) -> Markup {
    let id = format!("reader-slider-{pref}");
    html! {
        div {
            span { (label) }
            input id=(id) class="reader-slider" type="range" data-pref=(pref)
                aria-label=(label) min=(min) max=(max) step=(step) value=(value);
            output class="reader-slider-value" for=(id) { (value) (unit) }
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
) -> Html<String> {
    // Check if pages exist — quick peek, release lock
    let (manga, chapter, needs_sync) = {
        let db = state.db.lock().await;
        let manga = match mangaread_core::db::get_manga_by_id(&db, manga_id) {
            Ok(m) => m,
            Err(_) => {
                return Html(crate::templates::layout::error_layout(
                    "Not Found",
                    "This manga doesn't exist or may have been removed.",
                    &crate::config::resolve_theme(&db, user.as_ref()),
                ).0);
            }
        };
        let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
            Ok(c) => c,
            Err(_) => {
                return Html(crate::templates::layout::error_layout(
                    "Not Found",
                    "This chapter doesn't exist or may have been removed.",
                    &crate::config::resolve_theme(&db, user.as_ref()),
                ).0);
            }
        };
        let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();
        (manga, chapter, pages.is_empty())
    };

    let syncing = needs_sync;
    if needs_sync {
        crate::scraper_helper::sync_chapter_background(chapter_id);
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
    let prev_chapter = current_idx.and_then(|i| i.checked_sub(1)).and_then(|i| all_chapters.get(i));
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

    // Keybind rows in a stable order (HashMap iteration is not deterministic).
    let keybind_rows: Vec<(&str, Vec<String>)> = mangaread_core::ACTION_IDS
        .iter()
        .map(|action| {
            (
                *action,
                prefs.keybinds.get(*action).cloned().unwrap_or_default(),
            )
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
        header id="reader-topbar" class="reader-topbar" {
            div class="reader-topbar-inner" {
                a class="reader-back-btn" href=(back_url) aria-label="Back to series" {
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
                button id="reader-settings-btn" class="reader-icon-btn" type="button"
                    aria-expanded="false" aria-controls="reader-sheet"
                    aria-label="Reader settings" { "⚙" }
                button id="reader-focus-btn" class="reader-icon-btn" type="button"
                    aria-label="Toggle focus mode" { "◱" }
            }
        }

        // ── Floating page counter, shown while scrolling (strip) ──
        // Interactive on strip, where it replaces the center tap zone (§4.1):
        // a long-press opens the settings sheet, so it is exposed as a button
        // rather than hidden from assistive tech.
        div id="reader-hud" class="reader-hud" role="button" tabindex="0"
            aria-label="Reader menu" {
            "1 / " (total_pages)
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
                a class="reader-nav-btn" href=(back_url) { "Back to series" }
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
                            button class="reader-retry-btn" type="button" { "Retry" }
                        }
                        @let page_notes: Vec<_> = notes.iter().filter(|n| n.page == (i as i64 + 1)).collect();
                        @if !page_notes.is_empty() {
                            button class="reader-note-pin" type="button"
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
                div class="reader-bottom-nav-inner" {
                    @if let Some(prev) = prev_chapter {
                        a href={"/read/"(manga_id)"/"(prev.id)} class="reader-nav-btn" {
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M19 12H5m7-7-7 7 7 7" {}
                            }
                            "Ch. " (prev.number)
                        }
                    } @else {
                        span class="reader-nav-btn disabled" {
                            "First Chapter"
                        }
                    }
                    a href=(back_url) class="reader-nav-btn secondary" { "Series Page" }
                    @if let Some(next) = next_chapter {
                        a href={"/read/"(manga_id)"/"(next.id)} class="reader-nav-btn" {
                            "Ch. " (next.number)
                            svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
                                path d="M5 12h14m-7-7 7 7-7 7" {}
                            }
                        }
                    } @else {
                        span class="reader-nav-btn disabled" {
                            "Last Chapter"
                        }
                    }
                }
            }

            // Subtle hint that fades out (existing element; kept per the
            // contract's "existing elements keep their ids and classes").
            div id="reader-hint" class="reader-hint" { "Tap sides to navigate pages" }

            // ── End-of-chapter overlay for paged layouts ──
            div id="reader-chapter-nav" class="reader-chapter-nav" hidden {
                div class="reader-bottom-nav-inner" {
                    @if let Some(prev) = prev_chapter {
                        a href={"/read/"(manga_id)"/"(prev.id)} class="reader-nav-btn" {
                            "Ch. " (prev.number)
                        }
                    } @else {
                        span class="reader-nav-btn disabled" { "First Chapter" }
                    }
                    a href=(back_url) class="reader-nav-btn secondary" { "Series Page" }
                    @if let Some(next) = next_chapter {
                        a href={"/read/"(manga_id)"/"(next.id)} class="reader-nav-btn" {
                            "Ch. " (next.number)
                        }
                        button class="reader-nav-btn" type="button" data-action="next" {
                            "Next up: Ch. " (next.number)
                        }
                    } @else {
                        span class="reader-nav-btn disabled" { "Last Chapter" }
                    }
                }
            }
        }

        @if next_url.is_some() {
            // ── Next-up queue ──
            // `.reader-nextup-item` is a layout row, not the link: nesting a
            // button inside an anchor is invalid, and §5.5 asks for a
            // prefetch button "per item".
            div id="reader-nextup" class="reader-nextup" {
                @for c in &upcoming {
                    div class="reader-nextup-item" data-chapter-id=(c.id) {
                        a href={"/read/"(manga_id)"/"(c.id)} { "Next: Ch. " (c.number) }
                        button class="reader-nextup-prefetch" type="button"
                            data-prefetch-state="idle" aria-label="Warm up chapter" {}
                    }
                }
            }
        }

        // ── Auto-next countdown ──
        div id="reader-countdown" class="reader-countdown" role="status" aria-live="polite" hidden {
            "Next chapter in "
            span class="reader-countdown-secs" { "3" }
            button type="button" { "Cancel" }
        }

        // ── Idle dim overlay ──
        div id="reader-dim" class="reader-dim" aria-hidden="true" {}

        // ── Local-only persistence notice ──
        div id="reader-offline" class="reader-offline" role="status" hidden {
            "Saved on this device only."
        }

        // ── Note composer (bottom sheet, keeps the target page visible) ──
        div id="reader-note-composer" class="reader-note-composer" hidden {
            form id="reader-note-form" class="reader-note-form" {
                textarea id="reader-note-body" class="reader-note-body"
                    required maxlength="2000" aria-label="Note" {}
                input id="reader-note-page" type="hidden" value="1";
                button class="reader-note-save" type="submit" { "Save note" }
                button class="reader-note-cancel" type="button" data-r-close { "Cancel" }
                p class="reader-note-error" role="alert" hidden {}
            }
        }

        // ── Notes list ──
        aside id="reader-notes-panel" class="reader-notes-panel" hidden {
            header {
                h2 { "Notes — Ch. " (chapter.number) }
                span class="reader-notes-count" { (notes.len()) }
                button class="reader-notes-close" type="button" data-r-close aria-label="Close notes" { "✕" }
            }
            @if notes.is_empty() {
                p class="reader-notes-empty" { "No notes in this chapter." }
            } @else {
                ul class="reader-notes-list" {
                    @for n in &notes {
                        li class="reader-note-item" data-note-id=(n.id) data-page-idx=(n.page) {
                            button class="reader-note-jump" type="button" { "p. " (n.page) }
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

        // ── Settings sheet ──
        div id="reader-sheet" class="reader-sheet" hidden {
            div class="reader-sheet-backdrop" data-r-close {}
            div class="reader-sheet-panel" role="dialog" aria-modal="true" aria-labelledby="reader-sheet-title" {
                h2 id="reader-sheet-title" { "Reader settings" }

                // Per-series override sits above the tabs so it applies to
                // whatever group follows (design contract §3.4).
                label class="reader-series-remember" {
                    input id="reader-remember-series" type="checkbox" {}
                    "Remember for this series"
                }

                div class="reader-sheet-tabs" role="tablist" {
                    @for (g, label) in [("layout", "Layout"), ("fit", "Fit"), ("input", "Input"), ("comfort", "Comfort"), ("thisSeries", "This series")] {
                        button class="reader-sheet-tab" type="button" role="tab"
                            data-group=(g) aria-selected=(if g == "layout" { "true" } else { "false" }) {
                            (label)
                        }
                    }
                }

                // Group: layout
                section class="reader-sheet-group" data-group="layout" {
                    h3 { "Layout" }
                    div {
                        span { "Arrangement" }
                        (seg("layout", &prefs.layout, &[
                            ("single", "Single"),
                            ("double", "Double"),
                            ("strip", "Strip"),
                            ("vertical", "Vertical"),
                        ]))
                    }
                    div {
                        span { "Direction" }
                        (seg("direction", &prefs.direction, &[
                            ("ltr", "Left to right"),
                            ("rtl", "Right to left"),
                        ]))
                    }
                    div {
                        span { "Cover page alone" }
                        (toggle("doubleCover", prefs.double_cover, "Off", "On"))
                    }
                }

                // Group: fit
                section class="reader-sheet-group" data-group="fit" {
                    h3 { "Fit" }
                    div {
                        span { "Fit mode" }
                        (seg("fit", &prefs.fit, &[
                            ("", "Width"),
                            ("height", "Height"),
                            ("screen", "Screen"),
                        ]))
                    }
                    (slider("maxW", "Width", 400.0, 1400.0, 10.0, prefs.max_w.to_string(), "px"))
                    (slider("gap", "Page gap", 0.0, 64.0, 1.0, prefs.gap.to_string(), "px"))
                    div {
                        span { "Background" }
                        (seg("bg", &prefs.bg, &[
                            ("black", "Black"),
                            ("white", "White"),
                            ("sepia", "Sepia"),
                            ("custom", "Custom"),
                        ]))
                    }
                    div {
                        span { "Custom colour" }
                        input id="reader-bg-custom" class="reader-color" type="color"
                            data-pref="bgCustom" aria-label="Custom background colour"
                            value=(prefs.bg_custom);
                    }
                    div {
                        span { "Page border" }
                        (toggle("frame", prefs.frame, "Off", "On"))
                    }
                }

                // Group: input
                section class="reader-sheet-group" data-group="input" {
                    h3 { "Input" }
                    div {
                        span { "Tap zones" }
                        div id="reader-zone-editor" class="reader-zone-editor" data-pref="clickZones" {
                            span class="reader-zone-handle" data-edge="left" role="slider" tabindex="0"
                                aria-label="Left zone boundary"
                                aria-valuenow=(prefs.click_zones.first().copied().unwrap_or(30))
                                aria-valuemin="10" aria-valuemax="90" {}
                            span class="reader-zone-handle" data-edge="right" role="slider" tabindex="0"
                                aria-label="Right zone boundary"
                                aria-valuenow=(prefs.click_zones.get(1).copied().unwrap_or(70))
                                aria-valuemin="10" aria-valuemax="90" {}
                        }
                    }
                    @for (action, keys) in &keybind_rows {
                        div class="reader-keybind-row" data-action=(action) data-pref="keybinds" {
                            span { (action) }
                            @for (ki, key) in keys.iter().enumerate() {
                                button class="reader-keybind-key" type="button" data-key-index=(ki) { (key) }
                            }
                            button class="reader-keybind-reset" type="button" { "Reset" }
                        }
                    }
                    (slider("swipeThreshold", "Swipe distance", 40.0, 160.0, 5.0, prefs.swipe_threshold.to_string(), "px"))
                    div {
                        span { "Auto next chapter" }
                        (toggle("autoNext", prefs.auto_next, "Off", "On"))
                    }
                    div {
                        span { "Auto previous chapter" }
                        (toggle("autoPrev", prefs.auto_prev, "Off", "On"))
                    }
                }

                // Group: comfort
                section class="reader-sheet-group" data-group="comfort" {
                    h3 { "Comfort" }
                    div {
                        span { "Focus mode" }
                        button class="reader-seg-btn" type="button" data-action="ui.focus" { "Toggle" }
                    }
                    (slider("hideDelay", "Hide controls after", 1.0, 10.0, 1.0, prefs.hide_delay.to_string(), "s"))
                    div {
                        span { "Dim after idle" }
                        (seg("dimDelay", &prefs.dim_delay.to_string(), &[
                            ("0", "Off"),
                            ("15", "15s"),
                            ("30", "30s"),
                            ("60", "60s"),
                        ]))
                    }
                    (slider("dimOpacity", "Dim strength", 0.2, 0.8, 0.1, format!("{:.1}", prefs.dim_opacity), ""))
                    div {
                        span { "Show notes" }
                        // `#reader-notes-toggle` is named by contract §6.2 (the
                        // focus-mode pin exception) but missing from the §9
                        // index; the `on` half carries it so the CSS hook works.
                        (toggle_with_id("showNotes", prefs.show_notes, "Off", "On", Some("reader-notes-toggle")))
                    }
                }

                // Group: thisSeries
                section class="reader-sheet-group" data-group="thisSeries" {
                    h3 { "This series" }
                    p class="reader-series-remember-note" data-series-title=(manga.title) {
                        "Remembering for " (manga.title)
                    }
                }

                button class="reader-sheet-close" type="button" data-r-close aria-label="Close settings" { "✕" }
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
    let css = cfg.css_overrides();
    let theme = crate::config::resolve_theme(&db, user.as_ref());
    Html(crate::templates::layout::reader_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
        &css,
        &cfg.site_title,
        user.as_ref(),
        &theme,
        &prefs,
    ).0)
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
        assert!(a.contains(r#"data-r-direction="rtl""#), "{a}");
        assert!(a.contains(r#"data-r-fit="height""#), "{a}");
        assert!(a.contains(r#"data-r-bg="white""#), "{a}");
        assert!(a.contains(r#"data-r-frame="on""#), "{a}");
    }

    #[test]
    fn body_attrs_omit_empty_values() {
        let p = ReaderPrefs::default(); // fit == "" and frame == false
        let a = body_prefs_attrs(&p);
        assert!(!a.contains("data-r-fit=\"\""), "{a}");
        assert!(!a.contains("data-r-frame"), "{a}");
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
        p.keybinds
            .insert("page.next".into(), vec!["</script><script>alert(1)</script>".into()]);
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
        let out = crate::templates::layout::reader_layout(
            "t", maud::html! {}, "", "S", None, "", &p,
        ).into_string();
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
            (13i64, vec!["</script><script>alert(1)</script>".to_string()]),
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
