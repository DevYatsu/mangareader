# Round 1: Quick-win improvements for MangaRead

Date: 2026-07-10

## Overview

Eight low-effort, high-impact improvements to the MangaRead website, targeting UX polish,
reader experience, and operational basics.

## Items

### 1. Loading skeletons

Replace server-rendered text spinners with CSS-only shimmer skeleton placeholders.

- **Series list grid**: card-shaped skeletons (cover rect + title bar) that match `.manga-card` dimensions
- **Series detail chapter list**: row-shaped skeletons matching `.chapter-list > li` height
- **Reader sync page**: full-viewport skeleton matching page image aspect ratio
- Pure CSS + HTML template changes to `base_layout` / inline maud templates; no extra JS

### 2. Styled 404 / error page

Extract the inline `<h2>Series not found</h2>` pattern used in several routes into a
dedicated `error_layout(heading, message)` function in `templates/layout.rs`.

- Renders centered hero section with the error heading, a sub-message, and a "Back to Home" link
- Keeps the existing site header/footer

### 3. Continue Reading on home page

Show a horizontal scrollable row of series cards for chapters the user has started but not
finished.

- **Frontend**: JS reads all `mangaread_read_*` localStorage keys on page load, filters
  for entries with `status === "started"`, collects unique series IDs
- **Backend**: new `GET /api/progress-series?ids=1,2,3` returns JSON array of
  `{ id, title, cover_url, chapter_id, chapter_number, page, total_pages }`
- **Template**: if data is returned, render a `.continue-reading` section above "Recently Updated"

### 4. Fix dead `reader.js` reference

`reader_layout()` in `templates/layout.rs` loads `/static/reader.js` which does not exist.
Change to `/static/app.js` which contains all reader logic.

### 5. Preload next 3 pages in reader

When the reader scroll observer detects a new current page, dynamically inject
`<link rel="preload" as="image" href="...">` tags for images at `current + 1`, `+2`, `+3`.

- Existing `<img>` tags already use `loading="lazy"` — preload gives the browser a head start
- Clean up old preload links to avoid DOM bloat
- Implementation in `app.js` within `initReaderUX()`

### 6. Manual rescan button on series page

Add a "Refresh" button to the series hero that triggers a background re-sync.

- **Backend**: `POST /api/sync-manga/{manga_id}` — calls `sync_manga_background(id)`,
  returns `{"ok":true}`
- **Frontend**: button added to `.manga-hero-actions` alongside "Start Reading"
  - Click → POST to API → button shows spinner → page reloads after 3s

### 7. User-facing error toasts

Show scraper errors as non-blocking toast notifications in the UI.

- **Backend**: store last sync error string in a `Mutex<Option<String>>` on `AppState`
  - `scraper_helper::sync_manga_background` writes error messages to this field
  - New `GET /api/sync-status` returns `{ syncing: bool, last_error: string | null }`
  - Reading `/api/sync-status` clears the error (one-shot delivery)
- **Frontend**: toast component in `app.js`
  - Fixed bottom-right, fade-in/out animation
  - Series detail page calls `/api/sync-status` after triggering a sync; shows toast on error

### 8. Health endpoint + log level from env

- `GET /health` returns `{"status":"ok"}` — for Docker healthcheck
- Wire up `tracing-subscriber`'s env filter so `RUST_LOG=debug cargo run -p website` works

## Dependencies

```
health-endpoint ──────┐
log-level-from-env ───┤
reader-js-fix ────────┤ (all independent, parallel)
                      │
skeleton-css ─────────┤
                      │
error-page-html ──────┤
                      │
preload ──────────────┤
                      │
                      │
rescan-button ────────┤── depends on health (new state field pattern)
                      │
continue-reading ─────┤── depends on health (new API endpoint)
                      │
error-toasts ─────────┘── depends on health (new state field), rescan-button (user triggers it)
```

Actual dependencies are minimal — each item touches distinct files. Items can be implemented
in any order; the suggested order is: 8, 4, 2, 1, 5, 6, 7, 3.
