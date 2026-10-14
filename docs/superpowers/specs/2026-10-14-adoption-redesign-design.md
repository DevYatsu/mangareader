# MangaRead — Adoption Redesign (readers, admins, installers) — Design

Date: 2026-10-14
Status: draft for review
Scope: UX and capability redesign across onboarding, admin, discovery/library, and reader.
Builds on: `2026-09-19-mangareader-customizable-website-design.md` (master), `2026-09-20-bulma-ink-and-ember-design.md` (visual identity), `2026-09-20-reader2-design-contract.md` (reader).

## 1. Context & constraints

Target, decided 2026-10-14: **a self-host app that many people install.** One instance serves 1–20 users.
"Adoption" therefore means: an installer reaches a working, populated library in minutes, an admin understands what the system is doing without reading logs, and a reader finds and reads faster than on the source sites.

Constraints kept:
- Self-host first. Single Docker image. SQLite. Rust + Maud + vanilla JS, no build step.
- Ink & Ember identity and Bulma stay. This is **not** a second visual rewrite; visual work happens only where a screen's information architecture changes.
- Reader 2.0 behavior contract stays binding unless a section below amends it explicitly.
- Raspberry Pi–class hardware stays a target.

Decisions from review, 2026-10-14:
- **Registration is open to everyone.** Only `/admin` and `/mod` are restricted.
- **Four access levels:** anonymous, user, moderator, admin (§4 Slice O.5).
- **Sign-up goes through an email link.** This amends master spec §3, which said "no email delivery anywhere".
- **Chapters get a comment section** at the bottom (Slice C).

Explicitly out of scope (master-spec items deferred under YAGNI until an installer asks):
multi-library, 8 editable themes, theme builder, custom JS boxes, social beyond chapter comments (ratings, reviews, follows, profiles, reactions), i18n beyond `en`, OPDS, public JSON API, S3 backups, Prometheus metrics.

## 2. Audit findings (2026-10-14, branch `bulma-ink-and-ember`)

Findings are ordered by impact on adoption. Each one names the evidence.

### 2.1 Onboarding & setup
| # | Finding | Evidence |
|---|---|---|
| O1 | Admin is claimed by the first person to register. On an internet-exposed instance, any visitor can win that race. | `routes/auth_pages.rs:337-345`, `routes/oauth.rs:173` |
| O2 | The admin claim is also a TOCTOU (time-of-check to time-of-use) race: `count_users` and `create_user` run under separate lock scopes, so two concurrent first sign-ups can both become admin. | `routes/auth_pages.rs:337-345` |
| O3 | A fresh install shows an empty site with "the sync loop is fetching" and no progress, ETA, or failure signal. If chromedriver fails, the loop sleeps 300 s silently. | `routes/home.rs` empty state, `scraper_helper.rs:297-303` |
| O4 | README is stale: it says "No accounts" and "Paginated grid", lists 13 of ~35 routes, and documents `SITE_COLOR=#a78bfa` while compose ships `#3b82f6`. Neither matches Ink & Ember. | `README.md`, `docker-compose.yml` |
| O5 | Install requires a source build (`build:` only, no published image), `SYS_ADMIN`, and 2 GB RAM. | `docker-compose.yml` |

### 2.2 Admin
| # | Finding | Evidence |
|---|---|---|
| A1 | Adapter health is recorded but never rendered. The "Sources" admin card says "not available yet". | `main.rs:65`, `scraper_helper.rs:219-232`, `routes/admin.rs:193` |
| A2 | Sources are hardcoded in three places (state seed, `build_engine`, scraper CLI). Adding a source violates the Open/Closed principle. | `main.rs:97-120`, `scraper_helper.rs`, `scraper/src/main.rs` |
| A3 | No sync history: errors go to `sync_error` (single slot, cleared on read) and logs only. | `main.rs` `sync_error` |
| A4 | No catalog tools: an admin cannot fix a bad title merge, hide a series, or change a cover. | — |

### 2.3 Discovery & library
| # | Finding | Evidence |
|---|---|---|
| D1 | `/series` loads **every** series, then for every manga entry loads its full chapter list just to count it, while holding the global DB mutex. Cost grows with series × sources × chapters per request and blocks all other requests. | `routes/series.rs:262-271` |
| D2 | No pagination, sort, or filters. Search is `LIKE %q%` on title only. | `db.rs:465-478` |
| D3 | The schema has no genres, status, or alt titles, so filters are impossible without a data-model change. | `db.rs:29-60` |
| D4 | The home hero spends the first screen on static stats. "Series" counts *manga rows*, so a title from 2 sources counts twice. | `routes/home.rs`, `db.rs:631` |
| D5 | Series chapter badges sum chapters across sources, so duplicate chapters inflate the count. | `db.rs:381-392` |
| D6 | Bookmarks render client-side from `localStorage` even for logged-in users with server bookmarks. | `routes/bookmarks.rs` |
| D7 | Readers see sync state (banner duplicated in 5 handlers) and source names (series tags, `data-source`, public `/source/{source}`), which contradicts master spec §8. | `grep "Syncing series"`, `routes/series.rs:106-197` |

### 2.4 Reader
| # | Finding | Evidence |
|---|---|---|
| R1 | Page images hotlink third-party hosts. Hotlink protection or referer checks break pages. Each reader's IP leaks to the source. No cache exists, so offline and true prefetch are impossible. | `routes/reader.rs:461` |
| R2 | The reader UI itself is mature (Reader 2.0, 2.5k lines JS). Gains come from the image pipeline, not from more controls. | `static/reader.js` |

### 2.5 Cross-cutting
- Every HTML handler repeats the preamble of banner, `site_config`, `theme_overrides` and `base_layout`. This violates DRY and is why D7 leaks everywhere.

## 3. Options considered

- **Visual rewrite again:** rejected. Ink & Ember shipped last week, and no finding above is visual.
- **Implement the master spec in its §15 order:** rejected. It front-loads breadth (themes, social, i18n) before an installer can even reach a working admin.
- **Adoption-first slices (chosen):** fix what blocks a new installer, then what an admin needs to operate, then what makes readers stay.

## 4. Design

### Slice F — Foundation (prerequisite for all slices)
1. Add a `layout::page(cfg, title, nav, user, content)` helper that owns the shell inputs, so handlers stop repeating the preamble (fixes the DRY violation). It is a plain function, not an extractor, so each request still locks the DB once.
2. Remove the sync banner from reader-facing pages. Sync state moves to the admin UI (A1). Fixes D7 (banner half).
3. Replace the `/series` N+1 with one aggregated query: series id, title, cover of the primary manga, and distinct chapter count by `number`. Fixes D1, D5.
4. Fix the home series count to `COUNT(DISTINCT series_id)`. Fixes D4 (count half).

### Slice O — Onboarding & setup
1. **Setup token.** On boot with zero users, generate a one-time token, print it to the log, and redirect every route to `/setup`. `/setup` requires the token and creates the admin inside one transaction. Registration stays closed until setup completes, then opens to everyone. Fixes O1, O2 with a standard bootstrap-secret pattern (Jenkins' `initialAdminPassword` works the same way). Self-registration never grants a role above user.
2. **First-run wizard** (3 steps): site title and accent → email (SMTP) settings with a "send test email" button → sources to enable.
3. **Live first-sync screen.** Empty states link admins to `/admin/sync` and tell members "Your admin is setting up the library". Fixes O3.
4. **Install path.** Publish a prebuilt image (GHCR), add a minimal `compose.yaml` with no `build:`, and document memory/`SYS_ADMIN` needs and the seccomp alternative. Rewrite README around "install → setup → read". Fixes O4, O5.
5. **Roles.** Rename the unused `Editor` variant to `Moderator`. The rank order becomes user < moderator < admin. A migration rewrites stored `'editor'` values; none exist today, but the migration keeps old databases valid. Anonymous is the absence of a session, not a role.

   | Level | How you get it | Can |
   |---|---|---|
   | Anonymous | Not logged in | Read everything, read comments, keep progress in `localStorage`. |
   | User | Self sign-up or OAuth | + comment, reply, report, server-synced library/progress/notes. |
   | Moderator | Promoted by an admin | + hide/restore any comment, work the report queue at `/mod`, mute a user from commenting for a set period. |
   | Admin | `/setup`, or promoted by an admin | + everything under `/admin`, change roles, ban accounts. |

   Invariants: an admin cannot demote or ban the last admin. Role changes and moderation actions write to a `mod_log(actor_id, action, target, reason, at)` table for auditability. `/admin/users` gains a role selector and a ban toggle.
6. **Email-link sign-up** (double opt-in pattern):
   1. `/register` asks only for an email address. The response always reads "Check your inbox", whether or not the address exists, which prevents account enumeration.
   2. The server stores `email_tokens(token_hash, email, purpose, expires_at, used_at)`. The raw token goes out only in the email, and only the SHA-256 hash is stored. This reuses the hardening already applied to reset links (`29ed409`). Purpose is `signup` (24 h) or `reset` (1 h). Tokens are single-use.
   3. For a new address, the email links to `/register/{token}`, where the user picks a display name and password. For an existing address, the email links to password reset instead.
   4. OAuth sign-up (Google, GitHub, Discord) skips the email step because the provider already verified the address.
   5. Rate limits: 3 emails per address per hour and 10 per IP per hour, both in memory.
   6. `/forgot` becomes self-service through the same token table. Admin-issued reset links stay as the fallback.
   7. Mail goes out over SMTP via `lettre`, the standard Rust mail crate. `SMTP_URL` in the environment is the boot default, editable in `/admin/email`.
   8. **SMTP not configured:** sign-up shows "Sign-up is not available yet", and the admin dashboard shows a warning. An admin may set "Allow sign-up without email verification" (off by default, decided 2026-10-14). The trade-off: installers who skip SMTP can still open sign-ups, at the cost of spam protection.

### Slice A — Admin operations
1. **Source registry.** One `fn all_adapters() -> Vec<Box<dyn SiteAdapter>>` in the `scraper` crate feeds the engine, CLI, and health map. Fixes A2 (Open/Closed).
2. **`sync_runs` table:** `(id, source, kind, manga_id NULL, started_at, finished_at, status, items, error)`. The sync loop writes one row per scan or sync. This is the master spec's `jobs` table, narrowed to what exists today (YAGNI).
3. **`/admin/sources`:** per source, show health, last success, failure streak, last error, an enable/disable toggle (stored in `site_settings`), and "Run now".
4. **`/admin/sync`:** loop progress, last 100 runs, failed-run filter, and retry. Fixes A1, A3.
5. **`/admin/series`** (admin-only; moderators have no catalog access): search, rename, hide, cover override, and merge/split manga ↔ series. Fixes A4. Manual chapter upload stays deferred.

### Slice C — Chapter comments & moderation
1. **Anchor.** Comments attach to `(series_id, chapter_number)`, not to `chapter_id`. A chapter from two sources is two `chapters` rows. Keying by row would split one discussion in two and lose it when the preferred source changes.
2. **Schema.** `comments(id, series_id, chapter_number, user_id, parent_id NULL, body, spoiler, created_at, edited_at NULL, hidden_at NULL, hidden_by NULL, deleted_at NULL)` with an index on `(series_id, chapter_number, created_at)`. `comment_reports(id, comment_id, reporter_id, reason, created_at, resolved_by NULL, resolved_at NULL)`, unique on `(comment_id, reporter_id)`.
3. **Threading:** one reply level. A reply to a reply attaches to the top-level parent. Deep trees are YAGNI for chapter chatter, and they read badly on phones.
4. **Content:** plain text only, max 2,000 characters. Line breaks are kept, and Maud escaping is the XSS (cross-site scripting) boundary. There is no Markdown and no HTML. The spoiler flag blurs the body until it is tapped.
5. **Placement in the reader** (amends the Reader 2.0 contract):
   - The section `#chapter-comments` sits outside `.reader-pages`, after the chapter nav.
   - In `strip` layout it sits below the sticky bottom nav.
   - In paged layouts (`single`, `double`, `vertical`) the end-of-chapter overlay gains a "Comments (n)" button that scrolls to the section.
   - Comments load lazily: an `IntersectionObserver` fetches `/api/comments?series=&chapter=` when the section nears the viewport. Comments therefore never compete with page images.
   - Newest first, 20 per page, keyset pagination.
6. **Actions by level:**
   - Anonymous users read comments and see "Log in to comment".
   - Users post, reply, edit their own comments (marked "edited"), and delete their own. A deleted comment that has replies shows "[deleted]".
   - Users report others' comments.
   - Moderators and admins hide or restore any comment, resolve reports, and mute authors.
7. **Moderation queue:** `/mod/reports` lists open reports grouped by comment. It offers hide, dismiss, and mute (1 day / 7 days / permanent). One click shows the comment in its chapter.
8. **Anti-spam:** at most 1 comment per 15 s and 50 per day per user. Muted or banned users get a 403 with the reason and the end date.
9. **Deferred:** reply notifications, reactions, comments on series pages.

### Slice D — Discovery & library
1. **Metadata.** Add `series.status`, `series.genres` (normalized `series_genres` table), and `series.alt_titles`. Extend `MangaInfo` with optional fields so adapters fill what they can; nothing breaks when an adapter returns `None`.
2. **Search.** Use an SQLite FTS5 virtual table over title, alt titles, author, and description. Keep the `LIKE` fallback for queries shorter than 3 characters.
3. **Browse.** Add keyset pagination (stable under inserts, unlike `OFFSET`), sort (updated / added / A–Z), and filters (status, genre, my progress). The state lives in query params, so URLs stay shareable.
4. **Library replaces Bookmarks.** Server-rendered for members, with five fixed shelves: Reading, Plan to read, Completed, On hold, Dropped. Anonymous users keep `localStorage`; the existing signup merge carries their data over. Custom shelves stay deferred. Fixes D6.
5. **Home, reordered by intent:** Continue reading (server-rendered for members) → New chapters in your library → Latest updates → Browse. Remove the stats hero. Fixes D4.
6. **Hide sources from readers.** Remove source tags, `data-source`, and the public `/source/*` route (redirect to `/series`), per master spec §8. Source preference becomes a series-level admin setting. Fixes D7.

### Slice R — Reader image pipeline
1. **Image proxy** at `/img/{page_id}`. The server fetches with the source's expected `Referer`, stores the result in a content-addressed disk cache (`cache/`) with an LRU size cap set in admin, and serves `ETag` + `Cache-Control: immutable`. The reader never sees origin URLs. Fixes R1.
2. **Chapter prefetch.** When the reader passes 70 % of a chapter, the client warms the next chapter's first N pages through the proxy (N = 3, matching the current Next-up warm-up).
3. **Offline (deferred to a later slice):** a service worker plus per-series download builds on the proxy. Without the proxy, offline cannot work.

## 5. Rollout order & justification

F → O → A → C → D → R.
- **F first:** every later slice touches page rendering, and D1 grows worse as the library grows.
- **O before A:** an installer who cannot claim admin safely never reaches the admin UI. Roles and email sign-up also live in O, because open registration without email verification invites spam accounts.
- **A before C:** comments need the moderator role, `mod_log`, and the user ban/mute controls in place first. Shipping comments without moderation exposes every instance to abuse on day one.
- **C before D:** you asked for comments explicitly, and C has no dependency on adapter metadata.
- **D before R:** discovery metadata comes from adapters, and admins need sync visibility to debug metadata gaps.
- **R last:** the reader already works, and R1 is a reliability fix, not a blocker.

This order is a judgment call. If your current users complain mostly about broken images, move R ahead of C and D.

## 6. Acceptance checks

- A fresh `docker compose up` with the published image reaches a working admin account without editing SQLite or racing other visitors.
- An admin can answer "is source X working, and why not?" from `/admin/sources` alone.
- `/series` serves a 5,000-series library in under 100 ms on a Pi 4. It runs a constant number of queries per request, independent of library size.
- No reader-facing HTML contains a source name, origin URL, or sync banner (enforced by a template test like the existing `admin.rs` markup assertions).
- A member's library and continue-reading render server-side and match on a second device.
- Reader pages load when the source blocks hotlinking.
- A visitor signs up with only an email address. The link creates a user-role account, and the same form reveals nothing about whether an address is already registered.
- A user cannot reach `/admin` or `/mod` (403). A moderator reaches `/mod` but not `/admin`. Nobody can remove the last admin.
- A comment posted on chapter 12 from source A still appears after the series' preferred source changes to B.
- A reported comment appears in `/mod/reports`. Hiding it removes it for everyone except moderators and admins, and the action appears in `mod_log`.

## 7. Open questions

Resolved 2026-10-14:
- Registration is open to everyone.
- Without SMTP, "Allow sign-up without email verification" defaults to **off**.
- Catalog editing (titles, covers, merges, hide) is **admin-only**. Moderators handle comments and reports only.

1. Image cache default cap: 5 GB? It depends on typical host disks.
2. Do the existing adapters expose genres and status on their detail pages? This must be verified per adapter before Slice D estimates hold.
