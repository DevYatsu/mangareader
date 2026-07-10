# Architecture

## Overview

MangaRead is a self-hosted manga reader. It scrapes chapters from third-party scanlation sites, merges them into unified series, and serves them through a local web interface. Think of it as a personal Tachiyomi server, but in Rust.

The whole project lives in a Cargo workspace with three crates:

```
mangaread/              # Workspace root
├── mangaread-core/     # Domain models, database, services
├── scraper/            # Web scraper library + CLI tool
└── website/            # HTTP server + HTML templates
```

## Data Model

Four SQLite tables. Every insert uses `ON CONFLICT ... DO UPDATE` so syncing is idempotent — safe to run any operation twice.

```
series
  id            INTEGER PRIMARY KEY
  title         TEXT UNIQUE          -- normalized title (lowercase, trimmed)
  display_title TEXT                 -- human-readable title
  created_at    TEXT

manga
  id            INTEGER PRIMARY KEY
  series_id     INTEGER → series    -- which unified series this belongs to
  source        TEXT                 -- "demonicscans" | "manhuaplus"
  source_id     TEXT                 -- URL on the source site
  title         TEXT
  author        TEXT?
  description   TEXT?
  cover_url     TEXT?
  created_at    TEXT

chapters
  id            INTEGER PRIMARY KEY
  manga_id      INTEGER → manga
  number        REAL                 -- chapter number (supports decimals like 58.5)
  title         TEXT?
  url           TEXT                 -- chapter page URL on the source site
  synced        INTEGER DEFAULT 0   -- 1 = pages have been fetched
  created_at    TEXT

pages
  id            INTEGER PRIMARY KEY
  chapter_id    INTEGER → chapters
  page_number   INTEGER
  image_url     TEXT
```

The key relationship: **series → manga → chapters → pages**. A series groups manga from different sources that are the same title. For example, "One Piece" might have manga entries from demonicscans and manhuaplus, each with their own chapter lists.

## Crate Breakdown

### mangaread-core

The shared library with no HTTP or WebDriver dependencies. Everything else depends on it.

```
mangaread-core/src/
  lib.rs            -- re-exports
  db.rs             -- SQLite queries (one file, ~20k lines, all functions here)
  error.rs          -- unified error type
  domain/
    series.rs       -- Series, SeriesWithManga structs
    manga.rs        -- Manga, MangaWithCount, NewManga
    chapter.rs      -- Chapter, NewChapter, RecentChapter
    page.rs         -- Page, NewPage
    source.rs       -- MangaListing, MangaInfo, ChapterListing (scraper-facing)
    title.rs        -- title cleaning + normalization utilities
  service/
    series.rs       -- cross-source merging logic
```

The database layer is a monolith. All 30+ query functions live in `db.rs`. There's no per-entity repository pattern — it was planned but never completed. The service layer handles the tricky logic: picking which source to prefer when multiple sources have the same chapter number, deciding which manga entry is the "primary" one (the one with the most chapters), and checking staleness for re-sync.

### scraper

The scraping engine. Both a library (used by the website) and a standalone CLI binary.

```
scraper/src/
  adapter.rs        -- SiteAdapter trait
  engine.rs         -- ScraperEngine: orchestrates scan → fetch → persist
  main.rs           -- CLI entrypoint
  adapters/
    demonicscans.rs -- demonicscans.org adapter
    manhuaplus.rs   -- manhuaplus.top adapter
    helpers.rs      -- HTML parsing utilities
```

Each source site gets an implementation of the `SiteAdapter` trait, which has three methods:

1. `discover_manga(driver)` — visit the site's index page, find all manga listings, return titles + detail page URLs
2. `fetch_manga(driver, detail_url)` — visit a manga's detail page, extract metadata and chapter list
3. `fetch_pages(driver, chapter_url)` — visit a chapter's reader page, find all page image URLs

The `ScraperEngine` wraps these into higher-level operations:

- `scan_all()` — run `discover_manga` on every adapter, upsert results into the database
- `sync_manga(id)` — run `fetch_manga` on one manga entry, upsert its chapters
- `sync_chapter(id)` — run `fetch_pages` on one chapter, upsert its pages

Adapters use `thirtyfour` (Selenium WebDriver) to drive a real Chrome browser. HTML parsing uses `selectrs` (a CSS-selector-based parser, not regex). Images are identified by CSS selectors that match the specific site's reader page structure.

### website

The Axum web server. Serves HTML (rendered with Maud templates), static assets, and JSON API endpoints.

```
website/src/
  main.rs           -- AppState, router setup, tokio::main entrypoint
  config.rs         -- SiteConfig (reads SITE_TITLE, SITE_COLOR, etc. from env)
  scraper_helper.rs -- chromedriver management, background sync loop
  routes/
    home.rs         -- GET /
    series.rs       -- GET /series, GET /series/{id}
    bookmarks.rs    -- GET /bookmarks (client-side shell)
    source.rs       -- GET /source/{source}
    manga.rs        -- GET /manga/{id} (redirects to series page)
    reader.rs       -- GET /read/{manga_id}/{chapter_id}
    check.rs        -- GET /api/check-manga/{id}, /api/check-chapter/{id}
    api.rs          -- GET /api/sync-status, POST /api/sync-manga/{id}, GET /api/progress-series
    health.rs       -- GET /health
  templates/
    layout.rs       -- base_layout, reader_layout, error_layout
  static/
    style.css       -- all styles (2319 lines, one file)
    app.js          -- all client-side JavaScript
    favicon.svg
```

The `readme layout mentioned earlier shows how it all fits together.

## Scraping Pipeline

This is the heart of the project. Here's what happens when the server starts up:

```
startup
  └─ start_sync_loop(state)
       └─ loop every 6 hours:
            ├─ scan all sources
            │    └─ for each adapter:
            │         ├─ visit index page
            │         ├─ find manga links
            │         └─ upsert into manga table (auto-linking to series)
            │
            ├─ find stale manga (>2h since last sync)
            │
            └─ for each stale manga (parallel, up to 3 at a time):
                 ├─ spawn OS thread with own tokio runtime
                 │    ├─ open own DB connection (rusqlite is !Sync)
                 │    ├─ launch chromedriver
                 │    ├─ visit manga detail page → upsert chapters
                 │    └─ for each new chapter:
                 │         └─ visit chapter page → upsert page URLs
                 └─ thread exits, driver quits
```

The parallel sync is the most interesting part. `rusqlite::Connection` doesn't implement `Sync` (it's not safe to share across threads), so each background task opens its own connection. This also sidesteps the `Mutex<Connection>` contention — the main thread holds the lock for reads while background threads write independently.

Each chromedriver session is isolated. A session starts, does its work, and quits. No chromedriver pool, no persistent sessions. This is wasteful but simple, and the 3-concurrency cap prevents the system from spawning 50 simultaneous Chrome processes.

### WebDriver Setup

The server uses a system-installed chromedriver (not letting thirtyfour auto-download one, which caused version mismatches in Docker). The path is resolved by:

1. Check `CHROMEDRIVER_PATH` env var
2. Run `which chromedriver` (or `where chromedriver` on Windows)
3. Fall back to `/usr/bin/chromedriver` (the Docker default)

Chrome flags are set via `CHROMIUM_FLAGS` env var. In Docker this includes `--headless` since there's no display.

## Sync Loop Lifecycle

The sync loop runs in a background `std::thread` (not a tokio task) because the scraping is CPU-bound and blocking — it launches OS subprocesses (chromedriver) and waits on network I/O. Running it on the tokio runtime would starve the web server's event loop.

The loop state is published through `AppState` fields:

- `sync_error` — cleared on read, so the web UI shows a fresh error once and dismisses it
- `syncing_manga_ids` — which manga are currently being synced (for the "currently syncing" spinner)
- `auto_sync` — SyncProgress struct with running/total/current counters
- `adapter_health` — per-source failure tracking (consecutive failures, last error message)

The web UI polls `/api/sync-status` every few seconds to show progress. There's no WebSocket or server-sent events — polling is good enough for a single-user app.

## Web Server Structure

Routes are defined in `main.rs` and handled by modules in `routes/`. Each handler:

1. Locks the `Mutex<Connection>` to read data
2. Renders HTML with Maud (most pages have inline templates in the route handler, not extracted modules)
3. Returns `Html<String>`

Three layout shells exist in `templates/layout.rs`:

- **`base_layout`** — full page with site header, nav, footer, `<script src="/static/app.js">`. Used for home, series list, series detail, bookmarks, source list.
- **`reader_layout`** — minimal HTML shell, no header/footer, `<body class="reader-page">`. Used for the chapter reader.
- **`error_layout`** — same shell as `base_layout` but with an error section. Used for 404s and other failures.

## Client-Side JavaScript

All JS lives in `app.js`. No framework, no build step — just vanilla DOM manipulation and `localStorage`.

Key features:

- **Reading progress**: Uses `IntersectionObserver` to detect which page is visible. Saves current page to `localStorage` on `beforeunload`. Shows ○/▶/✓ status icons on chapter lists.
- **Continue Reading**: On the home page, reads `localStorage` for in-progress chapters and renders a horizontal scroll of cover cards (170x227px) with progress bars.
- **Bookmarks**: Read from `localStorage`, rendered client-side on the bookmarks page. No server-side bookmark storage exists.
- **Source Filters**: Checkboxes above chapter lists that toggle visibility of chapters from each source site.
- **Reader Navigation**: Click left/right third of the screen, or use arrow keys to jump between chapters. Space bar scrolls normally.
- **DOM Creation**: A small `h(tag, props, ...children)` helper for creating DOM elements without innerHTML.

## Design Decisions

### Why not use thirtyfour's auto-downloaded chromedriver?

The auto-downloaded version often mismatched the system Chrome version, especially in Docker where the Chrome version is pinned. The system-installed chromedriver always matches. The trade-off is you need to install chromedriver separately (it's in the Dockerfile).

### Why Mutex<Connection> instead of r2d2 or sqlx?

The project started with `rusqlite` directly. Adding a connection pool like `r2d2` would have been non-trivial and the app is single-user — there's almost never contention. The `Mutex` is fine for this scale. Each background scraper opens its own connection anyway.

### Why parallel sync with OS threads instead of tokio tasks?

Chromedriver launches a Chrome process and blocks on network I/O. Tokio's cooperative scheduling can't yield during synchronous chromedriver calls, so a tokio task doing scraping would block the entire runtime. OS threads are the correct abstraction here.

The 3-concurrency limit is arbitrary but pragmatic. Each Chrome process uses ~200MB RAM. Three concurrent sessions = 600MB + the server itself. On a 2GB container that leaves enough headroom.

### Why is the database layer a monolith?

ADR-001 proposed splitting `db.rs` into per-entity repository files. The domain types and service layer were extracted, but the database queries never got split. The refactoring was deprioritized because `db.rs` works fine and the cost of splitting (breaking every import site-wide) wasn't justified by the benefit.

### Why are most page templates inline in route handlers?

ADR-003 proposed extracting all templates into dedicated modules. The layout shells were extracted (they're shared across pages), but the per-page templates stayed inline. Same reason as above — it works, and the extraction is mechanical work with no behavioral benefit.

## Running Without Docker

```bash
# Prerequisites: Rust 1.82+, chromedriver on PATH
cargo run -p website
```

The server creates `mangaread.db` in the current directory by default (override with `MANGAREAD_DB`). The background sync loop starts automatically — no manual `scan` step needed. Visit http://localhost:3000 and wait for the first sync pass to complete.
