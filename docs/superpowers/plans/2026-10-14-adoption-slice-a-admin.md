# Adoption Redesign — Slice A: Admin Operations Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An admin can answer "is source X working, and why not?", trigger syncs, and fix the catalog from `/admin`, without reading logs or editing SQLite.

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice A.

**Split:** A1 (registry and sync history) → A2 (`/admin/sources`, `/admin/sync`) → A3 (`/admin/series`). One commit each.

## Bugs found while planning (fixed in A1)

| Bug | Effect | Fix |
|---|---|---|
| `scan_all` returns at the first failing adapter | One broken source stops every later source from being scanned | Scan each adapter independently and record each result |
| `list_manga_ids_needing_sync` joins chapters without grouping | A stale manga with N chapters is returned N times, so one pass re-syncs it up to 50 times | `NOT EXISTS` instead of the join |
| Every scan re-runs `ensure_manga_in_series`; every sync overwrites the series title | Admin merges, splits, and renames would be undone at the next sync | Link only unlinked manga; a renamed series sets `title_locked` |
| `AppState.adapter_health` is written but never read, and resets on restart | No visibility into source health | Derive health from the persisted `sync_runs` table; remove the in-memory map |

## A1 — Registry and sync history

- [x] `scraper::adapters::all()` is the single source list. `ScraperEngine::from_adapters`. The CLI and `build_engine` both use it (Open/Closed: a new adapter is one line in one place).
- [x] `ScraperEngine::scan_source(index)` scans one adapter. `scan_all` returns per-source results instead of failing fast. `sync_manga`/`sync_chapter` return item counts.
- [x] Scan links a manga to a series only when it has none. `series.title_locked`; `update_series_title` skips locked series.
- [x] `list_manga_ids_needing_sync` returns each manga once.
- [x] `sync_runs(id, source, kind, target_id, started_at, finished_at, status, items, error)`. Every scan, loop sync, and on-demand sync writes a row. On boot, rows left `running` become `failed` ("interrupted by restart"). Rows older than 30 days are pruned each pass.
- [x] `source_health(conn)` per source: last success, last failure, failure streak, last error. `adapter_health` is removed from `AppState`.
- [x] Tests: per-source scan isolation is covered by the engine's signature (no live sites in tests); DB tests for run lifecycle, streak, restart cleanup, pruning, distinct stale ids, link-only-unlinked, title lock.

## A2 — `/admin/sources` and `/admin/sync`

- [x] `sources_disabled` setting (CSV). Disabled sources are skipped by the loop's scans, by `list_manga_ids_needing_sync`, and by `run_job` (not recorded, since nothing ran).
- [x] `AppState.spawn_job` is the single entry point for background jobs (admin buttons, series page views, reader). Tests swap in a recorder, so no route test launches Chromium.
- [x] `/admin/sources`: status tag (Healthy / N recent failures / Failing after 3 in a row / Never run / Disabled), manga count, last success, expandable last error, "Scan now", enable/disable (logged in `mod_log`).
- [x] `/admin/sync`: loop status with "Run full sync now" (`Notify::notify_one`, so a press during a pass is not lost), all/failed tabs over the last 100 runs, Retry on failed manga and chapter runs.
- [x] Dashboard links both pages; the "Sources: not available yet" card is gone.
- [x] Tests: admin-only on all six endpoints, health and escaping, toggle round trip, retry rules, loop wake.

## A3 — `/admin/series`

- [x] `series.hidden_at` and `series.cover_override`. Hidden series leave the grid, search, home recents, the series count and continue-reading, and their series page and chapters return 404 for everyone but admins (who see a notice). The override cover wins everywhere a cover is shown.
- [x] `/admin/series`: search over all series (hidden included), newest first, capped at 100.
- [x] `/admin/series/{id}`: rename (sets `title_locked`), cover override (http/https only, blank clears), hide/show, merge into another series (moves source entries and bookmarks, deletes the merged series, one transaction), split a source entry into a new series (never the last one).
- [x] Admin-only (moderators get 403). Every change is written to `mod_log`.
- [x] Tests: permissions, hidden 404s for readers, cover/title validation, merge and split round trip including bad targets.

### Follow-ups for later slices
- Slice C: `merge_series` must also move comments (keyed by `series_id`) once they exist.
- Slice D: `/source/{source}` still lists hidden series' titles (their links 404). It is removed in D.
