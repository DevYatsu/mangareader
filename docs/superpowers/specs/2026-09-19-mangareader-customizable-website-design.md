# MangaReader — Full Featured Customizable Website (no plugins) — Design

Date: 2026-09-19
Status: draft for review
Scope: everything built-in. No plugin system. All customization ships in the binary.

## 1. Context & constraints

Current state:
- Rust workspace: `mangaread-core`, `scraper`, `website`.
- Web: Axum 0.8 + Maud 0.27 + Tokio + tower-http ServeDir.
- DB: SQLite rusqlite bundled, WAL, `Mutex<Connection>`.
- Frontend: vanilla CSS (2318 lines single file) + vanilla JS + localStorage.
- Theming: `.env` only (`SITE_TITLE`, `SITE_COLOR`, `SITE_BG`) + `css_overrides()` injecting `:root` vars.
- Routes: `/`, `/series`, `/series/{id}`, `/source/{source}`, `/manga/{id}`, `/read/{m}/{ch}`, `/bookmarks`, `/api/*`, `/health`, `/static/*`.
- No auth. No settings UI. No admin. `lang="en"` hardcoded. 2 hardcoded scrapers.

Constraints kept:
- Self-host first. Single Docker image. SQLite default.
- Stay in Rust for core. Vanilla CSS/JS stays unless a clear win demands change.
- No runtime plugin API. No WASM loaders. No dynamic libs. Customization = DB settings + UI + built-in sources.

## 2. Goals

- Any reader customizes reading without code.
- Any owner customizes branding, homepage, nav, SEO, legal without code and without restart.
- All data syncs across devices when logged in. Anonymous mode keeps working with localStorage with one-click import.
- All admin tasks happen in `/admin` with logs. All user tasks happen in `/settings`.
- Performance stays fast on Raspberry Pi class hardware for 1-20 users.

Non-goals:
- No third-party plugin marketplace.
- No multi-tenant SaaS billing.
- No native mobile apps (PWA covers offline).

## 3. Identity, accounts, sync

- Auth: password + OAuth (Google, GitHub, Discord). Sessions in DB with `tower-sessions`. Remember-me. Password reset via mail or admin reset link.
- Roles: `anon`, `member`, `editor`, `admin`. Middleware per route.
- Server tables: `users`, `sessions`, `bookmarks`, `progress (user_id, chapter_id, page, updated_at)`, `history`, `notes`, `profiles`.
- Migration path: anonymous localStorage -> account import button. Export JSON at any time.
- Profiles: multiple profiles per account (e.g. family server). Each profile has own shelves and progress.
- Stats: pages/day, streak, time read, completion %. Private by default, shareable toggle.
- Import/export: MyAnimeList, AniList, Kavita, Komga, localStorage JSON, CBZ sidecar.

## 4. Reader 2.0 (built-in, per-user + per-series overrides)

- Modes: single page, long strip, double page, RTL, vertical scroll. Per-series default + per-session override.
- Fit: width slider (400-1400px), fit-height, fit-screen, gap slider, background (black/white/sepia/custom), border/shadow toggle.
- Pipeline: WebP/AVIF transcode cache, blur-up placeholder, next-chapter prefetch (3 chapters), low-bandwidth mode.
- Input: clickable zones editor, keyboard remap UI, swipe sensitivity, auto-next chapter toggle.
- Comfort: auto-dim after idle, focus mode (hides nav/comments), hide-UI timer, remember zoom per device.
- Continuity: resume badge, "Next up" queue, offline download toggle per series.
- Notes: private note per chapter + page anchor. Spoiler flag for comments.

## 5. Library, discovery, CMS (built-in)

- Search: SQLite FTS5 over title, alt titles, author, genres, synopsis. Typo-tolerant fallback. Saved searches.
- Filters: source, status (ongoing/completed/hiatus), genre tags, year, rating, progress (unread/started/finished), language.
- Sort: recently updated, recently added, rating, popularity, random, last read.
- Series page tabs: Chapters | Details | Related | Comments. Stale badge + manual resync button.
- Shelves: built-in (Reading, Completed, On Hold, Dropped, Plan to Read) + unlimited custom shelves. Drag to reorder. Public/private toggle.
- Bulk actions: mark read/unread, download, retag, move shelf.
- Notifications: in-app bell + mail + RSS per series + webhook (Discord/Slack URL in settings). Digest frequency control.
- Public feeds: global RSS, per-series RSS, JSON API (`/api/v1/*` with token), OPDS for Tachiyomi/Panels/KOReader.

## 6. User customization (`/settings`)

- Sections: Appearance | Reader defaults | Homepage | Language | Notifications | Privacy | Data.
- Appearance: dark/light/system, 8 built-in themes, accent picker, font family/size/line-height, UI density (comfortable/compact), corner radius slider. Live preview panel.
- Homepage: toggle hero, shelf order (continue, subscriptions, new chapters, stats), grid density (2-8 columns), show/hide NSFW.
- Reader defaults: mode, fit, gap, background, auto-next, prefetch on/off. Per-series override editor lists exceptions.
- Language: UI locale picker, content language filter, RTL auto-switch.
- Data: export all, delete history, per-device sessions list with revoke.

## 7. Owner customization (`/admin`, no restart, no code)

- Site settings table: `site_settings(key,value,updated_by,updated_at)` replaces `.env` branding. `.env` remains as boot defaults only.
- Branding: title, logo upload (SVG/PNG), favicon generator, accent + background pickers, custom CSS box with variable autocomplete, custom JS head/footer boxes (CSP-safe), live draft preview + publish/rollback.
- Themes: 8 presets editable. Duplicate preset -> custom theme. Export/import theme JSON. Schedule dark theme by hour.
- Homepage builder: blocks (hero, continue reading, subscriptions grid, curated shelf, stats banner, announcement). Reorder via drag. Per-block visibility (anon/member only).
- Navigation editor: top nav links, footer columns, social buttons, announcement bar with start/end dates.
- Pages: built-in editor for About, Help, DMCA, Privacy, Terms. Markdown + Maud sanitized render. Version history.
- SEO: meta title/description templates, OpenGraph image default, sitemap toggle, robots.txt editor, JSON-LD for series.
- Multi-library: one binary can serve 2+ libraries (e.g. Manga + Manhwa) each with own slug, theme override, nav. Shared users table.
- White-label: hide MangaRead footer, custom powered-by text.

## 8. Sources (built-in only, since plugins out of scope)

- Keep `SiteAdapter` trait in Rust. Add 6-10 built-in adapters prioritized by user vote. Each adapter ships compiled.
- Source manager UI: enable/disable, priority drag, rate limit (req/min), timeout, test-fetch button with last-error display.
- Scheduler UI: global 6h default + per-source cron override, manual Run Now, failure log with retry, backoff display.
- Dedup tool: normalized-title merge preview, split/merge buttons, cover picker when 2 sources disagree.
- Importer: MangaDx API key field, local folder watcher (CBZ/CBR/PDF) with auto-scan, ComicInfo.xml parsing.

## 9. Social (built-in, moderated)

- Ratings (1-10), reviews, tag votes. Editor queue for tag corrections.
- Follows + shared shelves + activity feed (opt-in). Public profile `/u/{name}` with shelves and stats.
- Per-series threads + per-chapter comments with spoiler blur. Report button -> mod queue.
- Block/mute users. Instance-wide defederation not needed (single instance).

## 10. PWA, mobile, offline (built-in)

- PWA manifest + install prompt + offline shell. Icon generation from logo.
- Offline: per-series download (transcoded size estimate shown), storage quota per user, LRU eviction, airplane-mode reader.
- Mobile: bottom nav, swipe zones, safe-area CSS, haptic-friendly large tap targets.
- Reading sync: progress heartbeat every 10s + on page change. Conflict rule: latest timestamp wins, history kept 30d.

## 11. i18n, legal, accessibility

- UI locales: en, fr, es, ar (RTL test case). `Accept-Language` detection + manual override. Locale files in `website/locales/*.ftl`.
- Content language filter per series. NSFW blur + age gate toggle (owner setting).
- Legal: DMCA takedown form -> admin queue with action log. Privacy page auto-lists stored fields.
- A11y: focus rings, keyboard-only reader path, alt text for covers, contrast-checked presets, reduced-motion respect.

## 12. Ops, performance, reliability (built-in)

- DB: connection pool (`r2d2`), WAL kept, FTS5 index, nightly `VACUUM` + backup to `backups/` + optional S3 fields in admin.
- Jobs table: `jobs(id,type,status,run_at,log)` powers sync, transcode, backup, mail digest. UI shows queue + retry.
- Cache: image resize disk cache (`cache/`), ETag + `Cache-Control`, chapter JSON pre-render.
- Observability: `/health`, `/metrics` (Prometheus text), structured logs with request id, slow-query threshold setting.
- Docker: non-root, healthcheck, bundled chromedriver auto-update flag, `MANGAREAD_*` env still works as boot overrides.

## 13. Data model deltas (new tables, SQLite)

- `users(id, name, email unique, hash, role, locale, created_at)`
- `sessions(id, user_id, token, expires_at, device)`
- `profiles(id, user_id, name)`
- `bookmarks(user_id/profile_id, series_id, created_at)`
- `progress(user_id/profile_id, chapter_id, page, updated_at)`
- `shelves(id, owner_id, name, visibility, sort)` + `shelf_items(shelf_id, series_id, sort)`
- `site_settings(key primary, value, updated_by, updated_at)`
- `themes(id, name, tokens JSON, is_public, owner)`
- `pages(slug, title, body_md, visibility, updated_at)`
- `jobs(id, type, payload JSON, status, log, run_at)`
- `notifications(user_id, type, ref_id, read_at)`
- Indexes on `progress(updated_at)`, `chapters(series_id, num)`, FTS on `series(title, synopsis)`.

## 14. Routes deltas

- New: `/settings`, `/settings/*`, `/u/{name}`, `/shelves`, `/shelf/{id}`, `/admin`, `/admin/*`, `/login`, `/register`, `/logout`, `/oauth/*`, `/api/v1/*`, `/opds/*`, `/rss/*`.
- Keep: `/`, `/series`, `/read`, `/bookmarks` (becomes redirect to shelf when logged in).

## 15. Rollout order (no plugins)

1. Auth + server sync + import (unblocks all personalization).
2. Settings UI + themes + reader 2.0 defaults.
3. Search FTS + filters + shelves + notifications.
4. Admin + site_settings + theme builder + page builder.
5. PWA/offline + OPDS/RSS/API + i18n/legal.
6. Built-in source expansion + scheduler UI + jobs/metrics/backups.

## 16. Acceptance checks

- Owner changes logo/accent/homepage in admin, sees it live with no restart and no code edit.
- Reader changes width/gap/background/mode and it persists across devices when logged in.
- Anonymous reader still works, then imports localStorage to account in one click.
- Admin publishes announcement + custom page without deploy.
- Lighthouse mobile >= 90 on home and reader with 3G throttling (excludingacker images).
- Backup + restore documented and tested from admin with one download file.
