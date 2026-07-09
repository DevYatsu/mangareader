# MangaRead

Offline manga reader — scrapes chapters from third-party sites, merges them into unified series, and serves them through a local web interface with bookmarking and reading progress tracking.

## Architecture

```
┌─────────────────┐     ┌──────────────────────┐     ┌──────────────┐
│   CLI (scraper) │     │   Web Server (website)│     │  Browser     │
│                 │     │                      │     │              │
│  cargo run scan │───▶ │  GET /series         │◀───▶│  HTML pages  │
│  sync-manga N   │     │  GET /series/{id}    │     │  app.js      │
│  sync-chapter N │     │  GET /read/{m}/{ch}  │     │  localStorage│
│                 │     │                      │     │  (bookmarks) │
│  ┌───────────┐  │     │  ┌─────────────────┐ │     │              │
│  │Scraper    │  │     │  │bg sync thread   │ │     │              │
│  │Engine     │──┼─────│─▶│  opens chromedrv│ │     │              │
│  │ + adapters│  │     │  │  runs sync_*()  │ │     │              │
│  └───────────┘  │     │  │  quits driver   │ │     │              │
│                 │     │  └─────────────────┘ │     │              │
└────────┬────────┘     └──────────┬───────────┘     └──────────────┘
         │                        │
         │         ┌──────────────┴──────────────┐
         └────────▶│     SQLite (mangaread.db)    │
                   │                              │
                   │  series → manga → chapters   │
                   │              → pages         │
                   └──────────────────────────────┘
```

**Three crates** in a Cargo workspace:

| Crate | Role |
|-------|------|
| `mangaread-core` | Domain types, SQLite schema, repositories, services |
| `scraper` | CLI binary + library. `SiteAdapter` trait + implementations for each source site |
| `website` | Axum web server. Routes, templates, background scraper threads |

### Key design decisions

- **Unified series**: Manga from different source sites are linked to a single `series` by normalized title. Chapters from all sources are merged; the preferred source is used where available.
- **On-demand background sync**: Visiting a series page triggers a background chromedriver session if the data is stale (>30 min). The page shows a loading state and auto-reloads when the sync completes.
- **No job queue**: Syncs run in ephemeral `std::thread` + `tokio::runtime` instances (one per sync). Each creates a fresh chromedriver and quits it when done.
- **Client-side state**: Bookmarks, reading progress (current page), source preferences — all stored in `localStorage`. No user accounts.

## Quick Start

```bash
# 1. Scan source sites for manga listings
cargo run -p manga-scraper scan

# 2. Sync a specific manga's chapters (optional — website can do this on demand)
cargo run -p manga-scraper sync-manga 1

# 3. Start the web server
cargo run -p website

# 4. Open http://localhost:3000
```

The website will automatically sync chapters and pages when you visit a series or chapter that has no data yet.

## Docker

```bash
docker compose up --build
```

Opens on http://localhost:3000. The SQLite database persists in a named volume.

## CLI Reference

### `cargo run -p manga-scraper scan`
Visit the index page of every configured source site, discover manga listings, upsert into the database. Auto-links each manga to a series by normalized title.

### `cargo run -p manga-scraper sync-manga <id>`
Visit a manga's detail page, extract chapter list, upsert chapters. Updates the series title with the clean detail-page title.

### `cargo run -p manga-scraper sync-chapter <id>`
Visit a chapter's reader page, extract page image URLs, upsert pages.

## Source Adapters

| Adapter | Site | Coverage |
|---------|------|----------|
| `DemonicscansAdapter` | demonicscans.org | Chapters, pages |
| `ManhuaplusAdapter` | manhuaplus.top | Chapters, pages |

Adding a new source: implement `SiteAdapter` in `scraper/src/adapters/`, register it in `scraper_helper.rs`.

## Project Structure

```
mangaread/
├── mangaread-core/           # Shared domain + data layer
│   └── src/
│       ├── domain/            # Pure data types (no DB dependency)
│       │   ├── series.rs      Series, SeriesWithManga
│       │   ├── manga.rs       Manga, MangaWithCount, NewManga
│       │   ├── chapter.rs     Chapter, NewChapter, RecentChapter
│       │   ├── page.rs        Page, NewPage
│       │   ├── source.rs      MangaListing, MangaInfo (scraper types)
│       │   └── title.rs       Title cleaning/normalization
│       ├── db/                # SQLite repos (flat namespace via db.rs)
│       │   └── ...            insert/get/list for each aggregate
│       ├── service/           # Business logic
│       │   └── series.rs      pick_primary, merge_chapters, is_series_fresh
│       └── error.rs
│
├── scraper/                   # CLI + library
│   └── src/
│       ├── adapter.rs         SiteAdapter trait
│       ├── engine.rs          ScraperEngine (scan, sync_manga, sync_chapter)
│       ├── adapters/
│       │   ├── demonicscans.rs
│       │   ├── manhuaplus.rs
│       │   └── helpers.rs     Shared: wait_for_page, make_absolute, extract_chapter_number
│       └── main.rs            CLI entrypoint
│
├── website/                   # Web server
│   └── src/
│       ├── main.rs            AppState, route setup, seed data
│       ├── scraper_helper.rs  Background sync threads
│       ├── routes/
│       │   ├── home.rs        GET /
│       │   ├── series.rs      GET /series, /series/{id}
│       │   ├── reader.rs      GET /read/{manga_id}/{chapter_id}
│       │   ├── source.rs      GET /source/{source}
│       │   ├── manga.rs       GET /manga/{id} → redirect to series
│       │   └── check.rs       GET /api/check-manga, /api/check-chapter
│       ├── templates/
│       │   └── layout.rs      base_layout, reader_layout
│       └── static/
│           ├── style.css      All styles
│           └── app.js         Client-side: bookmarks, reader UX, source filters
│
├── docs/architecture/         Architecture Decision Records
├── Dockerfile                 Multi-stage Docker build
└── docker-compose.yml         Single service, port 3000, persistent volume
```

## Client-side Features

All state is stored in `localStorage` — no backend required.

### Reading progress
- Chapters show status icons: ○ unread, ▶ started, ✓ finished
- The reader tracks which page the user is on via `IntersectionObserver`
- Progress is saved on `beforeunload` and auto-restored on return
- A chapter is marked "finished" when the last page scrolls into view

### Source filters
- When a series has chapters from multiple source sites, checkboxes appear above the chapter list
- Toggling a source hides/shows chapters from that site
- The preferred source (most chapters) is used for duplicate chapter numbers

### Reader UX
- Click the left third of the screen for previous page, right third for next page
- Arrow keys: ← prev, → next
- Scroll up or mouse near top to show the top bar with page indicator
- Bottom nav after the last page links to prev/next chapters

## Database

SQLite with WAL mode. Tables:

- **series**: Unified title, groups manga from multiple sources
- **manga**: Per-source entries linked to a series
- **chapters**: Chapter metadata, linked to a manga entry
- **pages**: Page image URLs, linked to a chapter

All inserts use `ON CONFLICT ... DO UPDATE` (upsert) — safe to re-run.

## Architecture Decision Records

See [docs/architecture/](docs/architecture/) for ADRs:
- ADR-001: Domain model restructuring
- ADR-002: Centralized adapter registry
- ADR-003: Template extraction from route handlers

## License

Training project — use freely.
