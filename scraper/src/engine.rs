use mangaread_core::db;
use mangaread_core::{NewChapter, NewManga, NewPage};
use rusqlite::Connection;
use thirtyfour::WebDriver;
use tracing;

use crate::adapter::SiteAdapter;

/// Orchestrates the full scraping pipeline across one or more site adapters.
///
/// Holds a collection of [`SiteAdapter`] implementations and provides high-level
/// methods that drive each adapter through its three phases while persisting results
/// into the shared SQLite database.
pub struct ScraperEngine {
    adapters: Vec<Box<dyn SiteAdapter>>,
}

impl ScraperEngine {
    /// Create a new empty engine with no adapters registered.
    ///
    /// Use [`add_adapter`](Self::add_adapter) to register adapters before calling any
    /// sync methods.
    pub fn new() -> Self {
        Self {
            adapters: Vec::new(),
        }
    }

    /// Register a site adapter with the engine.
    ///
    /// Registered adapters are consulted (in insertion order) during [`scan_all`],
    /// [`sync_manga`], and [`sync_chapter`].
    ///
    /// # Arguments
    /// * `adapter` — A boxed adapter implementing [`SiteAdapter`].
    pub fn add_adapter(&mut self, adapter: Box<dyn SiteAdapter>) {
        self.adapters.push(adapter);
    }

    /// An engine over the given adapters (usually [`crate::adapters::all`],
    /// minus any the admin disabled).
    pub fn from_adapters(adapters: Vec<Box<dyn SiteAdapter>>) -> Self {
        Self { adapters }
    }

    /// Names of the registered sources, in scan order.
    pub fn source_names(&self) -> Vec<String> {
        self.adapters
            .iter()
            .map(|a| a.source_name().to_string())
            .collect()
    }

    /// Scan every registered source independently.
    ///
    /// Returns one `(source, result)` per adapter. A failing source no longer
    /// stops the sources after it from being scanned.
    pub async fn scan_all(
        &self,
        driver: &WebDriver,
        db: &Connection,
    ) -> Vec<(String, anyhow::Result<usize>)> {
        let mut results = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            let source = adapter.source_name().to_string();
            let result = Self::scan_adapter(adapter.as_ref(), driver, db).await;
            if let Err(e) = &result {
                tracing::error!("scan of {source} failed: {e:#}");
            }
            results.push((source, result));
        }
        results
    }

    /// Scan one source by name. Errors when no such adapter is registered.
    pub async fn scan_source(
        &self,
        source: &str,
        driver: &WebDriver,
        db: &Connection,
    ) -> anyhow::Result<usize> {
        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {source}"))?;
        Self::scan_adapter(adapter.as_ref(), driver, db).await
    }

    /// Discover listings on one adapter and upsert them. Returns how many
    /// listings were found.
    ///
    /// New manga are linked to a series by normalized title. Manga that
    /// already have a series keep it, so admin merges and splits survive.
    async fn scan_adapter(
        adapter: &dyn SiteAdapter,
        driver: &WebDriver,
        db: &Connection,
    ) -> anyhow::Result<usize> {
        let source = adapter.source_name();
        tracing::info!("scanning {source} for manga...");
        let listings = adapter.discover_manga(driver).await?;
        let found = listings.len();
        tracing::info!("found {found} manga on {source}");
        for listing in listings {
            let new_manga = NewManga {
                source: source.to_string(),
                source_id: listing.detail_url.clone(),
                title: listing.title.clone(),
                author: None,
                cover_url: listing.cover_url,
                description: None,
                detail_url: listing.detail_url,
                series_id: None,
            };
            let id = db::insert_manga(db, &new_manga)?;
            db::ensure_manga_linked(db, &listing.title, id)?;
            tracing::debug!("saved manga id={} \"{}\"", id, new_manga.title);
        }
        Ok(found)
    }

    /// Visit a manga's detail page and upsert its chapters into the database.
    ///
    /// Looks up the manga's source to find the matching adapter, then calls
    /// [`fetch_manga`](SiteAdapter::fetch_manga) to obtain full metadata and chapter
    /// listings. Chapters are inserted (or updated) and the manga's metadata (author,
    /// description, cover) is refreshed. The parent series title is also updated with
    /// the clean detail-page title.
    ///
    /// # Arguments
    /// * `driver` — Shared Selenium WebDriver.
    /// * `db` — SQLite connection.
    /// * `manga_id` — Database row ID of the manga to sync.
    pub async fn sync_manga(
        &self,
        driver: &WebDriver,
        db: &Connection,
        manga_id: i64,
    ) -> anyhow::Result<usize> {
        let manga = db::get_manga_by_id(db, manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!("syncing manga: {} ({})", manga.title, manga.detail_url);

        let info = adapter.fetch_manga(driver, &manga.detail_url).await?;

        // Ensure manga is linked to a series (for newly inserted or pre-feature entries)
        let series_id = if let Some(sid) = manga.series_id {
            sid
        } else {
            db::ensure_manga_in_series(db, &info.title, manga_id)?
        };

        // Update manga metadata (preserve series link from DB)
        let updated = NewManga {
            source: manga.source.clone(),
            source_id: manga.source_id.clone(),
            title: info.title.clone(),
            author: info.author,
            cover_url: info.cover_url,
            description: info.description,
            detail_url: info.detail_url,
            series_id: Some(series_id),
        };
        db::insert_manga(db, &updated)?;

        let chapter_count = info.chapters.len();
        for ch in info.chapters {
            tracing::debug!(
                "saving chapter: ch.{} \"{}\" ({})",
                ch.number,
                ch.title.as_deref().unwrap_or(""),
                ch.url
            );
            let new_chapter = NewChapter {
                manga_id,
                source_id: ch.url.clone(),
                number: ch.number,
                title: ch.title,
                url: ch.url,
            };
            let ch_id = db::insert_chapter(db, &new_chapter)?;
            tracing::debug!("saved chapter id={} ch.{}", ch_id, new_chapter.number);
        }

        // Mark when chapters were last fetched
        db::set_chapters_synced(db, manga_id)?;

        // Update the series title with the clean detail-page title
        db::update_series_title(db, series_id, &info.title)?;

        tracing::info!("synced {chapter_count} chapters for manga {manga_id}",);
        Ok(chapter_count)
    }

    /// Visit a chapter's reader page and upsert its page image URLs into the database.
    ///
    /// Resolves the manga owning this chapter to find the matching adapter, then calls
    /// [`fetch_pages`](SiteAdapter::fetch_pages) to obtain the ordered list of page
    /// image URLs. Each page is inserted as a separate row with its 1-based index.
    ///
    /// # Arguments
    /// * `driver` — Shared Selenium WebDriver.
    /// * `db` — SQLite connection.
    /// * `chapter_id` — Database row ID of the chapter to sync.
    pub async fn sync_chapter(
        &self,
        driver: &WebDriver,
        db: &Connection,
        chapter_id: i64,
    ) -> anyhow::Result<usize> {
        let chapter = db::get_chapter_by_id(db, chapter_id)?;
        let manga = db::get_manga_by_id(db, chapter.manga_id)?;

        let adapter = self
            .adapters
            .iter()
            .find(|a| a.source_name() == manga.source)
            .ok_or_else(|| anyhow::anyhow!("no adapter for source {}", manga.source))?;

        tracing::info!(
            "syncing chapter: {} ch.{} ({})",
            manga.title,
            chapter.number,
            chapter.url
        );

        let page_urls = adapter.fetch_pages(driver, &chapter.url).await?;
        tracing::info!(
            "fetched {} page URLs for chapter {}",
            page_urls.len(),
            chapter_id
        );

        for (i, url) in page_urls.iter().enumerate() {
            tracing::debug!("saving page {}: {url}", i + 1);
            let new_page = NewPage {
                chapter_id,
                index: (i + 1) as i32,
                image_url: url.clone(),
            };
            db::insert_page(db, &new_page)?;
        }

        tracing::info!(
            "synced {} pages for chapter {}",
            page_urls.len(),
            chapter_id
        );
        Ok(page_urls.len())
    }
}
