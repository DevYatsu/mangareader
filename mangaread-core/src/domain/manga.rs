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
    /// When chapters were last synced from the source site (ISO 8601).
    /// `None` means never synced.
    pub chapters_synced_at: Option<String>,
    /// Series this manga belongs to, if any.
    pub series_id: Option<i64>,
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
    pub series_id: Option<i64>,
}
