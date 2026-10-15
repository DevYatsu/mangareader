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

/// Where a page image comes from, for the image proxy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOrigin {
    pub image_url: String,
    /// Adapter name of the manga that owns the page.
    pub source: String,
    /// Whether an admin hid the owning series.
    pub hidden: bool,
}
