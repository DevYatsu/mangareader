# Slice 2: Settings + Owner Customization + Admin Themes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Users edit their profile in `/settings`; admins rebrand the whole site (title, accent, theme preset, SEO, legal, homepage) in `/admin` with no restart; three built-in visual themes (Webtoon-clean, Asura-dark, Demonic-crimson) are admin-selectable with live preview and per-user override.

**Architecture:** `site_settings(key,value,updated_by,updated_at)` KV table is the single source of branding truth; `SiteConfig::from_env` stays as boot defaults only and a new `site_config(conn)` helper overlays DB values per request (no reload logic, no stale state). Themes are pure CSS: same Maud markup, one extra `<link>` to `/static/themes/{preset}.css` overriding `:root` tokens. `users.theme` (`''` = follow site) gives per-user override.

**Tech Stack:** Rust edition 2024, Axum 0.8, Maud 0.27, rusqlite 0.32 (bundled), vanilla JS + new vanilla CSS theme files.

**Spec:** `docs/superpowers/specs/2026-09-19-mangareader-customizable-website-design.md` (§6 user customization minus stats, §7 owner customization minus theme builder/export/schedule/multi-library, §16 acceptance)

## Global Constraints

- Self-host first. Single Docker image. SQLite default (WAL + busy_timeout, existing pattern; PRAGMA foreign_keys=ON from Slice 1 stays).
- Stay in Rust for core. Vanilla CSS/JS stays.
- No runtime plugin API. No WASM loaders. No dynamic libs. Themes are static CSS files only.
- One account = one reader identity. No profiles.
- Readers never see sources, adapters, sync state, or origin URLs. This slice touches no reader-visible scraping UI (`/source/*` removal stays a Slice-5 job).
- OAuth, reading stats, theme export/import JSON, dark-schedule, multi-library, 8-preset library are NOT in this slice. Three presets only: `webtoon`, `asura`, `demonic`.

---

## File Structure

New files and their single responsibility:

- `website/src/routes/settings.rs` — `GET /settings`, `POST /settings/profile` (name, locale), `POST /settings/password` (verify current + bcrypt new), `POST /settings/appearance` (theme override). All require `AuthUser` (member+).
- `website/src/routes/admin.rs` — `GET /admin` dashboard, `GET/POST /admin/branding` (title, tagline, accent, bg, theme preset radio, SEO description, terms, privacy, homepage toggles). All require `require_role(&user, Role::Admin)`.
- `website/src/routes/legal.rs` — `GET /terms`, `GET /privacy` rendering stored text with built-in fallback.
- `website/static/themes/webtoon.css` — light clean theme (inspired by Webtoon: white surfaces, green accent, airy cards).
- `website/static/themes/asura.css` — dark theme (inspired by AsuraScans: near-black navy surfaces, violet accent, dense update rows).
- `website/static/themes/demonic.css` — dark theme (inspired by DemonicScans: black-red surfaces, crimson accent, heavy headers).
- `website/static/admin.js` — branding-page live preview (swap theme `<link>` href + accent var on input, no save needed to preview).

Modified files:

- `mangaread-core/src/db.rs` — `site_settings` table + `get_setting`/`set_setting`/`all_settings`; `users.theme` migration via PRAGMA table_info guard; `update_user_profile` (name, locale, theme), `update_user_password`.
- `website/src/config.rs` — keep `from_env`; add `from_settings(map: HashMap<String,String>)` overlay + `site_config(conn) -> SiteConfig` helper (env boot defaults, DB wins per key).
- `website/src/main.rs` — remove `config: SiteConfig` from AppState (single source = DB); register settings/admin/legal routes; bootstrap log line.
- `website/src/routes/mod.rs` — add `pub mod settings; pub mod admin; pub mod legal;`
- `website/src/templates/layout.rs` — `base_layout` + `reader_layout` gain `theme: &str` param (empty = no extra link, current look); `error_layout` gains `user + theme` (session-aware nav fix); every caller updated via grep; user-name nav link moves from `/bookmarks` to `/settings`.
- Every handler reading `state.config.site_title` — switch to `site_config(&conn)` helper (same grep sweep as theme param).
- `website/static/style.css` — append only: settings/admin form styles, theme-preview cards, legal page styles.
- `website/src/routes/auth_pages.rs` — bootstrap log gate `id == 1` → `role == Admin` (one-line deferred fix).

---

### Task 1: site_settings table + user profile repository

**Files:**
- Modify: `mangaread-core/src/db.rs`
- Test: unit tests inside `mangaread-core/src/db.rs` (`#[cfg(test)]`, in-memory SQLite)

**Interfaces:**
- Consumes: existing `init_db`, `Error`.
- Produces (used by Tasks 2-4):
  - `db::get_setting(conn, key: &str) -> Result<Option<String>, Error>`
  - `db::set_setting(conn, key: &str, value: &str, updated_by: i64) -> Result<(), Error>` (upsert on key)
  - `db::all_settings(conn) -> Result<HashMap<String, String>, Error>`
  - `users.theme TEXT NOT NULL DEFAULT ''` column via migration in `init_db`: check `PRAGMA table_info(users)` for `theme`, `ALTER TABLE` only if missing (init_db must stay idempotent for existing DBs).
  - `db::update_user_profile(conn, user_id: i64, name: &str, locale: &str, theme: &str) -> Result<(), Error>`
  - `db::update_user_password(conn, user_id: i64, new_hash: &str) -> Result<(), Error>`
  - Setting keys (exact strings, used by Tasks 2-4): `site_title`, `site_tagline`, `site_accent`, `site_bg`, `site_theme`, `seo_description`, `legal_terms`, `legal_privacy`, `home_sections` (CSV subset of `continue,latest,popular,bookmarks`, default all four).

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn settings_roundtrip_and_profile_update() {
    let conn = mem();
    assert_eq!(get_setting(&conn, "site_title").unwrap(), None);
    set_setting(&conn, "site_title", "MyDen", 1).unwrap();
    assert_eq!(get_setting(&conn, "site_title").unwrap().as_deref(), Some("MyDen"));
    let id = create_user(&conn, &NewUser {
        name: "r".into(), email: "r@x.io".into(),
        password_hash: "h".into(), role: Role::Member,
    }).unwrap();
    update_user_profile(&conn, id, "renamed", "fr", "asura").unwrap();
    let u = find_user_by_email(&conn, "r@x.io").unwrap();
    assert_eq!(u.name, "renamed");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mangaread-core settings_roundtrip_and_profile_update`
Expected: FAIL with "cannot find function `set_setting`" (or unresolved import).

- [ ] **Step 3: Write minimal implementation**

```sql
CREATE TABLE IF NOT EXISTS site_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL DEFAULT '',
    updated_by INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

`set_setting` SQL: `INSERT INTO site_settings (key, value, updated_by, updated_at) VALUES (?1, ?2, ?3, datetime('now')) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_by = excluded.updated_by, updated_at = datetime('now')`. Migration: `let has_theme: bool = ... PRAGMA table_info(users) ...; if !has_theme { conn.execute_batch("ALTER TABLE users ADD COLUMN theme TEXT NOT NULL DEFAULT ''")?; }`. Note: existing `User` struct gains `theme: String` — update its row-mapping query + struct (find in domain/user.rs and db.rs row mappers).

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mangaread-core`
Expected: PASS (all tests, including Slice 1 suite).

- [ ] **Step 5: Commit**

```bash
git add mangaread-core/src/db.rs
git commit -m "feat: site_settings KV store, users.theme, profile repository"
```

### Task 2: DB-backed SiteConfig, AppState.config removal

**Files:**
- Modify: `website/src/config.rs`, `website/src/main.rs`, every handler reading `state.config` (grep `state\.config` first)
- Test: unit tests inside `website/src/config.rs`

**Interfaces:**
- Consumes: Task 1 (`all_settings`, `get_setting`).
- Produces (used by Tasks 3-5):
  - `SiteConfig::from_settings(map: &HashMap<String, String>) -> Self` — starts from `from_env()` values, overlays `site_title`, `site_accent` (recompute hover + rgb via existing `lighten`/`parse_hex`), `site_bg` when present and non-empty. Invalid hex falls back to current value (never blank).
  - `pub fn site_config(conn: &rusqlite::Connection) -> SiteConfig` — `all_settings` then `from_settings`; on DB error returns `from_env()` (never 500 a page for a settings read).
  - `pub const THEMES: &[(&str, &str)] = &[("default","Default dark"),("webtoon","Webtoon clean"),("asura","Asura dark"),("demonic","Demonic crimson")]` — single allowlist used by layout link builder, settings/appearance validation, admin picker.
  - `pub fn theme_css(theme: &str) -> Option<String>` — `Some("/static/themes/{theme}.css")` when theme is `webtoon|asura|demonic`, else `None` (unknown/empty/default → no extra link, current look preserved).
  - AppState loses `config` field; `use crate::config::SiteConfig` import removed from main.rs; boot keeps `SiteConfig::from_env()` only for the startup log line.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn settings_overlay_beats_env() {
    let mut map = HashMap::new();
    map.insert("site_title".into(), "MyDen".into());
    map.insert("site_accent".into(), "#00dc64".into());
    map.insert("site_accent".into(), "not-a-color".into());
    let c = SiteConfig::from_settings(&map);
    assert_eq!(c.site_title, "MyDen");
    assert_eq!(c.accent, "#f59e0b"); // invalid hex keeps boot default
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p website settings_overlay_beats_env`
Expected: FAIL with "cannot find function `from_settings`".

- [ ] **Step 3: Write minimal implementation** (overlay + helper + allowlist + AppState change + update all `state.config` readers; grep first: `grep -rn "state\.config" website/src`)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p website`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add website/src/config.rs website/src/main.rs website/src/routes/
git commit -m "feat: DB-backed site config with env boot defaults"
```

### Task 3: /settings user pages

**Files:**
- Create: `website/src/routes/settings.rs`
- Modify: `website/src/routes/mod.rs`, `website/src/main.rs` (routes), `website/src/templates/layout.rs` (nav name link `/bookmarks` → `/settings`), `website/static/style.css` (append form styles)
- Test: `cargo build` + curl flow (steps below)

**Interfaces:**
- Consumes: Task 1 (`update_user_profile`, `update_user_password`, `find_user_by_email`), Task 2 (`THEMES`, `site_config`), Slice 1 (`AuthUser`, `hash_password`, `verify_password`).
- Produces: `GET /settings` (profile + appearance + password forms, one page three sections); `POST /settings/profile` fields `name` (1-32), `locale` (allowlist `en,fr,es,de,ja` — anything else → 400 re-render; page stays English until Slice 6 i18n); `POST /settings/password` fields `current`, `new` (≥8, verify current first, identical 400 message on failure — no oracle); `POST /settings/appearance` field `theme` (must be in `THEMES` keys else 400). All POSTs require `AuthUser`; success → `303` to `/settings`.

- [ ] **Step 1: Write the failing check**

Run: `curl -s http://localhost:3000/settings -o /dev/null -w '%{http_code}\n'` (server from Task 2 build) — but server isn't running; instead: `grep -rn "settings" website/src/routes/mod.rs` Expected: no match (module does not exist yet).

- [ ] **Step 2: Implement** (route module + Maud forms + routes + nav link change + CSS append)

- [ ] **Step 3: Verify build**

Run: `cargo build -p website`
Expected: exit 0.

- [ ] **Step 4: Verify settings flow with curl**

```bash
MANGAREAD_DB=/tmp/s2.db cargo run -p website &
sleep 3
curl -s -c /tmp/cj -X POST --data-urlencode 'name=a' --data-urlencode 'email=a@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null
curl -s -b /tmp/cj http://localhost:3000/settings | grep -o 'Settings' | head -1
curl -s -b /tmp/cj -X POST --data-urlencode 'name=anna' --data-urlencode 'locale=fr' --data-urlencode 'theme=asura' http://localhost:3000/settings/profile -o /dev/null -w '%{http_code}\n'
curl -s -b /tmp/cj http://localhost:3000/settings | grep -o 'anna' | head -1
curl -s http://localhost:3000/settings -o /dev/null -w '%{http_code}\n'
```

Expected: `Settings`, `303`, `anna`, `401` (logged-out rejected — 401 JSON only for Accept: application/json; browser form POST without auth redirects to /login per Slice 1 extractor, so accept either 401 or 303-to-login and record which). Kill server, delete `/tmp/s2.db*`, `/tmp/cj`.

- [ ] **Step 5: Commit**

```bash
git add website/src/routes/settings.rs website/src/routes/mod.rs website/src/main.rs website/src/templates/layout.rs website/static/style.css
git commit -m "feat: user settings page (profile, password, appearance)"
```

### Task 4: /admin branding + theme picker + legal + homepage flags

**Files:**
- Create: `website/src/routes/admin.rs`, `website/src/routes/legal.rs`, `website/static/admin.js`
- Modify: `website/src/routes/mod.rs`, `website/src/main.rs` (routes), `website/src/templates/layout.rs` (admin nav link shown only when `user.role == Admin`), `website/static/style.css` (append)
- Test: `cargo build` + curl flow (steps below)

**Interfaces:**
- Consumes: Task 1 (`set_setting`, `all_settings`), Task 2 (`THEMES`, `site_config`), Slice 1 (`AuthUser`, `require_role`).
- Produces:
  - `GET /admin` — dashboard with links (Branding, Users note "Slice 5", Sources note "Slice 5") + current theme name. Non-admin → `403`; logged-out → login redirect (via `AuthUser` extractor + `require_role(&user, Role::Admin)`).
  - `GET/POST /admin/branding` — fields: `site_title` (1-64), `site_tagline` (0-140), `site_accent` + `site_bg` (must match `^#[0-9a-fA-F]{6}$` else 400 re-render), `site_theme` radio (must be in `THEMES` keys), `seo_description` (0-300), `legal_terms` + `legal_privacy` (0-20000 chars, plain text rendered escaped), `home_sections` (checkboxes → CSV). POST writes each via `set_setting(conn, key, value, user.id)`, `303` to `/admin/branding`. Invalid input → 400 re-render with error, nothing written (validate all before any write).
  - `GET /terms`, `GET /privacy` — render stored text (fallback: "Terms of Service — coming soon." / "Privacy Policy — coming soon."), `error_layout`-style shell via `base_layout`.
  - `admin.js` (~40 lines): on branding page, theme radio change swaps the theme `<link id="theme-css">` href live; accent color input updates `--accent` on `:root` live. Pure preview — no fetch, save still goes through the form.
  - Deferred Slice-1 minor folded in: bootstrap log gate `id == 1` → `role == Admin` in `auth_pages.rs` (include in this task's commit).

- [ ] **Step 1: Write the failing check**

Run: `grep -rn "admin" website/src/routes/mod.rs`
Expected: no match.

- [ ] **Step 2: Implement** (admin + legal modules, routes, nav link, admin.js, CSS append, bootstrap log gate)

- [ ] **Step 3: Verify build**

Run: `cargo build -p website`
Expected: exit 0.

- [ ] **Step 4: Verify admin flow with curl**

```bash
MANGAREAD_DB=/tmp/s2b.db cargo run -p website &
sleep 3
curl -s -c /tmp/cj -X POST --data-urlencode 'name=owner' --data-urlencode 'email=o@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null
curl -s -b /tmp/cj http://localhost:3000/admin -o /dev/null -w '%{http_code}\n'
curl -s -b /tmp/cj -X POST --data-urlencode 'site_title=MyDen' --data-urlencode 'site_tagline=read more' --data-urlencode 'site_accent=#00dc64' --data-urlencode 'site_bg=#ffffff' --data-urlencode 'site_theme=webtoon' --data-urlencode 'seo_description=d' --data-urlencode 'legal_terms=t' --data-urlencode 'legal_privacy=p' --data-urlencode 'home_sections=latest,popular' http://localhost:3000/admin/branding -o /dev/null -w '%{http_code}\n'
curl -s -b /tmp/cj http://localhost:3000/ | grep -o 'MyDen' | head -1
curl -s -b /tmp/cj http://localhost:3000/ | grep -o '/static/themes/webtoon.css' | head -1
curl -s -c /tmp/cj2 -X POST --data-urlencode 'name=bob' --data-urlencode 'email=b@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null
curl -s -b /tmp/cj2 http://localhost:3000/admin -o /dev/null -w '%{http_code}\n'
```

Expected: `200`, `303`, `MyDen` (title live with no restart), `/static/themes/webtoon.css` (theme link served), `403` (member blocked). Kill server, delete `/tmp/s2b.db*`, `/tmp/cj*`.

- [ ] **Step 5: Commit**

```bash
git add website/src/routes/admin.rs website/src/routes/legal.rs website/static/admin.js website/src/routes/mod.rs website/src/main.rs website/src/templates/layout.rs website/static/style.css website/src/routes/auth_pages.rs
git commit -m "feat: admin branding, theme picker with preview, legal pages"
```

### Task 5: Three theme stylesheets (designer lane)

**Files:**
- Create: `website/static/themes/webtoon.css`, `website/static/themes/asura.css`, `website/static/themes/demonic.css`
- Modify: `website/src/templates/layout.rs` only if the `<link id="theme-css">` hook needs adjusting (read-only first; prefer no change)
- Test: visual checklist in browser (steps below) + `curl` link presence

**Interfaces:**
- Consumes: Task 2 (`theme_css` href contract: layout emits `<link id="theme-css" rel="stylesheet" href="/static/themes/{preset}.css">` right after `/static/style.css`; `admin.js` swaps that href for preview). Existing `:root` token list in `style.css:9-53` (bg-page/body/card/hover/elevated/input, text-primary/secondary/muted/dim, accent + rgb + hover/glow/dim, border/subtle/hover, radius-sm/md/lg/xl, shadow-sm/md/lg/glow, transitions, font-display/body, max-width, container-padding). `css_overrides()` inline `<style>` comes after and wins on `--accent*`/`--bg-page` — themes must not fight it: themes set the full token set, admin accent/BG still wins (documented layering: style.css < theme.css < inline overrides).
- Art direction (inspired-by, not clones):
  - `webtoon`: light clean reader-brand look — white `#ffffff` surfaces, ink `#1a1a1a` text, signature green `#00DC64` default accent hooks (still overridable), generous radius (16-20px cards), airy section spacing, pill genre chips, prominent hero strip on home.
  - `asura`: dark scanlator look — near-black navy `#0b0e17` page, `#151b2e` cards, violet `#8b5cf6` accent with cyan `#22d3ee` secondary glow, dense "latest updates" rows with borders instead of cards, compact uppercase section headers with accent bar.
  - `demonic`: dark crimson look — pure-black `#0a0a0a` page, `#141414` cards, crimson `#e11d48` accent, heavy condensed headers, red glow on featured covers, sharp 6-8px radii.
- Each file: `:root` token overrides + ≤150 lines of component rules scoped to existing classes only (`.site-header`, `.series-card`, `.chapter-row`, `.btn`, `.nav`, `.hero`, `.section-title`, etc. — grep style.css for exact class names; no markup changes, no new fonts beyond the two already loaded).

- [ ] **Step 1: Write the failing check**

Run: `ls website/static/themes/`
Expected: "No such file or directory".

- [ ] **Step 2: Implement** the three stylesheets per direction above (read style.css class names first; keep each file self-contained).

- [ ] **Step 3: Verify in browser**

Checklist: (1) set each theme via `/admin/branding`, home + series + reader pages render with no unstyled flash and no horizontal overflow at 375px and 1440px; (2) admin accent override still wins over theme accent (inline style order); (3) per-user override (`/settings/appearance`) shows that user's theme while logged-out visitors see the site theme; (4) unknown theme value in DB renders default look (no broken link).

- [ ] **Step 4: Commit**

```bash
git add website/static/themes/
git commit -m "feat: three admin-selectable site themes (webtoon, asura, demonic)"
```

### Task 6: Slice acceptance + full verify

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`
Expected: PASS, zero failures.

- [ ] **Step 2: Release-mode build + Docker smoke**

Run: `cargo build --workspace` then `docker compose up --build -d && sleep 8 && curl -s http://localhost:3000/health && docker compose down`
Expected: build clean, `/health` returns ok.

- [ ] **Step 3: Spec acceptance replay** (§6/§7/§16 as applicable to slice 2)

  - Env-only restart branding is gone: change title via `/admin/branding`, homepage shows it with no restart.
  - All three themes selectable, preview works without saving, user override independent of site theme.
  - Non-admin gets 403 on `/admin*`; logged-out gets login redirect; readers see no sources/admin traces.
  - `/terms` + `/privacy` render stored text.
  - `grep -ri "demonicscans\|manhuaplus" website/src/templates/` count unchanged from Slice 1 (0).

- [ ] **Step 4: Commit** (only if verification forced a fix; otherwise no commit — record "no changes" in the task result)

---

## Self-Review

**1. Spec coverage (§6/§7 minus deferred):** user profile + appearance + password → Task 3. Site settings table replacing `.env` branding (env = boot defaults) → Tasks 1-2. Admin branding/SEO/legal/homepage → Task 4. Themes admin-selectable → Tasks 4-5 (3 presets now; 8-preset library, builder, export/import, schedule, multi-library explicitly deferred with reason: one good picker beats half a builder). Deferred Slice-1 minors folded: error_layout session-aware (Task 4 layout touch), bootstrap log gate (Task 4 commit).

**2. Placeholder scan:** no TBD/TODO/generic steps. Exact keys, SQL, validation bounds (name 1-32/1-64, tagline 0-140, SEO 0-300, legal 0-20000, password ≥8, hex regex, locale allowlist, theme allowlist), exact curl commands, exact commit file lists.

**3. Type consistency:** `THEMES` allowlist defined once Task 2, consumed Tasks 3-5. `theme_css` href contract identical in Task 2 impl and Task 5 brief. `User.theme` added Task 1, read by layout resolution (Task 4 layout touch states the resolution rule: user.theme non-empty wins, else site_theme setting, else default). `site_config(&conn)` signature used consistently. `set_setting` upsert semantics match branding POST (validate-all-before-write).

---

## Deferred plans (one per subsystem, per skill scope check)

- Slice 1b: OAuth (Google/GitHub/Discord) + password reset mail/admin link.
- Slice 3: Reader 2.0 + notes/comments UI + reading stats.
- Slice 4: Library/discovery (FTS5, shelves, notifications, RSS/OPDS/JSON API tokens).
- Slice 5: Admin catalog (series/chapter editor, uploads, sources hidden incl. `/source` removal, scheduler, users, jobs, backups).
- Slice 6: PWA/offline + i18n (locale column activates) + perf hardening + release.
