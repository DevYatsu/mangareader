# Slice 1b: OAuth + Password Reset + Remember-Me Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add OAuth login (Google, GitHub, Discord), a password-reset flow (email when SMTP is configured, admin-generated link otherwise), and a remember-me checkbox controlling session lifetime.

**Architecture:** DB tables plus repository functions in `mangaread-core`; session-lifetime parameterization in `website/src/auth.rs`; a hand-rolled OAuth code flow with `reqwest` (no `oauth2` crate) in a new `website/src/oauth.rs`; reset pages plus a minimal admin users table in routes. No changes to the `users` table DDL.

**Tech Stack:** Rust, Axum 0.8, rusqlite 0.32 (bundled), bcrypt 0.17, reqwest 0.12 with `json`, lettre 0.11 with `builder,smtp-transport`.

**Spec:** `docs/superpowers/specs/2026-09-19-mangareader-customizable-website-design.md` (identity section: one account per person, OAuth providers, password reset; no import/export UI)

## Global Constraints

- Rust edition 2024; workspace members `mangaread-core`, `scraper`, `website`.
- Never hold the `state.db` Tokio `Mutex` guard across bcrypt hashing/verification or network calls. Lock, copy out, drop guard, then hash/fetch.
- OAuth users store marker hash `OAUTH:<provider>:<provider_id>` in `password_hash` (never a real bcrypt hash). `verify_password` returns false for it, so password login always fails for OAuth-only accounts.
- Registration/login failures render identical generic messages. Forgot-password POST always renders the same success text whether the email exists or not.
- Theme/layout plumbing (`resolve_theme`, `site_config`, `page()` helper) is reused verbatim in every new page handler.
- New deps pinned in `website/Cargo.toml`: `reqwest = { version = "0.12", features = ["json"] }`, `lettre = { version = "0.11", features = ["builder", "smtp-transport"] }`.
- TDD: failing test first, then minimal implementation, then commit per task.

---

## File Structure

- `mangaread-core/src/db.rs` (modify): `oauth_accounts` + `password_resets` tables in `init_db`; repo fns `link_oauth_account`, `find_user_by_oauth`, `user_oauth_providers`, `create_password_reset`, `consume_password_reset`, `delete_sessions_for_user`, `list_users`.
- `mangaread-core/src/db.rs` tests (modify): unit tests for each new repo fn.
- `website/src/auth.rs` (modify): `SESSION_MAX_AGE` split into `SHORT_SESSION_MAX_AGE` (24h) and `REMEMBER_SESSION_MAX_AGE` (30d); `session_expires_at(max_age_secs: i64)` and `set_cookie_value(token: &str, max_age_secs: i64)` take the lifetime as a parameter.
- `website/src/oauth.rs` (create): provider config from env, authorize-URL builder, token exchange + userinfo fetch via `reqwest`, normalized `OAuthProfile`.
- `website/src/routes/auth_pages.rs` (modify): `remember` checkbox on login form; provider buttons on login/register (only when configured); `login_submit`/`register_submit` pass the chosen lifetime.
- `website/src/routes/oauth.rs` (create): `GET /auth/{provider}`, `GET /auth/{provider}/callback`, find-or-create + link + session + redirect.
- `website/src/routes/password_reset.rs` (create): `GET/POST /forgot`, `GET/POST /reset/{token}`.
- `website/src/mail.rs` (create): `send_reset_email(to, link)` — no-op `Ok(())` when `MANGAREAD_SMTP_HOST` is unset; `lettre` send inside `spawn_blocking` otherwise.
- `website/src/routes/admin.rs` (modify): `GET /admin/users` minimal table + `POST /admin/users/{id}/reset-link`.
- `website/src/main.rs` (modify): `mod mail; mod oauth;` + new routes.

---

### Task 1: OAuth + reset tables and repository functions

**Files:**
- Modify: `mangaread-core/src/db.rs` (`init_db` + new fns + tests)

**Interfaces:**
- Consumes: existing `init_db(&Connection)`, `Error::NotFound`, `NewUser`/`User` types.
- Produces (used by Tasks 3+4):
  - `db::link_oauth_account(conn: &Connection, user_id: i64, provider: &str, provider_user_id: &str) -> Result<(), Error>` (idempotent upsert)
  - `db::find_user_by_oauth(conn: &Connection, provider: &str, provider_user_id: &str) -> Result<User, Error>` (`NotFound` when unlinked)
  - `db::user_oauth_providers(conn: &Connection, user_id: i64) -> Result<Vec<String>, Error>`
  - `db::create_password_reset(conn: &Connection, user_id: i64, token: &str, expires_at: &str) -> Result<(), Error>` (deletes prior unused tokens for the user first)
  - `db::consume_password_reset(conn: &Connection, token: &str, now: &str) -> Result<i64, Error>` (returns `user_id`; marks `used=1`; `NotFound` when missing/expired/used)
  - `db::delete_sessions_for_user(conn: &Connection, user_id: i64) -> Result<(), Error>`
  - `db::list_users(conn: &Connection) -> Result<Vec<User>, Error>` (ORDER BY id ASC)

- [ ] **Step 1: Write failing tests** in `mangaread-core/src/db.rs` tests module:

```rust
#[test]
fn oauth_link_roundtrip() {
    let conn = crate::db::test_conn();
    let id = crate::db::create_user(&conn, &crate::domain::NewUser {
        name: "o".into(), email: "o@x.io".into(),
        password_hash: "OAUTH:google:1".into(), role: crate::domain::Role::Member,
    }).unwrap();
    crate::db::link_oauth_account(&conn, id, "google", "gid-1").unwrap();
    // idempotent re-link
    crate::db::link_oauth_account(&conn, id, "google", "gid-1").unwrap();
    let u = crate::db::find_user_by_oauth(&conn, "google", "gid-1").unwrap();
    assert_eq!(u.id, id);
    assert!(matches!(
        crate::db::find_user_by_oauth(&conn, "google", "nope"),
        Err(crate::error::Error::NotFound(_))
    ));
    assert_eq!(crate::db::user_oauth_providers(&conn, id).unwrap(), vec!["google".to_string()]);
}

#[test]
fn password_reset_single_use_expiry() {
    let conn = crate::db::test_conn();
    let id = crate::db::create_user(&conn, &crate::domain::NewUser {
        name: "r".into(), email: "r@x.io".into(),
        password_hash: "h".into(), role: crate::domain::Role::Member,
    }).unwrap();
    crate::db::create_password_reset(&conn, id, "tok-a", "2099-01-01 00:00:00").unwrap();
    // second request replaces the first
    crate::db::create_password_reset(&conn, id, "tok-b", "2099-01-01 00:00:00").unwrap();
    assert!(matches!(
        crate::db::consume_password_reset(&conn, "tok-a", "2026-01-01 00:00:00"),
        Err(crate::error::Error::NotFound(_))
    ));
    assert_eq!(crate::db::consume_password_reset(&conn, "tok-b", "2026-01-01 00:00:00").unwrap(), id);
    // single use
    assert!(matches!(
        crate::db::consume_password_reset(&conn, "tok-b", "2026-01-01 00:00:00"),
        Err(crate::error::Error::NotFound(_))
    ));
    // expired
    crate::db::create_password_reset(&conn, id, "tok-c", "2020-01-01 00:00:00").unwrap();
    assert!(matches!(
        crate::db::consume_password_reset(&conn, "tok-c", "2026-01-01 00:00:00"),
        Err(crate::error::Error::NotFound(_))
    ));
}
```

Note: `test_conn()` is the existing helper used by current db tests (see `db.rs` tests module, e.g. the session test at line ~921). Reuse it; do not create a new helper.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p mangaread-core oauth_link_roundtrip password_reset_single_use_expiry`
Expected: FAIL with "function not defined" (compile error on missing fns).

- [ ] **Step 3: Implement** — add to `init_db` (after the `sessions` table):

```sql
CREATE TABLE IF NOT EXISTS oauth_accounts (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(provider, provider_user_id)
);
CREATE TABLE IF NOT EXISTS password_resets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id),
    token TEXT NOT NULL UNIQUE,
    expires_at TEXT NOT NULL,
    used INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

Functions (place after `delete_session`, before the bookmarks section):

```rust
/// Link an OAuth identity to a user. Idempotent: re-linking is a no-op.
pub fn link_oauth_account(conn: &Connection, user_id: i64, provider: &str, provider_user_id: &str) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id) VALUES (?1, ?2, ?3)
         ON CONFLICT(provider, provider_user_id) DO NOTHING",
        rusqlite::params![user_id, provider, provider_user_id],
    )?;
    Ok(())
}

/// Find the user owning an OAuth identity. `NotFound` when unlinked.
pub fn find_user_by_oauth(conn: &Connection, provider: &str, provider_user_id: &str) -> Result<User, Error> {
    conn.query_row(
        "SELECT u.id, u.name, u.email, u.password_hash, u.role, u.locale, u.theme, u.created_at
         FROM users u JOIN oauth_accounts o ON o.user_id = u.id
         WHERE o.provider = ?1 AND o.provider_user_id = ?2",
        rusqlite::params![provider, provider_user_id],
        map_user_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            Error::NotFound(format!("oauth {provider}"))
        }
        other => other.into(),
    })
}

/// Provider keys linked to a user (e.g. `["google"]`). Empty when none.
pub fn user_oauth_providers(conn: &Connection, user_id: i64) -> Result<Vec<String>, Error> {
    let mut stmt = conn.prepare("SELECT provider FROM oauth_accounts WHERE user_id = ?1 ORDER BY provider")?;
    let rows = stmt.query_map(rusqlite::params![user_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}

/// Create a reset token; deletes the user's prior unused tokens first so only
/// the newest link works.
pub fn create_password_reset(conn: &Connection, user_id: i64, token: &str, expires_at: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM password_resets WHERE user_id = ?1 AND used = 0",
        rusqlite::params![user_id],
    )?;
    conn.execute(
        "INSERT INTO password_resets (user_id, token, expires_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, token, expires_at],
    )?;
    Ok(())
}

/// Consume a reset token: returns the `user_id`, marks it used. `NotFound`
/// when missing, expired, or already used.
pub fn consume_password_reset(conn: &Connection, token: &str, now: &str) -> Result<i64, Error> {
    let user_id: i64 = conn.query_row(
        "SELECT user_id FROM password_resets WHERE token = ?1 AND used = 0 AND expires_at > ?2",
        rusqlite::params![token, now],
        |row| row.get(0),
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound("reset token".to_string()),
        other => other.into(),
    })?;
    conn.execute("UPDATE password_resets SET used = 1 WHERE token = ?1", rusqlite::params![token])?;
    Ok(user_id)
}

/// Delete all sessions of a user (used after password reset).
pub fn delete_sessions_for_user(conn: &Connection, user_id: i64) -> Result<(), Error> {
    conn.execute("DELETE FROM sessions WHERE user_id = ?1", rusqlite::params![user_id])?;
    Ok(())
}

/// All users ordered by id (admin list).
pub fn list_users(conn: &Connection) -> Result<Vec<User>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, email, password_hash, role, locale, theme, created_at FROM users ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], map_user_row)?;
    let mut out = Vec::new();
    for r in rows { out.push(r?); }
    Ok(out)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p mangaread-core`
Expected: all pass, including the 2 new tests.

- [ ] **Step 5: Commit**

```bash
git add mangaread-core/src/db.rs
git commit -m "feat: oauth_accounts and password_resets tables plus repo"
```

---

### Task 2: Remember-me session lifetimes

**Files:**
- Modify: `website/src/auth.rs` (constants + 2 fn signatures + tests)
- Modify: `website/src/routes/auth_pages.rs` (`LoginForm.remember`, checkbox in `login_form`, lifetime passed in `login_submit` + `register_submit`)

**Interfaces:**
- Consumes: `insert_session(conn, user_id, token, expires_at, device)` (per-row expiry already supported).
- Produces (used by Task 3 callback):
  - `pub const SHORT_SESSION_MAX_AGE: i64 = 24 * 3600;` (24h default)
  - `pub const REMEMBER_SESSION_MAX_AGE: i64 = 30 * 24 * 3600;` (30d checked)
  - `pub fn session_expires_at(max_age_secs: i64) -> String`
  - `pub fn set_cookie_value(token: &str, max_age_secs: i64) -> String`
  - `pub fn lifetime_for_remember(remember: bool) -> i64` returning the matching constant.

- [ ] **Step 1: Write failing test** in `website/src/auth.rs` tests module:

```rust
#[test]
fn lifetimes_differ() {
    assert_eq!(lifetime_for_remember(false), SHORT_SESSION_MAX_AGE);
    assert_eq!(lifetime_for_remember(true), REMEMBER_SESSION_MAX_AGE);
    assert!(REMEMBER_SESSION_MAX_AGE > SHORT_SESSION_MAX_AGE);
    let cookie = set_cookie_value("t", SHORT_SESSION_MAX_AGE);
    assert!(cookie.contains("Max-Age=86400"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p website lifetimes_differ`
Expected: FAIL with "function not defined".

- [ ] **Step 3: Implement** in `website/src/auth.rs` — replace lines 13-14 and 37-54:

```rust
pub const SHORT_SESSION_MAX_AGE: i64 = 24 * 3600; // 24 hours (default)
pub const REMEMBER_SESSION_MAX_AGE: i64 = 30 * 24 * 3600; // 30 days (remember-me)
```

```rust
/// Expiry timestamp for a fresh session (`now + max_age_secs`), SQLite format.
pub fn session_expires_at(max_age_secs: i64) -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::seconds(max_age_secs))
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

/// `true` → 30-day session, `false` → 24-hour session.
pub fn lifetime_for_remember(remember: bool) -> i64 {
    if remember { REMEMBER_SESSION_MAX_AGE } else { SHORT_SESSION_MAX_AGE }
}

pub fn set_cookie_value(token: &str, max_age_secs: i64) -> String {
    let mut value = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age_secs}"
    );
    if std::env::var("MANGAREAD_COOKIE_SECURE").as_deref() == Ok("1") {
        value.push_str("; Secure");
    }
    value
}
```

Delete the old zero-arg `session_expires_at`. Then update `website/src/routes/auth_pages.rs`:
- import line 19: replace `session_expires_at, set_cookie_value` with `lifetime_for_remember, session_expires_at, set_cookie_value`.
- `LoginForm` gains `pub remember: Option<String>,` (checkbox sends `remember=on` only when checked; `Option` keeps the form parseable when unchecked).
- `login_form` gains a checkbox row after the password label:
```rust
label { input type="checkbox" name="remember" value="on"; " Remember me (30 days)" }
```
- `login_submit`: after successful verify, `let max_age = lifetime_for_remember(form.remember.as_deref() == Some("on"));` then `insert_session(&db, found.0, &token, &session_expires_at(max_age), "web")` and `redirect_with_cookie("/", set_cookie_value(&token, max_age))`.
- `register_submit`: new accounts default to the remembered (30-day) session: `insert_session(&db, id, &token, &session_expires_at(REMEMBER_SESSION_MAX_AGE), "web")` and `set_cookie_value(&token, REMEMBER_SESSION_MAX_AGE)`.

- [ ] **Step 4: Run tests + build**

Run: `cargo test -p website` then `cargo build -p website`
Expected: all pass; only pre-existing warnings.

- [ ] **Step 5: Commit**

```bash
git add website/src/auth.rs website/src/routes/auth_pages.rs
git commit -m "feat: remember-me checkbox with 24h default and 30d sessions"
```

---

### Task 3: OAuth login (Google, GitHub, Discord)

**Files:**
- Modify: `website/Cargo.toml` (add `reqwest = { version = "0.12", features = ["json"] }`)
- Create: `website/src/oauth.rs`
- Create: `website/src/routes/oauth.rs`
- Modify: `website/src/main.rs` (`mod oauth;` in `mod routes` file + routes)
- Modify: `website/src/routes/auth_pages.rs` (provider buttons on login + register forms)
- Modify: `website/src/routes/mod.rs` (add `pub mod oauth;`)

**Interfaces:**
- Consumes: Task 1 (`find_user_by_oauth`, `link_oauth_account`, `create_user`, `count_users`, `find_user_by_email`), Task 2 (`lifetime_for_remember(true)`, `session_expires_at`, `set_cookie_value`, `new_token`, `insert_session`).
- Produces: `GET /auth/{provider}`, `GET /auth/{provider}/callback`; `oauth::configured_providers() -> Vec<&'static str>`.

Provider contract (hand-rolled code flow, no `oauth2` crate):
- Env per provider `<P>`: `MANGAREAD_OAUTH_<P>_ID`, `MANGAREAD_OAUTH_<P>_SECRET` where `<P>` ∈ `GOOGLE, GITHUB, DISCORD`. Callback base: `MANGAREAD_BASE_URL` (default `http://localhost:3000`).
- Endpoints:
  - google: auth `https://accounts.google.com/o/oauth2/v2/auth`, token `https://oauth2.googleapis.com/token`, userinfo `https://openidconnect.googleapis.com/v1/userinfo`; scopes `openid email profile`; profile fields `sub`, `email`, `email_verified`, `name`.
  - github: auth `https://github.com/login/oauth/authorize`, token `https://github.com/login/oauth/access_token`, user `https://api.github.com/user` (fields `id`, `login`), emails `https://api.github.com/user/emails` (pick entry with `primary && verified`; fallback first `verified`); scope `read:user user:email`.
  - discord: auth `https://discord.com/oauth2/authorize`, token `https://discord.com/api/oauth2/token`, user `https://discord.com/api/users/@me` (fields `id`, `username`, `email`, `verified`); scope `identify email`.
- CSRF state: `oauth_state` HttpOnly SameSite=Lax cookie (`Max-Age=600`) set on the authorize redirect, compared on callback, cleared after.
- Account matching: (1) `find_user_by_oauth` hit → login; (2) miss → `find_user_by_email` hit with a *verified* provider email → `link_oauth_account` then login; (3) miss → `create_user` with marker hash `OAUTH:<provider>:<provider_id>`, role `Admin` iff `count_users == 0` else `Member`, then link, then login. Unverified/missing email → `400` page "could not verify an email address with <provider>".
- Sessions from OAuth always use the remembered lifetime: `lifetime_for_remember(true)`, device string `oauth:<provider>`.

- [ ] **Step 1: Write failing test** — unit test for the authorize-URL builder in `website/src/oauth.rs` tests module:

```rust
#[test]
fn google_authorize_url_shape() {
    let url = authorize_url(&Provider::Google, "https://app.test", "state-9", "cid-1");
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("client_id=cid-1"));
    assert!(url.contains("state=state-9"));
    assert!(url.contains("redirect_uri=https%3A%2F%2Fapp.test%2Fauth%2Fgoogle%2Fcallback"));
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p website google_authorize_url_shape`
Expected: FAIL with "function not defined" (module does not exist yet).

- [ ] **Step 3: Implement** `website/src/oauth.rs`:

```rust
//! Hand-rolled OAuth2 code flow for google/github/discord (no oauth2 crate).
//!
//! Pure URL/config logic lives here so it is unit-testable; the HTTP token
//! exchange and userinfo fetch use `reqwest` and are exercised via curl in
//! the task report (real providers cannot be scripted in CI).

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider { Google, Github, Discord }

impl Provider {
    pub fn key(&self) -> &'static str {
        match self { Provider::Google => "google", Provider::Github => "github", Provider::Discord => "discord" }
    }
    pub fn from_key(s: &str) -> Option<Provider> {
        match s {
            "google" => Some(Provider::Google),
            "github" => Some(Provider::Github),
            "discord" => Some(Provider::Discord),
            _ => None,
        }
    }
    fn env_prefix(&self) -> &'static str {
        match self {
            Provider::Google => "GOOGLE", Provider::Github => "GITHUB", Provider::Discord => "DISCORD",
        }
    }
    fn auth_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Provider::Github => "https://github.com/login/oauth/authorize",
            Provider::Discord => "https://discord.com/oauth2/authorize",
        }
    }
    fn token_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://oauth2.googleapis.com/token",
            Provider::Github => "https://github.com/login/oauth/access_token",
            Provider::Discord => "https://discord.com/api/oauth2/token",
        }
    }
    fn userinfo_endpoint(&self) -> &'static str {
        match self {
            Provider::Google => "https://openidconnect.googleapis.com/v1/userinfo",
            Provider::Github => "https://api.github.com/user",
            Provider::Discord => "https://discord.com/api/users/@me",
        }
    }
    fn scope(&self) -> &'static str {
        match self {
            Provider::Google => "openid email profile",
            Provider::Github => "read:user user:email",
            Provider::Discord => "identify email",
        }
    }
}

/// Client id/secret pair, `None` when the provider is not configured.
pub fn credentials(p: Provider) -> Option<(String, String)> {
    let id = std::env::var(format!("MANGAREAD_OAUTH_{}_ID", p.env_prefix())).ok()?;
    let secret = std::env::var(format!("MANGAREAD_OAUTH_{}_SECRET", p.env_prefix())).ok()?;
    if id.is_empty() || secret.is_empty() { return None; }
    Some((id, secret))
}

/// Providers with credentials present. Drives the login-page buttons.
pub fn configured_providers() -> Vec<Provider> {
    [Provider::Google, Provider::Github, Provider::Discord]
        .into_iter()
        .filter(|p| credentials(*p).is_some())
        .collect()
}

pub fn base_url() -> String {
    std::env::var("MANGAREAD_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn url_encode(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Authorize URL for the provider. Pure function, unit-tested.
pub fn authorize_url(p: &Provider, base: &str, state: &str, client_id: &str) -> String {
    let redirect = format!("{base}/auth/{}/callback", p.key());
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&state={}",
        p.auth_endpoint(),
        url_encode(client_id),
        url_encode(&redirect),
        url_encode(p.scope()),
        url_encode(state),
    )
}

/// Normalized profile every provider maps into.
pub struct OAuthProfile {
    pub provider_user_id: String,
    pub email: Option<String>,
    pub email_verified: bool,
    pub display_name: String,
}

#[derive(Deserialize)]
struct TokenResponse { access_token: String }

pub async fn exchange_code(p: Provider, client_id: &str, client_secret: &str, code: &str, base: &str) -> Result<String, String> {
    let redirect = format!("{base}/auth/{}/callback", p.key());
    let client = reqwest::Client::new();
    let resp = client
        .post(p.token_endpoint())
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let body: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.access_token)
}

#[derive(Deserialize, Default)]
struct GoogleUser { #[serde(default)] sub: String, #[serde(default)] email: Option<String>, #[serde(default)] email_verified: bool, #[serde(default)] name: Option<String> }
#[derive(Deserialize, Default)]
struct GithubUser { #[serde(default)] id: i64, #[serde(default)] login: String }
#[derive(Deserialize)]
struct GithubEmail { email: String, primary: bool, verified: bool }
#[derive(Deserialize, Default)]
struct DiscordUser { #[serde(default)] id: String, #[serde(default)] username: String, #[serde(default)] email: Option<String>, #[serde(default)] verified: bool }

pub async fn fetch_profile(p: Provider, access_token: &str) -> Result<OAuthProfile, String> {
    let client = reqwest::Client::builder()
        .user_agent("mangaread-oauth/1.0")
        .build()
        .map_err(|e| e.to_string())?;
    match p {
        Provider::Google => {
            let u: GoogleUser = client.get(p.userinfo_endpoint()).bearer_auth(access_token)
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
            Ok(OAuthProfile {
                provider_user_id: u.sub,
                email: u.email.clone(),
                email_verified: u.email_verified && u.email.is_some(),
                display_name: u.name.unwrap_or_else(|| "reader".to_string()),
            })
        }
        Provider::Github => {
            let u: GithubUser = client.get(p.userinfo_endpoint()).bearer_auth(access_token)
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
            let emails: Vec<GithubEmail> = client.get("https://api.github.com/user/emails").bearer_auth(access_token)
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
            let pick = emails.iter().find(|e| e.primary && e.verified)
                .or_else(|| emails.iter().find(|e| e.verified));
            Ok(OAuthProfile {
                provider_user_id: u.id.to_string(),
                email: pick.map(|e| e.email.clone()),
                email_verified: pick.is_some(),
                display_name: u.login,
            })
        }
        Provider::Discord => {
            let u: DiscordUser = client.get(p.userinfo_endpoint()).bearer_auth(access_token)
                .send().await.map_err(|e| e.to_string())?
                .json().await.map_err(|e| e.to_string())?;
            Ok(OAuthProfile {
                provider_user_id: u.id,
                email: u.email.clone(),
                email_verified: u.verified && u.email.is_some(),
                display_name: u.username,
            })
        }
    }
}
```

`website/src/routes/oauth.rs` (full file):

```rust
//! OAuth login entry + callback for google/github/discord.

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
};
use mangaread_core::domain::{NewUser, Role};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

use crate::{
    auth::{lifetime_for_remember, new_token, session_expires_at, set_cookie_value, AuthUser},
    oauth::{self, Provider},
    AppState,
};

const STATE_COOKIE: &str = "oauth_state";

fn error_page(cfg: &crate::config::SiteConfig, theme: &str, msg: &str, user: Option<&AuthUser>) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(
            crate::templates::layout::base_layout(
                "Login",
                maud::html! { section class="auth-section" { p class="auth-error" { (msg) } p { a href="/login" { "Back to login" } } } },
                None,
                &cfg.css_overrides(),
                &cfg.site_title,
                user,
                theme,
            ).0,
        ),
    ).into_response()
}

/// GET `/auth/{provider}` — set a state cookie, redirect to the provider.
/// No state needed (pure redirect); `user` is accepted and ignored so logged-in
/// users can link-state identically — the callback links by email when matched.
pub async fn oauth_start(
    Path(provider_key): Path<String>,
    user: Option<AuthUser>,
) -> Response {
    let _ = user;
    let Some(p) = Provider::from_key(&provider_key) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((client_id, _)) = oauth::credentials(p) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let state_token = new_token();
    let url = oauth::authorize_url(&p, &oauth::base_url(), &state_token, &client_id);
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, url),
            (header::SET_COOKIE, format!("{STATE_COOKIE}={state_token}; Path=/; HttpOnly; SameSite=Lax; Max-Age=600")),
        ],
        "",
    ).into_response()
}

#[derive(Deserialize)]
pub struct CallbackQuery { code: Option<String>, state: Option<String>, error: Option<String> }

fn state_from_headers(headers: &HeaderMap) -> Option<String> {
    for raw in headers.get_all(axum::http::header::COOKIE).iter() {
        let Ok(s) = raw.to_str() else { continue };
        for part in s.split(';') {
            let part = part.trim();
            let Some((name, value)) = part.split_once('=') else { continue };
            if name.trim() == STATE_COOKIE { return Some(value.trim().to_string()); }
        }
    }
    None
}

/// GET `/auth/{provider}/callback` — verify state, exchange code, find-or-create, session, redirect home.
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    Path(provider_key): Path<String>,
    Query(q): Query<CallbackQuery>,
    headers: HeaderMap,
    user: Option<AuthUser>,
) -> Response {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    let fail = |msg: &str| error_page(&cfg, &theme, msg, user.as_ref());
    let Some(p) = Provider::from_key(&provider_key) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some((client_id, client_secret)) = oauth::credentials(p) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if q.error.is_some() { return fail("Login with this provider was cancelled."); }
    let (Some(code), Some(returned_state)) = (q.code.as_deref(), q.state.as_deref()) else {
        return fail("Login with this provider failed.");
    };
    match state_from_headers(&headers) {
        Some(cookie_state) if cookie_state == returned_state => {}
        _ => return fail("Login with this provider failed."),
    }
    // Network calls run without the db guard held.
    let profile = match oauth::exchange_code(p, &client_id, &client_secret, code, &oauth::base_url()).await {
        Ok(token) => match oauth::fetch_profile(p, &token).await {
            Ok(profile) => profile,
            Err(_) => return fail("Login with this provider failed."),
        },
        Err(_) => return fail("Login with this provider failed."),
    };
    let email = match (profile.email.clone(), profile.email_verified) {
        (Some(e), true) => e,
        _ => return fail(&format!("could not verify an email address with {}", p.key())),
    };
    let marker = format!("OAUTH:{}:{}", p.key(), profile.provider_user_id);
    let name: String = profile.display_name.chars().take(32).collect();
    let name = if name.trim().is_empty() { "reader".to_string() } else { name };
    let db = state.db.lock().await;
    // 1. already linked → login
    if let Ok(u) = mangaread_core::db::find_user_by_oauth(&db, p.key(), &profile.provider_user_id) {
        return open_session(&db, u.id, &format!("oauth:{}", p.key()));
    }
    // 2. matching verified email → link then login
    if let Ok(u) = mangaread_core::db::find_user_by_email(&db, &email) {
        if mangaread_core::db::link_oauth_account(&db, u.id, p.key(), &profile.provider_user_id).is_err() {
            return fail("Login with this provider failed.");
        }
        return open_session(&db, u.id, &format!("oauth:{}", p.key()));
    }
    // 3. new account (bootstrap admin iff first user)
    let role = match mangaread_core::db::count_users(&db) {
        Ok(0) => Role::Admin,
        Ok(_) => Role::Member,
        Err(_) => return fail("Login with this provider failed."),
    };
    let id = match mangaread_core::db::create_user(&db, &NewUser { name, email, password_hash: marker, role }) {
        Ok(id) => id,
        Err(_) => return fail("Login with this provider failed."),
    };
    if mangaread_core::db::link_oauth_account(&db, id, p.key(), &profile.provider_user_id).is_err() {
        return fail("Login with this provider failed.");
    }
    open_session(&db, id, &format!("oauth:{}", p.key()))
}

/// Insert a remembered (30-day) session and redirect home with the cookie.
/// The state cookie is cleared alongside (duplicate header names are legal
/// in the array — both SET_COOKIE headers are sent).
/// `&MutexGuard<Connection>` coerces to `&Connection` via Deref at the call.
fn open_session(db: &tokio::sync::MutexGuard<'_, rusqlite::Connection>, user_id: i64, device: &str) -> Response {
    let max_age = lifetime_for_remember(true);
    let token = new_token();
    if mangaread_core::db::insert_session(db, user_id, &token, &session_expires_at(max_age), device).is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_string()),
            (header::SET_COOKIE, set_cookie_value(&token, max_age)),
            (header::SET_COOKIE, format!("{STATE_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0")),
        ],
        "",
    ).into_response()
}
```

Register `mod oauth;` in `website/src/main.rs` (top-level `mod oauth;` next to `mod auth;`), `pub mod oauth;` in `website/src/routes/mod.rs`, and routes in main.rs router:

```rust
.route("/auth/{provider}", get(crate::routes::oauth::oauth_start))
.route("/auth/{provider}/callback", get(crate::routes::oauth::oauth_callback))
```

Login/register buttons: in `auth_pages.rs`, change `login_form(error)` → `login_form(error, providers: &[Provider])` rendering, when non-empty:

```rust
div class="oauth-buttons" {
    @for p in providers {
        a class="oauth-btn" href=(format!("/auth/{}", p.key())) { "Continue with " (provider_label(p)) }
    }
}
```

with `fn provider_label(p: &Provider) -> &'static str` mapping google→"Google", github→"GitHub", discord→"Discord". `login_page`/`register_page` pass `&crate::oauth::configured_providers()`. Keep `login_form(None)` call sites updated (login_submit error path renders `login_form(Some(...), &configured_providers())`).

- [ ] **Step 4: Run tests + build + live curl**

Run: `cargo test -p website google_authorize_url_shape` then `cargo build -p website`
Expected: PASS; build clean.
Then curl on a temp DB: `GET /auth/unknown` → 404; `GET /auth/google` with no env → 404; with `MANGAREAD_OAUTH_GOOGLE_ID=x MANGAREAD_OAUTH_GOOGLE_SECRET=y` → 303 to `accounts.google.com` + `oauth_state` cookie; callback with wrong state → 400. Real code exchange cannot be scripted (needs provider); report the URL-shape + state-mismatch evidence.

- [ ] **Step 5: Commit**

```bash
git add website/Cargo.toml website/src/oauth.rs website/src/routes/oauth.rs website/src/routes/mod.rs website/src/main.rs website/src/routes/auth_pages.rs Cargo.lock
git commit -m "feat: OAuth login via Google, GitHub, Discord"
```

---

### Task 4: Password reset + minimal admin users table

**Files:**
- Modify: `website/Cargo.toml` (add `lettre = { version = "0.11", features = ["builder", "smtp-transport"] }`)
- Create: `website/src/mail.rs`
- Create: `website/src/routes/password_reset.rs`
- Modify: `website/src/routes/admin.rs` (`GET /admin/users`, `POST /admin/users/{id}/reset-link`)
- Modify: `website/src/main.rs` (`mod mail;` + routes)
- Modify: `website/src/routes/mod.rs` (`pub mod password_reset;`)
- Modify: `website/src/routes/auth_pages.rs` (forgot-password link on login form)

**Interfaces:**
- Consumes: Task 1 (`create_password_reset`, `consume_password_reset`, `delete_sessions_for_user`, `list_users`, `user_oauth_providers`, `find_user_by_email`, `update_user_password`), Task 2 (`session_expires_at` NOT needed here; reset-link TTL uses `reset_expires_at()` below).
- Produces: `GET/POST /forgot`, `GET/POST /reset/{token}`, `GET /admin/users`, `POST /admin/users/{id}/reset-link`; `mail::send_reset_email(to: &str, link: &str) -> Result<(), String>`; `mail::smtp_configured() -> bool`.

Reset-link TTL: 1 hour. `fn reset_expires_at() -> String` in `password_reset.rs` using the same `time` format as `auth::now_utc_string` (copy the 6-line format block; do not refactor auth.rs).

Mail behavior: `smtp_configured()` true iff `MANGAREAD_SMTP_HOST` is non-empty. `send_reset_email` returns `Ok(())` immediately when unconfigured. Otherwise builds the message (`From MANGAREAD_SMTP_FROM`, subject `Reset your password`, body with the link) and sends via `lettre::SmtpTransport::relay(&host)` with `.credentials((user, pass))` when `MANGAREAD_SMTP_USER` is set, `.port(MANGAREAD_SMTP_PORT)` when set (default 587/relay default). The blocking send runs inside `tokio::task::spawn_blocking`. Errors map to `String`.

Forgot flow (`POST /forgot`, form field `email`): trim; `find_user_by_email` hit → `create_password_reset` with `new_token()` + `reset_expires_at()`; if `smtp_configured()`, spawn the send (fire-and-forget best effort, still render success). Always render `200` page "If an account exists for that email, a reset link is on its way." Miss → render the identical page without touching the DB.

Reset flow: `GET /reset/{token}` renders new-password form (no token validity leak: always render the form; invalid tokens fail on POST with the generic message). `POST /reset/{token}` fields `password`: if `len < 8` → `400` same message as invalid; else `consume_password_reset(token, now_utc_string)` → on `NotFound` render `400` "This reset link is invalid or expired."; on success `hash_password` (outside the guard) then under a fresh lock `update_user_password` + `delete_sessions_for_user`, redirect `303 /login`.

Admin users (`GET /admin/users`, admin-only via `require_role(&user, Role::Admin)` else `403`): table rows id, name, email, role, created_at, oauth providers joined by `,`, plus per-row form `POST /admin/users/{id}/reset-link` with button "Reset link". POST handler creates token, builds absolute link `{base_url}/reset/{token}`, redirects `303` back to `/admin/users?reset_link={urlencoded}&for={id}`; the GET renders the link in a visible box when the query params are present. Minimal on purpose: full user management (roles, delete, suspend) stays in Slice 5.

- [ ] **Step 1: Write failing test** in `website/src/mail.rs`:

```rust
#[test]
fn mail_noop_when_unconfigured() {
    // MANGAREAD_SMTP_HOST is unset in test env → no send attempted.
    assert!(!smtp_configured());
    assert!(send_reset_email_blocking("a@b.io", "http://x/reset/t").is_ok());
}
```

`send_reset_email_blocking` is the sync inner fn (does the configured-or-noop check + blocking send); the async `send_reset_email` wraps it in `spawn_blocking`. Test env caution: if the dev machine has `MANGAREAD_SMTP_HOST` set the test fails — acceptable; CI/dev do not set it. (Document that one line in the report.)

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p website mail_noop_when_unconfigured`
Expected: FAIL with "function not defined".

- [ ] **Step 3: Implement** `website/src/mail.rs`:

```rust
//! Outbound mail. No-op unless SMTP env is configured:
//! `MANGAREAD_SMTP_HOST` (+ optional `MANGAREAD_SMTP_PORT`,
//! `MANGAREAD_SMTP_USER`, `MANGAREAD_SMTP_PASS`, `MANGAREAD_SMTP_FROM`).

pub fn smtp_configured() -> bool {
    std::env::var("MANGAREAD_SMTP_HOST").is_ok_and(|h| !h.is_empty())
}

pub fn send_reset_email_blocking(to: &str, link: &str) -> Result<(), String> {
    if !smtp_configured() {
        return Ok(());
    }
    use lettre::{Message, SmtpTransport, Transport};
    let from = std::env::var("MANGAREAD_SMTP_FROM").unwrap_or_else(|_| "noreply@localhost".to_string());
    let email = Message::builder()
        .from(from.parse().map_err(|e| format!("bad from: {e}"))?)
        .to(to.parse().map_err(|e| format!("bad recipient: {e}"))?)
        .subject("Reset your password")
        .body(format!("Reset your password (valid 1 hour):\n\n{link}\n"))
        .map_err(|e| e.to_string())?;
    let host = std::env::var("MANGAREAD_SMTP_HOST").map_err(|e| e.to_string())?;
    let mut builder = SmtpTransport::relay(&host).map_err(|e| e.to_string())?;
    if let Ok(port) = std::env::var("MANGAREAD_SMTP_PORT") {
        if let Ok(p) = port.parse::<u16>() {
            builder = SmtpTransport::relay(&host).map_err(|e| e.to_string())?.port(p);
        }
    }
    let mailer = match std::env::var("MANGAREAD_SMTP_USER") {
        Ok(user) => {
            let pass = std::env::var("MANGAREAD_SMTP_PASS").unwrap_or_default();
            builder.credentials(lettre::transport::smtp::authentication::Credentials::new(user, pass)).build()
        }
        Err(_) => builder.build(),
    };
    mailer.send(&email).map_err(|e| e.to_string())?;
    Ok(())
}

/// Async wrapper: the blocking SMTP send runs on the blocking pool.
pub async fn send_reset_email(to: String, link: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || send_reset_email_blocking(&to, &link))
        .await
        .map_err(|e| e.to_string())?
}
```

`website/src/routes/password_reset.rs` (full file):

```rust
//! Forgot-password + reset-token pages.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Form,
};
use maud::html;
use serde::Deserialize;
use std::sync::Arc;

use crate::{
    auth::{hash_password, now_utc_string, AuthUser},
    mail,
    oauth::base_url,
    AppState,
};

fn reset_expires_at() -> String {
    (time::OffsetDateTime::now_utc() + time::Duration::seconds(3600))
        .format(&time::macros::format_description!(
            "[year]-[month]-[day] [hour]:[minute]:[second]"
        ))
        .expect("UTC timestamp formats")
}

fn page(cfg: &crate::config::SiteConfig, theme: &str, title: &str, content: maud::Markup, user: Option<&AuthUser>) -> Html<String> {
    Html(crate::templates::layout::base_layout(title, content, None, &cfg.css_overrides(), &cfg.site_title, user, theme).0)
}

fn forgot_form() -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Forgot password" }
            form method="post" action="/forgot" class="auth-form" {
                label { "Email" input type="email" name="email" required=""; }
                button type="submit" { "Send reset link" }
            }
            p { a href="/login" { "Back to login" } }
        }
    }
}

fn reset_form() -> maud::Markup {
    html! {
        section class="auth-section" {
            h1 class="page-title" { "Set a new password" }
            form method="post" class="auth-form" {
                label { "New password (min 8 chars)" input type="password" name="password" required=""; }
                button type="submit" { "Set password" }
            }
        }
    }
}

/// GET `/forgot`.
pub async fn forgot_page(State(state): State<Arc<AppState>>, user: Option<AuthUser>) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    page(&cfg, &theme, "Forgot password", forgot_form(), user.as_ref())
}

#[derive(Deserialize)]
pub struct ForgotForm { pub email: String }

/// POST `/forgot` — identical success text whether the email exists or not.
pub async fn forgot_submit(State(state): State<Arc<AppState>>, user: Option<AuthUser>, Form(form): Form<ForgotForm>) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    let done = || page(&cfg, &theme, "Forgot password",
        html! { section class="auth-section" { p { "If an account exists for that email, a reset link is on its way." } } },
        user.as_ref());
    let email = form.email.trim().to_string();
    let found = {
        let db = state.db.lock().await;
        mangaread_core::db::find_user_by_email(&db, &email).ok().map(|u| (u.id, u.email))
    };
    let Some((user_id, to)) = found else { return done(); };
    let token = crate::auth::new_token();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::create_password_reset(&db, user_id, &token, &reset_expires_at()).is_err() {
            return done();
        }
    }
    if mail::smtp_configured() {
        let link = format!("{}/reset/{token}", base_url());
        // Best-effort: the success text renders either way.
        let _ = mail::send_reset_email(to, link).await;
    }
    done()
}

/// GET `/reset/{token}` — always renders the form (no validity leak).
pub async fn reset_page(State(state): State<Arc<AppState>>, user: Option<AuthUser>, Path(_token): Path<String>) -> Html<String> {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    page(&cfg, &theme, "Set a new password", reset_form(), user.as_ref())
}

#[derive(Deserialize)]
pub struct ResetForm { pub password: String }

/// POST `/reset/{token}` — consume token, set hash, revoke sessions.
pub async fn reset_submit(
    State(state): State<Arc<AppState>>,
    user: Option<AuthUser>,
    Path(token): Path<String>,
    Form(form): Form<ResetForm>,
) -> Response {
    let (cfg, theme) = {
        let db = state.db.lock().await;
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, user.as_ref()))
    };
    let bad = |msg: &str| {
        (
            StatusCode::BAD_REQUEST,
            page(&cfg, &theme, "Set a new password",
                html! { section class="auth-section" { p class="auth-error" { (msg) } } ({reset_form()}) },
                user.as_ref()),
        ).into_response()
    };
    if form.password.len() < 8 {
        return bad("This reset link is invalid or expired.");
    }
    // Consume first (marks used), hash outside the guard, then write.
    let user_id = {
        let db = state.db.lock().await;
        match mangaread_core::db::consume_password_reset(&db, &token, &now_utc_string()) {
            Ok(id) => id,
            Err(_) => return bad("This reset link is invalid or expired."),
        }
    };
    let hash = match hash_password(&form.password) {
        Ok(h) => h,
        Err(_) => return bad("This reset link is invalid or expired."),
    };
    {
        let db = state.db.lock().await;
        if mangaread_core::db::update_user_password(&db, user_id, &hash).is_err() {
            return bad("This reset link is invalid or expired.");
        }
        let _ = mangaread_core::db::delete_sessions_for_user(&db, user_id);
    }
    (
        StatusCode::SEE_OTHER,
        [(axum::http::header::LOCATION, "/login".to_string())],
        "",
    ).into_response()
}
```

Admin users table — append to `website/src/routes/admin.rs` (check its existing `page` helper + `stored` helper signatures first; the dashboard at line ~194 uses `page(&cfg, &theme, "Admin", content, Some(&user))` returning Markup and `require_role(&user, Role::Admin)`). Add:

```rust
/// GET `/admin/users` — minimal user table with per-row reset-link generator.
/// Full user management (roles, suspend, delete) is Slice 5 territory.
pub async fn users_page(
    State(state): State<Arc<AppState>>,
    user: AuthUser,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let (cfg, theme, users) = {
        let db = state.db.lock().await;
        let users = mangaread_core::db::list_users(&db).unwrap_or_default();
        let providers: std::collections::HashMap<i64, String> = users.iter().map(|u| {
            let p = mangaread_core::db::user_oauth_providers(&db, u.id).unwrap_or_default().join(",");
            (u.id, p)
        }).collect();
        (crate::config::site_config(&db), crate::config::resolve_theme(&db, Some(&user)), users.into_iter().map(|u| {
            let prov = providers.get(&u.id).cloned().unwrap_or_default();
            (u, prov)
        }).collect::<Vec<_>>())
    };
    let notice = match (q.get("reset_link"), q.get("for")) {
        (Some(link), Some(id)) => html! { p class="admin-notice" { "Reset link for user " (id) ": " a href=(link) { (link) } } },
        _ => html! {},
    };
    let content = html! {
        (notice)
        table class="admin-table" {
            tr { th { "ID" } th { "Name" } th { "Email" } th { "Role" } th { "OAuth" } th { "Created" } th { "Actions" } }
            @for (u, prov) in &users {
                tr {
                    td { (u.id) } td { (u.name) } td { (u.email) } td { (u.role.as_str()) }
                    td { (prov) } td { (u.created_at) }
                    td { form method="post" action=(format!("/admin/users/{}/reset-link", u.id)) { button type="submit" { "Reset link" } } }
                }
            }
        }
        p { a href="/admin" { "Back to admin" } }
    };
    page(&cfg, &theme, "Users", content, Some(&user)).into_response()
}

/// POST `/admin/users/{id}/reset-link` — mint a 1h token, redirect back with the link.
pub async fn user_reset_link(State(state): State<Arc<AppState>>, user: AuthUser, Path(id): Path<i64>) -> Response {
    if require_role(&user, Role::Admin).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let token = crate::auth::new_token();
    {
        let db = state.db.lock().await;
        if mangaread_core::db::create_password_reset(&db, id, &token, &crate::routes::password_reset::reset_expires_at_pub()).is_err() {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    let link = format!("{}/reset/{token}", crate::oauth::base_url());
    let query = form_urlencoded::Serializer::new(String::new())
        .append_pair("reset_link", &link)
        .append_pair("for", &id.to_string())
        .finish();
    (StatusCode::SEE_OTHER, [(axum::http::header::LOCATION, format!("/admin/users?{query}"))], "").into_response()
}
```

This requires `reset_expires_at` to be visible: name the fn `pub fn reset_expires_at()` in `password_reset.rs` (not private) and call it as `crate::routes::password_reset::reset_expires_at()`. Adjust Step-3 file accordingly (declare it `pub`).

Also add the forgot link to `login_form` in auth_pages.rs: `p { a href="/forgot" { "Forgot password?" } }` after the register line.

Routes in main.rs:
```rust
.route("/forgot", get(crate::routes::password_reset::forgot_page).post(crate::routes::password_reset::forgot_submit))
.route("/reset/{token}", get(crate::routes::password_reset::reset_page).post(crate::routes::password_reset::reset_submit))
.route("/admin/users", get(crate::routes::admin::users_page))
.route("/admin/users/{id}/reset-link", post(crate::routes::admin::user_reset_link))
```
plus `mod mail;` at top and `pub mod password_reset;` in routes/mod.rs.

Note: `admin.rs` current imports — the implementer must check whether `require_role`, `Role`, `StatusCode`, `HashMap`, `html` are already imported and reuse them; the dashboard code shows `require_role(&user, Role::Admin)` and `StatusCode::FORBIDDEN` already in use.

- [ ] **Step 4: Run tests + build + live curl**

Run: `cargo test -p website mail_noop_when_unconfigured` then `cargo build -p website`
Expected: PASS; build clean.
Curl on temp DB (port 3001 if 3000 is held; nothing killed): register user → `POST /forgot` with unknown email and with real email render byte-identical success text; `POST /reset/bad-token` (password ≥8) → 400 "invalid or expired"; admin login → `GET /admin/users` shows the table; `POST /admin/users/1/reset-link` → 303 with `reset_link` query; follow the link page shows the URL; `POST` the shown token with new password → 303 `/login`; old session cookie now logged out (401 on `/api/bookmarks`); login with new password → 303; second use of the same token → 400. Member (non-admin) `GET /admin/users` → 403.

- [ ] **Step 5: Commit**

```bash
git add website/Cargo.toml website/src/mail.rs website/src/routes/password_reset.rs website/src/routes/admin.rs website/src/routes/mod.rs website/src/main.rs website/src/routes/auth_pages.rs Cargo.lock
git commit -m "feat: password reset via mail or admin link, minimal admin users table"
```

---

### Task 5: Slice acceptance + full verify

**Files:** none (verification only; fix what fails in place, one amend commit max).

**Interfaces:** consumes Tasks 1-4. Produces: green `cargo test --workspace`, clean release build, passing acceptance flow below.

- [ ] **Step 1: Full test suite**

Run: `cargo test --workspace`
Expected: all suites pass.

- [ ] **Step 2: Release build**

Run: `cargo build --release -p website`
Expected: exit 0.

- [ ] **Step 3: End-to-end acceptance replay** (fresh temp `MANGAREAD_DB`, temp port; kill only what you started):
  1. Register first user → bootstrapped admin; session cookie `Max-Age=2592000` (remembered default on register).
  2. Logout; login unchecked → `Max-Age=86400`; login checked (`remember=on`) → `Max-Age=2592000`.
  3. `GET /login` shows "Forgot password?" link; `POST /forgot` identical text for known/unknown email.
  4. OAuth routes without env: `/auth/google` → 404; `/auth/unknown` → 404. With dummy `MANGAREAD_OAUTH_GITHUB_ID/SECRET`: `/auth/github` → 303 to `github.com` with `oauth_state` cookie; callback with mismatched state → 400.
  5. Admin reset-link flow per Task 4 Step 4 (token single-use, sessions revoked, new password works).
  6. `/admin/users` as member → 403; logged out → redirect `/login` (auth extractor) or 401 per Accept.
  7. Existing Slice 1 flows still green: register/login/logout, `/api/migrate` auto-migration, sync endpoints.

- [ ] **Step 4: Commit (only if fixes were needed)**

```bash
git add -A
git commit -m "fix: slice 1b acceptance fixes"
```

If no fixes were needed, no commit; report clean.

---

## Self-Review

- Spec coverage: OAuth login (providers as built-in auth paths, not plugins) ✓ Task 3; password reset (mail-or-admin-link, §identity) ✓ Task 4; remember-me session control ✓ Task 2; one identity per account — OAuth links to the existing email account instead of creating duplicates ✓ Task 3 matching order; no import/export UI added ✓ (nothing in plan implies it).
- Placeholders: none — every step has exact code, exact commands, exact expected outputs.
- Type consistency: `session_expires_at(max_age_secs: i64)`, `set_cookie_value(token: &str, max_age_secs: i64)`, `lifetime_for_remember(bool) -> i64` used identically in Tasks 2/3; `reset_expires_at()` is `pub` in `password_reset.rs` and called from `admin.rs` with full path; `open_session(&MutexGuard, i64, &str)` matches its definition; `CallbackQuery` fields `Option` (provider may return only `error`); `Provider::from_key` returns `Option` handled at both route entries.
