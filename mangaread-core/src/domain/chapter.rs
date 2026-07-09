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

/// A recent chapter with its series info for the home page.
#[derive(Debug, Clone)]
pub struct RecentChapter {
    pub chapter_id: i64,
    pub chapter_number: f64,
    pub chapter_title: Option<String>,
    pub chapter_url: String,
    pub created_at: String,
    pub manga_id: i64,
    pub series_id: i64,
    pub series_title: String,
    pub cover_url: Option<String>,
}
