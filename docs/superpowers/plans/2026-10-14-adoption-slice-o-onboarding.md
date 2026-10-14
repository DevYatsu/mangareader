# Adoption Redesign — Slice O: Onboarding & Accounts Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A fresh install is claimed safely by its owner. Anyone can sign up by email link. Admins manage roles and bans.

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice O.

**Split:** O1 (setup, roles, user management) → O2 (email sign-up and reset) → O3 (install path and README). Each sub-slice is one commit and leaves the app shippable.

## Global Constraints

- Existing installs (≥ 1 user) never see `/setup`, and their sessions keep working.
- Stored role strings stay readable: legacy `'editor'` maps to moderator.
- Self-registration and OAuth sign-up never grant a role above `member`.
- Error copy on auth forms never reveals whether an email address exists.

## O1 — Setup token, roles, user management

### Task O1.1: Testable router
- [x] Move route construction from `main()` into `pub fn app(state: Arc<AppState>) -> Router`. Add `AppState::new(conn)`.
- [x] Add `tower` (feature `util`) as a dev-dependency. Write an in-memory test harness: `test_app()` returns the router over `Connection::open_in_memory()`.
- [x] `main()` keeps the sync loop and the bind. Tests never start the sync loop.

### Task O1.2: Roles and bans in core
- [x] `Role::{Member, Moderator, Admin}`. `as_str` emits `"moderator"`. `from_str` accepts `"moderator"` and the legacy `"editor"`.
- [x] Migration: `UPDATE users SET role='moderator' WHERE role='editor'`. Add `users.banned_at TEXT NULL`.
- [x] `find_user_by_session` excludes banned users, so a ban takes effect on the next request.
- [x] `create_first_admin(conn, NewUser) -> Result<Option<i64>>` runs a single `INSERT … SELECT … WHERE NOT EXISTS (SELECT 1 FROM users)`. The check and the insert are one statement, so there is no TOCTOU window.
- [x] `mod_log(id, actor_id, action, target_user_id, detail, created_at)` table plus `insert_mod_log`.
- [x] `set_user_role` and `set_user_banned` refuse to remove the last unbanned admin (`Error::Conflict`).
- [x] Tests: legacy editor round-trip, first-admin insert happens once, last-admin guard, banned session lookup returns NotFound.

### Task O1.3: Setup flow
- [x] On boot with zero users, generate a token, store it in `AppState.setup_token`, and log it at `warn` with the `/setup` URL.
- [x] Middleware: while a setup token is pending, redirect every route except `/setup`, `/static/*` and `/health` to `/setup` (303). API routes get a 503 JSON response instead.
- [x] `GET /setup` renders a form with token, name, email and password. `POST /setup` compares the token in constant time, creates the admin through `create_first_admin`, clears the token, opens a session, and redirects to `/admin`.
- [x] Remove the first-user-becomes-admin logic from `register_submit` and the OAuth callback.
- [x] Tests: fresh app redirects `/` to `/setup`; a wrong token gets 403; the correct token creates an admin and later visits are not redirected; register after setup yields a member.

### Task O1.4: Admin user management
- [x] `POST /admin/users/{id}/role` (`role=member|moderator|admin`) and `POST /admin/users/{id}/ban` (`banned=1|0`). Both are admin-only, write `mod_log`, and redirect back with a notice.
- [x] An admin cannot ban themself. Last-admin violations render a notice, not a 500.
- [x] Banning deletes the user's sessions.
- [x] `/admin/users` gains a role `<select>`, a ban/unban button, and a "Banned" tag.
- [x] Tests: member gets 403; promote writes `mod_log`; demoting the last admin is refused; a banned user's session stops working.

## O2 — Email sign-up and self-service reset (detailed when O1 lands)

`lettre` SMTP behind a `Mailer` trait (Strategy pattern; tests use an in-memory mailer). Hashed `email_tokens`. `/register` asks for email only. `/register/{token}` completes the account. `/forgot` becomes self-service. `/admin/email` holds SMTP settings with a test send. Without SMTP, sign-up shows "not available yet" unless the admin enables unverified sign-up (default off).

## O3 — Install path and README (detailed when O2 lands)

GitHub Actions workflow that publishes the image to GHCR, `compose.yaml` without `build:`, and a README rewritten around install → setup → read.
