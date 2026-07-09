/// Scraper-side types used by `SiteAdapter` — these model the raw data
/// extracted from source websites before it's mapped into domain entities.

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
