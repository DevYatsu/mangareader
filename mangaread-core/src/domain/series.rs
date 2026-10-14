/// A unified series that may have entries on multiple source sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub id: i64,
    pub title: String,
    pub created_at: String,
}

/// One row of the series grid: everything a card needs, nothing more.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeriesCard {
    pub id: i64,
    pub title: String,
    /// Cover of the source entry with the most chapters.
    pub cover_url: Option<String>,
    /// Distinct chapter numbers across all sources, so a chapter published
    /// by two sources counts once.
    pub chapter_count: i64,
}
