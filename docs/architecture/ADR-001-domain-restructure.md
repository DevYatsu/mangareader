# ADR-001: Domain Model Restructuring

## Status
Accepted

## Context

The project has three crates (`mangaread-core`, `scraper`, `website`) but no clear domain boundaries. All database operations are free functions in `db.rs` (250+ lines, 30+ public functions). Models are in one flat `models.rs` (110 lines). Business logic like staleness checks, series merging, and source preference is inline in route handlers.

Key problems:
1. No separation between read models and write models
2. Route handlers call `db::get_*` directly — schema changes break all handlers
3. No service layer — business logic is duplicated or inline
4. `rusqlite::Connection` is not `Sync`, forcing `Mutex<Connection>` + awkward threading

## Decision

Restructure `mangaread-core` into three layers: **domain**, **repository**, and **service**.

```
mangaread-core/src/
  domain/           # Domain models (no DB dependency)
    series.rs       Series + SeriesWithManga
    manga.rs        Manga + MangaWithCount + NewManga  
    chapter.rs      Chapter + NewChapter + RecentChapter
    page.rs         Page + NewPage
    source.rs       MangaListing, MangaInfo, ChapterListing (scraper types)
    mod.rs
  db/               # Repository implementations
    mod.rs          init_db, migrations, connection helpers
    series_repo.rs
    manga_repo.rs
    chapter_repo.rs
    page_repo.rs
  service/          # Business logic
    sync_service.rs     Orchestrates scraping + DB updates
    read_service.rs     Reading progress, staleness checks
    series_service.rs   Cross-source merging, source preference
    mod.rs
  error.rs
  lib.rs
```

### Key Decisions

**Repository pattern**: Each aggregate gets a repository struct with `&Connection` methods. Route handlers call services, never repos directly.

**Service layer**: Services take `&Connection` and repository references. Business logic lives here, not in route handlers.

**Domain models stay pure**: No `rusqlite` dependency in domain structs. Serialization is handled at the repository boundary.

**Error types unified**: `mangaread-core` owns a single `Error` enum. `scraper` and `website` map their errors into it.

## Consequences

**Easier**: 
- Adding a new domain concept means adding a domain file + repo file, not appending to a 300-line db.rs
- Business logic becomes testable in isolation
- Route handlers become thin — extract data, call service, render template
- Schema changes are contained to repository files

**Harder**:
- More files to navigate (but each file is smaller)
- Initial refactoring effort to split the monolith

**Neutral**:
- Number of lines increases slightly (boilerplate for repo struct impls)
