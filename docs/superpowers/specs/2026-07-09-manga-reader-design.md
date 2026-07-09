# Manga Reader — Design Spec

## Overview

A manga reading web application that scrapes chapters from third-party manga sites and serves them through a local web interface. Built as a Rust training project.

**Stack:**
- Web framework: Axum
- HTML templates: Maud
- Scraper: Thirtyfour (Selenium WebDriver)
- Storage: SQLite (via rusqlite)
- Workspace: 3-crate Cargo workspace

## Architecture

```
mangaread/                         # Cargo workspace
├── Cargo.toml                     # [workspace] with members
├── mangaread-core/                # Shared library — types, DB, migrations
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── db/                    # Database connection, migrations, queries
│       ├── models.rs              # Domain types (Manga, Chapter, Page)
│       └── error.rs               # Shared error type
├── scraper/                       # Thirtyfour-based scraper binary
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs                # CLI entry point
│       ├── engine.rs              # ScraperEngine — orchestrates adapters + DB writes
│       ├── adapters/              # SiteAdapter implementations
│       │   ├── mod.rs
│       │   └── demonicscans.rs    # Demo adapter for demonicscans.org
│       └── error.rs
└── website/                       # Axum + Maud web server binary
    ├── Cargo.toml
    └── src/
        ├── main.rs                # Axum server setup, routes
        ├── routes/                # Route handlers
        │   ├── mod.rs
        │   ├── home.rs            # GET /
        │   ├── source.rs          # GET /source/:source
        │   ├── manga.rs           # GET /manga/:id
        │   └── reader.rs          # GET /read/:manga_id/:chapter_id
        ├── templates/             # Maud HTML templates
        │   ├── mod.rs
        │   ├── layout.rs          # Base HTML layout (head, nav, dark theme)
        │   ├── home.rs
        │   ├── source.rs
        │   ├── manga_detail.rs
        │   └── reader.rs
        └── static/                # Served by tower-http
            └── style.css
```

### Data Flow

```
Scraper (thirtyfour)  ──write──▶  SQLite  ──read──▶  Website (axum + maud)
       │                                                     │
   discovers manga,                                     serves HTML
   fetches chapters,                                  with image URLs
   extracts page images
```

## Database Schema

### `manga` — manga series from source sites

| Column | Type | Constraints | Notes |
|---|---|---|---|
| id | INTEGER | PK AUTOINCREMENT | |
| source | TEXT | NOT NULL | e.g. "demonicscans" |
| source_id | TEXT | NOT NULL | detail page URL or source-internal ID |
| title | TEXT | NOT NULL | |
| author | TEXT | | |
| cover_url | TEXT | | |
| description | TEXT | | |
| detail_url | TEXT | | The detail page URL on the source site |
| created_at | TEXT | NOT NULL DEFAULT datetime('now') | |
| updated_at | TEXT | NOT NULL DEFAULT datetime('now') | |

UNIQUE(source, source_id)

### `chapters` — chapters belonging to a manga

| Column | Type | Constraints | Notes |
|---|---|---|---|
| id | INTEGER | PK AUTOINCREMENT | |
| manga_id | INTEGER | NOT NULL REFERENCES manga(id) ON DELETE CASCADE | |
| source_id | TEXT | NOT NULL | chapter URL or ID on source site |
| number | REAL | NOT NULL | allows fractional chapters |
| title | TEXT | | |
| url | TEXT | | The chapter page URL on the source site |
| created_at | TEXT | NOT NULL DEFAULT datetime('now') | |

UNIQUE(manga_id, source_id)

### `pages` — individual image URLs for a chapter

| Column | Type | Constraints | Notes |
|---|---|---|---|
| id | INTEGER | PK AUTOINCREMENT | |
| chapter_id | INTEGER | NOT NULL REFERENCES chapters(id) ON DELETE CASCADE | |
| index | INTEGER | NOT NULL | 1-based page order |
| image_url | TEXT | NOT NULL | direct link to image |
| downloaded | INTEGER | NOT NULL DEFAULT 0 | 0 = URL only, 1 = cached locally |

UNIQUE(chapter_id, index)

## Scraper Architecture

### `SiteAdapter` trait (in `scraper` crate)

```rust
#[async_trait]
pub trait SiteAdapter: Send + Sync {
    fn source_name(&self) -> &str;
    fn base_url(&self) -> &str;

    /// Step 1: Crawl the main listing page → discover manga and their detail URLs
    async fn discover_manga(&self) -> Result<Vec<MangaListing>>;

    /// Step 2: Visit a manga detail page → full info + chapter URLs
    async fn fetch_manga(&self, detail_url: &str) -> Result<MangaInfo>;

    /// Step 3: Visit a chapter page → extract page image URLs
    async fn fetch_pages(&self, chapter_url: &str) -> Result<Vec<String>>;
}
```

### Core types

```rust
pub struct MangaListing {
    pub title: String,
    pub detail_url: String,
    pub cover_url: Option<String>,
}

pub struct MangaInfo {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub detail_url: String,
    pub chapters: Vec<ChapterListing>,
}

pub struct ChapterListing {
    pub title: Option<String>,
    pub number: f64,
    pub url: String,
}
```

### `ScraperEngine`

- Owns a `Vec<Box<dyn SiteAdapter>>` and a DB connection pool
- `scan()` — runs `discover_manga()` on all adapters, upserts manga into DB
- `sync_manga(manga_id)` — runs `fetch_manga()` for the stored detail_url, upserts chapters
- `sync_chapter(chapter_id)` — runs `fetch_pages()` for the stored chapter URL, upserts pages
- Each step is idempotent — URLs are unique constraints, re-running updates existing rows

### Scraping flow (demonicscans.org example)

```
1. discover_manga()
   → WebDriver loads https://demonicscans.org/index.php
   → Finds all manga entry links/thumbnails on the page
   → Returns list of {title, detail_url, cover_url}

2. fetch_manga(detail_url)
   → WebDriver navigates to e.g. https://demonicscans.org/manga/123/one-piece
   → Extracts: description, author, cover, chapter table rows
   → Returns MangaInfo with chapters metadata

3. fetch_pages(chapter_url)
   → WebDriver navigates to reader page
   → Extracts all <img> tags containing manga pages
   → Returns ordered list of image URLs
```

## Website Routes

| Route | Method | Handler | Description |
|---|---|---|---|
| `/` | GET | `home` | Manga list grouped by source |
| `/source/:source` | GET | `source_list` | All manga from a specific source |
| `/manga/:id` | GET | `manga_detail` | Cover, info, chapter list |
| `/read/:manga_id/:chapter_id` | GET | `chapter_reader` | Vertical scroll reader with all pages |

### Template structure (maud)

- `layout.rs` — shared HTML shell: `<!DOCTYPE html>`, `<head>` with dark theme CSS, `<body>` wrapper
- `home.rs` — source links, manga thumbnails grouped by source
- `source.rs` — grid of manga covers from one source
- `manga_detail.rs` — hero section (cover + info), chapter table below
- `reader.rs` — sequential `<img>` elements vertically, prev/next chapter nav

### Styling

- Dark theme (background `#111`, text `#eee`, cards `#1a1a1a`)
- Max-width container (900px) centered
- Images constrained to `max-width: 100%` in reader
- No JavaScript required for MVP

## Implementation Order

### Phase 1: Workspace & Core
1. Create root `Cargo.toml` workspace
2. Update `scraper/Cargo.toml` and `website/Cargo.toml` to be workspace members
3. Create `mangaread-core` crate with rusqlite dependency
4. Define domain models
5. Set up SQLite connection + migrations (CREATE TABLE statements)
6. Implement CRUD functions: insert/update manga, chapters, pages

### Phase 2: Website (read-only, prove it works)
1. Add axum + maud + tower-http + core dependency to `website/Cargo.toml`
2. Set up axum server with static file serving + router
3. Implement maud layout template (dark theme CSS inline)
4. Implement home route — query manga list from DB
5. Implement source route — query manga by source
6. Implement manga detail route — query manga + chapters
7. Implement reader route — query pages for chapter
8. Manual test: seed a few rows via CLI, verify rendering

### Phase 3: Scraper
1. Add thirtyfour + core dependency to `scraper/Cargo.toml`
2. Implement `SiteAdapter` trait + error types
3. Implement `ScraperEngine` with scan/sync methods
4. Build CLI with clap: `scraper scan`, `scraper sync-manga <id>`, `scraper sync-chapter <id>`
5. Implement demonicscans.org adapter
6. Test end-to-end: scan → sync → browse via website

## Future Considerations (out of scope for MVP)

- Image downloading / local caching (the `downloaded` column is reserved)
- Docker Compose for one-command setup
- Multiple simultaneous adapters
- Headless browser configuration options
- Scheduled scraping / background refresh
- Search functionality
- Image proxy to avoid hotlinking issues

## Constraints

- MVP has zero JavaScript — pure server-rendered HTML + CSS
- Images served from original source URLs (no local proxy for MVP)
- Scraping requires a running WebDriver instance (geckodriver or chromedriver)
- SQLite database stored at a configurable path (default `./mangaread.db`)
