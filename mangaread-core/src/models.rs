/// A manga series scraped from a source site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manga {
    pub id: i64,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub detail_url: String,
    /// ISO 8601 date/time string (e.g. '2026-07-09 12:00:00')
    pub created_at: String,
    /// ISO 8601 date/time string (e.g. '2026-07-09 12:00:00')
    pub updated_at: String,
}

/// Manga with the number of scraped chapters.
#[derive(Debug, Clone, PartialEq)]
pub struct MangaWithCount {
    pub manga: Manga,
    pub chapter_count: i64,
}

/// Input data for inserting or updating a manga entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewManga {
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub detail_url: String,
}

/// A chapter belonging to a manga.
#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub id: i64,
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
    /// ISO 8601 date/time string (e.g. '2026-07-09 12:00:00')
    pub created_at: String,
}

/// Input data for inserting or updating a chapter.
#[derive(Debug, Clone, PartialEq)]
pub struct NewChapter {
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
}

/// A single page image in a chapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub id: i64,
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
    pub downloaded: bool,
}

/// Input data for inserting a page image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPage {
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
}

// --- Scraper-side types (used by SiteAdapter) ---

/// Lightweight result from discovering manga on a listing page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MangaListing {
    pub title: String,
    pub detail_url: String,
    pub cover_url: Option<String>,
}

/// Full manga info returned after visiting the detail page.
#[derive(Debug, Clone, PartialEq)]
pub struct MangaInfo {
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub detail_url: String,
    pub chapters: Vec<ChapterListing>,
}

/// A chapter entry discovered on a manga detail page.
#[derive(Debug, Clone, PartialEq)]
pub struct ChapterListing {
    pub title: Option<String>,
    pub number: f64,
    pub url: String,
}
