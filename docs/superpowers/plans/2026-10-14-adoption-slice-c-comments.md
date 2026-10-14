# Adoption Redesign — Slice C: Chapter Comments & Moderation Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Readers discuss each chapter at the bottom of the reader; moderators keep it civil.

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice C.

## Core
- [x] `comments` keyed by `(series_id, chapter_number)`, one reply level (replies to replies attach to the top-level parent), soft delete and moderator hide. `comment_reports` unique per reporter. `users.muted_until`.
- [x] `merge_series` moves comments before deleting the merged series (the FK cascade would drop them).
- [x] `find_chapter_in_series`: posts are refused for chapter numbers the series does not have.

## API (`/api/comments`, JSON)
- [x] List (anyone), keyset pages of 20 top-level comments with their replies. Visibility is a pure function: deleted → "[deleted]" placeholder only while it has visible replies; hidden → moderators see it tagged, others get "[removed]" under the same rule.
- [x] Post/reply, edit own, delete own, report others' (logged in). Hide/restore (moderators).
- [x] Limits: 1–2000 characters, 1 comment per 15 s and 50 per day per user, muted users get 403 with the end time.
- [x] CSRF: JSON bodies only; cross-site `application/json` needs a preflight this server never grants.

## Moderation (`/mod/reports`, moderators and admins)
- [x] Queue grouped by comment, most-reported first, with reasons and an "Open chapter" deep link. Hide (closes reports), dismiss, mute 1 day / 7 days / until lifted / lift. Moderators may mute only members. Everything logged in `mod_log`. Nav shows "Moderation" to moderators and admins.

## Reader UI
- [x] `#chapter-comments` after the chapter. Strip layout: inline, loads when within 400 px of the viewport. Paged layouts: bottom sheet above the fixed tap zones (z-index 50) and below the notes panel (210), opened from a "Comments (n)" button in the end-of-chapter overlay. Escape closes the sheet.
- [x] `comments.js` builds every node with `textContent`; no user text reaches `innerHTML`. Spoilers are blurred until revealed. The reader's key handlers already ignore form fields.

## Not verified
- Visual check in a real browser, especially the paged-layout sheet on a phone.
