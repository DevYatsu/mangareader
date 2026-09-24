# Bulma Migration — Slice C: Account & Admin Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move settings, admin dashboard, branding, and user-management surfaces onto Bulma/Ink & Ember without changing authentication, authorization, form, database, or branding behavior.

**Architecture:** Keep the existing `base_layout` and route handlers. Replace only legacy account/admin markup with Bulma primitives, retain compatibility values for existing theme fields until Slice E, and keep the current admin accent/background live-preview seam.

**Tech Stack:** Rust, Axum 0.8, Maud 0.27, Bulma 1.0.4, plain CSS, existing `admin.js`.

**Spec:** `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

## Global Constraints

- Preserve all existing auth, role checks, sessions, OAuth, password-reset, and reset-link behavior.
- Preserve every non-theme form `name`, `action`, and `method` exactly.
- Keep `POST /settings/appearance` available during Slice C as a compatibility endpoint; remove it only in Slice E.
- Hide, rather than expose, per-user theme selectors; do not delete `users.theme` or existing `site_theme` values in this slice.
- Do not add source management, homepage builders, moderation, notifications, or other master-spec features.
- Do not touch reader files, content routes, scraper code, or `website/static/style.css`.
- Use Bulma classes and existing approved primitives; do not add an account/admin-specific CSS framework.

## Files

- Modify: `website/src/routes/settings.rs`
- Modify: `website/src/routes/admin.rs`
- Modify: `website/static/admin.js`
- Modify: `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`
- Verification-only: `website/src/routes/password_reset.rs`, `website/src/routes/auth_pages.rs`, `website/src/routes/oauth.rs`, `website/src/main.rs`, `website/src/auth.rs`, `mangaread-core/src/db.rs`

### Task C0: Contract tests

Add focused tests in `settings.rs` and `admin.rs` before markup changes. Assert that settings markup contains Bulma `box`, `field`, `input`, `select`, and `notification`; contains no visible `select[name="theme"]`; preserves settings form actions/methods and password names; admin branding markup contains Bulma form primitives and no visible `site_theme` radio; repeated `home_sections` still parse; and the user table uses `table-container` plus `table is-fullwidth is-hoverable`.

Run:

```bash
cargo test -p website settings::
cargo test -p website admin::
```

Expected: new assertions fail until the markup migration lands.

### Task C1: Migrate settings

Replace `settings-section`, `page-title`, `auth-error`, and `settings-form` markup with `title is-2`, separate `box` blocks, Bulma `field is-horizontal` rows, `label`, `control`, `input`, `select`, `notification is-danger`, and `button`. Keep Profile fields `name` and `locale`, password fields `current` and `new`, and their original actions/methods. Remove the visible theme select while preserving a hidden valid legacy value if needed for compatibility. Keep the appearance route registered but unlinked.

Verify:

```bash
cargo fmt --all -- --check
cargo test -p website settings::
```

### Task C2: Migrate dashboard and branding

In `admin.rs`, replace legacy dashboard classes with Bulma columns/cards/boxes. Keep `/admin/branding` and `/admin/users` links and the unavailable Sources card visibly unavailable. Convert branding fields to Bulma `field`, `label`, `control`, `input`, `textarea`, checkbox, and buttons; remove visible theme radios/current-theme copy; add `id="admin-branding-form"`. Keep validation atomic: parse and validate every field before the first settings write. Preserve repeated `home_sections`, color bounds, and live accent/background values.

Update `admin.js` selectors to the stable branding form ID while retaining `previewAccent` and `previewBg`. Run:

```bash
cargo test -p website admin::
cargo build -p website
```

### Task C3: Migrate users table and audit auth surfaces

Wrap the users table in `div.table-container` and use `table is-fullwidth is-hoverable`. Preserve columns, reset-link form action/method, safe URL validation, expiry, and one-time behavior. Do not introduce a second account shell. Password-reset, login/register, and OAuth error pages are verification-only unless a regression is found.

Run the role matrix against a temporary database: anonymous settings/admin are rejected, member settings is allowed, member admin is forbidden, and admin settings/branding/users are allowed.

### Task C4: C verification

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build -p website
git diff --check
git grep -nE 'admin-section|admin-card|admin-form|theme-picker|theme-option|settings-section|auth-error|has-text-grey|style\.css' -- website/src/routes/settings.rs website/src/routes/admin.rs
```

The final grep must have no live legacy account/admin matches. Verify form contracts, valid color defaults, no partial branding writes, no external font request, and no repository database mutation. Browser screenshots remain required at 375/768/1024/1440px when Chrome is available.

## Deferrals

No new account features, theme builder, source manager, role editor, or deletion of compatibility fields/routes. Slice E removes theme plumbing after C is verified.
