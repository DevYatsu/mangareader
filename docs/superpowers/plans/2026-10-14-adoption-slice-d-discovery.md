# Adoption Redesign — Slice D: Discovery & Library Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice D.

- [x] **Hide sources from readers (D.6).** No source tag, per-source toggles or per-row source labels on the series page; no sync-error toast; `/source/{name}` redirects to `/series`.
- [x] **Search (D.2).** FTS5 `series_search` (title, every source's title, authors, description; bm25 weights 10/5/2/1; prefix match; FTS syntax neutralized). Queries under 3 characters or with zero hits fall back to a title substring. Full rebuild at boot, after scans, at pass end and after admin catalog edits.
- [x] **Browse (D.3).** Sort by latest updates (default), new, A–Z; 48 per page; keyset cursor `key~id`.
- [x] **Library (D.4).** `bookmarks.shelf` (reading, plan, completed, on_hold, dropped). `/library` server-rendered for members, device-only list for visitors, `/bookmarks` redirects. Series page shelf picker for members.
- [x] **Home (D.5).** Members: continue reading (resume page, or next chapter after a finished one), new in your library, latest updates. Visitors: intro with the admin's site title and tagline, device-based continue reading, latest updates. Stats hero removed.
- [ ] **Metadata (D.1), deferred.** Genres, status and alt titles need new selectors in each adapter. They cannot be verified without running the scrapers against the live sites (Chromium could not start a session in this environment). Do this with a working chromedriver: extend `MangaInfo` with optional fields, then add status/genre filters to `/series`.

## Known gap (pre-existing, not fixed here)
- Visitors' "Continue reading" (`/api/progress-series`) links each series' first chapter instead of the chapter in progress. Members now get the correct server-side version.
