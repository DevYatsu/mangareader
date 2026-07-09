use rusqlite::Connection;

use crate::error::Error;
use crate::domain::*;
use crate::domain::title::{clean_display_title, normalize_title};

/// Create tables if they don't exist + run migrations.
pub fn init_db(conn: &Connection) -> Result<(), Error> {
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
        ",
    )?;
    // Migrations for existing DBs (safe no-ops if column already exists)
    conn.execute_batch("ALTER TABLE manga ADD COLUMN chapters_synced_at TEXT;")
        .ok();
    conn.execute_batch("ALTER TABLE manga ADD COLUMN series_id INTEGER REFERENCES series(id);")
        .ok();
    Ok(())
}

// ── Manga CRUD ──

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

pub fn list_manga_by_source(conn: &Connection, source: &str) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE source = ?1 ORDER BY title",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

pub fn list_manga_by_source_with_count(conn: &Connection, source: &str) -> Result<Vec<MangaWithCount>, Error> {
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
        Ok(MangaWithCount { manga, chapter_count })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

pub fn list_all_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

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


pub fn insert_series(conn: &Connection, title: &str) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO series (title) VALUES (?1)",
        rusqlite::params![title],
    )?;
    Ok(conn.last_insert_rowid())
}

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

/// List all series with their manga entries and total chapter count.
pub fn list_all_series(conn: &Connection) -> Result<Vec<SeriesWithManga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.created_at,
                COALESCE(ch.cnt, 0) AS ch_count
         FROM series s
         LEFT JOIN (
             SELECT m.series_id, COUNT(*) AS cnt
             FROM chapters c
             JOIN manga m ON m.id = c.manga_id
             GROUP BY m.series_id
         ) ch ON ch.series_id = s.id
         ORDER BY s.title",
    )?;
    let rows = stmt.query_map([], |row| {
        let series = Series {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
        };
        let total_chapters: i64 = row.get(3)?;
        Ok((series, total_chapters))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (series, total_chapters) = row?;
        // Fetch manga entries for this series
        let manga_entries = list_manga_by_series(conn, series.id)?;
        result.push(SeriesWithManga {
            series,
            manga_entries,
            total_chapters,
        });
    }
    Ok(result)
}

/// Get all manga entries linked to a series.
pub fn list_manga_by_series(conn: &Connection, series_id: i64) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE series_id = ?1 ORDER BY source",
    )?;
    let rows = stmt.query_map(rusqlite::params![series_id], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

/// Get all manga that are NOT yet linked to any series.
pub fn list_unlinked_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at, chapters_synced_at, series_id
         FROM manga WHERE series_id IS NULL ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

/// Link a manga to a series.
pub fn link_manga_to_series(conn: &Connection, manga_id: i64, series_id: i64) -> Result<(), Error> {
    conn.execute(
        "UPDATE manga SET series_id = ?1 WHERE id = ?2",
        rusqlite::params![series_id, manga_id],
    )?;
    Ok(())
}

/// Try to find or create a series for this manga, then link it.
/// Returns the series id.
pub fn ensure_manga_in_series(conn: &Connection, title: &str, manga_id: i64) -> Result<i64, Error> {
    let series_id = find_or_create_series(conn, title)?;
    link_manga_to_series(conn, manga_id, series_id)?;
    Ok(series_id)
}

/// Update the series title (e.g. with a clean detail-page title after sync).
pub fn update_series_title(conn: &Connection, series_id: i64, title: &str) -> Result<(), Error> {
    conn.execute(
        "UPDATE series SET title = ?1 WHERE id = ?2 AND title != ?1",
        rusqlite::params![title, series_id],
    )?;
    Ok(())
}

/// Search series by title (LIKE %query%).
pub fn search_series(conn: &Connection, query: &str) -> Result<Vec<SeriesWithManga>, Error> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.created_at,
                COALESCE(ch.cnt, 0) AS ch_count
         FROM series s
         LEFT JOIN (
             SELECT m.series_id, COUNT(*) AS cnt
             FROM chapters c
             JOIN manga m ON m.id = c.manga_id
             GROUP BY m.series_id
         ) ch ON ch.series_id = s.id
         WHERE s.title LIKE ?1
         ORDER BY s.title
         LIMIT 50",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern], |row| {
        let series = Series {
            id: row.get(0)?,
            title: row.get(1)?,
            created_at: row.get(2)?,
        };
        let total_chapters: i64 = row.get(3)?;
        Ok((series, total_chapters))
    })?;
    let mut result = Vec::new();
    for row in rows {
        let (series, total_chapters) = row?;
        let manga_entries = list_manga_by_series(conn, series.id)?;
        result.push(SeriesWithManga {
            series,
            manga_entries,
            total_chapters,
        });
    }
    Ok(result)
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
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

// ── Chapter CRUD ──

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

pub fn list_chapters_by_manga(conn: &Connection, manga_id: i64) -> Result<Vec<Chapter>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, manga_id, source_id, number, title, url, created_at
         FROM chapters WHERE manga_id = ?1 ORDER BY number",
    )?;
    let rows = stmt.query_map(rusqlite::params![manga_id], map_chapter_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

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

pub fn insert_page(conn: &Connection, p: &NewPage) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO pages (chapter_id, page_index, image_url)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(chapter_id, page_index) DO UPDATE SET image_url = excluded.image_url",
        rusqlite::params![p.chapter_id, p.index, p.image_url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_pages_by_chapter(conn: &Connection, chapter_id: i64) -> Result<Vec<Page>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, chapter_id, page_index, image_url, downloaded
         FROM pages WHERE chapter_id = ?1 ORDER BY page_index",
    )?;
    let rows = stmt.query_map(rusqlite::params![chapter_id], map_page_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
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
