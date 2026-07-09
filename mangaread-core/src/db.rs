use rusqlite::Connection;

use crate::error::Error;
use crate::models::*;

/// Create tables if they don't exist.
pub fn init_db(conn: &Connection) -> Result<(), Error> {
    conn.execute_batch(
        "
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
            index           INTEGER NOT NULL,
            image_url       TEXT NOT NULL,
            downloaded      INTEGER NOT NULL DEFAULT 0,
            UNIQUE(chapter_id, index)
        );
        ",
    )?;
    Ok(())
}

// ── Manga CRUD ──

pub fn insert_manga(conn: &Connection, m: &NewManga) -> Result<i64, Error> {
    conn.execute(
        "INSERT INTO manga (source, source_id, title, author, cover_url, description, detail_url)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(source, source_id) DO UPDATE SET
            title = excluded.title,
            author = excluded.author,
            cover_url = excluded.cover_url,
            description = excluded.description,
            detail_url = excluded.detail_url,
            updated_at = datetime('now')",
        rusqlite::params![
            m.source,
            m.source_id,
            m.title,
            m.author,
            m.cover_url,
            m.description,
            m.detail_url,
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

pub fn list_manga_by_source(conn: &Connection, source: &str) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga WHERE source = ?1 ORDER BY title",
    )?;
    let rows = stmt.query_map(rusqlite::params![source], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

pub fn list_all_manga(conn: &Connection) -> Result<Vec<Manga>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga ORDER BY title",
    )?;
    let rows = stmt.query_map([], map_manga_row)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(Error::from)
}

pub fn get_manga_by_id(conn: &Connection, id: i64) -> Result<Manga, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, source, source_id, title, author, cover_url, description, detail_url, created_at, updated_at
         FROM manga WHERE id = ?1",
    )?;
    stmt.query_row(rusqlite::params![id], map_manga_row)
        .map_err(|_| Error::NotFound(format!("manga id={}", id)))
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
    })
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
        .map_err(|_| Error::NotFound(format!("chapter id={}", id)))
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
        "INSERT INTO pages (chapter_id, index, image_url)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(chapter_id, index) DO UPDATE SET image_url = excluded.image_url",
        rusqlite::params![p.chapter_id, p.index, p.image_url],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn list_pages_by_chapter(conn: &Connection, chapter_id: i64) -> Result<Vec<Page>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, chapter_id, index, image_url, downloaded
         FROM pages WHERE chapter_id = ?1 ORDER BY index",
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
