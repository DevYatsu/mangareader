# Manga Reader — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a 3-crate workspace: shared core library, thirtyfour-based manga scraper, and axum+maud web reader.

**Architecture:** Workspace with `mangaread-core` (domain types + SQLite DB), `scraper` (SiteAdapter trait + thirtyfour engine), `website` (axum server + maud templates). Scraper writes to DB, website reads from DB.

**Tech Stack:** Rust 2024 edition, rusqlite (bundled), thirtyfour, axum, maud, tokio, clap, tower-http

**Build phases:**
1. Workspace & Core — types, DB schema, CRUD
2. Website — axum routes + maud templates (read from DB, prove it works)
3. Scraper — SiteAdapter trait, thirtyfour engine, demonicscans.org adapter

---

### Task 1: Workspace root + crate scaffolding

**Files:**
- Create: `Cargo.toml` (root workspace)
- Create: `mangaread-core/Cargo.toml`
- Create: `mangaread-core/src/lib.rs`
- Modify: `scraper/Cargo.toml` (add workspace member + deps)
- Modify: `website/Cargo.toml` (add workspace member + deps)

- [ ] **Step 1: Create root workspace Cargo.toml**

```toml
[workspace]
resolver = "2"
members = ["mangaread-core", "scraper", "website"]
```

Path: `/Users/yanis/Programming/mangaread/Cargo.toml`

- [ ] **Step 2: Create mangaread-core crate**

`mangaread-core/Cargo.toml`:
```toml
[package]
name = "mangaread-core"
version = "0.1.0"
edition = "2024"

[dependencies]
rusqlite = { version = "0.32", features = ["bundled"] }
serde = { version = "1", features = ["derive"] }
thiserror = "2"
```

`mangaread-core/src/lib.rs`:
```rust
pub mod db;
pub mod error;
pub mod models;
```

- [ ] **Step 3: Update scraper/Cargo.toml**

```toml
[package]
name = "scraper"
version = "0.1.0"
edition = "2024"

[dependencies]
mangaread-core = { path = "../mangaread-core" }
thirtyfour = "0.35"
tokio = { version = "1", features = ["full"] }
clap = { version = "4", features = ["derive"] }
thiserror = "2"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
async-trait = "0.1"
```

- [ ] **Step 4: Update website/Cargo.toml**

```toml
[package]
name = "website"
version = "0.1.0"
edition = "2024"

[dependencies]
mangaread-core = { path = "../mangaread-core" }
axum = "0.8"
maud = "0.28"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["fs"] }
tracing = "0.1"
tracing-subscriber = "0.3"
thiserror = "2"
anyhow = "1"
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cargo check` from `/Users/yanis/Programming/mangaread`
Expected: clean compilation, 3 crates found

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml mangaread-core/ scraper/Cargo.toml website/Cargo.toml
git commit -m "feat: set up 3-crate workspace with core/scraper/website"
```

---

### Task 2: Core domain models

**Files:**
- Create: `mangaread-core/src/models.rs`
- Create: `mangaread-core/src/error.rs`

- [ ] **Step 1: Write error module**

`mangaread-core/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Not found: {0}")]
    NotFound(String),
}
```

- [ ] **Step 2: Write domain models**

`mangaread-core/src/models.rs`:
```rust
/// A manga series scraped from a source site.
pub struct Manga {
    pub id: i64,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub detail_url: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewManga {
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub detail_url: String,
}

/// A chapter belonging to a manga.
pub struct Chapter {
    pub id: i64,
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
    pub created_at: String,
}

pub struct NewChapter {
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
}

/// A single page image in a chapter.
pub struct Page {
    pub id: i64,
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
    pub downloaded: bool,
}

pub struct NewPage {
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
}

// --- Scraper-side types (used by SiteAdapter) ---

/// Lightweight result from discovering manga on a listing page.
pub struct MangaListing {
    pub title: String,
    pub detail_url: String,
    pub cover_url: Option<String>,
}

/// Full manga info returned after visiting the detail page.
pub struct MangaInfo {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub detail_url: String,
    pub chapters: Vec<ChapterListing>,
}

/// A chapter entry discovered on a manga detail page.
pub struct ChapterListing {
    pub title: Option<String>,
    pub number: f64,
    pub url: String,
}
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p mangaread-core`
Expected: clean compilation

- [ ] **Step 4: Commit**

```bash
git add mangaread-core/src/models.rs mangaread-core/src/error.rs
git commit -m "feat(core): add domain models and error types"
```

---

### Task 3: Core database layer

**Files:**
- Create: `mangaread-core/src/db.rs`

- [ ] **Step 1: Write the full db module**

`mangaread-core/src/db.rs`:
```rust
use rusqlite::Connection;

use crate::error::Error;
use crate::models::*;

/// Create tables if they don't exist.
pub fn init_db(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS manga (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            source          TEXT NOT NULL,
            source_id       TEXT NOT NULL,
            title           TEXT NOT NULL,
            author          TEXT,
            cover_url       TEXT,
            description     TEXT,
            detail_url      TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(source, source_id)
        );

        CREATE TABLE IF NOT EXISTS chapters (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            manga_id        INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
            source_id       TEXT NOT NULL,
            number          REAL NOT NULL,
            title           TEXT,
            url             TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(manga_id, source_id)
        );

        CREATE TABLE IF NOT EXISTS pages (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            chapter_id      INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            index           INTEGER NOT NULL,
            image_url       TEXT NOT NULL,
            downloaded      INTEGER NOT NULL DEFAULT 0,
            UNIQUE(chapter_id, index)
        );
        ",
    )?;
    Ok(())
}

// ── Manga CRUD ──

pub fn insert_manga(conn: &Connection, m: &NewManga) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO manga (source, source_id, title, author, cover_url, description, detail_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source, source_id) DO UPDATE SET
            title = excluded.title,
            author = excluded.author,
            cover_url = excluded.cover_url,
            description = excluded.description,
            detail_url = excluded.detail_url,
            updated_at = datetime('now')",
        rusqlite::params![
            m.source,
            m.source_id,
            m.title,
            m.author,
            m.cover_url,
            m.description,
            m.detail_url,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_manga_sources(conn: &Connection) -> Result<Vec<String>, Error> {
    let mut stmt = conn.prepare("SELECT DISTINCT source FROM manga ORDER BY source")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut sources = Vec::new();
    for row in rows {
        sources.push(row?);
    }
    Ok(sources)
}

pub fn list_manga_by_source(conn: &Connection, source: &str) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga WHERE source = ?1 ORDER BY title",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], map_manga_row)?;
    rows.collect()
}

pub fn list_all_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect()
}

pub fn get_manga_by_id(conn: &Connection, id: i64) -> Result<Manga, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga WHERE id = ?1",
    )?;
    stmt.query_row(rusqlite::params![id], map_manga_row)
        .map_err(|_| Error::NotFound(format!("manga id={}", id)))
}

fn map_manga_row(row: &rusqlite::Row) -> rusqlite::Result<Manga> {
    Ok(Manga {
        id: row.get(0)?,
        source: row.get(1)?,
        source_id: row.get(2)?,
        title: row.get(3)?,
        author: row.get(4)?,
        cover_url: row.get(5)?,
        description: row.get(6)?,
        detail_url: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

// ── Chapter CRUD ──

pub fn insert_chapter(conn: &Connection, ch: &NewChapter) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO chapters (manga_id, source_id, number, title, url)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(manga_id, source_id) DO UPDATE SET
            number = excluded.number,
            title = excluded.title,
            url = excluded.url",
        rusqlite::params![ch.manga_id, ch.source_id, ch.number, ch.title, ch.url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_chapters_by_manga(conn: &Connection, manga_id: i64) -> Result<Vec<Chapter>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, manga_id, source_id, number, title, url, created_at
         FROM chapters WHERE manga_id = ?1 ORDER BY number",
    )?;
    let rows = stmt.query_map(rusqlite::params![manga_id], map_chapter_row)?;
    rows.collect()
}

pub fn get_chapter_by_id(conn: &Connection, id: i64) -> Result<Chapter, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, manga_id, source_id, number, title, url, created_at
         FROM chapters WHERE id = ?1",
    )?;
    stmt.query_row(rusqlite::params![id], map_chapter_row)
        .map_err(|_| Error::NotFound(format!("chapter id={}", id)))
}

fn map_chapter_row(row: &rusqlite::Row) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        manga_id: row.get(1)?,
        source_id: row.get(2)?,
        number: row.get(3)?,
        title: row.get(4)?,
        url: row.get(5)?,
        created_at: row.get(6)?,
    })
}

// ── Page CRUD ──

pub fn insert_page(conn: &Connection, p: &NewPage) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO pages (chapter_id, index, image_url)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(chapter_id, index) DO UPDATE SET image_url = excluded.image_url",
        rusqlite::params![p.chapter_id, p.index, p.image_url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_pages_by_chapter(conn: &Connection, chapter_id: i64) -> Result<Vec<Page>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, chapter_id, index, image_url, downloaded
         FROM pages WHERE chapter_id = ?1 ORDER BY index",
    )?;
    let rows = stmt.query_map(rusqlite::params![chapter_id], map_page_row)?;
    rows.collect()
}

fn map_page_row(row: &rusqlite::Row) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        index: row.get(2)?,
        image_url: row.get(3)?,
        downloaded: row.get::<_, i32>(4)? != 0,
    })
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p mangaread-core`
Expected: clean compilation

- [ ] **Step 3: Commit**

```bash
git add mangaread-core/src/db.rs
git commit -m "feat(core): add SQLite CRUD operations for manga, chapters, pages"
```

---

### Task 4: Website — server skeleton + layout

**Files:**
- Create: `website/src/main.rs`
- Create: `website/src/templates/mod.rs`
- Create: `website/src/templates/layout.rs`
- Create: `website/static/style.css`

- [ ] **Step 1: Write maud layout template**

`website/src/templates/layout.rs`:
```rust
use maud::{html, Markup, DOCTYPE};

pub fn base_layout(title: &str, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — MangaRead" }
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                div.container {
                    header {
                        h1 { a href="/" { "MangaRead" } }
                    }
                    main { (content) }
                    footer {
                        p { "MangaRead — training project" }
                    }
                }
            }
        }
    }
}
```

`website/src/templates/mod.rs`:
```rust
pub mod layout;
```

- [ ] **Step 2: Write dark theme CSS**

`website/static/style.css`:
```css
* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

body {
    background: #111;
    color: #eee;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    line-height: 1.6;
}

.container {
    max-width: 960px;
    margin: 0 auto;
    padding: 1rem;
}

header {
    padding: 1rem 0;
    border-bottom: 1px solid #333;
    margin-bottom: 2rem;
}

header h1 a {
    color: #f59e0b;
    text-decoration: none;
    font-size: 1.5rem;
    font-weight: 700;
}

header h1 a:hover {
    color: #fbbf24;
}

.manga-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 1.5rem;
    margin-top: 1rem;
}

.manga-card {
    background: #1a1a1a;
    border-radius: 8px;
    overflow: hidden;
    transition: transform 0.2s;
}

.manga-card:hover {
    transform: translateY(-2px);
}

.manga-card img {
    width: 100%;
    aspect-ratio: 3/4;
    object-fit: cover;
    display: block;
}

.manga-card .info {
    padding: 0.75rem;
}

.manga-card .info h3 {
    font-size: 0.95rem;
    margin-bottom: 0.25rem;
}

.manga-card .info .source-tag {
    font-size: 0.75rem;
    color: #888;
}

.manga-detail {
    display: flex;
    gap: 2rem;
    margin-bottom: 2rem;
}

.manga-detail img {
    width: 250px;
    border-radius: 8px;
    flex-shrink: 0;
}

.manga-detail .meta h2 {
    font-size: 1.75rem;
    margin-bottom: 0.5rem;
}

.manga-detail .meta .author {
    color: #aaa;
    margin-bottom: 1rem;
}

.manga-detail .meta .description {
    color: #ccc;
    font-size: 0.95rem;
}

.chapter-list {
    list-style: none;
}

.chapter-list li {
    border-bottom: 1px solid #222;
}

.chapter-list li a {
    display: block;
    padding: 0.75rem 1rem;
    color: #eee;
    text-decoration: none;
    transition: background 0.15s;
}

.chapter-list li a:hover {
    background: #1a1a1a;
}

.chapter-list li .chapter-num {
    font-weight: 600;
    color: #f59e0b;
}

.reader {
    text-align: center;
}

.reader img {
    max-width: 100%;
    display: block;
    margin: 0 auto;
}

.reader-nav {
    display: flex;
    justify-content: space-between;
    padding: 1rem 0;
    margin-bottom: 1rem;
}

.reader-nav a, .reader-nav span {
    padding: 0.5rem 1rem;
    color: #f59e0b;
    text-decoration: none;
    border: 1px solid #333;
    border-radius: 4px;
}

.reader-nav a:hover {
    background: #1a1a1a;
}

.reader-nav .current {
    color: #888;
    border-color: transparent;
}

.source-section {
    margin-bottom: 2rem;
}

.source-section h2 {
    margin-bottom: 1rem;
    color: #f59e0b;
}

footer {
    margin-top: 3rem;
    padding: 1.5rem 0;
    border-top: 1px solid #333;
    text-align: center;
    color: #666;
    font-size: 0.85rem;
}

a { color: #f59e0b; }
a:hover { color: #fbbf24; }

.source-list {
    margin: 2rem 0;
}

.source-list ul {
    list-style: none;
}

.source-list li {
    margin-bottom: 0.5rem;
}

.source-list li a {
    display: block;
    padding: 1rem;
    background: #1a1a1a;
    border-radius: 8px;
    text-decoration: none;
    font-size: 1.1rem;
}

.source-list li a:hover {
    background: #222;
}
```

- [ ] **Step 3: Write axum server skeleton**

`website/src/main.rs`:
```rust
mod templates;

use axum::{
    routing::{get},
    Router,
};
use mangaread_core::db;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::ServeDir;
use tracing_subscriber;

struct AppState {
    db: Mutex<rusqlite::Connection>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let conn = rusqlite::Connection::open("mangaread.db")?;
    db::init_db(&conn)?;

    let state = Arc::new(AppState { db: Mutex::new(conn) });

    let app = Router::new()
        .route("/", get(crate::routes::home::home))
        .route("/source/:source", get(crate::routes::source::source_list))
        .route("/manga/:id", get(crate::routes::manga::manga_detail))
        .route(
            "/read/:manga_id/:chapter_id",
            get(crate::routes::reader::chapter_reader),
        )
        .nest_service("/static", ServeDir::new("static"))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await?;

    Ok(())
}
```

Note: This won't compile yet since we haven't created the route modules. That's fine — next task adds them.

- [ ] **Step 4: Verify core + website compile (excluding missing routes)**

Run: `cargo check -p mangaread-core`
Expected: clean compilation

- [ ] **Step 5: Commit**

```bash
git add website/src/main.rs website/src/templates/ website/static/
git commit -m "feat(website): add server skeleton with layout and dark CSS"
```

---

### Task 5: Website — route handlers

**Files:**
- Create: `website/src/routes/mod.rs`
- Create: `website/src/routes/home.rs`
- Create: `website/src/routes/source.rs`
- Create: `website/src/routes/manga.rs`
- Create: `website/src/routes/reader.rs`

- [ ] **Step 1: Routes module**

`website/src/routes/mod.rs`:
```rust
pub mod home;
pub mod manga;
pub mod reader;
pub mod source;
```

- [ ] **Step 2: Home route — list sources**

`website/src/routes/home.rs`:
```rust
use axum::extract::State;
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    let db = state.db.lock().await;
    let sources = mangaread_core::db::list_manga_sources(&db).unwrap_or_default();

    let content = html! {
        h2 { "Sources" }
        @if sources.is_empty() {
            p { "No manga sources found. Run the scraper first!" }
        } @else {
            div class="source-list" {
                ul {
                    @for s in &sources {
                        li { a href={"/source/"(s)} { (s) } }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout("Home", content).0)
}
```

- [ ] **Step 3: Source route — manga list for a source**

`website/src/routes/source.rs`:
```rust
use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn source_list(
    State(state): State<Arc<AppState>>,
    Path(source): Path<String>,
) -> Html<String> {
    let db = state.db.lock().await;
    let manga_list =
        mangaread_core::db::list_manga_by_source(&db, &source).unwrap_or_default();

    let content = html! {
        h2 { "Manga from " (source) }
        @if manga_list.is_empty() {
            p { "No manga found for this source." }
        } @else {
            div class="manga-grid" {
                @for m in &manga_list {
                    div class="manga-card" {
                        a href={"/manga/"(m.id)} {
                            @if let Some(ref cover) = m.cover_url {
                                img src=(cover) alt=(m.title);
                            } @else {
                                div style="background:#333;aspect-ratio:3/4;display:flex;align-items:center;justify-content:center;color:#888;" {
                                    "No cover"
                                }
                            }
                            div class="info" {
                                h3 { (m.title) }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&source, content).0)
}
```

- [ ] **Step 4: Manga detail route**

`website/src/routes/manga.rs`:
```rust
use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn manga_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Html<String> {
    let db = state.db.lock().await;

    let manga = match mangaread_core::db::get_manga_by_id(&db, id) {
        Ok(m) => m,
        Err(_) => {
            let content = html! { h2 { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapters = mangaread_core::db::list_chapters_by_manga(&db, manga.id).unwrap_or_default();

    let content = html! {
        div class="manga-detail" {
            @if let Some(ref cover) = manga.cover_url {
                img src=(cover) alt=(manga.title);
            }
            div class="meta" {
                h2 { (manga.title) }
                @if let Some(ref author) = manga.author {
                    p class="author" { "by " (author) }
                }
                @if let Some(ref desc) = manga.description {
                    p class="description" { (desc) }
                }
                p { "Source: " (manga.source) }
            }
        }
        h3 { "Chapters" }
        @if chapters.is_empty() {
            p { "No chapters scraped yet." }
        } @else {
            ul class="chapter-list" {
                @for ch in &chapters {
                    li {
                        a href={"/read/"(manga.id)"/"(ch.id)} {
                            span class="chapter-num" { "Ch. " (ch.number) }
                            @if let Some(ref t) = ch.title {
                                span { " — " (t) }
                            }
                        }
                    }
                }
            }
        }
    };

    Html(crate::templates::layout::base_layout(&manga.title, content).0)
}
```

- [ ] **Step 5: Chapter reader route**

`website/src/routes/reader.rs`:
```rust
use axum::extract::{Path, State};
use axum::response::Html;
use maud::html;
use std::sync::Arc;

use crate::AppState;

pub async fn chapter_reader(
    State(state): State<Arc<AppState>>,
    Path((manga_id, chapter_id)): Path<(i64, i64)>,
) -> Html<String> {
    let db = state.db.lock().await;

    let manga = match mangaread_core::db::get_manga_by_id(&db, manga_id) {
        Ok(m) => m,
        Err(_) => {
            let content = html! { h2 { "Manga not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let chapter = match mangaread_core::db::get_chapter_by_id(&db, chapter_id) {
        Ok(c) => c,
        Err(_) => {
            let content = html! { h2 { "Chapter not found" } };
            return Html(crate::templates::layout::base_layout("Not found", content).0);
        }
    };

    let pages = mangaread_core::db::list_pages_by_chapter(&db, chapter.id).unwrap_or_default();

    // Fetch all chapters for prev/next nav
    let all_chapters =
        mangaread_core::db::list_chapters_by_manga(&db, manga_id).unwrap_or_default();
    let current_idx = all_chapters.iter().position(|c| c.id == chapter.id);
    let prev_chapter = current_idx.and_then(|i| i.checked_sub(1)).and_then(|i| all_chapters.get(i));
    let next_chapter = current_idx.and_then(|i| all_chapters.get(i + 1));

    let content = html! {
        div class="reader-nav" {
            @if let Some(prev) = prev_chapter {
                a href={"/read/"(manga_id)"/"(prev.id)} { "← Ch. " (prev.number) }
            } @else {
                span { "← First chapter" }
            }
            span class="current" { (manga.title) " — Ch. " (chapter.number) }
            @if let Some(next) = next_chapter {
                a href={"/read/"(manga_id)"/"(next.id)} { "Ch. " (next.number) " →" }
            } @else {
                span { "Last chapter →" }
            }
        }
        div class="reader" {
            @for page in &pages {
                img src=(page.image_url) alt={"Page "(page.index)};
            }
        }
        div class="reader-nav" {
            @if let Some(prev) = prev_chapter {
                a href={"/read/"(manga_id)"/"(prev.id)} { "← Ch. " (prev.number) }
            } @else {
                span { "← First chapter" }
            }
            span { (pages.len()) " pages" }
            @if let Some(next) = next_chapter {
                a href={"/read/"(manga_id)"/"(next.id)} { "Ch. " (next.number) " →" }
            } @else {
                span { "Last chapter →" }
            }
        }
    };

    Html(crate::templates::layout::base_layout(
        &format!("Ch. {} — {}", chapter.number, manga.title),
        content,
    ).0)
}
```

- [ ] **Step 6: Update main.rs to include routes module**

`website/src/main.rs` — add `mod routes;` after `mod templates;`:

```rust
mod routes;
mod templates;
```

- [ ] **Step 7: Verify website compiles**

Run: `cargo check -p website`
Expected: clean compilation

- [ ] **Step 8: Commit**

```bash
git add website/src/routes/ website/src/main.rs
git commit -m "feat(website): add all route handlers with maud templates"
```

---

### Task 6: Website — seed data + manual verification

**Files:**
- Modify: `website/src/main.rs` (add seed step)

- [ ] **Step 1: Add data seeding function**

Append to main.rs before `#[tokio::main]`:

```rust
fn seed_data(conn: &rusqlite::Connection) {
    // Only seed if DB is empty
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
        .unwrap_or(0);
    if count > 0 {
        return;
    }

    tracing::info!("seeding sample data...");

    let manga_id = db::insert_manga(
        conn,
        &mangaread_core::models::NewManga {
            source: "demonicscans".into(),
            source_id: "one-piece".into(),
            title: "One Piece".into(),
            author: Some("Eiichiro Oda".into()),
            cover_url: Some("https://example.com/cover.jpg".into()),
            description: Some("A pirate adventure manga.".into()),
            detail_url: "https://demonicscans.org/manga/one-piece".into(),
        },
    )
    .expect("insert manga");

    let ch1_id = db::insert_chapter(
        conn,
        &mangaread_core::models::NewChapter {
            manga_id,
            source_id: "ch-1".into(),
            number: 1.0,
            title: Some("Romance Dawn".into()),
            url: "https://demonicscans.org/manga/one-piece/ch-1".into(),
        },
    )
    .expect("insert chapter");

    for i in 1..=5 {
        db::insert_page(
            conn,
            &mangaread_core::models::NewPage {
                chapter_id: ch1_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/333/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    let ch2_id = db::insert_chapter(
        conn,
        &mangaread_core::models::NewChapter {
            manga_id,
            source_id: "ch-2".into(),
            number: 2.0,
            title: Some("That Guy".into()),
            url: "https://demonicscans.org/manga/one-piece/ch-2".into(),
        },
    )
    .expect("insert chapter");

    for i in 1..=3 {
        db::insert_page(
            conn,
            &mangaread_core::models::NewPage {
                chapter_id: ch2_id,
                index: i,
                image_url: format!("https://placehold.co/800x1200/444/f59e0b?text=Page+{}", i),
            },
        )
        .expect("insert page");
    }

    tracing::info!("seed data inserted: 1 manga, 2 chapters, 8 pages");
}
```

Call it in main after `init_db`:
```rust
seed_data(&conn);
```

- [ ] **Step 2: Verify and run**

Run: `cargo build -p website`
Expected: clean build

Run: `cargo run -p website` (then Ctrl+C to stop)
Expected: server starts on port 3000

- [ ] **Step 3: Commit**

```bash
git add website/src/main.rs
git commit -m "feat(website): add seed data for manual testing"
```

---

### Task 7: Scraper — SiteAdapter trait + engine

**Files:**
- Create: `scraper/src/lib.rs`
- Create: `scraper/src/adapter.rs`
- Create: `scraper/src/engine.rs`
- Create: `scraper/src/error.rs`

- [ ] **Step 1: Scraper lib + error**

`scraper/src/error.rs`:
```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Scraping failed: {0}")]
    Scrape(String),
    #[error("Database error: {0}")]
    Database(#[from] mangaread_core::error::Error),
    #[error("WebDriver error: {0}")]
    WebDriver(#[from] thirtyfour::error::WebDriverError),
}
```

`scraper/src/lib.rs`:
```rust
pub mod adapter;
pub mod engine;
pub mod error;
```

- [ ] **Step 2: Write SiteAdapter trait**

`scraper/src/adapter.rs`:
```rust
use async_trait::async_trait;
use mangaread_core::models::{ChapterListing, MangaInfo, MangaListing};

#[async_trait]
pub trait SiteAdapter: Send + Sync {
    /// Unique identifier for this source (e.g. "demonicscans").
    fn source_name(&self) -> &str;

    /// Base URL of the source site.
    fn base_url(&self) -> &str;

    /// 1. Crawl the main listing page → discover manga + their detail URLs.
    async fn discover_manga(&self, driver: &thirtyfour::WebDriver) -> anyhow::Result<Vec<MangaListing>>;

    /// 2. Visit a manga detail page → full info + chapter URLs.
    async fn fetch_manga(
        &self,
        driver: &thirtyfour::WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo>;

    /// 3. Visit a chapter page → extract ordered page image URLs.
    async fn fetch_pages(
        &self,
        driver: &thirtyfour::WebDriver,
        chapter_url: &str,
    ) -> anyhow::Result<Vec<String>>;
}
```

- [ ] **Step 3: Write ScraperEngine**

`scraper/src/engine.rs`:
```rust
use mangaread_core::db;
use mangaread_core::models::{NewChapter, NewManga, NewPage};
use thirtyfour::WebDriver;
use tracing;

use crate::adapter::SiteAdapter;

pub struct ScraperEngine {
    adapters: Vec<Box<dyn SiteAdapter>>,
    db: rusqlite::Connection,
}

impl ScraperEngine {
    pub fn new(db: rusqlite::Connection) -> Self {
        Self {
            adapters: Vec::new(),
            db,
        }
    }

    pub fn add_adapter(&mut self, adapter: Box<dyn SiteAdapter>) {
        self.adapters.push(adapter);
    }

    /// Run discover_manga for all adapters and upsert into DB.
    pub async fn scan_all(&mut self, driver: &WebDriver) -> anyhow::Result<()> {
        for adapter in &self.adapters {
            let source = adapter.source_name();
            tracing::info!("scanning {source} for manga...");

            let listings = adapter.discover_manga(driver).await?;
            tracing::info!("found {} manga on {source}", listings.len());

            for listing in listings {
                let new_manga = NewManga {
                    source: source.to_string(),
                    source_id: listing.detail_url.clone(),
                    title: listing.title,
                    author: None,
                    cover_url: listing.cover_url,
                    description: None,
                    detail_url: listing.detail_url,
                };
                db::insert_manga(&self.db, &new_manga)?;
            }
        }
        Ok(())
    }

    /// For a given manga ID, visit its detail page and upsert chapters.
    pub async fn sync_manga(&mut self, driver: &WebDriver, manga_id: i64) -> anyhow::Result<()> {
        let manga = db::get_manga_by_id(&self.db, manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!("syncing manga: {} ({})", manga.title, manga.detail_url);

        let info = adapter.fetch_manga(driver, &manga.detail_url).await?;

        // Update manga metadata
        let updated = NewManga {
            source: manga.source.clone(),
            source_id: manga.source_id.clone(),
            title: info.title,
            author: info.author,
            cover_url: info.cover_url,
            description: info.description,
            detail_url: info.detail_url,
        };
        db::insert_manga(&self.db, &updated)?;

        for ch in info.chapters {
            let new_chapter = NewChapter {
                manga_id,
                source_id: ch.url.clone(),
                number: ch.number,
                title: ch.title,
                url: ch.url,
            };
            db::insert_chapter(&self.db, &new_chapter)?;
        }

        tracing::info!(
            "synced {} chapters for manga {}",
            info.chapters.len(),
            manga_id
        );
        Ok(())
    }

    /// For a given chapter ID, visit its page and upsert page image URLs.
    pub async fn sync_chapter(&mut self, driver: &WebDriver, chapter_id: i64) -> anyhow::Result<()> {
        let chapter = db::get_chapter_by_id(&self.db, chapter_id)?;
        let manga = db::get_manga_by_id(&self.db, chapter.manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!(
            "syncing chapter: {} ch.{} ({})",
            manga.title,
            chapter.number,
            chapter.url
        );

        let page_urls = adapter.fetch_pages(driver, &chapter.url).await?;

        for (i, url) in page_urls.iter().enumerate() {
            let new_page = NewPage {
                chapter_id,
                index: (i + 1) as i32,
                image_url: url.clone(),
            };
            db::insert_page(&self.db, &new_page)?;
        }

        tracing::info!(
            "synced {} pages for chapter {}",
            page_urls.len(),
            chapter_id
        );
        Ok(())
    }

    pub fn into_db(self) -> rusqlite::Connection {
        self.db
    }
}
```

- [ ] **Step 4: Verify scraper compiles**

Run: `cargo check -p scraper`
Expected: clean compilation

- [ ] **Step 5: Commit**

```bash
git add scraper/src/lib.rs scraper/src/adapter.rs scraper/src/engine.rs scraper/src/error.rs
git commit -m "feat(scraper): add SiteAdapter trait and ScraperEngine"
```

---

### Task 8: Scraper — demonicscans.org adapter

**Files:**
- Create: `scraper/src/adapters/mod.rs`
- Create: `scraper/src/adapters/demonicscans.rs`

- [ ] **Step 1: Adapters module**

`scraper/src/adapters/mod.rs`:
```rust
pub mod demonicscans;
```

- [ ] **Step 2: Add adapters module to lib.rs**

Add to `scraper/src/lib.rs`:
```rust
pub mod adapters;
```

- [ ] **Step 3: Write demonicscans adapter**

`scraper/src/adapters/demonicscans.rs`:
```rust
use async_trait::async_trait;
use mangaread_core::models::{ChapterListing, MangaInfo, MangaListing};
use scraper::{Html, Selector};
use thirtyfour::prelude::*;

use crate::adapter::SiteAdapter;

pub struct DemonicscansAdapter;

#[async_trait]
impl SiteAdapter for DemonicscansAdapter {
    fn source_name(&self) -> &str {
        "demonicscans"
    }

    fn base_url(&self) -> &str {
        "https://demonicscans.org"
    }

    async fn discover_manga(&self, driver: &WebDriver) -> anyhow::Result<Vec<MangaListing>> {
        driver.goto("https://demonicscans.org/index.php").await?;

        // Wait for the page to load
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let page_source = driver.source().await?;
        let document = Html::parse_document(&page_source);

        // These selectors will need adjustment for the actual site structure.
        // Common patterns: manga entry links in a grid/list on the index page.
        let link_selector = Selector::parse("a[href*='/manga/']").unwrap();
        let img_selector = Selector::parse("img").unwrap();

        let mut manga = Vec::new();
        for element in document.select(&link_selector) {
            let href = element.value().attr("href").unwrap_or("");
            // Skip non-manga links (e.g., /, /contact, etc.)
            if href.len() < 10 || href == "/" || !href.contains("/manga/") {
                continue;
            }

            // Make full URL
            let detail_url = if href.starts_with("http") {
                href.to_string()
            } else {
                format!("https://demonicscans.org{}", href)
            };

            // Try to get title from element text or alt text of child img
            let title = element.text().collect::<String>().trim().to_string();
            let title = if title.is_empty() {
                // Fallback: try alt from embedded image
                element
                    .select(&img_selector)
                    .next()
                    .and_then(|img| img.value().attr("alt"))
                    .unwrap_or("Unknown")
                    .to_string()
            } else {
                title
            };

            let cover_url = element
                .select(&img_selector)
                .next()
                .and_then(|img| img.value().attr("src"))
                .map(|s| {
                    if s.starts_with("http") {
                        s.to_string()
                    } else {
                        format!("https://demonicscans.org{}", s)
                    }
                });

            manga.push(MangaListing {
                title,
                detail_url,
                cover_url,
            });
        }

        Ok(manga)
    }

    async fn fetch_manga(
        &self,
        driver: &WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo> {
        driver.goto(detail_url).await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let page_source = driver.source().await?;
        let document = Html::parse_document(&page_source);

        // Extract title from the page
        let title_sel = Selector::parse("h1").unwrap();
        let title = document
            .select(&title_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        // Extract description
        let desc_sel = Selector::parse("meta[name='description']").unwrap();
        let description = document
            .select(&desc_sel)
            .next()
            .and_then(|e| e.value().attr("content"))
            .map(|s| s.to_string());

        // Extract author
        let author_sel = Selector::parse(".author, .artist, [class*='author']").unwrap();
        let author = document
            .select(&author_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        // Extract cover image
        let cover_sel = Selector::parse("img[class*='cover'], .manga-cover img, img[class*='thumbnail']").unwrap();
        let cover_url = document
            .select(&cover_sel)
            .next()
            .and_then(|e| e.value().attr("src"))
            .map(|s| {
                if s.starts_with("http") {
                    s.to_string()
                } else {
                    format!("https://demonicscans.org{}", s)
                }
            });

        // Extract chapter links
        let ch_sel = Selector::parse("a[href*='/manga/'][href*='chapter'], .chapter-list a, a[href*='capitulo']").unwrap();
        let chapters: Vec<ChapterListing> = document
            .select(&ch_sel)
            .filter_map(|e| {
                let href = e.value().attr("href")?;
                let text = e.text().collect::<String>().trim().to_string();
                let url = if href.starts_with("http") {
                    href.to_string()
                } else {
                    format!("https://demonicscans.org{}", href)
                };

                // Try to extract chapter number from text/URL
                let number = extract_chapter_number(&text)
                    .or_else(|| extract_chapter_number(&url))
                    .unwrap_or(0.0);

                Some(ChapterListing {
                    title: if text.is_empty() { None } else { Some(text) },
                    number,
                    url,
                })
            })
            .collect();

        Ok(MangaInfo {
            title,
            author,
            description,
            cover_url,
            detail_url: detail_url.to_string(),
            chapters,
        })
    }

    async fn fetch_pages(
        &self,
        driver: &WebDriver,
        chapter_url: &str,
    ) -> anyhow::Result<Vec<String>> {
        driver.goto(chapter_url).await?;
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        // Try common reader page patterns
        // Many readers load images via JavaScript or in <img>/<source> tags
        let page_source = driver.source().await?;
        let document = Html::parse_document(&page_source);

        // Try multiple selector patterns for chapter images
        let img_sel = Selector::parse(
            "img[class*='page-image'], .reader-img img, #reader img, .chapter-content img, img[alt*='page']"
        ).unwrap();

        let mut pages: Vec<String> = Vec::new();
        for img in document.select(&img_sel) {
            if let Some(src) = img.value().attr("src").or_else(|| img.value().attr("data-src")) {
                let url = if src.starts_with("http") {
                    src.to_string()
                } else {
                    format!("https://demonicscans.org{}", src)
                };
                pages.push(url);
            }
        }

        // Fallback: try to get all <img> tags on the page
        if pages.is_empty() {
            let all_img = Selector::parse("img").unwrap();
            for img in document.select(&all_img) {
                if let Some(src) = img.value().attr("src") {
                    // Skip icons, logos, etc.
                    if src.contains("logo") || src.contains("icon") || src.contains("avatar") {
                        continue;
                    }
                    let url = if src.starts_with("http") {
                        src.to_string()
                    } else {
                        format!("https://demonicscans.org{}", src)
                    };
                    pages.push(url);
                }
            }
        }

        Ok(pages)
    }
}

fn extract_chapter_number(s: &str) -> Option<f64> {
    // Matches patterns like "Chapter 1", "Ch. 1.5", "capitulo 100", "chapter-1"
    let re = regex_lite::Regex::new(r"(?i)(?:chapter|ch\.?|capitulo)\s*(\d+(?:\.\d+)?)").ok()?;
    if let Some(caps) = re.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse::<f64>().ok();
        }
    }
    // Fallback: try to find standalone number in URL
    let re2 = regex_lite::Regex::new(r"[_-](\d+(?:\.\d+)?)$").ok()?;
    if let Some(caps) = re2.captures(s) {
        if let Some(m) = caps.get(1) {
            return m.as_str().parse::<f64>().ok();
        }
    }
    None
}
```

- [ ] **Step 4: Add regex-lite dependency to scraper/Cargo.toml**

Add `regex-lite = "0.1"` to `scraper/Cargo.toml` dependencies.

- [ ] **Step 5: Add scraper crate dependency for HTML parsing**

Add `scraper = "0.21"` to `scraper/Cargo.toml` dependencies.

- [ ] **Step 6: Verify scraper compiles**

Run: `cargo check -p scraper`
Expected: clean compilation

- [ ] **Step 7: Commit**

```bash
git add scraper/src/adapters/ scraper/src/lib.rs scraper/Cargo.toml
git commit -m "feat(scraper): add demonicscans.org SiteAdapter"
```

---

### Task 9: Scraper — CLI entry point

**Files:**
- Modify: `scraper/src/main.rs`

- [ ] **Step 1: Write CLI main with clap**

`scraper/src/main.rs`:
```rust
use clap::{Parser, Subcommand};
use mangaread_core::db;
use scraper::adapters::demonicscans::DemonicscansAdapter;
use scraper::engine::ScraperEngine;
use thirtyfour::prelude::*;
use tracing_subscriber;

#[derive(Parser)]
#[command(name = "manga-scraper")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scan all adapters for manga listings
    Scan,
    /// Fetch details + chapters for a manga
    SyncManga {
        /// Manga ID in the database
        id: i64,
    },
    /// Fetch page images for a chapter
    SyncChapter {
        /// Chapter ID in the database
        id: i64,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    // Connect to WebDriver (expects chromedriver/geckodriver on localhost:4444)
    let caps = DesiredCapabilities::chrome();
    let driver = WebDriver::new("http://localhost:4444", caps).await?;

    // Open DB
    let conn = rusqlite::Connection::open("mangaread.db")?;
    db::init_db(&conn)?;

    let mut engine = ScraperEngine::new(conn);
    engine.add_adapter(Box::new(DemonicscansAdapter));

    match cli.command {
        Commands::Scan => {
            engine.scan_all(&driver).await?;
            let db = engine.into_db();
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM manga", [], |row| row.get(0))
                .unwrap_or(0);
            tracing::info!("scan complete. {} manga in database.", count);
        }
        Commands::SyncManga { id } => {
            engine.sync_manga(&driver, id).await?;
            tracing::info!("manga {} synced.", id);
        }
        Commands::SyncChapter { id } => {
            engine.sync_chapter(&driver, id).await?;
            tracing::info!("chapter {} synced.", id);
        }
    }

    driver.quit().await?;
    Ok(())
}
```

- [ ] **Step 2: Verify scraper compiles**

Run: `cargo check -p scraper`
Expected: clean compilation

- [ ] **Step 3: Commit**

```bash
git add scraper/src/main.rs
git commit -m "feat(scraper): add CLI with scan/sync-manga/sync-chapter commands"
```

---

### Task 10: Final workspace check + README

**Files:**
- Create: `.gitignore` (root)

- [ ] **Step 1: Create root .gitignore**

Path: `/Users/yanis/Programming/mangaread/.gitignore`
```
target/
mangaread.db
*.swp
.DS_Store
```

Note: The existing `.gitignore` files in `scraper/` and `website/` already cover `/target` for those sub-crates. This root one catches anything else.

- [ ] **Step 2: Full workspace build**

Run: `cargo build --workspace`
Expected: all 3 crates compile without warnings

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: add root .gitignore"
```

---

## Verification

After all tasks are complete:

1. `cargo build --workspace` — all crates compile
2. Start WebDriver: `chromedriver --port=4444` (requires Chrome + ChromeDriver installed)
3. Run `cargo run -p website` — server starts on :3000, seed data shows manga + chapters
4. Run `cargo run -p scraper scan` — scans demonicscans.org, inserts manga into DB
5. Run `cargo run -p scraper sync-manga 1` — fetches chapters for first manga
6. Run `cargo run -p scraper sync-chapter 1` — fetches page images for first chapter
7. Browse `http://localhost:3000` — see manga, click through to reader
