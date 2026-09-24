# Bulma Migration — Slice B: Content — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Convert the public content routes from the legacy `style.css` vocabulary to the shared Bulma 1.0.4/Ink & Ember vocabulary without changing routes, forms, links, auth, APIs, localStorage, or reader behavior.

**Architecture:** Keep the existing three-layer cascade (`vendor/bulma.min.css` → `theme.css` → `reader.css` for the reader only). Add one Maud `templates::content` module for server-rendered covers, cards, empty states, and breadcrumbs; use one matching browser-side card builder in `app.js` for bookmarks and continue-reading. Geometry belongs in the approved `m-*` primitives and `theme.css`; routes supply data and Bulma classes only.

**Tech Stack:** Rust, Axum 0.8, Maud 0.27, Bulma 1.0.4, plain CSS, browser JavaScript. No Node, Sass, CDN, or frontend framework.

**Spec:** `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

## Global Constraints

- Preserve every form `name=`, `action=`, and `method=`; preserve server-side search and all route/status behavior.
- Preserve the existing data hooks (`data-user-id`, `data-series-id`, `data-manga-id`, `data-chapter-id`, `data-source`, `data-sources`), IDs (`#chapter-list`, `#source-toggles`, `#source-reset`, `#rescan-btn`, `#bookmarks-container`, `#bookmarks-empty`, `#continue-reading-container`), localStorage keys, API paths, and reader `data-r-*` attributes.
- Do not change `/`, `/series`, `/series/{id}`, `/bookmarks`, `/source/{source}`, `/terms`, `/privacy`, or the `/manga/{id}` fallback/redirect behavior.
- Do not reintroduce or edit `static/style.css`; it is intentionally unlinked until Slice E. Do not change `reader.css`, `reader.js`, or reader markup in this slice.
- Use Bulma’s `is-*`/`has-*` vocabulary for Bulma-styled concerns. Add only `m-source-toggle` and `m-legal-body` as new content primitives; reuse existing `m-cover`, `m-chapter-row`, `m-empty`, `m-empty-icon`, `m-text-muted`, `m-display`, `m-nav-active`, and `m-footer`.
- Use `textContent`/DOM properties for API and localStorage-derived text. Preserve escaping for Maud/server data.
- Keep the card/grid change atomic across Rust, `app.js`, and `theme.css`; a server card and a browser card must not drift.
- Keep the live cascade offline-capable. Remove the Google Fonts links from the shell and vendor pinned local font files, or record an explicit documented deviation before closing the slice.
- Do not modify API handlers, auth/role checks, query semantics, sync behavior, or reader behavior.

## File Structure

| File | Responsibility in this slice |
|---|---|
| `website/src/templates/content.rs` | Shared Maud helpers for covers, manga cards, empty states, and breadcrumbs |
| `website/src/templates/mod.rs` | Export the new content module |
| `website/src/routes/home.rs` | Bulma hero, stats, recent media rows, and continue-reading container |
| `website/src/routes/series.rs` | Bulma list/search header, shared grid, detail hero, source filters, and chapter rows |
| `website/src/routes/bookmarks.rs` | Bulma header/grid/empty-state shell; client rendering remains in `app.js` |
| `website/src/routes/source.rs` | Bulma source header, shared grid, and breadcrumbs |
| `website/src/routes/legal.rs` | Bulma legal layout and escaped `m-legal-body` text |
| `website/static/app.js` | Shared browser card/cover/empty-state builders and migrated bookmark/continue/toast UI |
| `website/static/theme.css` | Content primitives, surface/shadow tokens, cover states, chapter rows, toast state, reduced motion |
| `website/static/fonts/*` | Pinned self-hosted font files and provenance, if Task 0 is included |
| `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md` | Record the binding Slice B markup rulings and deferrals |

---

### Task 0: Self-host the display/body fonts

**Files:**
- Create: `website/static/fonts/inter-var-latin.woff2`
- Create: `website/static/fonts/inter-var-latin-ext.woff2`
- Create: `website/static/fonts/space-grotesk-var-latin.woff2`
- Create: `website/static/fonts/space-grotesk-var-latin-ext.woff2`
- Create: `website/static/fonts/README.md`
- Modify: `website/static/theme.css`
- Modify: `website/src/templates/layout.rs`

**Interfaces:**
- Consumes: the existing `--bulma-family-primary` and `--bulma-family-secondary` token names.
- Produces: local `@font-face` rules and no runtime Google Fonts requests.

- [ ] **Step 1: Obtain pinned upstream font files**

Use the official Google Fonts repository or the project’s approved asset source to download the variable WOFF2 Latin and Latin-ext subsets. Pin the exact upstream version/URL, record the SIL Open Font License text/reference, and compute SHA-256 checksums:

```bash
mkdir -p website/static/fonts
shasum -a 256 website/static/fonts/*.woff2
```

If the official files cannot be fetched in the environment, do not substitute arbitrary binaries; record the blocked evidence and keep this task pending.

- [ ] **Step 2: Record provenance**

`website/static/fonts/README.md` must list each file, source URL, pinned version, license, and the checksum produced above. State that the files are served locally and that the app has no font CDN dependency.

- [ ] **Step 3: Add local font faces**

In `theme.css`, add `@font-face` rules for the four files with `font-display: swap`, using the family names `Inter` and `Space Grotesk` and the correct weight ranges. Keep the existing Bulma family variables as the consumers; do not introduce a new color or layout system.

- [ ] **Step 4: Remove external font links**

Remove only the Google Fonts `preconnect` and stylesheet links from `base_layout`; do not change the reader body attributes, scripts, or stylesheet order.

- [ ] **Step 5: Verify the font contract**

```bash
cargo test -p website reader::
git grep -n 'fonts.googleapis\|fonts.gstatic' -- website/src/templates website/static/theme.css
```

The grep must return no matches. In a local browser, verify `document.fonts.check('16px Inter')` and `document.fonts.check('16px "Space Grotesk"')`, and confirm the network panel has no external font request.

- [ ] **Step 6: Commit**

```bash
git add website/static/fonts website/static/theme.css website/src/templates/layout.rs
git commit -m "perf(web): self-host Ink and Ember fonts"
```

---

### Task 1: Establish shared content helpers and client card parity

**Files:**
- Create: `website/src/templates/content.rs`
- Modify: `website/src/templates/mod.rs`
- Modify: `website/src/routes/home.rs` only where the continue/empty containers are emitted
- Modify: `website/src/routes/series.rs` only where list cards/empty states are emitted
- Modify: `website/src/routes/source.rs` only where list cards/empty states are emitted
- Modify: `website/src/routes/bookmarks.rs`
- Modify: `website/static/app.js`
- Modify: `website/static/theme.css`

**Interfaces:**

Expose these Maud helpers from `website/src/templates/content.rs`:

```rust
pub fn cover(href: Option<&str>, title: &str, cover_url: Option<&str>, badge: Option<&str>) -> maud::Markup;
pub fn manga_card(href: &str, title: &str, cover_url: Option<&str>, badge: Option<&str>) -> maud::Markup;
pub fn empty_state(message: &str, show_icon: bool) -> maud::Markup;
pub fn breadcrumb(items: &[(&str, &str)]) -> maud::Markup;
```

The card must produce this shared shape (the bookmark button is outside the cover link):

```html
<div class="columns is-mobile is-multiline">
  <div class="column is-6-mobile is-4-tablet is-3-desktop">
    <div class="card">
      <div class="card-image">
        <figure class="image is-2by3 m-cover">
          <!-- image or data-cover-placeholder/data-cover-initial -->
          <span class="tag is-primary is-light" data-cover-badge>…</span>
          <span data-cover-overlay aria-hidden="true"></span>
        </figure>
      </div>
      <div class="card-content">
        <a href="/series/1"><p class="title is-6">Title</p></a>
      </div>
    </div>
  </div>
</div>
```

Use only these data hooks for visual state: `data-cover-placeholder`, `data-cover-initial`, `data-cover-badge`, `data-cover-overlay`, `data-bookmark-button`, and `data-bookmark-label`.

In `app.js`, add private builders with these data-only interfaces:

```javascript
buildCover({ href, title, coverUrl, badge, alt })
buildMangaCard({ href, title, coverUrl, badge, variant, bookmarked, progress })
buildEmptyState(message)
```

`variant` is `grid` or `continue`; both use the same `card`, `card-image`, and `figure.image.is-2by3.m-cover` structure. `renderBookmarkIds()` uses `grid`; `initContinueReading()` uses `continue`.

- [ ] **Step 1: Add failing helper tests**

Add Rust tests that assert:

```rust
assert!(markup.contains("columns is-mobile is-multiline"));
assert!(markup.contains("column is-6-mobile is-4-tablet is-3-desktop"));
assert!(markup.contains("card"));
assert!(markup.contains("image is-2by3 m-cover"));
assert!(!markup.contains("manga-grid"));
assert!(!markup.contains("manga-card"));
assert!(!markup.contains("cover-wrapper"));
```

Also assert that a title, URL, and badge containing `<`/`&` are escaped, that a placeholder includes `data-cover-placeholder` and `data-cover-initial`, and that `empty_state("…", true)` includes `m-empty` and `m-empty-icon`.

Run:

```bash
cargo test -p website content::
```

Expected failure before the helper implementation.

- [ ] **Step 2: Implement and export the Maud helpers**

Use Maud’s normal escaping for all server values. `cover` uses a linked cover when `href` is present, otherwise a non-linked figure; the card keeps the cover link and title link semantics compatible with current behavior. `empty_state` centers content and conditionally renders the icon. `breadcrumb` marks the last item active and sets `aria-current="page"` on its link. Add `pub mod content;` to `website/src/templates/mod.rs`.

Run:

```bash
cargo test -p website content::
```

Expected pass.

- [ ] **Step 3: Migrate server card consumers atomically**

Update the series list, source list, and bookmarks shell to use `manga_card`/`empty_state` and `columns is-mobile is-multiline`. Preserve all existing cover selection, chapter counts, `/series/{id}` links, `/manga/{id}` fallback links, anonymous access, and the `style="display:none"` contract for the bookmarks empty state. Do not add server-side bookmark queries.

- [ ] **Step 4: Migrate browser card consumers in the same commit**

Replace separate legacy bookmark/continue card markup with `buildCover`, `buildMangaCard`, and `buildEmptyState`. Preserve API ordering, `/api/progress-series?ids=...`, `/api/bookmarks`, localStorage fallback, initialization order, progress values, and bookmark API calls. Use `textContent` for fetched/localStorage values. Replace legacy toast markup with `notification is-danger`/`notification is-success` plus `data-toast`; keep the existing container and lifecycle.

- [ ] **Step 5: Add content primitives to `theme.css`**

Add the transparent shadow token override:

```css
--bulma-shadow-h: 0deg;
--bulma-shadow-s: 0%;
--bulma-shadow-l: 0%;
```

Bind `--bulma-card-background-color` and `--bulma-box-background-color` to `--bulma-scheme-main-bis`. Add `.m-cover` frame/ratio/placeholder/initial/badge/overlay states, `.m-empty-icon` geometry, `[data-toast]` positioning/transition, and reduced-motion rules. Use Bulma tokens only; do not consume legacy `--accent*`, `--bg-*`, or `--text-*` variables.

- [ ] **Step 6: Run the atomicity checks**

```bash
cargo fmt --all -- --check
cargo test -p website content::
cargo build -p website
git grep -nE 'manga-grid|manga-card|cover-wrapper|empty-state|continue-card|continue-scroll' -- \
  website/src/routes/home.rs website/src/routes/series.rs \
  website/src/routes/source.rs website/src/routes/bookmarks.rs website/static/app.js
```

The grep must return no live legacy matches. Commit all Rust, JS, and CSS changes together:

```bash
git add website/src/templates website/src/routes/home.rs website/src/routes/series.rs \
  website/src/routes/source.rs website/src/routes/bookmarks.rs website/static/app.js \
  website/static/theme.css
git commit -m "feat(web): share Bulma content cards across routes"
```

---

### Task 2: Migrate the home page

**Files:**
- Modify: `website/src/routes/home.rs`
- Modify: `website/static/app.js` only for home integration not completed in Task 1
- Modify: `website/static/theme.css` only for home-specific responsive geometry

**Preserve:** `relative_time()`, series/chapter queries, `home_sections` settings, `show()`, the always-present hero, recent limit 30, `/series` links, `/read/{manga_id}/{chapter_id}` links, and removal of continue cards without local progress.

- [ ] **Step 1: Replace the hero markup**

Use `.hero`, `.hero-body`, `.title.is-1`, `.subtitle.is-4.m-text-muted`, a wrapping Bulma `.level` for stats, and `a.button.is-primary.is-medium` for the browse CTA. Replace the stat divider markup with the level layout.

- [ ] **Step 2: Replace recent rows with media markup**

Render `ul#recent-chapters`; each item uses `a.media`, `media-content`, `.title.is-6`, `.subtitle.is-7`, `media-right`, and an unclassed `span[aria-hidden="true"]` for the existing arrow SVG. Keep the existing links and relative-time text.

- [ ] **Step 3: Preserve the continue-reading container**

Keep `#continue-reading-container` and make its heading `.title.is-3`. The browser builder renders a horizontally scrollable Bulma `columns is-mobile is-flex-wrap-nowrap`; cards use `progress.is-primary.is-small`, `.tag.is-primary.is-light`, and `.is-size-7.m-text-muted`. Do not add a second server-side progress query.

- [ ] **Step 4: Use the shared empty state**

Replace the home empty branch with `empty_state(..., true)`; no live `.empty-state*` class may remain.

- [ ] **Step 5: Verify the route and contracts**

```bash
cargo test -p website content::
curl -s http://localhost:3000/ | grep -oE 'hero|level|recent-chapters|continue-reading-container|m-empty'
```

At 375px, the page must not overflow horizontally; only the continue strip may scroll.

---

### Task 3: Migrate the series list and search header

**Files:**
- Modify: `website/src/routes/series.rs` (`series_list`)

- [ ] **Step 1: Replace the page header**

Use a `div.box` containing a Bulma `level`: `.title.is-2` on the left and a `form.field.has-addons` on the right. Keep the form as:

```html
<form action="/series" method="get">
  <input class="input" type="search" name="q" value="…">
  <button class="button is-primary" type="submit">Search</button>
</form>
```

Preserve the exact existing action, method, input name, reflected value, `q.trim()` filtering, zero-chapter filtering, and no-JavaScript behavior. Do not add client filtering, pagination, or new query parameters.

- [ ] **Step 2: Use the shared card grid and empty states**

Render `manga_card` inside `columns is-mobile is-multiline`; use `empty_state` for both no-results and no-series branches. Keep the current series and chapter counts and links.

- [ ] **Step 3: Verify the form contract**

```bash
curl -sG --data-urlencode 'q=One Piece' http://localhost:3000/series \
  | grep -oE '<form[^>]*>|name="q"|value="[^"]*"'
git diff --unified=0 -- website/src/routes/series.rs \
  | grep -E '^[+-].*(action=|method=|name=|type="search")'
```

The form contract must be unchanged. Confirm `/series?q=...` still returns the server-rendered result without JavaScript.

---

### Task 4: Migrate series detail, source filters, and chapter rows

**Files:**
- Modify: `website/src/routes/series.rs` (`series_detail`)
- Modify: `website/src/templates/content.rs` only if breadcrumb use/tests were not completed in Task 1
- Modify: `website/static/app.js`
- Modify: `website/static/theme.css`

- [ ] **Step 1: Use breadcrumbs in all existing detail branches**

Render the existing Home/Series/current breadcrumb with:

```html
<nav class="breadcrumb has-arrow-separator" aria-label="breadcrumb">
  <ul>
    <li><a href="/">Home</a></li>
    <li class="is-active"><a aria-current="page">…</a></li>
  </ul>
</nav>
```

Do not alter the existing `error_content` path or the series not-found behavior.

- [ ] **Step 2: Replace the detail hero**

Use `#series-detail-hero.media`, `.media-left`, `.media-content`, `m-cover`, `.title`, `.subtitle.is-6`, `.tags`, and `.buttons`. Preserve primary-cover selection, description condition, `pref_src`, primary chapter count, first-chapter link, `data-series-id`, `data-manga-id`, rescan text/disabled behavior, stale-series sync trigger, and the 15-second reload script.

- [ ] **Step 3: Replace source tabs with independent checkboxes**

Use:

```html
<div id="source-toggles" class="field is-grouped is-grouped-multiline"></div>
<button id="source-reset" class="button is-small is-ghost" type="button">Reset filters</button>
```

`app.js` creates a `p.control` containing `label.checkbox.m-source-toggle` and the source checkbox. Update selectors to `.m-source-toggle input`; preserve the `mangaread_sources` localStorage key, source values, independent filtering, and reset behavior. Do not use `tabs is-toggle`.

- [ ] **Step 4: Preserve loading and chapter behavior**

Use `.loader` for the series loading state and `.m-text-muted` for its hint. Do not rename the reader loading markup. Render:

```html
<ul id="chapter-list" data-series-id="…" data-sources="…">
  <li class="m-chapter-row" data-source="…" data-chapter-id="…">
    <a href="/read/…">
      <span class="ch-status-icon" aria-hidden="true"></span>
      <span class="chapter-num">Ch. …</span>
      <span class="chapter-source tag">source</span>
      <span class="ch-resume" style="display:none"></span>
      <span class="chapter-arrow" aria-hidden="true">…</span>
    </a>
  </li>
</ul>
```

Update `app.js` selectors to `#chapter-list > li`. Preserve `data-series-id`, `data-sources`, `data-source`, `data-chapter-id`, `ch-unread`, `ch-started`, `ch-finished`, `ch-resume`, read URLs, inline `style.display` filtering, and the absence of a `data-pages` emitter. A started chapter remains `Resume p.N`; no page-total suffix is added.

- [ ] **Step 5: Add the primitive styles and verify behavior**

Add `.m-chapter-row` and `.m-source-toggle` styles to `theme.css`; the row must wrap at 320/375px and must not use `display: ... !important`. Run:

```bash
cargo test -p website content::
cargo test -p website reader::
git grep -n 'data-pages' -- website/src || true
```

Browser evidence: initial filters reflect localStorage, toggling one source hides only its rows, reload preserves filters, reset removes the key and restores rows, and a started chapter renders `Resume p.N`.

---

### Task 5: Migrate source list, bookmarks shell, and legal pages

**Files:**
- Modify: `website/src/routes/source.rs`
- Modify: `website/src/routes/bookmarks.rs`
- Modify: `website/src/routes/legal.rs`
- Modify: `website/static/theme.css` only for legal geometry or shared content states

- [ ] **Step 1: Migrate the source list**

Use the shared breadcrumb, a Bulma `level` header, `.title.is-2`, a `.tag.m-text-muted` count, the shared card grid, and shared empty state. Preserve source path handling, chapter-count filtering, singular/plural copy, `/series/{id}` links, `/manga/{id}` fallback links, and `Nav::Series`.

- [ ] **Step 2: Migrate the bookmarks shell**

Use a `level` header with `.title.is-2`, `#bookmarks-container` with `columns is-mobile is-multiline`, and `#bookmarks-empty` as the shared `m-empty` with its existing `style="display:none"` contract. Keep all client rendering, server-first behavior, localStorage fallback, and API calls in `app.js`.

- [ ] **Step 3: Migrate legal pages**

Use:

```html
<div class="columns is-centered">
  <div class="column is-8-desktop is-10-tablet">
    <section class="content">
      <h1 class="title is-2">Terms of Service</h1>
      <p class="m-legal-body">Stored plain text…</p>
    </section>
  </div>
</div>
```

Preserve `legal_content()` Maud escaping, fallback settings, and empty-setting behavior. In `theme.css`, set `.m-legal-body { white-space: pre-wrap; overflow-wrap: anywhere; line-height: 1.7; }`. Do not use a full-height hero or CTA.

- [ ] **Step 4: Verify route output**

```bash
curl -s http://localhost:3000/source/demonicscans \
  | grep -oE 'class="[^"]*tag[^"]*"|/series/[0-9]+|/manga/[0-9]+'
curl -s http://localhost:3000/terms | grep -o 'm-legal-body\|class="content"'
```

Seed one linked and one unlinked source manga and verify both link forms. Store legal text containing newlines and `<b>literal</b>` and verify it renders literally with line breaks.

---

### Task 6: Amend the design spec with the Slice B rulings

**File:**
- Modify: `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

Record the implemented decisions and contracts:

1. Source filters are independent checkboxes (`m-source-toggle`), not `tabs is-toggle`.
2. Chapter navigation is `ul > li.m-chapter-row`, with the five retained child hooks.
3. `m-cover` owns frame, 2:3 ratio, placeholder/initial, badge, and overlay.
4. `m-empty`, `m-empty-icon`, and `m-cover` are used by both Rust and `app.js`.
5. Bookmarks are client-rendered through the shared JS card builder.
6. List headers use Bulma `level`; `/manga/{id}` fallback links remain.
7. Breadcrumbs use the shared helper and Bulma `breadcrumb`.
8. Legal uses `m-legal-body`, distinct from `error_content`.
9. Home uses `media` rows, `media-right`, and `level`/tag stats.
10. Series loading uses `.loader`.
11. `m-text-muted` covers home time, loading hints, and source counts.
12. The series search form/query contract is unchanged.
13. The shared grid requires `columns is-mobile is-multiline` and the exact 2/3/4 column modifiers.
14. `data-pages` remains absent; page-total resume suffix remains deferred.

Also record the `m-cover` data hooks and transparent shadow-token decision. Do not modify the reader sections or the master feature spec.

- [ ] **Step 1: Review the spec diff for scope**

```bash
git diff --check
git diff -- docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md
```

- [ ] **Step 2: Commit the ruling record**

```bash
git add docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md
git commit -m "docs(web): record Slice B content migration rulings"
```

---

### Task 7: Slice B verification and evidence

**Files:**
- No production changes unless a check finds a regression.

- [ ] **Step 1: Run Rust/build checks**

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build -p website
cargo test -p website config::contrast -- --nocapture
cargo test -p website reader:: -- --nocapture
```

- [ ] **Step 2: Run the static contract audit**

```bash
git grep -nE 'has-text-grey|has-text-grey-' -- website/src website/static/app.js
git grep -nE 'tabs[^\n]*is-toggle|class="panel' -- website/src/routes website/static/app.js
git grep -n 'style.css' -- website/src/templates website/src/routes
git grep -n 'data-pages' -- website/src || true
```

Expected: no live `has-text-grey*`, source-filter tabs, chapter panels, `style.css` links, or `data-pages` emitter. Legacy definitions may remain in `style.css` and `static/themes/*.css` until Slice E.

- [ ] **Step 3: Run the route sweep**

Use a temporary database, never the repository’s `mangaread.db`:

```bash
for p in / /series /bookmarks /source/demonicscans /terms /privacy; do
  printf '%-24s %s\n' "$p" "$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:3000$p")"
done
```

All listed routes must return `200`. Also verify `/series?q=...`, `/manga/{linked}` and `/manga/{unlinked}` fallback behavior, and unchanged `/settings`/`/admin` auth behavior.

- [ ] **Step 4: Run browser/CDP evidence**

At 375, 768, 1024, and 1440px verify: 2/3/4 grid columns; 2:3 covers; no horizontal page overflow; home hero/stats/continue/recent layout; stacked series hero below 768px; wrapping source checkboxes; non-overflowing chapter rows; server/browser card parity; legal newlines/literal markup; no console errors or 404s; `--bulma-primary-h: 38deg`; muted text via `m-text-muted`; and no white card/box glow. Verify bookmark, continue-reading, source-filter, reset, and rescan flows against their existing API/localStorage contracts.

- [ ] **Step 5: Measure the live cascade**

```bash
for f in /static/vendor/bulma.min.css /static/theme.css; do
  raw=$(curl -s -o /dev/null -w '%{size_download}' "http://localhost:3000$f")
  gzip=$(curl -s -H 'Accept-Encoding: gzip' -o /dev/null -w '%{size_download}' "http://localhost:3000$f")
  printf '%-34s raw=%8d gzip=%8d\n' "$f" "$raw" "$gzip"
done
```

Do not measure `style.css` as shipped; it is intentionally unlinked until Slice E.

- [ ] **Step 6: Commit only real fixes and close the slice**

```bash
git diff --check
git status --short
git add -A
git commit -m "chore(web): close Slice B content migration" --allow-empty
```

Record any unavailable browser/CDP evidence, Chrome-driver failure, or unverified boundary explicitly rather than claiming it passed.

## Explicit Deferrals

- Do not delete `style.css`, legacy theme CSS, `THEMES`, or `theme_css()`; Slice E owns cleanup.
- Do not change reader markup, `reader.js`, or `reader.css`; Slice D owns the reader.
- Do not add a `data-pages` attribute or page-total resume suffix.
- Do not change API routes, auth/role checks, query parameters, localStorage keys, or sync behavior.
- Do not remove source labels or alter the existing `/manga/{id}` fallback semantics.
