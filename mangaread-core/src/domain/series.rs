/// A unified series that may have entries on multiple source sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub id: i64,
    pub title: String,
    pub created_at: String,
    /// Hidden by an admin: invisible to everyone else.
    pub hidden: bool,
    /// Admin-chosen cover; wins over every source's cover.
    pub cover_override: Option<String>,
    /// Renamed by an admin; syncs no longer change the title.
    pub title_locked: bool,
}

/// One row of the admin series list. Includes hidden series.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminSeriesRow {
    pub id: i64,
    pub title: String,
    pub hidden: bool,
    pub title_locked: bool,
    /// Distinct chapter numbers across sources.
    pub chapter_count: i64,
    /// How many source entries are merged into this series.
    pub manga_count: i64,
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
