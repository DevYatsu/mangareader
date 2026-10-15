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
| Home | `hero`, `hero-body`, `title`, `subtitle is-4 m-text-muted`, `level` + `tag` (stats), `button is-primary is-medium`, `media` (recent rows) |
| Series list | `box`, `level`, `field has-addons` (search), `columns is-mobile is-multiline` + exact 2/3/4 columns, `card` + `image is-2by3 m-cover` + `tag` |
| Series detail | shared `breadcrumb has-arrow-separator`, `media`, `m-cover`, `level` (actions), independent `checkbox.m-source-toggle` filters, `loader`, `ul > li.m-chapter-row` |
| Bookmarks | as series list; server shell plus client-rendered shared cards |
| Source list | as series list |
| Auth / reset / OAuth | `box` in `columns is-centered`, `field`, `control`, `input`, `button is-fullwidth`, `notification is-danger` (errors), `buttons` (OAuth) |
| Settings | `box` per group, `field is-horizontal`, `select`, `notification is-danger` |
| Admin dashboard | `box` / `card` grid |
| Admin branding | `field`, `input`, `input[type=color]`, `radio`, `checkbox`, `textarea` |
| Admin users | `table is-fullwidth is-hoverable` in `table-container` |
| Legal | `columns is-centered`, `column is-8-desktop is-10-tablet`, `content`, `m-legal-body` |
| Error | existing `error_content` path and error primitives remain separate from legal `m-legal-body` |

Bulma defines no navbar "current page" style, so active nav gets exactly one custom
class (`m-nav-active`). Bulma has no empty-state or list-row primitive, so `m-empty`,
`m-empty-icon`, and `m-chapter-row` remain, built from tokens.

### 5.1 Slice B content rulings

These decisions are binding for the public content routes. They preserve the existing
routes, forms, links, data hooks, APIs, localStorage keys, and reader behavior.

- **Shared cards and ownership.** Rust and `app.js` render the same manga card
  structure: `columns is-mobile is-multiline`, `column is-6-mobile is-4-tablet
  is-3-desktop`, `card`, `card-image`, and `figure.image.is-2by3.m-cover`. The
  server card helper and browser builders share the same cover, title-link, badge,
  and empty-state vocabulary. `m-empty`, `m-empty-icon`, and `m-cover` are used by
  both Rust and `app.js`. Bookmarks are client-rendered through the shared JS card
  builder; the server does not add bookmark queries.
- **Covers.** `m-cover` owns the fixed `2:3` frame, placeholder and initial states,
  badge, and overlay. Its visual state uses only `data-cover-placeholder`,
  `data-cover-initial`, `data-cover-badge`, and `data-cover-overlay`; the bookmark
  button/label hooks remain `data-bookmark-button` and `data-bookmark-label`.
- **Source filters.** Source selection uses independent checkboxes inside
  `#source-toggles`: each filter is a `p.control` containing
  `label.checkbox.m-source-toggle` and its input. It does not use `tabs is-toggle`.
  `#source-reset`, the `mangaread_sources` localStorage key, source values,
  independent filtering, and reset behavior are preserved.
- **Chapter rows.** Chapter navigation is `ul > li.m-chapter-row`. The row retains
  the `ch-status-icon`, `chapter-num`, `chapter-source`, `ch-resume`, and
  `chapter-arrow` child hooks, plus the existing `data-series-id`, `data-sources`,
  `data-source`, and `data-chapter-id` hooks. The `ch-unread`, `ch-started`,
  `ch-finished`, and `ch-resume` state hooks, read URLs, and inline filtering
  contract are preserved. `data-pages` remains absent; a started chapter remains
  `Resume p.N`, and a page-total resume suffix remains deferred.
- **Headers, breadcrumbs, and fallbacks.** Series, source, and bookmarks list
  headers use a Bulma `level`. Detail and source routes use the shared breadcrumb
  helper and `breadcrumb has-arrow-separator`. Existing `/series/{id}` links and
  `/manga/{id}` fallback links remain unchanged.
- **Home and loading.** Home recent chapters use `ul#recent-chapters` with
  `media` rows, `media-content`, `media-right`, and the existing relative-time
  text. Home stats use `level` and `tag`; continue reading remains a client-rendered
  scroll strip. Series loading uses `.loader`, with `.m-text-muted` for its hint.
  `m-text-muted` also covers home time text and source counts.
- **Search contract.** The `/series` form remains server-rendered with the same
  action, method, `name="q"`, reflected value, `q.trim()` filtering,
  zero-chapter filtering, and no-JavaScript behavior. No client filter, pagination,
  or query parameter is added.
- **Legal content.** Legal pages use `m-legal-body` inside a centered Bulma
  content column. It is separate from the existing `error_content` path; legal
  plain text remains escaped and preserves newlines.
- **Responsive grid.** Every shared content grid uses the exact
  `columns is-mobile is-multiline` plus `is-6-mobile is-4-tablet is-3-desktop`
  column modifiers: two columns on mobile, three on tablet, and four on desktop.
- **Surface treatment.** The transparent shadow-token decision is
  `--bulma-shadow-h: 0deg`, `--bulma-shadow-s: 0%`, and `--bulma-shadow-l: 0%`.
  Cards and boxes use the scheme's raised surface rather than a glow.

## 6. Custom primitives (complete list)

Everything here is because Bulma has no equivalent. No item may be added without
being listed here first.

| Class | Why Bulma can't do it | Used by |
|---|---|---|
| `m-nav-active` | Bulma has no navbar current-page state | navbar |
| `m-empty` | No empty-state component | home, series, bookmarks, source |
| `m-empty-icon` | No muted icon slot inside an empty state | empty states |
| `m-chapter-row` | No data-row primitive for a dense chapter list | series detail |
| ~~`m-source-toggle`~~ | Removed 2026-10-14 (Slice D): readers no longer see sources | — |
| `m-comment-quote` | Bulma styles `blockquote` only inside `.content` | moderation queue (added 2026-10-14, Slice C) |
| `m-shelf-picker` | Spacing for a form row under `.buttons` | series detail (added 2026-10-14, Slice D) |
| `m-chapter-feed` | Unstyled list of `.media` chapter rows (replaces the `#recent-chapters` id rule) | home (Slice D) |
| `m-home-intro`, `m-home-section` | Vertical rhythm between home sections; Bulma `section` padding is too large for stacked feeds | home (Slice D) |
| `m-cover` | Fixed 2:3 cover frame with placeholder/initial, badge, and overlay states | grids, series hero |
| `m-legal-body` | No legal plain-text body primitive that preserves wrapped source newlines | legal pages |
| `m-range` | Bulma does not style `input[type=range]` | reader width/gap, zone editor |
| `m-reader-*` | Fullscreen surface, HUD, click zones, dim, offline, next-up, countdown, sheet geometry, note pins | reader only (§7.1) |
| `m-display` | Bulma emits no font-family for `.title` (`--bulma-title-family` is false), so the display face needs an explicit hook | display headings |
| `m-wordmark` | No site-chrome wordmark component | navbar brand |
| `m-footer` | No footer-spacing primitive | footer |
| `m-text-muted` | Bulma's `has-text-grey*` utilities emit hardcoded cool hex: `--bulma-grey*` is a literal **no rule consumes**, so the tokens are dead. `has-text-grey` measures 3.99:1 on `scheme-main` and 3.81:1 on `scheme-main-bis`, below WCAG AA 4.5:1. `m-text-muted` binds `var(--bulma-text-weak)`, the token the `config.rs` contrast guard models (5.02:1 on `--bulma-background-l`) | footer, auth link blocks, home time, loading hints, source counts |

The four Slice-A additions `m-display`, `m-wordmark`, `m-footer`, and `m-empty-icon`
exist in `theme.css` but were absent from the original list above; they are recorded
here retroactively so the closed-class rule stays true.

**Bulma's `has-text-grey*` utilities are hardcoded hex and must not be used on
identity surfaces.** `--bulma-grey*` tokens are dead (defined, never consumed), so
repointing the token does not move these utilities. On warm Ink & Ember surfaces they
fail WCAG AA. Use `m-text-muted` instead. Do not substitute `has-text-text-*`: those
resolve through Bulma literals (`--bulma-text-NN-l`) that `theme.css` does not bind,
so the `config.rs` contrast guard does not model them.

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

Slice D keeps the stored `ReaderPrefs` shape and all of those runtime paths, but makes the public surface opinionated:

- **Flow** is the default `strip` preset; **Page** covers `single` and the storage-compatible `vertical`; **Spread** is `double` and is unavailable below `768px`.
- The visible sheet contains only the three presets, reading order, tone, focus mode, Add note, per-series memory, and restore recommended settings.
- Stored advanced values (`fit`, width/gap, frame, zones, keybinds, swipe, auto-next, dim, and note visibility) remain normalized, persisted, and active; their editors are intentionally absent.
- Existing `bg=custom` remains a selected, non-editable Custom tone. Restore does not discard hidden legacy values or a custom background.
- `L` and the legacy `W` binding cycle the same public preset IDs; neither exposes a hidden fit/layout state.
- `reader.css` owns all reader geometry, including topbar, bottom navigation, hint, skeleton, page bases, zones, and overlays; it does not depend on `style.css` or transitional accent/font aliases.
- Bulma classes are additive: existing IDs, `data-r-*`, `data-pref`, and public JavaScript hooks remain stable.

The contract tests cover preset mapping, first-paint attributes, preference equality/normalization, notes/progress response fields, and the server progress seam. Hydration is read-only: the reader consumes the existing `/api/progress` response and does not create a write loop.

## 8. Theming model change

Removed:
- `config::THEMES`, `theme_css()`, `is_theme()`, `templates::layout::theme_body_class()`.
- `static/themes/asura.css`, `demonic.css`, `webtoon.css`.
- The per-user theme `<select>` in `/settings` and the user-theme branch in `resolve_theme()`.
- The admin theme picker.

Added / changed:
- Runtime per-page theme resolution is removed rather than renamed. Every layout emits
  `<html lang="en" data-theme="dark">` as a constant, so there is no consumer for a
  `resolve_theme()`/`resolve_scheme()` indirection and no client-side scheme detection.
- `css_overrides()` becomes `theme_overrides()`, emitting admin-derived overrides
  inside the `:root[data-theme="dark"]` block: `--bulma-primary-h/s/l`,
  `--bulma-primary-rgb`, and the Bulma surface-lightness components.
- This requires hex→HSL conversion in `config.rs` (today only RGB parsing exists),
  plus manual `--bulma-primary-rgb` because Bulma stores that as a literal.

**Data compatibility.** No schema migration and no data loss. Existing `users.theme`
values and `site_theme` rows are retained but unused, so legacy data remains intact
without participating in runtime resolution. The `users.theme` column migration stays
in place for a future scheme slice that does not need a migration.

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
| Two systems lingering | §6 caps custom classes explicitly. Cleanup removes the unlinked legacy assets and fails if a retired selector or token remains referenced. |

## 10. Rollout

Five slices, following the repo's existing slice convention. Each slice is
independently reviewable, and the app is fully usable at the end of each.

- **A — Foundation.** Vendor Bulma v1.0.4; add `theme.css` with the token layer;
  rebuild `base_layout` (navbar, footer), `error_layout`, and the auth pages;
  add `CompressionLayer`; verify contrast and transfer size. Establishes the identity.
- **B — Content.** Home, series list, series detail, bookmarks, source, legal.
- **C — Account & admin.** Settings, admin dashboard/branding/users, password reset.
- **D — Reader.** Full restyle on Bulma components + `--m-*` tokens; §7.2 checklist.
- **E — Cleanup & verification.** Delete the unlinked legacy CSS and theme assets,
  audit retired selectors/tokens, then run contrast, page-weight, performance, and
  screenshot review against `docs/website/{home,series,series-details,chapter}.jpg`.

**Verification.** `cargo test` per slice; manual route sweep per slice; the reader
checklist in Slice D; contrast and page-weight measurement in Slice E. There are no
automated visual tests in this repo, and this design does not add any.
