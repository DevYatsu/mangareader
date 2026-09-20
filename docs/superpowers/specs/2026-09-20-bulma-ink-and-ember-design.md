# MangaReader — Bulma Migration & "Ink & Ember" Identity — Design

Date: 2026-09-20
Status: draft for review
Scope: replace all bespoke CSS with Bulma 1.0.4, collapse four themes into one dark-first identity, and restyle every screen including the reader.

## 1. Context & constraints

Current state:
- Web: Axum 0.8 + Maud 0.27 + Tokio + tower-http `ServeDir`. No Node, no Sass, no build step.
- Bespoke CSS: `static/style.css` (2465 lines), `static/reader.css` (1873 lines),
  `static/themes/{asura,demonic,webtoon}.css` (405 lines). Total ≈ 4743 lines.
- Theming: `THEMES` allowlist in `config.rs`, `theme_css()`, `is_theme()`,
  `theme_body_class()`, `resolve_theme()`, plus `css_overrides()` injecting
  `:root{--accent;--accent-hover;--accent-dim;--bg-page;--accent-r/-g/-b}`.
- Four themes: `default`, `webtoon` (light), `asura` (violet), `demonic` (crimson).
- Two accent sources: `.env` (`SITE_COLOR`, `SITE_BG`) as boot defaults, overlaid
  by DB settings `site_accent` / `site_bg` via `SiteConfig::from_settings`.
- Per-user theme: `users.theme` column, editable in `/settings`, validated with `is_theme()`.
- Every HTML route repeats a 4-line preamble:
  `site_config(&db)` → `css_overrides()` → `resolve_theme(&db, user)` → `base_layout(..)`.
- No `CompressionLayer` in the Axum stack; `tower-http` is built with only the `fs` feature.

Constraints kept (from the master design, `2026-09-19-mangareader-customizable-website-design.md`):
- Self-host first. Single Docker image. SQLite default.
- Stay in Rust for core. Vanilla CSS/JS — no framework build step.
- Performance stays fast on Raspberry Pi class hardware for 1-20 users.
- Quality bar: production-tier like the famous Webtoon apps, or better.

Decisions made during brainstorming:
- **Migration depth:** full rewrite. Bulma everywhere, reader included.
- **Theme model:** one dark-first identity. Accent + background stay admin-customizable.
  The asura/demonic/webtoon novelty themes and the per-user theme picker are removed.
- **Identity:** "Ink & Ember" — warm editorial dark, ember accent, restrained surfaces.

## 2. Goals

- Every screen is built from Bulma components plus a small, explicitly-enumerated
  set of custom primitives. No bespoke button, card, form, or table styling survives.
- One coherent visual identity with a real reading-site character: warm near-black
  surfaces, ember accent, editorial type, covers treated as printed volume art.
- Reader 2.0 behavior is preserved exactly. This is a styling change, not a behavior change.
- An admin can still rebrand accent and background without touching code.
- Total CSS shipped to a page drops substantially versus today.

Non-goals:
- No behavior changes. No new runtime features.
- No JS framework, no Sass, no Node, no CDN, no runtime plugin API.
- No light scheme in this pass. (Bulma's `data-theme` makes it additive later.)
- No per-user theme picker. (Replaced by an explicit single-scheme model; see §7.)
- No changes to `name=`, `action=`, or `method=` on any form.

## 3. Architecture — three-layer cascade, zero build step

**Decision:** plain CSS, served by the existing `ServeDir` at `/static`.

| Layer | File | Contents | Est. size |
|---|---|---|---|
| Base | `static/vendor/bulma.min.css` (vendored, v1.0.4) | Untouched upstream build. | ~678 KB |
| Identity | `static/theme.css` | Ink & Ember. Part 1 `--bulma-*` overrides; part 2 `--m-*` tokens; part 3 `m-*` components Bulma lacks. | ~450 lines |
| Reader | `static/reader.css` | Reader-only layout geometry Bulma has no primitive for, on `--m-*` tokens. | ~700 lines |

Load order in every layout: `bulma.min.css` → `theme.css` → (`reader.css`, reader only).

**Why the cascade is robust.** Bulma declares its themes on `:root` and
`[data-theme=…]`, both specificity `(0,1,0)`. This design puts every override on
`:root[data-theme="dark"]`, specificity `(0,2,0)`. Overrides therefore win on
specificity, not on file order. This removes the load-order dependency the current
theme files rely on (their headers document "theme tokens < inline `css_overrides()`"
as a contract that must be preserved by ordering).

**Authority rule:** specificity beats document order. Admin-derived overrides are
delivered inside the `:root[data-theme="dark"]` block, so their precedence over the
static identity tokens is stated by specificity, not by where the `<style>` element
happens to sit.

**Class vocabulary rule.** Use Bulma's own `is-*` / `has-*` modifiers for anything
Bulma styles. Invent `m-*` classes *only* for primitives Bulma does not define, and
record every one in §6. This rule is what prevents two competing styling systems.

**Vendoring, not CDN.** The app must render fully offline on a Pi. Bulma is committed
at `static/vendor/bulma.min.css` with its version recorded in a short `README` beside it.

## 4. Identity specification — Ink & Ember

### 4.1 Tokens

Bulma's palette is generated from hue/saturation/lightness *components*. Override the
components, never the composed color — overriding `--bulma-primary` alone leaves the
whole derived palette (`-00`…`-100`, `-invert`, `-soft`, `-bold`, `-on-scheme`, and
every component-local use) stale.

```css
:root[data-theme="dark"] {
  /* Ember accent: #f59e0b = hsl(38, 92%, 50%) */
  --bulma-primary-h: 38deg;
  --bulma-primary-s: 92%;
  --bulma-primary-l: 50%;
  --bulma-primary-rgb: 245, 158, 11;   /* literal, NOT computed by Bulma */

  /* Warm neutral scheme: greys read as warm ink, not cool blue-grey */
  --bulma-scheme-h: 30deg;
  --bulma-scheme-s: 8%;
  --bulma-scheme-main-l: 7%;           /* deeper than Bulma's 9% */
  --bulma-scheme-main-bis-l: 9%;
  --bulma-scheme-main-ter-l: 11%;
  --bulma-background-l: 13%;
  --bulma-border-l: 22%;
  --bulma-text-l: 72%;
  --bulma-text-strong-l: 94%;
}
```

Surface elevation comes from `scheme-main` (page) → `scheme-main-bis` (raised card) →
`scheme-main-ter` (hover), so hierarchy reads as paper stock, not as borders.

### 4.2 Typography

Keep the two families already loaded — Space Grotesk for display, Inter for body.
Space Grotesk is genuinely editorial and already in the critical path; swapping it
would add a font download for no identity gain.

Mapping onto Bulma's family variables is verified against the vendored stylesheet
during Slice A (the exact variable names are enumerated there, not assumed here).
Posture rules, which are the actual identity decisions:
- Display face for `title`/`subtitle` and the wordmark only.
- Body face for everything else, including all reader chrome.
- Tight, deliberate heading scale; generous whitespace over dividers.
- No uppercase tracking-widening except small metadata labels.

### 4.3 Accent policy (anti-dilution)

The ember accent carries hierarchy. That only works if it stays scarce:
- Accent: primary action, active nav state, focus ring, current selection.
- Never: body text, borders at rest, decorative backgrounds, glows.
- Covers get no glow and no gradient. Restrained radius, fixed `2:3`, no exceptions.

## 5. Screen mapping

| Screen | Bulma components |
|---|---|
| Nav + footer | `navbar`, `navbar-brand`, `navbar-burger`, `navbar-menu`, `navbar-start`, `navbar-end`, `navbar-item`, `footer`, `content` |
| Home | `hero`, `hero-body`, `title`, `subtitle`, `tags` (stats), `button is-primary is-medium`, `media` (recent rows) |
| Series list | `field has-addons` (search), `columns is-multiline` + `column is-*-3`, `card` + `image is-2by3` + `tag` |
| Series detail | `breadcrumb has-arrow-separator`, `media`, `level` (actions), `tabs is-toggle` (source filter), `panel` (chapter list) |
| Bookmarks | as series list |
| Source list | as series list |
| Auth / reset / OAuth | `box` in `columns is-centered`, `field`, `control`, `input`, `button is-fullwidth`, `notification is-danger` (errors), `buttons` (OAuth) |
| Settings | `box` per group, `field is-horizontal`, `select`, `notification is-danger` |
| Admin dashboard | `box` / `card` grid |
| Admin branding | `field`, `input`, `input[type=color]`, `radio`, `checkbox`, `textarea` |
| Admin users | `table is-fullwidth is-hoverable` in `table-container` |
| Legal / error | `content`, `hero is-fullheight`, `title`, `subtitle`, `button` |

Bulma defines no navbar "current page" style, so active nav gets exactly one custom
class (`m-nav-active`). Bulma has no empty-state or list-row primitive, so `m-empty`
and `m-chapter-row` remain, built from tokens.

## 6. Custom primitives (complete list)

Everything here is because Bulma has no equivalent. No item may be added without
being listed here first.

| Class | Why Bulma can't do it | Used by |
|---|---|---|
| `m-nav-active` | Bulma has no navbar current-page state | navbar |
| `m-empty` | No empty-state component | home, series, bookmarks, source |
| `m-chapter-row` | No data-row primitive for a dense chapter list | series detail |
| `m-cover` | Fixed 2:3 cover frame with placeholder fallback | grids, series hero |
| `m-range` | Bulma does not style `input[type=range]` | reader width/gap, zone editor |
| `m-reader-*` | Fullscreen surface, HUD, click zones, dim, offline, next-up, countdown, sheet geometry, note pins | reader only (§7.1) |

Grid, card, hero, stat, and list-row markup are rebuilt using Bulma components, so
the old `manga-grid` / `manga-card` / `recent-list` / `hero-*` / `empty-state`
vocabulary is deleted rather than ported.

## 7. Reader

### 7.1 What Bulma actually contributes

Bulma has **no** fullscreen-surface, bottom-sheet, HUD, click-zone, or fit-mode
primitives. Mapping what it genuinely covers:

- topbar → `navbar` + `button is-ghost`
- sheet tabs and segmented controls → `tabs is-toggle is-fullwidth`
- sliders → `input[type=range]` + `m-range`
- presets → `buttons` of swatch buttons
- notes → `panel`, `textarea`, `button`

Everything else is `m-reader-*` on `--m-*` tokens. `reader.css` therefore shrinks to
layout geometry. **No bespoke button, card, or form rule survives it** — that is the
measurable success criterion for this slice.

### 7.2 Behavior preservation

Reader 2.0 shipped with: modes (single, long-strip, double, RTL, vertical), fit modes,
width/gap sliders, background presets, per-user server-persisted prefs, zone editor,
key remapping, notes, auto-dim, focus mode, next-up queue, auto-next countdown,
offline banner, resume badge, and reduced-motion handling.

Slice D must verify this checklist, not assume it:

- [ ] All five layout modes render correctly.
- [ ] Fit modes and width/gap sliders behave as before.
- [ ] Background presets apply and persist.
- [ ] Prefs round-trip to the server and survive reload.
- [ ] Click zones fire at the configured edges and are configurable.
- [ ] Key remapping and keyboard navigation work.
- [ ] Notes: create, list, jump-to-page, pin, delete.
- [ ] Auto-dim, focus mode, resume badge, next-up, countdown.
- [ ] Offline banner and image-failure retry.
- [ ] Reduced-motion and no-smooth-scroll honored.
- [ ] `reader.js` selectors updated in lockstep with the markup.

## 8. Theming model change

Removed:
- `config::THEMES`, `theme_css()`, `is_theme()`, `templates::layout::theme_body_class()`.
- `static/themes/asura.css`, `demonic.css`, `webtoon.css`.
- The per-user theme `<select>` in `/settings` and the user-theme branch in `resolve_theme()`.
- The admin theme picker.

Added / changed:
- `resolve_theme()` is replaced by `resolve_scheme() -> &'static str`, which returns
  `"dark"` unconditionally for now. This is a rename, so every call site changes name
  but keeps the same shape. Slices A–C may keep the old name temporarily to avoid a
  repo-wide sweep mid-migration; Slice E finishes the rename.
- Every layout emits `<html lang="en" data-theme="dark">` server-side. Dark-first,
  no flash of wrong theme, and no client JS needed to pick a scheme.
- `css_overrides()` becomes `theme_overrides()`, emitting admin-derived overrides
  inside the `:root[data-theme="dark"]` block: `--bulma-primary-h/s/l`,
  `--bulma-primary-rgb`, and `--bulma-scheme-main-l`.
- This requires hex→HSL conversion in `config.rs` (today only RGB parsing exists),
  plus manual `--bulma-primary-rgb` because Bulma stores that as a literal.

**Data compatibility.** No schema migration and no data loss. Existing `users.theme`
and the `site_theme` setting are reinterpreted as scheme keys; legacy values
(`default`, `asura`, `demonic`, `webtoon`) normalize to `"dark"` on read, so existing
rows keep working and simply resolve to the single identity. The `users.theme` column
is retained (unused for now) so a future light-scheme slice does not need a migration.

Admin keeps: site title, accent color, background color.

## 9. Risks

| Risk | Mitigation |
|---|---|
| Template rewrite touches every route — form regressions | Never touch `name=`/`action=`/`method=`. Only class attributes. `cargo test` per slice + a manual route sweep. |
| Ember accent on near-black fails contrast | Validate WCAG AA (4.5:1 body text, 3:1 UI/large text) before proceeding past Slice A. Adjust `--bulma-primary-l`, not the hue. |
| Reader is the highest-risk surface | Its own slice, gated on the §7.2 checklist. `reader.js` selector renames land in the same commit as the markup. |
| Vendored Bulma (~678 KB) hits the Pi budget | `CompressionLayer` is **not currently in the stack** and `tower-http` only enables `fs`. Slice A adds the `compression-gzip` (or `compression-br`) feature and layers it. Verify transfer size after. |
| Bulma derived variables going stale | Override `-h/-s/-l` components only; set `-rgb` literals explicitly. |
| JS/markup class drift | `app.js`, `auth.js`, `reader.js`, `admin.js` hold class references. Renames are enumerated in the implementation plan and done per slice. |
| Two systems lingering | §6 caps custom classes explicitly. Slice E deletes legacy CSS and fails if any removed class is still referenced. |

## 10. Rollout

Five slices, following the repo's existing slice convention. Each slice is
independently reviewable, and the app is fully usable at the end of each.

- **A — Foundation.** Vendor Bulma v1.0.4; add `theme.css` with the token layer;
  rebuild `base_layout` (navbar, footer), `error_layout`, and the auth pages;
  add `CompressionLayer`; verify contrast and transfer size. Establishes the identity.
- **B — Content.** Home, series list, series detail, bookmarks, source, legal.
- **C — Account & admin.** Settings, admin dashboard/branding/users, password reset.
- **D — Reader.** Full restyle on Bulma components + `--m-*` tokens; §7.2 checklist.
- **E — Cleanup & verification.** Delete legacy CSS and dead theme code; rename JS
  class references; contrast, page-weight, and perf pass; screenshot review against
  `docs/website/{home,series,series-details,chapter}.jpg`.

**Verification.** `cargo test` per slice; manual route sweep per slice; the reader
checklist in Slice D; contrast and page-weight measurement in Slice E. There are no
automated visual tests in this repo, and this design does not add any.
