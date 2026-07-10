//! Background scraper helpers that spawn chromedriver-based sync tasks.
//!
//! Each sync runs on a dedicated OS thread with its own tokio runtime to
//! work around rusqlite's `!Send` constraint on `&Connection`.

use std::sync::Arc;
use std::time::Duration;
use thirtyfour::manager::BrowserKind;
use thirtyfour::ChromiumLikeCapabilities;

use crate::AppState;

/// How often (in seconds) to re-scan and re-sync.
const SYNC_INTERVAL_SECS: u64 = 6 * 3600; // 6 hours
/// Number of concurrent WebDriver sessions for parallel syncing.
const MAX_CONCURRENT_SYNCS: usize = 3;
/// Staleness threshold for incremental re-sync (minutes).
const STALE_MINUTES: i64 = 120;

// ── Chrome / WebDriver helpers ──

/// Create Chrome capabilities, applying any extra CLI flags from
/// the `CHROMIUM_FLAGS` environment variable (e.g. for Docker).
pub(crate) fn build_chrome_caps() -> thirtyfour::ChromeCapabilities {
    let mut caps = thirtyfour::DesiredCapabilities::chrome();
    if let Ok(flags) = std::env::var("CHROMIUM_FLAGS") {
        for flag in flags.split_whitespace() {
            let _ = caps.add_arg(flag);
        }
    }
    caps
}

fn find_chromedriver() -> String {
    if let Ok(path) = std::env::var("CHROMEDRIVER_PATH") {
        return path;
    }
    if let Ok(out) = std::process::Command::new("which")
        .arg("chromedriver")
        .output()
    {
        if out.status.success() {
            if let Ok(path) = String::from_utf8(out.stdout) {
                let trimmed = path.trim().to_string();
                if !trimmed.is_empty() {
                    return trimmed;
                }
            }
        }
    }
    "/usr/bin/chromedriver".to_string()
}

pub(crate) async fn create_webdriver(
    caps: thirtyfour::ChromeCapabilities,
) -> thirtyfour::error::WebDriverResult<thirtyfour::WebDriver> {
    let chromedriver_path = find_chromedriver();
    thirtyfour::manager::WebDriverManager::builder()
        .driver_binary(BrowserKind::Chrome, chromedriver_path)
        .offline()
        .build()
        .launch(caps)
        .await
}

// ── DB connection ──

/// Open a dedicated read-write connection for a background scraper task.
pub(crate) fn open_db() -> rusqlite::Result<rusqlite::Connection> {
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
pub(crate) fn build_engine() -> scraper::engine::ScraperEngine {
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

        let driver = match create_webdriver(build_chrome_caps()).await {
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

        let driver = match create_webdriver(build_chrome_caps()).await {
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

        let driver = match create_webdriver(build_chrome_caps()).await {
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

/// Sync a single manga using a fresh chromedriver session.
///
/// Opens its own WebDriver, syncs the manga, quits the driver.
/// Reports success/failure to [`AppState::adapter_health`].
async fn sync_single_manga(manga_id: i64, state: Arc<AppState>) {
    // Look up source name so we can track per-source health
    let source = match open_db() {
        Ok(ref db) => match mangaread_core::db::get_manga_by_id(db, manga_id) {
            Ok(m) => m.source,
            Err(e) => {
                tracing::error!("sync worker: failed to look up manga {manga_id}: {e}");
                return;
            }
        },
        Err(e) => {
            tracing::error!("sync worker: DB error for manga {manga_id}: {e}");
            return;
        }
    };

    // Helper: record a failure for this source and log a warning
    async fn record_failure(state: &AppState, src: &str, msg: &str) {
        let mut h = state.adapter_health.lock().await;
        if let Some(entry) = h.get_mut(src) {
            if entry.healthy {
                tracing::warn!("adapter {} is now UNHEALTHY: {msg}", src);
            }
            entry.healthy = false;
            entry.consecutive_failures += 1;
            entry.last_error = Some(msg.to_string());
        }
    }

    // Helper: record a success for this source, log recovery
    async fn record_success(state: &AppState, src: &str) {
        let mut h = state.adapter_health.lock().await;
        if let Some(entry) = h.get_mut(src) {
            if !entry.healthy {
                tracing::info!("adapter {} recovered after {} failures", src, entry.consecutive_failures);
            }
            entry.healthy = true;
            entry.consecutive_failures = 0;
            entry.last_error = None;
        }
    }

    let engine = build_engine();
    let driver = match create_webdriver(build_chrome_caps()).await {
        Ok(d) => d,
        Err(e) => {
            let msg = format!("WebDriver error: {e}");
            tracing::error!("sync worker: {msg}");
            record_failure(&state, &source, &msg).await;
            return;
        }
    };
    let db = match open_db() {
        Ok(db) => db,
        Err(e) => {
            let msg = format!("DB error: {e}");
            tracing::error!("sync worker: {msg}");
            let _ = driver.quit().await;
            record_failure(&state, &source, &msg).await;
            return;
        }
    };

    if let Err(e) = engine.sync_manga(&driver, &db, manga_id).await {
        let msg = format!("Sync failed: {e}");
        tracing::error!("sync worker: {msg}");
        record_failure(&state, &source, &msg).await;
    } else {
        record_success(&state, &source).await;
    }

    let _ = driver.quit().await;
}

/// Start the automatic sync loop in the background.
///
/// # Flow
/// 1. **Scan** — one chromedriver session discovers manga listings from all sources
/// 2. **Sync** — finds manga that need syncing (unsynced or stale), syncs them in
///    parallel with up to [`MAX_CONCURRENT_SYNCS`] concurrent OS threads
/// 3. **Sleep** — waits [`SYNC_INTERVAL_SECS`], then repeats
///
/// Each manga sync gets its own chromedriver + DB connection on a dedicated OS
/// thread, avoiding rusqlite's `!Send` constraint.  Progress is reported through
/// [`AppState::auto_sync`] and exposed via the `/api/sync-status` endpoint.
pub fn start_sync_loop(state: Arc<AppState>) {
    spawn_scraper(move || async move {
        loop {
            // ── 1. Scan sources (single driver) ──
            tracing::info!("sync loop: scanning sources...");
            let engine = build_engine();

            let scan_driver = match create_webdriver(build_chrome_caps()).await {
                Ok(d) => d,
                Err(e) => {
                    tracing::error!("sync loop: scan driver failed: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    continue;
                }
            };

            let scan_db = match open_db() {
                Ok(db) => db,
                Err(e) => {
                    tracing::error!("sync loop: scan DB failed: {:?}", e);
                    let _ = scan_driver.quit().await;
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    continue;
                }
            };

            let _ = engine.scan_all(&scan_driver, &scan_db).await;
            let _ = scan_driver.quit().await;

            // ── 2. Get manga IDs needing sync ──
            let manga_ids = match mangaread_core::db::list_manga_ids_needing_sync(&scan_db, STALE_MINUTES) {
                Ok(ids) => ids,
                Err(e) => {
                    tracing::error!("sync loop: DB query failed: {:?}", e);
                    tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SECS)).await;
                    continue;
                }
            };

            let total = manga_ids.len();
            if total == 0 {
                tracing::info!("sync loop: nothing to sync, sleeping {SYNC_INTERVAL_SECS}s");
                tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SECS)).await;
                continue;
            }

            tracing::info!(
                "sync loop: syncing {total} manga (max {MAX_CONCURRENT_SYNCS} concurrent)..."
            );

            {
                let mut p = state.auto_sync.lock().await;
                p.running = true;
                p.total = total as i64;
                p.current = 0;
            }

            // ── 3. Sync in parallel on OS threads (avoids rusqlite !Send issue) ──
            let completed = Arc::new(std::sync::atomic::AtomicI64::new(0));

            // Process in chunks so at most MAX_CONCURRENT_SYNCS threads exist at once
            for chunk in manga_ids.chunks(MAX_CONCURRENT_SYNCS) {
                let mut thread_handles = Vec::with_capacity(chunk.len());

                for manga_id in chunk {
                    let state_clone = state.clone();
                    let completed_clone = completed.clone();
                    let mid = *manga_id;

                    let handle = std::thread::spawn(move || {
                        let rt = tokio::runtime::Runtime::new().expect("sync worker rt");
                        rt.block_on(async move {
                            sync_single_manga(mid, state_clone.clone()).await;

                            let done = completed_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                            let mut p = state_clone.auto_sync.lock().await;
                            p.current = done;
                        });
                    });

                    thread_handles.push(handle);
                }

                // Wait for this chunk before launching the next
                for h in thread_handles {
                    let _ = h.join();
                }
            }

            {
                let mut p = state.auto_sync.lock().await;
                p.running = false;
            }

            tracing::info!(
                "sync loop: pass complete ({total} synced), sleeping {SYNC_INTERVAL_SECS}s"
            );
            tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SECS)).await;
        }
    });
}
