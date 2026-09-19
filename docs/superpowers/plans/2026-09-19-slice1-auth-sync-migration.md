# Slice 1: Auth + Server Sync + Auto-Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Readers can register, log in, and have bookmarks plus reading progress stored server-side and synced across devices, with anonymous localStorage data merging automatically on signup.

**Architecture:** DB-backed token sessions (HttpOnly cookie + `sessions` table) with bcrypt password hashes. New `auth`/`sync` route modules in the website crate; new `domain/user.rs` types plus identity tables in mangaread-core. Frontend keeps localStorage as offline cache and write-throughs to the server when logged in.

**Tech Stack:** Rust edition 2024, Axum 0.8, Maud 0.27, rusqlite 0.32 (bundled), bcrypt 0.17 (new), uuid 1 with v4 (new), vanilla JS.

**Spec:** `docs/superpowers/specs/2026-09-19-mangareader-customizable-website-design.md` (§2 goals, §3 identity minus OAuth/stats, §16 acceptance)

## Global Constraints

- Self-host first. Single Docker image. SQLite default (WAL + busy_timeout, existing pattern).
- Stay in Rust for core. Vanilla CSS/JS stays.
- No runtime plugin API. No WASM loaders. No dynamic libs.
- One account = one reader identity. No profiles.
- No manual import button. No export. Only automatic localStorage merge on signup.
- Readers never see sources, adapters, sync state, or origin URLs. This slice touches no reader-visible scraping UI.
- OAuth (Google/GitHub/Discord) and reading stats are NOT in this slice. They get their own plan.
- Deviation from spec: spec §3 names tower-sessions. This plan uses DB-backed token cookies instead (same properties: server-side `sessions` table, HttpOnly cookie, expiry), because the codebase uses raw rusqlite behind a `Mutex<Connection>` and pulling the tower-sessions-sqlx stack would add sqlx plus a second DB layer for zero user-facing gain.

---

## File Structure

New files and their single responsibility:

- `mangaread-core/src/domain/user.rs` — identity domain types only: `Role`, `User`, `NewUser`, `Bookmark`, `Progress`, `HistoryEntry`, `Note`. No hashing, no HTTP.
- `website/src/auth.rs` — password hashing/verify (bcrypt), session token issue/parse, `Set-Cookie` builder, `AuthUser` extractor (`FromRequestParts`), `require_role` gate. Pure fns unit-tested.
- `website/src/routes/auth_pages.rs` — `GET /login`, `POST /login`, `GET /register`, `POST /register`, `POST /logout` + Maud login/register forms.
- `website/src/routes/sync.rs` — `GET/POST /api/bookmarks`, `GET/POST /api/progress`, `POST /api/history`, `POST /api/migrate`. Includes pure `merge_progress` fn (unit-tested).
- `website/static/auth.js` — runs on every page when logged in: one-shot auto-migration POST, then write-through of bookmarks and read-state to server APIs.

Modified files:

- `mangaread-core/src/domain/mod.rs` — add `pub mod user;` + `pub use user::*;`
- `mangaread-core/src/error.rs` — add `Auth(String)` variant.
- `mangaread-core/src/db.rs` — `init_db` gains `users`, `sessions`, `bookmarks`, `progress`, `history`, `notes` tables + indexes; new CRUD fns listed in Task 1.
- `website/Cargo.toml` — add `bcrypt = "0.17"`, `uuid = { version = "1", features = ["v4"] }`.
- `website/src/main.rs` — `mod auth;`, register auth + sync routes, bootstrap log line.
- `website/src/routes/mod.rs` — add `pub mod auth_pages;` + `pub mod sync;`
- `website/src/templates/layout.rs` — `base_layout` gains `user: Option<&crate::auth::AuthUser>` param; nav shows Login link or name + logout button; `<body>` gains `data-user-id` when logged in.
- Every `base_layout` caller (find via grep `base_layout(`) — pass the new arg.
- `website/static/app.js` — `setChapterStatus`/`toggleBookmark` write-through when `data-user-id` present; bookmarks page loads IDs from `/api/bookmarks` when logged in.

---

### Task 1: Core identity tables + repository functions

**Files:**
- Create: `mangaread-core/src/domain/user.rs`
- Modify: `mangaread-core/src/domain/mod.rs`, `mangaread-core/src/error.rs`, `mangaread-core/src/db.rs`
- Test: unit tests inside `mangaread-core/src/db.rs` (`#[cfg(test)]`, in-memory SQLite)

**Interfaces:**
- Consumes: existing `db::init_db(&Connection)`, `Error::NotFound(String)`.
- Produces (used by Tasks 2-4):
  - `pub enum Role { Member, Editor, Admin }` with `fn as_str(&self) -> &'static str`, `fn from_str(s: &str) -> Role` (unknown → Member), `fn at_least(&self, min: &Role) -> bool` (Member < Editor < Admin).
  - `pub struct User { id: i64, name: String, email: String, password_hash: String, role: Role, locale: String, created_at: String }`
  - `pub struct NewUser { name: String, email: String, password_hash: String, role: Role }`
  - `pub struct Progress { user_id: i64, chapter_id: i64, page: i64, status: String, updated_at: String }`
  - `db::create_user(conn, &NewUser) -> Result<i64, Error>` (fails on duplicate email)
  - `db::find_user_by_email(conn, &str) -> Result<User, Error>` (NotFound when missing)
  - `db::count_users(conn) -> Result<i64, Error>`
  - `db::insert_session(conn, user_id: i64, token: &str, expires_at: &str, device: &str) -> Result<(), Error>`
  - `db::find_user_by_session(conn, token: &str, now: &str) -> Result<User, Error>` (joins sessions, checks `expires_at > now`, NotFound otherwise)
  - `db::delete_session(conn, token: &str) -> Result<(), Error>`
  - `db::set_bookmark(conn, user_id: i64, series_id: i64, bookmarked: bool) -> Result<(), Error>`
  - `db::list_bookmarks(conn, user_id: i64) -> Result<Vec<i64>, Error>`
  - `db::upsert_progress(conn, &Progress) -> Result<(), Error>` (upsert on `(user_id, chapter_id)`; only overwrite when incoming `updated_at >=` stored)
  - `db::list_progress_by_series(conn, user_id: i64, series_id: i64) -> Result<Vec<Progress>, Error>` (join chapters → filter `m.series_id`)
  - `db::add_history(conn, user_id: i64, chapter_id: i64, at: &str) -> Result<(), Error>`
  - `db::insert_note(conn, user_id: i64, chapter_id: i64, page: i64, body: &str) -> Result<i64, Error>` + `db::list_notes(conn, user_id: i64, chapter_id: i64)`

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    #[test]
    fn user_roundtrip_and_session_lookup() {
        let conn = mem();
        let id = create_user(&conn, &NewUser {
            name: "r".into(), email: "r@x.io".into(),
            password_hash: "h".into(), role: Role::Member,
        }).unwrap();
        assert!(id > 0);
        insert_session(&conn, id, "tok123", "2099-01-01 00:00:00", "test").unwrap();
        let u = find_user_by_session(&conn, "tok123", "2026-01-01 00:00:00").unwrap();
        assert_eq!(u.email, "r@x.io");
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p mangaread-core user_roundtrip_and_session_lookup`
Expected: FAIL with "cannot find function `create_user`" (or unresolved import).

- [ ] **Step 3: Write minimal implementation**

```rust
// domain/user.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role { Member, Editor, Admin }

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self { Role::Member => "member", Role::Editor => "editor", Role::Admin => "admin" }
    }
    pub fn from_str(s: &str) -> Role {
        match s { "admin" => Role::Admin, "editor" => Role::Editor, _ => Role::Member }
    }
    fn rank(&self) -> u8 {
        match self { Role::Member => 0, Role::Editor => 1, Role::Admin => 2 }
    }
    pub fn at_least(&self, min: &Role) -> bool { self.rank() >= min.rank() }
}
```

Tables in `init_db` (append to the existing batch):

```sql
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'member',
    locale TEXT NOT NULL DEFAULT 'en',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    device TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS bookmarks (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, series_id)
);
CREATE TABLE IF NOT EXISTS progress (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    page INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'started',
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(user_id, chapter_id)
);
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    read_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE TABLE IF NOT EXISTS notes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    page INTEGER NOT NULL DEFAULT 1,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_progress_user_updated ON progress(user_id, updated_at);
CREATE INDEX IF NOT EXISTS idx_history_user ON history(user_id, read_at);
```

`find_user_by_session` SQL:

```sql
SELECT u.id, u.name, u.email, u.password_hash, u.role, u.locale, u.created_at
FROM users u JOIN sessions s ON s.user_id = u.id
WHERE s.token = ?1 AND s.expires_at > ?2
```

`upsert_progress` SQL (server-side last-write-wins guard):

```sql
INSERT INTO progress (user_id, chapter_id, page, status, updated_at)
VALUES (?1, ?2, ?3, ?4, ?5)
ON CONFLICT(user_id, chapter_id) DO UPDATE SET
    page = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.page ELSE progress.page END,
    status = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.status ELSE progress.status END,
    updated_at = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.updated_at ELSE progress.updated_at END
```

`error.rs` addition:

```rust
#[error("Auth error: {0}")]
Auth(String),
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p mangaread-core`
Expected: PASS (all tests, including existing suite).

- [ ] **Step 5: Commit**

```bash
git add mangaread-core/src/domain/user.rs mangaread-core/src/domain/mod.rs mangaread-core/src/error.rs mangaread-core/src/db.rs
git commit -m "feat: identity tables and repository (users, sessions, bookmarks, progress, history, notes)"
```

### Task 2: Auth core — hashing, cookies, extractor, role gate

**Files:**
- Create: `website/src/auth.rs`
- Modify: `website/Cargo.toml`, `website/src/main.rs` (`mod auth;`)
- Test: unit tests inside `website/src/auth.rs`

**Interfaces:**
- Consumes: Task 1 (`db::find_user_by_session`, `Role`, `User`), `AppState.db: Mutex<Connection>`.
- Produces (used by Tasks 3-4):
  - `pub fn hash_password(pw: &str) -> Result<String, String>` (bcrypt DEFAULT_COST; Err on cost/hash failure)
  - `pub fn verify_password(pw: &str, hash: &str) -> bool` (false on any error — no user enumeration signal)
  - `pub const SESSION_COOKIE: &str = "mangaread_session";`
  - `pub fn new_token() -> String` (uuid v4)
  - `pub fn set_cookie_value(token: &str) -> String` → `mangaread_session=<token>; Path=/; HttpOnly; SameSite=Lax; Max-Age=2592000` plus `; Secure` when env `MANGAREAD_COOKIE_SECURE=1`
  - `pub fn token_from_headers(headers: &HeaderMap) -> Option<String>` (parse `Cookie` header, exact-name match)
  - `pub struct AuthUser { pub id: i64, pub name: String, pub role: Role }` implementing `FromRequestParts<AppState>`: missing/invalid token → rejection that redirects to `/login` (return `axum::response::Redirect`; note `Option<AuthUser>` keeps working for optional auth because Axum implements `FromRequestParts` for `Option<T>`)
  - `pub fn require_role(user: &AuthUser, min: Role) -> Result<(), StatusCode>` (FORBIDDEN otherwise)

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn cookie_parse_exact_name() {
        let mut headers = HeaderMap::new();
        headers.insert(COOKIE, HeaderValue::from_str("a=1; mangaread_session=tok-9; b=2").unwrap());
        assert_eq!(token_from_headers(&headers).as_deref(), Some("tok-9"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p website password_roundtrip`
Expected: FAIL with "cannot find function `hash_password`" (module does not exist yet).

- [ ] **Step 3: Write minimal implementation**

```rust
// website/src/auth.rs
use axum::{
    extract::FromRequestParts,
    http::{header::COOKIE, request::Parts, HeaderMap, StatusCode},
    response::Redirect,
};
use mangaread_core::{db, domain::Role};
use std::sync::Arc;

pub const SESSION_COOKIE: &str = "mangaread_session";
const SESSION_MAX_AGE: i64 = 30 * 24 * 3600; // 30 days

pub fn hash_password(pw: &str) -> Result<String, String> {
    bcrypt::hash(pw, bcrypt::DEFAULT_COST).map_err(|e| e.to_string())
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    bcrypt::verify(pw, hash).unwrap_or(false)
}

pub fn new_token() -> String {
    uuid::Uuid::new_v4().to_string()
}
```

Cookie builder reads `MANGAREAD_COOKIE_SECURE` at call time (so tests can set/unset it). Extractor locks `state.db`, calls `find_user_by_session` with current UTC time formatted `%Y-%m-%d %H:%M:%S`, maps `Error::NotFound` → Redirect, other DB errors → 500 via `(StatusCode::INTERNAL_SERVER_ERROR, ...)` rejection tuple — implement rejection as `axum::response::Response` to support both cases.

`Cargo.toml` additions:

```toml
bcrypt = "0.17"
uuid = { version = "1", features = ["v4"] }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p website`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add website/src/auth.rs website/Cargo.toml website/src/main.rs
git commit -m "feat: auth core (bcrypt, session cookies, AuthUser extractor, role gate)"
```

### Task 3: Auth pages, routes, nav state, admin bootstrap

**Files:**
- Create: `website/src/routes/auth_pages.rs`
- Modify: `website/src/routes/mod.rs`, `website/src/main.rs` (routes), `website/src/templates/layout.rs` (new `user` param + nav), every `base_layout(` caller (grep first), `website/static/style.css` (auth form styles — append only)
- Test: `cargo build` + curl flow (steps below)

**Interfaces:**
- Consumes: Task 2 (`hash_password`, `verify_password`, `new_token`, `set_cookie_value`, `AuthUser`, `token_from_headers`), Task 1 (`create_user`, `find_user_by_email`, `count_users`, `insert_session`, `delete_session`).
- Produces: routes `GET /login`, `POST /login`, `GET /register`, `POST /register`, `POST /logout`; layout nav reflects session; first-ever registered user gets `Role::Admin` (bootstrap; log it with tracing::warn so the owner sees it).

Handler contracts:

- `POST /register` form fields: `name`, `email`, `password` (min 8 chars, else 400 re-render with error). If `count_users == 0` → role Admin, else Member. On success: insert session (expiry now + 30d), respond `303 SeeOther` to `/` with `Set-Cookie`. Duplicate email → 400 re-render "could not create account" (same message as any failure — no enumeration).
- `POST /login` form fields: `email`, `password`. Unknown email OR bad password → identical 401 page "Invalid email or password". Success → new session + `303` to `/`.
- `POST /logout`: read token from headers, `delete_session`, `303` to `/` with expired cookie (`Max-Age=0`).
- Validation: name 1-32 chars, email contains `@` and `.`, password ≥ 8. Keep validation inline in the handler (no validator crate — YAGNI).
- Layout: `pub fn base_layout(title, content, sync_banner, config_css, site_title, user: Option<&AuthUser>)`. Nav: `None` → "Login" link; `Some(u)` → `u.name` (link to `/settings`, which 404s until slice 2 — so link to `/bookmarks` instead for now) + inline `<form method="post" action="/logout"><button>Logout</button></form>`. Body tag gains `data-user-id` attr when `Some`.
- Every handler that calls `base_layout` takes `Option<AuthUser>` extractor and passes `user.as_ref()`.

- [ ] **Step 1: Write the failing check**

Run: `cargo build -p website 2>&1 | head -5`
Then: `grep -rn "base_layout(" website/src --include='*.rs' | wc -l` (lists callers to update).

- [ ] **Step 2: Implement** (routes, forms, layout change, all callers, CSS append)

- [ ] **Step 3: Verify build**

Run: `cargo build -p website`
Expected: exit 0, no warnings about unused extractor.

- [ ] **Step 4: Verify auth flow with curl**

```bash
MANGAREAD_DB=/tmp/slice1.db cargo run -p website &
sleep 3
curl -s -c /tmp/cj -X POST --data-urlencode 'name=owner' --data-urlencode 'email=o@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null -w '%{http_code}\n'
curl -s -b /tmp/cj http://localhost:3000/ | grep -o 'owner' | head -1
curl -s -b /tmp/cj -X POST http://localhost:3000/logout -o /dev/null -w '%{http_code}\n'
curl -s -X POST --data-urlencode 'email=nope@x.io' --data-urlencode 'password=password123' http://localhost:3000/login -w '%{http_code}\n'
```

Expected: `303`, `owner`, `303`, `401`. Kill the server, delete `/tmp/slice1.db*`, `/tmp/cj`.

- [ ] **Step 5: Commit**

```bash
git add website/src/routes/auth_pages.rs website/src/routes/mod.rs website/src/main.rs website/src/templates/layout.rs website/static/style.css
git commit -m "feat: login/register/logout pages, nav session state, first-user admin bootstrap"
```

### Task 4: Server sync APIs + merge endpoint

**Files:**
- Create: `website/src/routes/sync.rs`
- Modify: `website/src/routes/mod.rs`, `website/src/main.rs` (routes)
- Test: unit tests for `merge_progress` + curl flow with two devices (steps below)

**Interfaces:**
- Consumes: Task 1 (`set_bookmark`, `list_bookmarks`, `upsert_progress`, `list_progress_by_series`, `add_history`), Task 2 (`AuthUser` — all endpoints require `AuthUser`, 401→redirect for browsers, JSON 401 for `Accept: application/json`? Keep simple: missing auth → `401` JSON `{"ok":false,"error":"login required"}`).
- Produces:
  - `GET /api/bookmarks` → `{"series":[1,2,3]}`
  - `POST /api/bookmarks` body `{"series_id":1,"bookmarked":true}` → `{"ok":true}`
  - `GET /api/progress?series_id=1` → `{"chapters":{"12":{"page":5,"status":"started","updated_at":"..."}}}` (chapter id as string key)
  - `POST /api/progress` body `{"chapter_id":12,"page":5,"status":"started"}` → server stamps `updated_at` = now UTC, `{"ok":true}`
  - `POST /api/history` body `{"chapter_id":12}` → `{"ok":true}`
  - `POST /api/migrate` body `{"bookmarks":[1,2],"progress":[{"chapter_id":12,"page":5,"status":"started","updated_at":"..."}]}` → unions bookmarks (insert-or-ignore), upserts progress via same last-write-wins rule, returns `{"ok":true,"bookmarks":N,"progress":M}`
  - Pure fn `merge_progress(existing: Option<&str>, incoming: &str) -> bool` (true = incoming wins; string `>=` compare on ISO8601 `updated_at`) — unit-tested with three cases (newer wins, older loses, missing existing wins).

- [ ] **Step 1: Write the failing test**

```rust
#[cfg(test)]
mod tests {
    use super::merge_progress;

    #[test]
    fn newer_incoming_wins() {
        assert!(merge_progress(Some("2026-01-01 00:00:00"), "2026-09-19 00:00:00"));
        assert!(!merge_progress(Some("2026-09-19 00:00:00"), "2026-01-01 00:00:00"));
        assert!(merge_progress(None, "2026-01-01 00:00:00"));
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p website merge_progress`
Expected: FAIL with "cannot find function `merge_progress`".

- [ ] **Step 3: Write minimal implementation** (handlers + routes + merge fn; `updated_at` stamped with `chrono`-free formatting — use `std::time` → format via `time`? No new dep: store `datetime('now')` by letting SQLite default fill it — `upsert_progress` takes `updated_at: String`; for live writes pass the sentinel `"now"`? Sentinel is a hack. Cleaner: format UTC with `std::time::SystemTime` + manual epoch→datetime conversion is error-prone. Decision: add `time = { version = "0.3", features = ["formatting", "std"] }` to website/Cargo.toml and format `%Y-%m-%d %H:%M:%S` UTC. One tiny dep, exact API: `time::OffsetDateTime::now_utc().format(&time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))`.)

Routes in main.rs:

```rust
.route("/api/bookmarks", get(crate::routes::sync::get_bookmarks).post(crate::routes::sync::set_bookmark))
.route("/api/progress", get(crate::routes::sync::get_progress).post(crate::routes::sync::post_progress))
.route("/api/history", post(crate::routes::sync::post_history))
.route("/api/migrate", post(crate::routes::sync::post_migrate))
```

- [ ] **Step 4: Run tests + curl two-device flow**

Run: `cargo test -p website`
Expected: PASS.

```bash
MANGAREAD_DB=/tmp/slice1b.db cargo run -p website &
sleep 3
curl -s -c /tmp/cj -X POST --data-urlencode 'name=a' --data-urlencode 'email=a@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null
curl -s -b /tmp/cj -X POST -H 'Content-Type: application/json' -d '{"series_id":1,"bookmarked":true}' http://localhost:3000/api/bookmarks
curl -s -c /tmp/cj2 -X POST --data-urlencode 'name=b' --data-urlencode 'email=b@x.io' --data-urlencode 'password=password123' http://localhost:3000/register -o /dev/null
curl -s -b /tmp/cj2 http://localhost:3000/api/bookmarks
curl -s http://localhost:3000/api/bookmarks -w '%{http_code}\n'
```

Expected: `{"ok":true}`, `{"series":[]}` (user B sees nothing — isolation), `401` (logged-out rejected). Kill server, delete `/tmp/slice1b.db*`, `/tmp/cj*`.

- [ ] **Step 5: Commit**

```bash
git add website/src/routes/sync.rs website/src/routes/mod.rs website/src/main.rs website/Cargo.toml
git commit -m "feat: server sync APIs (bookmarks, progress, history, migrate)"
```

### Task 5: Frontend session adapter + one-shot auto-migration

**Files:**
- Create: `website/static/auth.js`
- Modify: `website/src/templates/layout.rs` (include `<script src="/static/auth.js">` after app.js), `website/static/app.js` (write-through only — no import/export UI)
- Test: manual browser checklist (steps below)

**Interfaces:**
- Consumes: Task 4 endpoints; `document.body.dataset.userId` (set by layout when logged in).
- Produces:
  - On load, if `data-user-id` present AND `localStorage.mangaread_migrated !== userId`: collect `mangaread_bookmarks` (array) + every `mangaread_read_*` key (parse `ch_<id>` entries), POST to `/api/migrate`, on success set `mangaread_migrated = userId`. On failure: retry next page load (do not set flag). Never show an import button or dialog.
  - `window.MangaSync = { loggedIn(): bool, pushBookmark(seriesId, bookmarked), pushProgress(chapterId, page, status) }` — fire-and-forget `fetch` POSTs, failures swallowed (localStorage stays source of truth offline).
  - `app.js` changes: `toggleBookmark` calls `MangaSync.pushBookmark` after local write; `setChapterStatus` calls `MangaSync.pushProgress` after local write; bookmarks page: if logged in, seed grid IDs from `GET /api/bookmarks` instead of localStorage (fallback to localStorage on fetch failure).

- [ ] **Step 1: Write the failing check** (no automated test — static file)

Run: `ls website/static/auth.js`
Expected: "No such file or directory".

- [ ] **Step 2: Implement** `auth.js` (~80 lines) + `app.js` hooks (3 call sites) + layout script tag.

Migration payload builder (shape contract with Task 4):

```js
function collectLocal() {
  const out = { bookmarks: [], progress: [] };
  try { out.bookmarks = JSON.parse(localStorage.getItem('mangaread_bookmarks') || '[]'); } catch (_) {}
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (!key || key.indexOf('mangaread_read_') !== 0) continue;
    try {
      const state = JSON.parse(localStorage.getItem(key));
      for (const chKey in state) {
        if (chKey.indexOf('ch_') !== 0) continue;
        const e = state[chKey];
        out.progress.push({
          chapter_id: parseInt(chKey.slice(3), 10),
          page: e.page || 1,
          status: e.status || 'started',
          updated_at: e.updated || new Date().toISOString()
        });
      }
    } catch (_) {}
  }
  return out;
}
```

Note: `updated` from existing read-state is ISO8601 with `T` (e.g. `2026-09-19T12:00:00.000Z`); server `updated_at` is `YYYY-MM-DD HH:MM:SS`. String comparison across formats is only valid after normalization — Task 4's `upsert_progress` compares with `>=`, so `auth.js` MUST normalize: `new Date(e.updated).toISOString().slice(0, 19).replace('T', ' ')` with try/catch fallback to epoch `1970-01-01 00:00:00`. (This is the one cross-task type contract that must match exactly.)

- [ ] **Step 3: Verify in browser**

Checklist: (1) anonymous: bookmark a series, read pages — works, no network calls to `/api/bookmarks`; (2) register — after redirect, one POST to `/api/migrate`, `mangaread_migrated` set; (3) second browser/profile login as same user — bookmarks page shows migrated series; (4) no import/export UI exists anywhere (grep `import|export` in templates returns nothing user-facing).

- [ ] **Step 4: Commit**

```bash
git add website/static/auth.js website/static/app.js website/src/templates/layout.rs
git commit -m "feat: session-aware sync adapter with one-shot auto-migration"
```

### Task 6: Slice acceptance + full verify

**Files:** none (verification only).

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`
Expected: PASS, zero failures.

- [ ] **Step 2: Release-mode build + Docker smoke**

Run: `cargo build --workspace` then `docker compose up --build -d && sleep 8 && curl -s http://localhost:3000/health && docker compose down`
Expected: build clean, `/health` returns ok.

- [ ] **Step 3: Spec acceptance replay** (§16 as applicable to slice 1)

  - Register → login → logout → login works; wrong password gives identical message to unknown email.
  - Anonymous reader still works with zero account (bookmarks + progress in localStorage only).
  - On signup the browser data merges automatically with no manual step (Task 5 checklist).
  - Reader-facing pages contain no source names/URLs/sync badges beyond what already existed (this slice adds none; `grep -ri "demonicscans\|manhuaplus" website/src/templates/` shows only pre-existing hits — record count, must not increase).
  - Per-user isolation: user B never sees user A's bookmarks/progress (Task 4 curl).

- [ ] **Step 4: Commit** (only if verification forced a fix; otherwise no commit — record "no changes" in the task result)

---

## Self-Review

**1. Spec coverage (§3 minus deferred):** password auth + sessions + remember (30d cookie) → Tasks 2-3. Roles `anon/member/editor/admin` + per-route middleware (`AuthUser`, `require_role`) → Tasks 2-3; editor/admin routes arrive with later slices that consume `require_role`. Server tables users/sessions/bookmarks/progress/history/notes → Task 1. Auto-merge on signup, no manual import, no export → Tasks 4-5. Deferred with reason: OAuth (own plan — needs new crates + provider setup docs), reading stats (needs reader instrumentation from slice 3). No profiles anywhere — matches corrected spec.

**2. Placeholder scan:** no TBD/TODO/import/export UI/generic "error handling" steps. Every code step names exact files, SQL, and commands. Validation rules are explicit numbers (name 1-32, password ≥ 8). Cookie format exact. Timestamp format contract (`%Y-%m-%d %H:%M:%S` UTC, `auth.js` normalization) stated in both Task 4 and Task 5.

**3. Type consistency:** `Role::{as_str, from_str, at_least}` defined once in Task 1, used in Tasks 2-3. `AuthUser { id: i64, name: String, role: Role }` defined Task 2, consumed Tasks 3-5 (`data-user-id` = `id`). `Progress { user_id, chapter_id, page, status: String, updated_at: String }` Task 1 → Task 4 handlers. `merge_progress(Option<&str>, &str) -> bool` signature identical in test and impl steps. `upsert_progress` last-write-wins rule matches `merge_progress` semantics. `base_layout` gains exactly one param (`user: Option<&AuthUser>`), all callers updated in Task 3 via grep.

---

## Deferred plans (one per subsystem, per skill scope check)

- Slice 1b: OAuth (Google/GitHub/Discord) + password reset mail/admin link.
- Slice 2: Settings pages + owner customization (branding/homepage/SEO/legal without restart).
- Slice 3: Reader 2.0 + notes/comments UI + reading stats.
- Slice 4: Library/discovery (FTS5, shelves, notifications, RSS/OPDS/JSON API tokens).
- Slice 5: Admin catalog (series/chapter editor, uploads, sources hidden, scheduler, users, jobs, backups).
- Slice 6: PWA/offline + i18n + perf hardening + release.
