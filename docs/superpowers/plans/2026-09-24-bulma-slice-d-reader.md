# Bulma Migration — Slice D: Reader Contract & UX Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the reader migration around the opinionated Flow/Page/Spread surface, make `reader.css` standalone, and remove dead advanced UI without changing stored preference or navigation contracts.

**Architecture:** Keep the existing flat `ReaderPrefs` JSON and `MangaReader`/`MangaReadState` JavaScript APIs. The public reader exposes only Flow, Page, Spread, order, tone, focus, notes, per-series memory, and restore; hidden advanced fields remain stored and functional. Consolidate reader CSS rather than appending another override layer.

**Tech Stack:** Rust, Maud 0.27, Bulma 1.0.4, plain CSS, existing reader JavaScript.

**Spec:** `docs/superpowers/specs/2026-09-20-reader2-design-contract.md` and `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

## Global Constraints

- Curated three-preset UX is authoritative: Flow is default; stored `vertical` maps to Page; Spread is wide-screen only.
- Preserve `data-r-*` attributes, progress 1-indexing, keyboard/touch navigation, source switching, notes privacy, focus mode, reduced motion, and anonymous-to-account preference merge.
- Do not add a persisted `preset` field or expose width/gap/fit/zone/keybind/dim editors.
- Preserve stored advanced values and runtime behavior even when their editors are hidden.
- Do not add source/origin/adapter/resync concepts to reader output.
- Do not link `style.css` or add another stylesheet/build step.

## Files

- Modify: `website/src/routes/reader.rs`, `reader_prefs.rs`, `notes.rs`, `sync.rs`
- Modify: `website/static/reader.js`, `reader.css`
- Modify: `website/src/config.rs`, `website/static/theme.css` for transitional token removal
- Modify: `mangaread-core/src/db.rs`, `mangaread-core/src/domain/reader.rs` only for narrow contract/data fixes
- Modify: `docs/superpowers/specs/2026-09-20-reader2-design-contract.md`, `docs/superpowers/specs/2026-09-20-bulma-ink-and-ember-design.md`

### Task D0: Resolve the contract gate

Record in the reader contract that hidden advanced preferences remain supported, the visible surface is curated, `vertical` is storage compatibility, phone Spread falls back to Page, and the current sibling next-up/HUD hooks are valid. Keep `/api/progress` and `/api/reader-prefs` as the only server seams.

### Task D1: Add red contract tests

Assert preset mapping for `strip`, `single`, `vertical`, and `double`; unknown layout normalization; `data-r-preset`; custom-tone escaping; JSON-island escaping; curated controls present and advanced controls absent; complete preference equality; progress 1-indexing and last-write-wins; note response fields including `created_at` if the additive response change is made.

Run:

```bash
cargo test -p mangaread-core reader::
cargo test -p website reader::
cargo test -p website reader_prefs::
cargo test -p website notes::
```

### Task D2: Align reader markup

Add Bulma classes to the topbar, buttons, preset controls, sheet, notes composer, retry, and navigation while preserving existing IDs/hooks. Move the per-series memory control to the top of the sheet. Keep next-up as a row wrapper containing a chapter link and sibling prefetch button. Keep first-paint `data-r-*` attributes and phone-safe Spread behavior. Hydrate existing progress through `/api/progress` without creating a write loop if the current response shape supports it.

### Task D3: Make reader CSS standalone

Port fixed topbar, visible/translate states, bottom navigation, hints, skeleton/error geometry, and reader page bases from the unlinked legacy stylesheet into the consolidated `reader.css`. Replace `var(--accent)`, `var(--font-display)`, and `var(--font-body)` with Bulma/approved tokens. Remove the transitional aliases from `theme.css` and `config.rs` after no consumers remain. Delete dead CSS for tabs, sliders, zone editor, keybind editor, and custom-color picker. Preserve reader-specific `--r-*` state variables, 44px targets, contrast, reduced motion, RTL, and no horizontal overflow.

Audit:

```bash
git grep -nE 'var\(--accent|var\(--font-display|var\(--font-body|style\.css' -- website/static/reader.css website/static/theme.css website/src/config.rs
```

### Task D4: Complete curated JavaScript

Use one public preset map for buttons and `L`/legacy `W`; preserve advanced fields on preset changes; move the memory checkbox wiring; implement complete anonymous preference comparison; flush pending saves; preserve hidden keymaps/zones/swipe/auto-next; hydrate newer server progress safely; implement or explicitly amend the boundary `nudge()` no-op; keep one auto-next start path; retain notes draft/list/jump/privacy; keep focus trap, RTL, swipe, retry, and public APIs.

### Task D5: Reader acceptance

Run formatting, workspace tests, build, reader tests, JS syntax, and static audits. With a real browser, verify Flow/Page/Spread at 375/768/1024/1440px, RTL, resize fallback, first paint, persistence, notes, image retry, focus exit, reduced motion, and no horizontal overflow. Static reader output audit:

```bash
git grep -nE 'source|origin|adapter|resync|sync-badge' -- website/src/routes/reader.rs website/static/reader.js website/static/reader.css
```

Expected: no reader-facing scraping concepts.

## Deferrals

No new persisted preset field, visible advanced editor, source/catalog feature, image transcoding, offline download system, or page-total `data-pages` fix.
