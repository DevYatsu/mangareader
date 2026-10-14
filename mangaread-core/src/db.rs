use std::collections::HashMap;

use rusqlite::Connection;

use crate::domain::title::{clean_display_title, normalize_title};
use crate::domain::*;
use crate::error::Error;

/// Return the database file path, using `MANGAREAD_DB` env var or a default.
pub fn database_path() -> String {
    std::env::var("MANGAREAD_DB").unwrap_or_else(|_| "mangaread.db".to_string())
}

/// Open a connection to the configured database.
pub fn open_db() -> Result<Connection, Error> {
    let path = database_path();
    let conn = Connection::open(&path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
    Ok(conn)
}

/// Create tables if they don't exist + run migrations.
pub fn init_db(conn: &Connection) -> Result<(), Error> {
    // SQLite disables FK enforcement by default; without this the
    // REFERENCES/ON DELETE CASCADE clauses below are parsed but ignored.
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS series (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            title           TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS manga (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            source          TEXT NOT NULL,
            source_id       TEXT NOT NULL,
            title           TEXT NOT NULL,
            author          TEXT,
            cover_url       TEXT,
            description     TEXT,
            detail_url      TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            chapters_synced_at TEXT,
            series_id       INTEGER REFERENCES series(id),
            UNIQUE(source, source_id)
        );

        CREATE TABLE IF NOT EXISTS chapters (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            manga_id        INTEGER NOT NULL REFERENCES manga(id) ON DELETE CASCADE,
            source_id       TEXT NOT NULL,
            number          REAL NOT NULL,
            title           TEXT,
            url             TEXT NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(manga_id, source_id)
        );

        CREATE TABLE IF NOT EXISTS pages (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            chapter_id      INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            page_index      INTEGER NOT NULL,
            image_url       TEXT NOT NULL,
            downloaded      INTEGER NOT NULL DEFAULT 0,
            UNIQUE(chapter_id, page_index)
        );

        CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT 'member',
            locale TEXT NOT NULL DEFAULT 'en',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL,
            device TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS oauth_accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id),
            provider TEXT NOT NULL,
            provider_user_id TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(provider, provider_user_id)
        );
        -- Legacy: reset links moved to email_tokens (hashed). Kept so old
        -- databases open unchanged; nothing writes here any more.
        CREATE TABLE IF NOT EXISTS password_resets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id),
            token TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL,
            used INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS bookmarks (
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(user_id, series_id)
        );
        CREATE TABLE IF NOT EXISTS progress (
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            page INTEGER NOT NULL DEFAULT 1,
            status TEXT NOT NULL DEFAULT 'started',
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(user_id, chapter_id)
        );
        CREATE TABLE IF NOT EXISTS history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            read_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE TABLE IF NOT EXISTS notes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            chapter_id INTEGER NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
            page INTEGER NOT NULL DEFAULT 1,
            body TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_progress_user_updated ON progress(user_id, updated_at);
        CREATE INDEX IF NOT EXISTS idx_history_user ON history(user_id, read_at);
        CREATE TABLE IF NOT EXISTS site_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL DEFAULT '',
            updated_by INTEGER NOT NULL DEFAULT 0,
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        ",
    )?;
    // Migrations for existing DBs (safe no-ops if column already exists)
    conn.execute_batch("ALTER TABLE manga ADD COLUMN chapters_synced_at TEXT;")
        .ok();
    conn.execute_batch("ALTER TABLE manga ADD COLUMN series_id INTEGER REFERENCES series(id);")
        .ok();
    // users.theme migration: ALTER TABLE only if the column is missing
    // (keeps init_db idempotent for existing DBs).
    let has_theme: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(users)")?;
        let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for col in cols {
            if col? == "theme" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_theme {
        conn.execute_batch("ALTER TABLE users ADD COLUMN theme TEXT NOT NULL DEFAULT ''")?;
    }
    // users.reader_prefs migration: one JSON blob per user (design contract §2.3).
    let has_reader_prefs: bool = {
        let mut stmt = conn.prepare("PRAGMA table_info(users)")?;
        let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
        let mut found = false;
        for col in cols {
            if col? == "reader_prefs" {
                found = true;
                break;
            }
        }
        found
    };
    if !has_reader_prefs {
        conn.execute_batch("ALTER TABLE users ADD COLUMN reader_prefs TEXT NOT NULL DEFAULT '{}'")?;
    }
    // Set when an admin renames a series, so syncs stop overwriting it.
    if !has_column(conn, "series", "title_locked")? {
        conn.execute_batch(
            "ALTER TABLE series ADD COLUMN title_locked INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    if !has_column(conn, "users", "banned_at")? {
        conn.execute_batch("ALTER TABLE users ADD COLUMN banned_at TEXT")?;
    }
    // `editor` was the pre-2026-10 name of the moderator rank.
    conn.execute_batch(
        "UPDATE users SET role = 'moderator' WHERE role = 'editor';
         CREATE TABLE IF NOT EXISTS mod_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            actor_id INTEGER NOT NULL REFERENCES users(id),
            action TEXT NOT NULL,
            target_user_id INTEGER REFERENCES users(id),
            detail TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_mod_log_created ON mod_log(created_at);
         CREATE TABLE IF NOT EXISTS email_tokens (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            token_hash TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL,
            purpose TEXT NOT NULL,
            expires_at TEXT NOT NULL,
            used_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );
         CREATE INDEX IF NOT EXISTS idx_email_tokens_email ON email_tokens(email, purpose);
         CREATE TABLE IF NOT EXISTS sync_runs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source TEXT NOT NULL,
            kind TEXT NOT NULL,
            target_id INTEGER,
            started_at TEXT NOT NULL DEFAULT (datetime('now')),
            finished_at TEXT,
            status TEXT NOT NULL DEFAULT 'running',
            items INTEGER,
            error TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_sync_runs_source ON sync_runs(source, id);",
    )?;
    Ok(())
}

/// Whether `table` has a column named `column` (for idempotent migrations).
fn has_column(conn: &Connection, table: &str, column: &str) -> Result<bool, Error> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for col in cols {
        if col? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── Manga CRUD ──

/// Insert a manga entry, updating on conflict of `(source, source_id)`.
/// Returns the row id.
pub fn insert_manga(conn: &Connection, m: &NewManga) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO manga (source, source_id, title, author, cover_url, description, detail_url, series_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT(source, source_id) DO UPDATE SET
            title = excluded.title,
            author = COALESCE(excluded.author, author),
            cover_url = COALESCE(excluded.cover_url, cover_url),
            description = COALESCE(excluded.description, description),
            detail_url = excluded.detail_url,
            series_id = COALESCE(excluded.series_id, series_id),
            updated_at = datetime('now')",
        rusqlite::params![
            m.source,
            m.source_id,
            m.title,
            m.author,
            m.cover_url,
            m.description,
            m.detail_url,
            m.series_id,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all distinct source site identifiers known in the database.
pub fn list_manga_sources(conn: &Connection) -> Result<Vec<String>, Error> {
    let mut stmt = conn.prepare("SELECT DISTINCT source FROM manga ORDER BY source")?;
    let rows = stmt.query_map([], |row| row.get(0))?;
    let mut sources = Vec::new();
    for row in rows {
        sources.push(row?);
    }
    Ok(sources)
}

/// Set the chapters_synced_at timestamp to now for a manga.
pub fn set_chapters_synced(conn: &Connection, manga_id: i64) -> Result<(), Error> {
    conn.execute(
        "UPDATE manga SET chapters_synced_at = datetime('now') WHERE id = ?1",
        rusqlite::params![manga_id],
    )?;
    Ok(())
}

/// Returns `true` if the manga's chapters were synced recently enough
/// (within the last `stale_minutes` minutes).
pub fn is_manga_fresh(conn: &Connection, manga_id: i64, stale_minutes: i64) -> Result<bool, Error> {
    let fresh: bool = conn.query_row(
        "SELECT chapters_synced_at IS NOT NULL
             AND chapters_synced_at >= datetime('now', ?1)
         FROM manga WHERE id = ?2",
        rusqlite::params![format!("-{} minutes", stale_minutes), manga_id],
        |row| row.get(0),
    )?;
    Ok(fresh)
}

/// List manga entries from a given source site, ordered by title.
pub fn list_manga_by_source(conn: &Connection, source: &str) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE source = ?1 ORDER BY title",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// List manga entries from a source site with their scraped chapter count.
pub fn list_manga_by_source_with_count(
    conn: &Connection,
    source: &str,
) -> Result<Vec<MangaWithCount>, Error> {
    let mut stmt = conn.prepare(
        "SELECT m.id, m.source, m.source_id, m.title, m.author, m.cover_url, m.description, m.detail_url, m.created_at, m.updated_at, m.chapters_synced_at, m.series_id,
                COALESCE(cnt.c, 0) AS chapter_count
         FROM manga m
         LEFT JOIN (SELECT manga_id, COUNT(*) AS c FROM chapters GROUP BY manga_id) cnt ON cnt.manga_id = m.id
         WHERE m.source = ?1
         ORDER BY m.title",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], |row| {
        let manga = map_manga_row(row)?;
        let chapter_count: i64 = row.get(12)?;
        Ok(MangaWithCount {
            manga,
            chapter_count,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// List every manga entry across all sources, ordered by title.
pub fn list_all_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Fetch a single manga entry by its database id.
///
/// # Errors
/// Returns `Error::NotFound` if no manga matches the given id.
pub fn get_manga_by_id(conn: &Connection, id: i64) -> Result<Manga, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE id = ?1",
    )?;
    stmt.query_row(rusqlite::params![id], map_manga_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("manga id={}", id)),
            other => other.into(),
        })
}

fn map_manga_row(row: &rusqlite::Row) -> rusqlite::Result<Manga> {
    Ok(Manga {
        id: row.get(0)?,
        source: row.get(1)?,
        source_id: row.get(2)?,
        title: row.get(3)?,
        author: row.get(4)?,
        cover_url: row.get(5)?,
        description: row.get(6)?,
        detail_url: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        chapters_synced_at: row.get(10)?,
        series_id: row.get(11)?,
    })
}

// ── Series CRUD ──

/// Insert a new series and return its id.
pub fn insert_series(conn: &Connection, title: &str) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO series (title) VALUES (?1)",
        rusqlite::params![title],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Fetch a single series by its database id.
///
/// # Errors
/// Returns `Error::NotFound` if no series matches the given id.
pub fn get_series_by_id(conn: &Connection, id: i64) -> Result<Series, Error> {
    conn.query_row(
        "SELECT id, title, created_at FROM series WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(Series {
                id: row.get(0)?,
                title: row.get(1)?,
                created_at: row.get(2)?,
            })
        },
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("series id={}", id)),
        other => other.into(),
    })
}

/// Find existing series by normalized title, or create a new one.
pub fn find_or_create_series(conn: &Connection, title: &str) -> Result<i64, Error> {
    let normalized = normalize_title(title);
    // Try to find an existing series with matching normalized title
    let mut stmt = conn.prepare("SELECT id, title FROM series")?;
    let rows = stmt.query_map([], |row| {
        let id: i64 = row.get(0)?;
        let t: String = row.get(1)?;
        Ok((id, t))
    })?;
    for row in rows {
        let (id, t) = row?;
        if normalize_title(&t) == normalized {
            return Ok(id);
        }
    }
    // No match — create new with cleaned display title
    let display = clean_display_title(title);
    insert_series(conn, &display)
}

/// List series cards for the grid in one query, optionally filtered by a
/// title substring. Series without chapters are excluded.
///
/// The cost is constant in round trips regardless of library size: the
/// previous implementation loaded every manga entry and every chapter list
/// per series just to pick a cover and count chapters.
///
/// Search results are capped at 50, matching the previous behavior.
pub fn list_series_cards(
    conn: &Connection,
    search: Option<&str>,
) -> Result<Vec<SeriesCard>, Error> {
    // `ROW_NUMBER()` picks one cover per series: the manga with the most
    // chapters wins, and the lowest id breaks ties deterministically.
    let sql = "
        WITH per_manga AS (
            SELECT m.id, m.series_id, m.cover_url, COUNT(c.id) AS cnt
            FROM manga m
            LEFT JOIN chapters c ON c.manga_id = m.id
            WHERE m.series_id IS NOT NULL
            GROUP BY m.id
        ),
        covers AS (
            SELECT series_id, cover_url,
                   ROW_NUMBER() OVER (PARTITION BY series_id ORDER BY cnt DESC, id ASC) AS rn
            FROM per_manga
        ),
        counts AS (
            SELECT m.series_id, COUNT(DISTINCT c.number) AS n
            FROM chapters c
            JOIN manga m ON m.id = c.manga_id
            GROUP BY m.series_id
        )
        SELECT s.id, s.title, cv.cover_url, counts.n
        FROM series s
        JOIN counts ON counts.series_id = s.id
        LEFT JOIN covers cv ON cv.series_id = s.id AND cv.rn = 1
        WHERE ?1 IS NULL OR s.title LIKE ?1 ESCAPE '\\'
        ORDER BY s.title
        LIMIT ?2";
    let pattern = search.map(|q| format!("%{}%", escape_like(q)));
    let limit: i64 = if search.is_some() { 50 } else { -1 };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        Ok(SeriesCard {
            id: row.get(0)?,
            title: row.get(1)?,
            cover_url: row.get(2)?,
            chapter_count: row.get(3)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Escape `LIKE` wildcards so user input matches literally (SQL `ESCAPE '\'`).
fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for ch in q.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Get all manga entries linked to a series.
pub fn list_manga_by_series(conn: &Connection, series_id: i64) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE series_id = ?1 ORDER BY source",
    )?;
    let rows = stmt.query_map(rusqlite::params![series_id], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Get all manga that are NOT yet linked to any series.
pub fn list_unlinked_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE series_id IS NULL ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Link a manga to a series.
pub fn link_manga_to_series(conn: &Connection, manga_id: i64, series_id: i64) -> Result<(), Error> {
    conn.execute(
        "UPDATE manga SET series_id = ?1 WHERE id = ?2",
        rusqlite::params![series_id, manga_id],
    )?;
    Ok(())
}

/// Link a manga to a series by title only if it has none yet. Returns the
/// manga's series id either way.
///
/// Scans call this on every pass. Relinking an already-linked manga would
/// undo an admin's manual merge or split, so existing links are kept.
pub fn ensure_manga_linked(conn: &Connection, title: &str, manga_id: i64) -> Result<i64, Error> {
    let current: Option<i64> = conn.query_row(
        "SELECT series_id FROM manga WHERE id = ?1",
        rusqlite::params![manga_id],
        |row| row.get(0),
    )?;
    match current {
        Some(sid) => Ok(sid),
        None => ensure_manga_in_series(conn, title, manga_id),
    }
}

/// Try to find or create a series for this manga, then link it.
/// Returns the series id.
pub fn ensure_manga_in_series(conn: &Connection, title: &str, manga_id: i64) -> Result<i64, Error> {
    let series_id = find_or_create_series(conn, title)?;
    link_manga_to_series(conn, manga_id, series_id)?;
    Ok(series_id)
}

/// Update the series title (e.g. with a clean detail-page title after sync).
/// A no-op for series an admin renamed (`title_locked`).
pub fn update_series_title(conn: &Connection, series_id: i64, title: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE series SET title = ?1 WHERE id = ?2 AND title != ?1 AND title_locked = 0",
        rusqlite::params![title, series_id],
    )?;
    Ok(())
}

/// Get the most recently scraped chapters with their series info.
pub fn list_recent_chapters(conn: &Connection, limit: i64) -> Result<Vec<RecentChapter>, Error> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.number, c.title, c.url, c.created_at,
                m.id, m.series_id, s.title, m.cover_url
         FROM chapters c
         JOIN manga m ON m.id = c.manga_id
         JOIN series s ON s.id = m.series_id
         ORDER BY c.created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], |row| {
        Ok(RecentChapter {
            chapter_id: row.get(0)?,
            chapter_number: row.get(1)?,
            chapter_title: row.get(2)?,
            chapter_url: row.get(3)?,
            created_at: row.get(4)?,
            manga_id: row.get(5)?,
            series_id: row.get(6)?,
            series_title: row.get(7)?,
            cover_url: row.get(8)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

// ── Chapter CRUD ──

/// Insert a chapter, updating on conflict of `(manga_id, source_id)`.
/// Returns the row id.
pub fn insert_chapter(conn: &Connection, ch: &NewChapter) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO chapters (manga_id, source_id, number, title, url)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(manga_id, source_id) DO UPDATE SET
            number = excluded.number,
            title = excluded.title,
            url = excluded.url",
        rusqlite::params![ch.manga_id, ch.source_id, ch.number, ch.title, ch.url],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all chapters for a manga, ordered by chapter number.
pub fn list_chapters_by_manga(conn: &Connection, manga_id: i64) -> Result<Vec<Chapter>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, manga_id, source_id, number, title, url, created_at
         FROM chapters WHERE manga_id = ?1 ORDER BY number",
    )?;
    let rows = stmt.query_map(rusqlite::params![manga_id], map_chapter_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Fetch a single chapter by its database id.
///
/// # Errors
/// Returns `Error::NotFound` if no chapter matches the given id.
pub fn get_chapter_by_id(conn: &Connection, id: i64) -> Result<Chapter, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, manga_id, source_id, number, title, url, created_at
         FROM chapters WHERE id = ?1",
    )?;
    stmt.query_row(rusqlite::params![id], map_chapter_row)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("chapter id={}", id)),
            other => other.into(),
        })
}

fn map_chapter_row(row: &rusqlite::Row) -> rusqlite::Result<Chapter> {
    Ok(Chapter {
        id: row.get(0)?,
        manga_id: row.get(1)?,
        source_id: row.get(2)?,
        number: row.get(3)?,
        title: row.get(4)?,
        url: row.get(5)?,
        created_at: row.get(6)?,
    })
}

// ── Page CRUD ──

/// Insert a page, updating on conflict of `(chapter_id, page_index)`.
/// Returns the row id.
pub fn insert_page(conn: &Connection, p: &NewPage) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO pages (chapter_id, page_index, image_url)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(chapter_id, page_index) DO UPDATE SET image_url = excluded.image_url",
        rusqlite::params![p.chapter_id, p.index, p.image_url],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List all pages for a chapter, ordered by page index.
pub fn list_pages_by_chapter(conn: &Connection, chapter_id: i64) -> Result<Vec<Page>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, chapter_id, page_index, image_url, downloaded
         FROM pages WHERE chapter_id = ?1 ORDER BY page_index",
    )?;
    let rows = stmt.query_map(rusqlite::params![chapter_id], map_page_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Get the IDs of all manga entries that have zero chapters synced.
///
/// Used by the background sync loop to discover which manga still need
/// their first chapter sync.
pub fn list_manga_ids_with_zero_chapters(conn: &Connection) -> Result<Vec<i64>, Error> {
    let mut stmt = conn.prepare(
        "SELECT m.id
         FROM manga m
         LEFT JOIN chapters c ON c.manga_id = m.id
         WHERE c.id IS NULL
         ORDER BY m.id",
    )?;
    let ids = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    ids.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Count series with at least one chapter. A series with two sources counts once.
pub fn count_series_with_chapters(conn: &Connection) -> Result<i64, Error> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT m.series_id)
         FROM manga m
         JOIN chapters c ON c.manga_id = m.id",
        [],
        |row| row.get(0),
    )?;
    Ok(count)
}

/// Get manga IDs that need syncing — either never synced or stale.
///
/// A manga is stale when its `chapters_synced_at` is older than
/// `stale_minutes` ago. Returns IDs ordered by stalest first.
///
/// Manga from `skip_sources` (sources an admin disabled) are left out.
pub fn list_manga_ids_needing_sync(
    conn: &Connection,
    stale_minutes: i64,
    skip_sources: &[String],
) -> Result<Vec<i64>, Error> {
    let mut stmt = conn.prepare(
        // `NOT EXISTS`, not a join: a join returned a stale manga once per
        // chapter, so one pass could re-sync the same manga up to 50 times.
        "SELECT m.id
         FROM manga m
         WHERE (NOT EXISTS (SELECT 1 FROM chapters c WHERE c.manga_id = m.id)
            OR m.chapters_synced_at IS NULL
            OR m.chapters_synced_at < datetime('now', ?1))
           AND m.source NOT IN (SELECT value FROM json_each(?2))
         ORDER BY m.chapters_synced_at IS NOT NULL, m.chapters_synced_at ASC
         LIMIT 50",
    )?;
    let ids = stmt.query_map(
        rusqlite::params![
            format!("-{} minutes", stale_minutes),
            serde_json::to_string(skip_sources).unwrap_or_else(|_| "[]".into())
        ],
        |row| row.get::<_, i64>(0),
    )?;
    ids.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Get all manga IDs (used for full sync pass).
pub fn list_all_manga_ids(conn: &Connection) -> Result<Vec<i64>, Error> {
    let mut stmt = conn.prepare("SELECT id FROM manga ORDER BY id")?;
    let ids = stmt.query_map([], |row| row.get::<_, i64>(0))?;
    ids.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

fn map_page_row(row: &rusqlite::Row) -> rusqlite::Result<Page> {
    Ok(Page {
        id: row.get(0)?,
        chapter_id: row.get(1)?,
        index: row.get(2)?,
        image_url: row.get(3)?,
        downloaded: row.get::<_, i32>(4)? != 0,
    })
}

// ── Sync runs (admin visibility into scraping) ──

/// What a sync run did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// Discover manga listings on one source.
    Scan,
    /// Fetch one manga's metadata and chapter list.
    Manga,
    /// Fetch one chapter's page URLs.
    Chapter,
}

impl SyncKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncKind::Scan => "scan",
            SyncKind::Manga => "manga",
            SyncKind::Chapter => "chapter",
        }
    }
}

/// One row of `sync_runs`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncRun {
    pub id: i64,
    pub source: String,
    pub kind: String,
    pub target_id: Option<i64>,
    pub started_at: String,
    pub finished_at: Option<String>,
    /// `running`, `ok`, or `failed`.
    pub status: String,
    pub items: Option<i64>,
    pub error: Option<String>,
}

/// Open a run in `running` state and return its id.
pub fn start_sync_run(
    conn: &Connection,
    source: &str,
    kind: SyncKind,
    target_id: Option<i64>,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO sync_runs (source, kind, target_id) VALUES (?1, ?2, ?3)",
        rusqlite::params![source, kind.as_str(), target_id],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Close a run: `Ok(items)` marks it `ok`, `Err(message)` marks it `failed`.
pub fn finish_sync_run(
    conn: &Connection,
    run_id: i64,
    outcome: Result<i64, &str>,
) -> Result<(), Error> {
    let (status, items, error) = match outcome {
        Ok(n) => ("ok", Some(n), None),
        Err(e) => ("failed", None, Some(e)),
    };
    conn.execute(
        "UPDATE sync_runs SET status = ?2, items = ?3, error = ?4, finished_at = datetime('now')
         WHERE id = ?1",
        rusqlite::params![run_id, status, items, error],
    )?;
    Ok(())
}

/// On boot, runs still marked `running` were cut off by a restart.
pub fn fail_interrupted_sync_runs(conn: &Connection) -> Result<usize, Error> {
    Ok(conn.execute(
        "UPDATE sync_runs SET status = 'failed', error = 'interrupted by restart',
                finished_at = datetime('now')
         WHERE status = 'running'",
        [],
    )?)
}

/// Delete finished runs older than `days`.
pub fn prune_sync_runs(conn: &Connection, days: i64) -> Result<usize, Error> {
    Ok(conn.execute(
        "DELETE FROM sync_runs WHERE status != 'running' AND started_at < datetime('now', ?1)",
        rusqlite::params![format!("-{days} days")],
    )?)
}

/// Most recent runs, newest first, optionally only failures.
pub fn list_sync_runs(
    conn: &Connection,
    failed_only: bool,
    limit: i64,
) -> Result<Vec<SyncRun>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, kind, target_id, started_at, finished_at, status, items, error
         FROM sync_runs
         WHERE ?1 = 0 OR status = 'failed'
         ORDER BY id DESC
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![failed_only as i64, limit], map_sync_run)?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// One run by id.
pub fn get_sync_run(conn: &Connection, id: i64) -> Result<SyncRun, Error> {
    conn.query_row(
        "SELECT id, source, kind, target_id, started_at, finished_at, status, items, error
         FROM sync_runs WHERE id = ?1",
        rusqlite::params![id],
        map_sync_run,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("sync run id={id}")),
        other => other.into(),
    })
}

fn map_sync_run(row: &rusqlite::Row) -> rusqlite::Result<SyncRun> {
    Ok(SyncRun {
        id: row.get(0)?,
        source: row.get(1)?,
        kind: row.get(2)?,
        target_id: row.get(3)?,
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        status: row.get(6)?,
        items: row.get(7)?,
        error: row.get(8)?,
    })
}

/// Health of one source, derived from its sync runs.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceHealth {
    pub last_success: Option<String>,
    pub last_failure: Option<String>,
    /// Failed runs since the last successful one.
    pub failure_streak: i64,
    /// Error of the newest failed run, when it is newer than the last success.
    pub last_error: Option<String>,
    /// Manga rows discovered on this source.
    pub manga_count: i64,
}

/// Health for one source. Unknown sources yield the default (never run).
pub fn source_health(conn: &Connection, source: &str) -> Result<SourceHealth, Error> {
    Ok(conn.query_row(
        "WITH last_ok AS (
             SELECT MAX(id) AS id FROM sync_runs WHERE source = ?1 AND status = 'ok'
         )
         SELECT
             (SELECT finished_at FROM sync_runs WHERE id = (SELECT id FROM last_ok)),
             (SELECT finished_at FROM sync_runs WHERE source = ?1 AND status = 'failed'
                ORDER BY id DESC LIMIT 1),
             (SELECT COUNT(*) FROM sync_runs WHERE source = ?1 AND status = 'failed'
                AND id > COALESCE((SELECT id FROM last_ok), 0)),
             (SELECT error FROM sync_runs WHERE source = ?1 AND status = 'failed'
                AND id > COALESCE((SELECT id FROM last_ok), 0)
                ORDER BY id DESC LIMIT 1),
             (SELECT COUNT(*) FROM manga WHERE source = ?1)",
        rusqlite::params![source],
        |row| {
            Ok(SourceHealth {
                last_success: row.get(0)?,
                last_failure: row.get(1)?,
                failure_streak: row.get(2)?,
                last_error: row.get(3)?,
                manga_count: row.get(4)?,
            })
        },
    )?)
}

// ── Identity (users, sessions, bookmarks, progress, history, notes) ──

fn map_user_row(row: &rusqlite::Row) -> rusqlite::Result<User> {
    Ok(User {
        id: row.get(0)?,
        name: row.get(1)?,
        email: row.get(2)?,
        password_hash: row.get(3)?,
        role: Role::from_str(&row.get::<_, String>(4)?),
        locale: row.get(5)?,
        created_at: row.get(6)?,
        banned_at: row.get(7)?,
    })
}

/// Insert a new user. Fails on duplicate email.
pub fn create_user(conn: &Connection, u: &NewUser) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO users (name, email, password_hash, role) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![u.name, u.email, u.password_hash, u.role.as_str()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Create the instance's first admin, atomically.
///
/// The emptiness check and the insert are one statement, so two concurrent
/// setup submissions cannot both succeed. Returns `None` when any user
/// already exists.
pub fn create_first_admin(conn: &Connection, u: &NewUser) -> Result<Option<i64>, Error> {
    let inserted = conn.execute(
        "INSERT INTO users (name, email, password_hash, role)
         SELECT ?1, ?2, ?3, 'admin'
         WHERE NOT EXISTS (SELECT 1 FROM users)",
        rusqlite::params![u.name, u.email, u.password_hash],
    )?;
    Ok((inserted == 1).then(|| conn.last_insert_rowid()))
}

/// Number of admins that are not banned.
fn active_admin_count(conn: &Connection) -> Result<i64, Error> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM users WHERE role = 'admin' AND banned_at IS NULL",
        [],
        |row| row.get(0),
    )?)
}

/// Whether `user_id` is currently an unbanned admin.
fn is_active_admin(conn: &Connection, user_id: i64) -> Result<bool, Error> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id = ?1 AND role = 'admin' AND banned_at IS NULL)",
        rusqlite::params![user_id],
        |row| row.get(0),
    )?)
}

/// Change a user's role. Refuses (`Conflict`) to demote the last active
/// admin, so an instance can never lock itself out of `/admin`.
pub fn set_user_role(conn: &Connection, user_id: i64, role: &Role) -> Result<(), Error> {
    let tx = conn.unchecked_transaction()?;
    if *role != Role::Admin && is_active_admin(&tx, user_id)? && active_admin_count(&tx)? <= 1 {
        return Err(Error::Conflict("cannot demote the last admin".into()));
    }
    let n = tx.execute(
        "UPDATE users SET role = ?1 WHERE id = ?2",
        rusqlite::params![role.as_str(), user_id],
    )?;
    if n == 0 {
        return Err(Error::NotFound(format!("user id={user_id}")));
    }
    tx.commit()?;
    Ok(())
}

/// Ban or unban a user. Banning deletes their sessions and refuses
/// (`Conflict`) to ban the last active admin.
pub fn set_user_banned(
    conn: &Connection,
    user_id: i64,
    banned: bool,
    now: &str,
) -> Result<(), Error> {
    let tx = conn.unchecked_transaction()?;
    if banned && is_active_admin(&tx, user_id)? && active_admin_count(&tx)? <= 1 {
        return Err(Error::Conflict("cannot ban the last admin".into()));
    }
    let value = banned.then_some(now);
    let n = tx.execute(
        "UPDATE users SET banned_at = ?1 WHERE id = ?2",
        rusqlite::params![value, user_id],
    )?;
    if n == 0 {
        return Err(Error::NotFound(format!("user id={user_id}")));
    }
    if banned {
        tx.execute(
            "DELETE FROM sessions WHERE user_id = ?1",
            rusqlite::params![user_id],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// Append to the moderation audit log.
pub fn insert_mod_log(
    conn: &Connection,
    actor_id: i64,
    action: &str,
    target_user_id: Option<i64>,
    detail: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO mod_log (actor_id, action, target_user_id, detail) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![actor_id, action, target_user_id, detail],
    )?;
    Ok(())
}

/// Most recent moderation log entries, newest first.
pub fn list_mod_log(conn: &Connection, limit: i64) -> Result<Vec<ModLogEntry>, Error> {
    let mut stmt = conn.prepare(
        "SELECT a.name, l.action, t.name, l.detail, l.created_at
         FROM mod_log l
         JOIN users a ON a.id = l.actor_id
         LEFT JOIN users t ON t.id = l.target_user_id
         ORDER BY l.id DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![limit], |row| {
        Ok(ModLogEntry {
            actor: row.get(0)?,
            action: row.get(1)?,
            target: row.get(2)?,
            detail: row.get(3)?,
            created_at: row.get(4)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// What an emailed link is allowed to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
    /// Finish creating an account for a verified address.
    Signup,
    /// Set a new password for the account with this address.
    Reset,
}

impl TokenPurpose {
    fn as_str(self) -> &'static str {
        match self {
            TokenPurpose::Signup => "signup",
            TokenPurpose::Reset => "reset",
        }
    }
}

/// Store a link token by its hash. Earlier unused tokens for the same
/// address and purpose are deleted, so only the newest link works.
pub fn create_email_token(
    conn: &Connection,
    token_hash: &str,
    email: &str,
    purpose: TokenPurpose,
    expires_at: &str,
) -> Result<(), Error> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM email_tokens WHERE email = ?1 AND purpose = ?2 AND used_at IS NULL",
        rusqlite::params![email, purpose.as_str()],
    )?;
    tx.execute(
        "INSERT INTO email_tokens (token_hash, email, purpose, expires_at) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![token_hash, email, purpose.as_str(), expires_at],
    )?;
    tx.commit()?;
    Ok(())
}

/// The address behind a live (unused, unexpired) token, without consuming it.
pub fn peek_email_token(
    conn: &Connection,
    token_hash: &str,
    purpose: TokenPurpose,
    now: &str,
) -> Result<Option<String>, Error> {
    let found = conn.query_row(
        "SELECT email FROM email_tokens
         WHERE token_hash = ?1 AND purpose = ?2 AND used_at IS NULL AND expires_at > ?3",
        rusqlite::params![token_hash, purpose.as_str(), now],
        |row| row.get(0),
    );
    match found {
        Ok(email) => Ok(Some(email)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Consume a live token and return its address. The check and the mark are
/// one `UPDATE … RETURNING`, so concurrent submissions cannot both succeed.
pub fn consume_email_token(
    conn: &Connection,
    token_hash: &str,
    purpose: TokenPurpose,
    now: &str,
) -> Result<Option<String>, Error> {
    let found = conn.query_row(
        "UPDATE email_tokens SET used_at = ?3
         WHERE token_hash = ?1 AND purpose = ?2 AND used_at IS NULL AND expires_at > ?3
         RETURNING email",
        rusqlite::params![token_hash, purpose.as_str(), now],
        |row| row.get(0),
    );
    match found {
        Ok(email) => Ok(Some(email)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Find a user by id. `NotFound` when missing.
pub fn find_user_by_id(conn: &Connection, id: i64) -> Result<User, Error> {
    conn.query_row(
        "SELECT id, name, email, password_hash, role, locale, created_at, banned_at FROM users WHERE id = ?1",
        rusqlite::params![id],
        map_user_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("user id={id}")),
        other => other.into(),
    })
}

/// Find a user by email. `NotFound` when missing.
pub fn find_user_by_email(conn: &Connection, email: &str) -> Result<User, Error> {
    conn.query_row(
        "SELECT id, name, email, password_hash, role, locale, created_at, banned_at FROM users WHERE email = ?1",
        rusqlite::params![email],
        map_user_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            Error::NotFound(format!("user email={}", email))
        }
        other => other.into(),
    })
}

/// Count all users.
pub fn count_users(conn: &Connection) -> Result<i64, Error> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get(0))?;
    Ok(n)
}

/// Insert a session token for a user.
pub fn insert_session(
    conn: &Connection,
    user_id: i64,
    token: &str,
    expires_at: &str,
    device: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO sessions (user_id, token, expires_at, device) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![user_id, token, expires_at, device],
    )?;
    Ok(())
}

/// Find the user owning a live session token (`expires_at > now`).
pub fn find_user_by_session(conn: &Connection, token: &str, now: &str) -> Result<User, Error> {
    conn.query_row(
        "SELECT u.id, u.name, u.email, u.password_hash, u.role, u.locale, u.created_at, u.banned_at
         FROM users u JOIN sessions s ON s.user_id = u.id
         WHERE s.token = ?1 AND s.expires_at > ?2 AND u.banned_at IS NULL",
        rusqlite::params![token, now],
        map_user_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("session token={}", token)),
        other => other.into(),
    })
}

/// Delete a session token (logout). No-op when missing.
pub fn delete_session(conn: &Connection, token: &str) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM sessions WHERE token = ?1",
        rusqlite::params![token],
    )?;
    Ok(())
}

/// Link an OAuth identity to a user. Idempotent: re-linking is a no-op.
pub fn link_oauth_account(
    conn: &Connection,
    user_id: i64,
    provider: &str,
    provider_user_id: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO oauth_accounts (user_id, provider, provider_user_id) VALUES (?1, ?2, ?3)
         ON CONFLICT(provider, provider_user_id) DO NOTHING",
        rusqlite::params![user_id, provider, provider_user_id],
    )?;
    Ok(())
}

/// Find the user owning an OAuth identity. `NotFound` when unlinked.
pub fn find_user_by_oauth(
    conn: &Connection,
    provider: &str,
    provider_user_id: &str,
) -> Result<User, Error> {
    conn.query_row(
        "SELECT u.id, u.name, u.email, u.password_hash, u.role, u.locale, u.created_at, u.banned_at
         FROM users u JOIN oauth_accounts o ON o.user_id = u.id
         WHERE o.provider = ?1 AND o.provider_user_id = ?2",
        rusqlite::params![provider, provider_user_id],
        map_user_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("oauth {provider}")),
        other => other.into(),
    })
}

/// Provider keys linked to a user (e.g. `["google"]`). Empty when none.
pub fn user_oauth_providers(conn: &Connection, user_id: i64) -> Result<Vec<String>, Error> {
    let mut stmt =
        conn.prepare("SELECT provider FROM oauth_accounts WHERE user_id = ?1 ORDER BY provider")?;
    let rows = stmt.query_map(rusqlite::params![user_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Delete all sessions of a user (used after password reset).
pub fn delete_sessions_for_user(conn: &Connection, user_id: i64) -> Result<(), Error> {
    conn.execute(
        "DELETE FROM sessions WHERE user_id = ?1",
        rusqlite::params![user_id],
    )?;
    Ok(())
}

/// All users ordered by id (admin list).
pub fn list_users(conn: &Connection) -> Result<Vec<User>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, email, password_hash, role, locale, created_at, banned_at FROM users ORDER BY id ASC",
    )?;
    let rows = stmt.query_map([], map_user_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Set or clear a bookmark.
pub fn set_bookmark(
    conn: &Connection,
    user_id: i64,
    series_id: i64,
    bookmarked: bool,
) -> Result<(), Error> {
    if bookmarked {
        conn.execute(
            "INSERT INTO bookmarks (user_id, series_id) VALUES (?1, ?2) ON CONFLICT(user_id, series_id) DO NOTHING",
            rusqlite::params![user_id, series_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM bookmarks WHERE user_id = ?1 AND series_id = ?2",
            rusqlite::params![user_id, series_id],
        )?;
    }
    Ok(())
}

/// List bookmarked series ids for a user.
pub fn list_bookmarks(conn: &Connection, user_id: i64) -> Result<Vec<i64>, Error> {
    let mut stmt =
        conn.prepare("SELECT series_id FROM bookmarks WHERE user_id = ?1 ORDER BY series_id")?;
    let ids = stmt.query_map(rusqlite::params![user_id], |row| row.get::<_, i64>(0))?;
    ids.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Upsert progress with server-side last-write-wins guard.
pub fn upsert_progress(conn: &Connection, p: &Progress) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO progress (user_id, chapter_id, page, status, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(user_id, chapter_id) DO UPDATE SET
             page = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.page ELSE progress.page END,
             status = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.status ELSE progress.status END,
             updated_at = CASE WHEN excluded.updated_at >= progress.updated_at THEN excluded.updated_at ELSE progress.updated_at END",
        rusqlite::params![p.user_id, p.chapter_id, p.page, p.status, p.updated_at],
    )?;
    Ok(())
}

/// List a user's progress rows for chapters in a series.
pub fn list_progress_by_series(
    conn: &Connection,
    user_id: i64,
    series_id: i64,
) -> Result<Vec<Progress>, Error> {
    let mut stmt = conn.prepare(
        "SELECT p.user_id, p.chapter_id, p.page, p.status, p.updated_at
         FROM progress p
         JOIN chapters c ON c.id = p.chapter_id
         JOIN manga m ON m.id = c.manga_id
         WHERE p.user_id = ?1 AND m.series_id = ?2
         ORDER BY p.chapter_id",
    )?;
    let rows = stmt.query_map(rusqlite::params![user_id, series_id], |row| {
        Ok(Progress {
            user_id: row.get(0)?,
            chapter_id: row.get(1)?,
            page: row.get(2)?,
            status: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Record a chapter read in history.
pub fn add_history(
    conn: &Connection,
    user_id: i64,
    chapter_id: i64,
    at: &str,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO history (user_id, chapter_id, read_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![user_id, chapter_id, at],
    )?;
    Ok(())
}

/// Insert a note, returning its id.
pub fn insert_note(
    conn: &Connection,
    user_id: i64,
    chapter_id: i64,
    page: i64,
    body: &str,
) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO notes (user_id, chapter_id, page, body) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![user_id, chapter_id, page, body],
    )?;
    Ok(conn.last_insert_rowid())
}

/// List a user's notes on a chapter.
pub fn list_notes(conn: &Connection, user_id: i64, chapter_id: i64) -> Result<Vec<Note>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, user_id, chapter_id, page, body, created_at FROM notes
         WHERE user_id = ?1 AND chapter_id = ?2 ORDER BY id",
    )?;
    let rows = stmt.query_map(rusqlite::params![user_id, chapter_id], |row| {
        Ok(Note {
            id: row.get(0)?,
            user_id: row.get(1)?,
            chapter_id: row.get(2)?,
            page: row.get(3)?,
            body: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Error::from)
}

/// Update a user's editable profile fields (name and locale).
///
/// The legacy `users.theme` value is intentionally left untouched.
pub fn update_user_profile(
    conn: &Connection,
    user_id: i64,
    name: &str,
    locale: &str,
) -> Result<(), Error> {
    conn.execute(
        "UPDATE users SET name = ?1, locale = ?2 WHERE id = ?3",
        rusqlite::params![name, locale, user_id],
    )?;
    Ok(())
}

/// Update a user's password hash.
pub fn update_user_password(conn: &Connection, user_id: i64, new_hash: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE users SET password_hash = ?1 WHERE id = ?2",
        rusqlite::params![new_hash, user_id],
    )?;
    Ok(())
}

// ── Site settings (KV store) ──

/// Fetch a single site setting by key. `Ok(None)` when missing.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, Error> {
    let mut stmt = conn.prepare("SELECT value FROM site_settings WHERE key = ?1")?;
    match stmt.query_row(rusqlite::params![key], |row| row.get::<_, String>(0)) {
        Ok(v) => Ok(Some(v)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Upsert a site setting.
pub fn set_setting(
    conn: &Connection,
    key: &str,
    value: &str,
    updated_by: i64,
) -> Result<(), Error> {
    conn.execute(
        "INSERT INTO site_settings (key, value, updated_by, updated_at) VALUES (?1, ?2, ?3, datetime('now')) ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_by = excluded.updated_by, updated_at = datetime('now')",
        rusqlite::params![key, value, updated_by],
    )?;
    Ok(())
}

/// Load all site settings into a map.
pub fn all_settings(conn: &Connection) -> Result<HashMap<String, String>, Error> {
    let mut stmt = conn.prepare("SELECT key, value FROM site_settings")?;
    let rows = stmt.query_map([], |row| {
        let k: String = row.get(0)?;
        let v: String = row.get(1)?;
        Ok((k, v))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (k, v) = row?;
        map.insert(k, v);
    }
    Ok(map)
}

// ── Reader preferences (one JSON blob per user) ──

/// Load a user's reader preferences. Missing user or unreadable blob yields
/// defaults — a prefs read must never fail a page render.
pub fn get_reader_prefs(conn: &Connection, user_id: i64) -> Result<ReaderPrefs, Error> {
    let mut stmt = conn.prepare("SELECT reader_prefs FROM users WHERE id = ?1")?;
    match stmt.query_row(rusqlite::params![user_id], |row| row.get::<_, String>(0)) {
        Ok(raw) => Ok(ReaderPrefs::from_json(&raw)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(ReaderPrefs::default()),
        Err(e) => Err(e.into()),
    }
}

/// Persist reader preferences, normalized first so a hand-crafted request can
/// never store an illegal value.
pub fn set_reader_prefs(conn: &Connection, user_id: i64, prefs: &ReaderPrefs) -> Result<(), Error> {
    let json = prefs.clone().normalized().to_json();
    conn.execute(
        "UPDATE users SET reader_prefs = ?1 WHERE id = ?2",
        rusqlite::params![json, user_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn mem() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        conn
    }

    /// Seed one series with manga entries; each entry is
    /// `(source, cover, chapter numbers)`. Returns the series id.
    fn seed_series(
        conn: &Connection,
        title: &str,
        entries: &[(&str, Option<&str>, &[f64])],
    ) -> i64 {
        let sid = insert_series(conn, title).unwrap();
        for (source, cover, numbers) in entries {
            conn.execute(
                "INSERT INTO manga (source, source_id, title, cover_url, detail_url, series_id)
                 VALUES (?1, ?2, ?2, ?3, 'u', ?4)",
                rusqlite::params![source, title, cover, sid],
            )
            .unwrap();
            let mid = conn.last_insert_rowid();
            for n in *numbers {
                conn.execute(
                    "INSERT INTO chapters (manga_id, source_id, number, url) VALUES (?1, ?2, ?3, 'u')",
                    rusqlite::params![mid, n.to_string(), n],
                )
                .unwrap();
            }
        }
        sid
    }

    #[test]
    fn series_cards_count_shared_chapters_once_and_pick_fullest_cover() {
        let conn = mem();
        seed_series(
            &conn,
            "Alpha",
            &[
                ("a", Some("small.jpg"), &[1.0, 2.0]),
                ("b", Some("full.jpg"), &[1.0, 2.0, 3.0]),
            ],
        );
        let cards = list_series_cards(&conn, None).unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].chapter_count, 3);
        assert_eq!(cards[0].cover_url.as_deref(), Some("full.jpg"));
        assert_eq!(count_series_with_chapters(&conn).unwrap(), 1);
    }

    #[test]
    fn series_cards_exclude_empty_series_and_sort_by_title() {
        let conn = mem();
        seed_series(&conn, "Zeta", &[("a", None, &[1.0])]);
        seed_series(&conn, "Empty", &[("a", None, &[])]);
        seed_series(&conn, "Beta", &[("a", None, &[1.0])]);
        let titles: Vec<_> = list_series_cards(&conn, None)
            .unwrap()
            .into_iter()
            .map(|c| c.title)
            .collect();
        assert_eq!(titles, ["Beta", "Zeta"]);
        assert_eq!(count_series_with_chapters(&conn).unwrap(), 2);
    }

    #[test]
    fn series_card_search_treats_wildcards_literally() {
        let conn = mem();
        seed_series(&conn, "100% Hero", &[("a", None, &[1.0])]);
        seed_series(&conn, "100 Heroes", &[("a", None, &[1.0])]);
        let hits = list_series_cards(&conn, Some("100%")).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "100% Hero");
        assert_eq!(list_series_cards(&conn, Some("hero")).unwrap().len(), 2);
    }

    fn new_user(email: &str, role: Role) -> NewUser {
        NewUser {
            name: email.split('@').next().unwrap().into(),
            email: email.into(),
            password_hash: "h".into(),
            role,
        }
    }

    #[test]
    fn legacy_editor_role_reads_and_migrates_as_moderator() {
        let conn = mem();
        let id = create_user(&conn, &new_user("e@x.io", Role::Member)).unwrap();
        conn.execute("UPDATE users SET role = 'editor' WHERE id = ?1", [id])
            .unwrap();
        assert_eq!(
            find_user_by_email(&conn, "e@x.io").unwrap().role,
            Role::Moderator
        );
        init_db(&conn).unwrap();
        let stored: String = conn
            .query_row("SELECT role FROM users WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(stored, "moderator");
    }

    #[test]
    fn first_admin_is_created_only_on_an_empty_instance() {
        let conn = mem();
        let first = create_first_admin(&conn, &new_user("a@x.io", Role::Member)).unwrap();
        assert!(first.is_some());
        assert_eq!(
            find_user_by_email(&conn, "a@x.io").unwrap().role,
            Role::Admin
        );
        let second = create_first_admin(&conn, &new_user("b@x.io", Role::Member)).unwrap();
        assert_eq!(second, None);
        assert_eq!(count_users(&conn).unwrap(), 1);
    }

    #[test]
    fn last_admin_cannot_be_demoted_or_banned() {
        let conn = mem();
        let admin = create_first_admin(&conn, &new_user("a@x.io", Role::Member))
            .unwrap()
            .unwrap();
        assert!(matches!(
            set_user_role(&conn, admin, &Role::Member),
            Err(Error::Conflict(_))
        ));
        assert!(matches!(
            set_user_banned(&conn, admin, true, "2026-10-14 00:00:00"),
            Err(Error::Conflict(_))
        ));
        let other = create_user(&conn, &new_user("b@x.io", Role::Member)).unwrap();
        set_user_role(&conn, other, &Role::Admin).unwrap();
        set_user_role(&conn, admin, &Role::Moderator).unwrap();
        assert_eq!(
            find_user_by_email(&conn, "a@x.io").unwrap().role,
            Role::Moderator
        );
        assert!(matches!(
            set_user_role(&conn, 999, &Role::Member),
            Err(Error::NotFound(_))
        ));
    }

    #[test]
    fn banned_user_sessions_stop_resolving_and_unban_restores_login() {
        let conn = mem();
        create_first_admin(&conn, &new_user("a@x.io", Role::Member)).unwrap();
        let id = create_user(&conn, &new_user("m@x.io", Role::Member)).unwrap();
        insert_session(&conn, id, "tok", "2099-01-01 00:00:00", "t").unwrap();
        set_user_banned(&conn, id, true, "2026-10-14 00:00:00").unwrap();
        assert!(matches!(
            find_user_by_session(&conn, "tok", "2026-10-14 00:00:00"),
            Err(Error::NotFound(_))
        ));
        assert!(
            find_user_by_email(&conn, "m@x.io")
                .unwrap()
                .banned_at
                .is_some()
        );
        set_user_banned(&conn, id, false, "2026-10-14 00:00:00").unwrap();
        assert!(
            find_user_by_email(&conn, "m@x.io")
                .unwrap()
                .banned_at
                .is_none()
        );
        insert_session(&conn, id, "tok2", "2099-01-01 00:00:00", "t").unwrap();
        assert!(find_user_by_session(&conn, "tok2", "2026-10-14 00:00:00").is_ok());
    }

    #[test]
    fn mod_log_lists_newest_first_with_names() {
        let conn = mem();
        let a = create_first_admin(&conn, &new_user("a@x.io", Role::Member))
            .unwrap()
            .unwrap();
        let m = create_user(&conn, &new_user("m@x.io", Role::Member)).unwrap();
        insert_mod_log(&conn, a, "role", Some(m), "member -> moderator").unwrap();
        insert_mod_log(&conn, a, "ban", Some(m), "").unwrap();
        let log = list_mod_log(&conn, 10).unwrap();
        assert_eq!(log[0].action, "ban");
        assert_eq!(log[1].actor, "a");
        assert_eq!(log[1].target.as_deref(), Some("m"));
    }

    #[test]
    fn email_tokens_are_single_use_scoped_and_expiring() {
        let conn = mem();
        let now = "2026-10-14 12:00:00";
        create_email_token(
            &conn,
            "h1",
            "a@x.io",
            TokenPurpose::Signup,
            "2026-10-15 12:00:00",
        )
        .unwrap();
        assert_eq!(
            peek_email_token(&conn, "h1", TokenPurpose::Reset, now).unwrap(),
            None,
            "purpose is part of the key"
        );
        assert_eq!(
            peek_email_token(&conn, "h1", TokenPurpose::Signup, now)
                .unwrap()
                .as_deref(),
            Some("a@x.io")
        );
        assert_eq!(
            consume_email_token(&conn, "h1", TokenPurpose::Signup, now)
                .unwrap()
                .as_deref(),
            Some("a@x.io")
        );
        assert_eq!(
            consume_email_token(&conn, "h1", TokenPurpose::Signup, now).unwrap(),
            None
        );
        assert_eq!(
            consume_email_token(&conn, "h1", TokenPurpose::Signup, "2026-10-16 00:00:00").unwrap(),
            None
        );
    }

    #[test]
    fn newer_email_token_revokes_the_older_one() {
        let conn = mem();
        let now = "2026-10-14 12:00:00";
        let exp = "2026-10-15 12:00:00";
        create_email_token(&conn, "old", "a@x.io", TokenPurpose::Reset, exp).unwrap();
        create_email_token(&conn, "new", "a@x.io", TokenPurpose::Reset, exp).unwrap();
        assert_eq!(
            peek_email_token(&conn, "old", TokenPurpose::Reset, now).unwrap(),
            None
        );
        assert!(
            peek_email_token(&conn, "new", TokenPurpose::Reset, now)
                .unwrap()
                .is_some()
        );
        // Expired tokens are rejected even when unused.
        create_email_token(
            &conn,
            "late",
            "b@x.io",
            TokenPurpose::Reset,
            "2026-10-14 11:00:00",
        )
        .unwrap();
        assert_eq!(
            peek_email_token(&conn, "late", TokenPurpose::Reset, now).unwrap(),
            None
        );
    }

    #[test]
    fn sync_run_lifecycle_and_health_streak() {
        let conn = mem();
        assert_eq!(source_health(&conn, "a").unwrap(), SourceHealth::default());
        let r1 = start_sync_run(&conn, "a", SyncKind::Scan, None).unwrap();
        finish_sync_run(&conn, r1, Ok(12)).unwrap();
        let r2 = start_sync_run(&conn, "a", SyncKind::Manga, Some(7)).unwrap();
        finish_sync_run(&conn, r2, Err("timeout")).unwrap();
        let r3 = start_sync_run(&conn, "a", SyncKind::Manga, Some(8)).unwrap();
        finish_sync_run(&conn, r3, Err("404")).unwrap();
        let h = source_health(&conn, "a").unwrap();
        assert!(h.last_success.is_some());
        assert_eq!(h.failure_streak, 2);
        assert_eq!(h.last_error.as_deref(), Some("404"));
        // A success resets the streak and clears the shown error.
        let r4 = start_sync_run(&conn, "a", SyncKind::Scan, None).unwrap();
        finish_sync_run(&conn, r4, Ok(3)).unwrap();
        let h = source_health(&conn, "a").unwrap();
        assert_eq!(h.failure_streak, 0);
        assert_eq!(h.last_error, None);
        assert!(h.last_failure.is_some(), "history keeps the old failure");
        let failed = list_sync_runs(&conn, true, 10).unwrap();
        assert_eq!(failed.iter().map(|r| r.id).collect::<Vec<_>>(), [r3, r2]);
        assert_eq!(get_sync_run(&conn, r2).unwrap().target_id, Some(7));
    }

    #[test]
    fn interrupted_runs_fail_on_boot_and_old_runs_prune() {
        let conn = mem();
        let r = start_sync_run(&conn, "a", SyncKind::Scan, None).unwrap();
        assert_eq!(fail_interrupted_sync_runs(&conn).unwrap(), 1);
        let run = get_sync_run(&conn, r).unwrap();
        assert_eq!(run.status, "failed");
        assert_eq!(run.error.as_deref(), Some("interrupted by restart"));
        conn.execute(
            "UPDATE sync_runs SET started_at = datetime('now', '-40 days')",
            [],
        )
        .unwrap();
        start_sync_run(&conn, "a", SyncKind::Scan, None).unwrap();
        assert_eq!(
            prune_sync_runs(&conn, 30).unwrap(),
            1,
            "running rows are kept"
        );
    }

    #[test]
    fn stale_manga_ids_are_distinct() {
        let conn = mem();
        seed_series(&conn, "Many", &[("a", None, &[1.0, 2.0, 3.0])]);
        let ids = list_manga_ids_needing_sync(&conn, 120, &[]).unwrap();
        assert_eq!(ids.len(), 1);
        let skipped = list_manga_ids_needing_sync(&conn, 120, &["a".to_string()]).unwrap();
        assert!(skipped.is_empty(), "disabled sources are skipped");
    }

    #[test]
    fn scan_keeps_manual_links_and_renames_stick() {
        let conn = mem();
        let target = insert_series(&conn, "Manual Pick").unwrap();
        seed_series(&conn, "Solo Leveling", &[("a", None, &[1.0])]);
        let mid: i64 = conn
            .query_row(
                "SELECT id FROM manga WHERE title = 'Solo Leveling'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        link_manga_to_series(&conn, mid, target).unwrap();
        assert_eq!(
            ensure_manga_linked(&conn, "Solo Leveling", mid).unwrap(),
            target
        );
        conn.execute("UPDATE series SET title_locked = 1 WHERE id = ?1", [target])
            .unwrap();
        update_series_title(&conn, target, "Scraped Title").unwrap();
        assert_eq!(
            get_series_by_id(&conn, target).unwrap().title,
            "Manual Pick"
        );
    }

    #[test]
    fn user_roundtrip_and_session_lookup() {
        let conn = mem();
        let id = create_user(
            &conn,
            &NewUser {
                name: "r".into(),
                email: "r@x.io".into(),
                password_hash: "h".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        assert!(id > 0);
        insert_session(&conn, id, "tok123", "2099-01-01 00:00:00", "test").unwrap();
        let u = find_user_by_session(&conn, "tok123", "2026-01-01 00:00:00").unwrap();
        assert_eq!(u.email, "r@x.io");
    }

    #[test]
    fn settings_roundtrip_and_profile_update() {
        let conn = mem();
        assert_eq!(get_setting(&conn, "site_title").unwrap(), None);
        set_setting(&conn, "site_title", "MyDen", 1).unwrap();
        set_setting(&conn, "site_theme", "retired-theme", 1).unwrap();
        assert_eq!(
            get_setting(&conn, "site_title").unwrap().as_deref(),
            Some("MyDen")
        );
        let id = create_user(
            &conn,
            &NewUser {
                name: "r".into(),
                email: "r@x.io".into(),
                password_hash: "h".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE users SET theme = 'asura' WHERE id = ?1",
            rusqlite::params![id],
        )
        .unwrap();
        update_user_profile(&conn, id, "renamed", "fr").unwrap();
        let u = find_user_by_email(&conn, "r@x.io").unwrap();
        assert_eq!(u.name, "renamed");
        let stored_theme: String = conn
            .query_row(
                "SELECT theme FROM users WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_theme, "asura");
        assert_eq!(
            get_setting(&conn, "site_theme").unwrap().as_deref(),
            Some("retired-theme")
        );
    }

    #[test]
    fn oauth_link_roundtrip() {
        let conn = mem();
        let id = create_user(
            &conn,
            &NewUser {
                name: "o".into(),
                email: "o@x.io".into(),
                password_hash: "OAUTH:google:1".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        link_oauth_account(&conn, id, "google", "gid-1").unwrap();
        // idempotent re-link
        link_oauth_account(&conn, id, "google", "gid-1").unwrap();
        let u = find_user_by_oauth(&conn, "google", "gid-1").unwrap();
        assert_eq!(u.id, id);
        assert!(matches!(
            find_user_by_oauth(&conn, "google", "nope"),
            Err(crate::error::Error::NotFound(_))
        ));
        assert_eq!(
            user_oauth_providers(&conn, id).unwrap(),
            vec!["google".to_string()]
        );
    }

    #[test]
    fn bookmark_unknown_series_fails_on_fk() {
        let conn = mem();
        let user_id = create_user(
            &conn,
            &NewUser {
                name: "fk".into(),
                email: "fk@x.io".into(),
                password_hash: "h".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        // No series with id 9999 exists → FK violation must error.
        assert!(set_bookmark(&conn, user_id, 9999, true).is_err());
    }

    #[test]
    fn reader_prefs_roundtrip_and_defaults() {
        let conn = mem();
        let uid = create_user(
            &conn,
            &NewUser {
                name: "r".into(),
                email: "r@x.io".into(),
                password_hash: "h".into(),
                role: Role::Member,
            },
        )
        .unwrap();
        // Fresh user -> defaults.
        assert_eq!(
            get_reader_prefs(&conn, uid).unwrap(),
            ReaderPrefs::default()
        );
        // Illegal values are normalized on write.
        let mut p = ReaderPrefs::default();
        p.layout = "nope".into();
        p.max_w = 99999;
        set_reader_prefs(&conn, uid, &p).unwrap();
        let got = get_reader_prefs(&conn, uid).unwrap();
        assert_eq!(got.layout, "strip");
        assert_eq!(got.max_w, 1400);
        // A corrupted blob degrades to defaults, never errors.
        conn.execute(
            "UPDATE users SET reader_prefs = '{{{' WHERE id = ?1",
            rusqlite::params![uid],
        )
        .unwrap();
        assert_eq!(
            get_reader_prefs(&conn, uid).unwrap(),
            ReaderPrefs::default()
        );
        // Unknown user -> defaults, not an error.
        assert_eq!(
            get_reader_prefs(&conn, 12_345).unwrap(),
            ReaderPrefs::default()
        );
    }
}
