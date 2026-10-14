# Adoption Redesign — Slice R: Reader Image Pipeline Implementation Plan

> **For agentic workers:** Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-10-14-adoption-redesign-design.md` §4 Slice R.

- [x] **Image proxy (R.1).** `GET /img/{page_id}` looks up the page's origin URL and source, fetches it with `Referer: <source base URL>/`, and serves it with `ETag` and `Cache-Control: public, max-age=31536000, immutable`. The reader emits `/img/{id}?v=<url hash>`, so a re-synced page URL busts browser caches. Hidden series 404 for non-admins.
  - Disk cache under `MANGAREAD_CACHE_DIR` (default: `cache/` next to the database). Files are keyed by SHA-256 of the origin URL and written via temp file + rename.
  - LRU cap from `site_settings.image_cache_max_mb` (default 5120). A hit refreshes the file's mtime; a write over the cap evicts oldest-mtime files down to 90 % of the cap.
  - SSRF guard: http/https only, no userinfo; resolve the host once and refuse any non-public address (loopback, RFC 1918, link-local, CGNAT, ULA, multicast, documentation, unspecified, IPv4-mapped forms); pin the connection to the vetted addresses; follow at most 3 redirects manually, re-vetting each hop.
  - Response guard: 2xx only, body ≤ 20 MiB, magic bytes must be JPEG/PNG/GIF/WebP/AVIF. The served `Content-Type` comes from the sniff, never from the origin.
  - At most 8 concurrent origin fetches.
- [ ] **Admin control.** `/admin/sources` shows cache usage and a cap form.
- [ ] **Chapter prefetch (R.2).** Past 70 % of a chapter, `reader.js` warms the next chapter's first 3 pages once.
- [ ] **Offline (R.3), deferred** per spec.

Out of scope: covers still hotlink (spec names reader pages only).
