# Round 1 Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship 8 quick-win improvements to MangaRead: loading skeletons, styled 404 page, continue reading, dead reference fix, image preloading, rescan button, error toasts, health endpoint.

**Architecture:** All changes are in the `website` crate. Backend gets new route modules and an `AppState` field for sync errors. Templates get skeleton/error/continue-reading markup. CSS gets new component styles. JS gets toast, preload, and continue-reading logic.

**Tech Stack:** Rust (Axum + Maud), vanilla JS, CSS custom properties.

---

### Task 1: Health endpoint + log level from env

**Files:**
- Create: `website/src/routes/health.rs`
- Modify: `website/src/main.rs`

- [ ] **Step 1: Create health route module**

```rust
// website/src/routes/health.rs
use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok".into() })
}
```

- [ ] **Step 2: Register health route and env filter in main.rs**

In `main.rs`, add the route to the router and switch the tracing subscriber to use an env filter:

```rust
// At the top of main()
tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
    .init();
```

Replace `tracing_subscriber::fmt::init();` with the above.

Add to router (before `.with_state(state)`):
```rust
.route("/health", get(crate::routes::health::health))
```

Add `mod health;` to `website/src/routes/mod.rs`.

- [ ] **Step 3: Build and verify**

```
cd /Users/yanis/Programming/mangaread && cargo build -p website
```

- [ ] **Step 4: Commit**

```
git add -A && git commit -m "feat: add health endpoint and RUST_LOG env filter"
```

---

### Task 2: Fix dead `reader.js` reference

**Files:**
- Modify: `website/src/templates/layout.rs:49`

- [ ] **Step 1: Change the script src**

In `reader_layout()`, change `script src="/static/reader.js"` to `script src="/static/app.js"`:
```rust
script src="/static/app.js" {}
```

- [ ] **Step 2: Build**

```
cargo build -p website
```

- [ ] **Step 3: Commit**

```
git add -A && git commit -m "fix: load app.js instead of dead reader.js in reader layout"
```

---

### Task 3: Styled 404 / error page

**Files:**
- Modify: `website/src/templates/layout.rs`
- Modify: `website/src/routes/series.rs` (use new error page)
- Modify: `website/src/routes/reader.rs` (use new error page)

- [ ] **Step 1: Add `error_layout` to layout.rs**

```rust
/// Render an error page with a heading, detail message, and a link back home.
pub fn error_layout(heading: &str, message: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (heading) " — MangaRead" }
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="";
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Space+Grotesk:wght@400;500;600;700&family=Inter:wght@400;500;600&display=swap";
                link rel="stylesheet" href="/static/style.css";
            }
            body {
                div.container {
                    header {
                        h1 { a href="/" { "MangaRead" } }
                    }
                    main {
                        section class="error-section" {
                            h1 class="error-heading" { (heading) }
                            p class="error-message" { (message) }
                            a href="/" class="error-home-btn" { "Back to Home" }
                        }
                    }
                    footer {
                        p { "MangaRead — training project" }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 2: Replace inline 404s with error_layout calls**

In `series.rs`:
```rust
Err(_) => {
    let content = html! { h2 class="page-title" { "Series not found" } };
    return Html(crate::templates::layout::base_layout("Not found", content).0);
}
```
Replace with:
```rust
Err(_) => {
    return Html(crate::templates::layout::error_layout(
        "Not Found",
        "This series doesn't exist or may have been removed.",
    ));
}
```

Same pattern in `reader.rs` for manga not found and chapter not found.

In `manga.rs`:
```rust
Err(_) => {
    use maud::html;
    let content = html! { h2 class="page-title" { "Manga not found" } };
    Err(axum::response::Html(
        crate::templates::layout::base_layout("Not found", content).0,
    ))
}
```
Replace with `error_layout("Not Found", "Manga not found.")`.

- [ ] **Step 3: Add error section CSS to style.css**

```css
.error-section {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-height: 50vh;
  text-align: center;
  gap: 1rem;
}

.error-heading {
  font-family: var(--font-display);
  font-size: 3rem;
  color: var(--text-primary);
}

.error-message {
  color: var(--text-muted);
  font-size: 1rem;
  max-width: 400px;
}

.error-home-btn {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.65rem 1.25rem;
  background: var(--accent);
  color: #000;
  font-family: var(--font-display);
  font-size: 0.95rem;
  font-weight: 600;
  border-radius: var(--radius-sm);
  text-decoration: none;
  margin-top: 0.5rem;
  transition: all var(--transition-base);
}

.error-home-btn:hover {
  background: var(--accent-hover);
  transform: translateY(-1px);
  box-shadow: var(--shadow-glow);
}
```

- [ ] **Step 4: Build**

```
cargo build -p website
```

- [ ] **Step 5: Commit**

```
git add -A && git commit -m "feat: add styled 404/error page layout"
```

---

### Task 4: Loading skeletons (CSS + templates)

**Files:**
- Modify: `website/static/style.css` — add skeleton keyframes and classes
- Modify: `website/src/routes/home.rs` — skeleton for recent chapters
- Modify: `website/src/routes/series.rs` — skeleton for chapter list
- Modify: `website/src/routes/reader.rs` — skeleton for reader loading

- [ ] **Step 1: Add skeleton CSS to style.css**

```css
/* ── Skeleton Loading ── */
@keyframes shimmer {
  0% { background-position: -400px 0; }
  100% { background-position: 400px 0; }
}

.skeleton {
  background: linear-gradient(90deg, var(--bg-card) 25%, var(--bg-hover) 50%, var(--bg-card) 75%);
  background-size: 800px 100%;
  animation: shimmer 1.5s ease-in-out infinite;
  border-radius: var(--radius-sm);
}

.skeleton-card {
  width: 100%;
  aspect-ratio: 3/4;
  border-radius: var(--radius-md);
}

.skeleton-text {
  height: 1rem;
  width: 80%;
  margin-top: 0.5rem;
}

.skeleton-text.short {
  width: 50%;
}

.skeleton-row {
  height: 3.5rem;
  width: 100%;
  margin-bottom: 0.5rem;
  border-radius: var(--radius-sm);
}

.skeleton-page {
  width: 100%;
  aspect-ratio: 3/4;
  border-radius: var(--radius-md);
  margin-bottom: 0.5rem;
}

.skeleton-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 1.5rem;
}

@media (max-width: 600px) {
  .skeleton-grid {
    grid-template-columns: repeat(2, 1fr);
    gap: 1rem;
  }
}
```

- [ ] **Step 2: Add skeleton template helpers**

Add inline functions or use plain HTML in templates. For series list page, replace the empty state with skeletons when search is empty:

In `series.rs` `series_list()`, replace empty section with:
```rust
@if all_series.is_empty() {
    @if search.is_empty() {
        // Show skeletons as placeholder until data loads
        div class="skeleton-grid" {
            @for _ in 0..12 {
                div class="manga-card" {
                    div class="cover-wrapper" {
                        div class="skeleton skeleton-card" {}
                    }
                    div class="info" {
                        div class="skeleton skeleton-text" {}
                        div class="skeleton skeleton-text short" {}
                    }
                }
            }
        }
    } @else {
        div class="empty-state" {
            p { "No series match \"" (search) "\"." }
        }
    }
}
```

- [ ] **Step 3: Skeleton for reader loading state**

In `reader.rs`, replace the current loading state with a skeleton:
```rust
@if syncing {
    div class="loading-state" {
        div class="skeleton skeleton-page" {}
        div class="skeleton skeleton-page" style="aspect-ratio:3/5" {}
        div class="skeleton skeleton-page" style="aspect-ratio:3/4" {}
    }
}
```

- [ ] **Step 4: Skeleton for home recent chapters**

In `home.rs`, when recent is empty, show skeleton rows instead of text message:
```rust
@if recent.is_empty() {
    @for _ in 0..5 {
        div class="skeleton skeleton-row" {}
    }
} @else {
```

- [ ] **Step 5: Build**

```
cargo build -p website
```

- [ ] **Step 6: Commit**

```
git add -A && git commit -m "feat: add loading skeleton CSS and template placeholders"
```

---

### Task 5: Preload next 3 pages in reader

**Files:**
- Modify: `website/static/app.js`

- [ ] **Step 1: Add preload logic to `initReaderUX()`**

In `app.js`, inside `initReaderUX()` after the page observer setup, add:

```javascript
// Preload next images
const pageImages = Array.from(pages).map(p => {
  const img = p.querySelector('img');
  return img ? img.getAttribute('src') : null;
});

let preloadLinks = [];

function updatePreload() {
  // Remove old preload links
  preloadLinks.forEach(l => l.remove());
  preloadLinks = [];

  // Preload next 3 images
  for (let i = currentPage; i < currentPage + 3 && i < pageImages.length; i++) {
    const src = pageImages[i];
    if (!src) continue;
    const link = document.createElement('link');
    link.rel = 'preload';
    link.as = 'image';
    link.href = src;
    document.head.appendChild(link);
    preloadLinks.push(link);
  }
}

// Call on page change (in the pageObserver callback)
// Modify the existing pageObserver to also call updatePreload():
// After `updateIndicator();` add `updatePreload();`
```

- [ ] **Step 2: Integrate into existing observer**

Add `updatePreload();` call after `updateIndicator();` inside the pageObserver's entries.forEach callback.

- [ ] **Step 3: Build (JS has no build step)**

JS is served as-is, no build needed.

- [ ] **Step 4: Commit**

```
git add -A && git commit -m "feat: preload next 3 images in reader"
```

---

### Task 6: Backend API endpoints (sync-manga, sync-status, progress-series)

**Files:**
- Create: `website/src/routes/api.rs`
- Modify: `website/src/main.rs` — AppState + register routes
- Modify: `website/src/scraper_helper.rs` — track sync status

- [ ] **Step 1: Add sync state to AppState in main.rs**

```rust
pub struct AppState {
    pub db: Mutex<Connection>,
    pub sync_error: Mutex<Option<String>>,
    pub syncing_manga_ids: Mutex<Vec<i64>>,
}
```

Update the constructor:
```rust
let state = Arc::new(AppState {
    db: Mutex::new(conn),
    sync_error: Mutex::new(None),
    syncing_manga_ids: Mutex::new(Vec::new()),
});
```

- [ ] **Step 2: Create api.rs route module**

```rust
use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Serialize, Deserialize};
use std::sync::Arc;

use crate::AppState;

#[derive(Serialize)]
pub struct SyncStatusResponse {
    pub syncing: bool,
    pub manga_ids: Vec<i64>,
    pub last_error: Option<String>,
}

#[derive(Serialize)]
pub struct SyncMangaResponse {
    pub ok: bool,
}

#[derive(Serialize)]
pub struct ProgressSeriesItem {
    pub id: i64,
    pub title: String,
    pub cover_url: Option<String>,
    pub manga_id: i64,
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub page: i64,
    pub total_pages: i64,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

/// GET /api/sync-status — current sync state
pub async fn sync_status(State(state): State<Arc<AppState>>) -> Json<SyncStatusResponse> {
    let error = state.sync_error.lock().await.take();
    let manga_ids = state.syncing_manga_ids.lock().await.clone();
    Json(SyncStatusResponse {
        syncing: !manga_ids.is_empty(),
        manga_ids,
        last_error: error,
    })
}

/// POST /api/sync-manga/{id} — trigger a background sync for a manga
pub async fn sync_manga(
    State(state): State<Arc<AppState>>,
    Path(id): Path<i64>,
) -> Json<SyncMangaResponse> {
    // Mark as syncing
    state.syncing_manga_ids.lock().await.push(id);
    let state_clone = state.clone();
    tokio::spawn(async move {
        // Run sync in background
        let result = {
            let db = state_clone.db.lock().await;
            let driver = match thirtyfour::WebDriver::managed(
                crate::scraper_helper::build_chrome_caps()
            ).await {
                Ok(d) => d,
                Err(e) => {
                    *state_clone.sync_error.lock().await = Some(format!("WebDriver error: {e}"));
                    state_clone.syncing_manga_ids.lock().await.retain(|&x| x != id);
                    return;
                }
            };
            let engine = crate::scraper_helper::build_engine();
            let result = engine.sync_manga(&driver, &db, id).await;
            if let Err(e) = driver.quit().await {
                tracing::warn!("sync driver quit error: {:?}", e);
            }
            result
        };
        if let Err(e) = result {
            *state_clone.sync_error.lock().await = Some(format!("Sync failed: {e}"));
        }
        state_clone.syncing_manga_ids.lock().await.retain(|&x| x != id);
    });
    Json(SyncMangaResponse { ok: true })
}

/// GET /api/progress-series?ids=1,2,3 — get series info for continue-reading
#[derive(Deserialize)]
pub struct ProgressQuery {
    pub ids: String,
}

pub async fn progress_series(
    State(state): State<Arc<AppState>>,
    query: axum::extract::Query<ProgressQuery>,
) -> Json<Vec<ProgressSeriesItem>> {
    let ids: Vec<i64> = query.ids.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    if ids.is_empty() {
        return Json(vec![]);
    }

    let db = state.db.lock().await;
    let mut results = Vec::new();

    for series_id in ids {
        if let Ok(series) = mangaread_core::db::get_series_by_id(&db, series_id) {
            let manga_entries = mangaread_core::db::list_manga_by_series(&db, series_id).unwrap_or_default();
            let primary = mangaread_core::service::series::pick_primary(&manga_entries, &db)
                .or_else(|| manga_entries.first());
            if let Some(m) = primary {
                let chapters = mangaread_core::db::list_chapters_by_manga(&db, m.id).unwrap_or_default();
                // Return the first unfinished chapter as the resume point
                if let Some(ch) = chapters.first() {
                    let pages = mangaread_core::db::list_pages_by_chapter(&db, ch.id).unwrap_or_default();
                    results.push(ProgressSeriesItem {
                        id: series_id,
                        title: series.title,
                        cover_url: m.cover_url.clone(),
                        manga_id: m.id,
                        chapter_id: ch.id,
                        chapter_number: ch.number,
                        page: 1,
                        total_pages: pages.len() as i64,
                    });
                }
            }
        }
    }

    Json(results)
}
```

- [ ] **Step 3: Register routes and module**

In `main.rs`, add `.route("/api/sync-status", get(crate::routes::api::sync_status))` etc.

In `routes/mod.rs` add `mod api;`.

- [ ] **Step 4: Build**

```
cargo build -p website
```

- [ ] **Step 5: Commit**

```
git add -A && git commit -m "feat: add api endpoints for sync-manga, sync-status, progress-series"
```

---

### Task 7: Manual rescan button + error toasts + continue reading

**Files:**
- Modify: `website/src/routes/series.rs` — rescan button + error check
- Modify: `website/src/routes/home.rs` — continue reading section
- Modify: `website/static/app.js` — rescan handler, toast component, continue reading
- Modify: `website/static/style.css` — toast + continue reading styles

- [ ] **Step 1: Add rescan button to series template**

In `series.rs` `series_detail()`, in the `.manga-hero-actions` div, add after `Start Reading`:
```rust
button id="rescan-btn" class="rescan-btn" data-manga-id=(primary.id) {
    svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" {
        path d="M23 4v6h-6M1 20v-6h6" {}
        path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" {}
    }
    " Refresh"
}
```

- [ ] **Step 2: Add toast CSS**

```css
/* ── Toast ── */
#toast-container {
  position: fixed;
  bottom: 1.5rem;
  right: 1.5rem;
  z-index: 1000;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
  pointer-events: none;
}

.toast {
  pointer-events: auto;
  background: var(--bg-elevated);
  color: var(--text-primary);
  border: 1px solid var(--border);
  padding: 0.75rem 1rem;
  border-radius: var(--radius-md);
  font-size: 0.85rem;
  max-width: 360px;
  box-shadow: var(--shadow-lg);
  animation: toastIn 0.3s ease forwards;
}

.toast.error {
  border-color: #ef4444;
  background: rgba(239, 68, 68, 0.1);
  color: #fca5a5;
}

.toast.success {
  border-color: #22c55e;
  background: rgba(34, 197, 94, 0.1);
  color: #86efac;
}

@keyframes toastIn {
  from { opacity: 0; transform: translateY(1rem); }
  to { opacity: 1; transform: translateY(0); }
}

.toast.removing {
  animation: toastOut 0.3s ease forwards;
}

@keyframes toastOut {
  from { opacity: 1; transform: translateY(0); }
  to { opacity: 0; transform: translateY(1rem); }
}
```

- [ ] **Step 3: Add continue reading CSS**

```css
/* ── Continue Reading ── */
.continue-section {
  margin-bottom: 2rem;
}

.continue-scroll {
  display: flex;
  gap: 1rem;
  overflow-x: auto;
  padding-bottom: 0.5rem;
  scroll-snap-type: x mandatory;
  -webkit-overflow-scrolling: touch;
}

.continue-scroll::-webkit-scrollbar {
  height: 6px;
}

.continue-card {
  flex: 0 0 160px;
  scroll-snap-align: start;
  text-decoration: none;
}

.continue-card .cover-wrapper {
  position: relative;
  width: 100%;
  aspect-ratio: 3/4;
  border-radius: var(--radius-md);
  overflow: hidden;
  background: var(--bg-card);
}

.continue-card img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.continue-card .continue-overlay {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  background: linear-gradient(transparent, rgba(0,0,0,0.85));
  padding: 0.75rem 0.5rem 0.5rem;
}

.continue-card .continue-ch {
  font-size: 0.75rem;
  color: var(--accent);
  font-weight: 600;
}

.continue-card .continue-pct {
  font-size: 0.7rem;
  color: var(--text-muted);
}

.continue-card h3 {
  font-family: var(--font-display);
  font-size: 0.85rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-top: 0.4rem;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
```

- [ ] **Step 4: Add JS features to app.js**

Add a toast system at the top of the IIFE:
```javascript
// ============================================================
//  TOAST SYSTEM
// ============================================================

function showToast(message, type) {
  type = type || 'error';
  let container = document.getElementById('toast-container');
  if (!container) {
    container = document.createElement('div');
    container.id = 'toast-container';
    document.body.appendChild(container);
  }
  const el = document.createElement('div');
  el.className = 'toast ' + type;
  el.textContent = message;
  container.appendChild(el);
  setTimeout(function () {
    el.classList.add('removing');
    setTimeout(function () { el.remove(); }, 300);
  }, 4000);
}
```

Add rescan handler in DOMContentLoaded:
```javascript
// Rescan button
const rescanBtn = document.getElementById('rescan-btn');
if (rescanBtn) {
  rescanBtn.addEventListener('click', function () {
    const mangaId = rescanBtn.getAttribute('data-manga-id');
    rescanBtn.disabled = true;
    rescanBtn.classList.add('loading');
    fetch('/api/sync-manga/' + mangaId, { method: 'POST' })
      .then(function () {
        showToast('Sync started, page will refresh soon...', 'success');
        setTimeout(function () { location.reload(); }, 5000);
      })
      .catch(function () {
        showToast('Failed to start sync');
        rescanBtn.disabled = false;
        rescanBtn.classList.remove('loading');
      });
  });
}

// Check for sync errors on series page
const seriesId = document.querySelector('[data-series-id]');
if (seriesId) {
  fetch('/api/sync-status')
    .then(function (r) { return r.json(); })
    .then(function (data) {
      if (data.last_error) {
        showToast(data.last_error, 'error');
      }
    })
    .catch(function () {});
}
```

Add continue reading logic:
```javascript
// ============================================================
//  CONTINUE READING
// ============================================================

function initContinueReading() {
  const container = document.getElementById('continue-reading-container');
  if (!container) return;

  // Collect unique series IDs from localStorage
  const inProgress = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key && key.startsWith('mangaread_read_')) {
      const seriesId = key.replace('mangaread_read_', '');
      try {
        const state = JSON.parse(localStorage.getItem(key));
        // Find chapters that are started but not finished
        for (const chKey in state) {
          if (state[chKey].status === 'started') {
            inProgress[seriesId] = (inProgress[seriesId] || 0) + 1;
          }
        }
      } catch (_) {}
    }
  }

  const ids = Object.keys(inProgress);
  if (ids.length === 0) {
    // Check for finished series too — show the most recent
    container.remove();
    return;
  }

  fetch('/api/progress-series?ids=' + ids.join(','))
    .then(function (r) { return r.json(); })
    .then(function (data) {
      if (!data || data.length === 0) {
        container.remove();
        return;
      }
      let html = '<h2 class="section-title">Continue Reading</h2>';
      html += '<div class="continue-scroll">';
      data.forEach(function (item) {
        const cover = item.cover_url || '';
        const url = '/read/' + item.manga_id + '/' + item.chapter_id;
        html += '<a href="' + url + '" class="continue-card">';
        html += '<div class="cover-wrapper">';
        if (cover) html += '<img src="' + cover + '" alt="" loading="lazy">';
        html += '<div class="continue-overlay">';
        html += '<span class="continue-ch">Ch. ' + item.chapter_number + '</span>';
        html += '</div></div>';
        html += '<h3>' + escapeHtml(item.title) + '</h3>';
        html += '</a>';
      });
      html += '</div>';
      container.innerHTML = html;
    })
    .catch(function () { container.remove(); });
}

function escapeHtml(str) {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}
```

Call `initContinueReading();` in the DOMContentLoaded listener.

- [ ] **Step 5: Add continue-reading container to home.rs**

In `home.rs`, add after the hero section and before the recently updated section:
```rust
// ── Continue Reading ──
div id="continue-reading-container" class="continue-section" {}
```

- [ ] **Step 6: Add data-series-id to series page body**

In `series.rs`, make sure the body has `data-series-id` on some element. The `#chapter-list` already has `data-series-id`. The JS can use that.

- [ ] **Step 7: Build**

```
cargo build -p website
```

- [ ] **Step 8: Commit**

```
git add -A && git commit -m "feat: rescan button, error toasts, continue reading section"
```
