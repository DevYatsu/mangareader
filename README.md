# MangaRead

A Rust training project — manga reader that scrapes chapters from third-party sites and serves them through a local web interface.

**Stack:** Axum (web) + Maud (templates) + Thirtyfour (Selenium WebDriver) + SQLite

## Project Structure

```
mangaread-core/    Shared library — domain types, SQLite schema + CRUD
scraper/           Binary — thirtyfour-based manga scraper with configurable SiteAdapter
website/           Binary — axum + maud web server for browsing manga
```

## Prerequisites

### Rust toolchain
```bash
rustup update
```

### WebDriver (for the scraper)

The scraper uses Selenium WebDriver to render JavaScript-heavy manga sites. Install one of:

**ChromeDriver:**
```bash
brew install chromedriver
# or download from https://chromedriver.chromium.org/
```

**GeckoDriver (Firefox):**
```bash
brew install geckodriver
```

## Running the Website

The website serves manga and chapters from the local SQLite database. It includes seed data for testing.

```bash
# Start the server (with seed data if DB is empty)
cargo run -p website
```

Open **http://localhost:3000** in your browser. With seed data you'll see "One Piece" with 2 chapters and placeholder page images.

The server listens on `0.0.0.0:3000` and stores the database at `./mangaread.db`.

## Running the Scraper

The scraper navigates manga sites via WebDriver, extracts manga listings, chapters, and page images, then saves everything to SQLite.

### 1. Start WebDriver

In a separate terminal:

```bash
# If using Chrome (default port used by the scraper)
chromedriver --port=9515

# If using geckodriver (Firefox) — override URL with WEBDRIVER_URL
geckodriver --port=4444
```

### 2. Discover manga from source sites

Scans the index page of each configured source and saves manga entries to the database:

```bash
cargo run -p scraper scan
```

Example output:
```
2026-07-09T16:00:00.123Z  INFO scanning demonicscans for manga...
2026-07-09T16:00:02.456Z  INFO found 42 manga on demonicscans
2026-07-09T16:00:02.789Z  INFO scan complete. 42 manga in database.
```

### 3. Sync a manga's chapters

Visit a specific manga's detail page, extract chapter list and update metadata:

```bash
cargo run -p scraper sync-manga <id>
```

Replace `<id>` with the manga's database ID (shown from the scan output or by checking the DB).

Example output:
```
2026-07-09T16:01:00.123Z  INFO syncing manga: One Piece (https://demonicscans.org/manga/one-piece)
2026-07-09T16:01:05.456Z  INFO synced 200 chapters for manga 1
```

### 4. Sync a chapter's page images

Visit a chapter's reader page and extract all page image URLs:

```bash
cargo run -p scraper sync-chapter <id>
```

Replace `<id>` with the chapter's database ID.

Example output:
```
2026-07-09T16:02:00.123Z  INFO syncing chapter: One Piece ch.1 (https://demonicscans.org/manga/one-piece/ch-1)
2026-07-09T16:02:05.456Z  INFO synced 15 pages for chapter 1
```

### Full workflow

```bash
# Terminal 1: Start WebDriver
chromedriver --port=9515

# Terminal 2: Scrape + serve
cargo run -p scraper scan
cargo run -p scraper sync-manga 1
cargo run -p scraper sync-chapter 1
cargo run -p website
```

Then browse to http://localhost:3000, click through to the reader, and view the scraped images.

## Adding a New Source Site

Implement the `SiteAdapter` trait in a new file under `scraper/src/adapters/`:

```rust
#[async_trait]
impl SiteAdapter for MyCustomAdapter {
    fn source_name(&self) -> &str { "my-site" }
    fn base_url(&self) -> &str { "https://my-site.org" }

    async fn discover_manga(&self, driver: &WebDriver) -> anyhow::Result<Vec<MangaListing>> { /* ... */ }
    async fn fetch_manga(&self, driver: &WebDriver, detail_url: &str) -> anyhow::Result<MangaInfo> { /* ... */ }
    async fn fetch_pages(&self, driver: &WebDriver, chapter_url: &str) -> anyhow::Result<Vec<String>> { /* ... */ }
}
```

Then register it in `scraper/src/main.rs`:
```rust
engine.add_adapter(Box::new(MyCustomAdapter));
```

## License

Training project — use freely.
