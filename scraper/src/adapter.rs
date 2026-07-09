use async_trait::async_trait;
use mangaread_core::{MangaInfo, MangaListing};

#[async_trait]
pub trait SiteAdapter: Send + Sync {
    /// Unique identifier for this source (e.g. "demonicscans").
    fn source_name(&self) -> &str;

    /// Base URL of the source site.
    fn base_url(&self) -> &str;

    /// 1. Crawl the main listing page → discover manga + their detail URLs.
    async fn discover_manga(&self, driver: &thirtyfour::WebDriver) -> anyhow::Result<Vec<MangaListing>>;

    /// 2. Visit a manga detail page → full info + chapter URLs.
    async fn fetch_manga(
        &self,
        driver: &thirtyfour::WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo>;

    /// 3. Visit a chapter page → extract ordered page image URLs.
    async fn fetch_pages(
        &self,
        driver: &thirtyfour::WebDriver,
        chapter_url: &str,
    ) -> anyhow::Result<Vec<String>>;
}
