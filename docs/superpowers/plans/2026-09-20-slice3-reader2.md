# Slice 3 — Reader 2.0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the reader into a production-tier, fully customizable reading surface: orthogonal layout/direction modes, a fit system, a controls sheet, remappable input, comfort chrome, private per-page notes, and per-user preferences that persist across devices.

**Architecture:** Preferences are a validated, server-persisted JSON blob per user (`users.reader_prefs`), rendered into the page as body `data-r-*` attributes so the first paint is already correct (no flash of default layout). A new client module `website/static/reader.js` owns all reader behavior (replacing the dead `reader.js` and the reader branches currently inside `app.js`). Presentation lives in a new `website/static/reader.css` loaded only by the reader layout. Two small JSON APIs persist reader prefs and notes. Server-rendered markup carries the contract from `docs/superpowers/specs/2026-09-20-reader2-design-contract.md`.

**Tech Stack:** Rust (edition 2024), axum 0.8, maud 0.27, rusqlite 0.32 (SQLite/WAL), serde/serde_json 1, vanilla CSS + vanilla JS (no framework, no build step).

**Spec:** `docs/superpowers/specs/2026-09-19-mangareader-customizable-website-design.md` §4 (Reader 2.0), §6 (reader defaults), §13 (data model deltas), §14 (routes deltas), §16 (acceptance).

**Design contract (binding for all UI/CSS/JS names):** `docs/superpowers/specs/2026-09-20-reader2-design-contract.md`

## Global Constraints

- Rust edition 2024. `mangaread-core` has no HTTP and no hashing: pure data + SQL only.
- **Never hold the `state.db` mutex guard across `.await`, bcrypt, or network I/O.** Acquire, run SQL, drop.
- Reader-facing surfaces must never expose scraping concepts: no source names, adapter names, origin URLs, sync badges, or resync buttons (spec §3, §8).
- All reader CSS custom properties are consumed with fallbacks: `var(--r-x, <fallback>)`. Theme files (`themes/{webtoon,asura,demonic}.css`) contain zero reader selectors and must not be required for correctness.
- All interactive controls are ≥44×44px at ≤768px. Chrome text contrast ≥4.5:1. Every control has an accessible name.
- Progress semantics are unchanged: `progress.page` is a 1-indexed page index; last-write-wins server-side. `data-page-idx` stays 1-indexed.
- localStorage keys stay backward compatible: `mangaread_read_<seriesId>`, `mangaread_bookmarks`, `mangaread_migrated`, `mangaread_sources` are untouched by this slice.
- No new npm/node tooling, no build step. Scripts are plain `<script src>` files.
- TDD: each task writes its failing test first, then the minimal implementation. Commit per task.
- Do not add anonymous note writes: `notes.user_id` is `NOT NULL`.
- Out of scope (do not build): image proxy, WebP/AVIF transcode cache, offline download, blur-up dominant-color extraction (placeholder marks only where the server already has data), OPDS/RSS, comments, social.
- New dependency: none. `serde`, `serde_json`, `uuid`, `time` are already present.

---

### Task 1: `ReaderPrefs` domain type + persisted prefs column

**Files:**
- Create: `mangaread-core/src/domain/reader.rs`
- Modify: `mangaread-core/src/domain/mod.rs` (add `pub mod reader;` + `pub use reader::*;`)
- Modify: `mangaread-core/src/db.rs` (add `users.reader_prefs` migration + `get_reader_prefs` / `set_reader_prefs`)
- Test: `mangaread-core/src/domain/reader.rs` (unit tests), `mangaread-core/src/db.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `db::init_db` migration pattern (`PRAGMA table_info(users)` guard, `mangaread-core/src/db.rs:148-166`); existing `mem()` test helper (`db.rs:1010`).
- Produces:
  - `pub struct ReaderPrefs` with `pub` fields, `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, `#[serde(default)]`.
  - `impl Default for ReaderPrefs`.
  - `impl ReaderPrefs { pub fn normalized(self) -> Self; pub fn to_json(&self) -> String; pub fn from_json(s: &str) -> Self; }`
  - `pub const LAYOUTS: &[&str]`, `pub const DIRECTIONS: &[&str]`, `pub const FITS: &[&str]`, `pub const BGS: &[&str]`.
  - `pub const DEFAULT_KEYBINDS: &[(&str, &[&str])]`; `pub const ACTION_IDS: &[&str]`.
  - `pub fn get_reader_prefs(conn: &Connection, user_id: i64) -> Result<ReaderPrefs, Error>`
  - `pub fn set_reader_prefs(conn: &Connection, user_id: i64, prefs: &ReaderPrefs) -> Result<(), Error>`

- [ ] **Step 1: Write the failing domain tests**

Append to `mangaread-core/src/domain/reader.rs` (create the file with just the test module and the types stubbed so it compiles — or write tests first and let the build fail, per repo convention either is acceptable; the test bodies below are the requirement):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_current_behavior() {
        let p = ReaderPrefs::default();
        assert_eq!(p.layout, "strip");      // preserve today's long-strip default
        assert_eq!(p.direction, "ltr");
        assert_eq!(p.fit, "");              // "" = width mode
        assert_eq!(p.max_w, 700);           // matches .reader-images max-width:700px
        assert_eq!(p.gap, 0);
        assert_eq!(p.bg, "black");
        assert_eq!(p.hide_delay, 3);
        assert_eq!(p.dim_delay, 0);
        assert_eq!(p.dim_opacity, 0.5);
        assert_eq!(p.swipe_threshold, 70);
        assert_eq!(p.click_zones, vec![30, 70]);
        assert!(!p.auto_next);
        assert!(p.show_notes);
    }

    #[test]
    fn normalization_clamps_and_rejects_unknown() {
        let mut p = ReaderPrefs::default();
        p.layout = "hologram".into();       // unknown -> default
        p.direction = "diagonal".into();    // unknown -> default
        p.max_w = 99_999;                   // clamp to 1400
        p.gap = 9_999;                      // clamp to 64
        p.hide_delay = 99;                  // clamp to 10
        p.dim_opacity = 5.0;                // clamp to 0.8
        p.swipe_threshold = 1;              // clamp to 40
        p.click_zones = vec![5, 99];        // clamp to 10..90, >=10 apart
        let n = p.normalized();
        assert_eq!(n.layout, "strip");
        assert_eq!(n.direction, "ltr");
        assert_eq!(n.max_w, 1400);
        assert_eq!(n.gap, 64);
        assert_eq!(n.hide_delay, 10);
        assert_eq!(n.dim_opacity, 0.8);
        assert_eq!(n.swipe_threshold, 40);
        assert_eq!(n.click_zones, vec![10, 20]); // second forced to first+10
    }

    #[test]
    fn keybinds_are_sanitized() {
        use std::collections::HashMap;
        let mut p = ReaderPrefs::default();
        let mut kb = HashMap::new();
        kb.insert("page.next".to_string(), vec!["KeyX".to_string()]);
        kb.insert("bogus.action".to_string(), vec!["KeyY".to_string()]);
        kb.insert("page.prev".to_string(), vec![]); // empty -> dropped
        p.keybinds = kb;
        let n = p.normalized();
        assert!(n.keybinds.contains_key("page.next"));
        assert!(!n.keybinds.contains_key("bogus.action"));
        assert!(!n.keybinds.contains_key("page.prev"));
        // every action present, defaults restored for the missing ones
        assert_eq!(n.keybinds.len(), ACTION_IDS.len());
    }

    #[test]
    fn json_roundtrip_preserves_values() {
        let mut p = ReaderPrefs::default();
        p.layout = "double".into();
        p.max_w = 1100;
        let s = p.to_json();
        assert_eq!(ReaderPrefs::from_json(&s), p);
    }

    #[test]
    fn from_json_garbage_yields_defaults() {
        assert_eq!(ReaderPrefs::from_json("not json"), ReaderPrefs::default());
        assert_eq!(ReaderPrefs::from_json("{}"), ReaderPrefs::default());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p mangaread-core reader:: -q`
Expected: FAIL to compile — `ReaderPrefs` not found.

- [ ] **Step 3: Implement `mangaread-core/src/domain/reader.rs`**

```rust
//! Reader 2.0 preference types.
//!
//! Pure data + validation. No SQL, no HTTP. Persisted as one JSON blob in
//! `users.reader_prefs`; `normalized()` is the single source of truth for
//! which values are legal, so a hand-crafted API body can never poison the
//! stored blob.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const LAYOUTS: &[&str] = &["single", "double", "strip", "vertical"];
pub const DIRECTIONS: &[&str] = &["ltr", "rtl"];
pub const FITS: &[&str] = &["", "height", "screen"];
pub const BGS: &[&str] = &["black", "white", "sepia", "custom"];

/// Stable action ids for the remap UI (design contract §4.3).
pub const ACTION_IDS: &[&str] = &[
    "page.next",
    "page.prev",
    "chapter.next",
    "chapter.prev",
    "ui.toggle",
    "ui.focus",
    "ui.fitCycle",
    "layout.cycle",
    "note.add",
];

/// Default bindings. Keys are `KeyboardEvent.code` values; modifiers are a
/// `+`-joined prefix in a stable order.
pub const DEFAULT_KEYBINDS: &[(&str, &[&str])] = &[
    ("page.next", &["ArrowRight", "ArrowDown", "Space", "PageDown"]),
    ("page.prev", &["ArrowLeft", "ArrowUp", "PageUp"]),
    ("chapter.next", &["Shift+ArrowRight", "KeyN"]),
    ("chapter.prev", &["Shift+ArrowLeft", "KeyP"]),
    ("ui.toggle", &["KeyS", "Escape"]),
    ("ui.focus", &["KeyF"]),
    ("ui.fitCycle", &["KeyW"]),
    ("layout.cycle", &["KeyL"]),
    ("note.add", &["KeyM"]),
];

/// Reader preferences. Every field carries a serde default so an older or
/// partial blob always deserializes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReaderPrefs {
    pub layout: String,
    pub direction: String,
    /// `""` = width mode, `height`, or `screen`.
    pub fit: String,
    pub max_w: i64,
    pub gap: i64,
    pub bg: String,
    pub bg_custom: String,
    pub frame: bool,
    pub double_cover: bool,
    pub auto_next: bool,
    pub auto_prev: bool,
    pub hide_delay: i64,
    pub dim_delay: i64,
    pub dim_opacity: f64,
    pub swipe_threshold: i64,
    /// Two boundary percentages from the left, e.g. `[30, 70]`.
    pub click_zones: Vec<i64>,
    pub keybinds: HashMap<String, Vec<String>>,
    pub show_notes: bool,
}

impl Default for ReaderPrefs {
    fn default() -> Self {
        Self {
            layout: "strip".into(),
            direction: "ltr".into(),
            fit: String::new(),
            max_w: 700,
            gap: 0,
            bg: "black".into(),
            bg_custom: "#0a0a0a".into(),
            frame: false,
            double_cover: false,
            auto_next: false,
            auto_prev: false,
            hide_delay: 3,
            dim_delay: 0,
            dim_opacity: 0.5,
            swipe_threshold: 70,
            click_zones: vec![30, 70],
            keybinds: default_keybinds(),
            show_notes: true,
        }
    }
}

fn default_keybinds() -> HashMap<String, Vec<String>> {
    DEFAULT_KEYBINDS
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.iter().map(|s| (*s).to_string()).collect()))
        .collect()
}

fn clamp(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

fn allow(value: &str, allowed: &[&str], fallback: &str) -> String {
    if allowed.contains(&value) {
        value.to_string()
    } else {
        fallback.to_string()
    }
}

impl ReaderPrefs {
    /// Clamp and allowlist every field. Idempotent. This is the only place
    /// legal values are defined, so it is also the API's input validator.
    pub fn normalized(mut self) -> Self {
        self.layout = allow(&self.layout, LAYOUTS, "strip");
        self.direction = allow(&self.direction, DIRECTIONS, "ltr");
        if !FITS.contains(&self.fit.as_str()) {
            self.fit = String::new();
        }
        self.max_w = clamp(self.max_w, 400, 1400);
        self.gap = clamp(self.gap, 0, 64);
        self.bg = allow(&self.bg, BGS, "black");
        if self.bg_custom.len() != 7
            || !self.bg_custom.starts_with('#')
            || !self.bg_custom[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            self.bg_custom = "#0a0a0a".into();
        } else {
            self.bg_custom = self.bg_custom.to_lowercase();
        }
        self.hide_delay = clamp(self.hide_delay, 0, 10);
        // dimDelay is a small enumerated choice, not a continuum.
        if ![0, 15, 30, 60].contains(&self.dim_delay) {
            self.dim_delay = 0;
        }
        self.dim_opacity = if self.dim_opacity.is_finite() {
            self.dim_opacity.max(0.2).min(0.8)
        } else {
            0.5
        };
        self.swipe_threshold = clamp(self.swipe_threshold, 40, 160);

        // Zones: always exactly two boundaries, 10..90, at least 10 apart.
        let a = clamp(*self.click_zones.first().unwrap_or(&30), 10, 80);
        let b = clamp(*self.click_zones.get(1).unwrap_or(&70), a + 10, 90);
        self.click_zones = vec![a, b];

        // Keybinds: every known action present, unknown actions dropped,
        // empty bindings dropped, duplicates removed, values length-capped.
        let mut kb: HashMap<String, Vec<String>> = HashMap::new();
        for action in ACTION_IDS {
            let mut keys: Vec<String> = self
                .keybinds
                .get(*action)
                .map(|v| {
                    let mut seen = std::collections::HashSet::new();
                    v.iter()
                        .filter(|k| !k.trim().is_empty() && k.len() <= 32)
                        .filter(|k| seen.insert((*k).clone()))
                        .take(4)
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if keys.is_empty() {
                keys = DEFAULT_KEYBINDS
                    .iter()
                    .find(|(k, _)| k == action)
                    .map(|(_, v)| v.iter().map(|s| (*s).to_string()).collect())
                    .unwrap_or_default();
            }
            kb.insert((*action).to_string(), keys);
        }
        self.keybinds = kb;
        self
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    /// Never fails: an unreadable blob degrades to defaults rather than
    /// breaking the reader page.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str::<ReaderPrefs>(s)
            .map(|p| p.normalized())
            .unwrap_or_default()
    }
}
```

Add to `mangaread-core/src/domain/mod.rs` after `pub mod page;`:

```rust
pub mod reader;
```

and after `pub use page::*;`:

```rust
pub use reader::*;
```

- [ ] **Step 4: Add the column migration + repo functions to `mangaread-core/src/db.rs`**

After the existing `users.theme` migration block (ends `db.rs:166`), append inside `init_db`:

```rust
    // users.reader_prefs migration: one JSON blob per user (design contract §2.3).
    let has_reader_prefs: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(users)")?;
        let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for col in cols {
            if col? == "reader_prefs" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_reader_prefs {
        conn.execute_batch("ALTER TABLE users ADD COLUMN reader_prefs TEXT NOT NULL DEFAULT '{}'")?;
    }
```

Add near the settings KV section (before `#[cfg(test)]`):

```rust
// ── Reader preferences (one JSON blob per user) ──

/// Load a user's reader preferences. Missing user or unreadable blob yields
/// defaults — a prefs read must never fail a page render.
pub fn get_reader_prefs(conn: &Connection, user_id: i64) -> Result<ReaderPrefs, Error> {
    let mut stmt = conn.prepare("SELECT reader_prefs FROM users WHERE id = ?1")?;
    match stmt.query_row(rusqlite::params![user_id], |row| row.get::<_, String>(0)) {
        Ok(raw) => Ok(ReaderPrefs::from_json(&raw)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ReaderPrefs::default()),
        Err(e) => Err(e.into()),
    }
}

/// Persist reader preferences, normalized first so a hand-crafted request can
/// never store an illegal value.
pub fn set_reader_prefs(conn: &Connection, user_id: i64, prefs: &ReaderPrefs) -> Result<(), Error> {
    let json = prefs.clone().normalized().to_json();
    conn.execute(
        "UPDATE users SET reader_prefs = ?1 WHERE id = ?2",
        rusqlite::params![json, user_id],
    )?;
    Ok(())
}
```

- [ ] **Step 5: Append the db-level round-trip test**

Inside `mangaread-core/src/db.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn reader_prefs_roundtrip_and_defaults() {
        let conn = mem();
        let uid = create_user(
            &conn,
            &NewUser {
                name: "r".into(),
                email: "r@x.io".into(),
                password_hash: "h".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        // Fresh user -> defaults.
        assert_eq!(get_reader_prefs(&conn, uid).unwrap(), ReaderPrefs::default());
        // Illegal values are normalized on write.
        let mut p = ReaderPrefs::default();
        p.layout = "nope".into();
        p.max_w = 99999;
        set_reader_prefs(&conn, uid, &p).unwrap();
        let got = get_reader_prefs(&conn, uid).unwrap();
        assert_eq!(got.layout, "strip");
        assert_eq!(got.max_w, 1400);
        // A corrupted blob degrades to defaults, never errors.
        conn.execute("UPDATE users SET reader_prefs = '{{{' WHERE id = ?1", rusqlite::params![uid])
            .unwrap();
        assert_eq!(get_reader_prefs(&conn, uid).unwrap(), ReaderPrefs::default());
        // Unknown user -> defaults, not an error.
        assert_eq!(get_reader_prefs(&conn, 12_345).unwrap(), ReaderPrefs::default());
    }
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p mangaread-core -q`
Expected: PASS (existing tests + the 5 reader tests + the db round-trip).

- [ ] **Step 7: Verify the migration is idempotent on an existing DB**

Run:
```bash
cargo test -p mangaread-core -q   # mem() calls init_db twice per process already
```
Then confirm the guard exists:
```bash
grep -n "has_reader_prefs" mangaread-core/src/db.rs
```
Expected: two hits (guard + ALTER).

- [ ] **Step 8: Commit**

```bash
git add mangaread-core/src/domain/reader.rs mangaread-core/src/domain/mod.rs mangaread-core/src/db.rs
git commit -m "feat(reader): ReaderPrefs type, validation, and users.reader_prefs storage"
```

**Review focus:** normalization is total (no panics on any input, including empty `click_zones`, NaN opacity, unknown keybind actions); `from_json` never errors; migration is guarded and additive.

---

### Task 2: Reader preferences API

**Files:**
- Create: `website/src/routes/reader_prefs.rs`
- Modify: `website/src/routes/mod.rs` (add `pub mod reader_prefs;`)
- Modify: `website/src/main.rs` (register the route)
- Test: `website/src/routes/reader_prefs.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `mangaread_core::db::get_reader_prefs` / `set_reader_prefs` (Task 1); `mangaread_core::ReaderPrefs`; `crate::auth::AuthUser` (`website/src/auth.rs:78`); `axum::extract::{State, Json}`; `AppState { db: tokio::sync::Mutex<rusqlite::Connection>, .. }`.
- Produces:
  - `pub async fn get_prefs(State<Arc<AppState>>, AuthUser) -> Response` → `200 {"prefs": {...}}`; `401` for anonymous.
  - `pub async fn post_prefs(State<Arc<AppState>>, AuthUser, Json<ReaderPrefs>) -> Response` → `200 {"ok":true,"prefs":{...}}` with the normalized blob echoed back.
  - Routes: `GET /api/reader-prefs`, `POST /api/reader-prefs`.

- [ ] **Step 1: Write the failing test**

Create `website/src/routes/reader_prefs.rs` with only the test module (the file will not compile until handlers exist — that is the failing state). Test bodies:

```rust
#[cfg(test)]
mod tests {
    use mangaread_core::ReaderPrefs;

    /// The API response shape is `{"prefs": {...}}` and the inner object
    /// round-trips through the same serde contract the DB stores.
    #[test]
    fn prefs_blob_serializes_under_prefs_key() {
        use serde_json::json;
        let p = ReaderPrefs::default();
        let body = json!({ "prefs": p });
        assert_eq!(body["prefs"]["layout"], "strip");
        assert_eq!(body["prefs"]["maxW"], 700); // camelCase, per the design contract
        assert!(body["prefs"]["clickZones"].is_array());
        assert!(body["prefs"]["keybinds"]["page.next"].is_array());
    }

    /// A partial body must deserialize and normalize rather than 400.
    #[test]
    fn partial_body_normalizes() {
        let p: ReaderPrefs = serde_json::from_value(serde_json::json!({ "maxW": 5000 })).unwrap();
        let n = p.normalized();
        assert_eq!(n.max_w, 1400);
        assert_eq!(n.layout, "strip"); // unspecified field -> default
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p website reader_prefs -q`
Expected: FAIL — the module is not registered, so the test does not run yet. Register it first (Step 3 file list), then FAIL-to-compile on missing handlers.

- [ ] **Step 3: Implement the handlers**

```rust
//! Reader 2.0 preferences API.
//!
//! One JSON blob per user, validated by [`mangaread_core::ReaderPrefs::normalized`].
//! Anonymous callers get 401: preferences are an identity-level concern and
//! the design contract's server blob is only authoritative when logged in.

use axum::extract::{Json, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::Arc;

use crate::auth::AuthUser;
use crate::AppState;

/// GET `/api/reader-prefs` — the current user's preferences.
pub async fn get_prefs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
) -> Response {
    let prefs = {
        let db = state.db.lock().await;
        match mangaread_core::db::get_reader_prefs(&db, user.id) {
            Ok(p) => p,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false}))).into_response(),
        }
    };
    Json(json!({ "prefs": prefs })).into_response()
}

/// POST `/api/reader-prefs` — replace the current user's preferences.
///
/// The body is normalized before storage and the stored value is echoed back,
/// so the client never has to guess what was accepted.
pub async fn post_prefs(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<mangaread_core::ReaderPrefs>,
) -> Response {
    let prefs = body.normalized();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::set_reader_prefs(&db, user.id, &prefs).is_err() {
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false}))).into_response();
        }
    }
    Json(json!({ "ok": true, "prefs": prefs })).into_response()
}
```

Wire up:
- `website/src/routes/mod.rs`: add `pub mod reader_prefs;` beside the other `pub mod` lines.
- `website/src/main.rs`: add beside the other API routes (after `/api/migrate`, `main.rs:151`):

```rust
        .route(
            "/api/reader-prefs",
            get(crate::routes::reader_prefs::get_prefs).post(crate::routes::reader_prefs::post_prefs),
        )
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p website reader_prefs -q`
Expected: PASS (2 tests).

- [ ] **Step 5: Verify auth behavior end to end**

Build and probe a temp instance:
```bash
cargo build -p website -q
MANGAREAD_DB=/tmp/mr-s3-t2.db ./target/debug/website &
# anonymous -> 401
curl -s -o /dev/null -w '%{http_code}\n' http://localhost:3000/api/reader-prefs
# register + login, then read/write
curl -s -c /tmp/mr-s3.jar -X POST http://localhost:3000/register \
  -d 'name=t&email=t@x.io&password=password123' -o /dev/null
curl -s -b /tmp/mr-s3.jar http://localhost:3000/api/reader-prefs
curl -s -b /tmp/mr-s3.jar -X POST http://localhost:3000/api/reader-prefs \
  -H 'Content-Type: application/json' -d '{"layout":"double","maxW":99999}'
```
Expected: `401`; then `{"prefs":{...}}` with `"layout":"strip"` and `"maxW":700`; then `{"ok":true,"prefs":{...,"layout":"double","maxW":1400,...}}`.
Kill the server and delete the temp DB and jar.

- [ ] **Step 6: Commit**

```bash
git add website/src/routes/reader_prefs.rs website/src/routes/mod.rs website/src/main.rs
git commit -m "feat(reader): reader preferences API"
```

**Review focus:** the `db` guard is never held across an await; anonymous is 401 (not 500); the echoed blob equals the stored blob.

---

### Task 3: Notes API

**Files:**
- Create: `website/src/routes/notes.rs`
- Modify: `website/src/routes/mod.rs` (add `pub mod notes;`)
- Modify: `website/src/main.rs` (register the routes)
- Test: `website/src/routes/notes.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `mangaread_core::db::{insert_note, list_notes}` (`db.rs:906`, `db.rs:921`); `mangaread_core::Note { id, user_id, chapter_id, page, body, created_at }`; `AuthUser`.
- Produces:
  - `GET /api/notes?chapter_id={id}` → `200 {"notes":[{"id":N,"page":N,"body":"...","created_at":"..."}]}`; `401` anonymous.
  - `POST /api/notes` body `{"chapter_id":N,"page":N,"body":"..."}` → `201 {"ok":true,"note":{...}}`; `400` on invalid body; `401` anonymous.
  - Handler names: `list_notes`, `create_note`.

- [ ] **Step 1: Write the failing tests**

Create `website/src/routes/notes.rs` containing the validation tests. The validation helper must be a pure function so it is testable without HTTP:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_validation() {
        assert!(validate_body("").is_err());
        assert!(validate_body(&"x".repeat(2000)).is_ok());
        assert!(validate_body(&"x".repeat(2001)).is_err());
        assert!(validate_body("  trimmed  ").is_ok());
    }

    #[test]
    fn page_validation() {
        assert!(validate_page(0).is_err());
        assert!(validate_page(1).is_ok());
        assert!(validate_page(9999).is_ok());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p website notes -q`
Expected: FAIL to compile (`validate_body` / `validate_page` missing).

- [ ] **Step 3: Implement**

```rust
//! Private per-chapter reader notes.
//!
//! A note is anchored to a 1-indexed page, matching `progress.page` and
//! `data-page-idx`. There is no anonymous write path: `notes.user_id` is NOT
//! NULL and the design contract forbids inventing a shadow table.

use axum::extract::{Json, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;

use crate::auth::AuthUser;
use crate::AppState;

/// Max note length, mirrored in the client's `maxlength` and the design contract.
const MAX_NOTE_LEN: usize = 2000;

#[derive(Deserialize)]
pub struct ListQuery {
    pub chapter_id: i64,
}

#[derive(Deserialize)]
pub struct NewNote {
    pub chapter_id: i64,
    pub page: i64,
    pub body: String,
}

/// Note bodies are trimmed; empty and over-long bodies are rejected.
pub fn validate_body(body: &str) -> Result<String, ()> {
    let t = body.trim();
    if t.is_empty() || t.chars().count() > MAX_NOTE_LEN {
        return Err(());
    }
    Ok(t.to_string())
}

/// Pages are 1-indexed.
pub fn validate_page(page: i64) -> Result<i64, ()> {
    if page < 1 { Err(()) } else { Ok(page) }
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({"ok": false, "error": msg}))).into_response()
}

/// GET `/api/notes?chapter_id=N` — the caller's notes on a chapter.
pub async fn list_notes(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Query(q): Query<ListQuery>,
) -> Response {
    let notes = {
        let db = state.db.lock().await;
        match mangaread_core::db::list_notes(&db, user.id, q.chapter_id) {
            Ok(n) => n,
            Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"ok": false}))).into_response(),
        }
    };
    let items: Vec<_> = notes
        .into_iter()
        .map(|n| json!({"id": n.id, "page": n.page, "body": n.body, "created_at": n.created_at}))
        .collect();
    Json(json!({ "notes": items })).into_response()
}

/// POST `/api/notes` — create a note on a page.
pub async fn create_note(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    Json(body): Json<NewNote>,
) -> Response {
    let text = match validate_body(&body.body) {
        Ok(t) => t,
        Err(_) => return bad("Note must be 1-2000 characters."),
    };
    let page = match validate_page(body.page) {
        Ok(p) => p,
        Err(_) => return bad("Invalid page."),
    };
    let id = {
        let db = state.db.lock().await;
        // The chapter FK fails if the chapter does not exist.
        match mangaread_core::db::insert_note(&db, user.id, body.chapter_id, page, &text) {
            Ok(id) => id,
            Err(_) => return bad("Could not save note."),
        }
    };
    (
        StatusCode::CREATED,
        Json(json!({"ok": true, "note": {"id": id, "page": page, "body": text}})),
    )
        .into_response()
}
```

Wire up:
- `website/src/routes/mod.rs`: `pub mod notes;`
- `website/src/main.rs`:

```rust
        .route(
            "/api/notes",
            get(crate::routes::notes::list_notes).post(crate::routes::notes::create_note),
        )
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p website notes -q`
Expected: PASS.

- [ ] **Step 5: Verify per-user isolation end to end**

```bash
cargo build -p website -q
MANGAREAD_DB=/tmp/mr-s3-t3.db ./target/debug/website &
# register two users, each writes a note on the same chapter
# (use an existing chapter id; create one via the admin/editor path or pick a synced id)
```
Expected: user A's `GET /api/notes` never returns user B's note; anonymous gets 401; `page:0` gets 400; a 2001-char body gets 400.
Kill the server, remove the temp DB and jars.

- [ ] **Step 6: Commit**

```bash
git add website/src/routes/notes.rs website/src/routes/mod.rs website/src/main.rs
git commit -m "feat(reader): private per-chapter notes API"
```

**Review focus:** `user_id` always comes from `AuthUser`, never the request body; list is scoped by `(user_id, chapter_id)`.

---

### Task 4: Reader template — full contract markup

**Files:**
- Modify: `website/src/routes/reader.rs` (template blocks, ~lines 86-173)
- Modify: `website/src/templates/layout.rs` (`reader_layout` signature + body attributes + script/style links)
- Test: `website/src/routes/reader.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `ReaderPrefs` + `db::get_reader_prefs` (Task 1); `db::list_notes` (existing); `db::list_chapters_by_manga` (existing); `Option<AuthUser>`; the design contract's naming index (§9) verbatim.
- Produces: the exact DOM in design contract §3.2, plus:
  - `pub fn reader_layout(title: &str, content: Markup, config_css: &str, site_title: &str, user: Option<&AuthUser>, theme: &str, prefs: &mangaread_core::ReaderPrefs) -> Markup`
  - `pub fn body_prefs_attrs(prefs: &ReaderPrefs) -> String` in `reader.rs` (pure, testable)
  - `#reader-track` gains `data-prev-chapter`, `data-next-chapter`, `data-auto-next`, `data-prime-idx`
  - `<script type="application/json" id="reader-prefs">` carrying the full blob
  - `<link rel="stylesheet" href="/static/reader.css">` and `<script src="/static/reader.js" defer>`

- [ ] **Step 1: Write the failing tests**

In a new `#[cfg(test)] mod tests` in `website/src/routes/reader.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mangaread_core::ReaderPrefs;

    #[test]
    fn body_attrs_reflect_prefs() {
        let mut p = ReaderPrefs::default();
        p.layout = "double".into();
        p.direction = "rtl".into();
        p.fit = "height".into();
        p.bg = "white".into();
        p.frame = true;
        let a = body_prefs_attrs(&p);
        assert!(a.contains(r#"data-r-layout="double""#), "{a}");
        assert!(a.contains(r#"data-r-direction="rtl""#), "{a}");
        assert!(a.contains(r#"data-r-fit="height""#), "{a}");
        assert!(a.contains(r#"data-r-bg="white""#), "{a}");
        assert!(a.contains(r#"data-r-frame="on""#), "{a}");
    }

    #[test]
    fn body_attrs_omit_empty_values() {
        let p = ReaderPrefs::default(); // fit == "" and frame == false
        let a = body_prefs_attrs(&p);
        assert!(!a.contains("data-r-fit=\"\""), "{a}");
        assert!(!a.contains("data-r-frame"), "{a}");
    }

    #[test]
    fn custom_bg_emits_the_custom_property() {
        let mut p = ReaderPrefs::default();
        p.bg = "custom".into();
        p.bg_custom = "#123456".into();
        let a = body_prefs_attrs(&p);
        assert!(a.contains("--r-bg-custom:#123456"), "{a}");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p website reader:: -q`
Expected: FAIL to compile — `body_prefs_attrs` missing.

- [ ] **Step 3: Implement `body_prefs_attrs`**

Add above `chapter_reader` in `reader.rs`:

```rust
/// Body attributes for the reader, derived from stored preferences.
///
/// Rendered server-side so the first paint already has the right layout —
/// applying prefs in JS would flash the default layout on every navigation.
pub fn body_prefs_attrs(prefs: &mangaread_core::ReaderPrefs) -> String {
    let mut a = String::new();
    a.push_str(&format!(" data-r-layout=\"{}\"", prefs.layout));
    a.push_str(&format!(" data-r-direction=\"{}\"", prefs.direction));
    if !prefs.fit.is_empty() {
        a.push_str(&format!(" data-r-fit=\"{}\"", prefs.fit));
    }
    a.push_str(&format!(" data-r-bg=\"{}\"", prefs.bg));
    if prefs.frame {
        a.push_str(" data-r-frame=\"on\"");
    }
    if prefs.bg == "custom" {
        a.push_str(&format!(" style=\"--r-bg-custom:{}\"", prefs.bg_custom));
    }
    a
}
```

Note: `display_notes` is not a body attribute; it is carried in the prefs JSON only.

- [ ] **Step 4: Rewrite the template body to the contract**

Key changes in `chapter_reader`:
1. After the `needs_sync` block, load prefs and notes under the existing guard:

```rust
    let prefs = match user.as_ref() {
        Some(u) => mangaread_core::db::get_reader_prefs(&db, u.id).unwrap_or_default(),
        None => mangaread_core::ReaderPrefs::default(),
    };
    let notes = match user.as_ref() {
        Some(u) => mangaread_core::db::list_notes(&db, u.id, chapter.id).unwrap_or_default(),
        None => Vec::new(),
    };
```
(Inside the same scope as the `all_chapters` query, so the guard is held once.)

2. Compute URLs once:

```rust
    let prev_url = prev_chapter.map(|c| format!("/read/{}/{}", manga_id, c.id));
    let next_url = next_chapter.map(|c| format!("/read/{}/{}", manga_id, c.id));
```

3. `#reader-track` becomes:

```rust
        div id="reader-track" style="display:none"
            data-series-id=(manga.series_id.unwrap_or(0))
            data-chapter-id=(chapter.id)
            data-manga-id=(manga_id)
            data-total-pages=(total_pages)
            data-prev-chapter=[prev_url.as_deref()]
            data-next-chapter=[next_url.as_deref()]
            data-auto-next=[if prefs.auto_next { Some("1") } else { None }]
            data-auto-prev=[if prefs.auto_prev { Some("1") } else { None }]
            data-prime-idx="0" {}
```

4. `#reader-prefs` JSON + resume badge:

```rust
        script type="application/json" id="reader-prefs" {
            (maud::PreEscaped(prefs.to_json()))
        }
```

5. Top bar: keep `.reader-back-btn` (raise to 44px in CSS), keep title/chapter, keep `#reader-page-indicator` with `aria-live="polite"`, add:

```rust
                button id="reader-settings-btn" class="reader-icon-btn"
                    aria-expanded="false" aria-controls="reader-sheet"
                    aria-label="Reader settings" { "⚙" }
                button id="reader-focus-btn" class="reader-icon-btn"
                    aria-label="Toggle focus mode" { "◱" }
```

6. Replace the two `<div id="reader-click-left/right">` with the zone container:

```rust
            div id="reader-zones" data-r-zone-mode="three" {
                button class="reader-zone" data-zone="prev" aria-label="Previous page" {}
                button class="reader-zone" data-zone="menu" aria-label="Reader menu" {}
                button class="reader-zone" data-zone="next" aria-label="Next page" {}
            }
```

7. Pages: add state + placeholder + error + note pin:

```rust
                @for (i, page) in pages.iter().enumerate() {
                    @let load = if i == 0 { "eager" } else { "lazy" };
                    div class="reader-page" data-page-idx=(i + 1) data-page-state="idle" {
                        div class="reader-page-placeholder" aria-hidden="true" {}
                        img src=(page.image_url) alt={"Page "(i + 1)} loading=(load) decoding="async";
                        div class="reader-page-error" hidden {
                            p { "Could not load this page." }
                            button class="reader-retry-btn" type="button" { "Retry" }
                        }
                        @let page_notes: Vec<_> = notes.iter().filter(|n| n.page == (i as i64 + 1)).collect();
                        @if !page_notes.is_empty() {
                            button class="reader-note-pin" type="button"
                                data-note-id=(page_notes[0].id)
                                data-count=(page_notes.len())
                                data-page-idx=(i + 1)
                                aria-label={"Note on page "(i + 1)} { (page_notes.len()) }
                        }
                    }
                }
```

8. Add, after `#reader-images`: the HUD, empty state, chapter-nav overlay, resume badge, next-up, notes panel, note composer, dim overlay, offline banner, countdown, and the settings sheet. Each block uses the design contract's names verbatim (§3.2, §5.4, §5.5, §6, §7). The sheet ships the five `reader-sheet-group` sections with every control carrying `data-pref` (`layout`, `direction`, `fit`, `maxW`, `gap`, `bg`, `frame`, `clickZones`, `keybinds`, `swipeThreshold`, `autoNext`, `autoPrev`, `hideDelay`, `dimDelay`, `dimOpacity`, `showNotes`).

9. `#reader-nextup` is rendered from `next_chapter` plus up to 2 following chapters (`all_chapters` is already in scope): only render when `next_url.is_some()`; when absent, render nothing and let `#reader-chapter-nav` show the existing "Last Chapter" disabled button.

10. Update the `reader_layout` call to pass `&prefs`.

- [ ] **Step 5: Update `reader_layout`**

```rust
pub fn reader_layout(title: &str, content: Markup, config_css: &str, site_title: &str, user: Option<&AuthUser>, theme: &str, prefs: &mangaread_core::ReaderPrefs) -> Markup {
    let theme_class = theme_body_class(theme).map(|c| format!("reader-page {c}")).unwrap_or_else(|| "reader-page".to_string());
    let theme_link = crate::config::theme_css(theme);
    let prefs_attrs = PreEscaped(crate::routes::reader::body_prefs_attrs(prefs));
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { (title) " — " (site_title) }
                link rel="stylesheet" href="/static/style.css";
                @if let Some(href) = &theme_link {
                    link rel="stylesheet" href=(href);
                }
                link rel="stylesheet" href="/static/reader.css";
                style { (PreEscaped(config_css.to_string())) }
            }
            body class=(theme_class) data-user-id=[user.map(|u| u.id.to_string())] (prefs_attrs) {
                (content)
                script src="/static/app.js" {}
                script src="/static/auth.js" {}
                script src="/static/reader.js" defer {}
            }
        }
    }
}
```

Note: the `body` element needs the raw attributes; if maud will not accept `(prefs_attrs)` in the attribute position, use the documented escape hatch — build the body tag via `PreEscaped` for the opening tag only, or set the attributes with a loop over a `Vec<(&str, String)>`. Choose whichever compiles; the test in Step 1 asserts the string, not the mechanism.

- [ ] **Step 6: Run the tests + build**

Run: `cargo test -p website -q && cargo build -p website -q`
Expected: PASS; build clean apart from the pre-existing dead-code warnings.

- [ ] **Step 7: Verify the rendered DOM**

```bash
cargo build -p website -q
MANGAREAD_DB=/tmp/mr-s3-t4.db ./target/debug/website &
curl -s http://localhost:3000/read/1/1 | grep -o 'data-r-layout="[^"]*"'
curl -s http://localhost:3000/read/1/1 | grep -c 'reader-zone\|reader-note-pin\|reader-sheet'
```
Expected: `data-r-layout="strip"`; a non-zero count for the contract elements. Confirm no `data-source`, no adapter name, no origin URL appears in the response:
```bash
curl -s http://localhost:3000/read/1/1 | grep -ci 'source\|origin\|adapter\|resync'
```
Expected: `0`.
Kill the server; remove the temp DB.

- [ ] **Step 8: Commit**

```bash
git add website/src/routes/reader.rs website/src/templates/layout.rs
git commit -m "feat(reader): reader 2.0 markup contract"
```

**Review focus:** every control in the sheet carries a `data-pref`; no scraping concepts leak; the prefs JSON is escaped inside the script tag (a `</script>` in a keybind string cannot break out — maud's `PreEscaped` here is a real risk: assert with a prefs blob containing `</script>` that the response escapes it, or store JSON via `data-` attributes instead).

---

### Task 5: Reader stylesheet (`reader.css`)

**Files:**
- Create: `website/static/reader.css`
- Modify: `website/static/style.css` (remove the three dead reader selectors at `2293-2308` that target the removed `reader.js` nav; leave the rest as the base layer)
- Test: none (no CSS test harness). Verification is visual + a Lighthouse check at Task 7.

**Interfaces:**
- Consumes: the naming index in `docs/superpowers/specs/2026-09-20-reader2-design-contract.md` §9; existing base look in `style.css:1206-1424`.
- Produces: every selector in the contract, with all custom properties consumed via `var(--r-x, <fallback>)`.

**Delegation:** this task belongs to @designer (visual/interaction quality). The design contract is the spec; the implementer must not invent names.

- [ ] **Step 1: Implement the layout modes** — `[data-r-layout="single"]`, `"double"`, `"strip"`, `"vertical"]` blocks exactly as specified in contract §1.1, including the RTL spread order (`[data-r-direction="rtl"]`) and the `--r-double-cover` first-page-alone rule.

- [ ] **Step 2: Implement the fit system** — all §2.1 custom properties with fallbacks, the presets in §2.2, and the precedence rule ("fit mode wins over width"; the slider is `aria-disabled` while a fit mode is active). Light BG presets must flip `--r-chrome-bg`/`--r-chrome-fg`.

- [ ] **Step 3: Implement the controls surface** — top bar, HUD, `#reader-sheet` as a bottom sheet below 768px and a 340px right rail at ≥768px, tab strip with `scroll-snap-type: x mandatory`, sheet groups, `.reader-seg`/`.reader-seg-btn`, `.reader-slider` + `.reader-slider-value`, `#reader-zone-editor` handles, `.reader-keybind-row`.

- [ ] **Step 4: Implement comfort states** — `[data-r-focus="1"]`, the `#reader-dim` overlay (`pointer-events: none`, above pages, below chrome), `#reader-chapter-nav`, `#reader-resume-badge`, `#reader-nextup`, `#reader-countdown`, `#reader-offline`, `.reader-empty`.

- [ ] **Step 5: Implement states + notes** — `.reader-page[data-page-state]` for `idle|loading|ready|error`, `.reader-page-placeholder` reserving its box via `aspect-ratio`, `.reader-page-error`, `.reader-note-pin`, the composer, and the notes panel (right rail ≥768px, bottom sheet below).

- [ ] **Step 6: Implement responsive + a11y** — the three existing breakpoints only (`480`, `600`, `768`); 44px minimum touch targets including `.reader-back-btn` (currently 36px, `style.css:1246`); raise the page-indicator color from `rgba(255,255,255,0.4)` to `rgba(255,255,255,0.72)` (2.9:1 → ≥4.5:1) and mirror for light presets; `:focus-visible` outline using `var(--accent, #f59e0b)`; `@media (prefers-reduced-motion: reduce)` disabling the `hintPulse` keyframe and setting transitions to `0.01ms`; override `scroll-behavior: smooth` to `auto` inside `.reader-page`.

- [ ] **Step 7: Remove the dead rules from `style.css`**

Delete only these three selectors (they target markup that no longer exists): `.reader-page .nav-btn`, `.reader-page .nav-top`, `.reader-page .reader-nav` (`style.css:2293-2306`). Leave every other reader rule as the base layer `reader.css` builds on.

- [ ] **Step 8: Visual verification**

```bash
cargo build -p website -q
MANGAREAD_DB=/tmp/mr-s3-t5.db ./target/debug/website &
```
Check by hand on a synced chapter: all four layouts, both directions, each fit mode, all four backgrounds, frame on/off, sheet open/close at phone width and at ≥768px, focus mode, dim after idle, keyboard-only path, reduced-motion emulation.
Report what was checked and what could not be verified.

- [ ] **Step 9: Commit**

```bash
git add website/static/reader.css website/static/style.css
git commit -m "feat(reader): reader 2.0 stylesheet"
```

---

### Task 6: Reader client module (`reader.js`)

**Files:**
- Create: `website/static/reader.js` (replaces the dead same-named file)
- Modify: `website/static/app.js`: expose the read-state helpers and remove the reader branches now owned by `reader.js`
- Test: none (no JS harness). Behavior is verified by a browser pass; unit-testable pure helpers are optional but must not require a runner.

**Interfaces:**
- Consumes: the DOM contract from Task 4; `window.MangaSync` (`auth.js:107`) for `pushProgress`; `GET/POST /api/reader-prefs` (Task 2); `GET/POST /api/notes` (Task 3).
- Produces:
  - `window.MangaReader = { prefs, save, openSheet, closeSheet, goToPage, currentPage }`
  - localStorage keys exactly as contract §9 (`mangaread_reader_prefs`, `mangaread_series_prefs_<seriesId>`, `mangaread_reader_local`, `mangaread_note_draft_<chapterId>`)
  - `app.js` exposes `window.MangaReadState = { storageKey, getReadState, saveReadState, setChapterStatus }` and no longer initializes the reader itself.

- [ ] **Step 1: Refactor `app.js`**

Extract the existing read-state functions (`storageKey` `app.js:115`, `getReadState`/`saveReadState` `app.js:119-135`, `setChapterStatus` `app.js:143-154`) into `window.MangaReadState` without changing behavior, and delete the reader-only entry points: the `#reader-track` branch of `DOMContentLoaded` (`app.js:551-559`), `initReader` (`app.js:268-338`), `initReaderUX` (`app.js:341+`), and the scroll-to-resume block (`app.js:562-578`) when it is reader-page-only. Keep `initContinueReading`, the series chapter list, bookmarks, and the sync-status branches.
Verify the series page and home page still work (they use `MangaReadState`).

- [ ] **Step 2: Implement prefs resolution**

```js
// Resolution order: series override (local) > server blob (logged in) > local (anon) > defaults.
```
- `GET /api/reader-prefs` only when `document.body.dataset.userId` is set and `MangaSync.loggedIn()`.
- Merge `localStorage['mangaread_series_prefs_<seriesId>']` over the base object (full replacement per contract §3.4).
- Local-device state (`resumeBadgeSeen`, `hintSeen`, `zoom`, `lastUsedLayout`, `scrollPositions`) from `mangaread_reader_local`.
- Apply body attributes + `--r-*` properties on first run so the page matches the prefs even if the server attribute was absent (defensive; the server already did it for logged-in users).

- [ ] **Step 3: Implement the controls sheet** — open/close (center zone, `ui.toggle` key, `#reader-settings-btn`), focus trap, `aria-expanded` sync, focus restore, every `[data-pref]` control two-way bound, sliders with live `output`, segmented controls with `aria-pressed`, `#reader-remember-series` writing/deleting the series key.

- [ ] **Step 4: Implement the navigator** — one code path for `single`/`double`/`vertical` page-step and one for `strip` scroll, with the "current page" rule per contract §1.1, direction-aware zones, prev/next chapter at boundaries honoring `auto-prev`/`auto-next`, and `location.assign` (never `pushState`).

- [ ] **Step 5: Implement input** — zones (with the zone editor, clamped ≥10% apart), keybinds with `preventDefault` and the input-focus guard, collision rejection with an inline message, swipe by distance only, auto-next with the 3s countdown and sticky cancel.

- [ ] **Step 6: Implement comfort** — hide-UI timer (`hideDelay`, 0 = never), auto-dim (`dimDelay`, overlay, instant restore), focus mode, resume badge, HUD, end-of-chapter overlay.

- [ ] **Step 7: Implement notes** — composer open from `note.add`/long-press/sheet button, anchor to the current page, save via `POST /api/notes`, render pins from `GET /api/notes`, notes panel with jump-to-page, draft preservation for anonymous users in `sessionStorage`, "Discard note?" guard.

- [ ] **Step 8: Implement states + persistence** — page `idle→loading→ready→error` transitions with one auto-retry at 2000ms plus a manual retry, empty-chapter state, offline banner after repeated write-through failures, progress write-through via `MangaReadState.setChapterStatus` (so `MangaSync.pushProgress` still fires), debounced `POST /api/reader-prefs` for logged-in users, and the local-only fallback for anonymous users.

- [ ] **Step 9: Browser verification**

Drive the reader in a real browser (the repo's sync loop uses chromedriver; do not disturb a running instance — start your own on a temp port and temp DB). Verify: each layout advances correctly, RTL mirrors zones, fit modes clamp the slider, prefs survive a reload when logged in AND when anonymous, the series override wins and deletes cleanly, keybinds fire and remap, notes save/list/jump, progress still lands in `POST /api/progress`.
Report exactly which checks ran and which could not.

- [ ] **Step 10: Commit**

```bash
git add website/static/reader.js website/static/app.js
git commit -m "feat(reader): reader 2.0 client module"
```

**Review focus:** no duplicate progress writes after removing `app.js`'s reader path; localStorage keys are unchanged for series/home pages; the sheet traps focus and restores it; anonymous users can still read and their local prefs merge on signup (existing `/api/migrate` path untouched).

---

### Task 7: Slice acceptance + full verify

**Files:**
- Test: whole workspace; no production files unless a verification step finds a defect (one amend commit maximum, message `fix: slice 3 acceptance fixes`).

**Interfaces:** consumes everything above; produces a verification report only.

- [ ] **Step 1: Full suite**

Run: `cargo test --workspace -q`
Expected: all green (scraper + core + website).

- [ ] **Step 2: Release build**

Run: `cargo build --release -p website`
Expected: exit 0.

- [ ] **Step 3: Acceptance replay** (fresh temp DB, temp port; kill only what you start)

1. Register the bootstrap admin; open a synced chapter as admin.
2. Change layout to `double`, direction to `rtl`, fit to `height`, bg to `white`, frame on; reload — every choice survives.
3. Log out; change layout to `single` anonymously; reload — the anonymous choice survives locally; log back in — the server blob wins and no error appears.
4. Set a per-series override (remember-for-this-series) with a different gap; open a second series — the override does not leak.
5. Remap `page.next` to `KeyX`; confirm the old key no longer advances and `KeyX` does; confirm a collision is rejected inline.
6. Open the zone editor; drag boundaries; confirm clamping at 10% and that RTL still mirrors the actions.
7. Create a note on page 3; confirm the pin appears, the notes panel lists it, and jumping returns to page 3.
8. Confirm anonymous note creation prompts to sign in and the draft is offered back after login.
9. Confirm progress still writes (`POST /api/progress` observed) and resume works after a reload.
10. Confirm no reader-facing response contains a source/origin/adapter/resync string (grep the HTML of `/read/*`).
11. Confirm `/api/reader-prefs` and `/api/notes` return 401 for anonymous callers.
12. Lighthouse mobile on the reader page: performance/accessibility ≥90 with 3G throttling (excluding third-party images), or record the exact failing audits if the external image hosts dominate.

- [ ] **Step 4: Record results**

Write the verification report; if any step fails, make the minimum fix, re-run the failing step, and commit as `fix: slice 3 acceptance fixes`.

- [ ] **Step 5: Commit (only if a fix was needed)**

```bash
git add -A
git commit -m "fix: slice 3 acceptance fixes"
```

---

## Self-Review

**Spec coverage:**
- §4 modes → Task 1 (`layout`/`direction`), Task 4 (body attrs), Task 5 (mode CSS), Task 6 (navigator). ✅
- §4 fit → Task 1 (fields + clamps), Task 4 (presets), Task 5 (properties). ✅
- §4 pipeline (transcode, blur-up, prefetch, low-bandwidth) → **deliberately deferred to Slice 6**; the contract's placeholder marks ship only where the server has data (Task 4 Step 7). Recorded as a ruling.
- §4 input → Task 1 (keybinds/zones/swipe/auto-next), Task 5 (editor/keymap CSS), Task 6 (behavior). ✅
- §4 comfort → Task 1 (delays/opacity), Task 5, Task 6. ✅
- §4 continuity → Task 4 (resume badge, next-up), Task 6 (badge/queue/prefetch warm-up). ✅ Offline download toggle deferred to Slice 6.
- §4 notes → Task 3 (API), Task 4 (pins/composer/panel), Task 5 (styles), Task 6 (behavior). ✅
- §6 reader defaults → Task 1 + Task 2 (persisted, validated). ✅
- §13 data model → `users.reader_prefs` column (Task 1); `notes` reuse (Task 3). ✅
- §14 routes → `/api/reader-prefs`, `/api/notes` (Tasks 2-3). ✅
- §16 acceptance → Task 7 Steps 3.2, 3.3, 3.10, 3.12. ✅

**Placeholder scan:** no TBD/TODO; every task names files, symbols, and exact values. Task 4 Step 4 item 8 and Task 5 Steps 1-6 reference the contract by section rather than repeating ~400 lines of CSS — the contract is a committed artifact in the same repo and is named in the plan header as binding, so this is a reference to a real document, not a placeholder.

**Type consistency:** `ReaderPrefs` fields are snake_case in Rust and camelCase on the wire (`#[serde(rename_all = "camelCase")]`), matching the contract's prefs keys (`maxW`, `bgCustom`, `clickZones`, `autoNext`, `hideDelay`, `dimOpacity`, `swipeThreshold`, `showNotes`). `data-pref` values use the camelCase wire names. `progress.page` and `data-page-idx` are both 1-indexed; `notes.page` matches.

**Known risk to watch in review:** Task 4 Step 4 item 8 requires a large template write; if the reviewer finds the prefs JSON unescaped inside the script tag, that is a real defect (see Task 4 Review focus).
