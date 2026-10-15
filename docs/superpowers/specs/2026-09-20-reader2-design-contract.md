# Reader 2.0 — UI/UX Design Contract

Date: 2026-09-20
Status: contract for engineering plan
Scope: reader surface only (`/read/{manga}/{chapter}`, `website/src/routes/reader.rs`, `website/static/reader.js`, `website/static/reader.css`).

This document is the interface between the CSS author and the JS author. Every class, id, custom property, and data attribute named here is binding. Where existing markup is insufficient, the contract says so explicitly and marks the required template change.

Ground truth this contract builds on (verified):
- `reader.rs` renders the reader DOM listed in §2.1. `layout.rs` gives `<body class="reader-page">`, with `data-user-id` when logged in.
- `reader.css` owns the standalone reader geometry; `theme.css` supplies the shared identity tokens.
- `reader.js` owns the reader page module; `app.js` supplies `MangaReadState` and the single progress write-through.
- Progress is 1-indexed page, `localStorage['mangaread_read_<seriesId>']['ch_<chapterId>'] = {status, page, updated}`, write-through via `MangaSync.pushProgress`.
- `notes(id, user_id, chapter_id, page, body, created_at)` exists and the reader keeps note pages 1-indexed.

### Slice D contract gate

The stored flat `ReaderPrefs` blob remains authoritative for advanced fields and server/local persistence. The public surface is the curated Flow/Page/Spread set; hidden editors are not part of the reader contract. `vertical` remains a storage-compatible Page layout, and a stored Spread is rendered as Page below `768px` without rewriting storage. The existing `/api/progress` response is the only server progress hydration seam; hydration is read-only and must not create a write loop. The current sibling next-up row and HUD hooks are valid, including the compact `Add note` action.


**Ruling:** the five spec "modes" are not orthogonal. They resolve to `direction` × `layout`; RTL is not a layout, and "vertical scroll" is `layout=strip` on the vertical axis.

| Axis | Values | Notes |
|---|---|---|
| `layout` | `single` \| `double` \| `strip` \| `vertical` | `strip` = continuous vertical long strip. `vertical` = one page at a time, paged **vertically** (manga-page-by-page with vertical advance). |
| `direction` | `ltr` \| `rtl` | Flips reading order of pages within a spread and the meaning of prev/next. Independent of layout. |

`layout` and `direction` are both stored. `strip` + `rtl` is legal (stacked, tap zones mirrored). `double` + `rtl` is the standard manga case: page 1 on the **right**, page 2 on the left.

### 1.1 Per-layout behavior

| | Arrangement | Advance | "Current page" | End of chapter |
|---|---|---|---|---|
| `single` | One `.reader-page` centered in viewport (`--r-fit-h`). | Page-step. Tap zones / keyboard / swipe move exactly 1 page. | `[data-page-idx]` of the centered page, 1-indexed. | Nav overlay after last page (§5.4). Auto-next if enabled. |
| `double` | Two `.reader-page` side by side. Order **within a row** is `direction`-dependent: LTR = [n, n+1], RTL = [n+1, n]. A cover (first page when `--r-double-cover` set) renders alone, centered, and shifts pairing parity by 1. | Page-step by 2 (spread). | First page index of the visible spread. | Nav overlay. |
| `strip` | Vertical stack, `--r-gap` between pages, `--r-max-w` clamp. | Scroll. Tap zones hidden (`display:none`); prev/next chapter are links only. | Highest `.reader-page` whose box has ≥50% of viewport height visible (topmost such page). | Sticky bottom nav (§5.4) + chapter nav row. |
| `vertical` | One page at a time, stacked in a `overflow-y: auto` scroll-snap container. | Scroll-snap step; wheel/tap/keyboard snap one page. | Page snapped into view. | Nav overlay after last page. |

**Ruling on `vertical` vs `single`:** `vertical` exists to satisfy the spec's literal wording, but it is a thin variant of `single`. Cost if wrong: one extra CSS block. It is not worth a separate JS code path — both use the page-step navigator; `vertical` only changes the scroll container and snap.

### 1.2 Navigation invariants (all layouts)

- Prev page at page 1 → previous chapter, **at its last page**, only when `data-auto-prev="1"`. Otherwise prev is a no-op with a nudge animation.
- Next page at page N → next chapter, page 1. Always available via explicit nav button; automatic only when auto-next is on.
- Chapter changes never mutate history: use `location.assign`, not `pushState` (server-rendered, no client router).
- `direction=rtl` in `single`/`vertical`: the *left* zone advances, the *right* zone goes back. Zone semantics are derived from direction, never hardcoded to side.

---

## 2. Fit system

### 2.1 Custom properties

All reader variables are declared on `:root` **with fallbacks**, and consumed with `var(--r-x, <fallback>)`. Reader CSS is self-contained: no retired theme assets are required.

| Property | Type | Default | Semantics |
|---|---|---|---|
| `--r-max-w` | length | `700px` | Max content width of the page column. The 400–1400px slider writes this. |
| `--r-gap` | length | `0px` | Vertical gap between pages in `strip`. `0` = seamless webtoon seam. |
| `--r-zoom` | number | `1` | Scale multiplier applied to fit modes only (see precedence). |
| `--r-bg` | color | `#0a0a0a` | Reader backdrop. |
| `--r-chrome-bg` | color | `rgba(10,10,10,0.92)` | Top bar / sheet background. |
| `--r-chrome-fg` | color | `#ffffff` | Chrome text/icons. |
| `--r-chrome-border` | color | `rgba(255,255,255,0.06)` | Chrome hairline. |
| `--r-page-shadow` | shadow | `none` | Page border/shadow; `none` when the toggle is off. |
| `--r-radius` | length | `0px` | Page corner radius; `0` in strip for seamless joins. |
| `--r-dim` | number | `0` | Idle auto-dim opacity target, 0–0.8 (§5.2). |
| `--r-controls-offset` | length | `0px` | Pushes sticky chrome above the safe area / bottom nav. |

### 2.2 Presets (what the UI writes)

| Control | DOM effect |
|---|---|
| Width slider 400–1400 | `--r-max-w: {n}px`; clears `data-r-fit`. |
| Fit height | `<body data-r-fit="height">` → `.reader-page img { max-height: 100dvh; width: auto; }` |
| Fit screen | `<body data-r-fit="screen">` → page sized to `100dvw × 100dvh`, `object-fit: contain`. |
| Gap slider 0–64px | `--r-gap: {n}px` |
| BG preset black/white/sepia/custom | `data-r-bg="black\|white\|sepia\|custom"`; sets `--r-bg`. `custom` reads `--r-bg-custom` (user color). |
| Border/shadow toggle | `data-r-frame="on"` → `--r-page-shadow: 0 0 0 1px var(--r-chrome-border), 0 8px 32px rgba(0,0,0,.5)`. |

**Precedence ruling:** fit modes win over width. `[data-r-fit]` sets `--r-max-w: 100%` and ignores the slider value while active. The slider is *disabled in the UI* (visually + `aria-disabled`) when a fit mode is on, so the two can never fight. Cost if wrong: users who set width then enable fit lose their width on toggle-off; we therefore **do not clear** the stored width, it reapplies.

`--r-bg="white"`/`"sepia"` must also flip `--r-chrome-fg` and `--r-chrome-bg`, otherwise the fixed top bar (white text) vanishes on white backgrounds. Ruling: light presets get `--r-chrome-bg: rgba(255,255,255,0.92)`, `--r-chrome-fg: #111`.

### 2.3 Device-local vs server-synced

| Synced (server prefs blob) | Device-local (localStorage) |
|---|---|
| `layout`, `direction`, `fit`, `maxW`, `gap`, `bg`, `bgCustom`, `frame`, `doubleCover`, `autoNext`, `autoPrev`, `hideDelay`, `dimDelay`, `dimOpacity`, `clickZones`, `keybinds` | `resumeBadgeSeen`, `hintSeen`, `scrollPositions` (per chapter), `lastUsedLayout` (pre-signup anonymous), `zoom` |

Rationale: layout/comfort preferences are identity-level and cheap to sync; scroll position and one-shot hints are device-specific and produce confusing cross-device jumps if synced. `zoom` is device-local because it compensates for physical screen size.

Per-series override: see §3.4.

---

## 3. Controls surface

### 3.1 Structure

Two surfaces, one shared component:
- **Top bar** — always present, hidden/shown by transform through `.reader-topbar.visible`.
- **Bottom sheet** — settings. On phones it is a bottom sheet; at `≥768px` it becomes a right-side rail (`data-r-sheet="rail"`). Same DOM, CSS decides placement. This avoids two markup trees.

Opening: tapping the **center zone** (§4.1), or `KeyS`/`?`. Closing: `Escape`, backdrop tap, or the sheet's close button.

### 3.2 Current markup contract

The reader keeps the existing IDs, `data-r-*` attributes, `data-pref` bindings, page-state attributes, navigation hooks, notes hooks, and next-up sibling structure. The current public markup is:

```
body[data-r-layout][data-r-preset][data-r-direction][data-r-fit][data-r-bg][data-r-frame]
  div#reader-track[data-series-id][data-chapter-id][data-manga-id][data-total-pages]
  header#reader-topbar.navbar > .reader-topbar-inner
  div#reader-zones[data-r-zone-mode] > button.reader-zone[data-zone]
  main#reader-images > section.reader-page[data-page-idx][data-page-state]
  div#reader-sheet.reader-sheet > .reader-sheet-panel[role=dialog]
    .reader-preset-btn[data-r-preset=flow|page|spread]
    .reader-seg-btn[data-pref=direction|bg]
    #reader-focus-toggle[data-focus-toggle]
    button[data-action=note.add]
    #reader-remember-series
    button[data-action=ui.restoreDefaults]
  div#reader-note-composer > #reader-note-form
  aside#reader-notes-panel > .reader-note-item[data-note-id][data-page-idx]
```

Advanced editor markup is not rendered. Stored values remain part of the JSON island and the public JavaScript preference object.

### 3.3 Grouping and phone usability

The curated sheet is one responsive group: preset cards stack into an auto-fit grid, secondary choices stack on narrow screens, and the sheet becomes a rail at `≥768px`. All interactive controls keep a 44px minimum target, focus trapping, and visible keyboard focus. There are no public tabs, sliders, zone editor, key remap editor, or color picker.

### 3.3.1 Curated reader surface (current product ruling)

The reader exposes three opinionated reading styles instead of a dense matrix of independent low-level controls:

| Preset | Layout | Behavior |
|---|---|---|
| `flow` (default) | `strip` | Long, seamless vertical scroll; tap zones are hidden. |
| `page` | `single` | One page at a time. `vertical` is treated as the same public preset. |
| `spread` | `double` | Book-like pairs; unavailable below `768px`. |

The visible sheet retains only reading order, page tone, focus mode, add note, per-series memory, and restore recommended settings. The existing preference blob continues to store advanced fields (`keybinds`, `clickZones`, `swipeThreshold`, auto-next, dim, notes, and fit values) so existing behavior and stored data are not lost, but those fields are intentionally hidden from the public settings surface. `L` and the legacy fit shortcut cycle the same three public preset IDs; neither introduces a hidden fourth layout or fit state.

Custom backgrounds remain compatible: when `bg=custom` is already stored, the sheet shows a selected, non-editable **Custom** tone. The stored `bgCustom` value is never replaced by a color picker. Restore recommended settings resets only the curated preset, tone, and reading-order fields; it preserves hidden legacy preferences and preserves a custom tone.

On screens narrower than `768px`, a stored Spread is rendered as Page without rewriting the stored preference. Resizing back to a wide viewport restores the saved Spread. The navigation hint is layout-aware: Flow says “Scroll to read”; Page and Spread say “Tap sides to navigate pages.” Notes remain discoverable through the sheet’s compact **Add note** action, in addition to the long-press gesture.

### 3.4 "Remember for this series"

`input#reader-remember-series` at the **top** of the sheet, above the tabs, so it applies to whatever follows.

- Checked → every `data-pref` change also writes to `localStorage['mangaread_series_prefs_<seriesId>']` (a direct override, not a union).
- On load, series prefs overlay user prefs: `effective = {...userPrefs, ...seriesPrefsOverride}`.
- Unchecked → the series key is deleted.
- When checked, the sheet header shows `Remembering for <series title>` (`.reader-series-remember-note [data-series-title]`).
- Ruling: an override is a **full replacement** of the prefs object for that series, not a diff. Cost if wrong: a user who sets a width for one series then changes their global bg sees the old bg in that series. We accept this; diffs are harder to explain.
- Anonymous users get the same affordance; it writes to localStorage and merges on signup like other local data.

---

## 4. Input

### 4.1 Click zones (default)

| Zone | `data-zone` | Default width | Action (LTR) | Action (RTL) |
|---|---|---|---|---|
| Left | `prev` | 30% | prev | next |
| Center | `menu` | 40% | toggle `#reader-sheet` | same |
| Right | `next` | 30% | next | prev |

- Zones are `<button>` (focusable, real semantics) replacing the current `<div id="reader-click-left/right">` (`reader.rs:126-127`). Previous/next get `aria-label="Previous page"` / `"Next page"`.
- Zones are `pointer-events: none` while `#reader-sheet` is open and never overlay chrome (`z-index` between pages and chrome).
- Default zone widths are the *only* defaults; on `strip` layout zones are hidden entirely, and the center zone is replaced by a `keydown`/long-press menu trigger on the HUD (§5).

### 4.2 Tap zones

The stored `clickZones` array remains normalized and drives the three real buttons at runtime. Its advanced editor is not rendered in the curated sheet; keyboard/touch navigation and RTL side mapping remain unchanged.

### 4.3 Keyboard + compatibility bindings

Default bindings (namespaced action ids, not raw keys):

| Action id | Default | Meaning |
|---|---|---|
| `page.next` | `ArrowRight`, `ArrowDown`, `Space`, `PageDown` | Next page (mirrors with direction). |
| `page.prev` | `ArrowLeft`, `ArrowUp`, `PageUp` | Previous page. |
| `chapter.next` | `Shift+ArrowRight`, `KeyN` | Next chapter. |
| `chapter.prev` | `Shift+ArrowLeft`, `KeyP` | Previous chapter. |
| `ui.toggle` | `KeyS`, `Escape` | Toggle sheet. `Escape` also closes. |
| `ui.focus` | `KeyF` | Focus mode. |
| `ui.fitCycle` | `KeyW` | Compatibility shortcut: cycles the public preset IDs; it does not expose a hidden fit mode. |
| `layout.cycle` | `KeyL` | Cycle layout. |
| `note.add` | `KeyM` | Open note composer on the current page. |

- Storage shape: `keybinds: { "page.next": ["ArrowRight","Space"], ... }`. Keys are `KeyboardEvent.code` values; modifiers are a `+`-joined prefix in a stable order (`Ctrl`, `Alt`, `Shift`, `Meta`). Stored bindings remain active even though the advanced remap editor is no longer rendered.
- Handlers must `preventDefault` on matched actions (otherwise `Space` scrolls) **and** must not fire when `event.target` is an `input`, `textarea`, or `[contenteditable]`.
- `KeyL` and the legacy `KeyW` binding both cycle Flow → Page → Spread. On phones, the cycle skips Spread.

### 4.4 Swipe and auto-next

- Swipe: horizontal delta `> swipeThreshold` advances one page-step; vertical does nothing in `strip`/`vertical` (scroll owns it). `swipeThreshold` is a 40–160px slider (`data-pref="swipeThreshold"`, default 70). Left-to-right = previous in LTR.
- Ruling: swipe does **not** use velocity, only distance. Velocity gestures fight the browser's back-swipe on iOS. Cost if wrong: deliberate slow swipes are the only way; acceptable.
- Auto-next: `data-auto-next="1"` on `#reader-track` (reflects pref). When on, reaching the end of the chapter (last page fully visible for `1500ms` in `strip`, or on the last page's next-action at the boundary) triggers `location.assign(nextChapterUrl)` after a **3s countdown** with a cancel button (`#reader-countdown`, `role="status"`, `aria-live="polite"`). Cancel is sticky for the chapter (sets a session flag) so we don't re-nag.
- Auto-next requires `data-next-chapter`; absent → never fires, and the last-page nav overlay shows "Last chapter".

---

## 5. Comfort and chrome

### 5.1 Hide-UI timer

`hideDelay` (slider 1–10s, default **3s**, `0` = never). On scroll/tap/keypress, `#reader-topbar` and `#reader-hud` get `.visible`; a timer removes it. On any user input the timer resets. `hideDelay = 0` keeps chrome permanently visible (accessibility escape hatch).

### 5.2 Auto-dim

`dimDelay` (0 = off, default **0/off**; options 15s/30s/60s) and `dimOpacity` (0.2–0.8, default **0.5**). After idle, `body[data-r-dim="1"]` sets `--r-dim` and applies an overlay `#reader-dim.reader-dim { background:#000; opacity:var(--r-dim) }` above pages, below chrome. Any input restores instantly with a `120ms` fade.

Ruling: dim is a full-screen overlay, not a filter on content. A `filter` on `#reader-images` would repaint the whole strip on a Pi-class device. Cost if wrong: overlay must be `pointer-events: none` or it swallows the tap that should wake the reader — it is.

### 5.3 Focus mode

`body[data-r-focus="1"]` → `#reader-topbar`, `#reader-hud`, `#reader-bottom-nav`, `.reader-zone` chrome affordances hidden; `#reader-sheet` still opens. Only the pages and the page counter outside the top bar remain. Toggled by `ui.focus`, the sheet, or `#reader-focus-btn`.

### 5.4 End-of-chapter nav

- `strip`: the existing `.reader-bottom-nav` (`reader.rs:140-168`) is the terminal element of the document and needs no overlay.
- `single`/`double`/`vertical`: nav lives in a **fixed** overlay `div#reader-chapter-nav.reader-chapter-nav[hidden]`, revealed at the last page and also reachable from the sheet ("Go to…"). It reuses `.reader-nav-btn` markup verbatim, so CSS is shared.
- The overlay additionally carries `button.reader-nav-btn[data-action="next"]` with a `Next up: Ch. N` label (`Next up` = `data-next-chapter`). The server renders it; JS only reveals it.

### 5.5 Resume badge and Next-up queue

- Resume badge: `span#reader-resume-badge.reader-resume-badge[data-page]` rendered server-side, hidden until JS confirms `resumeTarget.page > 1`. Text: `Resume p. N`. Clicking scrolls/advances to that page and hides the badge. If `lastUsedLayout` in localStorage differs from the server layout, the badge appends `· last read in <layout>` and offers a one-tap switch — this is the merge UX for anon → logged-in.
- Next-up: `div#reader-nextup.reader-nextup` lists up to 3 upcoming chapters (`a.reader-nextup-item[data-chapter-id]`, `Next: Ch. N`). Server-rendered from `list_chapters_by_manga` if available; else hidden. Prefetch affordance: a `button.reader-nextup-prefetch[data-prefetch-state="idle|prefetching|ready"]` per item. Until an image proxy exists this only triggers a browser-warm `fetch(url, {mode:'no-cors'})` of the first 3 pages and sets `ready`; it is honest about being a warm-up, not an offline copy.

### 5.6 Reduced motion

`@media (prefers-reduced-motion: reduce)`:
- `transition-duration: 0.01ms` on chrome transforms, `.reader-page`, dim overlay.
- The reader's `hintPulse` keyframe is disabled entirely (`animation: none`).
- Auto-dim and hide-UI still work, but both switch instantly instead of fading.
- Auto-next's countdown still runs (it is a timer, not motion) but the progress ring is replaced by text.
- No parallax; reader scrolling is explicitly `auto`.

---

## 6. Notes

Private note per chapter, anchored to a page. Backed by `notes(user_id, chapter_id, page, body, created_at)`.

### 6.1 Create

- Trigger: the `note.add` keybinding (`KeyM`), or long-press `400ms` on a page, or the sheet's `button[data-action="note.add"]`.
- The composer is `#reader-note-composer.reader-note-composer` — a bottom sheet, not a modal, so the target page stays visible behind it.
  ```
  form#reader-note-form.reader-note-form
    textarea#reader-note-body.reader-note-body[required][maxlength=2000]
    input#reader-note-page[type=hidden][value=N]
    button.reader-note-save   → "Save note"
    button.reader-note-cancel[data-r-close]
    p.reader-note-error[role="alert"][hidden]
  ```
- Page anchor is the **current page** at trigger time, resolved by the same "current page" rule as §1.1, so it matches progress semantics exactly.
- `slug`/ids: the note is created with `page = data-page-idx`, 1-indexed, matching `progress.page`.
- Unsaved text is guarded: closing with content prompts "Discard note?".
- Anonymous: composing prompts inline to sign in; text is preserved in `sessionStorage['mangaread_note_draft_<chapterId>']` and offered back after login. Ruling: no anonymous note writes; the DB requires `user_id`. Cost if wrong: a forced login interruption mid-read — mitigated by the draft restore.

### 6.2 Indicator on the page

`button.reader-note-pin[data-note-id][data-page-idx]` absolutely positioned inside `.reader-page` at `inset-inline-end: 8px; top: 8px`, visible only when the page has ≥1 note. Count >1 → `.reader-note-pin[data-count]` shows the number. Hidden in focus mode unless `#reader-notes-toggle` is on.

### 6.3 List

`aside#reader-notes-panel.reader-notes-panel[hidden]` — right rail at `≥768px`, bottom sheet below.

- Header: `h2` "Notes — Ch. N" + `button.reader-notes-close[data-r-close]` + `span.reader-notes-count`.
- Body: `ul.reader-notes-list > li.reader-note-item[data-note-id][data-page-idx]`, each with a `button.reader-note-jump` (label: `p. N`), `time.reader-note-time[datetime]`, and `p.reader-note-text` (clamped to 4 lines, expands on click).
- Empty: `.reader-notes-empty` text "No notes in this chapter."
- Jumping sets the reader to that page (`data-page-idx`) using the current layout's page-step rule, then closes the panel.
- No edit/delete UI this slice (the DB fns are insert + list only). The item exposes `data-note-id` so a later slice can add them without a DOM change. Ruling: don't render disabled buttons we can't honor.

---

## 7. States

| State | DOM | Notes |
|---|---|---|
| Syncing (no pages yet) | existing `.loading-state > .skeleton.skeleton-page` (`reader.rs:118-123`) | Keep. Must render in the reader's fit/width so the layout doesn't jump on load: skeleton uses the same `--r-max-w` and `aspect-ratio` 3/5, 3/4 as today. Polling reload (`reader.rs:69-84`) is unchanged. |
| Page loading | `.reader-page[data-page-state="loading"]` with `.reader-page-placeholder` | Blur-up: placeholder holds a dominant color + aspect-ratio so no layout shift. Allowed without a new pipeline iff the server emits `data-page-w`/`data-page-h` or a `data-avg` color per page. **Template change is a prerequisite**; until then, `loading` uses the fixed skeleton height. |
| Page ready | `.reader-page[data-page-state="ready"]` | Set on `img` `load` (also drives the old finish check, `app.js:327-336`). |
| Image failure | `.reader-page[data-page-state="error"]` reveals `.reader-page-error` | `button.reader-retry-btn` sets `img.src = img.src` and returns to `loading`. Also auto-retries once after `2000ms`. Alt text stays `Page N`. |
| Empty chapter | `.reader-empty.reader-empty` in place of `#reader-images` | Triggered when `data-total-pages="0"` and not syncing. `role="status"`. Text + `a.reader-nav-btn` to the series page. |
| End of series | last-page nav overlay variant with `.reader-nav-btn.disabled` "Last Chapter" (already in `reader.rs:163-166`) | Suppress auto-next and the Next-up queue. |
| Offline / nav failure | `#reader-offline.reader-offline[hidden]` banner | Shown when progress write-through fails repeatedly. Text: "Saved on this device only." No retry spam. |

---

## 8. Responsive and accessibility

### 8.1 Breakpoints

Reader CSS uses the established `600px`, `768px`, and `480px` breakpoints.

| Width | Behavior |
|---|---|
| `<480px` | Sheet = full-height bottom sheet, tabs scroll. Top bar title truncates, page counter only. Zones 30/40/30. `--r-max-w` ignored above 100vw. |
| `480–767px` | Same as above; sheet at `85dvh`. |
| `≥768px` | Sheet becomes right rail (`340px`). Width slider becomes meaningful; `--r-max-w` up to 1400. Notes panel docks right. |
| `≥1200px` | Rail + notes may both be open (`#reader-images` gets `margin-inline: auto` and max-width untouched, so pages stay centered). |

`double` layout on `<768px` is disallowed in the UI (segmented button disabled with a title) — two pages at phone width are illegible. Ruling explicitly stated because it is a visible product limitation.

### 8.2 Accessibility bar (Lighthouse mobile ≥ 90)

- **Touch targets:** every interactive control `≥44×44px` at `≤768px`.
- **Contrast:** chrome text on transparent chrome must hit 4.5:1. The page indicator uses `rgba(255,255,255,0.72)` and mirrors the active tone; do not rely on the blurred backdrop for contrast because it is decorative.
- **Keyboard path:** Tab reaches `#reader-topbar` controls → zones → bottom nav → sheet. Sheet traps focus (`aria-modal`) and restores focus to `#reader-settings-btn` on close. `.reader-page` and images are not focusable (no tab stops on 40 images).
- **Focus rings:** `:focus-visible { outline: 2px solid var(--r-accent); outline-offset: 2px }`, using the reader token default.
- **Landmarks:** reader is `main#reader-images`; chrome is not inside `main`. Sheet is `role="dialog"` with `aria-labelledby`. `#reader-page-indicator` is `aria-live="polite"`, updated at most once per 500ms to avoid chatter.
- **Images:** every `img` keeps `alt="Page N"`. Decorative placeholders are `aria-hidden="true"`.
- **Motion:** §5.6.
- **No horizontal scroll:** `body.reader-page { overflow-x: hidden }` remains, and zero-width zones must not create overflow.
- **CLS:** every page and skeleton reserves its box via `aspect-ratio` before load. This is the main Lighthouse lever on a strip of remote images.

### 8.3 Third-party image reality

Images are hotlinked (`<img src=page.image_url>`). Therefore: `loading="lazy"` stays, `decoding="async"` stays, no `fetchpriority` above `high` on page 1, and no client-side `width`/`height` claims the server can't substantiate. A missing remote host must degrade to the `error` state, not a broken layout.

---

## 9. Naming index

### Body / track state

| Name | Semantics |
|---|---|
| `body[data-r-layout]` | `single\|double\|strip\|vertical` |
| `body[data-r-preset]` | `flow\|page\|spread`; `vertical` maps to `page` |
| `body[data-r-direction]` | `ltr\|rtl` |
| `body[data-r-fit]` | `""\|height\|screen` |
| `body[data-r-bg]` | `black\|white\|sepia\|custom` |
| `body[data-r-frame]` | `""\|on` |
| `body[data-r-focus]` | `""\|1` |
| `body[data-r-dim]` | `""\|1` |
| `#reader-track[data-prev-chapter]` | Prev chapter URL or absent |
| `#reader-track[data-next-chapter]` | Next chapter URL or absent |
| `#reader-track[data-auto-next]` | `1` when on |
| `#reader-track[data-prime-idx]` | Page to jump to on load (`0` = none) |

### Custom properties

`--r-max-w`, `--r-gap`, `--r-zoom`, `--r-bg`, `--r-chrome-bg`, `--r-chrome-fg`, `--r-chrome-border`, `--r-page-shadow`, `--r-radius`, `--r-dim`, `--r-controls-offset`, `--r-bg-custom`.

### Classes and ids

| Name | Semantics |
|---|---|
| `.reader-hud` / `#reader-hud` | Floating page counter, auto-hides |
| `#reader-settings-btn.reader-icon-btn` | Opens sheet; `aria-expanded` |
| `#reader-focus-btn.reader-icon-btn` | Toggles focus mode |
| `#reader-zones` | Zone container; `data-r-zone-mode="three\|two\|off"` |
| `.reader-zone[data-zone=prev\|menu\|next]` | Tap target button |
| `#reader-sheet.reader-sheet` | Settings surface; `hidden` when closed |
| `.reader-sheet-panel[role=dialog]` | Focus trap scope |
| `.reader-sheet-group[data-group="reading"]` | Curated reading controls |
| `.reader-preset-btn[data-r-preset]` | Flow/Page/Spread selection |
| `.reader-seg` / `.reader-seg-btn[data-value][aria-pressed]` | Reading-order and tone segmented control |
| `[data-pref]` | Binds a public control to a stored prefs key |
| `#reader-remember-series` | Per-series override checkbox |
| `body[data-r-preset]` | First-paint and runtime public preset |
| `.reader-page[data-page-idx][data-page-state]` | Page wrapper; state `idle\|loading\|ready\|error` |
| `.reader-page-placeholder` | Blur-up / reserved box |
| `.reader-page-error` / `.reader-retry-btn` | Failure + retry |
| `.reader-note-pin[data-note-id][data-count]` | Per-page note indicator |
| `#reader-note-composer` / `#reader-note-form` / `#reader-note-body` / `#reader-note-page` | Note composer |
| `#reader-notes-panel` / `.reader-notes-list` / `.reader-note-item[data-note-id][data-page-idx]` | Notes list |
| `.reader-note-jump` / `.reader-note-time` / `.reader-note-text` / `.reader-notes-empty` | Note list parts |
| `#reader-chapter-nav.reader-chapter-nav` | Fixed end-of-chapter overlay (paged layouts) |
| `#reader-resume-badge.reader-resume-badge[data-page]` | Resume affordance |
| `#reader-nextup.reader-nextup` / `.reader-nextup-item[data-chapter-id]` / `.reader-nextup-prefetch[data-prefetch-state]` | Next-up queue + prefetch affordance |
| `#reader-countdown` | Auto-next countdown + cancel |
| `#reader-dim.reader-dim` | Idle dim overlay |
| `#reader-offline.reader-offline` | Local-only persistence notice |
| `.reader-empty` | Empty chapter state |

### localStorage keys

| Key | Shape |
|---|---|
| `mangaread_reader_prefs` | prefs object (anon only; server blob wins when logged in) |
| `mangaread_series_prefs_<seriesId>` | full per-series prefs override |
| `mangaread_reader_local` | `{resumeBadgeSeen, hintSeen, zoom, lastUsedLayout, scrollPositions}` |
| `mangaread_note_draft_<chapterId>` | string draft (sessionStorage) |
| `mangaread_read_<seriesId>` | unchanged, `app.js:115-135` |

Server prefs blob keys (one flat object, camelCase): `layout, direction, fit, maxW, gap, bg, bgCustom, frame, doubleCover, autoNext, autoPrev, hideDelay, dimDelay, dimOpacity, swipeThreshold, clickZones, keybinds, showNotes`.

---

## 10. Rejected alternatives

- **Five flat modes.** Rejected: `rtl` and `vertical` are not mutually exclusive with `double`/`strip`; a flat enum would need 8+ combinations and forbid legitimate pairings.
- **A canvas/WebGL strip renderer.** Rejected: hotlinked third-party images, no proxy, and Pi-class hardware target. Cost: no per-page zoom smoothing. `--r-zoom` on the fit path is enough.
- **`transform`-based page transitions in `strip`.** Rejected: animating a 40-image stack repaints the whole layer on low-end mobile; the strip scrolls natively.
- **`filter: brightness()` for auto-dim.** Rejected for the overlay (repaint cost, §5.2).
- **`pushState` chapter transitions.** Rejected: no client router, server-rendered pages, and progress persistence keys off page load.
- **Anonymous note writes.** Rejected: DB requires `user_id`; a draft-in-sessionStorage path covers the intent without a new table.
- **Syncing scroll position.** Rejected: it makes cross-device resume worse, not better (§2.3).
- **`data-r-max-w` attribute instead of a custom property.** Rejected: a property can be read by themes and inline admin CSS; an attribute cannot.
