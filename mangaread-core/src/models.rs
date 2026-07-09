/// A manga series scraped from a source site.
pub struct Manga {
    pub id: i64,
    pub source: String,
    pub source_id: String,
    pub title: String,
    pub author: Option<String>,
    pub cover_url: Option<String>,
    pub description: Option<String>,
    pub detail_url: String,
    pub created_at: String,
    pub updated_at: String,
}

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
pub struct Chapter {
    pub id: i64,
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
    pub created_at: String,
}

pub struct NewChapter {
    pub manga_id: i64,
    pub source_id: String,
    pub number: f64,
    pub title: Option<String>,
    pub url: String,
}

/// A single page image in a chapter.
pub struct Page {
    pub id: i64,
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
    pub downloaded: bool,
}

pub struct NewPage {
    pub chapter_id: i64,
    pub index: i32,
    pub image_url: String,
}

// --- Scraper-side types (used by SiteAdapter) ---

/// Lightweight result from discovering manga on a listing page.
pub struct MangaListing {
    pub title: String,
    pub detail_url: String,
    pub cover_url: Option<String>,
}

/// Full manga info returned after visiting the detail page.
pub struct MangaInfo {
    pub title: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub cover_url: Option<String>,
    pub detail_url: String,
    pub chapters: Vec<ChapterListing>,
}

/// A chapter entry discovered on a manga detail page.
pub struct ChapterListing {
    pub title: Option<String>,
    pub number: f64,
    pub url: String,
}
