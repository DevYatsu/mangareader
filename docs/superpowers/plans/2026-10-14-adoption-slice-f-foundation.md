# Adoption Redesign — Slice F: Foundation Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the duplicated page preamble and the reader-facing sync banner, and make `/series` cost a constant number of queries.

**Architecture:** One `layout::page()` helper owns the shell inputs (`SiteConfig` → overrides, title, user, nav). `base_layout` loses its `sync_banner` parameter. One SQL query replaces the per-series and per-manga loops in `list_all_series` and `search_series`.

**Tech Stack:** Rust, Axum 0.8, Maud 0.27, rusqlite 0.32 (bundled SQLite ≥ 3.25, so window functions are available).

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice F.

## Global Constraints

- No behavior change other than: the banner is gone, chapter counts are distinct per chapter number, and the home "Series" stat counts series.
- No `name=`, `action=`, or `method=` changes on any form.
- `/api/sync-status` keeps its response shape. The series-page error toast stays until Slice D hides sync state from readers.

## Decision: helper function, not an extractor

The spec said `Page` extractor. A `FromRequestParts` extractor would lock the DB mutex once to build `SiteConfig`, and the handler would lock it again. A plain function that takes a `&SiteConfig` keeps one lock acquisition per request and has no hidden I/O (KISS). The spec is amended to match.

### Task F1: `layout::page` helper and banner removal

- [x] Add `pub fn page(cfg: &SiteConfig, title: &str, nav: Nav, user: Option<&AuthUser>, content: Markup) -> Html<String>` in `templates/layout.rs`.
- [x] Drop `sync_banner` from `base_layout`. Delete the five `auto_sync` banner blocks (`home`, `series` ×2, `source`, `bookmarks`).
- [x] Migrate every `base_layout` caller to `page()`. Keep local wrappers only where they add page-specific behavior.
- [x] Delete `.sync-banner*` rules and the now-unused `m-spin` keyframes from `theme.css`.
- [x] Test: rendered home HTML contains no `sync-banner`.

### Task F2: Constant-query series listing

- [x] Add `db::list_series_cards(conn, search: Option<&str>) -> Vec<SeriesCard { id, title, cover_url, chapter_count }>`. It is one query: a per-manga chapter count ranked by `ROW_NUMBER()` per series picks the cover, and `COUNT(DISTINCT c.number)` gives the chapter count. Series with zero chapters are excluded in SQL. Search escapes `%`, `_`, and `\`.
- [x] `series_list` uses it. Remove `list_all_series` and `search_series` if they have no other callers.
- [x] Tests: two sources sharing chapter numbers count once; the cover comes from the manga with the most chapters; a series without chapters is excluded; a `%` search is literal.

### Task F3: Home stat counts series

- [x] Replace `count_manga_with_chapters` with `count_series_with_chapters` (`COUNT(DISTINCT m.series_id)`).
- [x] Test: one series with two sources counts as 1.

### Verify

```bash
cargo fmt --all -- --check
# `-D warnings` currently fails on lints that predate this slice (mangaread-core
# domain/source.rs, title.rs, reader.rs, user.rs). Gate: no new warnings in touched files.
cargo clippy --workspace --all-targets
cargo test --workspace
git grep -n "sync_banner\|Syncing series\|sync-banner" -- website/src website/static
```

Expected for the grep: no output.
