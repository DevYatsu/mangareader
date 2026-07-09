use async_trait::async_trait;
use mangaread_core::{MangaInfo, MangaListing};

/// Interface that each scanlation-site adapter must implement.
///
/// Every adapter handles three sequential phases of scraping — manga discovery, detail
/// fetching, and chapter page extraction — all driven by a shared Selenium [`WebDriver`].
#[async_trait]
pub trait SiteAdapter: Send + Sync {
    /// Unique identifier for this source (e.g. `"demonicscans"`).
    ///
    /// Used as the `source` column value in the database and for adapter lookup during
    /// sync operations.
    fn source_name(&self) -> &str;

    /// Base URL of the source site (e.g. `"https://demonicscans.org"`).
    ///
    /// Used to resolve relative URLs into absolute ones.
    fn base_url(&self) -> &str;

    /// Crawl the main listing page and discover manga entries with their detail URLs.
    ///
    /// This is the first phase: navigate to the site's index or "all manga" page, wait
    /// for dynamic content, and extract each manga card's title, detail URL, and cover
    /// image.
    ///
    /// # Arguments
    /// * `driver` — Shared Selenium WebDriver, already configured and connected.
    ///
    /// # Returns
    /// A list of discovered manga entries. The [`MangaListing::detail_url`] is passed to
    /// [`fetch_manga`] during the second phase.
    async fn discover_manga(&self, driver: &thirtyfour::WebDriver) -> anyhow::Result<Vec<MangaListing>>;

    /// Visit a manga detail page and extract full metadata plus chapter listings.
    ///
    /// This is the second phase: navigate to the manga's detail page, wait for dynamic
    /// content, and extract the title, author, description, cover image, and an ordered
    /// list of available chapters.
    ///
    /// # Arguments
    /// * `driver` — Shared Selenium WebDriver.
    /// * `detail_url` — URL of the manga's detail page (returned by [`discover_manga`]).
    ///
    /// # Returns
    /// Full manga metadata including the list of chapters. Each chapter's URL is later
    /// passed to [`fetch_pages`].
    async fn fetch_manga(
        &self,
        driver: &thirtyfour::WebDriver,
        detail_url: &str,
    ) -> anyhow::Result<MangaInfo>;

    /// Visit a chapter page and extract the ordered list of page image URLs.
    ///
    /// This is the third phase: navigate to the chapter reader page, wait for images to
    /// load, and collect every page image URL in display order.
    ///
    /// # Arguments
    /// * `driver` — Shared Selenium WebDriver.
    /// * `chapter_url` — URL of the chapter's reader page (returned by [`fetch_manga`]).
    ///
    /// # Returns
    /// Page image URLs in reading order (first page first).
    async fn fetch_pages(
        &self,
        driver: &thirtyfour::WebDriver,
        chapter_url: &str,
    ) -> anyhow::Result<Vec<String>>;
}
