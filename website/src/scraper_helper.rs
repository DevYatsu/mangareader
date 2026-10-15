//! Background scraper helpers that spawn chromedriver-based sync tasks.
//!
//! Each sync runs on a dedicated OS thread with its own tokio runtime to
//! work around rusqlite's `!Send` constraint on `&Connection`.

use std::sync::Arc;
use std::time::Duration;
use thirtyfour::ChromiumLikeCapabilities;
use thirtyfour::manager::BrowserKind;

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
    let which = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    if let Ok(out) = std::process::Command::new(which)
        .arg("chromedriver")
        .output()
    {
        if out.status.success() {
            if let Ok(path) = String::from_utf8(out.stdout) {
                for line in path.lines() {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        return trimmed.to_string();
                    }
                }
            }
        }
    }
    let default = if cfg!(target_os = "windows") {
        "chromedriver.exe"
    } else {
        "/usr/bin/chromedriver"
    };
    default.to_string()
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

/// Build an engine over every registered source.
pub(crate) fn build_engine() -> scraper::engine::ScraperEngine {
    scraper::engine::ScraperEngine::from_adapters(scraper::adapters::all())
}

/// Names of every registered source, in scan order.
pub(crate) fn source_names() -> Vec<String> {
    build_engine().source_names()
}

/// `(name, base URL)` of every registered source, for the admin list.
pub(crate) fn source_list() -> Vec<(String, String)> {
    scraper::adapters::all()
        .iter()
        .map(|a| (a.source_name().to_string(), a.base_url().to_string()))
        .collect()
}

/// `site_settings` key holding the comma-separated disabled source names.
pub(crate) const SOURCES_DISABLED_KEY: &str = "sources_disabled";

/// Sources an admin switched off. They are not scanned or synced.
pub(crate) fn disabled_sources(conn: &rusqlite::Connection) -> Vec<String> {
    mangaread_core::db::get_setting(conn, SOURCES_DISABLED_KEY)
        .ok()
        .flatten()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}

// ── Tracked jobs ──

/// One unit of scraping work. Every job is recorded in `sync_runs`, so the
/// admin pages show its outcome.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Job {
    /// Discover listings on one source.
    Scan(String),
    /// Fetch one manga's metadata and chapters.
    Manga(i64),
    /// Fetch one chapter's page URLs.
    Chapter(i64),
}

impl Job {
    fn kind(&self) -> mangaread_core::db::SyncKind {
        use mangaread_core::db::SyncKind;
        match self {
            Job::Scan(_) => SyncKind::Scan,
            Job::Manga(_) => SyncKind::Manga,
            Job::Chapter(_) => SyncKind::Chapter,
        }
    }

    fn target_id(&self) -> Option<i64> {
        match self {
            Job::Scan(_) => None,
            Job::Manga(id) | Job::Chapter(id) => Some(*id),
        }
    }

    /// The source this job scrapes, for the run record.
    fn source(&self, db: &rusqlite::Connection) -> Result<String, String> {
        let manga_source = |id: i64| {
            mangaread_core::db::get_manga_by_id(db, id)
                .map(|m| m.source)
                .map_err(|e| e.to_string())
        };
        match self {
            Job::Scan(source) => Ok(source.clone()),
            Job::Manga(id) => manga_source(*id),
            Job::Chapter(id) => mangaread_core::db::get_chapter_by_id(db, *id)
                .map_err(|e| e.to_string())
                .and_then(|c| manga_source(c.manga_id)),
        }
    }
}

/// Run one job with its own browser and DB connection, recording it in
/// `sync_runs`. Returns the item count or the error message.
///
/// Must run on a scraper thread (see [`spawn_scraper`]): the engine holds a
/// `!Send` `&Connection` across awaits.
pub(crate) async fn run_job(job: Job) -> Result<usize, String> {
    let db = open_db().map_err(|e| format!("DB error: {e}"))?;
    let source = job.source(&db)?;
    // Not recorded as a run: nothing was attempted.
    if disabled_sources(&db).contains(&source) {
        tracing::debug!("{job:?} skipped: source {source} is disabled");
        return Err(format!("source {source} is disabled"));
    }
    let run = mangaread_core::db::start_sync_run(&db, &source, job.kind(), job.target_id())
        .map_err(|e| format!("DB error: {e}"))?;
    let outcome = async {
        let engine = build_engine();
        let driver = create_webdriver(build_chrome_caps())
            .await
            .map_err(|e| format!("WebDriver error: {e}"))?;
        let result = match &job {
            Job::Scan(source) => engine.scan_source(source, &driver, &db).await,
            Job::Manga(id) => engine.sync_manga(&driver, &db, *id).await,
            Job::Chapter(id) => engine.sync_chapter(&driver, &db, *id).await,
        };
        if let Err(e) = driver.quit().await {
            tracing::warn!("driver quit error: {e:?}");
        }
        result.map_err(|e| format!("{e:#}"))
    }
    .await;
    match &outcome {
        Ok(n) => tracing::info!("{job:?} on {source}: ok ({n} items)"),
        Err(e) => tracing::error!("{job:?} on {source}: {e}"),
    }
    let recorded = outcome.as_ref().map(|n| *n as i64).map_err(String::as_str);
    if let Err(e) = mangaread_core::db::finish_sync_run(&db, run, recorded) {
        tracing::error!("could not record sync run {run}: {e}");
    }
    // Scans add series; make them searchable right away.
    if matches!(job, Job::Scan(_)) && outcome.is_ok() {
        refresh_search_index(&db);
    }
    outcome
}

/// Rebuild the search index, logging instead of failing: a stale index only
/// costs search freshness until the next rebuild.
pub(crate) fn refresh_search_index(db: &rusqlite::Connection) {
    if let Err(e) = mangaread_core::db::rebuild_search_index(db) {
        tracing::error!("search index rebuild failed: {e}");
    }
}

// ── Public API ──

/// Cap on concurrent page-view-triggered syncs. Each one runs a headless
/// browser, so the cap bounds memory no matter how many pages visitors open.
/// It matches the sync loop's 3 parallel drivers.
const MAX_ON_DEMAND_SYNCS: usize = 3;

/// Syncs started by page views, keyed by `(kind, id)`.
static IN_FLIGHT: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<(&'static str, i64)>>,
> = std::sync::LazyLock::new(Default::default);

/// RAII claim on one on-demand sync slot. Dropping it frees the slot, so
/// every early return in the sync task releases it.
struct InFlight {
    key: (&'static str, i64),
}

impl InFlight {
    /// `None` when the same item is already syncing or all slots are busy.
    /// The caller then skips; the next page view retries.
    fn try_claim(kind: &'static str, id: i64) -> Option<Self> {
        let mut set = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if set.len() >= MAX_ON_DEMAND_SYNCS || !set.insert((kind, id)) {
            return None;
        }
        Some(Self { key: (kind, id) })
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.key);
    }
}

/// Run `job` in the background unless the same item is already running or
/// the on-demand cap is reached. Returns whether it started.
pub(crate) fn spawn_job(job: Job) -> bool {
    let key = match &job {
        // Scans are keyed by source position, so each source scans once at a time.
        Job::Scan(source) => (
            "scan",
            source_names()
                .iter()
                .position(|s| s == source)
                .unwrap_or(usize::MAX) as i64,
        ),
        Job::Manga(id) => ("manga", *id),
        Job::Chapter(id) => ("chapter", *id),
    };
    let Some(claim) = InFlight::try_claim(key.0, key.1) else {
        tracing::debug!("{job:?} skipped (in flight or at cap)");
        return false;
    };
    spawn_scraper(move || async move {
        let _claim = claim;
        let _ = run_job(job).await;
    });
    true
}

/// Days of sync history kept for the admin pages.
const SYNC_HISTORY_DAYS: i64 = 30;

/// Start the automatic sync loop in the background.
///
/// # Flow
/// 1. **Scan** — each source is scanned as its own recorded job, so one
///    broken source cannot block the others
/// 2. **Sync** — manga that are unsynced or stale sync in parallel, at most
///    [`MAX_CONCURRENT_SYNCS`] at a time, each on its own OS thread
/// 3. **Sleep** — waits [`SYNC_INTERVAL_SECS`], or until an admin presses
///    "Run full sync now" ([`AppState::sync_wake`])
pub fn start_sync_loop(state: Arc<AppState>) {
    spawn_scraper(move || async move {
        if let Ok(db) = open_db() {
            match mangaread_core::db::fail_interrupted_sync_runs(&db) {
                Ok(n) if n > 0 => tracing::warn!("marked {n} interrupted sync runs as failed"),
                _ => {}
            }
        }
        loop {
            run_pass(&state).await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(SYNC_INTERVAL_SECS)) => {}
                _ = state.sync_wake.notified() => {
                    tracing::info!("sync loop: woken by admin");
                }
            }
        }
    });
}

/// One full scan + sync pass.
async fn run_pass(state: &Arc<AppState>) {
    {
        let mut p = state.auto_sync.lock().await;
        p.running = true;
        p.total = 0;
        p.current = 0;
    }

    let disabled = open_db()
        .map(|db| disabled_sources(&db))
        .unwrap_or_default();

    // ── 1. Scan each enabled source ──
    for source in source_names() {
        if !disabled.contains(&source) {
            let _ = run_job(Job::Scan(source)).await;
        }
    }

    // ── 2. Manga needing sync ──
    let manga_ids = match open_db().map_err(|e| e.to_string()).and_then(|db| {
        let _ = mangaread_core::db::prune_sync_runs(&db, SYNC_HISTORY_DAYS);
        mangaread_core::db::list_manga_ids_needing_sync(&db, STALE_MINUTES, &disabled)
            .map_err(|e| e.to_string())
    }) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::error!("sync loop: could not list stale manga: {e}");
            Vec::new()
        }
    };
    let total = manga_ids.len();
    tracing::info!("sync loop: syncing {total} manga (max {MAX_CONCURRENT_SYNCS} concurrent)");
    state.auto_sync.lock().await.total = total as i64;

    // ── 3. Sync in parallel on OS threads (rusqlite is !Send) ──
    let completed = Arc::new(std::sync::atomic::AtomicI64::new(0));
    for chunk in manga_ids.chunks(MAX_CONCURRENT_SYNCS) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|&mid| {
                let state = state.clone();
                let completed = completed.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().expect("sync worker rt");
                    rt.block_on(async move {
                        let _ = run_job(Job::Manga(mid)).await;
                        let done = completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                        state.auto_sync.lock().await.current = done;
                    });
                })
            })
            .collect();
        for h in handles {
            let _ = h.join();
        }
    }

    if let Ok(db) = open_db() {
        refresh_search_index(&db);
    }
    let mut p = state.auto_sync.lock().await;
    p.running = false;
    p.last_finished = Some(crate::auth::now_utc_string());
    tracing::info!("sync loop: pass complete ({total} manga)");
}

#[cfg(test)]
mod in_flight_tests {
    use super::*;

    #[test]
    fn claims_dedupe_cap_and_release_on_drop() {
        // Distinct kind so parallel tests in this binary cannot collide.
        let a = InFlight::try_claim("test", 1).expect("first claim");
        assert!(
            InFlight::try_claim("test", 1).is_none(),
            "duplicate refused"
        );
        let b = InFlight::try_claim("test", 2).expect("second");
        let c = InFlight::try_claim("test", 3).expect("third");
        assert!(InFlight::try_claim("test", 4).is_none(), "cap reached");
        drop(a);
        let d = InFlight::try_claim("test", 1).expect("slot freed on drop");
        drop((b, c, d));
    }
}
