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

## A2 — `/admin/sources` and `/admin/sync` (detailed when A1 lands)

Sources table with health, manga count, enable/disable (`sources_disabled` setting, honored by the loop and on-demand syncs), and "Scan now". Sync page with loop status, recent runs, a failed-only filter, per-run retry, and "Run full sync now" (wakes the loop via `Notify`).

## A3 — `/admin/series` (detailed when A2 lands)

Search including hidden series; rename (locks the title); hide/unhide (hidden series disappear for non-admins); cover override; merge two series (moves manga and bookmarks); split one manga into its own series. Admin-only; every action logged in `mod_log`.
