//! Background scraper helpers that spawn chromedriver-based sync tasks.
//!
//! Each sync runs on a dedicated OS thread with its own tokio runtime to
//! work around rusqlite's `!Send` constraint on `&Connection`.

use thirtyfour::ChromiumLikeCapabilities;

// ── Chrome / WebDriver helpers ──

/// Create Chrome capabilities, applying any extra CLI flags from
/// the `CHROMIUM_FLAGS` environment variable (e.g. for Docker).
fn build_chrome_caps() -> thirtyfour::ChromeCapabilities {
    let mut caps = thirtyfour::DesiredCapabilities::chrome();
    if let Ok(flags) = std::env::var("CHROMIUM_FLAGS") {
        for flag in flags.split_whitespace() {
            let _ = caps.add_arg(flag);
        }
    }
    caps
}

// ── DB connection ──

/// Open a dedicated read-write connection for a background scraper task.
fn open_db() -> rusqlite::Result<rusqlite::Connection> {
    let path = mangaread_core::db::database_path();
    let conn = rusqlite::Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=10000;")?;
    Ok(conn)
}

// ── Scraper runner ──

/// Spawn a scraper future on a dedicated OS thread.
///
/// We use `std::thread::spawn` + a private tokio runtime because the scraper
/// methods hold `&Connection` across `.await` points, and `&Connection` is
/// not `Send` (rusqlite uses `RefCell` internally).
fn spawn_scraper<F, R>(f: F)
where
    F: FnOnce() -> R + Send + 'static,
    R: std::future::Future<Output = ()> + 'static,
{
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("bg rt");
        rt.block_on(f());
    });
}

/// Build an engine with all registered adapters.
fn build_engine() -> scraper::engine::ScraperEngine {
    let mut engine = scraper::engine::ScraperEngine::new();
    engine.add_adapter(Box::new(
        scraper::adapters::demonicscans::DemonicscansAdapter,
    ));
    engine.add_adapter(Box::new(
        scraper::adapters::manhuaplus::ManhuaplusAdapter,
    ));
    engine
}

// ── Public API ──

/// Sync a manga's chapters in the background.
///
/// Each task creates its own chromedriver and quits it when done.
pub fn sync_manga_background(manga_id: i64) {
    spawn_scraper(move || async move {
        tracing::info!("bg sync: manga {} starting...", manga_id);

        let engine = build_engine();

        let driver = match thirtyfour::WebDriver::managed(build_chrome_caps()).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("bg sync: failed to create WebDriver: {:?}", e);
                return;
            }
        };

        let db = match open_db() {
            Ok(db) => db,
            Err(e) => {
                tracing::error!("bg sync: failed to open DB: {:?}", e);
                let _ = driver.quit().await;
                return;
            }
        };

        if let Err(e) = engine.sync_manga(&driver, &db, manga_id).await {
            tracing::error!("bg sync: manga {} failed: {:?}", manga_id, e);
        }

        // Properly close the browser session
        if let Err(e) = driver.quit().await {
            tracing::warn!("bg sync: driver quit error: {:?}", e);
        }

        tracing::info!("bg sync: manga {} complete", manga_id);
    });
}

/// Sync a chapter's pages in the background.
///
/// Each task creates its own chromedriver and quits it when done.
/// Spawned on a dedicated OS thread with its own tokio runtime.
pub fn sync_chapter_background(chapter_id: i64) {
    spawn_scraper(move || async move {
        tracing::info!("bg sync: chapter {} starting...", chapter_id);

        let engine = build_engine();

        let driver = match thirtyfour::WebDriver::managed(build_chrome_caps()).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("bg sync: failed to create WebDriver: {:?}", e);
                return;
            }
        };

        let db = match open_db() {
            Ok(db) => db,
            Err(e) => {
                tracing::error!("bg sync: failed to open DB: {:?}", e);
                let _ = driver.quit().await;
                return;
            }
        };

        if let Err(e) = engine.sync_chapter(&driver, &db, chapter_id).await {
            tracing::error!("bg sync: chapter {} failed: {:?}", chapter_id, e);
        }

        // Properly close the browser session
        if let Err(e) = driver.quit().await {
            tracing::warn!("bg sync: driver quit error: {:?}", e);
        }

        tracing::info!("bg sync: chapter {} complete", chapter_id);
    });
}

/// Scan all sources for manga listings at startup, discovering new series.
///
/// Spawned as a background task so the server can start accepting requests
/// immediately.  Runs `scan_all` on each configured adapter.
pub fn scan_background() {
    spawn_scraper(|| async move {
        tracing::info!("startup scan: starting...");

        let engine = build_engine();

        let driver = match thirtyfour::WebDriver::managed(build_chrome_caps()).await {
            Ok(d) => d,
            Err(e) => {
                tracing::error!("startup scan: failed to create WebDriver: {:?}", e);
                return;
            }
        };

        let db = match open_db() {
            Ok(db) => db,
            Err(e) => {
                tracing::error!("startup scan: failed to open DB: {:?}", e);
                let _ = driver.quit().await;
                return;
            }
        };

        if let Err(e) = engine.scan_all(&driver, &db).await {
            tracing::error!("startup scan: scan failed: {:?}", e);
        }

        if let Err(e) = driver.quit().await {
            tracing::warn!("startup scan: driver quit error: {:?}", e);
        }

        tracing::info!("startup scan: complete");
    });
}
