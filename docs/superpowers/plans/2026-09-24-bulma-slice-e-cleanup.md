# Bulma Migration — Slice E: Legacy Cleanup & Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the unlinked legacy stylesheet/theme system after account/admin and reader migrations, while preserving legacy database values and all current behavior.

**Architecture:** The final live cascade is vendored Bulma → `theme.css` → reader-only `reader.css`. `data-theme="dark"` is constant; `theme_overrides()` emits only Bulma variables. Runtime theme code and files are removed, but SQLite `users.theme` and existing `site_theme` rows remain untouched for compatibility.

**Tech Stack:** Rust, Maud 0.27, Axum 0.8, Bulma 1.0.4, plain CSS.

**Spec:** `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

## Global Constraints

- Never delete or rewrite `users.theme` column data or existing `site_theme` rows.
- Do not reintroduce light mode, theme pickers, `style.css`, or runtime theme resolution.
- Preserve account/admin form/auth behavior and reader/content behavior from prior slices.
- Do not change scraper behavior or unrelated dirty files.
- Do not delete nested Cargo lockfiles without a separate decision.

## Files

Delete `website/static/style.css`, `website/static/themes/asura.css`, `website/static/themes/demonicscans.css` if present, and `website/static/themes/webtoon.css`; remove empty theme directory. Modify `website/src/config.rs`, `website/src/auth.rs`, all `css_overrides()` callers, `website/src/templates/layout.rs`, `website/static/theme.css`, `website/static/reader.css`, `website/static/app.js`, `website/static/reader.js`, `website/static/admin.js`, `README.md`, `docs/architecture/README.md`, and both current design specs.

### Task E0: Add cleanup guards

Test `resolve_scheme() == "dark"`, exact `theme_overrides()` output, absence of legacy aliases, preserved database column migration, and current auth/admin behavior before deletion. Run config, auth, website, and core tests.

### Task E1: Remove theme plumbing

Rename `css_overrides()` to `theme_overrides()` and update every caller. Add/use `resolve_scheme() -> "dark"` in layout. Remove `THEMES`, `theme_css`, `is_theme`, `AuthUser.theme`, runtime `User.theme` mapping if compatible with all queries, profile theme writes, hidden theme fields, and the obsolete appearance route after confirming C’s compatibility bridge is no longer needed. Keep database column migration and existing rows. Update layout/comments without reintroducing per-user resolution.

Verify:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo build --workspace
```

### Task E2: Delete assets and dead rules

Delete only after E1 passes:

```bash
rm website/static/style.css
rm website/static/themes/asura.css
rm website/static/themes/demonicscans.css 2>/dev/null || true
rm website/static/themes/webtoon.css
```

Audit before removing each selector; do not blind-delete. Remove stale theme comments/selectors from reader/theme/app/admin JS. Keep vendored Bulma, theme.css, reader.css, fonts, and local provenance files.

### Task E3: Static legacy audit

```bash
git grep -nE 'style\.css|/static/themes|THEMES|theme_css|is_theme|resolve_theme|theme_body_class|--accent|--font-display|--font-body' -- website/src website/static
```

Expected: no live source/static references. Historical plans may retain historical references. Also audit content legacy selectors and reader-facing source terms.

### Task E4: Final route, contrast, and performance verification

Run full formatting/tests/build, route sweep for public/protected/reader routes, live cascade checks, no external font requests, contrast tests, compression measurements for Bulma/theme/reader, and 375/768/1024/1440 screenshots when Chrome exists. Verify admin branding, protected-route status matrix, reader presets, content cards, legal text, and reduced motion.

### Task E5: Repository audit

Run `git diff --check`, `git status --short`, and `git ls-files website/static/themes`. Confirm no temporary database/browser artifacts, no unrelated scraper changes, and no accidental deletion of compatibility data.

## Deferrals

No nested lockfile cleanup, unrelated warning cleanup, source-manager work, new account features, light mode, or unrelated core/scraper refactors.
