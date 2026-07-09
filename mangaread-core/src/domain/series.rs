/// A unified series that may have entries on multiple source sites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Series {
    pub id: i64,
    pub title: String,
    pub created_at: String,
}

/// Series with its manga entries and total chapter count.
#[derive(Debug, Clone)]
pub struct SeriesWithManga {
    pub series: Series,
    pub manga_entries: Vec<super::manga::Manga>,
    pub total_chapters: i64,
}
