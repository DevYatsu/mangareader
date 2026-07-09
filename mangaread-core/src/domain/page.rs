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
